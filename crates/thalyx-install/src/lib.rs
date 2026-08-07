//! Turning a disk with no operating system on it into a Thalyx machine.
//!
//! `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md` ends Phase 1 with a
//! medium that does that, and Cesar decided on 2026-08-06 that the ISO **installs
//! and is then removed** — the PC has Thalyx rather than borrowing it. That decision
//! is what makes this crate exist.
//!
//! ## The act that joins the two expensive pieces
//!
//! Both halves were already built and neither was reachable from the other:
//!
//! - `thalyx-btrfs` writes a store, and needs a block device to write it onto.
//! - `image/Makefile` builds a kernel that is a valid UEFI application, and needs
//!   an EFI system partition to be found in.
//!
//! What was missing is the thing that makes those two partitions out of one disk,
//! and it needed two more pieces of byte-writing that could not be borrowed:
//! [`gpt`] for the partition table and [`fat`] for the filesystem the firmware
//! insists on. `sgdisk` and `mkfs.vfat` are the tools a person would use, and
//! `vault/01-Filosofia/Filosofia-Fundacional.md` puts the Linux kernel and one
//! program in the image. Fourth and fifth time, same answer.
//!
//! ## What comes out
//!
//! ```text
//!   LBA 0          protective MBR
//!   LBA 1..34      the partition table, and its copy at the far end
//!   1 MiB          partition 1, 512 MiB, FAT32, holding \EFI\BOOT\BOOTX64.EFI
//!   513 MiB..      partition 2, the rest, Btrfs labelled `thalyx-store`,
//!                  with the three subvolumes `Journal-y-Snapshots.md` decrees
//! ```
//!
//! One file on the boot partition, and it is the kernel with Thalyx inside it.
//! `make -C image count` says the image holds one program; this is the same count
//! carried through to the installed disk.
//!
//! ## Who may call it
//!
//! Not PID 1, and the reason is the one `crates/thalyx-cli/src/store_disk.rs`
//! records: a machine that fabricates a store when it cannot find the old one boots
//! looking perfect on the day the disk was not attached. Installing is a human act
//! with a human confirmation in front of it, and this crate is reachable only from
//! `thalyx install`.
//!
//! ## What has been exercised, and what has not
//!
//! Everything below writes bytes into a file, so all of it runs anywhere. **None of
//! it establishes that a machine boots from what it wrote**, and the difference
//! matters more here than anywhere else in this repository, because a partition
//! table with a wrong checksum is not reported as broken — it is ignored, and the
//! disk comes back looking untouched.
//!
//! Three instruments, in increasing order of what they settle, and only the first
//! is a test in this crate:
//!
//! 1. `tests/layout.rs` — every offset against `block/partitions/efi.h` and
//!    `include/uapi/linux/msdos_fs.h`, captured verbatim.
//! 2. Stage 20 of `dev/verify.sh` — the **kernel** parses the table (`losetup -P`),
//!    mounts the FAT, reads the file back, and mounts the store's subvolumes.
//! 3. `make -C image run-installed` — a UEFI firmware, given only the installed
//!    disk, finds the kernel and starts it. That is the claim, and nothing short of
//!    it is the claim.

pub mod crc32;
pub mod fat;
pub mod gpt;
pub mod medium;
pub mod partitions;

use std::path::{Path, PathBuf};

/// One mebibyte, in sectors. Everything is aligned to it.
///
/// Not cosmetic: an SSD erases in blocks far larger than a sector, and a partition
/// starting mid-block makes every write to it a read-modify-write of two. One MiB is
/// what every partitioner has aligned to since 2009, for a reason that has only got
/// stronger.
pub const ALIGNMENT: u64 = 1024 * 1024 / gpt::SECTOR;

/// How big the EFI system partition is made.
///
/// 512 MiB for a file that is tens of megabytes, and the slack is the point. The
/// partition cannot be grown later without moving the store, and the one thing that
/// will certainly happen is that a kernel update has to be written *beside* the
/// running one before the old one is removed — a machine that overwrites its only
/// bootable file and loses power is a machine that does not come back.
pub const ESP_SECTORS: u64 = 512 * 1024 * 1024 / gpt::SECTOR;

