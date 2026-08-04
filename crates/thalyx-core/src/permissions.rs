//! The permission registry.
//!
//! One rule shapes this module: a permission confirmed by the user is recorded
//! as **pending**, tied to the request that asked for it, and only becomes
//! effective inside the commit. If there is no commit, it is discarded and
//! leaves no trace among the active grants.
//!
//! That closes a real hole in the earlier design, where a persistent grant was
//! issued before verification: a failed installation left a live network
//! permission belonging to a module that was never installed.
//!
//! ## Why a grant carries the version it was granted for
//!
//! The first form of this registry was keyed by module id alone, and the
//! reasoning around it was right about the case it considered and wrong about
//! the one it did not. On a *first* install the record is inert until the
//! `current` symlink swings, because until then no version of that id exists —
//! so writing it before the commit is safe.
//!
//! An **upgrade** is not that case. Version 1 is already current when version
//! 2's grants overwrite the entry, and if the process dies before the symlink
//! swings, version 1 goes on running under permissions the human confirmed for
//! version 2. The registry said "installed" and meant "some version"; the
//! question it needed to answer was "this one".
//!
//! So a grant records its version, and [`crate::effective_permissions`] only
//! honours grants whose version is the one `current` points at. The symlink is
//! still the single atomic point — it now decides which *set* of grants is
//! live, rather than merely whether the one set is.
//!
//! ## Why the session id is here too
//!
//! `vault/03-Primitivas/Permisos-JIT.md` decrees three kinds, and `session`
//! means "until the session ends". A grant of that kind records the session it
//! was made in, and stops being effective when that session is over. Without
//! it the kind existed in the schema and behaved exactly like `persistent`,
//! which is the worst of both: a promise of expiry that nothing performs.
//!
//! See `vault/03-Primitivas/Permisos-JIT.md`.

