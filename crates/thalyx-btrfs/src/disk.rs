//! The on-disk shapes, and the constants that name them.
//!
//! Every struct here is `__attribute__((packed))` in
//! `include/uapi/linux/btrfs_tree.h`, so nothing is derived from a Rust type's
//! layout: each one is written field by field into a byte buffer. That is
//! deliberate rather than primitive. A `#[repr(C, packed)]` struct plus a
//! transmute would read better and would put the correctness of the filesystem
//! on the compiler's padding rules, which is not where anyone can check it.
//!
//! `tests/uapi_header.h` is the header itself, captured verbatim, and the tests
//! in `tests/layout.rs` parse it and check every size and offset below against
//! it. That is the same arrangement `thalyx-permd` uses for `union bpf_attr`,
//! and for the same reason: a field written at the wrong offset produces bytes
//! that are wrong in a way no amount of reading finds.

/// A key: object id, type, offset. 17 bytes, packed — note that it is *not* 24,
/// which is what a Rust struct of the same three fields would be.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Key {
    /// Compared first.
    pub objectid: u64,
    /// Then this. The field order of this struct is the sort order, which is
    /// what `derive(Ord)` is doing here and why the fields may not be reordered.
    pub kind: u8,
    /// Then this.
    pub offset: u64,
}

impl Key {
    pub const ENCODED_LEN: usize = 17;

    pub fn new(objectid: u64, kind: u8, offset: u64) -> Self {
        Self {
            objectid,
            kind,
            offset,
        }
    }

    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut out = [0u8; Self::ENCODED_LEN];
        out[0..8].copy_from_slice(&self.objectid.to_le_bytes());
        out[8] = self.kind;
        out[9..17].copy_from_slice(&self.offset.to_le_bytes());
        out
    }
}

/// Object ids that mean something specific.
pub mod objectid {
    pub const ROOT_TREE: u64 = 1;
    pub const EXTENT_TREE: u64 = 2;
    pub const CHUNK_TREE: u64 = 3;
    pub const DEV_TREE: u64 = 4;
    pub const FS_TREE: u64 = 5;
    pub const ROOT_TREE_DIR: u64 = 6;
    pub const CSUM_TREE: u64 = 7;
    pub const UUID_TREE: u64 = 9;
    /// `-9`, and it has to be written as the unsigned two's complement because
    /// that is how it sorts: last in the root tree, after 9.
    pub const DATA_RELOC_TREE: u64 = u64::MAX - 8;
    /// The object id every device item is filed under.
    pub const DEV_ITEMS: u64 = 1;
    /// The object id every chunk item is filed under.
    pub const FIRST_CHUNK_TREE: u64 = 256;
    /// The first inode number a filesystem tree may use.
    pub const FIRST_FREE: u64 = 256;
}

/// Key types. The numbers are the sort order within one object id, so the gaps
/// are not accidental and the values may not be renumbered for tidiness.
pub mod kind {
    pub const INODE_ITEM: u8 = 1;
    pub const INODE_REF: u8 = 12;
    pub const DIR_ITEM: u8 = 84;
    pub const ROOT_ITEM: u8 = 132;
    pub const METADATA_ITEM: u8 = 169;
    pub const TREE_BLOCK_REF: u8 = 176;
    pub const BLOCK_GROUP_ITEM: u8 = 192;
    pub const DEV_EXTENT: u8 = 204;
    pub const DEV_ITEM: u8 = 216;
    pub const CHUNK_ITEM: u8 = 228;
    pub const UUID_SUBVOL: u8 = 251;
}

/// What a block group holds and how it is replicated.
pub mod block_group {
    pub const DATA: u64 = 1 << 0;
    pub const SYSTEM: u64 = 1 << 1;
    pub const METADATA: u64 = 1 << 2;
    /// Two copies on one device. The default `mkfs.btrfs` picks for metadata on
    /// a single disk, and kept for the same reason: the store is where
    /// everything that survives a reboot lives, and a single bad sector in a
    /// tree block with one copy takes the filesystem rather than a file.
    pub const DUP: u64 = 1 << 5;
}

/// `_BHRfS_M`, at offset 64 of the superblock.
pub const MAGIC: u64 = 0x4D5F_5366_5248_425F;

