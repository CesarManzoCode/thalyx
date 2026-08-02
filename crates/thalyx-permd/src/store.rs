//! Where policy is written.
//!
//! A trait rather than a concrete type, for two reasons. The policy logic can
//! be tested without a kernel, which is most of what this crate is. And the
//! mechanism that talks to the map is replaceable without touching any of it —
//! the same split as the parser and the graph: a stable contract in front of
//! an engine that can be swapped.

use crate::Policy;
use crate::encoding::{as_hex_args, cgroup_key_bytes, policy_bytes};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not run bpftool: {0}")]
    Spawn(#[source] std::io::Error),

    #[error("bpftool failed: {0}")]
    Bpftool(String),

    #[error("the policy map is not pinned at {0}; is thalyx-lsm loaded?")]
    NotPinned(PathBuf),
}

pub trait PolicyStore {
    fn set(&self, cgroup: u64, policy: Policy) -> Result<(), StoreError>;
    fn remove(&self, cgroup: u64) -> Result<(), StoreError>;
    fn get(&self, cgroup: u64) -> Result<Option<Policy>, StoreError>;
}

/// Writes policy through `bpftool`.
///
/// Shelling out rather than linking libbpf is a deliberate Phase 1 choice.
/// It has no build-time dependency on kernel headers, works identically to
/// what a person would type by hand while debugging, and every write can be
/// checked with `bpftool map dump` outside the program. A libbpf backend
/// replaces this later without the policy logic noticing.
pub struct BpftoolStore {
    map: PathBuf,
    bpftool: PathBuf,
    /// Writing to bpffs needs root. When Thalyx runs as a service this is
    /// false, because the service already has the privilege.
    use_sudo: bool,
}

impl BpftoolStore {
    pub const DEFAULT_MAP: &'static str = "/sys/fs/bpf/thalyx/maps/thalyx_policy";

    pub fn new(map: impl Into<PathBuf>) -> Self {
        Self {
            map: map.into(),
            bpftool: PathBuf::from("bpftool"),
            use_sudo: !running_as_root(),
        }
    }

    pub fn default_map() -> Self {
        Self::new(Self::DEFAULT_MAP)
    }

    pub fn with_bpftool(mut self, path: impl Into<PathBuf>) -> Self {
        self.bpftool = path.into();
        self
    }

    /// Whether the map the kernel side pinned is actually there.
    ///
    /// Checked through the same privilege the writes use: bpffs is mode 700,
    /// so an unprivileged existence check reports "missing" for a map that is
    /// present — the same mistake that once made the tooling read as disarmed
    /// while it was armed.
    pub fn is_available(&self) -> bool {
        self.command(&["map", "show", "pinned", &self.map.to_string_lossy()])
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn command(&self, args: &[&str]) -> Result<std::process::Output, StoreError> {
        let mut command = if self.use_sudo {
            let mut c = std::process::Command::new("sudo");
            c.arg(&self.bpftool);
            c
        } else {
            std::process::Command::new(&self.bpftool)
        };
        command.args(args);
        command.output().map_err(StoreError::Spawn)
    }

    fn run(&self, args: &[&str]) -> Result<String, StoreError> {
        let output = self.command(args)?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if message.contains("No such file") {
                return Err(StoreError::NotPinned(self.map.clone()));
            }
            return Err(StoreError::Bpftool(message));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Effective uid, read from procfs rather than through libc.
///
/// Defaults to "not root" if it cannot be determined: assuming the privilege
/// is there and being wrong means every policy write fails at the point of
/// use, while assuming it is absent costs one `sudo` that turns into a no-op.
fn running_as_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("Uid:"))
                .and_then(|line| line.split_whitespace().nth(2))
                .and_then(|uid| uid.parse::<u32>().ok())
        })
        .is_some_and(|uid| uid == 0)
}

impl PolicyStore for BpftoolStore {
    fn set(&self, cgroup: u64, policy: Policy) -> Result<(), StoreError> {
        let map = self.map.to_string_lossy().into_owned();
        let key = as_hex_args(&cgroup_key_bytes(cgroup));
        let value = as_hex_args(&policy_bytes(&policy));

        let mut args: Vec<&str> = vec!["map", "update", "pinned", &map, "key", "hex"];
        args.extend(key.iter().map(String::as_str));
        args.push("value");
        args.push("hex");
        args.extend(value.iter().map(String::as_str));

        self.run(&args)?;
        Ok(())
    }

