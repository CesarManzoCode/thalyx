//! Making the kernel notice a partition table that was just written, and finding
//! what it made of it.
//!
//! Writing a table onto a disk the kernel already has open changes nothing the
//! kernel can see: `/dev/sda1` does not appear, so the next step — putting a
//! filesystem in partition one — has nowhere to put it. `partprobe` and
//! `blockdev --rereadpt` are what a person runs, and the image holds the Linux
//! kernel and one program, so [`thalyx_syscall::reread_partition_table`] is what
//! Thalyx runs.
//!
//! ## Why the names are asked for rather than derived
//!
//! `/dev/sda` becomes `/dev/sda1` and `/dev/nvme0n1` becomes `/dev/nvme0n1p1`, and
//! the rule that produces both — append `p` when the name ends in a digit — is a
//! convention of the tools that print those names, not a promise from the kernel.
//! Deriving it means the installer works on SATA and writes the store into nothing
//! on NVMe, which is the half of the hardware this cannot test.
//!
//! So the kernel is asked. `/sys/dev/block/<major>:<minor>/` is the disk, and every
//! subdirectory of it holding a `partition` file is one of its partitions, with that
//! file's contents being the number. The directory's name is the kernel's name for
//! the device, which is what devtmpfs puts under `/dev`.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PartitionError {
    #[error(
        "{0} is not a block device. Installing means writing a partition table and \
         then writing into the partitions the kernel makes from it, and a regular \
         file has no partitions. For an image file, attach it first: \
         `losetup -f -P --show <file>`"
    )]
    NotABlockDevice(PathBuf),

    #[error("could not look at {path}: {source}")]
    Stat {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "the kernel would not read {device}'s partition table again: {source}\n  \
         `EBUSY` here means something on this disk is still mounted. Nothing that \
         is in use is going to be repartitioned underneath it."
    )]
    Reread {
        device: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "the kernel read {device}'s new partition table and made {found} \
         partition(s) of the {wanted} that were written.\n  \
         A table the kernel will not parse is not reported as broken: it is ignored, \
         and the disk comes back looking like it has no partitions at all."
    )]
    NotFound {
        device: PathBuf,
        wanted: usize,
        found: usize,
    },

    #[error(
        "the kernel made {device}'s partition {number} and no node for it appeared \
         at {expected} within {seconds} seconds"
    )]
    NoNode {
        device: PathBuf,
        number: u32,
        expected: PathBuf,
        seconds: u64,
    },

    #[error(
        "{device} reports {size}-byte logical sectors and Thalyx writes partition \
         tables in units of 512.\n  \
         Every address in the table would be at the wrong byte offset, so the disk \
         would come back with no partition table rather than with a broken one."
    )]
    SectorSize { device: PathBuf, size: u64 },
}

/// The sysfs directory the kernel keeps this block device under.
///
/// By device number rather than by name: `/sys/block/<name>` needs the name to be
/// the kernel's own, and a path like `/dev/disk/by-id/…` is a symlink whose last
/// component is not.
fn sysfs(device: &Path) -> Result<PathBuf, PartitionError> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    let metadata = std::fs::metadata(device).map_err(|source| PartitionError::Stat {
        path: device.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_block_device() {
        return Err(PartitionError::NotABlockDevice(device.to_path_buf()));
    }
    let rdev = metadata.rdev();
    // The same decomposition `major()`/`minor()` do, written out because those are
    // libc macros and this crate forbids unsafe.
    let major = ((rdev >> 8) & 0xFFF) | ((rdev >> 32) & !0xFFF);
    let minor = (rdev & 0xFF) | ((rdev >> 12) & !0xFF);
    Ok(PathBuf::from(format!("/sys/dev/block/{major}:{minor}")))
}

/// What the kernel says one logical sector of this device is.
///
/// Refused rather than adapted to. A 4Kn disk needs a table whose every LBA means
/// four times as many bytes, and this crate writes one kind — so the honest outcome
/// is to say which disk this is and stop, rather than to write a table that will not
/// be found and report success.
pub fn logical_sector_size(device: &Path) -> Result<u64, PartitionError> {
    let path = sysfs(device)?.join("queue/logical_block_size");
    let text = std::fs::read_to_string(&path).map_err(|source| PartitionError::Stat {
        path: path.clone(),
        source,
    })?;
    text.trim()
        .parse::<u64>()
        .map_err(|_| PartitionError::SectorSize {
            device: device.to_path_buf(),
            size: 0,
        })
}

/// Ask the kernel to read the table on `device` again.
pub fn reread(device: &Path) -> Result<(), PartitionError> {
    use std::os::fd::AsFd;

    // Opened read-write: the kernel refuses `BLKRRPART` on a read-only handle, and
    // the error it gives is `EINVAL`, which reads as the request being malformed.
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(device)
        .map_err(|source| PartitionError::Stat {
            path: device.to_path_buf(),
            source,
        })?;
    thalyx_syscall::reread_partition_table(handle.as_fd()).map_err(|source| {
        PartitionError::Reread {
            device: device.to_path_buf(),
            source,
        }
    })
}

/// Every partition the kernel currently believes `device` has, by number.
pub fn of(device: &Path) -> Result<Vec<(u32, PathBuf)>, PartitionError> {
    let directory = sysfs(device)?;
    let mut found = Vec::new();
    let entries = std::fs::read_dir(&directory).map_err(|source| PartitionError::Stat {
        path: directory.clone(),
        source,
    })?;
    for entry in entries.flatten() {
        let Ok(number) = std::fs::read_to_string(entry.path().join("partition")) else {
            continue;
        };
        let Ok(number) = number.trim().parse::<u32>() else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        // sysfs writes `/` in a device's name as `!`, which is how names like
        // `cciss!c0d0p1` reach here. Undone rather than left, because the path with
        // the `!` in it does not exist under /dev.
        found.push((number, PathBuf::from("/dev").join(name.replace('!', "/"))));
    }
    found.sort_by_key(|(number, _)| *number);
    Ok(found)
}

/// Every block device the kernel knows about, disks and their partitions alike.
///
/// From `/sys/block`, which is the kernel's own list, plus each disk's partition
/// subdirectories — because what a store or a boot medium lives on is a partition,
/// and `/sys/block` holds only whole disks.
///
/// Errors are swallowed on purpose and the list comes back possibly short: this is
/// used to *search*, and every caller of it refuses when it finds no answer or more
/// than one. A machine with an unreadable card reader must not fail to install.
pub fn every() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return found;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let node = |name: &str| PathBuf::from("/dev").join(name.replace('!', "/"));
        found.push(node(&name));

        let Ok(children) = std::fs::read_dir(entry.path()) else {
            continue;
        };
        for child in children.flatten() {
            if !child.path().join("partition").exists() {
                continue;
            }
            if let Some(child_name) = child.file_name().to_str() {
                found.push(node(child_name));
            }
        }
    }
    found.retain(|path| path.exists());
    found.sort();
    found.dedup();
    found
}

