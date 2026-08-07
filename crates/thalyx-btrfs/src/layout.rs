//! Where everything goes on the device, decided arithmetically and once.
//!
//! Btrfs has two address spaces. A *physical* address is a byte offset into the
//! device; a *logical* address is what every tree pointer and every key holds,
//! and the chunk tree is the map between them. Nothing else in this crate is
//! allowed to conflate them, which is why they are separate types here.
//!
//! ## Why this layout and not the one `mkfs.btrfs` produces
//!
//! `mkfs.btrfs` builds a temporary set of chunks near the start of the device,
//! then relocates onto the final ones, which leaves a hole and a set of
//! addresses that look arbitrary because they are a residue rather than a
//! decision. This crate writes the final layout directly: three chunks, packed
//! from 1 MiB, in one pass.
//!
//! That buys one property worth naming. Every superblock mirror sits outside
//! every chunk, so no block group ever overlaps one. A chunk that covers a
//! mirror is legal — `mkfs.btrfs` produces them — but it means the kernel has to
//! exclude the superblock's sectors from allocation inside that block group, and
//! a filesystem that got the exclusion wrong would allocate a tree block on top
//! of its own backup superblock. Not overlapping is a smaller thing to be right
//! about than excluding correctly.

/// A byte offset into the device.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Physical(pub u64);

/// An address in the space that tree pointers and keys use.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Logical(pub u64);

pub const KIB: u64 = 1024;
pub const MIB: u64 = 1024 * KIB;

/// The first megabyte of the device is never allocated: the first superblock
/// lives at 64 KiB, and the kernel treats the region as reserved.
pub const RESERVED: u64 = MIB;

/// Where a superblock and its mirrors go.
///
/// From `btrfs_sb_offset`: the first is at 64 KiB, and each later one is 16 KiB
/// shifted left by twelve bits more than the last.
pub const SUPERBLOCKS: [u64; 3] = [64 * KIB, (16 * KIB) << 12, (16 * KIB) << 24];

/// The size of one superblock, and the amount written at each mirror.
pub const SUPERBLOCK_LEN: usize = 4096;

/// One chunk: a logical range, its replicas on the device, and what it holds.
pub struct Chunk {
    pub logical: Logical,
    pub length: u64,
    pub flags: u64,
    /// One entry per copy. Each covers the whole logical length, so a chunk with
    /// two stripes takes `2 * length` of the device.
    pub stripes: Vec<Physical>,
}

impl Chunk {
    /// Whether a logical address falls in this chunk.
    pub fn holds(&self, address: Logical) -> bool {
        address.0 >= self.logical.0 && address.0 < self.logical.0 + self.length
    }

    /// The physical offsets that a logical address inside this chunk maps to —
    /// one per copy, all of which have to be written.
    pub fn map(&self, address: Logical) -> Vec<Physical> {
        let within = address.0 - self.logical.0;
        self.stripes
            .iter()
            .map(|stripe| Physical(stripe.0 + within))
            .collect()
    }
}

/// The geometry of a store, fixed at these values for every device.
///
/// Not configurable, and that is a decision rather than an omission: a store
/// whose node size varied by device would be a store whose failures varied by
/// device, and there is one machine that can verify any of this.
pub struct Geometry {
    pub sectorsize: u32,
    pub nodesize: u64,
    pub stripe_len: u64,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            // The page size of every machine this runs on. Btrfs supports a
            // sector size below the page size only for reads.
            sectorsize: 4096,
            nodesize: 16 * KIB,
            stripe_len: 64 * KIB,
        }
    }
}

/// The smallest device this will format.
///
/// Set by the second superblock rather than by the chunks: the chunks need about
/// 33 MiB, and a device that cannot hold the mirror at 64 MiB would get a store
/// with one superblock. Refusing is better than silently producing a store whose
/// only copy of its own root pointer is one bad sector from gone — and "it was
/// too small" is a fact a person can act on, which "it mounted until it didn't"
/// is not.
pub const MINIMUM_DEVICE: u64 = 128 * MIB;

/// The three chunks, packed from the end of the reserved region.
///
/// Sizes chosen small on purpose. The kernel allocates more of both metadata and
/// data on demand the first time it needs to, so making them large here would
/// only mean a longer write for a store that has nothing in it yet.
pub struct Plan {
    pub geometry: Geometry,
    pub chunks: Vec<Chunk>,
    /// The bytes of the device the chunks occupy, which is what a device item
    /// reports as used.
    pub device_used: u64,
}

impl Plan {
    pub fn new(geometry: Geometry) -> Self {
        use crate::disk::block_group;

        let system = 4 * MIB;
        let metadata = 8 * MIB;
        let data = 8 * MIB;

        // Physical and logical are laid out in the same order, and both are
        // packed, but they do not advance together: a DUP chunk takes twice as
        // much of the device as it does of the logical space. Conflating the two
        // is the mistake this struct's two types exist to make impossible.
        let mut physical = RESERVED;
        let mut take = |length: u64, copies: usize| {
            let stripes = (0..copies)
                .map(|copy| Physical(physical + length * copy as u64))
                .collect::<Vec<_>>();
            physical += length * copies as u64;
            stripes
        };
        let system_stripes = take(system, 2);
        let metadata_stripes = take(metadata, 2);
        let data_stripes = take(data, 1);
        let device_used = physical - RESERVED;

        let mut logical = RESERVED;
        let mut place = |length: u64, flags: u64, stripes: Vec<Physical>| {
            let chunk = Chunk {
                logical: Logical(logical),
                length,
                flags,
                stripes,
            };
            logical += length;
            chunk
        };

        let chunks = vec![
            place(
                system,
                block_group::SYSTEM | block_group::DUP,
                system_stripes,
            ),
            place(
                metadata,
                block_group::METADATA | block_group::DUP,
                metadata_stripes,
            ),
            place(data, block_group::DATA, data_stripes),
        ];

        Self {
            geometry,
            chunks,
            device_used,
        }
    }

