//! A FAT32 filesystem holding one file, written without `mkfs.vfat`.
//!
//! ## Why FAT at all, in a project that chose Btrfs
//!
//! Because the firmware chose it. The UEFI specification requires firmware to
//! understand FAT and nothing else, so the partition it looks in for
//! `\EFI\BOOT\BOOTX64.EFI` has to be FAT — Btrfs there would produce a disk that is
//! correct in every way and does not boot. This is the one filesystem in Thalyx that
//! exists to satisfy something outside it.
//!
//! ## Why Thalyx writes it
//!
//! Same reason as everything else: `vault/01-Filosofia/Filosofia-Fundacional.md`
//! puts the Linux kernel and one program in the image, so `mkfs.vfat` cannot be
//! there. It is the fifth time this project has met a missing binary — `bpftool`,
//! `cpio`, `btrfs`, `partprobe` — and the fifth time the answer is to do the work
//! rather than to carry the tool.
//!
//! ## What this is not
//!
//! It is not a FAT implementation. It writes one volume with one file in it, at one
//! fixed path, and it can neither read a volume nor add a second file to one. There
//! are no long filenames because there is nothing here that needs one: `EFI`, `BOOT`
//! and `BOOTX64.EFI` are all valid 8.3 names in upper case, and a name that is not
//! is [`FatError::NotAShortName`] rather than a directory entry nobody planned.
//!
//! ## How this is known to be right
//!
//! `tests/uapi_msdos_fs.h` is `include/uapi/linux/msdos_fs.h`, captured verbatim,
//! and `tests/layout.rs` checks every offset written here against it. That
//! establishes the bytes are where Linux's own reader looks — it does not establish
//! that a volume comes out. Two things do, and neither is in this crate:
//! `fsck.vfat` walking it and Linux mounting it, both in stage 20 of
//! `dev/verify.sh`. And beyond both of those is the only claim that finally matters,
//! which is a firmware finding the file: `make -C image run-installed`.

use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

/// Bytes per logical sector. The same 512 the partition table assumes.
pub const SECTOR: u64 = 512;

/// Sectors per cluster: 4 KiB clusters.
///
/// Not tuned. It is the size at which a 512 MiB volume has comfortably more than
/// the 65525 clusters that make a volume FAT32 rather than FAT16, and a FAT small
/// enough to build in memory. A cluster size that put the count under that boundary
/// would produce a volume whose boot sector says FAT32 and whose cluster count says
/// FAT16 — which readers resolve differently, and firmware most of all.
pub const SECTORS_PER_CLUSTER: u64 = 8;

/// Sectors before the first FAT. 32 is what every FAT32 volume uses; the FSInfo
/// sector and the backup boot sector live inside it.
pub const RESERVED_SECTORS: u64 = 32;

/// Two, so a bad sector in one does not take the volume.
pub const FATS: u64 = 2;

/// Where the backup boot sector goes, by convention and by every reader's default.
const BACKUP_BOOT_SECTOR: u64 = 6;

/// The lowest cluster count that makes a volume FAT32.
///
/// `MAX_FAT16` in the captured header, and the boundary is exclusive on that side:
/// a volume with this many clusters or fewer is FAT16 no matter what its boot
/// sector claims, because the type is defined by the count and not by the text.
const MIN_FAT32_CLUSTERS: u64 = 65525;

/// `MAX_FAT32` from the captured header.
const MAX_FAT32_CLUSTERS: u64 = 0x0FFF_FFF6;

/// The end-of-chain mark, `EOF_FAT32`.
const END_OF_CHAIN: u32 = 0x0FFF_FFFF;

/// The cluster the root directory starts at. Two is the first cluster there is.
const ROOT_CLUSTER: u32 = 2;

/// What the volume calls itself, in the boot sector and in the root directory.
///
/// Eleven bytes, space-padded, upper case — the field has no other form. `blkid`
/// reports it as `LABEL`, which is how a human with the disk in another machine
/// finds out what they are holding.
pub const LABEL: &str = "THALYX";

/// The path the firmware looks for with nothing configured.
///
/// Not a name anybody picked: `\EFI\BOOT\BOOTX64.EFI` is the removable-media
/// fallback in the UEFI specification, which is what a PC with no operating system
/// is looking for. `image/Makefile` builds the same path for the ISO.
pub const BOOT_PATH: [&str; 3] = ["EFI", "BOOT", "BOOTX64.EFI"];

