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
//!
//! ## Why the partitions are handed over open
//!
//! Closing a handle that had the whole disk open for writing makes the kernel
//! re-examine the disk on its own, in its own time — which is a second partition
//! rescan, after the one [`reread`] asked for and some unknowable number of
//! milliseconds later. While it runs, every partition is deleted and then made
//! again, and a node opened inside that window fails with `ENXIO`: *No such device
//! or address*, for a name that is right there in `/dev`.
//!
//! Nothing observable says when that has finished. What *is* true is that the
//! kernel refuses to drop the partitions of a disk while any of them is open, so
//! [`appear`] returns the partitions **already open** and the caller keeps the
//! handles: the second rescan then finds the disk busy and leaves it alone.
//!
//! Found on 2026-08-23, on the second install onto the same disk. The first one
//! could not hit it — there were no partitions to delete and no leftover nodes, so
//! the wait below did what it says. The second one had both, and the wait ended
//! before it had waited for anything.

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
        "the kernel made {device}'s partition {number} and would not hand it over at \
         {expected} within {seconds} seconds: {source}\n  \
         `No such device or address` here means the name is there and the partition \
         behind it is not — a node left by the table that was on this disk before."
    )]
    NoNode {
        device: PathBuf,
        number: u32,
        expected: PathBuf,
        seconds: u64,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "{device} reports {size}-byte logical sectors and Thalyx writes partition \
         tables in units of 512.\n  \
         Every address in the table would be at the wrong byte offset, so the disk \
         would come back with no partition table rather than with a broken one."
    )]
    SectorSize { device: PathBuf, size: u64 },

    #[error(
        "{0} is a partition, not a whole disk. Installing writes a partition table \
         at the start of what it is given, and a table written inside a partition \
         is legal, invisible to every tool that looks for one, and boots nothing — \
         while whatever filesystem was there is gone."
    )]
    NotAWholeDisk(PathBuf),
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

/// Whether this sysfs directory is somebody's partition rather than a whole disk.
///
/// The `partition` file is the kernel's own marker: it holds the partition number
/// and exists only inside a partition's directory. Asked of the directory rather
/// than derived from the name for the reason the whole crate asks — `nvme0n1` and
/// `nvme0n1p1` differ by a convention of the tools that print them, and `sda` and
/// `sda1` differ by another.
///
/// Takes the directory rather than the device so it can be exercised without a
/// block device to hand, which the development container has no partitioned one of.
fn is_a_partition(directory: &Path) -> bool {
    directory.join("partition").exists()
}

/// Refuses anything that is not a whole disk.
///
/// Installing writes a partition table at LBA 0 of what it is given. Given a
/// partition, that is a table written *inside* one — legal, invisible to every
/// tool, and it boots nothing, while the filesystem that used to be there is gone.
///
/// Its own function because two callers need it and they need it at different
/// moments: [`of`] so that "is this a whole disk" has an answer, and
/// [`crate::install`] before it writes a byte. Found on 2026-08-07 on Cesar's own
/// machine, where `discos` listed `/dev/sdb3` — 444 GiB of his Fedora — as
/// something `instalar-en` would take.
pub fn whole_disk(device: &Path) -> Result<(), PartitionError> {
    if is_a_partition(&sysfs(device)?) {
        return Err(PartitionError::NotAWholeDisk(device.to_path_buf()));
    }
    Ok(())
}

