//! Writing the filesystem: eight trees, three chunks, and the superblocks.
//!
//! The order of this file is the order of the dependencies. The chunk tree has to
//! describe the chunks the other trees live in; the extent tree has to account
//! for every block including its own; the superblock points at the root tree and
//! carries a copy of the system chunk so that any of it can be found at all.

use std::io::{Seek, SeekFrom, Write};

use crate::crc32c;
use crate::disk::{self, Bytes, Key, kind, objectid};
use crate::layout::{
    Geometry, Logical, MINIMUM_DEVICE, Physical, Plan, SUPERBLOCK_LEN, SUPERBLOCKS,
};
use crate::leaf::{Leaf, LeafError};

/// The label every Thalyx store carries.
///
/// It is the name an installed machine finds its store by, decided by Cesar on
/// 2026-08-06 and recorded in `vault/09-Notas-Tecnicas/Construccion-del-ISO.md`.
/// The kernel command line is compiled into the kernel and cannot name a device
/// that exists on more than one machine; a label is a name Thalyx wrote itself,
/// which is what makes looking for it different from probing devices until one
/// answers.
pub const LABEL: &str = "thalyx-store";

/// The generation every tree block, root item and superblock is stamped with.
///
/// One transaction, so one number. `mkfs.btrfs` ends at 6 because it commits
/// several times; a single number is both simpler and checkable — nothing may
/// claim a generation the superblock does not know about.
const GENERATION: u64 = 1;

/// Why a store could not be written.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("writing {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Refused rather than attempted. See [`MINIMUM_DEVICE`].
    #[error(
        "{path} is {size} bytes, and a Thalyx store needs at least {MINIMUM_DEVICE}. \
         The limit is the backup superblock at 64 MiB, not the space: a smaller device \
         would get a store with one copy of its own root pointer"
    )]
    TooSmall { path: String, size: u64 },

    #[error("building the {tree} tree: {source}")]
    Tree {
        tree: &'static str,
        #[source]
        source: LeafError,
    },

    /// A logical address that the plan does not map. Not reachable from the
    /// current plan, and checked anyway: the alternative is a write at physical
    /// offset 0, which is where the superblock is.
    #[error("logical address {0} is not inside any chunk, so there is nowhere to write it")]
    Unmapped(u64),
}

/// The uuids a store carries, kept as a parameter so a test can ask for the same
/// bytes twice.
///
/// Four separate uuids, and they are not interchangeable: `fsid` names the
/// filesystem, `device` names this device within it, `chunk_tree` is stamped into
/// every tree block, and `subvolume` identifies the root subvolume in the uuid
/// tree.
pub struct Uuids {
    pub fsid: [u8; 16],
    pub device: [u8; 16],
    pub chunk_tree: [u8; 16],
    pub subvolume: [u8; 16],
}

impl Uuids {
    /// Four fresh random uuids.
    pub fn random() -> Self {
        // `into_bytes`, not `to_bytes_le`. Btrfs stores a uuid as the sixteen
        // bytes of its textual form in order, so the little-endian variant
        // produces a filesystem whose uuid is a byte-swapped version of the one
        // reported — which is invisible until something has to match two of them.
        Self {
            fsid: *uuid::Uuid::new_v4().as_bytes(),
            device: *uuid::Uuid::new_v4().as_bytes(),
            chunk_tree: *uuid::Uuid::new_v4().as_bytes(),
            subvolume: *uuid::Uuid::new_v4().as_bytes(),
        }
    }
}

/// Which tree a block belongs to, in the order the blocks are laid out.
///
/// The order is arbitrary except that the root tree is first, which only makes
/// the superblock's `root` pointer the first block of the metadata chunk and is
/// therefore easier to read in a hexdump.
const TREES: [(&str, u64); 7] = [
    ("root", objectid::ROOT_TREE),
    ("extent", objectid::EXTENT_TREE),
    ("fs", objectid::FS_TREE),
    ("csum", objectid::CSUM_TREE),
    ("uuid", objectid::UUID_TREE),
    ("data reloc", objectid::DATA_RELOC_TREE),
    ("dev", objectid::DEV_TREE),
];

