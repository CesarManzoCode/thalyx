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
//! ## Two backends, and why the second one had to exist
//!
//! [`Btrfs`] runs the `btrfs` command, for the reason `thalyx-permd` runs
//! `bpftool`: no build-time dependency on kernel headers or on `libbtrfsutil`,
//! and every step can be run by hand while debugging.
//!
//! That reasoning holds on a host and is false inside Thalyx. The image carries
//! the Linux kernel and one program, so there is no `btrfs` to run — and on
//! 2026-08-28 that showed up as `thalyx_attempt` answering `not_a_subvolume`
//! about a workspace that *was* a subvolume, because the spawn failed and a
//! failure to ask was reported as a fact about the filesystem. [`Native`] is the
//! same four operations as the ioctls the kernel exports for them, which is what
//! `intento` uses; the command backend stays for the host, and as the second
//! opinion the two are graded against each other with.
//!
//! ## Why there is a trait
//!
//! Almost none of what Thalyx does with snapshots is Btrfs. Naming them,
//! ordering them, deciding which one a restore means, refusing when the world
//! has moved — all of that is policy, and policy that can only be exercised on
//! a Btrfs filesystem is policy that is never exercised. [`Volumes`] lets the
//! reasoning be tested anywhere, and [`Btrfs`] and [`Native`] are the two
//! implementations that need a real filesystem underneath them.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("could not run `btrfs`: {0}\n  Thalyx requires Btrfs; install btrfs-progs.")]
    Spawn(#[source] std::io::Error),

    #[error("`btrfs {command}` failed: {message}")]
    Btrfs { command: String, message: String },

    #[error("the kernel would not {operation} {path}: {source}")]
    Kernel {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

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

/// Real Btrfs, through the kernel's own ioctls and no binary at all.
///
/// ## The defect this exists for
///
/// `make -C image agent` creates the agent's workspace as a real subvolume, and
/// inside the running machine `thalyx_attempt` answered `not_a_subvolume` anyway.
/// [`Btrfs`] asks the question by running `btrfs subvolume show`, and
/// `vault/09-Notas-Tecnicas/Construccion-del-ISO.md` puts the kernel and one
/// program in the image — so there is no `btrfs` to run, the spawn fails, and a
/// failure to *ask* was being reported as a fact about the filesystem. Rule 10 of
/// `Estrategia-de-Pruebas.md` from the wrong side, and the one verb the whole
/// design leans on — *intenta esto y si sale mal deshazlo* — was the casualty.
///
/// It is the fifth time the answer has been the same one: `bpftool` for the LSM,
/// `cpio` for the initramfs, `mkfs.btrfs` for the store, `partprobe` for the
/// partition table. Thalyx asks the kernel itself.
///
/// ## Why [`Btrfs`] is still here
///
/// This one needs a kernel that answers the ioctls. That is every Btrfs, and it is
/// not every machine — the development container has no Btrfs at all — so the
/// command backend stays as what a person reaches for while debugging on a host,
/// and as the second opinion in `tests/natively.rs`, where the two are asked the
/// same question about the same path and have to agree.
pub struct Native;

impl Native {
    /// A directory descriptor to ask the filesystem about.
    ///
    /// `File::open` on a directory is an `O_RDONLY` open on Linux; nothing is read
    /// through it, it is only something for the ioctl to be answered by.
    fn directory(path: &Path) -> Result<std::fs::File> {
        std::fs::File::open(path).map_err(|source| SnapshotError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Where a subvolume is to be made, and what it is to be called.
    ///
    /// Both ioctls take a parent descriptor and one name, never a path — which is
    /// also the shape that cannot be talked into touching a level further up.
    fn place(path: &Path) -> Result<(&Path, &str)> {
        let bad = |why| SnapshotError::BadName(path.display().to_string(), why);
        let parent = path
            .parent()
            .ok_or_else(|| bad("it has no parent directory"))?;
        let name = path
            .file_name()
            .ok_or_else(|| bad("it does not end in a name"))?
            .to_str()
            .ok_or_else(|| bad("its last component is not UTF-8"))?;
        Ok((parent, name))
    }

    fn create(&self, source: &Path, destination: &Path, read_only: bool) -> Result<()> {
        use std::os::fd::AsFd;

        let (parent, name) = Self::place(destination)?;
        let source_directory = Self::directory(source)?;
        let parent_directory = Self::directory(parent)?;

        thalyx_syscall::btrfs_snapshot_create(
            parent_directory.as_fd(),
            source_directory.as_fd(),
            name,
            read_only,
        )
        .map_err(|source| SnapshotError::Kernel {
            operation: if read_only {
                "take a read-only snapshot at"
            } else {
                "make a writable copy at"
            },
            path: destination.to_path_buf(),
            source,
        })
    }
}

impl Volumes for Native {
    fn is_subvolume(&self, path: &Path) -> Result<bool> {
        use std::os::fd::AsFd;

        let directory = match std::fs::File::open(path) {
            Ok(directory) => directory,
            // Nothing there, or not a directory at all: that is an answer, and it
            // is `false`. Anything else — a permission, an I/O error — is not an
            // answer, and returning `false` for it would be this crate making the
            // same mistake it was written to fix.
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(false);
            }
            Err(source) => {
                return Err(SnapshotError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };

        thalyx_syscall::btrfs_is_subvolume(directory.as_fd()).map_err(|source| {
            SnapshotError::Kernel {
                operation: "say what kind of thing",
                path: path.to_path_buf(),
                source,
            }
        })
    }

    fn snapshot(&self, source: &Path, destination: &Path) -> Result<()> {
        // Read-only, and in one ioctl. A snapshot that is created writable and
        // sealed afterwards is a working copy for as long as the second call takes.
        self.create(source, destination, true)
    }

    fn restore_from(&self, snapshot: &Path, destination: &Path) -> Result<()> {
        // Writable, because this becomes the live tree again.
        self.create(snapshot, destination, false)
    }

    fn delete(&self, subvolume: &Path) -> Result<()> {
        use std::os::fd::AsFd;

        let (parent, name) = Self::place(subvolume)?;
        let parent_directory = Self::directory(parent)?;

        thalyx_syscall::btrfs_subvolume_destroy(parent_directory.as_fd(), name).map_err(|source| {
            SnapshotError::Kernel {
                operation: "delete",
                path: subvolume.to_path_buf(),
                source,
            }
        })
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

                // Carry the modification time over. A real snapshot shares the
                // inode, so every timestamp is identical by construction; a
                // copy that did not do this would differ from its source in
                // every file, and a fake that fails the property under test is
                // not a fake, it is a different system.
                if let Ok(metadata) = entry.metadata()
                    && let Ok(modified) = metadata.modified()
                    && let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH)
                {
                    let _ = thalyx_syscall::set_mtime(&target, since.as_nanos() as i64);
                }
            }
        }
        Ok(())
    }
}

/// What a subvolume gained, lost and changed since a snapshot.
///
/// Produced before a restore, so the human is told what a restore would cost
/// in the same terms it will cost it. `vault/04-Flujo-Canonico/Rollback-vs-Restore.md`
/// requires exactly this: the confirmation shows the diff of what will be lost.
///
/// It is bounded. A subvolume can hold a million files, and a confirmation
/// nobody can read is a confirmation nobody gives meaningfully — so the lists
/// stop at [`Difference::SHOWN`] and the counts keep going.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Difference {
    /// Files that exist now and are not in the snapshot. **These are lost.**
    pub added: Vec<String>,
    /// Files whose contents differ. Restoring returns them to the old version.
    pub modified: Vec<String>,
    /// Files in the snapshot that are gone now. Restoring brings them back.
    pub removed: Vec<String>,
    pub added_total: usize,
    pub modified_total: usize,
    pub removed_total: usize,
    /// Paths neither tree could be read at. Counted as differences, because
    /// what cannot be compared must not be reported as identical.
    pub unreadable: Vec<String>,
}

impl Difference {
    /// How many paths of each kind are listed before the counts take over.
    pub const SHOWN: usize = 20;

    pub fn is_empty(&self) -> bool {
        self.added_total == 0
            && self.modified_total == 0
            && self.removed_total == 0
            && self.unreadable.is_empty()
    }

    /// Work that would be destroyed, as opposed to merely reverted.
    ///
    /// A file created since the snapshot has no older version to go back to:
    /// restoring does not change it, it deletes it. That distinction is the
    /// one the human most needs before answering.
    pub fn lost_outright(&self) -> usize {
        self.added_total
    }
}

/// Compare a live tree against a snapshot of it.
///
/// By size and modification time, not by content. Reading every byte of a
/// subvolume to render a confirmation would make the confirmation take longer
/// than the restore — and this is the same comparison the index's freshness
/// check makes, so a file Thalyx would call changed is a file this calls
/// changed.
pub fn difference(live: &Path, snapshot: &Path) -> Difference {
    let mut difference = Difference::default();
    let mut here = std::collections::BTreeMap::new();
    let mut there = std::collections::BTreeMap::new();

    collect(live, live, &mut here, &mut difference);
    collect(snapshot, snapshot, &mut there, &mut difference);

    for (path, stat) in &here {
        match there.get(path) {
            None => {
                difference.added_total += 1;
                if difference.added.len() < Difference::SHOWN {
                    difference.added.push(path.clone());
                }
            }
            Some(other) if other != stat => {
                difference.modified_total += 1;
                if difference.modified.len() < Difference::SHOWN {
                    difference.modified.push(path.clone());
                }
            }
            Some(_) => {}
        }
    }

    for path in there.keys() {
        if !here.contains_key(path) {
            difference.removed_total += 1;
            if difference.removed.len() < Difference::SHOWN {
                difference.removed.push(path.clone());
            }
        }
    }

    difference
}

fn collect(
    root: &Path,
    directory: &Path,
    into: &mut std::collections::BTreeMap<String, (u64, i64)>,
    difference: &mut Difference,
) {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => {
            // Not skipped. A directory that cannot be read is a difference
            // nobody can rule out, and reporting it as identical is how a
            // confirmation ends up understating what it costs.
            difference
                .unreadable
                .push(relative(root, directory).to_string());
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // A subvolume's snapshots live beside it, but a nested one would
        // otherwise be compared against itself.
        if entry.file_name() == SNAPSHOT_DIR {
            continue;
        }

        match entry.metadata() {
            Ok(metadata) if metadata.is_dir() => collect(root, &path, into, difference),
            Ok(metadata) => {
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos() as i64)
                    .unwrap_or(0);
                into.insert(relative(root, &path).to_string(), (metadata.len(), mtime));
            }
            Err(_) => difference
                .unreadable
                .push(relative(root, &path).to_string()),
        }
    }
}