/// Every partition the kernel currently believes `device` has, by number.
///
/// Errors for a partition, which is the property `discos` relies on to tell a disk
/// from one. It did not hold until 2026-08-07: `/sys/dev/block/<major>:<minor>`
/// exists for both, `read_dir` succeeds on both, and a partition simply has no
/// children with a `partition` file — so this returned `Ok([])` and every partition
/// on the machine was offered as a disk to install onto.
pub fn of(device: &Path) -> Result<Vec<(u32, PathBuf)>, PartitionError> {
    let directory = sysfs(device)?;
    whole_disk(device)?;
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

/// Re-read the table and wait until the kernel will hand over `wanted` partitions,
/// then keep holding them.
///
/// The wait exists for hosts where `/dev` is made by udev rather than by the kernel:
/// sysfs has the partition the instant the ioctl returns, and the node appears when
/// a program in userspace gets round to it. Inside the image there is no udev and no
/// wait — devtmpfs has already made the node — so the loop below normally goes round
/// once, and the timeout is for the machine where it does not.
///
/// **The wait is for the partition, not for the name.** Until 2026-08-23 it asked
/// whether the path existed, which on a disk being installed onto a second time is
/// true before anything has happened at all: the nodes from the table that was there
/// before are still in `/dev`. `exists` answers with `stat(2)`, and `stat(2)` on a
/// node whose partition has been deleted succeeds — it is a name, and the name is
/// fine. Only opening it asks the question the caller needs answered.
///
/// The handles come back with it because the answer has a shelf life; see the note
/// at the top of this file.
pub fn appear(
    device: &Path,
    wanted: usize,
    seconds: u64,
) -> Result<Vec<(PathBuf, std::fs::File)>, PartitionError> {
    reread(device)?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    loop {
        let found = of(device)?;
        if found.len() >= wanted {
            match hold(&found[..wanted]) {
                Ok(open) => return Ok(open),
                Err((number, expected, source)) if std::time::Instant::now() >= deadline => {
                    return Err(PartitionError::NoNode {
                        device: device.to_path_buf(),
                        number,
                        expected,
                        seconds,
                        source,
                    });
                }
                Err(_) => {}
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

/// Open every one of them, or none of them.
///
/// All or nothing on purpose. Holding one partition of a disk is enough to make the
/// kernel refuse to drop any of them, so a half-open set left behind by a failed
/// attempt would block the very rescan this is waiting for — and the next time round
/// would find the same disk in the same wrong state, until the deadline.
///
/// Read-write, which is what the caller needs and therefore the only question worth
/// asking. A probe that opened read-only could succeed where the real open is about
/// to fail, and an instrument that answers an easier question than the one being
/// asked is how this project has been fooled before.
fn hold(
    partitions: &[(u32, PathBuf)],
) -> Result<Vec<(PathBuf, std::fs::File)>, (u32, PathBuf, std::io::Error)> {
    let mut held = Vec::with_capacity(partitions.len());
    for (number, path) in partitions {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
        {
            Ok(handle) => held.push((path.clone(), handle)),
            // `held` goes out of scope here, which closes what was opened. That is
            // the point: see the paragraph above.
            Err(source) => return Err((*number, path.clone(), source)),
        }
    }
    Ok(held)
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

    /// A block major number no driver on this machine has registered.
    ///
    /// Read from `/proc/devices` rather than picked, because a number picked by
    /// hand is a number that is free on the machine where it was picked. `8:200`
    /// looks safely absent until somebody attaches a fourteenth SATA disk, and then
    /// this test opens one of their partitions read-write.
    fn a_major_nobody_has() -> Option<u32> {
        let devices = std::fs::read_to_string("/proc/devices").ok()?;
        let block = devices.split("Block devices:").nth(1)?;
        let taken: Vec<u32> = block
            .lines()
            .filter_map(|line| line.split_whitespace().next()?.parse().ok())
            .collect();
        (60..=250).find(|major| !taken.contains(major))
    }

    #[test]
    fn a_node_that_is_there_and_a_partition_that_is_not_are_two_different_answers() {
        // The whole of the 2026-08-23 defect, in three lines. `exists` is `stat(2)`
        // and `stat(2)` answers about the name: it succeeds on a node whose
        // partition the kernel has deleted. Installing a second time onto the same
        // disk leaves exactly such a node behind, so the wait for the partitions
        // ended before it had waited for anything, and the install died on
        // `opening /dev/loop0p1: No such device or address`.
        let Some(major) = a_major_nobody_has() else {
            eprintln!("NOT PROVEN: every block major on this machine is taken");
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let node = directory.path().join("p1");
        let made = std::process::Command::new("mknod")
            .arg(&node)
            .args(["b", &major.to_string(), "1"])
            .status();
        if !matches!(&made, Ok(status) if status.success()) {
            let gap = "a device node could not be made; mknod(2) needs root";
            assert!(
                std::env::var_os("THALYX_REQUIRE_DEVICE_NODE_TESTS").is_none(),
                "NOT PROVEN: {gap}, and this run demanded it"
            );
            eprintln!("NOT PROVEN: {gap}");
            return;
        }

        // The half that fooled it.
        assert!(node.exists(), "the name is there");

        // And the half that matters: it cannot be opened. **Which** errno says so
        // is a fact about the machine and not about this crate, so the test does
        // not name one. Both of these were captured on 2026-08-23:
        //
        //   ENXIO  — the node is there and no partition is behind it. What the
        //            install hit on Cesar's disk.
        //   EACCES — Fedora mounts /tmp as a tmpfs with `nodev`, and no device node
        //            on such a filesystem can be opened at all, whatever is behind
        //            it. What this very test hit on his machine, after an earlier
        //            version of it asserted ENXIO and failed his whole suite for a
        //            mount option.
        //
        // Both are the property being tested: the name resolves and the device does
        // not. Pinning the errno would have made this a test of where `tempdir()`
        // happens to put things.
        let (number, named, why) = hold(&[(1, node.clone())]).unwrap_err();
        assert_eq!((number, named), (1, node.clone()));
        assert!(
            why.raw_os_error().is_some(),
            "the open failed with no errno at all: {why}"
        );
    }

    #[test]
    fn one_partition_that_will_not_open_means_none_of_them_are_held() {
        // All or nothing, because holding one partition of a disk is enough to make
        // the kernel refuse to drop any of them — so a half-open set left by a
        // failed attempt would block the rescan the next attempt is waiting for.
        let directory = tempfile::tempdir().unwrap();
        let fine = directory.path().join("p1");
        std::fs::write(&fine, b"").unwrap();
        let absent = directory.path().join("p2");

        let (number, named, _) = hold(&[(1, fine), (2, absent.clone())]).unwrap_err();
        assert_eq!((number, named), (2, absent));
    }

    #[test]
    fn a_directory_the_kernel_marked_with_a_partition_number_is_not_a_whole_disk() {
        // The bug this exists to stop, found on hardware on 2026-08-07. `discos`
        // filtered its list with `of(...).is_ok()` and a comment claiming that
        // errors for a partition. It did not: `/sys/dev/block/<major>:<minor>`
        // exists for a partition too, `read_dir` succeeds on it, and it simply has
        // no children carrying a `partition` file — so the answer was `Ok([])` and
        // every partition on the machine was offered as somewhere to install.
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("partition"), "3\n").unwrap();
        assert!(
            is_a_partition(directory.path()),
            "a directory carrying the kernel's own partition marker read as a disk"
        );
    }

    #[test]
    fn a_directory_without_that_marker_is_a_whole_disk() {
        // The control. Without it, a predicate that answered "partition" to
        // everything would pass the test above and leave `discos` listing nothing
        // at all — which looks like a machine with no disks rather than a bug.
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("size"), "7831552\n").unwrap();
        assert!(!is_a_partition(directory.path()));
    }

    #[test]
    fn the_kernel_running_this_agrees_that_a_whole_disk_carries_no_partition_file() {
        // The two tests above are built on a model of sysfs. This one asks the
        // kernel whether the model is right, because a fake that models the wrong
        // property is not a fake, it is a different system — and everything in
        // /sys/block is by definition a whole disk.
        let Ok(entries) = std::fs::read_dir("/sys/block") else {
            eprintln!("NOT PROVEN: no /sys/block here, so the kernel was not asked");
            return;
        };
        let mut asked = 0;
        for entry in entries.flatten() {
            assert!(
                !is_a_partition(&entry.path()),
                "{} is in /sys/block and carries a partition file, so the marker \
                 this relies on does not mean what it is taken to mean",
                entry.path().display()
            );
            asked += 1;
        }
        if asked == 0 {
            eprintln!("NOT PROVEN: /sys/block is empty, so nothing was checked");
        }
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
