//! cgroup v2 as the identity a kernel policy is attached to.
//!
//! The LSM keys its policy map on `bpf_get_current_cgroup_id()`, which is the
//! inode number of the cgroup directory. That is the whole reason this module
//! exists: a module's permissions have to hang on something the kernel can see
//! at the moment of a hook, and a cgroup is that something.
//!
//! No controller is enabled here. Thalyx uses the cgroup for identity, not for
//! resource limits — those come later, in the `module_standard` profile. Not
//! touching `cgroup.subtree_control` also means nothing has to be delegated for
//! this to work.

use crate::{Result, SandboxError};
use std::path::{Path, PathBuf};

/// Where the cgroup2 hierarchy is mounted, when it should not be discovered.
pub const MOUNT_ENV: &str = "THALYX_CGROUP_ROOT";

/// The directory under the cgroup2 root that Thalyx owns.
pub const PARENT_NAME: &str = "thalyx";

/// Present in every cgroup v2 directory and in no cgroup v1 one.
const MARKER: &str = "cgroup.controllers";

/// Where a process is written to join, and read to see who is in.
const PROCS: &str = "cgroup.procs";

/// The mounted cgroup2 root.
///
/// Discovered from `/proc/mounts` rather than assumed to be `/sys/fs/cgroup`:
/// on a host running the legacy hierarchies the unified one is mounted
/// somewhere else entirely, and guessing wrong would mean creating an ordinary
/// directory that looks like a cgroup and confines nothing.
pub fn mount_point() -> Result<PathBuf> {
    if let Some(override_path) = std::env::var_os(MOUNT_ENV) {
        let path = PathBuf::from(override_path);
        if !is_cgroup2(&path) {
            return Err(SandboxError::NotCgroup2(path));
        }
        return Ok(path);
    }

    let mounts = std::fs::read_to_string("/proc/mounts")
        .map_err(|source| SandboxError::io("/proc/mounts", source))?;

    mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _device = fields.next()?;
            let mount_point = fields.next()?;
            let fstype = fields.next()?;
            (fstype == "cgroup2").then(|| PathBuf::from(unescape(mount_point)))
        })
        .find(|path| is_cgroup2(path))
        .ok_or(SandboxError::NoCgroup2)
}

/// The Thalyx-owned parent, created if it is not there yet.
pub fn parent() -> Result<PathBuf> {
    let path = mount_point()?.join(PARENT_NAME);
    create_or_reuse(&path)?;
    Ok(path)
}

/// Create a cgroup directory, accepting one that is already there.
///
/// Asked as `mkdir`-and-then-look rather than look-and-then-`mkdir`, because
/// the second is a race and the race lands. Two `ejecutar` runs starting at the
/// same moment both find `/sys/fs/cgroup/thalyx` missing, both call `mkdir`,
/// and the loser used to be handed `File exists (os error 17)` — a machine
/// reported as broken because it was *not* broken, and the cheapest possible
/// reading of the error said the opposite of what happened. It cost a stage of
/// `verify.sh` and one test in ten, non-deterministically, which is the worst
/// way for anything to fail.
///
/// `EEXIST` is only forgiven for a directory. A *file* by this name is a
/// machine that is genuinely not what Thalyx expects, and swallowing that would
/// hand the caller a path no process can ever join, with every step reporting
/// success — the failure with no symptom this module already exists to refuse.
fn create_or_reuse(path: &Path) -> Result<()> {
    match std::fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            if path.is_dir() {
                Ok(())
            } else {
                Err(SandboxError::io(path, source))
            }
        }
        Err(source) => Err(SandboxError::io(path, source)),
    }
}

/// Whether a directory is really on a cgroup2 filesystem.
///
/// Checked by the presence of a file the kernel puts in every cgroup v2
/// directory. An ordinary directory would accept a write to `cgroup.procs` and
/// confine nothing, which is the failure with no symptom.
pub fn is_cgroup2(path: &Path) -> bool {
    path.join(MARKER).is_file()
}

/// A cgroup Thalyx placed a module in.
#[derive(Debug, Clone)]
pub struct Cgroup {
    path: PathBuf,
}

impl Cgroup {
    /// Open an existing cgroup, or create it under `parent`.
    ///
    /// One cgroup per module, not per process: the policy is a property of the
    /// module, so two instances of the same module belong in the same cgroup
    /// and share the same grants. Anything else would let a second instance
    /// hold permissions the user confirmed once but sees as two entries.
    pub fn ensure(parent: &Path, name: &str) -> Result<Self> {
        if !is_cgroup2(parent) {
            return Err(SandboxError::NotCgroup2(parent.to_path_buf()));
        }
        let name = validate_name(name)?;
        let path = parent.join(name);

        create_or_reuse(&path)?;
        Self::attach(&path)
    }

