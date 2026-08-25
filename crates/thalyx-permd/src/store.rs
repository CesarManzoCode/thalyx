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

/// Whether denials are real, as opposed to merely attached.
///
/// `make -C lsm load` deliberately lands in observe mode — the kernel side is
/// attached, every hook runs, every denial is written to the ring, and none of
/// them is applied. That is a good default for measuring a policy before it
/// binds, and it is a terrible thing to mistake for enforcement.
///
/// Nothing on this side ever asked. [`PolicyStore::is_available`] answers
/// "does the policy map open", the code deciding whether to confine read that
/// as "the kernel is enforcing", and the two are not the same question — the
/// same shape of mistake this type's own documentation records twice above,
/// arriving from the opposite direction. Found on 2026-08-25 by Cesar running
/// `ejecutar` after `verify.sh` had detached the LSM on its way out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enforcement {
    /// A denial reaches the caller as `-EPERM`.
    Enforcing,
    /// Attached, logging what it would have denied, denying nothing.
    Observing,
    /// Rule 10: a failure to read is not a failure to exist. A caller that
    /// treats this as `Observing` is guessing, and one that treats it as
    /// `Enforcing` is guessing in the dangerous direction — so it is neither,
    /// and it carries what went wrong.
    Unreadable(String),
}

impl Enforcement {
    /// For a report meant for a human, in the same voice `make -C lsm status`
    /// uses.
    pub fn describe(&self) -> String {
        match self {
            Self::Enforcing => "enforcing".to_string(),
            Self::Observing => "observing (denials are logged, not applied)".to_string(),
            Self::Unreadable(reason) => format!("COULD NOT BE READ — {reason}"),
        }
    }
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

    /// Whether what is attached is denying or only watching.
    ///
    /// Separate from [`is_available`](Self::is_available) because the two
    /// failures are different and the human needs different words for them:
    /// one is fixed with `make -C lsm load` and the other with
    /// `make -C lsm enforce`. Defaults to `Enforcing` for stores that always
    /// mean what they say; [`KernelStore`] reads the map.
    fn enforcement(&self) -> Enforcement {
        Enforcement::Enforcing
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

    /// The mode flag, beside the policy map in whatever directory that one was
    /// pinned in.
    ///
    /// Derived rather than a second constant, so a `KernelStore` pointed at a
    /// test pin cannot answer with the mode of the machine's real enforcement
    /// — which would make the fake say `Enforcing` while the thing under test
    /// enforced nothing.
    fn enforcing_map(&self) -> PathBuf {
        self.map.with_file_name("thalyx_enforcing")
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

    /// Reads the one-entry array the BPF side consults on every hook.
    ///
    /// Asked of the map rather than of `bpftool map dump`, for the reason the
    /// type documentation gives twice: this project has now four times asked
    /// something other than the kernel a question only the kernel can answer.
    fn enforcement(&self) -> Enforcement {
        let path = self.enforcing_map();
        let map = match thalyx_syscall::bpf_obj_get(&path) {
            Ok(map) => map,
            Err(error) => {
                return Enforcement::Unreadable(format!("opening {}: {error}", path.display()));
            }
        };

        let mut value = [0u8; 4];
        match thalyx_syscall::bpf_map_lookup(map.as_fd(), &0u32.to_ne_bytes(), &mut value) {
            Ok(true) if u32::from_ne_bytes(value) != 0 => Enforcement::Enforcing,
            Ok(true) => Enforcement::Observing,
            // A BPF array map's entries all exist from the moment it is
            // created, so a miss here means the pin is not the map this
            // expects. Rule 9: that is the cautious answer, not `Observing`
            // and not a panic.
            Ok(false) => Enforcement::Unreadable(format!(
                "{} answered that it has no entry 0, so it is not the mode flag",
                path.display()
            )),
            Err(error) => Enforcement::Unreadable(format!("reading {}: {error}", path.display())),
        }
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
    enforcement: Enforcement,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            entries: Mutex::default(),
            available: true,
            enforcement: Enforcement::Enforcing,
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
            enforcement: Enforcement::Unreadable("nothing is loaded".to_string()),
        }
    }

    /// A store that is loaded and denies nothing — the state
    /// `make -C lsm load` leaves a machine in.
    ///
    /// Rule 8: a fake must model the property under test. A `MemoryStore` that
    /// reported `Enforcing` no matter what could not be used to test the one
    /// thing this distinction exists for.
    pub fn observing() -> Self {
        Self {
            entries: Mutex::default(),
            available: true,
            enforcement: Enforcement::Observing,
        }
    }

    /// A store that is loaded and cannot say which mode it is in.
    pub fn mode_unreadable(reason: &str) -> Self {
        Self {
            entries: Mutex::default(),
            available: true,
            enforcement: Enforcement::Unreadable(reason.to_string()),
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

    fn enforcement(&self) -> Enforcement {
        self.enforcement.clone()
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
    #[test]
    fn a_mode_flag_that_is_not_pinned_is_unreadable_rather_than_observing() {
        // Rule 10, at the place it costs the most. "Observing" is a claim
        // about a loaded kernel; a missing pin is a claim about nothing. A
        // caller that saw `Observing` here would print a remedy —
        // `make -C lsm enforce` — for a machine where the fix is
        // `make -C lsm load`.
        let store = KernelStore::new("/nonexistent/thalyx/maps/thalyx_policy");

        match store.enforcement() {
            Enforcement::Unreadable(reason) => {
                assert!(reason.contains("thalyx_enforcing"), "{reason}")
            }
            other => panic!("a missing pin answered {other:?}"),
        }
    }

    #[test]
    fn the_mode_flag_is_looked_for_beside_the_policy_map_it_was_given() {
        // Not at the machine's real pin. A store pointed somewhere else — a
        // test, a second machine's bpffs mounted for inspection — that read
        // the running kernel's mode would report enforcement belonging to
        // something other than the map it writes into.
        let store = KernelStore::new("/tmp/somewhere/maps/thalyx_policy");

        assert_eq!(
            store.enforcing_map(),
            Path::new("/tmp/somewhere/maps/thalyx_enforcing")
        );
    }

    #[test]
    fn the_fake_can_be_any_of_the_three_states() {
        // Rule 8: a fake that could only say `Enforcing` would make every test
        // of the refusal path a test of nothing.
        assert_eq!(MemoryStore::new().enforcement(), Enforcement::Enforcing);
        assert_eq!(
            MemoryStore::observing().enforcement(),
            Enforcement::Observing
        );
        assert!(matches!(
            MemoryStore::mode_unreadable("because").enforcement(),
            Enforcement::Unreadable(reason) if reason == "because"
        ));
    }

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
