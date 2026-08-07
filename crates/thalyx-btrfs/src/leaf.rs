//! One Btrfs leaf: a header, an array of item descriptors growing forward, and
//! the item bodies growing backward from the end.
//!
//! Every tree this crate writes is a single leaf, so there is no node builder and
//! no splitting. That is a real limit and it is stated rather than hidden: see
//! [`Leaf::add`], which refuses rather than overflowing.

use crate::crc32c;
use crate::disk::{Bytes, HEADER_FLAG_WRITTEN, HEADER_MIXED_BACKREF_REV, Key};

/// `struct btrfs_header`: 32 + 16 + 8 + 8 + 16 + 8 + 8 + 4 + 1.
pub const HEADER_LEN: usize = 101;

/// `struct btrfs_item`: a key, then the body's offset and size.
const ITEM_LEN: usize = Key::ENCODED_LEN + 4 + 4;

/// A leaf under construction.
pub struct Leaf {
    nodesize: usize,
    bytenr: u64,
    owner: u64,
    generation: u64,
    fsid: [u8; 16],
    chunk_tree_uuid: [u8; 16],
    items: Vec<(Key, Vec<u8>)>,
}

/// Why a leaf could not be built.
#[derive(Debug, thiserror::Error)]
pub enum LeafError {
    /// Items are stored in key order and searched by bisection. A leaf whose
    /// items are out of order is not a leaf with a mistake in it — it is a leaf
    /// in which the kernel's lookups return "absent" for things that are there.
    ///
    /// Refused rather than sorted. Sorting here would make a caller that built
    /// its items in the wrong order work anyway, and the next caller would
    /// inherit the habit into a tree with more than one leaf, where the order
    /// also decides which key goes in which block.
    #[error(
        "leaf for tree {owner} has {later:?} after {earlier:?}, which is out of key order; \
         the kernel bisects on these, so the later one would be unfindable"
    )]
    OutOfOrder {
        owner: u64,
        earlier: Key,
        later: Key,
    },

    /// Every tree here is one leaf, so running out of room is not something to
    /// recover from by splitting: it means the caller asked for a filesystem
    /// this crate does not write.
    #[error(
        "tree {owner} does not fit in one {nodesize}-byte leaf: {items} items need {needed} bytes. \
         This crate writes one leaf per tree and does not split"
    )]
    TooBig {
        owner: u64,
        nodesize: usize,
        items: usize,
        needed: usize,
    },
}

impl Leaf {
    pub fn new(
        nodesize: usize,
        bytenr: u64,
        owner: u64,
        generation: u64,
        fsid: [u8; 16],
        chunk_tree_uuid: [u8; 16],
    ) -> Self {
        Self {
            nodesize,
            bytenr,
            owner,
            generation,
            fsid,
            chunk_tree_uuid,
            items: Vec::new(),
        }
    }

    /// Append an item, which must sort after everything already added.
    pub fn add(&mut self, key: Key, body: Vec<u8>) -> Result<(), LeafError> {
        if let Some((earlier, _)) = self.items.last()
            && *earlier >= key
        {
            return Err(LeafError::OutOfOrder {
                owner: self.owner,
                earlier: *earlier,
                later: key,
            });
        }
        self.items.push((key, body));
        Ok(())
    }

    /// How many bytes the items currently occupy, headers included.
    fn occupied(&self) -> usize {
        HEADER_LEN
            + self
                .items
                .iter()
                .map(|(_, body)| ITEM_LEN + body.len())
                .sum::<usize>()
    }