/// What was written, for a caller that wants to say so.
#[derive(Debug)]
pub struct Written {
    pub label: String,
    pub fsid: [u8; 16],
    pub total_bytes: u64,
    pub metadata_bytes: u64,
    pub superblocks: usize,
}

/// Write an empty Thalyx store onto `device`, destroying whatever is there.
///
/// The caller is responsible for having asked. Nothing in this crate is reachable
/// from PID 1, and that is the decree rather than an accident:
/// `crates/thalyx-cli/src/store_disk.rs` records why a machine that fabricates a
/// store when it cannot find one comes up looking perfect on the day the disk was
/// not attached.
pub fn write(
    path: &std::path::Path,
    label: &str,
    uuids: &Uuids,
    seconds_since_epoch: u64,
) -> Result<Written, FormatError> {
    let display = path.display().to_string();
    let io = |source: std::io::Error| FormatError::Io {
        path: display.clone(),
        source,
    };

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(io)?;
    let size = file.seek(SeekFrom::End(0)).map_err(io)?;
    if size < MINIMUM_DEVICE {
        return Err(FormatError::TooSmall {
            path: display,
            size,
        });
    }

    let plan = Plan::new(Geometry::default());
    let nodesize = plan.geometry.nodesize;

    // Where each tree's single block goes. The chunk tree is alone in the system
    // chunk; everything else is packed into the metadata chunk.
    let chunk_root = plan.system().logical;
    let metadata_start = plan.metadata().logical;
    let at = |index: usize| Logical(metadata_start.0 + index as u64 * nodesize);

    let blocks: Vec<(&str, u64, Logical)> =
        std::iter::once(("chunk", objectid::CHUNK_TREE, chunk_root))
            .chain(
                TREES
                    .iter()
                    .enumerate()
                    .map(|(index, (name, owner))| (*name, *owner, at(index))),
            )
            .collect();
    let logical_of = |owner: u64| {
        blocks
            .iter()
            .find(|(_, tree, _)| *tree == owner)
            .map(|(_, _, address)| *address)
            .expect("every tree in TREES has a block")
    };

    let new_leaf = |owner: u64| {
        Leaf::new(
            usize::try_from(nodesize).expect("a node fits in a usize"),
            logical_of(owner).0,
            owner,
            GENERATION,
            uuids.fsid,
            uuids.chunk_tree,
        )
    };

    // ---- the chunk tree: the map from logical to physical -----------------
    let mut chunk_tree = Leaf::new(
        usize::try_from(nodesize).expect("a node fits in a usize"),
        chunk_root.0,
        objectid::CHUNK_TREE,
        GENERATION,
        uuids.fsid,
        uuids.chunk_tree,
    );
    add(
        &mut chunk_tree,
        "chunk",
        Key::new(objectid::DEV_ITEMS, kind::DEV_ITEM, 1),
        disk::dev_item(
            1,
            size,
            plan.device_used,
            plan.geometry.sectorsize,
            uuids.device,
            uuids.fsid,
        ),
    )?;
    for chunk in &plan.chunks {
        add(
            &mut chunk_tree,
            "chunk",
            Key::new(
                objectid::FIRST_CHUNK_TREE,
                kind::CHUNK_ITEM,
                chunk.logical.0,
            ),
            disk::chunk_item(
                chunk.length,
                plan.geometry.stripe_len,
                plan.geometry.sectorsize,
                chunk.flags,
                &stripes(chunk, uuids.device),
            ),
        )?;
    }

    // ---- the device tree: the same map, read from the device's side --------
    //
    // One item per copy, keyed by physical offset. Both directions exist because
    // the kernel needs to answer "what is at this logical address" when reading
    // and "is this part of the device free" when allocating.
    let mut dev_tree = new_leaf(objectid::DEV_TREE);
    let mut extents: Vec<(Physical, Logical, u64)> = plan
        .chunks
        .iter()
        .flat_map(|chunk| {
            chunk
                .stripes
                .iter()
                .map(move |stripe| (*stripe, chunk.logical, chunk.length))
        })
        .collect();
    extents.sort_by_key(|(physical, _, _)| physical.0);
    for (physical, logical, length) in extents {
        add(
            &mut dev_tree,
            "dev",
            Key::new(1, kind::DEV_EXTENT, physical.0),
            disk::dev_extent(logical.0, length, uuids.chunk_tree),
        )?;
    }

    // ---- the extent tree: what is allocated, including itself -------------
    let mut used_per_chunk: Vec<u64> = vec![0; plan.chunks.len()];
    for (_, _, address) in &blocks {
        let index = plan
            .chunks
            .iter()
            .position(|chunk| chunk.holds(*address))
            .ok_or(FormatError::Unmapped(address.0))?;
        used_per_chunk[index] += nodesize;
    }

    // Built as a list and sorted, because the two kinds of item interleave: a
    // block group item and the extent item of a tree block that happens to be
    // the block group's first block share an object id, and the type decides
    // which comes first.
    let mut rows: Vec<(Key, Vec<u8>)> = blocks
        .iter()
        .map(|(_, owner, address)| {
            (
                Key::new(address.0, kind::METADATA_ITEM, 0),
                disk::tree_block_extent(GENERATION, *owner),
            )
        })
        .collect();
    for (chunk, used) in plan.chunks.iter().zip(&used_per_chunk) {
        rows.push((
            Key::new(chunk.logical.0, kind::BLOCK_GROUP_ITEM, chunk.length),
            disk::block_group_item(*used, chunk.flags),
        ));
    }
    rows.sort_by_key(|row| row.0);

    let mut extent_tree = new_leaf(objectid::EXTENT_TREE);
    for (key, body) in rows {
        add(&mut extent_tree, "extent", key, body)?;
    }

    // ---- the two trees that are an empty directory ------------------------
    let empty_directory = |owner: u64, time: u64| -> Result<Leaf, FormatError> {
        let mut leaf = new_leaf(owner);
        add(
            &mut leaf,
            "fs",
            Key::new(objectid::FIRST_FREE, kind::INODE_ITEM, 0),
            disk::inode_item(0o040_755, 0, nodesize, GENERATION, time),
        )?;
        // `..` pointing at itself. The root of a subvolume has no parent inside
        // the subvolume, and this is the shape mkfs.btrfs writes.
        add(
            &mut leaf,
            "fs",
            Key::new(objectid::FIRST_FREE, kind::INODE_REF, objectid::FIRST_FREE),
            disk::inode_ref(0, b".."),
        )?;
        Ok(leaf)
    };
    let fs_tree = empty_directory(objectid::FS_TREE, seconds_since_epoch)?;
    let data_reloc_tree = empty_directory(objectid::DATA_RELOC_TREE, 0)?;

    // ---- the checksum tree, which is empty because no data exists yet -----
    let csum_tree = new_leaf(objectid::CSUM_TREE);

    // ---- the uuid tree: subvolume uuid to subvolume id --------------------
    let mut uuid_tree = new_leaf(objectid::UUID_TREE);
    let (low, high) = (
        u64::from_le_bytes(uuids.subvolume[0..8].try_into().expect("16 bytes")),
        u64::from_le_bytes(uuids.subvolume[8..16].try_into().expect("16 bytes")),
    );
    let mut subvolume_id = Bytes::new();
    subvolume_id.u64(objectid::FS_TREE);
    add(
        &mut uuid_tree,
        "uuid",
        Key::new(low, kind::UUID_SUBVOL, high),
        subvolume_id.finish(),
    )?;

    // ---- the root tree: where every other tree's root is named ------------
    let root_item = |owner: u64, dirid: u64, uuid: [u8; 16], time: u64| {
        disk::root_item(
            logical_of(owner).0,
            dirid,
            GENERATION,
            0,
            nodesize,
            uuid,
            time,
        )
    };
    let mut rows = vec![
        (
            Key::new(objectid::EXTENT_TREE, kind::ROOT_ITEM, 0),
            root_item(objectid::EXTENT_TREE, 0, [0; 16], 0),
        ),
        (
            Key::new(objectid::DEV_TREE, kind::ROOT_ITEM, 0),
            root_item(objectid::DEV_TREE, 0, [0; 16], 0),
        ),
        (
            Key::new(objectid::FS_TREE, kind::INODE_REF, objectid::ROOT_TREE_DIR),
            disk::inode_ref(0, b"default"),
        ),
        (
            Key::new(objectid::FS_TREE, kind::ROOT_ITEM, 0),
            root_item(
                objectid::FS_TREE,
                objectid::FIRST_FREE,
                uuids.subvolume,
                seconds_since_epoch,
            ),
        ),
        (
            Key::new(objectid::ROOT_TREE_DIR, kind::INODE_ITEM, 0),
            disk::inode_item(0o040_755, 0, nodesize, GENERATION, seconds_since_epoch),
        ),
        (
            Key::new(
                objectid::ROOT_TREE_DIR,
                kind::INODE_REF,
                objectid::ROOT_TREE_DIR,
            ),
            disk::inode_ref(0, b".."),
        ),
        (
            // The entry that makes `default` resolve to subvolume 5. Its key
            // offset is the hash of the name, so a wrong hash is a name the
            // kernel does not find while the item sits there being correct.
            Key::new(
                objectid::ROOT_TREE_DIR,
                kind::DIR_ITEM,
                crc32c::name_hash(b"default"),
            ),
            disk::dir_item(
                // `-1` as the offset, which is how a directory entry points at a
                // root rather than at a specific generation of one.
                Key::new(objectid::FS_TREE, kind::ROOT_ITEM, u64::MAX),
                disk::FILE_TYPE_DIR,
                b"default",
            ),
        ),
        (
            Key::new(objectid::CSUM_TREE, kind::ROOT_ITEM, 0),
            root_item(objectid::CSUM_TREE, 0, [0; 16], 0),
        ),
        (
            Key::new(objectid::UUID_TREE, kind::ROOT_ITEM, 0),
            root_item(objectid::UUID_TREE, 0, [0; 16], 0),
        ),
        (
            Key::new(objectid::DATA_RELOC_TREE, kind::ROOT_ITEM, 0),
            root_item(objectid::DATA_RELOC_TREE, objectid::FIRST_FREE, [0; 16], 0),
        ),
    ];
    rows.sort_by_key(|row| row.0);

    let mut root_tree = new_leaf(objectid::ROOT_TREE);
    for (key, body) in rows {
        add(&mut root_tree, "root", key, body)?;
    }

    // ---- write the blocks -------------------------------------------------
    let built: Vec<(&str, Logical, Vec<u8>)> = [
        ("chunk", chunk_root, &chunk_tree),
        ("root", logical_of(objectid::ROOT_TREE), &root_tree),
        ("extent", logical_of(objectid::EXTENT_TREE), &extent_tree),
        ("fs", logical_of(objectid::FS_TREE), &fs_tree),
        ("csum", logical_of(objectid::CSUM_TREE), &csum_tree),
        ("uuid", logical_of(objectid::UUID_TREE), &uuid_tree),
        (
            "data reloc",
            logical_of(objectid::DATA_RELOC_TREE),
            &data_reloc_tree,
        ),
        ("dev", logical_of(objectid::DEV_TREE), &dev_tree),
    ]
    .into_iter()
    .map(|(name, address, leaf)| {
        leaf.build()
            .map(|block| (name, address, block))
            .map_err(|source| FormatError::Tree { tree: name, source })
    })
    .collect::<Result<_, _>>()?;

    for (_, address, block) in &built {
        let targets = plan.map(*address);
        if targets.is_empty() {
            return Err(FormatError::Unmapped(address.0));
        }
        for target in targets {
            file.seek(SeekFrom::Start(target.0)).map_err(io)?;
            file.write_all(block).map_err(io)?;
        }
    }

    // ---- and the superblocks, last ---------------------------------------
    //
    // Last on purpose. Until a superblock exists the device is unformatted, and a
    // write interrupted before this point leaves something nothing will try to
    // mount. Written after everything it points at, a torn format is a device
    // with no filesystem rather than a device with a filesystem whose trees are
    // half there.
    let metadata_bytes: u64 = used_per_chunk.iter().sum();
    let superblock = build_superblock(
        &plan,
        label,
        uuids,
        size,
        metadata_bytes,
        logical_of(objectid::ROOT_TREE),
        chunk_root,
    );

    let mut mirrors = 0;
    for mirror in SUPERBLOCKS {
        if mirror + SUPERBLOCK_LEN as u64 > size {
            continue;
        }
        let mut copy = superblock.clone();
        // Each mirror records its own address, and the checksum has to be
        // recomputed after that: a mirror carrying the first one's bytenr is
        // refused by the kernel as a superblock found somewhere it does not
        // claim to be.
        copy[48..56].copy_from_slice(&mirror.to_le_bytes());
        let digest = crc32c::checksum(&copy[32..]);
        copy[0..4].copy_from_slice(&digest);

        file.seek(SeekFrom::Start(mirror)).map_err(io)?;
        file.write_all(&copy).map_err(io)?;
        mirrors += 1;
    }

    // The trees are already on the device by the time the first superblock is
    // written, but "already written" and "already durable" are different facts
    // and only this call establishes the second.
    file.sync_all().map_err(io)?;

    Ok(Written {
        label: label.to_string(),
        fsid: uuids.fsid,
        total_bytes: size,
        metadata_bytes,
        superblocks: mirrors,
    })
}

