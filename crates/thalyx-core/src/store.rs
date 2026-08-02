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
//!   journal.jsonl
//! ```
//!
//! See `vault/04-Flujo-Canonico/Fase-Commit-Atomico.md`.

use crate::{CoreError, Result};
use std::path::{Path, PathBuf};

pub struct Store {
    root: PathBuf,
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

    pub fn journal_path(&self) -> PathBuf {
        self.root.join("journal.jsonl")
    }

    pub fn keystore_path(&self) -> PathBuf {
        self.state_root().join("keys.json")
    }

    pub fn permissions_path(&self) -> PathBuf {
        self.state_root().join("permissions.json")
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
