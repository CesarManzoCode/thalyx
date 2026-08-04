//! Where policy is written.
//!
//! A trait rather than a concrete type, for two reasons. The policy logic can
//! be tested without a kernel, which is most of what this crate is. And the
//! mechanism that talks to the map is replaceable without touching any of it —
//! the same split as the parser and the graph: a stable contract in front of
//! an engine that can be swapped.

use crate::Policy;
use crate::encoding::{POLICY_BYTES, cgroup_key_bytes, policy_bytes, policy_from_bytes};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("the policy map is not pinned at {0}; is thalyx-lsm loaded?")]
    NotPinned(PathBuf),

    #[error("{what}: {source}")]
    Kernel {
        what: &'static str,
        #[source]
        source: std::io::Error,
    },
}

pub trait PolicyStore {
    fn set(&self, cgroup: u64, policy: Policy) -> Result<(), StoreError>;
    fn remove(&self, cgroup: u64) -> Result<(), StoreError>;
    fn get(&self, cgroup: u64) -> Result<Option<Policy>, StoreError>;

    /// Whether writing here would actually reach something that enforces.
    ///
    /// Callers about to run module code have to ask, because the answer
    /// decides between confining it and refusing to start it. Defaults to
    /// `true` for stores that are always writable; [`KernelStore`] overrides
    /// it, since its map only exists while the kernel side is loaded.
    fn is_available(&self) -> bool {
        true
    }
}

/// Writes policy with `bpf(2)`, through the pin the loader left.
///
/// ## Why this replaced shelling out to bpftool
///
/// `BpftoolStore` needs two things the image does not have and cannot have: a
/// second program, and a shell to invoke it from. So inside the machine every
/// answer it gave was the same answer — the map is not there — no matter what
/// the kernel actually held.
///
/// That was not a cosmetic wrongness. `is_available()` is what decides between
/// confining a module and refusing to start it, so on 2026-08-04 a machine
/// that had attached its own enforcement, with both hooks live and all three
/// maps pinned, refused to run a module confined and offered `sin-confinar` as
/// the way out. Enforcement was real and the only thing that could not see it
/// was the code deciding whether to use it.
///
/// It is the third time this project has asked bpftool a question about
/// something bpftool did not do, and the vault has a rule about it. The fix is
/// the same each time: ask the kernel.
///
/// ## What went away with bpftool, and is not missed
///
/// `BpftoolStore` prefixed its writes with `sudo` when Thalyx was not root.
/// That is gone rather than reimplemented: a process that cannot open the map
/// cannot write policy, and quietly acquiring the privilege to do it anyway is
/// not something the thing that enforces permissions should be doing on its
/// own. Confining a module needs namespaces and cgroups regardless, so the
/// caller was already privileged or already failing.
///
/// ## Why the descriptor is opened per operation
///
/// A held descriptor would keep a map alive across `thalyx enforce detach`,
/// so a detached machine would still have somewhere to write policy — and
/// writes would appear to succeed into a map nothing reads. Opening the pin
/// each time means the answer always comes from what is pinned now.
pub struct KernelStore {
    map: PathBuf,
}

impl KernelStore {
    pub const DEFAULT_MAP: &'static str = "/sys/fs/bpf/thalyx/maps/thalyx_policy";

    pub fn new(map: impl Into<PathBuf>) -> Self {
        Self { map: map.into() }
    }

    pub fn default_map() -> Self {
        Self::new(Self::DEFAULT_MAP)
    }

    pub fn path(&self) -> &Path {
        &self.map
    }

    fn open(&self) -> Result<std::os::fd::OwnedFd, StoreError> {
        thalyx_syscall::bpf_obj_get(&self.map).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotPinned(self.map.clone())
            } else {
                StoreError::Kernel {
                    what: "opening the policy map",
                    source: error,
                }
            }
        })
    }
}

impl PolicyStore for KernelStore {
    /// Whether the map the kernel side pinned is really there and can be
    /// opened.
    ///
    /// Opening it, rather than asking whether the path exists. bpffs is mode
    /// 700, so an unprivileged existence check reports missing for a map that
    /// is present — the mistake that once made the tooling read as disarmed
    /// while it was armed — and a path can exist while the object behind it is
    /// something else entirely.
    fn is_available(&self) -> bool {
        self.open().is_ok()
    }

    fn set(&self, cgroup: u64, policy: Policy) -> Result<(), StoreError> {
        let map = self.open()?;
        thalyx_syscall::bpf_map_update(
            map.as_fd(),
            &cgroup_key_bytes(cgroup),
            &policy_bytes(&policy),
        )
        .map_err(|source| StoreError::Kernel {
            what: "writing a policy",
            source,
        })
    }

    fn remove(&self, cgroup: u64) -> Result<(), StoreError> {
        let map = self.open()?;
        match thalyx_syscall::bpf_map_delete(map.as_fd(), &cgroup_key_bytes(cgroup)) {
            Ok(()) => Ok(()),
            // Removing a grant that is not there is the desired end state, not
            // a failure. Revocation has to be idempotent: it is what runs on
            // the recovery paths, where the state is by definition unknown.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(StoreError::Kernel {
                what: "removing a policy",
                source,
            }),
        }
    }

    fn get(&self, cgroup: u64) -> Result<Option<Policy>, StoreError> {
        let map = self.open()?;
        let mut value = [0u8; POLICY_BYTES];
        let found =
            thalyx_syscall::bpf_map_lookup(map.as_fd(), &cgroup_key_bytes(cgroup), &mut value)
                .map_err(|source| StoreError::Kernel {
                    what: "reading a policy back",
                    source,
                })?;
        Ok(found.then(|| policy_from_bytes(&value)))
    }
}

/// An in-memory store, for tests.
pub struct MemoryStore {
    entries: Mutex<std::collections::BTreeMap<u64, Policy>>,
    available: bool,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            entries: Mutex::default(),
            available: true,
        }
    }

    /// A store that reports it cannot enforce, so the refusal path can be
    /// exercised without unloading the kernel side.
    ///
    /// Hand-written rather than derived alongside `Default`: a derived
    /// `Default` would leave `available` false, quietly making every test that
    /// used it exercise the refusal path instead of the one it named.
    pub fn unavailable() -> Self {
        Self {
            entries: Mutex::default(),
            available: false,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.lock().expect("not poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyStore for MemoryStore {
    fn is_available(&self) -> bool {
        self.available
    }

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
        assert!(KernelStore::DEFAULT_MAP.starts_with("/sys/fs/bpf/thalyx/maps/"));
        assert!(KernelStore::DEFAULT_MAP.ends_with("thalyx_policy"));
    }
}