    /// The finished block, checksum in place.
    pub fn build(&self) -> Result<Vec<u8>, LeafError> {
        if self.occupied() > self.nodesize {
            return Err(LeafError::TooBig {
                owner: self.owner,
                nodesize: self.nodesize,
                items: self.items.len(),
                needed: self.occupied(),
            });
        }

        let mut block = vec![0u8; self.nodesize];

        // An item's `offset` is measured from the end of the header, not from
        // the start of the block. Getting that wrong by 101 bytes produces a
        // leaf whose item bodies all read as whatever is 101 bytes away.
        let mut cursor = self.nodesize - HEADER_LEN;
        for (index, (key, body)) in self.items.iter().enumerate() {
            cursor -= body.len();

            let mut descriptor = Bytes::new();
            descriptor
                .key(*key)
                .u32(u32::try_from(cursor).expect("an offset inside one node"))
                .u32(u32::try_from(body.len()).expect("an item smaller than one node"));

            let at = HEADER_LEN + index * ITEM_LEN;
            block[at..at + ITEM_LEN].copy_from_slice(&descriptor.finish());
            block[HEADER_LEN + cursor..HEADER_LEN + cursor + body.len()].copy_from_slice(body);
        }

        let mut header = Bytes::new();
        header
            .raw(&self.fsid)
            .u64(self.bytenr)
            .u64(HEADER_FLAG_WRITTEN | HEADER_MIXED_BACKREF_REV)
            .raw(&self.chunk_tree_uuid)
            .u64(self.generation)
            .u64(self.owner)
            .u32(u32::try_from(self.items.len()).expect("few items"))
            .u8(0); // level: every tree here is one leaf
        block[32..HEADER_LEN].copy_from_slice(&header.finish());

        // From the end of the checksum field to the end of the block. Starting
        // at zero instead would checksum the field being computed and produce a
        // number that is stable, wrong, and impossible to notice by reading.
        let digest = crc32c::checksum(&block[32..]);
        block[0..4].copy_from_slice(&digest);
        Ok(block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::kind;

    fn leaf() -> Leaf {
        Leaf::new(16384, 1_048_576, 1, 1, [1u8; 16], [2u8; 16])
    }

    #[test]
    fn an_item_added_out_of_order_is_refused_rather_than_sorted() {
        let mut leaf = leaf();
        leaf.add(Key::new(20, kind::INODE_ITEM, 0), vec![0; 8])
            .unwrap();
        let error = leaf
            .add(Key::new(10, kind::INODE_ITEM, 0), vec![0; 8])
            .expect_err("a smaller key after a larger one");
        assert!(matches!(error, LeafError::OutOfOrder { .. }), "{error}");
    }

    #[test]
    fn keys_sort_by_objectid_then_type_then_offset() {
        // The ordering is what decides that the root tree's `DATA_RELOC_TREE`
        // item comes last, and it comes last only because -9 is written as an
        // unsigned two's complement. A signed comparison would put it first and
        // every lookup past it would stop early.
        let mut keys = vec![
            Key::new(crate::disk::objectid::DATA_RELOC_TREE, kind::ROOT_ITEM, 0),
            Key::new(5, kind::ROOT_ITEM, 0),
            Key::new(5, kind::INODE_REF, 6),
            Key::new(5, kind::INODE_REF, 1),
        ];
        keys.sort();
        assert_eq!(
            keys,
            vec![
                Key::new(5, kind::INODE_REF, 1),
                Key::new(5, kind::INODE_REF, 6),
                Key::new(5, kind::ROOT_ITEM, 0),
                Key::new(crate::disk::objectid::DATA_RELOC_TREE, kind::ROOT_ITEM, 0),
            ]
        );
    }

    #[test]
    fn a_tree_that_does_not_fit_in_one_leaf_is_refused_and_says_so() {
        let mut leaf = leaf();
        for index in 0..40 {
            let _ = leaf.add(Key::new(index, kind::INODE_ITEM, 0), vec![0; 500]);
        }
        let error = leaf
            .build()
            .expect_err("40 items of 500 bytes exceed 16 KiB");
        assert!(matches!(error, LeafError::TooBig { .. }), "{error}");
    }

    #[test]
    fn an_empty_leaf_is_a_whole_block_with_a_checksum() {
        // The checksum tree is written empty, so this is not a degenerate case
        // being tolerated — it is one of the trees.
        let block = leaf().build().expect("an empty leaf is valid");
        assert_eq!(block.len(), 16384);
        assert_eq!(&block[0..4], &crc32c::checksum(&block[32..]));
        assert_ne!(&block[0..4], &[0, 0, 0, 0], "nothing was checksummed");
    }

    #[test]
    fn item_bodies_are_placed_at_offsets_measured_from_the_end_of_the_header() {
        let mut leaf = leaf();
        let body = vec![0xAB; 16];
        leaf.add(Key::new(1, kind::INODE_ITEM, 0), body.clone())
            .unwrap();
        let block = leaf.build().unwrap();

        // Read the offset back out of the item descriptor and follow it, which
        // is what the kernel does. A body placed relative to the block start
        // would still be in the block and would be found 101 bytes away.
        let at = HEADER_LEN + Key::ENCODED_LEN;
        let offset = u32::from_le_bytes(block[at..at + 4].try_into().unwrap()) as usize;
        let size = u32::from_le_bytes(block[at + 4..at + 8].try_into().unwrap()) as usize;
        assert_eq!(size, body.len());
        assert_eq!(
            &block[HEADER_LEN + offset..HEADER_LEN + offset + size],
            &body[..]
        );
    }

    #[test]
    fn every_tree_block_says_it_uses_the_mixed_backref_format() {
        // The bit whose absence produced a filesystem that parsed perfectly and
        // whose every extent reference `btrfs check` called a mismatch. It is
        // one bit at position 56 of a field whose other set bit is at position
        // 0, so a hexdump of the header looks entirely ordinary without it.
        let block = leaf().build().unwrap();
        let flags = u64::from_le_bytes(block[56..64].try_into().unwrap());
        assert_eq!(flags & HEADER_MIXED_BACKREF_REV, HEADER_MIXED_BACKREF_REV);
        assert_eq!(flags & HEADER_FLAG_WRITTEN, HEADER_FLAG_WRITTEN);
    }
}