/// The smallest disk this can install onto.
///
/// Composed rather than written down, so that a change to any part of the layout
/// moves it. A number typed here would go stale in the direction that matters: it
/// would keep saying yes to a disk that no longer fits.
pub const MINIMUM_DEVICE: u64 = (ALIGNMENT * gpt::SECTOR)
    + (ESP_SECTORS * gpt::SECTOR)
    + thalyx_btrfs::layout::MINIMUM_DEVICE
    + (ALIGNMENT * gpt::SECTOR);

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(
        "{path} is {size} bytes, and installing needs at least {MINIMUM_DEVICE}: \
         1 MiB before the first partition, {esp} for the boot partition, \
         {store} for the smallest store Thalyx will write, and a megabyte at the \
         end for the second copy of the partition table",
        esp = ESP_SECTORS * gpt::SECTOR,
        store = thalyx_btrfs::layout::MINIMUM_DEVICE
    )]
    TooSmall { path: PathBuf, size: u64 },

    #[error("{what} {path}: {source}")]
    Io {
        what: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Partitions(#[from] partitions::PartitionError),

    #[error("the partition table: {0}")]
    Table(#[from] gpt::TableError),

    #[error("the boot partition: {0}")]
    Boot(#[from] fat::FatError),

    #[error("the store: {0}")]
    Store(#[from] thalyx_btrfs::FormatError),

    #[error("the store's subvolumes: {0}")]
    Subvolumes(#[from] thalyx_btrfs::SubvolumeError),
}

/// Where the two partitions go on a disk of a given size.
///
/// Worked out separately from writing anything, so that `thalyx install` can print
/// it next to what is already on the disk and before the confirmation. A
/// confirmation that says what is being destroyed and not what replaces it is half
/// a question.
#[derive(Debug, Clone, Copy)]
pub struct Plan {
    pub sectors: u64,
    pub esp_first: u64,
    pub esp_last: u64,
    pub store_first: u64,
    pub store_last: u64,
}

impl Plan {
    pub fn of(path: &Path, sectors: u64) -> Result<Self, InstallError> {
        let too_small = || InstallError::TooSmall {
            path: path.to_path_buf(),
            size: sectors * gpt::SECTOR,
        };
        if sectors * gpt::SECTOR < MINIMUM_DEVICE {
            return Err(too_small());
        }

        let esp_first = ALIGNMENT;
        let esp_last = esp_first + ESP_SECTORS - 1;
        let store_first = esp_last + 1;

        // The store stops at the last aligned sector before the backup table.
        // Rounded down rather than up, because the sector after the last usable one
        // is the first sector of the copy of the partition table that has to survive
        // the primary being lost.
        let last_usable = sectors - gpt::ENTRY_ARRAY_SECTORS - 2;
        let store_last = ((last_usable + 1) / ALIGNMENT) * ALIGNMENT - 1;

        if store_last <= store_first
            || (store_last + 1 - store_first) * gpt::SECTOR < thalyx_btrfs::layout::MINIMUM_DEVICE
        {
            return Err(too_small());
        }

        Ok(Self {
            sectors,
            esp_first,
            esp_last,
            store_first,
            store_last,
        })
    }

    pub fn esp_sectors(&self) -> u64 {
        self.esp_last + 1 - self.esp_first
    }

    pub fn store_sectors(&self) -> u64 {
        self.store_last + 1 - self.store_first
    }

    /// The table this plan becomes.
    pub fn table(&self) -> gpt::Table {
        gpt::Table {
            disk_guid: gpt::Guid::random(),
            sectors: self.sectors,
            partitions: vec![
                gpt::Partition {
                    kind: gpt::ESP,
                    unique: gpt::Guid::random(),
                    first_lba: self.esp_first,
                    last_lba: self.esp_last,
                    // The name every partitioner gives it. A human looking at this
                    // disk from another machine should not have to learn a Thalyx
                    // word to recognise the one partition that is not ours.
                    name: "EFI System Partition".into(),
                },
                gpt::Partition {
                    kind: gpt::LINUX_FILESYSTEM,
                    unique: gpt::Guid::random(),
                    first_lba: self.store_first,
                    last_lba: self.store_last,
                    name: "Thalyx store".into(),
                },
            ],
        }
    }
}

/// What an install produced, for the caller to report.
pub struct Installed {
    pub device: PathBuf,
    pub plan: Plan,
    pub esp: PathBuf,
    pub store: PathBuf,
    pub boot: fat::Written,
    pub filesystem: thalyx_btrfs::Written,
    pub subvolumes: thalyx_btrfs::Outcome,
}

/// How big a block device is.
///
/// By seeking to the end, which is what the kernel answers for a block device and
/// what `thalyx-btrfs` already does. `metadata().len()` answers zero for one, and a
/// zero here would come out as "this disk is too small" — a message pointing at the
/// disk instead of at the question.
fn size_of(path: &Path, file: &mut std::fs::File) -> Result<u64, InstallError> {
    use std::io::Seek;
    file.seek(std::io::SeekFrom::End(0))
        .map_err(|source| InstallError::Io {
            what: "measuring",
            path: path.to_path_buf(),
            source,
        })
}

/// Partition `device`, write `kernel` onto the boot partition, and make the store.
///
/// Everything on the disk is gone. The caller is responsible for having asked; this
/// function asks nothing.
///
/// `workspace` is a directory the subvolume step may put mount points under, and
/// `seconds` is the time to stamp things with — passed in rather than read, so a
/// test can produce the same disk twice.
pub fn install(
    device: &Path,
    kernel: &Path,
    workspace: &Path,
    seconds: u64,
) -> Result<Installed, InstallError> {
    // Refused before a byte is written, and each for its own reason. A disk this
    // cannot address correctly is not a disk to find out about halfway through.
    //
    // The whole-disk check is first because it is the one whose failure is
    // unrecoverable. A partition table written at the start of a partition is
    // legal and invisible: no tool looks for one there, so nothing reports it as
    // broken, and the machine comes back looking as if the install simply did not
    // happen — with the filesystem that was there overwritten. `discos` offered
    // exactly that on 2026-08-07, listing 444 GiB of Cesar's Fedora as a disk.
    partitions::whole_disk(device)?;
    let sector_size = partitions::logical_sector_size(device)?;
    if sector_size != gpt::SECTOR {
        return Err(partitions::PartitionError::SectorSize {
            device: device.to_path_buf(),
            size: sector_size,
        }
        .into());
    }

    let io = |what: &'static str, path: &Path| {
        let path = path.to_path_buf();
        move |source| InstallError::Io { what, path, source }
    };

    let mut disk = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(device)
        .map_err(io("opening", device))?;
    let size = size_of(device, &mut disk)?;
    let plan = Plan::of(device, size / gpt::SECTOR)?;

    plan.table().write(device, &mut disk)?;
    // Closed before the kernel is asked to re-read: an open handle on the whole disk
    // does not stop `BLKRRPART`, and holding one while the partitions are opened
    // means two views of the same blocks in the page cache. The store's superblock
    // being written through one and read through the other is the kind of thing that
    // works on every machine until it does not.
    drop(disk);

    let made = partitions::appear(device, 2, 10)?;
    let (esp, store) = (made[0].clone(), made[1].clone());

    let mut boot_partition = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&esp)
        .map_err(io("opening", &esp))?;
    let esp_sectors = size_of(&esp, &mut boot_partition)? / gpt::SECTOR;
    // The kernel's number and not the plan's. They agree, and if they ever do not,
    // the one that decides where a reader looks is the kernel's — a filesystem
    // written for a partition larger than the one it is in has its last clusters
    // outside the device.
    let boot = fat::write(&esp, &mut boot_partition, esp_sectors, kernel, seconds)?;
    drop(boot_partition);

    let filesystem = thalyx_btrfs::write(
        &store,
        thalyx_btrfs::LABEL,
        &thalyx_btrfs::Uuids::random(),
        seconds,
    )?;
    let subvolumes = thalyx_btrfs::subvolume::create(&store, workspace, &thalyx_btrfs::DECREED)?;

    Ok(Installed {
        device: device.to_path_buf(),
        plan,
        esp,
        store,
        boot,
        filesystem,
        subvolumes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(bytes: u64) -> Result<Plan, InstallError> {
        Plan::of(Path::new("/dev/pretend"), bytes / gpt::SECTOR)
    }

    #[test]
    fn nothing_in_the_plan_touches_the_partition_table_at_either_end() {
        // The arithmetic mistake with the worst symptom. A store whose last sectors
        // sit under the backup GPT works perfectly until Btrfs allocates that far,
        // and then there is neither a filesystem nor a recovery copy of the table.
        for size in [
            MINIMUM_DEVICE,
            MINIMUM_DEVICE + 1,
            8 * 1024 * 1024 * 1024,
            931 * 1024 * 1024 * 1024 + 12345,
        ] {
            let plan = plan(size).unwrap_or_else(|error| panic!("{size} bytes: {error}"));
            assert!(plan.esp_first >= gpt::FIRST_USABLE_LBA);
            assert!(plan.store_last <= plan.sectors - gpt::ENTRY_ARRAY_SECTORS - 2);
            assert!(plan.esp_last < plan.store_first);
        }
    }

    #[test]
    fn both_partitions_start_on_a_mebibyte() {
        // An SSD erases in blocks much larger than a sector. A partition that starts
        // mid-block turns every write into a read-modify-write of two, forever, and
        // nothing ever reports it.
        let plan = plan(64 * 1024 * 1024 * 1024).unwrap();
        assert_eq!(plan.esp_first % ALIGNMENT, 0);
        assert_eq!(plan.store_first % ALIGNMENT, 0);
    }

    #[test]
    fn the_smallest_disk_that_is_accepted_leaves_a_store_thalyx_can_write() {
        // The two minimums have to agree, and they are held in two crates. A plan
        // that accepted a disk `thalyx-btrfs` then refused would leave a partitioned
        // disk with no store on it — halfway, which is the state this whole crate is
        // arranged to avoid being in.
        let plan = plan(MINIMUM_DEVICE).unwrap();
        assert!(
            plan.store_sectors() * gpt::SECTOR >= thalyx_btrfs::layout::MINIMUM_DEVICE,
            "the smallest accepted disk leaves {} bytes of store and the writer needs {}",
            plan.store_sectors() * gpt::SECTOR,
            thalyx_btrfs::layout::MINIMUM_DEVICE
        );
    }

    #[test]
    fn a_disk_one_byte_too_small_is_refused_and_says_what_the_parts_cost() {
        let error = plan(MINIMUM_DEVICE - gpt::SECTOR).unwrap_err();
        assert!(matches!(error, InstallError::TooSmall { .. }), "{error:?}");
        // The message has to break the number down. "At least 673 megabytes" tells a
        // person with a 512 MiB stick nothing about which part does not fit.
        let text = error.to_string();
        assert!(text.contains("boot partition"), "{text}");
        assert!(text.contains("store"), "{text}");
    }

    #[test]
    fn the_boot_partition_is_large_enough_to_be_fat32_at_all() {
        // fat.rs refuses a volume with too few clusters to be FAT32, and the ESP's
        // size is decided here. Two constants in two modules that have to agree, and
        // the failure if they stop is an installer that partitions a disk and then
        // cannot make a filesystem on the partition it just made.
        assert!(fat::Geometry::of(ESP_SECTORS).is_ok());
    }

    #[test]
    fn the_table_the_plan_becomes_is_one_the_table_writer_accepts() {
        // `Table::write` checks bounds and overlap and refuses. Everything it refuses
        // is something this plan could in principle produce, so the two are tied
        // together here rather than discovered on a disk.
        for size in [MINIMUM_DEVICE, 8 * 1024 * 1024 * 1024, 2 * 1024_u64.pow(4)] {
            let plan = plan(size).unwrap();
            let table = plan.table();
            table
                .entry_array()
                .unwrap_or_else(|error| panic!("{size} bytes: {error}"));
            assert_eq!(table.partitions[0].kind, gpt::ESP);
            assert_eq!(table.partitions[1].kind, gpt::LINUX_FILESYSTEM);
        }
    }

    #[test]
    fn the_esp_is_the_only_partition_a_firmware_will_look_in() {
        // And the store must not be given the ESP type. A firmware enumerates every
        // partition of that type looking for \EFI\BOOT\BOOTX64.EFI; a Btrfs partition
        // wearing that label is one more thing for it to fail to read, and on some
        // firmware that is a hang rather than a skip.
        let plan = plan(8 * 1024 * 1024 * 1024).unwrap();
        let esps = plan
            .table()
            .partitions
            .iter()
            .filter(|partition| partition.kind == gpt::ESP)
            .count();
        assert_eq!(esps, 1);
    }
}