    fn remove(&self, cgroup: u64) -> Result<(), StoreError> {
        let map = self.map.to_string_lossy().into_owned();
        let key = as_hex_args(&cgroup_key_bytes(cgroup));

        let mut args: Vec<&str> = vec!["map", "delete", "pinned", &map, "key", "hex"];
        args.extend(key.iter().map(String::as_str));

        match self.run(&args) {
            Ok(_) => Ok(()),
            // Removing a grant that is not there is the desired end state, not
            // a failure. Revocation has to be idempotent: it is what runs on
            // the recovery paths, where the state is by definition unknown.
            Err(StoreError::Bpftool(message)) if message.contains("No such") => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn get(&self, _cgroup: u64) -> Result<Option<Policy>, StoreError> {
        // Reading back a single entry is only needed for diagnostics, and
        // `bpftool map dump` already serves that outside the program. Left
        // unimplemented rather than half-implemented.
        Ok(None)
    }
}

/// An in-memory store, for tests.
#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<std::collections::BTreeMap<u64, Policy>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().expect("not poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl PolicyStore for MemoryStore {
    fn set(&self, cgroup: u64, policy: Policy) -> Result<(), StoreError> {
        self.entries
            .lock()
            .expect("not poisoned")
            .insert(cgroup, policy);
        Ok(())
    }

    fn remove(&self, cgroup: u64) -> Result<(), StoreError> {
        self.entries.lock().expect("not poisoned").remove(&cgroup);
        Ok(())
    }

    fn get(&self, cgroup: u64) -> Result<Option<Policy>, StoreError> {
        Ok(self
            .entries
            .lock()
            .expect("not poisoned")
            .get(&cgroup)
            .copied())
    }
}

/// Where the LSM pins its policy map, for callers that want to check first.
pub fn default_map_path() -> &'static Path {
    Path::new(BpftoolStore::DEFAULT_MAP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FS_READ, NET_OUTBOUND};

    #[test]
    fn a_policy_can_be_written_read_and_withdrawn() {
        let store = MemoryStore::new();
        let policy = Policy {
            allowed: NET_OUTBOUND | FS_READ,
            flags: 0,
            expires_ns: 0,
        };

        store.set(42, policy).unwrap();
        assert_eq!(store.get(42).unwrap(), Some(policy));

        store.remove(42).unwrap();
        assert_eq!(store.get(42).unwrap(), None);
        assert!(store.is_empty());
    }

    #[test]
    fn revoking_something_that_is_not_there_succeeds() {
        // Revocation runs on recovery paths, where the state is unknown by
        // definition. If it could fail for an absent grant, every caller would
        // have to check first — and the check would race with the removal.
        let store = MemoryStore::new();
        assert!(store.remove(999).is_ok());
        assert!(store.remove(999).is_ok());
    }

    #[test]
    fn policies_for_different_cgroups_are_independent() {
        let store = MemoryStore::new();
        store
            .set(
                1,
                Policy {
                    allowed: NET_OUTBOUND,
                    ..Default::default()
                },
            )
            .unwrap();
        store
            .set(
                2,
                Policy {
                    allowed: FS_READ,
                    ..Default::default()
                },
            )
            .unwrap();

        assert!(store.get(1).unwrap().unwrap().allows(NET_OUTBOUND));
        assert!(!store.get(2).unwrap().unwrap().allows(NET_OUTBOUND));
    }

    #[test]
    fn the_bpftool_store_names_the_map_the_lsm_pins() {
        // If these drift apart, permd writes policy nowhere and every module
        // silently runs unconstrained — the failure mode with no symptom.
        let makefile = include_str!("../../../lsm/Makefile");
        assert!(
            makefile.contains("MAPDIR  := $(PINDIR)/maps"),
            "the LSM Makefile no longer pins maps where BpftoolStore looks"
        );
        assert!(BpftoolStore::DEFAULT_MAP.starts_with("/sys/fs/bpf/thalyx/maps/"));
        assert!(BpftoolStore::DEFAULT_MAP.ends_with("thalyx_policy"));
    }
}
