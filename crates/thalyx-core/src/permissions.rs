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
}

impl Grant {
    fn from_permission(permission: &Permission, request_id: &str) -> Self {
        Self {
            resource: permission.resource.clone(),
            action: permission.action.clone(),
            kind: permission.kind,
            granted_at: thalyx_journal::now(),
            request_id: request_id.to_string(),
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
    request_id: String,
    permissions: Vec<Permission>,
}

impl PendingGrants {
    pub fn new(module_id: &str, request_id: &str, permissions: Vec<Permission>) -> Self {
        Self {
            module_id: module_id.to_string(),
            request_id: request_id.to_string(),
            permissions,
        }
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn permissions(&self) -> &[Permission] {
        &self.permissions
    }

    pub fn is_empty(&self) -> bool {
        self.permissions.is_empty()
    }
}

impl Registry {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => RegistryFile::default(),
            Err(e) => return Err(CoreError::io(&path, e)),
        };
        Ok(Self { path, file })
    }

    /// Promote pending grants to effective. Called only from inside the commit.
    pub fn make_effective(&mut self, pending: &PendingGrants) -> Result<()> {
        let grants: Vec<Grant> = pending
            .permissions
            .iter()
            .map(|p| Grant::from_permission(p, &pending.request_id))
            .collect();
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
            "req-1",
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
                "req-1",
                vec![permission("net", PermissionKind::Persistent)],
            );
        }
        let registry = Registry::load(&path).unwrap();
        assert!(registry.effective("org.demo.thing").is_empty());
    }

    #[test]
    fn promoted_grants_persist_and_can_be_revoked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.json");
        let mut registry = Registry::load(&path).unwrap();

        let pending = PendingGrants::new(
            "org.demo.thing",
            "req-1",
            vec![permission("net", PermissionKind::Persistent)],
        );
        registry.make_effective(&pending).unwrap();

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
