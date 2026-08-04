//! Which user a module runs as.
//!
//! `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md` decrees **one uid per
//! module**: modules are isolated from each other, not only from the system.
//! The alternative — one shared unprivileged user for all of them — was
//! considered and rejected, because the human confirmed each module
//! separately and two modules sharing a uid can read each other's files.
//!
//! ## Nothing is ever reused
//!
//! A uid freed by uninstalling a module is retired, not recycled. A module
//! leaves files behind in places Thalyx does not track — a granted directory,
//! a temporary file that outlived it — and those files stay owned by the
//! number, not by the module. Handing that number to a different module later
//! would silently give it everything the previous one left.
//!
//! So allocation is monotonic and the high-water mark is persisted. Uninstall
//! removes the *assignment*, never lowers the counter.

use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The first uid Thalyx hands out.
///
/// Above the ranges anything else is likely to be using: system accounts,
/// ordinary users, systemd's dynamic users (61184–65519), and the subordinate
/// ranges `/etc/subuid` conventionally starts at 100000 and extends by 65536
/// per user. Starting here means a Thalyx uid is recognisable on sight and
/// collides with nothing.
pub const FIRST_UID: u32 = 700_000;

/// The last one. Well below the 32-bit boundary and the reserved `nobody`
/// values, so an overflow cannot quietly become a meaningful id.
pub const LAST_UID: u32 = 1_700_000;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    /// The next uid to hand out. Never decreases.
    #[serde(default)]
    next: u32,
    /// Module id to uid.
    #[serde(default)]
    assigned: BTreeMap<String, u32>,
    /// Uids that were assigned once and will never be handed out again.
    ///
    /// Kept for the record rather than for the logic — `next` alone prevents
    /// reuse. Someone auditing a stray file owned by 700003 should be able to
    /// find out which module that was.
    #[serde(default)]
    retired: BTreeMap<String, u32>,
}

/// The persistent map from module to the user it runs as.
pub struct UidRegistry {
    path: PathBuf,
    stored: Stored,
}

impl UidRegistry {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let stored = match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map_err(|e| CoreError::io(&path, std::io::Error::other(e)))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Stored {
                next: FIRST_UID,
                ..Default::default()
            },
            Err(e) => return Err(CoreError::io(&path, e)),
        };

        Ok(Self { path, stored })
    }

    /// The uid a module runs as, allocating one the first time.
    pub fn assign(&mut self, module_id: &str) -> Result<u32> {
        if let Some(uid) = self.stored.assigned.get(module_id) {
            return Ok(*uid);
        }

        // A registry written before this field existed, or one hand-edited to
        // zero, must not start handing out uid 0.
        let next = self.stored.next.max(FIRST_UID);
        if next > LAST_UID {
            return Err(CoreError::UidRangeExhausted {
                first: FIRST_UID,
                last: LAST_UID,
            });
        }

        self.stored.next = next + 1;
        self.stored.assigned.insert(module_id.to_string(), next);
        self.save()?;
        Ok(next)
    }

    /// The uid a module was assigned, without allocating one.
    pub fn assigned(&self, module_id: &str) -> Option<u32> {
        self.stored.assigned.get(module_id).copied()
    }

    /// Give up a module's assignment. The uid itself is never handed out again.
    pub fn retire(&mut self, module_id: &str) -> Result<()> {
        if let Some(uid) = self.stored.assigned.remove(module_id) {
            self.stored.retired.insert(module_id.to_string(), uid);
            self.save()?;
        }
        Ok(())
    }

    /// Every current assignment, for display.
    pub fn all(&self) -> impl Iterator<Item = (&String, &u32)> {
        self.stored.assigned.iter()
    }

    /// Write the registry through the same atomic, durable path as every other
    /// piece of state.
    ///
    /// This used to be a plain `write` straight over the live file, which is
    /// the one place in the store where a crash could destroy state rather
    /// than merely fail to add to it. It matters more here than anywhere else:
    /// [`UidRegistry::load`] refuses to parse a damaged file — correctly, since
    /// guessing at a uid map is how a module inherits another module's files —
    /// so a truncated write did not fail open, it bricked the machine. Every
    /// install and every confined run would refuse from then on.
    fn save(&self) -> Result<()> {
        crate::keystore::save_json(&self.path, &self.stored)
    }
}

