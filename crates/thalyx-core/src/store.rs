//! On-disk layout.
//!
//! The one rule that shapes everything here: **the staging area lives in the
//! same subvolume as the destination**. `rename(2)` is atomic within a
//! filesystem, but returns `EXDEV` across filesystems — and, less obviously,
//! across Btrfs subvolumes too. Staging under `/tmp` (tmpfs on Alpine) would
//! make the atomic commit fail every time.
//!
//! ```text
//! <root>/
//!   .staging/<uuid>/                     build area, same subvolume as modules/
//!   modules/<id>/<version>/              published versions
//!   modules/<id>/<version>/.thalyx/      Thalyx's record, not the module's files
//!   modules/<id>/current                 symlink -> <version>
//!   state/keys.json                      pinned publisher keys
//!   state/permissions.json               granted permissions
//!   state/lock                            the global contract lock
//!   journal.jsonl
//! ```
//!
//! See `vault/04-Flujo-Canonico/Fase-Commit-Atomico.md`.

use crate::{CoreError, Result};
use std::path::{Path, PathBuf};

pub struct Store {
    root: PathBuf,
}

/// The global lock decreed by `vault/04-Flujo-Canonico/Concurrencia.md`.
///
/// Held for the whole of a contract, released when it is dropped — or when the
/// process holding it dies, which is the property that matters most. A crash
/// mid-install must not leave the machine unable to install anything again.
///
/// ## What this closes
///
/// The decree said `thalyx-core` is the only writer and serialises contracts.
/// Nothing implemented it, and the individual atomic steps did not add up to
/// one: the permission registry, the keystore, the uid registry and the
/// `current` symlink are four separate files, and two processes interleaving
/// between them could each write a state the other never saw. Two installs
/// racing could hand the same uid to different modules, or leave one module's
/// grants recorded under the other's commit.
///
/// A single `rename` is atomic. A transaction across four files is not, and no
/// arrangement of renames makes it so.
///
/// ## What it does not promise
///
/// Arrival order. `flock` wakes one waiter, not the one that waited longest,
/// so the decree's "queued in order of arrival" is serialisation without a
/// guaranteed order. Phase 1 has one user and one agent, so no two contracts
/// contend in a way anybody could observe the order of — and a fair queue
/// would need a broker process, which is a larger thing than the problem.
#[must_use = "the lock is released the moment it is dropped"]
pub struct ContractLock {
    _file: std::fs::File,
}

impl Store {
    /// Open a store, creating the directory skeleton if needed.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let store = Self { root: root.into() };
        for dir in [
            store.staging_root(),
            store.modules_root(),
            store.state_root(),
        ] {
            std::fs::create_dir_all(&dir).map_err(|e| CoreError::io(&dir, e))?;
        }
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn staging_root(&self) -> PathBuf {
        self.root.join(".staging")
    }

    pub fn modules_root(&self) -> PathBuf {
        self.root.join("modules")
    }

    pub fn state_root(&self) -> PathBuf {
        self.root.join("state")
    }

    /// Where bundles wait to be installed.
    ///
    /// A directory of `.thmod` files — that is all a local repository is, and
    /// `repo::resolve` picks the highest version in it whose signature
    /// validates. It is separate from `modules_root` because the two answer
    /// different questions: this one is what *could* be installed, that one is
    /// what *is*.
    ///
    /// It exists because of `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md`,
    /// whose second step is a person installing a signed module from a local
    /// repository. Inside the image there is no shell to hand a path to, so the
    /// repository has to be somewhere the session can find on its own.
    pub fn repo_root(&self) -> PathBuf {
        self.root.join("repo")
    }

    pub fn journal_path(&self) -> PathBuf {
        self.root.join("journal.jsonl")
    }

    pub fn keystore_path(&self) -> PathBuf {
        self.state_root().join("keys.json")
    }

    pub fn permissions_path(&self) -> PathBuf {
        self.state_root().join("permissions.json")
    }

    /// Which user each module runs as.
    pub fn uids_path(&self) -> PathBuf {
        self.state_root().join("uids.json")
    }

    /// The current session id, for `session` permissions. See [`crate::session`].
    pub fn session_path(&self) -> PathBuf {
        self.state_root().join("session")
    }