/// `add`, with the tree's name attached to whatever it refuses.
fn add(leaf: &mut Leaf, tree: &'static str, key: Key, body: Vec<u8>) -> Result<(), FormatError> {
    leaf.add(key, body)
        .map_err(|source| FormatError::Tree { tree, source })
}

/// Offsets inside `struct btrfs_super_block`, which is a stable ABI.
///
/// Written as numbers rather than accumulated from the fields above, because the
/// accumulation would be a second copy of the layout with nothing checking it.
/// These are checked: `tests/layout.rs` computes all three from
/// `tests/uapi_header.h`, which is the header captured verbatim.
///
/// Everything between `metadata_uuid` and the chunk array is left zero, and the
/// kernel has added fields there over time by shrinking `reserved`. That is why
/// the offsets are safe to pin while the field list is not: every field this
/// crate writes sits *before* that region, and the region's total size cannot
/// change without changing the size of the superblock, which is fixed at 4096.
pub mod super_offset {
    pub const LABEL: usize = 299;
    pub const CACHE_GENERATION: usize = 555;
    pub const SYSTEM_CHUNK_ARRAY: usize = 811;
}

/// The label field's fixed width. A shorter label leaves zeros rather than
/// whatever the device held.
///
/// Public because `superblock::interpret` reads the same field, and two copies of
/// a width in two modules disagree eventually — here that would be a reader that
/// walks past the label into `cache_generation` and reports a filesystem whose
/// name has eight bytes of a generation number stuck to it.
pub const LABEL_LEN: usize = 256;

