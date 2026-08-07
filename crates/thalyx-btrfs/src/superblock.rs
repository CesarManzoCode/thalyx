//! Reading a Btrfs superblock back off a device: what it is, and what it is called.
//!
//! This is the other half of the label decision recorded in
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md`. An installed machine cannot
//! be told which device holds its store — the kernel command line is compiled
//! into the kernel and the disk has a different name on every machine — so it
//! looks for a name Thalyx itself wrote. Asking a device what it is called is
//! this module; deciding what to do with the answers is
//! `crates/thalyx-cli/src/store_disk.rs`, which is where the refusals live.
//!
//! Everything here reads and nothing repairs. A device that is not Btrfs, a
//! superblock with a checksum that does not match, a short read — each is its own
//! answer, and none of them is "no label". Rule 10: a failure to read is not a
//! failure to exist, and here the difference decides between *this is somebody
//! else's disk* and *this is our disk and it is damaged*.

use crate::crc32c;
use crate::disk::MAGIC;
use crate::format::{LABEL_LEN, super_offset};
use crate::layout::{SUPERBLOCK_LEN, SUPERBLOCKS};

/// What a device turned out to be.
#[derive(Debug, PartialEq, Eq)]
pub enum Identity {
    /// A Btrfs filesystem whose superblock checksum verifies.
    Btrfs {
        /// Empty when the filesystem has no label, which is a real answer and
        /// not a missing one.
        label: String,
        fsid: [u8; 16],
    },
    /// Read fine, and it is not Btrfs. Somebody else's disk, or a blank one.
    NotBtrfs,
    /// The magic is there and the checksum is not what it should be.
    ///
    /// Kept apart from [`Identity::NotBtrfs`] because they call for opposite
    /// actions: one means look elsewhere, the other means this is a Btrfs
    /// filesystem that has been damaged, and treating it as "not Btrfs" would
    /// mean a machine walking past its own broken store in silence.
    Corrupt { expected: [u8; 4], found: [u8; 4] },
}

/// Why the question could not be asked at all.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("opening {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("reading the superblock of {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// Shorter than the first superblock's offset plus its size. Not an error
    /// about Btrfs — the device cannot hold a superblock, so there is no claim to
    /// evaluate.
    #[error("{path} is {size} bytes, which is too small to hold a superblock at {offset}")]
    TooShort {
        path: String,
        size: u64,
        offset: u64,
    },
}

/// Ask a device what filesystem it holds.
///
/// Reads only the primary superblock. The mirrors exist for a kernel recovering
/// from a damaged primary, and using one here would answer a different question:
/// a machine looking for its store wants to know what it would find on mounting,
/// and mounting reads the primary.
pub fn identify(path: &std::path::Path) -> Result<Identity, ReadError> {
    use std::io::{Read, Seek, SeekFrom};

    let display = path.display().to_string();
    let mut file = std::fs::File::open(path).map_err(|source| ReadError::Open {
        path: display.clone(),
        source,
    })?;

    let offset = SUPERBLOCKS[0];
    let size = file
        .seek(SeekFrom::End(0))
        .map_err(|source| ReadError::Read {
            path: display.clone(),
            source,
        })?;
    if size < offset + SUPERBLOCK_LEN as u64 {
        return Err(ReadError::TooShort {
            path: display,
            size,
            offset,
        });
    }

    file.seek(SeekFrom::Start(offset))
        .map_err(|source| ReadError::Read {
            path: display.clone(),
            source,
        })?;
    let mut block = vec![0u8; SUPERBLOCK_LEN];
    file.read_exact(&mut block)
        .map_err(|source| ReadError::Read {
            path: display,
            source,
        })?;

    Ok(interpret(&block))
}

/// What a 4096-byte block claims to be. Separated from the reading so that every
/// branch can be exercised without a device.
pub fn interpret(block: &[u8]) -> Identity {
    if block.len() < SUPERBLOCK_LEN {
        return Identity::NotBtrfs;
    }

    let magic = u64::from_le_bytes(block[64..72].try_into().expect("8 bytes"));
    if magic != MAGIC {
        return Identity::NotBtrfs;
    }

    // The magic is checked first and the checksum second, on purpose. A block
    // that is not Btrfs will fail the checksum too, and reporting that as
    // corruption would tell a person to repair somebody else's disk.
    let expected = crc32c::checksum(&block[32..]);
    let found: [u8; 4] = block[0..4].try_into().expect("4 bytes");
    if expected != found {
        return Identity::Corrupt { expected, found };
    }

    let label = &block[super_offset::LABEL..super_offset::LABEL + LABEL_LEN];
    let label = label
        .iter()
        .position(|byte| *byte == 0)
        .map_or(label, |end| &label[..end]);

    Identity::Btrfs {
        // Lossy, and it has to be: a label is arbitrary bytes a person or another
        // tool wrote, and a store that refused to identify itself because its
        // label was not UTF-8 would be a store nobody could find. What is
        // matched against `LABEL` is the decoded string, so invalid bytes become
        // replacement characters and simply do not match.
        label: String::from_utf8_lossy(label).into_owned(),
        fsid: block[32..48].try_into().expect("16 bytes"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::LABEL;

    /// A block that would pass, so each test can break exactly one thing.
    fn valid(label: &str) -> Vec<u8> {
        let mut block = vec![0u8; SUPERBLOCK_LEN];
        block[32..48].copy_from_slice(&[0xAB; 16]);
        block[64..72].copy_from_slice(&MAGIC.to_le_bytes());
        block[super_offset::LABEL..super_offset::LABEL + label.len()]
            .copy_from_slice(label.as_bytes());
        let digest = crc32c::checksum(&block[32..]);
        block[0..4].copy_from_slice(&digest);
        block
    }

    #[test]
    fn a_thalyx_store_is_recognised_by_the_label_it_was_given() {
        assert_eq!(
            interpret(&valid(LABEL)),
            Identity::Btrfs {
                label: LABEL.to_string(),
                fsid: [0xAB; 16],
            }
        );
    }

    #[test]
    fn a_device_that_is_not_btrfs_is_not_reported_as_a_damaged_one() {
        // The control this whole module exists for. Almost every device on a
        // machine is not Btrfs, so if "not ours" arrived as "ours and broken",
        // a machine looking for its store would report damage on every disk it
        // walked past.
        let mut block = valid(LABEL);
        block[64..72].copy_from_slice(&0u64.to_le_bytes());
        assert_eq!(interpret(&block), Identity::NotBtrfs);

        assert_eq!(interpret(&vec![0u8; SUPERBLOCK_LEN]), Identity::NotBtrfs);
    }

    #[test]
    fn a_btrfs_superblock_with_a_bad_checksum_is_corrupt_and_not_absent() {
        // Rule 10 at the point where it decides an action. This device is ours;
        // saying "no store here" would send the human to look for a disk they are
        // holding.
        let mut block = valid(LABEL);
        block[super_offset::LABEL] ^= 0xFF; // change the label, leave the checksum
        match interpret(&block) {
            Identity::Corrupt { expected, found } => assert_ne!(expected, found),
            other => panic!("a tampered superblock read as {other:?}"),
        }
    }

    #[test]
    fn a_filesystem_with_no_label_says_so_rather_than_reporting_one_byte_of_zero() {
        // An unlabelled Btrfs is a real thing and a legitimate answer. It is not
        // a Thalyx store, and the distinction is that it is a filesystem with no
        // name rather than a device with no filesystem.
        match interpret(&valid("")) {
            Identity::Btrfs { label, .. } => assert_eq!(label, ""),
            other => panic!("an unlabelled Btrfs read as {other:?}"),
        }
    }

    #[test]
    fn a_label_shorter_than_its_field_does_not_drag_in_the_zero_padding() {
        match interpret(&valid("x")) {
            Identity::Btrfs { label, .. } => assert_eq!(label.len(), 1),
            other => panic!("read as {other:?}"),
        }
    }

    #[test]
    fn a_label_that_is_not_utf8_identifies_the_filesystem_without_matching_ours() {
        let mut block = valid("");
        block[super_offset::LABEL..super_offset::LABEL + 2].copy_from_slice(&[0xFF, 0xFE]);
        let digest = crc32c::checksum(&block[32..]);
        block[0..4].copy_from_slice(&digest);
        match interpret(&block) {
            Identity::Btrfs { label, .. } => assert_ne!(label, LABEL),
            other => panic!("read as {other:?}"),
        }
    }

    #[test]
    fn a_short_block_is_refused_rather_than_read_past() {
        assert_eq!(interpret(&[0u8; 16]), Identity::NotBtrfs);
    }
}
