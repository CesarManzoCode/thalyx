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
    /// Load the pinned keys, or refuse to proceed without them.
    ///
    /// ## The bug this signature exists to state
    ///
    /// This used to be `unwrap_or_default()`. A `keys.json` that could not be
    /// parsed — truncated by a power loss mid-write, or corrupted on purpose —
    /// became an **empty keystore**, and an empty keystore trusts everything:
    /// [`Keystore::check`] answers `Ok` for an id it has never seen, because
    /// that is what trust on first use means.
    ///
    /// So corrupting one file downgraded every pinned publisher back to a
    /// first sighting, and the next bundle offered for any installed id,
    /// signed by anybody, would have been accepted. That is adversary 3 in
    /// `vault/11-Seguridad/Modelo-de-Amenaza.md` reached by damaging a file
    /// rather than by breaking any cryptography.
    ///
    /// A trust store that cannot be read is not an empty trust store. It is an
    /// unknown one, and the only safe answer is to stop.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = match std::fs::read_to_string(&path) {
            Ok(contents) => {
                serde_json::from_str(&contents).map_err(|source| CoreError::StateUnreadable {
                    path: path.clone(),
                    reason: source.to_string(),
                })?
            }
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
    let contents = serde_json::to_string_pretty(value).expect("state is always serialisable");
    write_durably(path, contents.as_bytes())
}

/// Replace a file's contents atomically **and** durably.
///
/// The atomic half was already here and the durable half was not, and the
/// difference is the difference between two failures:
///
/// - Without the rename, a crash mid-write leaves a file that is half of one
///   state and half of another. Nothing can read it.
/// - Without the fsyncs, the rename is atomic with respect to other *readers*
///   and not with respect to *power*. The rename can reach the disk before the
///   bytes it published do, so a power loss can leave the new name pointing at
///   a file the filesystem never finished writing — or lose the write outright
///   while every call that made it reported success.
///
/// Both matter here because the caller is the keystore, and the keystore
/// failing to load is now a hard error by design. Losing it to a power cut
/// would turn a durability gap into a machine that refuses to install
/// anything.
///
/// The unique temporary name is not tidiness either: a fixed `.tmp` is a
/// second name two writers can collide on, and one of them would rename a file
/// the other was still filling.
pub(crate) fn write_durably(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;

    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;

    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "state".to_string()),
        uuid::Uuid::new_v4()
    ));

    {
        let mut file =
            std::fs::File::create(&temporary).map_err(|e| CoreError::io(&temporary, e))?;
        file.write_all(contents)
            .map_err(|e| CoreError::io(&temporary, e))?;
        // The bytes, before the name that will point at them.
        file.sync_all().map_err(|e| CoreError::io(&temporary, e))?;
    }

    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(CoreError::io(path, error));
    }

    // And the directory entry, so the rename itself survives.
    let dir = std::fs::File::open(parent).map_err(|e| CoreError::io(parent, e))?;
    dir.sync_all().map_err(|e| CoreError::io(parent, e))?;

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
    fn a_corrupt_keystore_is_refused_rather_than_read_as_an_empty_one() {
        // The sharpest fail-open this codebase had.
        //
        // `unwrap_or_default()` turned an unparseable `keys.json` into an empty
        // keystore, and an empty keystore trusts everything it is offered —
        // that is what trust-on-first-use means. So damaging one file
        // downgraded every pinned publisher back to a first sighting, and the
        // next bundle offered for any installed id, signed by anybody, would
        // have been accepted.
        //
        // No cryptography had to break. A truncated write would do it.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys.json");

        let mut keystore = Keystore::load(&path).unwrap();
        keystore.pin("org.demo.thing", KEY_A).unwrap();

        // What a power loss mid-write leaves behind.
        let good = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, &good[..good.len() / 2]).unwrap();

        match Keystore::load(&path) {
            Err(CoreError::StateUnreadable { .. }) => {}
            Err(other) => panic!("expected a refusal to read, got {other:?}"),
            Ok(keystore) => panic!(
                "a corrupt keystore loaded as one holding {} key(s); the next \
                 bundle for any installed id would be a first sighting",
                keystore.entries().count()
            ),
        }
    }

    #[test]
    fn a_keystore_that_was_never_written_is_still_an_empty_one() {
        // The control, and the distinction the whole change rests on. Absent
        // means nothing was ever pinned, and trusting the first key for an id
        // is correct — it is the definition of the policy. Unreadable means
        // something *was* pinned and nobody knows what. Collapsing the two is
        // the bug; refusing both would make a fresh machine unable to install
        // anything at all.
        let dir = tempfile::tempdir().unwrap();
        let keystore = Keystore::load(dir.path().join("never-written.json"))
            .expect("a store that does not exist yet is not an error");
        assert_eq!(keystore.entries().count(), 0);
        assert!(keystore.check("org.demo.thing", KEY_A).is_ok());
    }

    #[test]
    fn a_state_file_is_replaced_whole_or_not_at_all() {
        // `write_durably` must never leave the destination holding a mixture of
        // two states, and must not leave its temporary behind either — a fixed
        // `.tmp` name was a second file two writers could collide on.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        write_durably(&path, b"{\"first\": true}").unwrap();
        write_durably(&path, b"{\"second\": true}").unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"second\": true}"
        );

        let strays: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "state.json")
            .collect();
        assert!(
            strays.is_empty(),
            "temporary files were left behind: {strays:?}"
        );
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