    /// The file the global contract lock is taken on.
    ///
    /// Its contents are never read. What matters is the open file description,
    /// which is what `flock` attaches to.
    pub fn lock_path(&self) -> PathBuf {
        self.state_root().join("lock")
    }

    /// Take the global lock, waiting for whoever holds it.
    ///
    /// Every operation that writes more than one thing takes this: install,
    /// remove, rollback and restore. Reads do not, because each of them reads
    /// one file and a torn read of one file is not a state the lock could
    /// prevent — the writers all publish by `rename`.
    pub fn lock(&self) -> Result<ContractLock> {
        use std::os::fd::AsFd;

        let path = self.lock_path();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| CoreError::io(&path, e))?;

        thalyx_syscall::lock_exclusive(file.as_fd()).map_err(|e| CoreError::io(&path, e))?;

        Ok(ContractLock { _file: file })
    }

    /// Whether another process is inside a contract right now.
    ///
    /// For diagnosis only. The answer is stale the instant it is returned, so
    /// nothing may act on it — [`Store::lock`] is the only thing that decides.
    pub fn contract_in_progress(&self) -> Result<bool> {
        use std::os::fd::AsFd;

        let path = self.lock_path();
        let file = match std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(e) => return Err(CoreError::io(&path, e)),
        };

        let acquired = thalyx_syscall::try_lock_exclusive(file.as_fd())
            .map_err(|e| CoreError::io(&path, e))?;
        Ok(!acquired)
    }

    pub fn module_root(&self, id: &str) -> PathBuf {
        self.modules_root().join(id)
    }

    pub fn version_dir(&self, id: &str, version: &str) -> PathBuf {
        self.module_root(id).join(version)
    }

    /// The `current` symlink. Its target is the single source of truth for
    /// "which version is installed"; a version directory that exists but is not
    /// pointed at is an interrupted commit, not an installation.
    pub fn current_link(&self, id: &str) -> PathBuf {
        self.module_root(id).join("current")
    }

    /// Thalyx's own directory inside a module tree.
    ///
    /// Written by the core during staging, so it is published by the same
    /// `rename` as the module's files. There is no moment at which a version
    /// directory exists without its record — which is what lets the runtime
    /// treat a missing manifest as corruption rather than as a normal state.
    pub fn reserved_dir(&self, id: &str, version: &str) -> PathBuf {
        self.version_dir(id, version)
            .join(crate::bundle::RESERVED_DIR)
    }

    /// The manifest as it arrived, kept beside the module it describes.
    pub fn manifest_path(&self, id: &str, version: &str) -> PathBuf {
        self.reserved_dir(id, version)
            .join(crate::bundle::MANIFEST_MEMBER)
    }

    /// The detached signature over that manifest.
    pub fn manifest_signature_path(&self, id: &str, version: &str) -> PathBuf {
        self.reserved_dir(id, version)
            .join(crate::bundle::SIGNATURE_MEMBER)
    }

    /// The directory of the version currently published, if any.
    pub fn current_dir(&self, id: &str) -> Option<PathBuf> {
        self.installed_version(id)
            .map(|version| self.version_dir(id, &version))
    }

    /// A fresh staging directory, in the same subvolume as the destination.
    pub fn new_staging_dir(&self) -> Result<PathBuf> {
        let dir = self.staging_root().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).map_err(|e| CoreError::io(&dir, e))?;
        Ok(dir)
    }

    /// The installed version of a module, if any.
    ///
    /// Reads the `current` symlink rather than listing directories, so an
    /// interrupted commit is correctly reported as "not installed".
    pub fn installed_version(&self, id: &str) -> Option<String> {
        let target = std::fs::read_link(self.current_link(id)).ok()?;
        let name = target.file_name()?.to_str()?.to_string();
        // A dangling link means the target was never fully published.
        if self.version_dir(id, &name).is_dir() {
            Some(name)
        } else {
            None
        }
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.installed_version(id).is_some()
    }

    /// Every installed module, sorted by id.
    pub fn installed(&self) -> Result<Vec<(String, String)>> {
        let root = self.modules_root();
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(CoreError::io(&root, e)),
        };

        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| CoreError::io(&root, e))?;
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if let Some(version) = self.installed_version(&id) {
                out.push((id, version));
            }
        }
        out.sort();
        Ok(out)
    }

    /// Delete leftover staging directories.
    ///
    /// Staging leftovers are the expected residue of an interrupted commit:
    /// they are inert, because nothing outside the store ever points at them.
    pub fn clean_staging(&self) -> Result<usize> {
        let root = self.staging_root();
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(CoreError::io(&root, e)),
        };

        let mut removed = 0;
        for entry in entries {
            let entry = entry.map_err(|e| CoreError::io(&root, e))?;
            let path = entry.path();
            if path.is_dir() {
                std::fs::remove_dir_all(&path).map_err(|e| CoreError::io(&path, e))?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Version directories that exist but are not pointed at by `current`.
    ///
    /// Each one is the footprint of a commit interrupted between the directory
    /// rename and the symlink swap. Reporting them is how the fault-injection
    /// tests confirm that an interruption left the store consistent rather than
    /// half-published.
    pub fn orphaned_versions(&self) -> Result<Vec<(String, String)>> {
        let root = self.modules_root();
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(CoreError::io(&root, e)),
        };

        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| CoreError::io(&root, e))?;
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let installed = self.installed_version(&id);
            let module_dir = entry.path();
            let versions = match std::fs::read_dir(&module_dir) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for version_entry in versions.flatten() {
                let Some(name) = version_entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if name == "current" || !version_entry.path().is_dir() {
                    continue;
                }
                if installed.as_deref() != Some(name.as_str()) {
                    out.push((id.clone(), name));
                }
            }
        }
        out.sort();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_is_released_when_it_goes_out_of_scope() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();

        {
            let _held = store.lock().unwrap();
            assert!(
                store.contract_in_progress().unwrap(),
                "a lock that is held has to be visible as held"
            );
        }

        assert!(
            !store.contract_in_progress().unwrap(),
            "the lock outlived the scope that took it"
        );
    }

    #[test]
    fn a_second_process_waits_for_the_first_to_finish_its_contract() {
        // The claim the decree makes and nothing used to implement: one
        // contract at a time. Across *processes*, because that is where the
        // race actually is — two `thalyx` invocations, not two threads.
        //
        // The child is a real process rather than a thread on purpose: a
        // thread would share the open file description and `flock` would let it
        // straight through, which is the mistake that would make this test pass
        // while the property it is named for was absent.
        use std::io::Read;

        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let witness = dir.path().join("witness");

        let held = store.lock().unwrap();

        // A second process that takes the lock and only then writes. If the
        // lock does nothing, it writes immediately.
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("store::tests::the_child_half_of_the_waiting_test")
            .arg("--nocapture")
            .arg("--ignored")
            .env("THALYX_TEST_STORE", dir.path())
            .env("THALYX_TEST_WITNESS", &witness)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the child half");

        // Long enough that a child which ignored the lock would have finished.
        // The measurement is one-sided on purpose: ambient slowness can only
        // make the child *later*, never earlier, so a witness that is absent
        // here is absent because the lock held.
        std::thread::sleep(std::time::Duration::from_millis(750));
        assert!(
            !witness.exists(),
            "the second process wrote while the first held the lock"
        );

        drop(held);

        let status = child.wait().expect("waiting for the child");
        let mut output = String::new();
        if let Some(mut out) = child.stdout.take() {
            let _ = out.read_to_string(&mut output);
        }
        assert!(status.success(), "the child half failed: {output}");
        assert!(
            witness.exists(),
            "the second process never got the lock after it was released"
        );
    }

    /// The other half of the test above. Ignored so it only runs when named.
    #[test]
    #[ignore]
    fn the_child_half_of_the_waiting_test() {
        let Ok(root) = std::env::var("THALYX_TEST_STORE") else {
            return;
        };
        let witness = std::env::var("THALYX_TEST_WITNESS").expect("a witness path");

        let store = Store::open(&root).unwrap();
        let _lock = store.lock().expect("the lock, once the parent lets go");
        std::fs::write(&witness, b"the lock was granted").expect("writing the witness");
    }
}