/// Whether a uid could use a path directly, without any remapping.
///
/// Kept for diagnosis rather than for the launch path: granted paths are bound
/// through an idmapped mount, which is what makes a grant on somebody else's
/// directory work at all. This answers the different question of whether the
/// module could have reached it unaided — useful when explaining why a bind
/// had to be remapped.
///
/// The module has no supplementary groups and its group is its own, so only
/// the owner and other bits can apply.
pub fn usable_by(path: &Path, uid: u32, writing: bool) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(path).map_err(|e| CoreError::io(path, e))?;
    let mode = metadata.permissions().mode();

    let (owner_bit, other_bit) = if writing {
        (0o200, 0o002)
    } else {
        (0o400, 0o004)
    };

    if metadata.uid() == uid {
        return Ok(mode & owner_bit != 0);
    }
    Ok(mode & other_bit != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> (tempfile::TempDir, UidRegistry) {
        let dir = tempfile::tempdir().unwrap();
        let registry = UidRegistry::load(dir.path().join("uids.json")).unwrap();
        (dir, registry)
    }

    #[test]
    fn every_module_gets_a_different_user() {
        let (_dir, mut registry) = registry();

        let first = registry.assign("org.thalyx.a").unwrap();
        let second = registry.assign("org.thalyx.b").unwrap();

        assert_ne!(first, second);
        assert!(first >= FIRST_UID);
        assert!(second >= FIRST_UID);
    }

    #[test]
    fn a_module_keeps_the_same_user_forever() {
        // A module whose uid changed between runs would lose access to
        // everything it had written itself.
        let (dir, mut registry) = registry();
        let first = registry.assign("org.thalyx.demo").unwrap();

        assert_eq!(registry.assign("org.thalyx.demo").unwrap(), first);

        // And across processes.
        let reopened = UidRegistry::load(dir.path().join("uids.json")).unwrap();
        assert_eq!(reopened.assigned("org.thalyx.demo"), Some(first));
    }

    #[test]
    fn a_retired_uid_is_never_handed_out_again() {
        // The module left files behind that are owned by the number, not by
        // the module. Recycling it would give the next module everything the
        // last one dropped.
        let (_dir, mut registry) = registry();

        let retired = registry.assign("org.thalyx.gone").unwrap();
        registry.retire("org.thalyx.gone").unwrap();
        assert_eq!(registry.assigned("org.thalyx.gone"), None);

        for name in ["org.thalyx.a", "org.thalyx.b", "org.thalyx.c"] {
            assert_ne!(
                registry.assign(name).unwrap(),
                retired,
                "a retired uid came back"
            );
        }
    }

    #[test]
    fn reinstalling_a_module_after_retiring_it_gives_it_a_new_user() {
        let (_dir, mut registry) = registry();

        let before = registry.assign("org.thalyx.demo").unwrap();
        registry.retire("org.thalyx.demo").unwrap();
        let after = registry.assign("org.thalyx.demo").unwrap();

        assert_ne!(
            before, after,
            "the reinstalled module must not inherit what the old one left"
        );
    }

    #[test]
    fn a_registry_written_without_a_counter_does_not_start_at_zero() {
        // Zero is root. A missing or hand-cleared field must not become the
        // most privileged user on the machine.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("uids.json");
        std::fs::write(&path, "{}").unwrap();

        let mut registry = UidRegistry::load(&path).unwrap();
        let uid = registry.assign("org.thalyx.demo").unwrap();

        assert_eq!(uid, FIRST_UID);
    }

    #[test]
    fn running_out_of_uids_is_an_error_rather_than_a_wraparound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("uids.json");
        std::fs::write(&path, format!(r#"{{"next": {}}}"#, LAST_UID + 1)).unwrap();

        let mut registry = UidRegistry::load(&path).unwrap();
        assert!(matches!(
            registry.assign("org.thalyx.demo"),
            Err(CoreError::UidRangeExhausted { .. })
        ));
    }

    #[test]
    fn a_path_the_module_cannot_read_is_reported_as_unusable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();

        // Owned by whoever runs the test, and readable only by them.
        assert!(!usable_by(&path, FIRST_UID, false).unwrap());
    }

    #[test]
    fn a_world_readable_path_is_usable_for_reading_and_not_for_writing() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(usable_by(&path, FIRST_UID, false).unwrap());
        assert!(
            !usable_by(&path, FIRST_UID, true).unwrap(),
            "a module must not be told it can write somewhere it cannot"
        );
    }

    #[test]
    fn a_world_writable_path_is_usable_for_writing() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("open");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777)).unwrap();

        assert!(usable_by(&path, FIRST_UID, true).unwrap());
    }
}