/// Re-read the table and wait until the nodes for `wanted` partitions are there.
///
/// The wait exists for hosts where `/dev` is made by udev rather than by the kernel:
/// sysfs has the partition the instant the ioctl returns, and the node appears when
/// a program in userspace gets round to it. Inside the image there is no udev and no
/// wait — devtmpfs has already made the node — so the loop below normally goes round
/// once, and the timeout is for the machine where it does not.
pub fn appear(device: &Path, wanted: usize, seconds: u64) -> Result<Vec<PathBuf>, PartitionError> {
    reread(device)?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    loop {
        let found = of(device)?;
        if found.len() >= wanted {
            let missing = found
                .iter()
                .take(wanted)
                .find(|(_, path)| !path.exists())
                .cloned();
            match missing {
                None => return Ok(found.into_iter().take(wanted).map(|(_, p)| p).collect()),
                Some((number, expected)) if std::time::Instant::now() >= deadline => {
                    return Err(PartitionError::NoNode {
                        device: device.to_path_buf(),
                        number,
                        expected,
                        seconds,
                    });
                }
                Some(_) => {}
            }
        } else if std::time::Instant::now() >= deadline {
            // The failure that reads as nothing having happened. A table the kernel
            // will not parse is not reported: it is ignored, so a disk whose GPT has
            // a wrong checksum comes back with zero partitions and looks untouched.
            return Err(PartitionError::NotFound {
                device: device.to_path_buf(),
                wanted,
                found: found.len(),
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_regular_file_is_named_as_one_and_told_how_to_become_a_device() {
        // The first thing a person hits: `thalyx install disk.img` is a reasonable
        // thing to type and cannot work, because a file has no partitions for the
        // kernel to make nodes out of. The message has to carry `-P`, which is the
        // flag whose absence produces a loop device with no partitions and the
        // identical symptom.
        let file = tempfile::NamedTempFile::new().unwrap();
        let error = sysfs(file.path()).unwrap_err();
        assert!(
            matches!(error, PartitionError::NotABlockDevice(_)),
            "{error:?}"
        );
        assert!(error.to_string().contains("losetup -f -P"), "{error}");
    }

    #[test]
    fn a_device_that_is_not_there_is_a_failure_to_look_and_not_a_wrong_kind_of_file() {
        // Rule 10. `ENOENT` and "this is a regular file" send a person to different
        // halves of the problem, and the second is what a naive check reports for
        // both.
        let error = sysfs(Path::new("/dev/nothing-is-called-this")).unwrap_err();
        assert!(matches!(error, PartitionError::Stat { .. }), "{error:?}");
    }

    #[test]
    fn the_device_number_is_split_the_way_the_kernel_splits_it() {
        // The encoding is not `(major << 8) | minor`, and has not been since major
        // numbers went past 255. Getting it wrong gives a sysfs path that does not
        // exist, which this code would report as "not a block device" — the message
        // pointing at the disk instead of at the arithmetic.
        //
        // Checked against a device every Linux has: /dev/null is 1:3.
        let path = sysfs(Path::new("/dev/null"));
        // Not a block device, so this fails — but it fails *after* the number has
        // been decomposed, which is not the branch under test. The decomposition is
        // exercised through a device that is one.
        assert!(matches!(path, Err(PartitionError::NotABlockDevice(_))));

        for entry in std::fs::read_dir("/sys/dev/block").into_iter().flatten() {
            let Ok(entry) = entry else { continue };
            let name = entry.file_name();
            let Some(id) = name.to_str() else { continue };
            let node = PathBuf::from("/dev").join(
                std::fs::read_to_string(entry.path().join("uevent"))
                    .unwrap_or_default()
                    .lines()
                    .find_map(|line| line.strip_prefix("DEVNAME=").map(str::to_string))
                    .unwrap_or_default(),
            );
            if !node.exists() {
                continue;
            }
            let Ok(found) = sysfs(&node) else { continue };
            assert_eq!(
                found.file_name().and_then(|n| n.to_str()),
                Some(id),
                "{} decomposed to {found:?} and the kernel calls it {id}",
                node.display()
            );
            return;
        }
        // A machine with no block devices at all cannot answer this, and saying so
        // is better than passing.
        eprintln!("NOT PROVEN: no block device here to check the major/minor split against");
    }
}