#[derive(Debug, thiserror::Error)]
pub enum FatError {
    #[error(
        "a {sectors}-sector volume with {SECTORS_PER_CLUSTER}-sector clusters has \
         {clusters} clusters, and FAT32 begins above {MIN_FAT32_CLUSTERS}. A volume \
         this small has to be FAT16, which this does not write."
    )]
    TooSmall { sectors: u64, clusters: u64 },

    #[error(
        "a {sectors}-sector volume has {clusters} clusters and FAT32 addresses \
         {MAX_FAT32_CLUSTERS}"
    )]
    TooLarge { sectors: u64, clusters: u64 },

    #[error(
        "`{0}` is not an 8.3 name in upper case, and this writes no long filenames. \
         What goes on an EFI system partition does not need one."
    )]
    NotAShortName(String),

    #[error(
        "{path} is {size} bytes and the {free} free clusters of {cluster_size} bytes \
         hold {capacity}"
    )]
    DoesNotFit {
        path: std::path::PathBuf,
        size: u64,
        free: u64,
        cluster_size: u64,
        capacity: u64,
    },

    #[error("{what} {path}: {source}")]
    Io {
        what: &'static str,
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// What the volume came out as.
#[derive(Debug)]
pub struct Written {
    pub sectors: u64,
    pub clusters: u64,
    pub free_clusters: u64,
    pub fat_sectors: u64,
    pub volume_id: u32,
    /// The size of the file that went in, which is what the boot medium carries.
    pub kernel_bytes: u64,
}

/// The arithmetic that decides where everything lands.
///
/// Separate from the writing so it can be checked without a device: every one of
/// these numbers appears in the boot sector, and a reader that disagrees with any
/// of them reads the wrong sector for everything after it.
#[derive(Debug, Clone, Copy)]
pub struct Geometry {
    pub sectors: u64,
    pub fat_sectors: u64,
    pub clusters: u64,
}

impl Geometry {
    /// Work out the FAT size for a volume of `sectors` sectors.
    ///
    /// The FAT has to hold four bytes for every cluster, and how many clusters there
    /// are depends on how much room the FAT leaves — so the two define each other.
    /// Solved by iterating rather than by the closed form in the FAT specification,
    /// which over-estimates: an over-estimate is harmless and an under-estimate is a
    /// FAT that stops short of the clusters it is supposed to describe, which reads
    /// as a volume whose last files are corrupt.
    pub fn of(sectors: u64) -> Result<Self, FatError> {
        let mut fat_sectors = 1u64;
        // Bounded, because a loop that decides where a filesystem goes should not be
        // able to run forever on a size nobody anticipated. It converges in two or
        // three rounds; anything past that is a bug and comes out as `TooLarge`.
        for _ in 0..64 {
            let overhead = RESERVED_SECTORS + FATS * fat_sectors;
            let Some(data) = sectors.checked_sub(overhead) else {
                return Err(FatError::TooSmall {
                    sectors,
                    clusters: 0,
                });
            };
            let clusters = data / SECTORS_PER_CLUSTER;
            let needed = ((clusters + 2) * 4).div_ceil(SECTOR);
            if needed <= fat_sectors {
                if clusters < MIN_FAT32_CLUSTERS {
                    return Err(FatError::TooSmall { sectors, clusters });
                }
                if clusters > MAX_FAT32_CLUSTERS {
                    return Err(FatError::TooLarge { sectors, clusters });
                }
                return Ok(Self {
                    sectors,
                    fat_sectors,
                    clusters,
                });
            }
            fat_sectors = needed;
        }
        Err(FatError::TooLarge {
            sectors,
            clusters: sectors / SECTORS_PER_CLUSTER,
        })
    }

    /// The first sector of the data area — where cluster 2 begins.
    pub fn data_start(&self) -> u64 {
        RESERVED_SECTORS + FATS * self.fat_sectors
    }

    /// The byte offset of a cluster.
    pub fn cluster_at(&self, cluster: u32) -> u64 {
        (self.data_start() + (u64::from(cluster) - 2) * SECTORS_PER_CLUSTER) * SECTOR
    }

    pub fn cluster_bytes(&self) -> u64 {
        SECTORS_PER_CLUSTER * SECTOR
    }
}

/// An 8.3 name as the eleven bytes a directory entry holds it in.
///
/// Refused rather than mangled. A name this cannot represent needs a long-filename
/// entry, and generating one silently would put a file on the volume under a name
/// the caller did not ask for — which for `BOOTX64.EFI` means a firmware that finds
/// nothing.
fn short_name(name: &str) -> Result<[u8; 11], FatError> {
    let bad = || FatError::NotAShortName(name.to_string());
    let (base, extension) = match name.split_once('.') {
        Some((base, extension)) => (base, extension),
        None => (name, ""),
    };
    if base.is_empty() || base.len() > 8 || extension.len() > 3 {
        return Err(bad());
    }
    let allowed =
        |c: char| c.is_ascii_uppercase() || c.is_ascii_digit() || "$%'-_@~`!(){}^#&".contains(c);
    if !base.chars().all(allowed) || !extension.chars().all(allowed) {
        return Err(bad());
    }
    let mut field = [b' '; 11];
    field[..base.len()].copy_from_slice(base.as_bytes());
    field[8..8 + extension.len()].copy_from_slice(extension.as_bytes());
    Ok(field)
}

/// A FAT date and time pair, from seconds since the epoch.
///
/// FAT counts years from 1980 and cannot hold anything earlier, so anything before
/// that is clamped to the first instant it can represent. Clamped and not refused:
/// a machine whose clock has not been set yet is a completely ordinary thing for an
/// installer to meet, and refusing to install over it would be absurd.
fn timestamp(seconds: u64) -> (u16, u16) {
    let epoch = time::OffsetDateTime::from_unix_timestamp(seconds.min(i64::MAX as u64) as i64)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let year = epoch.year();
    if year < 1980 {
        return ((1 << 5) | 1, 0); // 1980-01-01, 00:00
    }
    let date =
        (((year - 1980) as u16) << 9) | ((epoch.month() as u16) << 5) | u16::from(epoch.day());
    let time = (u16::from(epoch.hour()) << 11)
        | (u16::from(epoch.minute()) << 5)
        | (u16::from(epoch.second()) / 2);
    (date, time)
}

/// Attribute bits, from the captured header.
mod attr {
    pub const VOLUME: u8 = 8;
    pub const DIRECTORY: u8 = 16;
    pub const ARCHIVE: u8 = 32;
}

/// One 32-byte directory entry.
fn entry(name: [u8; 11], attributes: u8, cluster: u32, size: u32, when: (u16, u16)) -> [u8; 32] {
    let (date, time) = when;
    let mut bytes = [0u8; 32];
    bytes[0..11].copy_from_slice(&name);
    bytes[11] = attributes;
    // 12 is the case flag and 13 the creation centiseconds; both stay zero, which
    // means "the name is exactly as written" and "no finer than two seconds".
    bytes[14..16].copy_from_slice(&time.to_le_bytes()); // creation
    bytes[16..18].copy_from_slice(&date.to_le_bytes());
    bytes[18..20].copy_from_slice(&date.to_le_bytes()); // last access
    bytes[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
    bytes[22..24].copy_from_slice(&time.to_le_bytes()); // modification
    bytes[24..26].copy_from_slice(&date.to_le_bytes());
    bytes[26..28].copy_from_slice(&((cluster & 0xFFFF) as u16).to_le_bytes());
    bytes[28..32].copy_from_slice(&size.to_le_bytes());
    bytes
}

/// The boot sector, which is the whole description of the volume.
fn boot_sector(geometry: &Geometry, volume_id: u32) -> Vec<u8> {
    let mut sector = vec![0u8; SECTOR as usize];
    // A jump nothing executes, because a UEFI firmware does not run boot code off
    // an ESP — it reads the filesystem. It is here because readers use the first
    // byte to decide whether this is a boot sector at all: Linux's own `fat_fill_super`
    // refuses a volume whose media byte and jump look wrong.
    sector[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
    // The FAT specification recommends this exact string for maximum compatibility,
    // and some firmware does check it. It is not a claim about who wrote the volume.
    sector[3..11].copy_from_slice(b"MSWIN4.1");
    sector[11..13].copy_from_slice(&(SECTOR as u16).to_le_bytes());
    sector[13] = SECTORS_PER_CLUSTER as u8;
    sector[14..16].copy_from_slice(&(RESERVED_SECTORS as u16).to_le_bytes());
    sector[16] = FATS as u8;
    // Root entries and the 16-bit sector count are zero on FAT32, and that is how a
    // reader tells the two apart before it has counted anything.
    sector[21] = 0xF8; // fixed disk
    sector[24..26].copy_from_slice(&32u16.to_le_bytes()); // sectors per track
    sector[26..28].copy_from_slice(&8u16.to_le_bytes()); // heads
    // Hidden sectors stays zero. It is meant to hold the partition's offset, and it
    // is read by nothing that matters here — while a wrong value is read by DOS-era
    // tools as the volume starting somewhere it does not.
    sector[32..36].copy_from_slice(&(geometry.sectors as u32).to_le_bytes());
    sector[36..40].copy_from_slice(&(geometry.fat_sectors as u32).to_le_bytes());
    // 40 ext_flags: zero means both FATs are live and mirrored.
    // 42 version: zero.
    sector[44..48].copy_from_slice(&ROOT_CLUSTER.to_le_bytes());
    sector[48..50].copy_from_slice(&1u16.to_le_bytes()); // FSInfo
    sector[50..52].copy_from_slice(&(BACKUP_BOOT_SECTOR as u16).to_le_bytes());
    sector[64] = 0x80; // drive number, meaningless here and conventional
    sector[66] = 0x29; // "the three fields below are present"
    sector[67..71].copy_from_slice(&volume_id.to_le_bytes());
    let mut label = [b' '; 11];
    label[..LABEL.len()].copy_from_slice(LABEL.as_bytes());
    sector[71..82].copy_from_slice(&label);
    sector[82..90].copy_from_slice(b"FAT32   ");
    sector[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());
    sector
}

/// The FSInfo sector: a hint about free space, and two signatures around it.
///
/// The counts are hints and a reader is allowed to distrust them. They are written
/// correctly anyway, because `fsck.vfat` reports a wrong one as an error and an
/// installer whose output does not survive a check has no way to prove it worked.
fn fsinfo(free_clusters: u64, next_free: u32) -> Vec<u8> {
    let mut sector = vec![0u8; SECTOR as usize];
    sector[0..4].copy_from_slice(&0x4161_5252u32.to_le_bytes());
    sector[484..488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
    sector[488..492].copy_from_slice(&(free_clusters as u32).to_le_bytes());
    sector[492..496].copy_from_slice(&next_free.to_le_bytes());
    // The trailing signature. Not in Linux's struct, which covers it with a
    // reserved field, and checked by fsck.vfat.
    sector[508..512].copy_from_slice(&0xAA55_0000u32.to_le_bytes());
    sector
}

/// Write a FAT32 volume onto `device`, holding `kernel` at [`BOOT_PATH`].
///
/// `device` is the partition, opened for writing and positioned wherever — every
/// write here seeks first. `sectors` is how many sectors the partition has, which
/// is the caller's knowledge and not something this can ask for: the file handed in
/// may be a partition device, whose length the kernel reports, or a plain file
/// under test.
pub fn write(
    path: &Path,
    device: &mut std::fs::File,
    sectors: u64,
    kernel: &Path,
    seconds: u64,
) -> Result<Written, FatError> {
    let geometry = Geometry::of(sectors)?;
    let when = timestamp(seconds);
    let io = |what: &'static str, path: &Path| {
        let path = path.to_path_buf();
        move |source| FatError::Io { what, path, source }
    };

    let kernel_bytes = std::fs::metadata(kernel)
        .map_err(io("looking at", kernel))?
        .len();

    // Three clusters of directories — root, EFI, BOOT — then the file.
    let (root, efi, boot) = (2u32, 3u32, 4u32);
    let first_file_cluster = 5u32;
    let cluster_bytes = geometry.cluster_bytes();
    let file_clusters = kernel_bytes.div_ceil(cluster_bytes);
    let usable = geometry.clusters - 3;
    if file_clusters > usable {
        return Err(FatError::DoesNotFit {
            path: kernel.to_path_buf(),
            size: kernel_bytes,
            free: usable,
            cluster_size: cluster_bytes,
            capacity: usable * cluster_bytes,
        });
    }

    // ── the file allocation table
    //
    // Built whole in memory and written twice. A 512 MiB volume's FAT is half a
    // megabyte, which is a reasonable thing to hold; a volume large enough for that
    // to stop being true is refused by `Geometry::of` long before here.
    let mut fat = vec![0u8; (geometry.fat_sectors * SECTOR) as usize];
    let mut put = |cluster: u32, value: u32| {
        let at = cluster as usize * 4;
        fat[at..at + 4].copy_from_slice(&value.to_le_bytes());
    };
    // Entry 0 carries the media byte in its low eight bits, entry 1 is the "clean
    // shutdown" pair. Both are read by fsck.vfat and both being wrong is how a
    // freshly written volume gets reported as dirty.
    put(0, 0x0FFF_FF00 | 0xF8);
    put(1, END_OF_CHAIN);
    for cluster in [root, efi, boot] {
        put(cluster, END_OF_CHAIN);
    }
    for index in 0..file_clusters {
        let cluster = first_file_cluster + index as u32;
        let value = if index + 1 == file_clusters {
            END_OF_CHAIN
        } else {
            cluster + 1
        };
        put(cluster, value);
    }

    let free_clusters = geometry.clusters - 3 - file_clusters;

    // ── the reserved region
    let mut reserved = vec![0u8; (RESERVED_SECTORS * SECTOR) as usize];
    let sector_at = |bytes: &mut Vec<u8>, sector: u64, content: &[u8]| {
        let at = (sector * SECTOR) as usize;
        bytes[at..at + content.len()].copy_from_slice(content);
    };
    let boot_bytes = boot_sector(&geometry, volume_id(seconds));
    let info_bytes = fsinfo(free_clusters, first_file_cluster + file_clusters as u32 - 1);
    sector_at(&mut reserved, 0, &boot_bytes);
    sector_at(&mut reserved, 1, &info_bytes);
    sector_at(&mut reserved, BACKUP_BOOT_SECTOR, &boot_bytes);
    sector_at(&mut reserved, BACKUP_BOOT_SECTOR + 1, &info_bytes);

    device
        .seek(SeekFrom::Start(0))
        .map_err(io("seeking in", path))?;
    device
        .write_all(&reserved)
        .map_err(io("writing to", path))?;

    for copy in 0..FATS {
        device
            .seek(SeekFrom::Start(
                (RESERVED_SECTORS + copy * geometry.fat_sectors) * SECTOR,
            ))
            .map_err(io("seeking in", path))?;
        device.write_all(&fat).map_err(io("writing to", path))?;
    }

    // ── the three directories
    //
    // Each written as a whole cluster of zeroes with its entries at the front. The
    // zeroes matter: a directory ends at the first entry whose name starts with a
    // zero byte, so leftover bytes from whatever was on the disk would be read as
    // more files.
    let mut label_field = [b' '; 11];
    label_field[..LABEL.len()].copy_from_slice(LABEL.as_bytes());

    let dot = |cluster: u32| entry(*b".          ", attr::DIRECTORY, cluster, 0, when);
    // The parent of the root is written as cluster zero, not as 2. It is what the
    // specification says and what readers check; a `..` pointing at the real root
    // cluster is one of the things fsck.vfat calls out by name.
    let dotdot = |parent: u32| {
        let parent = if parent == ROOT_CLUSTER { 0 } else { parent };
        entry(*b"..         ", attr::DIRECTORY, parent, 0, when)
    };

    let directories: Vec<(u32, Vec<[u8; 32]>)> = vec![
        (
            root,
            vec![
                entry(label_field, attr::VOLUME, 0, 0, when),
                entry(short_name(BOOT_PATH[0])?, attr::DIRECTORY, efi, 0, when),
            ],
        ),
        (
            efi,
            vec![
                dot(efi),
                dotdot(root),
                entry(short_name(BOOT_PATH[1])?, attr::DIRECTORY, boot, 0, when),
            ],
        ),
        (
            boot,
            vec![
                dot(boot),
                dotdot(efi),
                entry(
                    short_name(BOOT_PATH[2])?,
                    attr::ARCHIVE,
                    first_file_cluster,
                    u32::try_from(kernel_bytes).unwrap_or(u32::MAX),
                    when,
                ),
            ],
        ),
    ];

    for (cluster, entries) in &directories {
        let mut content = vec![0u8; cluster_bytes as usize];
        for (index, entry) in entries.iter().enumerate() {
            content[index * 32..(index + 1) * 32].copy_from_slice(entry);
        }
        device
            .seek(SeekFrom::Start(geometry.cluster_at(*cluster)))
            .map_err(io("seeking in", path))?;
        device.write_all(&content).map_err(io("writing to", path))?;
    }

    // ── the file
    //
    // Copied a cluster at a time rather than read whole: a kernel is tens of
    // megabytes today and an installer should not need to hold the thing it is
    // installing.
    let mut source = std::fs::File::open(kernel).map_err(io("opening", kernel))?;
    device
        .seek(SeekFrom::Start(geometry.cluster_at(first_file_cluster)))
        .map_err(io("seeking in", path))?;
    let mut buffer = vec![0u8; cluster_bytes as usize];
    let mut copied = 0u64;
    while copied < kernel_bytes {
        let want = std::cmp::min(cluster_bytes, kernel_bytes - copied) as usize;
        // The tail of the last cluster is zeroed rather than left as it was found.
        // Nothing reads past the recorded size, and a boot medium whose spare bytes
        // hold whatever the previous owner of this disk wrote is not a thing to hand
        // somebody.
        buffer[want..].fill(0);
        std::io::Read::read_exact(&mut source, &mut buffer[..want])
            .map_err(io("reading", kernel))?;
        device.write_all(&buffer).map_err(io("writing to", path))?;
        copied += want as u64;
    }

    device.sync_all().map_err(io("flushing", path))?;

    Ok(Written {
        sectors: geometry.sectors,
        clusters: geometry.clusters,
        free_clusters,
        fat_sectors: geometry.fat_sectors,
        volume_id: volume_id(seconds),
        kernel_bytes,
    })
}

/// The volume's serial number, which is not a checksum and does not have to be
/// unique — it is what DOS used to notice a floppy had been swapped.
fn volume_id(seconds: u64) -> u32 {
    (seconds as u32) ^ 0x5448_414C
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The size this crate gives an EFI system partition, in sectors.
    const ESP: u64 = 512 * 1024 * 1024 / SECTOR;

    #[test]
    fn the_geometry_leaves_a_fat_that_describes_every_cluster_it_has() {
        // The property the iteration exists for, and the one an under-estimate
        // breaks. A FAT one sector short is not a smaller FAT: it is a volume whose
        // last clusters have no entries, so a file that reaches them is read back
        // from wherever the FAT happens to run into next.
        for sectors in [ESP, ESP + 1, ESP * 2, ESP - 1, 700_000, 1_000_003] {
            let geometry = Geometry::of(sectors).unwrap();
            let described = geometry.fat_sectors * SECTOR / 4;
            assert!(
                described >= geometry.clusters + 2,
                "{sectors} sectors: the FAT describes {described} entries and the \
                 volume has {} clusters plus the two reserved",
                geometry.clusters
            );
            // And the data area really is inside the volume.
            let last = geometry.data_start() + geometry.clusters * SECTORS_PER_CLUSTER;
            assert!(
                last <= sectors,
                "{sectors} sectors: the data area ends at {last}"
            );
        }
    }

    #[test]
    fn a_volume_too_small_to_be_fat32_is_refused_rather_than_written_as_one() {
        // The boundary that matters and the one a reader resolves differently from
        // the boot sector: below 65525 clusters the volume *is* FAT16, whatever it
        // says about itself, so writing FAT32 structures onto it produces something
        // firmware reads as a different filesystem.
        let error = Geometry::of(64 * 1024 * 1024 / SECTOR).unwrap_err();
        assert!(matches!(error, FatError::TooSmall { .. }), "{error:?}");

        // And the one this crate actually uses is comfortably on the other side.
        let geometry = Geometry::of(ESP).unwrap();
        assert!(
            geometry.clusters > MIN_FAT32_CLUSTERS,
            "the ESP has {} clusters, which is FAT16",
            geometry.clusters
        );
    }

    #[test]
    fn the_three_names_on_the_boot_path_are_all_representable() {
        // Which is why there are no long filename entries here at all. If a decree
        // ever renames one of these to something 8.3 cannot hold, this fails rather
        // than a volume coming out with a name nobody asked for.
        for name in BOOT_PATH {
            short_name(name).unwrap_or_else(|_| panic!("`{name}` is not an 8.3 name"));
        }
        assert_eq!(short_name("BOOTX64.EFI").unwrap(), *b"BOOTX64 EFI");
        assert_eq!(short_name("EFI").unwrap(), *b"EFI        ");
    }

    #[test]
    fn a_name_needing_a_long_entry_is_refused_and_not_mangled() {
        for name in ["bootx64.efi", "TOOLONGNAME.EFI", "A.LONGEXT", ""] {
            assert!(
                matches!(short_name(name), Err(FatError::NotAShortName(_))),
                "`{name}` was accepted"
            );
        }
    }

    #[test]
    fn a_timestamp_before_fat_could_count_is_clamped_and_not_wrapped() {
        // A machine with no clock reports 1970, which is ten years before FAT's
        // epoch. Subtracting would wrap the year field and produce a date in 2107 —
        // valid, plausible, and enough to make `fsck.vfat` and every file manager
        // disagree about what happened when.
        let (date, _) = timestamp(0);
        assert_eq!(date >> 9, 0, "the year is not 1980");
        assert_eq!((date >> 5) & 0xF, 1, "the month is not January");
        assert_eq!(date & 0x1F, 1, "the day is not the first");
    }

    #[test]
    fn a_known_time_lands_on_the_date_it_is() {
        // 2026-08-07T12:34:56Z. A fixture with an answer worked out elsewhere, so
        // the encoding is checked against something other than itself.
        let (date, time) = timestamp(1_786_106_096);
        assert_eq!(date >> 9, 2026 - 1980);
        assert_eq!((date >> 5) & 0xF, 8);
        assert_eq!(date & 0x1F, 7);
        assert_eq!(time >> 11, 12);
        assert_eq!((time >> 5) & 0x3F, 34);
        assert_eq!((time & 0x1F) * 2, 56);
    }

    #[test]
    fn the_boot_sector_says_fat32_in_every_way_a_reader_decides_it() {
        // Three independent things a reader looks at, and they have to agree. A
        // volume with a 16-bit sector count set is read as FAT16 no matter what the
        // string at offset 82 says, because the string is documentation and the
        // fields are the format.
        let geometry = Geometry::of(ESP).unwrap();
        let sector = boot_sector(&geometry, 0);
        assert_eq!(u16::from_le_bytes(sector[17..19].try_into().unwrap()), 0);
        assert_eq!(u16::from_le_bytes(sector[19..21].try_into().unwrap()), 0);
        assert_eq!(u16::from_le_bytes(sector[22..24].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_le_bytes(sector[32..36].try_into().unwrap()) as u64,
            geometry.sectors
        );
        assert_eq!(
            u32::from_le_bytes(sector[36..40].try_into().unwrap()) as u64,
            geometry.fat_sectors
        );
        assert_eq!(
            u32::from_le_bytes(sector[44..48].try_into().unwrap()),
            ROOT_CLUSTER
        );
        assert_eq!(&sector[82..90], b"FAT32   ");
        assert_eq!(
            u16::from_le_bytes(sector[510..512].try_into().unwrap()),
            0xAA55
        );
    }

    /// Write a volume into a sparse file and hand back the bytes of it that matter.
    fn volume(kernel_bytes: usize) -> (tempfile::TempDir, std::path::PathBuf, Written) {
        let dir = tempfile::tempdir().unwrap();
        let kernel = dir.path().join("bzImage");
        std::fs::write(&kernel, vec![0x5Au8; kernel_bytes]).unwrap();

        let image = dir.path().join("esp.img");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .truncate(true)
            .open(&image)
            .unwrap();
        file.set_len(ESP * SECTOR).unwrap();
        let written = write(&image, &mut file, ESP, &kernel, 1_786_106_096).unwrap();
        (dir, image, written)
    }

    #[test]
    fn the_file_is_where_the_directory_chain_says_it_is() {
        // The whole volume, walked the way a reader walks it: root cluster from the
        // boot sector, `EFI` from the root, `BOOT` from that, and the file from
        // that. Reading it back with the numbers this module produced would prove
        // nothing, so every step reads a field out of the bytes on disk.
        let (_dir, image, written) = volume(3 * 4096 + 17);
        let bytes = std::fs::read(&image).unwrap();
        let geometry = Geometry::of(ESP).unwrap();

        let root = u32::from_le_bytes(bytes[44..48].try_into().unwrap());
        let find = |cluster: u32, name: &[u8; 11]| -> Option<(u32, u32)> {
            let at = geometry.cluster_at(cluster) as usize;
            let content = &bytes[at..at + geometry.cluster_bytes() as usize];
            for entry in content.chunks_exact(32) {
                if entry[0] == 0 {
                    return None;
                }
                if &entry[0..11] == name {
                    let high = u16::from_le_bytes(entry[20..22].try_into().unwrap());
                    let low = u16::from_le_bytes(entry[26..28].try_into().unwrap());
                    let size = u32::from_le_bytes(entry[28..32].try_into().unwrap());
                    return Some(((u32::from(high) << 16) | u32::from(low), size));
                }
            }
            None
        };

        let (efi, _) = find(root, b"EFI        ").expect("no EFI directory in the root");
        let (boot, _) = find(efi, b"BOOT       ").expect("no BOOT directory under EFI");
        let (file, size) = find(boot, b"BOOTX64 EFI").expect("no BOOTX64.EFI under BOOT");
        assert_eq!(u64::from(size), written.kernel_bytes);

        // And the contents, followed cluster by cluster through the FAT, which is
        // the part a wrong chain breaks silently: a file whose first cluster is
        // right reads back correctly for its first four kilobytes.
        // Anything from 0x0FFFFFF8 up is an end mark, not just the value written —
        // a reader that compared against one exact number would run off a volume
        // some other tool wrote.
        let mut cluster = file;
        let mut recovered = Vec::new();
        while cluster < 0x0FFF_FFF8 {
            let at = geometry.cluster_at(cluster) as usize;
            recovered.extend_from_slice(&bytes[at..at + geometry.cluster_bytes() as usize]);
            let fat_at = (RESERVED_SECTORS * SECTOR) as usize + cluster as usize * 4;
            cluster =
                u32::from_le_bytes(bytes[fat_at..fat_at + 4].try_into().unwrap()) & 0x0FFF_FFFF;
        }
        recovered.truncate(size as usize);
        assert_eq!(recovered, vec![0x5Au8; size as usize]);
    }

    #[test]
    fn the_two_copies_of_the_fat_are_the_same_bytes() {
        // Not tidiness: readers are entitled to use either, and fsck.vfat compares
        // them. A writer that filled the first and left the second as it found it
        // produces a volume that is fine until something reads the mirror.
        let (_dir, image, _) = volume(4096);
        let bytes = std::fs::read(&image).unwrap();
        let geometry = Geometry::of(ESP).unwrap();
        let one = (RESERVED_SECTORS * SECTOR) as usize;
        let two = ((RESERVED_SECTORS + geometry.fat_sectors) * SECTOR) as usize;
        let len = (geometry.fat_sectors * SECTOR) as usize;
        assert_eq!(bytes[one..one + len], bytes[two..two + len]);
    }

    #[test]
    fn the_backup_boot_sector_is_the_boot_sector() {
        // Its only job is to be identical, and it is the copy a reader falls back to
        // when sector 0 is unreadable. A backup that was written before a field was
        // filled in is worse than none: it repairs the volume into a different one.
        let (_dir, image, _) = volume(4096);
        let bytes = std::fs::read(&image).unwrap();
        let backup = (BACKUP_BOOT_SECTOR * SECTOR) as usize;
        assert_eq!(bytes[0..512], bytes[backup..backup + 512]);
    }

    #[test]
    fn the_free_count_matches_what_the_fat_actually_leaves() {
        // fsck.vfat reports a wrong FSInfo as an error, and an installer whose
        // output does not pass a check has no way to prove it worked.
        let (_dir, image, written) = volume(64 * 1024);
        let bytes = std::fs::read(&image).unwrap();
        let geometry = Geometry::of(ESP).unwrap();
        let fat_at = (RESERVED_SECTORS * SECTOR) as usize;
        let used = (2..geometry.clusters + 2)
            .filter(|cluster| {
                let at = fat_at + *cluster as usize * 4;
                u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) != 0
            })
            .count() as u64;
        assert_eq!(written.free_clusters, geometry.clusters - used);

        let recorded =
            u32::from_le_bytes(bytes[(SECTOR as usize) + 488..][..4].try_into().unwrap());
        assert_eq!(u64::from(recorded), written.free_clusters);
    }

    #[test]
    fn a_kernel_larger_than_the_partition_is_refused_before_anything_is_written() {
        // The failure that will actually happen one day: the kernel grows and the
        // ESP does not. What must not happen is a volume that is written up to the
        // point the room runs out — the boot medium would then be a filesystem whose
        // directory names a file that stops in the middle, and firmware would load
        // it and jump into whatever came after.
        let dir = tempfile::tempdir().unwrap();
        let kernel = dir.path().join("bzImage");
        // Sparse: the size is what matters and none of it is read, because the
        // refusal happens before the copy.
        std::fs::File::create(&kernel)
            .unwrap()
            .set_len(600 * 1024 * 1024)
            .unwrap();

        let image = dir.path().join("esp.img");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .truncate(true)
            .open(&image)
            .unwrap();
        file.set_len(ESP * SECTOR).unwrap();

        let error = write(&image, &mut file, ESP, &kernel, 0).unwrap_err();
        assert!(matches!(error, FatError::DoesNotFit { .. }), "{error:?}");

        // And nothing was written: the boot sector is still the zeroes the file was
        // created with, so a half-made volume cannot be mistaken for a made one.
        let bytes = std::fs::read(&image).unwrap();
        assert!(
            bytes[..512].iter().all(|byte| *byte == 0),
            "a boot sector was written for a volume that could not be finished"
        );
    }
}