fn relative<'a>(root: &Path, path: &'a Path) -> std::borrow::Cow<'a, str> {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy()
}

/// What a completed restore did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restored {
    pub snapshot: String,
    /// Where the tree that was replaced now lives.
    ///
    /// Kept rather than deleted. A restore is destructive by design, and
    /// keeping what it replaced costs nothing on Btrfs while turning "this
    /// destroys your work" into "this destroys your work and here is where it
    /// went". Removing it is a separate, deliberate act.
    pub replaced_kept_as: String,
    /// Whether the swap was a single atomic event.
    pub atomic: bool,
}

impl<V: Volumes> Snapshots<V> {
    /// Return the subvolume to a snapshot.
    ///
    /// **Destructive.** Everything created since the snapshot is gone from the
    /// live tree. This function does not ask; the caller must have asked, by
    /// the trusted path, having shown [`difference`].
    ///
    /// The swap is one `RENAME_EXCHANGE` where the filesystem supports it. The
    /// obvious alternative — move the live tree aside, move the restored copy
    /// in — has a window where the directory the human works in does not
    /// exist, and a tree that vanishes for a millisecond is exactly the "half"
    /// that build-then-commit exists to rule out. Where the exchange is not
    /// available it falls back, and says so, so the journal can record that
    /// this restore had a window and the atomic ones did not.
    pub fn restore(&self, name: &str, timestamp: &str) -> Result<Restored> {
        let snapshot = self.find(name)?;

        let directory = self.directory();
        let staging = directory.join(format!(".restoring-{}", std::process::id()));
        let _ = self.volumes.delete(&staging);

        // A writable copy first, and never the snapshot itself. Moving the
        // snapshot into place would consume the moment it records — a restore
        // that could only be done once, and that silently destroyed the thing
        // it restored from.
        self.volumes.restore_from(&snapshot.path, &staging)?;

        let kept = format!("replaced-{}", sanitise(timestamp));
        let kept_path = directory.join(&kept);

        let atomic = match thalyx_syscall::exchange_paths(&staging, &self.subvolume) {
            Ok(()) => true,
            Err(_) => {
                // No exchange on this filesystem. Two renames, with the live
                // tree moved out of the way first so the failure mode is "the
                // tree is missing and the journal says where it went" rather
                // than "the restore half happened".
                std::fs::rename(&self.subvolume, &kept_path).map_err(|source| {
                    SnapshotError::Io {
                        path: self.subvolume.clone(),
                        source,
                    }
                })?;
                std::fs::rename(&staging, &self.subvolume).map_err(|source| SnapshotError::Io {
                    path: staging.clone(),
                    source,
                })?;
                return Ok(Restored {
                    snapshot: snapshot.name,
                    replaced_kept_as: kept,
                    atomic: false,
                });
            }
        };

        // After the exchange the staging name holds the tree that was live.
        // Renaming it is tidying: if this fails the restore already happened
        // and nothing is lost, only oddly named.
        let _ = std::fs::rename(&staging, &kept_path);

        Ok(Restored {
            snapshot: snapshot.name,
            replaced_kept_as: kept,
            atomic,
        })
    }
}

fn sanitise(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}
