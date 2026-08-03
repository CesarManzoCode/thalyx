//! Btrfs subvolumes and snapshots, as Thalyx uses them.
//!
//! `vault/04-Flujo-Canonico/Journal-y-Snapshots.md` makes Btrfs a requirement
//! of Phase 1, and `Rollback-vs-Restore.md` explains what it buys: `rollback`
//! takes back what Thalyx published, and `restore` returns a whole subvolume
//! to a moment in time. The second one can destroy the human's work, which is
//! why it has its own name and its own confirmation — and why this crate does
//! nothing but the mechanics, leaving every decision about *whether* to a
//! caller that has asked.
//!
//! ## Why the `btrfs` command and not a library
//!
//! The same reasoning as `bpftool` in `thalyx-permd` and `thalyx-watch`: no
//! build-time dependency on kernel headers or on `libbtrfsutil`, and every
//! step this crate takes can be run by hand and checked while debugging. A
//! snapshot is the last thing that should be happening inside an opaque
//! binding — if Thalyx is about to replace a directory tree, the human should
//! be able to reproduce the exact command that did it.
//!
//! ## Why there is a trait
//!
//! Almost none of what Thalyx does with snapshots is Btrfs. Naming them,
//! ordering them, deciding which one a restore means, refusing when the world
//! has moved — all of that is policy, and policy that can only be exercised on
//! a Btrfs filesystem is policy that is never exercised. [`Volumes`] lets the
//! reasoning be tested anywhere, and [`Btrfs`] is the one implementation that
//! needs a real filesystem underneath it.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("could not run `btrfs`: {0}\n  Thalyx requires Btrfs; install btrfs-progs.")]
    Spawn(#[source] std::io::Error),

    #[error("`btrfs {command}` failed: {message}")]
    Btrfs { command: String, message: String },

    #[error("{0} is not a Btrfs subvolume, so it cannot be snapshotted")]
    NotASubvolume(PathBuf),

    #[error("a snapshot named `{0}` already exists")]
    AlreadyExists(String),

    #[error("no snapshot named `{0}`")]
    NoSuchSnapshot(String),

    #[error("`{0}` is not a usable snapshot name: {1}")]
    BadName(String, &'static str),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, SnapshotError>;

/// Where a subvolume's snapshots live.
///
/// Beside the subvolume rather than inside it, and never in the store: a
/// snapshot has to be on the same filesystem as its source, and the store root
/// is free to be somewhere else entirely.
pub const SNAPSHOT_DIR: &str = ".thalyx-snapshots";

/// A snapshot, as it is on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub name: String,
    pub path: PathBuf,
    /// The subvolume it was taken of.
    pub source: PathBuf,
}

/// The subvolume operations Thalyx needs.
pub trait Volumes {
    fn is_subvolume(&self, path: &Path) -> Result<bool>;
    /// Take a read-only snapshot of `source` at `destination`.
    fn snapshot(&self, source: &Path, destination: &Path) -> Result<()>;
    /// Make a writable copy of a snapshot.
    fn restore_from(&self, snapshot: &Path, destination: &Path) -> Result<()>;
    fn delete(&self, subvolume: &Path) -> Result<()>;
}

/// Real Btrfs, through the `btrfs` command.
pub struct Btrfs {
    command: PathBuf,
}

impl Default for Btrfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Btrfs {
    pub fn new() -> Self {
        Self {
            command: PathBuf::from("btrfs"),
        }
    }

    pub fn with_command(command: impl Into<PathBuf>) -> Self {
        Self {
            command: command.into(),
        }
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        let output = std::process::Command::new(&self.command)
            .args(args)
            .output()
            .map_err(SnapshotError::Spawn)?;

        if !output.status.success() {
            return Err(SnapshotError::Btrfs {
                command: args.join(" "),
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl Volumes for Btrfs {
    fn is_subvolume(&self, path: &Path) -> Result<bool> {
        // `subvolume show` fails for a plain directory, which is the answer.
        // Asked this way rather than by comparing inode numbers because the
        // inode trick (a subvolume root is inode 256) is true of Btrfs today
        // and is not a documented interface.
        Ok(self
            .run(&["subvolume", "show", &path.to_string_lossy()])
            .is_ok())
    }

    fn snapshot(&self, source: &Path, destination: &Path) -> Result<()> {
        // Read-only. A snapshot that can be written is not a record of a
        // moment, it is another working copy that will quietly drift from what
        // it claims to be.
        self.run(&[
            "subvolume",
            "snapshot",
            "-r",
            &source.to_string_lossy(),
            &destination.to_string_lossy(),
        ])
        .map(|_| ())
    }

    fn restore_from(&self, snapshot: &Path, destination: &Path) -> Result<()> {
        // Writable, because this becomes the live tree again.
        self.run(&[
            "subvolume",
            "snapshot",
            &snapshot.to_string_lossy(),
            &destination.to_string_lossy(),
        ])
        .map(|_| ())
    }

    fn delete(&self, subvolume: &Path) -> Result<()> {
        self.run(&["subvolume", "delete", &subvolume.to_string_lossy()])
            .map(|_| ())
    }
}

/// Snapshots of one subvolume.
pub struct Snapshots<V: Volumes> {
    volumes: V,
    subvolume: PathBuf,
}

impl<V: Volumes> Snapshots<V> {
    pub fn of(volumes: V, subvolume: impl Into<PathBuf>) -> Self {
        Self {
            volumes,
            subvolume: subvolume.into(),
        }
    }

    pub fn subvolume(&self) -> &Path {
        &self.subvolume
    }

    /// Where snapshots of this subvolume live.
    pub fn directory(&self) -> PathBuf {
        self.subvolume
            .parent()
            .unwrap_or(Path::new("/"))
            .join(SNAPSHOT_DIR)
    }

    /// Take a snapshot.
    ///
    /// Refuses if the source is not a subvolume rather than falling back to
    /// copying the tree. A copy is not a snapshot: it is not atomic, it takes
    /// time proportional to the data, and something that took twenty minutes
    /// is a picture of twenty minutes rather than of an instant.
    pub fn take(&self, name: &str) -> Result<Snapshot> {
        validate_name(name)?;

        if !self.volumes.is_subvolume(&self.subvolume)? {
            return Err(SnapshotError::NotASubvolume(self.subvolume.clone()));
        }

        let directory = self.directory();
        std::fs::create_dir_all(&directory).map_err(|source| SnapshotError::Io {
            path: directory.clone(),
            source,
        })?;

        let path = directory.join(name);
        if path.exists() {
            return Err(SnapshotError::AlreadyExists(name.to_string()));
        }

        self.volumes.snapshot(&self.subvolume, &path)?;

        Ok(Snapshot {
            name: name.to_string(),
            path,
            source: self.subvolume.clone(),
        })
    }

    /// Every snapshot of this subvolume, oldest name first.
    ///
    /// Sorted by name, and names carry a timestamp, so this is chronological
    /// for anything Thalyx made. It does not stat them: a snapshot's mtime is
    /// the source's mtime, not the moment it was taken, so ordering by it
    /// would put them in the wrong order and look right.
    pub fn list(&self) -> Result<Vec<Snapshot>> {
        let directory = self.directory();
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(SnapshotError::Io {
                    path: directory,
                    source,
                });
            }
        };

        let mut snapshots: Vec<Snapshot> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                validate_name(&name).ok()?;
                Some(Snapshot {
                    name,
                    path: entry.path(),
                    source: self.subvolume.clone(),
                })
            })
            .collect();

        snapshots.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(snapshots)
    }

    pub fn find(&self, name: &str) -> Result<Snapshot> {
        self.list()?
            .into_iter()
            .find(|snapshot| snapshot.name == name)
            .ok_or_else(|| SnapshotError::NoSuchSnapshot(name.to_string()))
    }

    /// The most recent one, if there is one.
    pub fn latest(&self) -> Result<Option<Snapshot>> {
        Ok(self.list()?.pop())
    }

    /// Delete a snapshot.
    ///
    /// Only ever a snapshot: the name is looked up in the snapshot directory
    /// first, so a caller cannot hand this a path and have a subvolume
    /// somewhere else deleted.
    pub fn forget(&self, name: &str) -> Result<()> {
        let snapshot = self.find(name)?;
        self.volumes.delete(&snapshot.path)
    }

    pub fn volumes(&self) -> &V {
        &self.volumes
    }
}

/// A name that is safe to use as a directory beside a subvolume.
///
/// The restriction is not tidiness. These names arrive from a command line and
/// become path components, so a `..` or a `/` would put a snapshot — or a
/// deletion — somewhere nobody asked for.
pub fn validate_name(name: &str) -> Result<()> {
    let bad = |why| SnapshotError::BadName(name.to_string(), why);

    if name.is_empty() {
        return Err(bad("it is empty"));
    }
    if name.len() > 128 {
        return Err(bad("it is longer than 128 characters"));
    }
    if name.starts_with('.') {
        return Err(bad("it starts with a dot"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        return Err(bad(
            "only letters, digits, dash, underscore, dot and colon are allowed",
        ));
    }
    Ok(())
}

/// A snapshot name from a label and the moment it is being taken.
///
/// The timestamp is first so that sorting by name sorts by time. A label first
/// would group by label and interleave the times, and every "the most recent
/// one" answer would be wrong for as long as nobody looked closely.
pub fn name_for(label: &str, timestamp: &str) -> String {
    let stamp: String = timestamp
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    let label: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    if label.is_empty() {
        stamp
    } else {
        format!("{stamp}-{label}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_could_escape_the_snapshot_directory_is_refused() {
        // These arrive from a command line and become path components. A `..`
        // would put a snapshot, or a deletion, somewhere nobody asked for.
        for name in ["..", "../elsewhere", "a/b", "/absolute", ".hidden", ""] {
            assert!(
                validate_name(name).is_err(),
                "`{name}` should not be a usable snapshot name"
            );
        }
    }

    #[test]
    fn ordinary_names_are_allowed() {
        for name in [
            "2026-08-03T04:00:00Z-before-upgrade",
            "nightly",
            "v1_2_3",
            "a.b",
        ] {
            assert!(validate_name(name).is_ok(), "`{name}` should be usable");
        }
    }

    #[test]
    fn the_timestamp_comes_first_so_names_sort_by_time() {
        // Label first would group by label and interleave the times, and every
        // "the most recent one" answer would be wrong while looking right.
        let mut names = [
            name_for("upgrade", "2026-08-03T04:00:00Z"),
            name_for("aaa", "2026-08-03T05:00:00Z"),
            name_for("zzz", "2026-08-03T03:00:00Z"),
        ];
        names.sort();

        assert!(names[0].contains("03T03"), "{names:?}");
        assert!(names[1].contains("03T04"), "{names:?}");
        assert!(names[2].contains("03T05"), "{names:?}");
    }

    #[test]
    fn a_generated_name_is_always_a_valid_one() {
        // The label reaches this from a command line, so the sanitising is
        // what stands between a slash in a label and a path component.
        let name = name_for("../../etc", "2026-08-03T04:00:00Z");
        assert!(validate_name(&name).is_ok(), "{name}");
        assert!(!name.contains('/'), "{name}");
    }
}

/// A [`Volumes`] backed by ordinary directories.
///
/// Not a Btrfs emulator, and it must never be mistaken for one: it copies
/// where Btrfs shares blocks, so it is neither atomic nor cheap. What it is
/// for is everything about snapshots that is *not* Btrfs — naming, ordering,
/// which one a restore means, refusing when the world has moved — because
/// policy that can only be exercised on a Btrfs filesystem is policy that is
/// never exercised.
pub mod directories {
    use super::*;

    /// The file that makes a directory a subvolume, as far as this pretends.
    const MARKER: &str = ".thalyx-fake-subvolume";

    pub struct Directories;

    impl Directories {
        /// Make a directory look like a subvolume.
        pub fn make_subvolume(path: &Path) -> Result<()> {
            std::fs::create_dir_all(path).map_err(|source| SnapshotError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            std::fs::write(path.join(MARKER), b"").map_err(|source| SnapshotError::Io {
                path: path.join(MARKER),
                source,
            })
        }
    }

    impl Volumes for Directories {
        fn is_subvolume(&self, path: &Path) -> Result<bool> {
            Ok(path.join(MARKER).exists())
        }

        fn snapshot(&self, source: &Path, destination: &Path) -> Result<()> {
            copy_tree(source, destination)
        }

        fn restore_from(&self, snapshot: &Path, destination: &Path) -> Result<()> {
            copy_tree(snapshot, destination)
        }

        fn delete(&self, subvolume: &Path) -> Result<()> {
            std::fs::remove_dir_all(subvolume).map_err(|source| SnapshotError::Io {
                path: subvolume.to_path_buf(),
                source,
            })
        }
    }

    fn copy_tree(from: &Path, to: &Path) -> Result<()> {
        let io = |path: &Path, source: std::io::Error| SnapshotError::Io {
            path: path.to_path_buf(),
            source,
        };

        std::fs::create_dir_all(to).map_err(|e| io(to, e))?;
        for entry in std::fs::read_dir(from).map_err(|e| io(from, e))? {
            let entry = entry.map_err(|e| io(from, e))?;
            let target = to.join(entry.file_name());

            // The snapshot directory is beside the subvolume, not inside it,
            // but a caller is free to nest trees however they like and a copy
            // that recursed into its own destination would never finish.
            if entry.path() == to {
                continue;
            }

            if entry.path().is_dir() {
                copy_tree(&entry.path(), &target)?;
            } else {
                std::fs::copy(entry.path(), &target).map_err(|e| io(&entry.path(), e))?;
            }
        }
        Ok(())
    }
}