    /// The chunk the chunk tree itself lives in.
    ///
    /// It has to be a system chunk: the superblock carries a copy of the system
    /// chunks and nothing else, so a chunk tree anywhere else could not be found
    /// until after the chunk tree had been read.
    pub fn system(&self) -> &Chunk {
        self.chunks
            .iter()
            .find(|chunk| chunk.flags & crate::disk::block_group::SYSTEM != 0)
            .expect("the plan always has a system chunk")
    }

    /// The chunk every tree except the chunk tree lives in.
    pub fn metadata(&self) -> &Chunk {
        self.chunks
            .iter()
            .find(|chunk| chunk.flags & crate::disk::block_group::METADATA != 0)
            .expect("the plan always has a metadata chunk")
    }

    /// Every physical offset a logical address has to be written to.
    pub fn map(&self, address: Logical) -> Vec<Physical> {
        self.chunks
            .iter()
            .find(|chunk| chunk.holds(address))
            .map(|chunk| chunk.map(address))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_chunk_covers_a_superblock_or_its_mirrors() {
        // The property this layout exists for. A chunk overlapping a superblock
        // is legal and mkfs.btrfs writes them, but then the kernel has to keep
        // the superblock's sectors out of that block group's free space, and
        // getting that wrong means a tree block written over the backup
        // superblock — a filesystem that is fine until the moment it needs the
        // backup.
        let plan = Plan::new(Geometry::default());
        for chunk in &plan.chunks {
            for stripe in &chunk.stripes {
                let span = stripe.0..stripe.0 + chunk.length;
                for mirror in SUPERBLOCKS {
                    assert!(
                        !span.contains(&mirror)
                            && !span.contains(&(mirror + SUPERBLOCK_LEN as u64 - 1)),
                        "chunk stripe at {} covers the superblock at {mirror}",
                        stripe.0
                    );
                }
            }
        }
    }

    #[test]
    fn nothing_is_allocated_in_the_reserved_first_megabyte() {
        let plan = Plan::new(Geometry::default());
        for chunk in &plan.chunks {
            for stripe in &chunk.stripes {
                assert!(
                    stripe.0 >= RESERVED,
                    "a stripe at {} is inside the reserved region",
                    stripe.0
                );
            }
        }
    }

    #[test]
    fn every_stripe_is_aligned_to_the_stripe_length() {
        let plan = Plan::new(Geometry::default());
        for chunk in &plan.chunks {
            for stripe in &chunk.stripes {
                assert_eq!(stripe.0 % plan.geometry.stripe_len, 0);
            }
        }
    }

    #[test]
    fn stripes_do_not_overlap_each_other() {
        // A DUP chunk whose two copies overlapped would report two copies and
        // have one, which is worse than having one: the machine would believe
        // it could survive a bad sector it cannot.
        let plan = Plan::new(Geometry::default());
        let mut spans: Vec<(u64, u64)> = plan
            .chunks
            .iter()
            .flat_map(|chunk| chunk.stripes.iter().map(|s| (s.0, s.0 + chunk.length)))
            .collect();
        spans.sort_unstable();
        for pair in spans.windows(2) {
            assert!(
                pair[0].1 <= pair[1].0,
                "{:?} overlaps {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn the_logical_space_is_contiguous_and_starts_where_the_device_does() {
        let plan = Plan::new(Geometry::default());
        let mut expected = RESERVED;
        for chunk in &plan.chunks {
            assert_eq!(chunk.logical.0, expected);
            expected += chunk.length;
        }
    }

    #[test]
    fn a_dup_chunk_takes_twice_as_much_device_as_logical_space() {
        // The arithmetic that the two address types exist to keep apart. If
        // physical and logical advanced together, the metadata chunk's second
        // copy would land on top of the data chunk.
        let plan = Plan::new(Geometry::default());
        let logical: u64 = plan.chunks.iter().map(|c| c.length).sum();
        let dup: u64 = plan
            .chunks
            .iter()
            .filter(|c| c.stripes.len() == 2)
            .map(|c| c.length)
            .sum();
        assert_eq!(plan.device_used, logical + dup);
    }

    #[test]
    fn a_logical_address_in_a_dup_chunk_maps_to_both_copies() {
        let plan = Plan::new(Geometry::default());
        let metadata = plan.metadata();
        let inside = Logical(metadata.logical.0 + 2 * plan.geometry.nodesize);
        let mapped = plan.map(inside);
        assert_eq!(mapped.len(), 2, "a DUP chunk has two copies to write");
        for (stripe, physical) in metadata.stripes.iter().zip(&mapped) {
            assert_eq!(physical.0 - stripe.0, 2 * plan.geometry.nodesize);
        }
    }

    #[test]
    fn an_address_outside_every_chunk_maps_nowhere_rather_than_to_zero() {
        // Rule 9. An unmapped address returning offset 0 would send a write to
        // the front of the device, which is where the superblock is.
        let plan = Plan::new(Geometry::default());
        assert!(plan.map(Logical(0)).is_empty());
        assert!(plan.map(Logical(4 * 1024 * MIB)).is_empty());
    }

    #[test]
    fn the_chunk_tree_can_live_in_the_system_chunk() {
        let plan = Plan::new(Geometry::default());
        assert!(plan.system().length >= plan.geometry.nodesize);
    }
}