/// Feature bits. All four are what `mkfs.btrfs` has set by default for years,
/// and the last two change the meaning of items written below — `SKINNY_METADATA`
/// decides that a tree block's extent item is keyed `METADATA_ITEM` with the
/// level in the offset, rather than `EXTENT_ITEM` with the length.
pub mod incompat {
    pub const MIXED_BACKREF: u64 = 1 << 0;
    pub const EXTENDED_IREF: u64 = 1 << 6;
    pub const SKINNY_METADATA: u64 = 1 << 8;
    pub const NO_HOLES: u64 = 1 << 9;

    pub const WRITTEN_BY_THALYX: u64 = MIXED_BACKREF | EXTENDED_IREF | SKINNY_METADATA | NO_HOLES;
}

/// This block has been written. Set on every tree block and on the superblock.
pub const HEADER_FLAG_WRITTEN: u64 = 1 << 0;

/// `BTRFS_MIXED_BACKREF_REV << BTRFS_BACKREF_REV_SHIFT`, in a tree block's
/// `flags`.
///
/// Leaving it out is the one mistake in this file that produced a filesystem
/// which parsed perfectly and was wrong throughout. Every tree read fine, every
/// key was in place — and `btrfs check` reported a reference mismatch on all
/// eleven extents, because revision 0 means the *old* backref format and the
/// extent items were then being read as a layout they were not written in. The
/// symptom was as far as it could possibly be from the cause.
pub const HEADER_MIXED_BACKREF_REV: u64 = 1 << 56;

/// This extent holds a tree block rather than file data.
pub const EXTENT_FLAG_TREE_BLOCK: u64 = 1 << 1;

/// A directory entry that points at a directory.
pub const FILE_TYPE_DIR: u8 = 2;

/// A little-endian byte sink, so that every field below is written by naming its
/// width rather than by trusting a struct.
#[derive(Default)]
pub struct Bytes(Vec<u8>);

impl Bytes {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn u16(&mut self, value: u16) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn u64(&mut self, value: u64) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn u8(&mut self, value: u8) -> &mut Self {
        self.0.push(value);
        self
    }

    pub fn raw(&mut self, value: &[u8]) -> &mut Self {
        self.0.extend_from_slice(value);
        self
    }

    /// `count` zero bytes. Named rather than written as `raw(&[0; 32])` so that
    /// reserved fields read as reserved fields.
    pub fn zeros(&mut self, count: usize) -> &mut Self {
        self.0.resize(self.0.len() + count, 0);
        self
    }

    pub fn key(&mut self, key: Key) -> &mut Self {
        self.raw(&key.encode())
    }

    /// A `btrfs_timespec`: seconds and nanoseconds, 12 bytes.
    pub fn timespec(&mut self, seconds: u64) -> &mut Self {
        self.u64(seconds).u32(0)
    }

