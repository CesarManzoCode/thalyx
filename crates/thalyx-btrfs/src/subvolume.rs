//! The three subvolumes a store is made of, created without a `btrfs` binary.
//!
//! `vault/04-Flujo-Canonico/Journal-y-Snapshots.md` decrees three — `system`,
//! `modules`, `user` — and `crates/thalyx-cli/src/store_disk.rs` is where PID 1
//! mounts them. A filesystem [`crate::write`] has just produced has none, so it
//! is not yet a store: PID 1 asks for `subvol=system` and gets `ENOENT`.
//!
//! ## Why an ioctl and not `btrfs subvolume create`
//!
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md` puts the kernel and one
//! program in the image. `thalyx-snapshot` runs `btrfs` and is right to — it runs
//! on a host that has btrfs-progs installed. An installer running *inside* the
//! image has no such host and no shell to run it from. Same shape as `bpftool`
//! for the LSM and `cpio` for the initramfs, same answer.
//!
//! ## What this module needs that the rest of the crate does not
//!
//! Everything else here writes bytes into a file. This mounts a filesystem and
//! asks the kernel to change it, so it needs root, a kernel that supports Btrfs,
//! and a **block device** — `mount(2)` answers `ENOTBLK` for a regular file,
//! because attaching a file to a loop device is work util-linux does in userspace
//! and Thalyx has no reason to reimplement: an installer partitions a disk and
//! writes to partitions.
//!
//! ## How it is known to have worked
//!
//! By mounting each one the way PID 1 does, with `-o subvol=<name>`, and saying
//! per name whether that worked. Not by asking whether a directory of that name
//! appeared — a plain directory would answer yes to that and `ENOENT` to PID 1 —
//! and not by the inode-number trick, which `thalyx-snapshot` already declines
//! for a stated reason: a subvolume root being inode 256 is true of Btrfs today
//! and is not a documented interface. Mounting it is the operation that has to
//! work, so it is the operation that gets measured.

use std::path::{Path, PathBuf};

/// The three of `Journal-y-Snapshots.md`.
///
/// `store_disk.rs` has the same three with the paths they mount at, and a test
/// there fails if the two lists ever stop agreeing. Two copies of a decree in two
/// crates is one copy too many; the test is why it is survivable.
pub const DECREED: [&str; 3] = ["system", "modules", "user"];

#[derive(Debug, thiserror::Error)]
pub enum SubvolumeError {
    #[error(
        "{0} is not a block device. Creating a subvolume means mounting the \
         filesystem, and the kernel will not mount a regular file directly. For an \
         image file, attach it first: `losetup -f --show <file>`"
    )]
    NotABlockDevice(PathBuf),

    #[error("could not look at {path}: {source}")]
    Stat {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not make the mount point {path}: {source}")]
    MountPoint {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "could not mount {device} at {at}: {source}\n  \
         A store has to be mountable before it can be given subvolumes. If this \
         kernel has no Btrfs, nothing here can run."
    )]
    Mount {
        device: PathBuf,
        at: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not open {path} to ask the filesystem for a subvolume: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("the kernel refused to create the subvolume `{name}`: {source}")]
    Create {
        name: String,
        #[source]
        source: std::io::Error,
    },
}

/// What happened to one name.
#[derive(Debug, PartialEq, Eq)]
pub enum Made {
    /// The kernel created it.
    Created,
    /// Something of that name was already there, so nothing was created.
    ///
    /// Kept apart from [`Made::Created`] because they are different facts about
    /// the disk, and because *what* was already there is deliberately not guessed
    /// at here — `mounted` below answers that, and answers it by doing the thing
    /// that has to work rather than by inspecting for it.
    AlreadyThere,
}

/// The three names, what happened to each, and whether PID 1 could mount it.
#[derive(Debug)]
pub struct Outcome {
    pub subvolumes: Vec<(String, Made)>,
    /// Per name: `None` if `-o subvol=<name>` mounted, otherwise why it did not.
    pub mounted: Vec<(String, Option<String>)>,
}

impl Outcome {
    /// Whether every name asked for can be mounted the way PID 1 mounts it.
    ///
    /// This and not "were they created" is the question, because a name that was
    /// already there is fine and a directory that is not a subvolume is not.
    pub fn is_a_store(&self) -> bool {
        !self.mounted.is_empty() && self.mounted.iter().all(|(_, why)| why.is_none())
    }
}

/// A mount that goes away when this does.
///
/// Not tidiness. Every path out of [`create`] between the mount and the unmount is
/// an error path, and a store left mounted after a failed format is a device that
/// the human's next command — `losetup -d`, another `format`, unplugging it —
/// finds busy, with nothing saying why.
struct Mounted {
    at: PathBuf,
}

impl Mounted {
    fn new(device: &Path, at: &Path, options: Option<&str>) -> Result<Self, SubvolumeError> {
        std::fs::create_dir_all(at).map_err(|source| SubvolumeError::MountPoint {
            path: at.to_path_buf(),
            source,
        })?;
        thalyx_syscall::mount(Some(device), at, Some("btrfs"), 0, options).map_err(|source| {
            SubvolumeError::Mount {
                device: device.to_path_buf(),
                at: at.to_path_buf(),
                source,
            }
        })?;
        Ok(Self {
            at: at.to_path_buf(),
        })
    }
}

impl Drop for Mounted {
    fn drop(&mut self) {
        // `MNT_DETACH`, so an unmount cannot fail for being busy and leave the
        // caller with a mount it has no handle on any more. The kernel finishes
        // when the last user goes.
        let _ = thalyx_syscall::umount2(&self.at, thalyx_syscall::MNT_DETACH);
    }
}

/// Give a freshly written store its subvolumes, and check that PID 1 could mount them.
///
/// `workspace` is a directory this may create mount points under. Two are used —
/// one for the top of the filesystem, where the subvolumes are created, and one to
/// mount each finished subvolume through — and both are unmounted before returning,
/// including when returning an error.
pub fn create(device: &Path, workspace: &Path, names: &[&str]) -> Result<Outcome, SubvolumeError> {
    use std::os::unix::fs::FileTypeExt;

    let metadata = std::fs::metadata(device).map_err(|source| SubvolumeError::Stat {
        path: device.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_block_device() {
        return Err(SubvolumeError::NotABlockDevice(device.to_path_buf()));
    }

    let mut subvolumes = Vec::with_capacity(names.len());
    {
        let top = Mounted::new(device, &workspace.join("top"), None)?;
        let directory = std::fs::File::open(&top.at).map_err(|source| SubvolumeError::Open {
            path: top.at.clone(),
            source,
        })?;

        for name in names {
            use std::os::fd::AsFd;
            match thalyx_syscall::btrfs_subvolume_create(directory.as_fd(), name) {
                Ok(()) => subvolumes.push(((*name).to_string(), Made::Created)),
                // The one error that is a fact about the disk rather than a
                // failure. A second `format` of the same device rewrites the
                // filesystem and this cannot happen; a `subvolumes` run over a
                // store that already has them is a repair, and refusing it would
                // make the only recovery path a reformat.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    subvolumes.push(((*name).to_string(), Made::AlreadyThere));
                }
                Err(source) => {
                    return Err(SubvolumeError::Create {
                        name: (*name).to_string(),
                        source,
                    });
                }
            }
        }
    }

    // Each one mounted the way PID 1 mounts it, after the top has been unmounted
    // — so this is a fresh mount of the device and not a view of the one that just
    // wrote to it. A check that reuses the writer's handle checks the handle.
    let mut mounted = Vec::with_capacity(names.len());
    for name in names {
        let at = workspace.join("check");
        let outcome = match Mounted::new(device, &at, Some(&format!("subvol={name}"))) {
            Ok(_mount) => None,
            Err(error) => Some(error.to_string()),
        };
        mounted.push(((*name).to_string(), outcome));
    }

    Ok(Outcome {
        subvolumes,
        mounted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_decreed_names_are_the_three_in_the_decree() {
        // Spelled out rather than derived, because this list is the decree and a
        // test that recomputed it from the same constant would agree with any
        // change including a wrong one.
        assert_eq!(DECREED, ["system", "modules", "user"]);
    }

    #[test]
    fn a_regular_file_is_refused_by_name_instead_of_by_errno() {
        // The failure a human hits first: `thalyx disk format store.img` writes a
        // perfectly good filesystem into a file, and then nothing can mount it
        // without a loop device. `mount(2)` says `ENOTBLK`, which does not tell
        // anybody what to do about it.
        let file = tempfile::NamedTempFile::new().unwrap();
        let error = create(file.path(), file.path().parent().unwrap(), &DECREED).unwrap_err();
        assert!(
            matches!(error, SubvolumeError::NotABlockDevice(_)),
            "a regular file gave {error:?} rather than being named as one"
        );
        assert!(
            error.to_string().contains("losetup"),
            "the message does not say how to proceed: {error}"
        );
    }

    #[test]
    fn a_device_that_is_not_there_is_a_failure_to_look_and_says_so() {
        // Rule 10. `ENOENT` on the device and `ENOTBLK` on a file are different
        // facts, and the second used to be reachable by handing this a path that
        // did not exist at all.
        let error = create(
            Path::new("/nonexistent/thalyx-store-device"),
            Path::new("/tmp"),
            &DECREED,
        )
        .unwrap_err();
        assert!(
            matches!(error, SubvolumeError::Stat { .. }),
            "a missing device gave {error:?}"
        );
    }

    #[test]
    fn an_outcome_with_an_unmountable_subvolume_is_not_a_store() {
        // The property the whole module exists for: created is not the same as
        // usable. A directory named `system` that is not a subvolume gets created
        // by nobody here, and would be reported `AlreadyThere` with a mount that
        // failed — which must not read as success.
        let good = Outcome {
            subvolumes: vec![("system".into(), Made::Created)],
            mounted: vec![("system".into(), None)],
        };
        assert!(good.is_a_store());

        let bad = Outcome {
            subvolumes: vec![("system".into(), Made::AlreadyThere)],
            mounted: vec![("system".into(), Some("ENOENT".into()))],
        };
        assert!(!bad.is_a_store());

        // And nothing at all is not a store either, which is the case a
        // `.all()` over an empty list gets wrong on its own.
        let nothing = Outcome {
            subvolumes: vec![],
            mounted: vec![],
        };
        assert!(!nothing.is_a_store());
    }
}