use crate::keystore::save_json;
use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use thalyx_manifest::{Permission, PermissionKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub resource: String,
    pub action: String,
    #[serde(rename = "type")]
    pub kind: PermissionKind,
    pub granted_at: String,
    pub request_id: String,
    /// The version this was confirmed for.
    ///
    /// Not optional in anything Thalyx writes. It is `Option` only so that a
    /// registry written before this field existed still parses — and such a
    /// grant is treated as belonging to no version, which means it is inert.
    /// That is the cautious direction: an old record is ignored rather than
    /// applied to whatever happens to be installed now.
    #[serde(default)]
    pub version: Option<String>,
    /// The session this was confirmed in, for `session` grants only.
    ///
    /// `None` on every other kind, because they do not end with the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

impl Grant {
    fn from_permission(
        permission: &Permission,
        request_id: &str,
        version: &str,
        session: &str,
    ) -> Self {
        Self {
            resource: permission.resource.clone(),
            action: permission.action.clone(),
            kind: permission.kind,
            granted_at: thalyx_journal::now(),
            request_id: request_id.to_string(),
            version: Some(version.to_string()),
            session: match permission.kind {
                PermissionKind::Session => Some(session.to_string()),
                _ => None,
            },
        }
    }

    /// Whether this grant is in force for a module at `version`, in `session`.
    ///
    /// Fail-closed on every unknown: a grant with no version recorded belongs
    /// to no version, and a `session` grant with no session recorded belongs
    /// to no session. Neither is ever treated as "applies to whatever is here
    /// now" — that reading is what turns a stale record into a live permission.
    pub fn in_force(&self, version: &str, session: &str) -> bool {
        if self.version.as_deref() != Some(version) {
            return false;
        }
        match self.kind {
            PermissionKind::Session => self.session.as_deref() == Some(session),
            _ => true,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    granted: BTreeMap<String, Vec<Grant>>,
}

pub struct Registry {
    path: PathBuf,
    file: RegistryFile,
}

/// Permissions confirmed by the user but not yet effective.
///
/// Held in memory only. Dropping this without calling
/// [`Registry::make_effective`] is exactly what should happen when an
/// installation fails: nothing was ever written.
#[derive(Debug, Clone)]
pub struct PendingGrants {
    module_id: String,
    /// The version these were confirmed for. An upgrade's grants belong to the
    /// version being installed, never to the one still current.
    version: String,
    request_id: String,
    session: String,
    permissions: Vec<Permission>,
}

impl PendingGrants {
    pub fn new(
        module_id: &str,
        version: &str,
        request_id: &str,
        session: &str,
        permissions: Vec<Permission>,
    ) -> Self {
        Self {
            module_id: module_id.to_string(),
            version: version.to_string(),
            request_id: request_id.to_string(),
            session: session.to_string(),
            permissions,
        }
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn permissions(&self) -> &[Permission] {
        &self.permissions
    }

    pub fn is_empty(&self) -> bool {
        self.permissions.is_empty()
    }
}

impl Registry {
    /// Load the registry, refusing to guess at a file it cannot read.
    ///
    /// A corrupt registry used to parse as an empty one. That direction is
    /// nearly safe — no grants means no permissions — but "nearly" is doing
    /// too much work: it silently discards every record of what the human
    /// authorised, and the module keeps running with the kernel policy already
    /// written for it. The honest answer is to stop and say the state is
    /// unreadable, which is rule 10 in `Estrategia-de-Pruebas.md`: a failure
    /// to read is not a failure to exist.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = match std::fs::read_to_string(&path) {
            Ok(contents) => {
                serde_json::from_str(&contents).map_err(|source| CoreError::StateUnreadable {
                    path: path.clone(),
                    reason: source.to_string(),
                })?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => RegistryFile::default(),
            Err(e) => return Err(CoreError::io(&path, e)),
        };
        Ok(Self { path, file })
    }

    /// Promote pending grants to effective. Called only from inside the commit.
    ///
    /// `still_current` is the version installed right now, if any. Its grants
    /// are **kept alongside** the new ones rather than replaced, and that is
    /// the difference between an interrupted upgrade being safe and being
    /// merely different.
    ///
    /// Replacing outright would mean that a process killed between this write
    /// and the symlink swap left the still-running previous version holding
    /// nothing at all. That direction is safe — no permission is better than
    /// the wrong permission — but it is not *correct*: a module the human
    /// authorised would silently stop being able to do its work, and the only
    /// symptom would be the module failing somewhere unrelated. Keeping both
    /// sets means whichever version `current` names has exactly the grants
    /// confirmed for it, before the swap and after it.
    ///
    /// Nothing else is kept. Two versions is the most that can be live across
    /// a single commit, so the registry cannot grow without bound.
    pub fn make_effective(
        &mut self,
        pending: &PendingGrants,
        still_current: Option<&str>,
    ) -> Result<()> {
        let mut grants: Vec<Grant> = self
            .file
            .granted
            .get(&pending.module_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .filter(|grant| {
                // The version still on disk, and only it. A grant for the
                // version being installed is about to be rewritten, and a
                // grant for any older version is already inert.
                grant.version.as_deref() == still_current
                    && grant.version.as_deref() != Some(pending.version.as_str())
            })
            .cloned()
            .collect();

        grants.extend(pending.permissions.iter().map(|p| {
            Grant::from_permission(p, &pending.request_id, &pending.version, &pending.session)
        }));

        self.file.granted.insert(pending.module_id.clone(), grants);
        self.save()
    }

    pub fn revoke_all(&mut self, module_id: &str) -> Result<()> {
        self.file.granted.remove(module_id);
        self.save()
    }

    pub fn effective(&self, module_id: &str) -> &[Grant] {
        self.file
            .granted
            .get(module_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn all(&self) -> impl Iterator<Item = (&String, &Vec<Grant>)> {
        self.file.granted.iter()
    }

    fn save(&self) -> Result<()> {
        save_json(&self.path, &self.file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permission(resource: &str, kind: PermissionKind) -> Permission {
        Permission {
            resource: resource.to_string(),
            action: "read".to_string(),
            kind,
        }
    }

    #[test]
    fn pending_grants_are_not_effective_until_promoted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.json");
        let registry = Registry::load(&path).unwrap();

        let pending = PendingGrants::new(
            "org.demo.thing",
            "1.0.0",
            "req-1",
            "session-a",
            vec![permission("/home/user", PermissionKind::Persistent)],
        );

        // The pending set exists, but nothing is granted and nothing is on disk.
        assert_eq!(pending.permissions().len(), 1);
        assert!(registry.effective("org.demo.thing").is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn dropping_pending_grants_leaves_no_trace() {
        // This is the failed-installation path: verification fails, the pending
        // grants are dropped, and no live permission remains for a module that
        // was never installed.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.json");
        {
            let _pending = PendingGrants::new(
                "org.demo.thing",
                "1.0.0",
                "req-1",
                "session-a",
                vec![permission("net", PermissionKind::Persistent)],
            );
        }
        let registry = Registry::load(&path).unwrap();
        assert!(registry.effective("org.demo.thing").is_empty());
    }

    #[test]
    fn a_grant_is_in_force_only_for_the_version_it_was_confirmed_for() {
        let persistent = Grant {
            resource: "net".to_string(),
            action: "outbound".to_string(),
            kind: PermissionKind::Persistent,
            granted_at: "2026-08-04T00:00:00Z".to_string(),
            request_id: "req-1".to_string(),
            version: Some("2.0.0".to_string()),
            session: None,
        };

        assert!(persistent.in_force("2.0.0", "any-session"));
        assert!(
            !persistent.in_force("1.0.0", "any-session"),
            "a grant confirmed for 2.0.0 was honoured for the version still running"
        );
    }

    #[test]
    fn a_grant_from_a_registry_written_before_versions_were_recorded_holds_nothing() {
        // Fail-closed on the unknown. An old record has no version, and the
        // tempting reading — "it must mean whatever is installed" — is exactly
        // what turns a stale record into a live permission.
        let old = Grant {
            resource: "net".to_string(),
            action: "outbound".to_string(),
            kind: PermissionKind::Persistent,
            granted_at: "2026-08-01T00:00:00Z".to_string(),
            request_id: "req-old".to_string(),
            version: None,
            session: None,
        };

        assert!(!old.in_force("1.0.0", "s"));
        assert!(!old.in_force("2.0.0", "s"));
    }

    #[test]
    fn a_session_grant_stops_being_in_force_when_the_session_changes() {
        // The decree says `session` means "until the session ends" and nothing
        // used to perform it: the kind existed in the schema, in the manifest
        // and in the prompt, and behaved exactly like `persistent`.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.json");
        let mut registry = Registry::load(&path).unwrap();

        let pending = PendingGrants::new(
            "org.demo.thing",
            "1.0.0",
            "req-1",
            "session-one",
            vec![permission("/home/user", PermissionKind::Session)],
        );
        registry.make_effective(&pending, None).unwrap();

        let grant = &registry.effective("org.demo.thing")[0];
        assert!(grant.in_force("1.0.0", "session-one"));
        assert!(
            !grant.in_force("1.0.0", "session-two"),
            "a session grant outlived the session it was made in"
        );
    }

    #[test]
    fn a_persistent_grant_does_not_end_with_the_session() {
        // The control. A registry that expired everything at the end of a
        // session would pass the test above and would silently revoke what the
        // human confirmed was permanent.
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::load(dir.path().join("permissions.json")).unwrap();

        let pending = PendingGrants::new(
            "org.demo.thing",
            "1.0.0",
            "req-1",
            "session-one",
            vec![permission("/home/user", PermissionKind::Persistent)],
        );
        registry.make_effective(&pending, None).unwrap();

        let grant = &registry.effective("org.demo.thing")[0];
        assert!(grant.in_force("1.0.0", "session-two"));
        assert_eq!(grant.session, None, "a persistent grant records no session");
    }

    #[test]
    fn an_upgrade_keeps_the_running_version_s_grants_until_the_commit() {
        // Written before the commit, so between this call and the symlink swap
        // the previous version is still what runs — and it has to keep exactly
        // what it was confirmed for. Dropping them would be safe and wrong:
        // a module the human authorised would quietly stop working, and the
        // only symptom would surface somewhere unrelated.
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::load(dir.path().join("permissions.json")).unwrap();

        registry
            .make_effective(
                &PendingGrants::new(
                    "org.demo.thing",
                    "1.0.0",
                    "req-1",
                    "s",
                    vec![permission("/home/user", PermissionKind::Persistent)],
                ),
                None,
            )
            .unwrap();

        registry
            .make_effective(
                &PendingGrants::new(
                    "org.demo.thing",
                    "2.0.0",
                    "req-2",
                    "s",
                    vec![
                        permission("/home/user", PermissionKind::Persistent),
                        permission("net", PermissionKind::Persistent),
                    ],
                ),
                Some("1.0.0"),
            )
            .unwrap();

        let grants = registry.effective("org.demo.thing");
        let live_for_v1: Vec<_> = grants.iter().filter(|g| g.in_force("1.0.0", "s")).collect();
        let live_for_v2: Vec<_> = grants.iter().filter(|g| g.in_force("2.0.0", "s")).collect();

        assert_eq!(live_for_v1.len(), 1, "the running version lost its grants");
        assert_eq!(
            live_for_v2.len(),
            2,
            "the new version's grants were not recorded"
        );
    }

    #[test]
    fn promoted_grants_persist_and_can_be_revoked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.json");
        let mut registry = Registry::load(&path).unwrap();

        let pending = PendingGrants::new(
            "org.demo.thing",
            "1.0.0",
            "req-1",
            "session-a",
            vec![permission("net", PermissionKind::Persistent)],
        );
        registry.make_effective(&pending, None).unwrap();

        let reloaded = Registry::load(&path).unwrap();
        assert_eq!(reloaded.effective("org.demo.thing").len(), 1);
        assert_eq!(reloaded.effective("org.demo.thing")[0].request_id, "req-1");

        let mut reloaded = reloaded;
        reloaded.revoke_all("org.demo.thing").unwrap();
        assert!(
            Registry::load(&path)
                .unwrap()
                .effective("org.demo.thing")
                .is_empty()
        );
    }
}