/// A chunk's stripes in the form a chunk item wants them.
fn stripes(chunk: &crate::layout::Chunk, device_uuid: [u8; 16]) -> Vec<(u64, u64, [u8; 16])> {
    chunk
        .stripes
        .iter()
        .map(|stripe| (1u64, stripe.0, device_uuid))
        .collect()
}

/// The superblock, with a placeholder checksum and the first mirror's address.
fn build_superblock(
    plan: &Plan,
    label: &str,
    uuids: &Uuids,
    total_bytes: u64,
    bytes_used: u64,
    root: Logical,
    chunk_root: Logical,
) -> Vec<u8> {
    // The system chunks, copied into the superblock. This is the bootstrap: the
    // chunk tree lives at a logical address, and nothing can translate a logical
    // address until the chunk tree has been read, so the chunks that hold the
    // chunk tree travel in the superblock itself.
    let mut system_chunks = Bytes::new();
    for chunk in &plan.chunks {
        if chunk.flags & disk::block_group::SYSTEM == 0 {
            continue;
        }
        system_chunks
            .key(Key::new(
                objectid::FIRST_CHUNK_TREE,
                kind::CHUNK_ITEM,
                chunk.logical.0,
            ))
            .raw(&disk::chunk_item(
                chunk.length,
                plan.geometry.stripe_len,
                plan.geometry.sectorsize,
                chunk.flags,
                &stripes(chunk, uuids.device),
            ));
    }
    let system_chunks = system_chunks.finish();

    let mut block = vec![0u8; SUPERBLOCK_LEN];
    let mut head = Bytes::new();
    head.raw(&uuids.fsid)
        .u64(SUPERBLOCKS[0])
        .u64(disk::HEADER_FLAG_WRITTEN)
        .u64(disk::MAGIC)
        .u64(GENERATION)
        .u64(root.0)
        .u64(chunk_root.0)
        .u64(0) // log_root
        .u64(0) // __unused_log_root_transid
        .u64(total_bytes)
        .u64(bytes_used)
        .u64(objectid::ROOT_TREE_DIR)
        .u64(1) // num_devices
        .u32(plan.geometry.sectorsize)
        .u32(u32::try_from(plan.geometry.nodesize).expect("16 KiB"))
        .u32(u32::try_from(plan.geometry.nodesize).expect("16 KiB")) // __unused_leafsize
        .u32(plan.geometry.sectorsize) // stripesize
        .u32(u32::try_from(system_chunks.len()).expect("under 2 KiB"))
        .u64(GENERATION) // chunk_root_generation
        .u64(0) // compat_flags
        .u64(0) // compat_ro_flags
        .u64(disk::incompat::WRITTEN_BY_THALYX)
        .u16(0) // csum_type: crc32c
        .u8(0) // root_level
        .u8(0) // chunk_root_level
        .u8(0) // log_root_level
        .raw(&disk::dev_item(
            1,
            total_bytes,
            plan.device_used,
            plan.geometry.sectorsize,
            uuids.device,
            uuids.fsid,
        ));
    // Everything above runs contiguously from the end of the checksum to the
    // label, so this one equality covers every field's width at once: a field
    // written two bytes short would shift the label and be caught here, before
    // the device is touched, rather than by a kernel refusing to mount.
    assert_eq!(
        32 + head.len(),
        super_offset::LABEL,
        "the superblock fields above the label do not reach the label's offset"
    );
    block[32..super_offset::LABEL].copy_from_slice(&head.finish());

    // Truncated rather than refused: the label is how a person recognises a
    // disk. The name Thalyx *matches* on is `LABEL`, which is twelve bytes and
    // never near this limit.
    let label_bytes = label.as_bytes();
    let keep = label_bytes.len().min(LABEL_LEN - 1);
    block[super_offset::LABEL..super_offset::LABEL + keep].copy_from_slice(&label_bytes[..keep]);

    let mut tail = Bytes::new();
    // `-1` means there is no free space cache to believe. Zero would mean a
    // cache generated at generation 0, which does not exist, and the kernel
    // would go looking for it.
    tail.u64(u64::MAX) // cache_generation
        .u64(0) // uuid_tree_generation
        .zeros(16); // metadata_uuid, unset because the feature bit is not set
    let at = super_offset::CACHE_GENERATION;
    block[at..at + tail.len()].copy_from_slice(&tail.finish());

    let at = super_offset::SYSTEM_CHUNK_ARRAY;
    block[at..at + system_chunks.len()].copy_from_slice(&system_chunks);

    block
}