    /// Open a cgroup that already exists, refusing anything that is not one.
    pub fn attach(path: &Path) -> Result<Self> {
        if !path.is_dir() {
            return Err(SandboxError::NoSuchCgroup(path.to_path_buf()));
        }
        if !is_cgroup2(path) {
            return Err(SandboxError::NotCgroup2(path.to_path_buf()));
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The id the kernel reports for this cgroup: its inode number.
    ///
    /// Delegated to `thalyx-permd` so that the value written into the map and
    /// the value the LSM compares against are derived by the same code.
    pub fn id(&self) -> Result<u64> {
        Ok(thalyx_permd::cgroup_id(&self.path)?)
    }

    /// Move a process into this cgroup.
    pub fn join(&self, pid: u32) -> Result<()> {
        use std::io::Write;

        let procs = self.path.join(PROCS);
        // Appended rather than truncated. The kernel ignores the file offset
        // on `cgroup.procs`, so this is the same write for it — and it is the
        // difference between a faithful and a destructive one everywhere else.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&procs)
            .map_err(|source| SandboxError::io(&procs, source))?;
        write!(file, "{pid}").map_err(|source| SandboxError::io(&procs, source))?;
        Ok(())
    }

    /// The processes currently in this cgroup.
    pub fn members(&self) -> Result<Vec<u32>> {
        let procs = self.path.join(PROCS);
        let contents = std::fs::read_to_string(&procs).map_err(|source| {
            SandboxError::MembershipUnreadable {
                cgroup: self.path.clone(),
                source,
            }
        })?;
        Ok(contents
            .split_whitespace()
            .filter_map(|pid| pid.parse().ok())
            .collect())
    }

    pub fn contains(&self, pid: u32) -> Result<bool> {
        Ok(self.members()?.contains(&pid))
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.members()?.is_empty())
    }

    /// Kill every process in this cgroup, whatever it is.
    ///
    /// `cgroup.kill` and not a walk of `cgroup.procs` sending signals: the walk
    /// races with the tree it is walking, and the tree here is a compiler that
    /// forks. A build script started between the read and the signal survives
    /// it. The kernel's own file has no such window — one write and every
    /// member of the cgroup and its descendants gets `SIGKILL`, atomically.
    ///
    /// It is a plain file write, which is why this is here rather than in
    /// `thalyx-syscall`: nothing about it is `unsafe`.
    ///
    /// `cgroup.kill` arrived in Linux 5.14. On an older kernel the file is not
    /// there, and this says so rather than pretending — a caller that read
    /// "killed" and got a live process tree would leave a compiler running
    /// under a policy that had just been withdrawn.
    pub fn kill(&self) -> Result<()> {
        let path = self.path.join("cgroup.kill");
        if !path.exists() {
            return Err(SandboxError::NoSuchCgroup(path));
        }
        std::fs::write(&path, "1").map_err(|source| SandboxError::io(&path, source))
    }

    /// Delete the cgroup.
    ///
    /// Only ever correct once the policy keyed on its id has been withdrawn:
    /// the id is an inode number, inode numbers are reused, and a map entry
    /// outliving its directory would hand a future cgroup permissions nobody
    /// granted it. See `Confinement::release`.
    ///
    /// A cgroup that is already gone is the outcome this asks for, not a
    /// failure to reach it. The same race as `create_or_reuse`, from the other
    /// end: two instances of one module can both find the cgroup empty and both
    /// `rmdir` it, and the loser would report `No such file or directory` about
    /// a guest that had already run and exited exactly as it was meant to.
    pub fn remove(&self) -> Result<()> {
        match std::fs::remove_dir(&self.path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(SandboxError::io(&self.path, source)),
        }
    }
}

/// Reject anything that would name a directory other than the one intended.
///
/// Module ids are reverse-DNS, so this is permissive by design — but a
/// separator or a relative component would escape the parent, and `.`/`..`
/// name the parent itself.
fn validate_name(name: &str) -> Result<&str> {
    let acceptable = !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));

    if acceptable {
        Ok(name)
    } else {
        Err(SandboxError::UnusableName(name.to_string()))
    }
}

