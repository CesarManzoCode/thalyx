//! Publisher key pinning (trust on first use).
//!
//! The first time a module id is seen, its publisher key is recorded. From then
//! on, that id must always be signed by that key.
//!
//! A key change for a known id is a **hard error**, never a warning: that is
//! exactly what publisher impersonation looks like, and it is adversary 3 in
//! `vault/11-Seguridad/Modelo-de-Amenaza.md`.

use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedKey {
    pub key: String,
    pub first_seen: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct KeystoreFile {
    #[serde(default)]
    keys: BTreeMap<String, PinnedKey>,
}

pub struct Keystore {
    path: PathBuf,
    file: KeystoreFile,
}

impl Keystore {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => KeystoreFile::default(),
            Err(e) => return Err(CoreError::io(&path, e)),
        };
        Ok(Self { path, file })
    }

    pub fn pinned(&self, module_id: &str) -> Option<&PinnedKey> {
        self.file.keys.get(module_id)
    }

    /// Check an offered key against what is pinned, without recording anything.
    ///
    /// Deliberately separate from [`Keystore::pin`]: nothing is recorded until
    /// the installation actually commits, so a refused or failed install cannot
    /// leave a key pinned behind it.
    pub fn check(&self, module_id: &str, offered_key: &str) -> Result<()> {
        match self.file.keys.get(module_id) {
            None => Ok(()),
            Some(pinned) if pinned.key == offered_key => Ok(()),
            Some(pinned) => Err(CoreError::PublisherKeyChanged {
                module_id: module_id.to_string(),
                pinned: pinned.key.clone(),
                offered: offered_key.to_string(),
            }),
        }
    }

    /// Record a key for an id seen for the first time. Idempotent.
    pub fn pin(&mut self, module_id: &str, key: &str) -> Result<()> {
        self.check(module_id, key)?;
        if !self.file.keys.contains_key(module_id) {
            self.file.keys.insert(
                module_id.to_string(),
                PinnedKey {
                    key: key.to_string(),
                    first_seen: thalyx_journal::now(),
                },
            );
            self.save()?;
        }
        Ok(())
    }

    pub fn entries(&self) -> impl Iterator<Item = (&String, &PinnedKey)> {
        self.file.keys.iter()
    }

    fn save(&self) -> Result<()> {
        save_json(&self.path, &self.file)
    }
}

/// Write JSON durably: to a temporary file in the same directory, then rename.
///
/// Same reasoning as the module commit — a half-written state file is worse
/// than no state file, and `rename` within a directory is atomic.
pub(crate) fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    }
    let temporary = path.with_extension("tmp");
    let contents = serde_json::to_string_pretty(value).expect("state is always serialisable");
    std::fs::write(&temporary, contents).map_err(|e| CoreError::io(&temporary, e))?;
    std::fs::rename(&temporary, path).map_err(|e| CoreError::io(path, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: &str = "ed25519:aaaa";
    const KEY_B: &str = "ed25519:bbbb";

    #[test]
    fn first_use_is_trusted_and_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys.json");
        let mut keystore = Keystore::load(&path).unwrap();

        assert!(keystore.check("org.demo.thing", KEY_A).is_ok());
        keystore.pin("org.demo.thing", KEY_A).unwrap();

        let reloaded = Keystore::load(&path).unwrap();
        assert_eq!(reloaded.pinned("org.demo.thing").unwrap().key, KEY_A);
    }

    #[test]
    fn a_changed_key_for_a_known_id_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut keystore = Keystore::load(dir.path().join("keys.json")).unwrap();
        keystore.pin("org.demo.thing", KEY_A).unwrap();

        assert!(matches!(
            keystore.check("org.demo.thing", KEY_B),
            Err(CoreError::PublisherKeyChanged { .. })
        ));
        assert!(matches!(
            keystore.pin("org.demo.thing", KEY_B),
            Err(CoreError::PublisherKeyChanged { .. })
        ));
    }

    #[test]
    fn pinning_the_same_key_again_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let mut keystore = Keystore::load(dir.path().join("keys.json")).unwrap();
        keystore.pin("org.demo.thing", KEY_A).unwrap();
        keystore.pin("org.demo.thing", KEY_A).unwrap();
        assert_eq!(keystore.entries().count(), 1);
    }

    #[test]
    fn different_ids_hold_independent_keys() {
        let dir = tempfile::tempdir().unwrap();
        let mut keystore = Keystore::load(dir.path().join("keys.json")).unwrap();
        keystore.pin("org.demo.one", KEY_A).unwrap();
        keystore.pin("org.demo.two", KEY_B).unwrap();
        assert!(keystore.check("org.demo.two", KEY_B).is_ok());
    }
}