    pub fn finish(&self) -> Vec<u8> {
        self.0.clone()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// `struct btrfs_inode_item`, 160 bytes.
pub fn inode_item(mode: u32, size: u64, nbytes: u64, generation: u64, time: u64) -> Vec<u8> {
    let mut out = Bytes::new();
    out.u64(generation)
        .u64(0) // transid
        .u64(size)
        .u64(nbytes)
        .u64(0) // block_group
        .u32(1) // nlink
        .u32(0) // uid
        .u32(0) // gid
        .u32(mode)
        .u64(0) // rdev
        .u64(0) // flags
        .u64(0) // sequence
        .zeros(32) // reserved[4]
        .timespec(time)
        .timespec(time)
        .timespec(time)
        .timespec(time);
    out.finish()
}

/// `struct btrfs_root_item`, 439 bytes.
///
/// `generation_v2` carries the same number as `generation`: a kernel reads the
/// fields after it only when the two agree, and a root item claiming a uuid it
/// then invalidates is a subvolume the uuid tree describes and nothing can find.
#[allow(clippy::too_many_arguments)]
pub fn root_item(
    bytenr: u64,
    root_dirid: u64,
    generation: u64,
    level: u8,
    nodesize: u64,
    uuid: [u8; 16],
    time: u64,
) -> Vec<u8> {
    let mut out = Bytes::new();
    out.raw(&inode_item(0, 0, nodesize, generation, 0))
        .u64(generation)
        .u64(root_dirid)
        .u64(bytenr)
        .u64(0) // byte_limit
        .u64(nodesize) // bytes_used: the one block this root's tree occupies
        .u64(0) // last_snapshot
        .u64(0) // flags
        .u32(1) // refs
        .key(Key::new(0, 0, 0)) // drop_progress
        .u8(0) // drop_level
        .u8(level)
        .u64(generation) // generation_v2
        .raw(&uuid)
        .zeros(16) // parent_uuid
        .zeros(16) // received_uuid
        .u64(0) // ctransid
        .u64(0) // otransid
        .u64(0) // stransid
        .u64(0) // rtransid
        .timespec(time) // ctime
        .timespec(time) // otime
        .timespec(0) // stime
        .timespec(0) // rtime
        .zeros(64); // reserved[8]
    out.finish()
}

/// `struct btrfs_inode_ref` plus the name it carries.
pub fn inode_ref(index: u64, name: &[u8]) -> Vec<u8> {
    let mut out = Bytes::new();
    out.u64(index)
        .u16(u16::try_from(name.len()).expect("a name this crate writes is short"))
        .raw(name);
    out.finish()
}

/// `struct btrfs_dir_item` plus the name it carries.
pub fn dir_item(location: Key, file_type: u8, name: &[u8]) -> Vec<u8> {
    let mut out = Bytes::new();
    out.key(location)
        .u64(0) // transid
        .u16(0) // data_len
        .u16(u16::try_from(name.len()).expect("a name this crate writes is short"))
        .u8(file_type)
        .raw(name);
    out.finish()
}

/// `struct btrfs_extent_item` plus one inline `TREE_BLOCK_REF`.
///
/// 33 bytes: 24 for the extent item, then a one-byte type and the eight-byte
/// object id of the tree that owns the block. There is no separate
/// `TREE_BLOCK_REF` item — the reference is inline, which is what
/// `MIXED_BACKREF` means.
pub fn tree_block_extent(generation: u64, owner: u64) -> Vec<u8> {
    let mut out = Bytes::new();
    out.u64(1) // refs
        .u64(generation)
        .u64(EXTENT_FLAG_TREE_BLOCK)
        .u8(kind::TREE_BLOCK_REF)
        .u64(owner);
    out.finish()
}

/// `struct btrfs_block_group_item`, 24 bytes.
pub fn block_group_item(used: u64, flags: u64) -> Vec<u8> {
    let mut out = Bytes::new();
    out.u64(used).u64(objectid::FIRST_CHUNK_TREE).u64(flags);
    out.finish()
}

/// `struct btrfs_dev_extent`, 48 bytes: which chunk owns this piece of the
/// device.
pub fn dev_extent(chunk_offset: u64, length: u64, chunk_tree_uuid: [u8; 16]) -> Vec<u8> {
    let mut out = Bytes::new();
    out.u64(objectid::CHUNK_TREE)
        .u64(objectid::FIRST_CHUNK_TREE)
        .u64(chunk_offset)
        .u64(length)
        .raw(&chunk_tree_uuid);
    out.finish()
}

/// `struct btrfs_dev_item`, 98 bytes.
pub fn dev_item(
    devid: u64,
    total_bytes: u64,
    bytes_used: u64,
    sectorsize: u32,
    uuid: [u8; 16],
    fsid: [u8; 16],
) -> Vec<u8> {
    let mut out = Bytes::new();
    out.u64(devid)
        .u64(total_bytes)
        .u64(bytes_used)
        .u32(sectorsize) // io_align
        .u32(sectorsize) // io_width
        .u32(sectorsize) // sector_size
        .u64(0) // type
        .u64(0) // generation
        .u64(0) // start_offset
        .u32(0) // dev_group
        .u8(0) // seek_speed
        .u8(0) // bandwidth
        .raw(&uuid)
        .raw(&fsid);
    out.finish()
}

/// `struct btrfs_chunk` plus one `btrfs_stripe` per copy.
///
/// For every profile this crate writes, each stripe covers the whole chunk, so
/// a chunk of `length` with two stripes consumes `2 * length` of the device.
pub fn chunk_item(
    length: u64,
    stripe_len: u64,
    sectorsize: u32,
    flags: u64,
    stripes: &[(u64, u64, [u8; 16])],
) -> Vec<u8> {
    let mut out = Bytes::new();
    out.u64(length)
        .u64(objectid::FIRST_CHUNK_TREE) // owner
        .u64(stripe_len)
        .u64(flags)
        .u32(u32::try_from(stripe_len).expect("the stripe length is 64 KiB")) // io_align
        .u32(u32::try_from(stripe_len).expect("the stripe length is 64 KiB")) // io_width
        .u32(sectorsize)
        .u16(u16::try_from(stripes.len()).expect("at most two copies"))
        .u16(1); // sub_stripes, which only means anything for raid10
    for (devid, physical, dev_uuid) in stripes {
        out.u64(*devid).u64(*physical).raw(dev_uuid);
    }
    out.finish()
}