/// Undo the octal escaping `/proc/mounts` applies to spaces and tabs.
fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();

    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let digits: String = chars.clone().take(3).collect();
        match u8::from_str_radix(&digits, 8) {
            Ok(byte) if digits.len() == 3 => {
                out.push(byte as char);
                chars.nth(2);
            }
            _ => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A directory shaped like a cgroup v2 one.
    ///
    /// Enough to exercise the protocol — which file is written, what is read
    /// back, what is refused — without root.
    ///
    /// What it deliberately does **not** model is the kernel populating the
    /// interface files when a cgroup is created, or `rmdir` succeeding on a
    /// directory that still contains them. Faking those would mean asserting
    /// against a model of the kernel rather than the kernel, and this project
    /// has already learnt what that costs. Anything that depends on them is
    /// tested in `tests/real_cgroup.rs`, against a real mount.
    pub(crate) fn fake_cgroup2(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join(MARKER), "").unwrap();
        std::fs::write(path.join(PROCS), "").unwrap();
    }

    #[test]
    fn an_ordinary_directory_is_not_accepted_as_a_cgroup() {
        // The failure this prevents has no symptom: writing a pid into a plain
        // file succeeds, and the process runs completely unconfined while
        // every step reports success.
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("not-a-cgroup");
        std::fs::create_dir(&plain).unwrap();

        assert!(!is_cgroup2(&plain));
        assert!(matches!(
            Cgroup::attach(&plain),
            Err(SandboxError::NotCgroup2(_))
        ));
    }

    #[test]
    fn a_missing_directory_is_distinguished_from_a_wrong_one() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            Cgroup::attach(&dir.path().join("absent")),
            Err(SandboxError::NoSuchCgroup(_))
        ));
    }

    #[test]
    fn two_runs_racing_to_create_the_same_cgroup_both_get_it() {
        // The defect this replaced was invisible to every sequential test: the
        // old code looked before it created, so the second caller only ever hit
        // `EEXIST` when it looked *before* the first caller's `mkdir` landed.
        // One test in ten failed, on a machine with enough cores to lose the
        // race, and the message it produced — `File exists` — reads as a broken
        // machine rather than as two callers agreeing.
        //
        // The barrier is what makes the two `mkdir` calls actually overlap
        // rather than happen to.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("thalyx");

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let racers: Vec<_> = (0..8)
            .map(|_| {
                let barrier = std::sync::Arc::clone(&barrier);
                let path = path.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    create_or_reuse(&path)
                })
            })
            .collect();

        for racer in racers {
            racer
                .join()
                .unwrap()
                .expect("a racer was told the cgroup could not be created");
        }
        assert!(path.is_dir());
    }

    #[test]
    fn a_file_in_the_way_of_a_cgroup_is_still_refused() {
        // The half of `EEXIST` that must never be forgiven. A regular file
        // accepts a write to something called `cgroup.procs` and confines
        // nothing, so a caller handed this path would report success at every
        // step and run the guest completely free.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("thalyx");
        std::fs::write(&path, "not a directory").unwrap();

        assert!(create_or_reuse(&path).is_err());
    }

    #[test]
    fn removing_a_cgroup_that_is_already_gone_is_not_a_failure() {
        // The same race from the teardown end: two instances of one module can
        // both find the cgroup empty and both `rmdir` it. Reporting `No such
        // file or directory` would fail a run whose guest had already done
        // exactly what it was asked, and rule 10 cuts the other way here —
        // this is not a failure to read, it is the outcome, reached by somebody
        // else.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cgroup2");
        fake_cgroup2(&path);
        let cgroup = Cgroup::attach(&path).unwrap();

        std::fs::remove_dir_all(&path).unwrap();
        cgroup
            .remove()
            .expect("a cgroup already gone was reported as a failure");
    }

    #[test]
    fn joining_writes_the_pid_and_membership_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("cgroup2");
        fake_cgroup2(&parent);
        fake_cgroup2(&parent.join("org.thalyx.demo"));

        let cgroup = Cgroup::ensure(&parent, "org.thalyx.demo").unwrap();
        assert!(cgroup.is_empty().unwrap());

        cgroup.join(4321).unwrap();
        assert!(cgroup.contains(4321).unwrap());
        assert!(!cgroup.contains(1).unwrap());
        assert!(!cgroup.is_empty().unwrap());
    }

    #[test]
    fn ensuring_an_existing_cgroup_reuses_it() {
        // Two instances of one module share a cgroup, because the policy
        // belongs to the module. Creating a second one would give the same
        // grants two identities in the map and two chances to be left behind.
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("cgroup2");
        fake_cgroup2(&parent);
        fake_cgroup2(&parent.join("org.thalyx.demo"));

        let first = Cgroup::ensure(&parent, "org.thalyx.demo").unwrap();
        first.join(11).unwrap();
        let second = Cgroup::ensure(&parent, "org.thalyx.demo").unwrap();

        assert_eq!(first.path(), second.path());
        assert!(second.contains(11).unwrap());
    }

    #[test]
    fn refuses_names_that_would_escape_the_parent() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("cgroup2");
        fake_cgroup2(&parent);

        for name in ["..", ".", "", "../elsewhere", "a/b", "with space"] {
            assert!(
                matches!(
                    Cgroup::ensure(&parent, name),
                    Err(SandboxError::UnusableName(_))
                ),
                "`{name}` should be refused as a cgroup name"
            );
        }
    }

    #[test]
    fn accepts_a_reverse_dns_module_id() {
        assert_eq!(validate_name("org.thalyx.demo").unwrap(), "org.thalyx.demo");
        assert_eq!(
            validate_name("com.example.a-b_1").unwrap(),
            "com.example.a-b_1"
        );
    }

    #[test]
    fn mount_points_with_escaped_characters_are_read_back_correctly() {
        assert_eq!(unescape("/sys/fs/cgroup"), "/sys/fs/cgroup");
        assert_eq!(unescape(r"/mnt/with\040space"), "/mnt/with space");
        assert_eq!(unescape(r"/trailing\"), r"/trailing\");
    }

    #[test]
    fn an_override_that_is_not_a_cgroup2_mount_is_refused() {
        // Better to fail than to silently create directories somewhere that
        // confines nothing. The variable exists for tests and for hosts where
        // discovery is wrong, not as a way to opt out of confinement.
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_cgroup2(dir.path()));
    }
}
