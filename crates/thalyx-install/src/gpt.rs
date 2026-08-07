//! The partition table, written byte by byte.
//!
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md`: the ISO installs Thalyx onto
//! a disk and is then removed, so something has to divide that disk into the two
//! parts an installed machine needs — an EFI system partition the firmware can find
//! a kernel in, and the store everything else lives on. `sgdisk`, `parted` and
//! `fdisk` are all the same answer to that, and it is the answer
//! `vault/01-Filosofia/Filosofia-Fundacional.md` refuses: the image holds the Linux
//! kernel and one program.
//!
//! Fourth time this project has met that shape — `bpftool`, `cpio`, `btrfs` — and
//! the fourth time with the same answer.
//!
//! ## What this is not
//!
//! It is not a partition editor. It writes one table, of two partitions, over
//! whatever was there. It cannot add a partition to an existing table, cannot move
//! one, and does not read one back at all — what an installed disk already holds is
//! asked of the kernel, in [`crate::partitions`]. An installer replaces a disk's
//! contents; it does not negotiate with them.
//!
//! ## How the numbers here are known to be right
//!
//! `tests/uapi_efi.h` is `block/partitions/efi.h`, captured verbatim, and
//! `tests/layout.rs` checks every structure size and every field offset this module
//! writes against it. That is the same instrument `thalyx-btrfs` uses and it has the
//! same limit: it establishes that the bytes are in the places the header names, not
//! that a kernel accepts the result.
//!
//! **What establishes that is a kernel reading it.** A GPT whose header checksum or
//! entry-array checksum is wrong is not read as a broken GPT — it is not read at
//! all, and Linux falls back to the protective MBR and reports one partition
//! covering the whole disk. That failure looks like success from inside this crate,
//! which is why stage 20 of `dev/verify.sh` hands the table to `losetup -P` and
//! asks the kernel what partitions it found.

use std::io::{Seek, SeekFrom, Write};

/// The only logical sector size this module writes for.
///
/// A 4Kn disk reports 4096, and every LBA below would then be at four times the
/// byte offset the kernel looks at — the table would simply not be found. That is
/// refused by [`crate::install`] rather than written, because a partition table
/// nobody can read is indistinguishable from a disk nobody touched.
pub const SECTOR: u64 = 512;

/// `sizeof(gpt_entry)`. Checked against the captured header.
pub const ENTRY_LEN: usize = 128;

/// How many entries the array holds. 128 is what every tool writes and what the
/// UEFI specification requires firmware to support at minimum; the number is in
/// the header on disk, so it is not a guess anyone else has to share.
pub const ENTRY_COUNT: u32 = 128;

/// `sizeof(gpt_header)` — the bytes the header checksum covers.
///
/// The rest of the logical block after this is reserved and must be zero, which is
/// why the checksum is over 92 bytes and not over the sector.
pub const HEADER_LEN: usize = 92;

/// How many sectors the entry array occupies: 128 × 128 bytes.
pub const ENTRY_ARRAY_SECTORS: u64 = (ENTRY_COUNT as u64 * ENTRY_LEN as u64) / SECTOR;

/// LBA 0 is the protective MBR, 1 the header, 2..34 the array.
pub const FIRST_USABLE_LBA: u64 = 2 + ENTRY_ARRAY_SECTORS;

/// `GPT_HEADER_SIGNATURE`, "EFI PART" read as a little-endian u64.
const SIGNATURE: u64 = 0x5452_4150_2049_4645;

/// `GPT_HEADER_REVISION_V1`.
const REVISION: u32 = 0x0001_0000;

/// `EFI_PMBR_OSTYPE_EFI_GPT`, the type that says "everything here belongs to a GPT".
const PROTECTIVE_TYPE: u8 = 0xEE;

/// `MSDOS_MBR_SIGNATURE`.
const MBR_SIGNATURE: u16 = 0xAA55;

/// A GUID in the byte order a partition table stores one.
///
/// **Mixed-endian, and this is the field most likely to be written wrong.** The
/// first three fields are little-endian and the last eight bytes are not, so a GUID
/// written as sixteen big-endian bytes is a different, valid-looking GUID — and the
/// consequence of getting the ESP's wrong is a firmware that boots past a partition
/// it was supposed to look in. `GUID_INIT` in `tests/linux_uuid.h` states the order
/// in its own text and `tests/layout.rs` reproduces it from there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Guid([u8; 16]);

impl Guid {
    /// From the five fields a GUID is written as, e.g. `C12A7328-F81F-11D2-BA4B-…`.
    pub const fn new(a: u32, b: u16, c: u16, rest: [u8; 8]) -> Self {
        let a = a.to_le_bytes();
        let b = b.to_le_bytes();
        let c = c.to_le_bytes();
        Self([
            a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], rest[0], rest[1], rest[2], rest[3],
            rest[4], rest[5], rest[6], rest[7],
        ])
    }

    /// A fresh one, for a disk or a partition nobody has named yet.
    pub fn random() -> Self {
        // `as_fields` hands back the five fields in the order they are written,
        // which is what `new` above expects — so the version and variant bits end
        // up where a reader looks for them. Taking `as_bytes` instead would produce
        // sixteen equally random bytes that display as a different GUID, and the
        // mistake would be invisible until somebody compared two tools' output.
        let fresh = uuid::Uuid::new_v4();
        let (a, b, c, rest) = fresh.as_fields();
        Self::new(a, b, c, *rest)
    }

    pub fn bytes(&self) -> [u8; 16] {
        self.0
    }
}

/// `PARTITION_SYSTEM_GUID` — the type a UEFI firmware looks for.
///
/// Not a name Thalyx picked and not one it may change: firmware enumerates
/// partitions of this type looking for `\EFI\BOOT\BOOTX64.EFI`, so a disk with the
/// files in the right places and this number wrong boots nothing. It is in the
/// captured header, and `tests/layout.rs` reads it from there rather than from here.
pub const ESP: Guid = Guid::new(
    0xC12A_7328,
    0xF81F,
    0x11d2,
    [0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B],
);

/// `Linux filesystem data`, the type the store partition is given.
///
/// Thalyx could mint its own type GUID and nothing would break — it finds its store
/// by the Btrfs label and never by this number. It does not, because the one time
/// this field matters is when a human has plugged the disk into another machine to
/// find out what is on it, and a type nothing recognises answers that question with
/// "unknown". Being legible to other tools costs nothing here.
pub const LINUX_FILESYSTEM: Guid = Guid::new(
    0x0FC6_3DAF,
    0x8483,
    0x4772,
    [0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4],
);

#[derive(Debug, thiserror::Error)]
pub enum TableError {
    #[error("a partition name is {0} UTF-16 units and the field holds 36")]
    NameTooLong(usize),

    #[error(
        "partition {which} runs from LBA {first} to {last}, outside the usable \
         range {usable_first}..={usable_last} this disk's table leaves"
    )]
    OutsideUsable {
        which: usize,
        first: u64,
        last: u64,
        usable_first: u64,
        usable_last: u64,
    },

    #[error("partitions {a} and {b} overlap")]
    Overlap { a: usize, b: usize },

    #[error("{0} partitions were asked for and the array holds {ENTRY_COUNT}")]
    TooMany(usize),

    #[error("writing the partition table to {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// One partition, in the terms the table records it.
#[derive(Clone, Debug)]
pub struct Partition {
    pub kind: Guid,
    pub unique: Guid,
    /// Inclusive, both of them, which is how a GPT states it.
    pub first_lba: u64,
    pub last_lba: u64,
    pub name: String,
}

impl Partition {
    pub fn sectors(&self) -> u64 {
        self.last_lba + 1 - self.first_lba
    }

    fn encode(&self) -> Result<[u8; ENTRY_LEN], TableError> {
        let mut entry = [0u8; ENTRY_LEN];
        entry[0..16].copy_from_slice(&self.kind.bytes());
        entry[16..32].copy_from_slice(&self.unique.bytes());
        entry[32..40].copy_from_slice(&self.first_lba.to_le_bytes());
        entry[40..48].copy_from_slice(&self.last_lba.to_le_bytes());
        // Attributes, left zero. The bit worth knowing about is 0, "required
        // partition", which asks firmware and utilities not to delete it. It is
        // deliberately not set: a human who decides to wipe this disk with somebody
        // else's tool should not have to fight Thalyx to do it.
        let units: Vec<u16> = self.name.encode_utf16().collect();
        if units.len() > 36 {
            return Err(TableError::NameTooLong(units.len()));
        }
        for (index, unit) in units.iter().enumerate() {
            entry[56 + index * 2..58 + index * 2].copy_from_slice(&unit.to_le_bytes());
        }
        Ok(entry)
    }
}

/// A whole table, for a device of a known size.
pub struct Table {
    pub disk_guid: Guid,
    /// The device's size, in 512-byte sectors.
    pub sectors: u64,
    pub partitions: Vec<Partition>,
}

impl Table {
    /// The first LBA a partition may start at.
    pub fn first_usable(&self) -> u64 {
        FIRST_USABLE_LBA
    }

    /// The last LBA a partition may end at.
    ///
    /// The backup header takes the final sector and the backup array the 32 before
    /// it, so the usable range stops 34 sectors from the end. Getting this one too
    /// large produces a table Linux reads and warns about, and a filesystem whose
    /// last blocks are underneath the backup table.
    pub fn last_usable(&self) -> u64 {
        self.sectors - ENTRY_ARRAY_SECTORS - 2
    }

    /// Where the backup entry array starts.
    fn backup_array_lba(&self) -> u64 {
        self.sectors - ENTRY_ARRAY_SECTORS - 1
    }

    /// Every partition, refused rather than adjusted if it will not fit.
    ///
    /// An installer that quietly moved a partition to make it fit would produce a
    /// disk whose layout is not the one anything else was told about.
    fn check(&self) -> Result<(), TableError> {
        if self.partitions.len() > ENTRY_COUNT as usize {
            return Err(TableError::TooMany(self.partitions.len()));
        }
        for (which, partition) in self.partitions.iter().enumerate() {
            if partition.first_lba < self.first_usable()
                || partition.last_lba > self.last_usable()
                || partition.last_lba < partition.first_lba
            {
                return Err(TableError::OutsideUsable {
                    which: which + 1,
                    first: partition.first_lba,
                    last: partition.last_lba,
                    usable_first: self.first_usable(),
                    usable_last: self.last_usable(),
                });
            }
        }
        for a in 0..self.partitions.len() {
            for b in a + 1..self.partitions.len() {
                let (one, other) = (&self.partitions[a], &self.partitions[b]);
                if one.first_lba <= other.last_lba && other.first_lba <= one.last_lba {
                    return Err(TableError::Overlap { a: a + 1, b: b + 1 });
                }
            }
        }
        Ok(())
    }

    /// The entry array, both copies of which are these same bytes.
    pub fn entry_array(&self) -> Result<Vec<u8>, TableError> {
        let mut array = vec![0u8; ENTRY_COUNT as usize * ENTRY_LEN];
        for (index, partition) in self.partitions.iter().enumerate() {
            let at = index * ENTRY_LEN;
            array[at..at + ENTRY_LEN].copy_from_slice(&partition.encode()?);
        }
        Ok(array)
    }

    /// One header sector. `my_lba` decides whether it is the primary or the backup.
    fn header(&self, my_lba: u64, alternate_lba: u64, entry_lba: u64, array_crc: u32) -> Vec<u8> {
        let mut sector = vec![0u8; SECTOR as usize];
        sector[0..8].copy_from_slice(&SIGNATURE.to_le_bytes());
        sector[8..12].copy_from_slice(&REVISION.to_le_bytes());
        sector[12..16].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
        // 16..20 is header_crc32, filled in below over these same bytes with it
        // zero — which is what it already is.
        // 20..24 is reserved and stays zero.
        sector[24..32].copy_from_slice(&my_lba.to_le_bytes());
        sector[32..40].copy_from_slice(&alternate_lba.to_le_bytes());
        sector[40..48].copy_from_slice(&self.first_usable().to_le_bytes());
        sector[48..56].copy_from_slice(&self.last_usable().to_le_bytes());
        sector[56..72].copy_from_slice(&self.disk_guid.bytes());
        sector[72..80].copy_from_slice(&entry_lba.to_le_bytes());
        sector[80..84].copy_from_slice(&ENTRY_COUNT.to_le_bytes());
        sector[84..88].copy_from_slice(&(ENTRY_LEN as u32).to_le_bytes());
        sector[88..92].copy_from_slice(&array_crc.to_le_bytes());

        let crc = crate::crc32::of(&sector[..HEADER_LEN]);
        sector[16..20].copy_from_slice(&crc.to_le_bytes());
        sector
    }

    /// The protective MBR, which exists so that a tool that only knows about MBRs
    /// sees one partition of an unknown type covering the disk, rather than free
    /// space it might offer to use.
    fn protective_mbr(&self) -> Vec<u8> {
        let mut sector = vec![0u8; SECTOR as usize];
        let record = 446;
        sector[record] = 0x00; // not bootable: there is no boot code here to run
        // The CHS fields are dead and the specification still states values for
        // them: 0x000200 is "the sector holding the GPT header", and the end is
        // all-ones because any disk this runs on exceeds what CHS can address.
        sector[record + 1] = 0x00;
        sector[record + 2] = 0x02;
        sector[record + 3] = 0x00;
        sector[record + 4] = PROTECTIVE_TYPE;
        sector[record + 5] = 0xFF;
        sector[record + 6] = 0xFF;
        sector[record + 7] = 0xFF;
        sector[record + 8..record + 12].copy_from_slice(&1u32.to_le_bytes());
        // Everything but LBA 0. Saturated rather than wrapped: a disk over 2 TiB
        // cannot state its size in this field, and 0xFFFFFFFF is what the
        // specification says to put there — a wrap would describe a small disk and
        // invite a tool to write past it.
        let covered = u32::try_from(self.sectors - 1).unwrap_or(u32::MAX);
        sector[record + 12..record + 16].copy_from_slice(&covered.to_le_bytes());
        sector[510..512].copy_from_slice(&MBR_SIGNATURE.to_le_bytes());
        sector
    }

    /// Write the whole table — both copies — onto an open device.
    ///
    /// Both copies always, and the backup is written first. A run interrupted
    /// between them leaves a disk whose primary header points at a backup that is
    /// not there yet, which Linux reports and repairs; the other order leaves a
    /// disk that looks entirely fine and has no recovery copy.
    pub fn write(
        &self,
        path: &std::path::Path,
        file: &mut std::fs::File,
    ) -> Result<(), TableError> {
        self.check()?;
        let array = self.entry_array()?;
        let array_crc = crate::crc32::of(&array);

        let io = |source| TableError::Io {
            path: path.to_path_buf(),
            source,
        };

        let backup_array = self.backup_array_lba();
        let backup_header = self.sectors - 1;

        // Anything that used to identify this disk, removed before anything that
        // does identify it is written. A leftover Btrfs superblock at 64 KiB — this
        // disk's own, from a previous `thalyx disk format` — would keep answering
        // "the whole disk is a Thalyx store" to every tool that asks, alongside the
        // partition that is now the real one. Two answers to "where is the store"
        // is the failure `Construccion-del-ISO.md` refuses to allow.
        let wipe = vec![0u8; 1024 * 1024];
        file.seek(SeekFrom::Start(0)).map_err(io)?;
        file.write_all(&wipe).map_err(io)?;
        file.seek(SeekFrom::Start((self.sectors * SECTOR) - wipe.len() as u64))
            .map_err(io)?;
        file.write_all(&wipe).map_err(io)?;

        file.seek(SeekFrom::Start(backup_array * SECTOR))
            .map_err(io)?;
        file.write_all(&array).map_err(io)?;
        file.seek(SeekFrom::Start(backup_header * SECTOR))
            .map_err(io)?;
        file.write_all(&self.header(backup_header, 1, backup_array, array_crc))
            .map_err(io)?;

        file.seek(SeekFrom::Start(0)).map_err(io)?;
        file.write_all(&self.protective_mbr()).map_err(io)?;
        file.write_all(&self.header(1, backup_header, 2, array_crc))
            .map_err(io)?;
        file.write_all(&array).map_err(io)?;

        file.sync_all().map_err(io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_partitions(sectors: u64) -> Table {
        Table {
            disk_guid: Guid::random(),
            sectors,
            partitions: vec![
                Partition {
                    kind: ESP,
                    unique: Guid::random(),
                    first_lba: 2048,
                    last_lba: 2048 + 2048 - 1,
                    name: "EFI System Partition".into(),
                },
                Partition {
                    kind: LINUX_FILESYSTEM,
                    unique: Guid::random(),
                    first_lba: 4096,
                    last_lba: sectors - 34,
                    name: "Thalyx store".into(),
                },
            ],
        }
    }

    #[test]
    fn the_esp_guid_is_the_one_a_firmware_enumerates() {
        // Written as bytes rather than as fields, because the mixed-endian encoding
        // is the thing being checked and expressing the expectation with the same
        // function that produces it would check nothing. C12A7328-F81F-11D2-BA4B-
        // 00A0C93EC93B, as it lies on a disk.
        assert_eq!(
            ESP.bytes(),
            [
                0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E,
                0xC9, 0x3B
            ]
        );
    }

    #[test]
    fn a_random_guid_is_a_version_four_one_where_a_reader_looks() {
        // The version nibble lives in the high half of the third field's second
        // byte once the field is little-endian, which is byte 7. Building a GUID out
        // of `as_bytes` instead of `as_fields` puts it at byte 6 — sixteen equally
        // random bytes, a GUID that displays differently in every tool, and nothing
        // that fails until two people compare notes.
        let guid = Guid::random().bytes();
        assert_eq!(guid[7] >> 4, 4, "not a version 4 GUID: {guid:02x?}");
        assert_eq!(guid[8] >> 6, 0b10, "not an RFC 4122 variant: {guid:02x?}");
    }

    #[test]
    fn the_header_checksum_covers_itself_as_zero() {
        // The one rule about this field that a reader has to obey and a writer can
        // silently get wrong: the CRC is computed with its own four bytes zeroed. A
        // header checksummed over itself-with-the-checksum-in-it produces a stable
        // number that no reader will ever reproduce, and Linux answers by ignoring
        // the GPT completely and reporting one partition covering the disk.
        let table = two_partitions(1024 * 1024);
        let header = table.header(1, table.sectors - 1, 2, 0);

        let mut zeroed = header.clone();
        zeroed[16..20].copy_from_slice(&[0; 4]);
        assert_eq!(
            u32::from_le_bytes(header[16..20].try_into().unwrap()),
            crate::crc32::of(&zeroed[..HEADER_LEN])
        );
    }

    #[test]
    fn the_two_headers_point_at_each_other_and_at_their_own_array() {
        let table = two_partitions(1024 * 1024);
        let backup_header = table.sectors - 1;
        let primary = table.header(1, backup_header, 2, 0);
        let backup = table.header(backup_header, 1, table.backup_array_lba(), 0);

        let lba =
            |sector: &[u8], at: usize| u64::from_le_bytes(sector[at..at + 8].try_into().unwrap());

        assert_eq!(lba(&primary, 24), 1);
        assert_eq!(lba(&primary, 32), backup_header);
        assert_eq!(lba(&primary, 72), 2);
        assert_eq!(lba(&backup, 24), backup_header);
        assert_eq!(lba(&backup, 32), 1);
        assert_eq!(lba(&backup, 72), table.backup_array_lba());

        // And the two arrays do not overlap the headers that name them. Off by one
        // here writes the backup array over the backup header, which leaves a disk
        // with one working table and a recovery copy that is garbage — invisible
        // until the day it is needed.
        assert!(table.backup_array_lba() + ENTRY_ARRAY_SECTORS <= backup_header);
        assert!(table.last_usable() < table.backup_array_lba());
    }

    #[test]
    fn a_partition_reaching_into_the_backup_table_is_refused_and_not_trimmed() {
        // The mistake is arithmetic and its symptom is not: a store whose last
        // blocks sit under the backup GPT works perfectly until Btrfs allocates
        // that far, and then the disk has neither a usable filesystem nor a
        // recovery copy of its table.
        let mut table = two_partitions(1024 * 1024);
        table.partitions[1].last_lba = table.sectors - 2;
        assert!(matches!(
            table.check(),
            Err(TableError::OutsideUsable { which: 2, .. })
        ));
    }

    #[test]
    fn two_partitions_that_share_a_sector_are_refused() {
        let mut table = two_partitions(1024 * 1024);
        table.partitions[1].first_lba = table.partitions[0].last_lba;
        assert!(matches!(table.check(), Err(TableError::Overlap { .. })));
    }

    #[test]
    fn a_name_longer_than_the_field_is_refused_rather_than_cut() {
        // A truncated partition name is not a smaller problem than a rejected one:
        // the name is what a human reads to decide which disk they are looking at.
        let mut table = two_partitions(1024 * 1024);
        table.partitions[0].name = "x".repeat(37);
        assert!(matches!(
            table.entry_array(),
            Err(TableError::NameTooLong(37))
        ));
    }

    #[test]
    fn the_entry_array_is_the_size_the_header_declares_it_to_be() {
        // The header states both numbers, and a reader multiplies them to find the
        // end of the array. An array shorter than they say is read past the end of.
        let table = two_partitions(1024 * 1024);
        let array = table.entry_array().unwrap();
        assert_eq!(array.len(), ENTRY_COUNT as usize * ENTRY_LEN);
        assert_eq!(array.len() as u64, ENTRY_ARRAY_SECTORS * SECTOR);
    }

    #[test]
    fn unused_entries_are_all_zero_so_nothing_reads_them_as_partitions() {
        // An entry is unused when its type GUID is zero, and nothing else marks it.
        // A leftover byte anywhere in an entry with a nonzero type is a partition
        // somebody's tool will offer to mount.
        let table = two_partitions(1024 * 1024);
        let array = table.entry_array().unwrap();
        for index in table.partitions.len()..ENTRY_COUNT as usize {
            let entry = &array[index * ENTRY_LEN..(index + 1) * ENTRY_LEN];
            assert!(
                entry.iter().all(|byte| *byte == 0),
                "entry {index} is not empty"
            );
        }
    }

    #[test]
    fn the_protective_mbr_claims_everything_except_its_own_sector() {
        // The whole job of this sector: a tool that reads MBRs must see no free
        // space. One that saw some would offer it, and the offer would be a write
        // into the middle of the store.
        let table = two_partitions(1024 * 1024);
        let mbr = table.protective_mbr();
        assert_eq!(mbr[446 + 4], PROTECTIVE_TYPE);
        assert_eq!(u32::from_le_bytes(mbr[454..458].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(mbr[458..462].try_into().unwrap()) as u64,
            table.sectors - 1
        );
        assert_eq!(
            u16::from_le_bytes(mbr[510..512].try_into().unwrap()),
            MBR_SIGNATURE
        );

        // And the other three records stay empty.
        assert!(mbr[462..510].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn a_disk_too_large_to_describe_in_the_mbr_saturates_instead_of_wrapping() {
        // Above 2 TiB the size field cannot hold the answer. Wrapping would describe
        // a small disk to any tool that only reads MBRs, and the offer of free space
        // it made would land in the middle of the store.
        let mut table = two_partitions(1024 * 1024);
        table.sectors = 0x1_0000_0000 + 4096;
        let mbr = table.protective_mbr();
        assert_eq!(
            u32::from_le_bytes(mbr[458..462].try_into().unwrap()),
            u32::MAX
        );
    }
}
