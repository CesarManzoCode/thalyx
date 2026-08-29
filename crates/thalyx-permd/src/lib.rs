//! `thalyx-permd` — the permission broker.
//!
//! The core decides *what* a module may do. `thalyx-lsm` decides *whether* a
//! particular operation is allowed, in the kernel, at the moment it happens.
//! This crate is what connects the two: it turns a set of granted permissions
//! into the policy the kernel reads.
//!
//! The whole interface is one pinned BPF map. Userspace writes it, the kernel
//! reads it, and nothing else crosses. That keeps policy out of the kernel and
//! keeps userspace off the critical path of a security decision.
//!
//! ## The rule that shapes this crate
//!
//! **A permission the kernel cannot express is refused, never silently
//! dropped.**
//!
//! If a grant cannot be translated into something the LSM can act on, the
//! translation fails and the caller has to deal with it. The alternative —
//! translating what fits and quietly discarding the rest — would mean the
//! human confirmed a permission through the trusted path that nothing
//! enforces. A promise the system cannot keep is worse than a refusal, because
//! only one of the two is visible.
//!
//! See `vault/03-Primitivas/Permisos-JIT.md`.

mod encoding;
mod store;

pub use encoding::{cgroup_key_bytes, policy_bytes};
pub use store::{Enforcement, KernelStore, MemoryStore, Mode, PolicyStore, StoreError};

use serde::{Deserialize, Serialize};
use std::path::Path;
use thalyx_manifest::{Permission, PermissionKind};

/// Permission bits, mirroring `THALYX_*` in `lsm/thalyx_lsm.bpf.c` exactly.
///
/// These two definitions have to agree or the kernel enforces something other
/// than what was granted. The test at the bottom of this file is the only
/// thing keeping them honest; there is no compiler that checks across the
/// boundary.
pub const NET_OUTBOUND: u32 = 1 << 0;
pub const FS_READ: u32 = 1 << 1;
pub const FS_WRITE: u32 = 1 << 2;

/// How long a JIT grant lasts before the kernel stops honouring it.
pub const DEFAULT_JIT_LIFETIME_NS: u64 = 30 * 1_000_000_000;

/// Boot-relative nanoseconds, matching `bpf_ktime_get_boot_ns()` in the LSM.
///
/// Read from `/proc/uptime` rather than a wall clock. The kernel compares
/// expiries against its own boot-relative clock, and a wall clock drifts from
/// it across suspend — silently extending or cutting short every JIT grant.
///
/// Every caller uses this one function, so the deadline written into the map is
/// always on the same clock as the comparison that enforces it.
pub fn boot_ns() -> u64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| {
            contents
                .split_whitespace()
                .next()
                .and_then(|seconds| seconds.parse::<f64>().ok())
        })
        .map(|seconds| (seconds * 1_000_000_000.0) as u64)
        .unwrap_or(0)
}

#[derive(Debug, thiserror::Error)]
pub enum PermdError {
    #[error(
        "permission `{action} {resource}` cannot be expressed as kernel policy.\n  \
         It is refused rather than dropped: a permission the human confirmed but \
         nothing enforces is a promise the system cannot keep."
    )]
    Inexpressible { resource: String, action: String },

    #[error("cgroup path `{0}` does not exist")]
    NoSuchCgroup(std::path::PathBuf),

    #[error("could not read the cgroup id of `{path}`: {source}")]
    CgroupId {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Store(#[from] StoreError),
}

pub type Result<T> = std::result::Result<T, PermdError>;

/// What the kernel is told about one cgroup.
///
/// Layout matches `struct policy` in the BPF program: two 32-bit words then a
/// 64-bit one. See [`policy_bytes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Policy {
    pub allowed: u32,
    pub flags: u32,
    /// Boot-relative nanoseconds after which this grant stops applying.
    /// Zero means it does not expire on its own.
    pub expires_ns: u64,
}

impl Policy {
    pub fn allows(&self, bit: u32) -> bool {
        self.allowed & bit != 0
    }
}

/// Translate one permission into the bit the kernel checks.
///
/// Exhaustive and fail-closed: anything not listed here is refused. Adding a
/// resource means adding it in both this function and the BPF program, and
/// there is no way to do only one of the two without a test failing.
pub fn bit_for(permission: &Permission) -> Result<u32> {
    let bit = match (permission.resource.as_str(), permission.action.as_str()) {
        ("net", "outbound") => NET_OUTBOUND,
        (path, "read") if path.starts_with('/') => FS_READ,
        (path, "write") if path.starts_with('/') => FS_WRITE,
        _ => {
            return Err(PermdError::Inexpressible {
                resource: permission.resource.clone(),
                action: permission.action.clone(),
            });
        }
    };
    Ok(bit)
}

/// Build the policy for a set of granted permissions.
///
/// `now_ns` is boot-relative, matching `bpf_ktime_get_boot_ns()` in the LSM.
/// JIT grants carry an expiry the kernel enforces on its own, so a stalled or
/// dead broker cannot extend a permission past its deadline.
pub fn policy_for(permissions: &[Permission], now_ns: u64, jit_lifetime_ns: u64) -> Result<Policy> {
    let mut policy = Policy::default();

    for permission in permissions {
        policy.allowed |= bit_for(permission)?;

        if permission.kind == PermissionKind::Jit {
            // The soonest expiry wins: a policy is a single entry, so mixing a
            // JIT grant with a persistent one must not extend the JIT one.
            let expiry = now_ns.saturating_add(jit_lifetime_ns);
            policy.expires_ns = match policy.expires_ns {
                0 => expiry,
                existing => existing.min(expiry),
            };
        }
    }

    Ok(policy)
}

/// The cgroup id the kernel reports through `bpf_get_current_cgroup_id()`.
///
/// It is the inode number of the cgroup directory, which is why this reads a
/// path rather than asking a daemon: the kernel and the filesystem agree on it
/// without anything in between having to be trusted.
pub fn cgroup_id(path: &Path) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;

    if !path.exists() {
        return Err(PermdError::NoSuchCgroup(path.to_path_buf()));
    }
    let metadata = std::fs::metadata(path).map_err(|source| PermdError::CgroupId {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(metadata.ino())
}

/// Bits every confined program needs before any grant is considered.
///
/// `lsm/file_open` is path-blind on purpose: the mount namespace decides *what*
/// a confined program can see, and the policy decides *read or write* on it.
/// The two only compose if the program can read what was mounted for it —
/// otherwise it cannot open its own binary, and the confinement is not a
/// confinement, it is a brick.
///
/// So this is not a grant and it is not asked about. It is the floor that makes
/// the mount namespace mean what the human was told it means. It widens nothing:
/// `/` inside the sandbox is the root Thalyx built, holding the module's or the
/// guest's own directory, the read-only system paths, and whatever was named
/// and confirmed.
///
/// Found on 2026-08-25, the first time anything ran under an *enforcing*
/// kernel: `ejecutar /usr/bin/node --version` grants nothing, came out
/// `allowed=0x0`, and died before `node` — the launcher joins the cgroup and
/// then reads `cgroup.procs` back to check the join took, which is a file open
/// by a task that is now inside a cgroup allowed nothing.
///
/// The evidence it had been found before and worked around rather than fixed is
/// in `lsm/demo-enforcement.sh`, whose header says it puts "filesystem allowed,
/// network denied" in the map. It had to: with filesystem denied, the `python3`
/// it runs inside the cgroup could not have started, and the demo would have
/// been measuring `exec` instead of `connect`.
pub const CONFINED_FLOOR: u32 = FS_READ;

/// Apply a module's granted permissions to the kernel.
///
/// `floor` is OR'd into the result before it is written. Passed by the caller
/// rather than assumed here, because the two callers want different things:
/// launching a program needs [`CONFINED_FLOOR`], and binding a cgroup by hand
/// for inspection must reproduce exactly what was asked for and nothing else.
pub fn apply(
    store: &dyn PolicyStore,
    cgroup: u64,
    permissions: &[Permission],
    now_ns: u64,
    jit_lifetime_ns: u64,
    floor: u32,
) -> Result<Policy> {
    let mut policy = policy_for(permissions, now_ns, jit_lifetime_ns)?;
    policy.allowed |= floor;
    store.set(cgroup, policy)?;
    Ok(policy)
}

/// Withdraw a module's policy.
///
/// Revocation is immediate and needs no cooperation: the entry disappears and
/// the next hook that runs finds nothing, which the LSM treats as "not a
/// Thalyx module". There is no process to notify and nothing to wait for.
pub fn revoke(store: &dyn PolicyStore, cgroup: u64) -> Result<()> {
    store.remove(cgroup)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_granted_nothing_can_still_read_what_was_mounted_for_it() {
        // The case `ejecutar <ruta>` with no words after it produces, and the
        // case that did not work. `lsm/file_open` is path-blind, so
        // `allowed=0x0` denies the program its own binary — it cannot reach
        // `exec`, let alone anything a human would have refused it.
        let store = MemoryStore::new();
        let policy = apply(
            &store,
            42,
            &[],
            1_000,
            DEFAULT_JIT_LIFETIME_NS,
            CONFINED_FLOOR,
        )
        .expect("a policy with no grants is still a policy");

        assert!(policy.allows(FS_READ));
        // And the floor is a floor, not a door. Rule 4's control: a floor that
        // granted everything would pass the line above and be a hole.
        assert!(!policy.allows(FS_WRITE));
        assert!(!policy.allows(NET_OUTBOUND));
    }

    #[test]
    fn binding_a_cgroup_by_hand_writes_exactly_what_was_asked_and_no_floor() {
        // `thalyx enforce apply` is for inspection and for processes Thalyx
        // did not start. A floor added there would make the command's own
        // report a description of a map other than the one it wrote.
        let store = MemoryStore::new();
        let policy =
            apply(&store, 42, &[], 1_000, DEFAULT_JIT_LIFETIME_NS, 0).expect("an empty policy");

        assert_eq!(policy.allowed, 0);
    }

    #[test]
    fn the_floor_expires_with_the_soonest_jit_grant_it_sits_beside() {
        // Not a bug being pinned as correct — a consequence being written down
        // where it can be found. The floor is OR'd into one entry, and an
        // entry has one deadline, so a guest that named a `leyendo` path loses
        // the floor when that grant runs out. Thirty seconds by default, which
        // is the ceiling on how long anything launched with a JIT grant can
        // run at all.
        let store = MemoryStore::new();
        let policy = apply(
            &store,
            42,
            &[permission("/tmp/x", "read", PermissionKind::Jit)],
            1_000,
            DEFAULT_JIT_LIFETIME_NS,
            CONFINED_FLOOR,
        )
        .expect("a policy");

        assert_eq!(policy.expires_ns, 1_000 + DEFAULT_JIT_LIFETIME_NS);
    }

    fn permission(resource: &str, action: &str, kind: PermissionKind) -> Permission {
        Permission {
            resource: resource.to_string(),
            action: action.to_string(),
            kind,
        }
    }

    #[test]
    fn translates_the_permissions_the_kernel_understands() {
        assert_eq!(
            bit_for(&permission("net", "outbound", PermissionKind::Persistent)).unwrap(),
            NET_OUTBOUND
        );
        assert_eq!(
            bit_for(&permission(
                "/home/user",
                "read",
                PermissionKind::Persistent
            ))
            .unwrap(),
            FS_READ
        );
        assert_eq!(
            bit_for(&permission("/home/user", "write", PermissionKind::Jit)).unwrap(),
            FS_WRITE
        );
    }

    #[test]
    fn refuses_a_permission_it_cannot_express() {
        // The rule this crate exists to hold. Silently returning zero bits
        // would leave the human having confirmed something nothing enforces.
        for (resource, action) in [
            ("net", "inbound"),
            ("camera", "read"),
            ("relative/path", "read"),
            ("net", "read"),
        ] {
            let result = bit_for(&permission(resource, action, PermissionKind::Persistent));
            assert!(
                matches!(result, Err(PermdError::Inexpressible { .. })),
                "`{action} {resource}` should be refused, not dropped"
            );
        }
    }

    #[test]
    fn one_inexpressible_permission_fails_the_whole_policy() {
        // Not "translate what fits". If any part of a grant cannot be
        // enforced, the grant as a whole is not applied — otherwise the module
        // would run with a subset the user was never shown.
        let permissions = vec![
            permission("net", "outbound", PermissionKind::Persistent),
            permission("camera", "read", PermissionKind::Persistent),
        ];
        assert!(matches!(
            policy_for(&permissions, 0, 0),
            Err(PermdError::Inexpressible { .. })
        ));
    }

    #[test]
    fn combines_bits_across_permissions() {
        let permissions = vec![
            permission("net", "outbound", PermissionKind::Persistent),
            permission("/home/user/projects", "read", PermissionKind::Persistent),
        ];
        let policy = policy_for(&permissions, 0, 0).unwrap();

        assert!(policy.allows(NET_OUTBOUND));
        assert!(policy.allows(FS_READ));
        assert!(!policy.allows(FS_WRITE));
    }

    #[test]
    fn persistent_grants_do_not_expire() {
        let permissions = vec![permission("net", "outbound", PermissionKind::Persistent)];
        let policy = policy_for(&permissions, 1_000, 30_000_000_000).unwrap();
        assert_eq!(policy.expires_ns, 0);
    }

    #[test]
    fn jit_grants_carry_a_deadline_the_kernel_can_enforce() {
        let permissions = vec![permission("/tmp/scratch", "write", PermissionKind::Jit)];
        let policy = policy_for(&permissions, 1_000, 30_000_000_000).unwrap();
        assert_eq!(policy.expires_ns, 30_000_001_000);
    }

    #[test]
    fn mixing_a_jit_grant_with_a_persistent_one_keeps_the_shorter_deadline() {
        // A single map entry holds one expiry, so the shortest has to win.
        // Otherwise adding a persistent permission would quietly extend a JIT
        // one past the moment it was supposed to lapse.
        let permissions = vec![
            permission("net", "outbound", PermissionKind::Persistent),
            permission("/tmp/scratch", "write", PermissionKind::Jit),
        ];
        let policy = policy_for(&permissions, 1_000, 5_000).unwrap();

        assert_eq!(policy.expires_ns, 6_000);
        assert!(policy.allows(NET_OUTBOUND));
        assert!(policy.allows(FS_WRITE));
    }

    #[test]
    fn an_empty_grant_allows_nothing() {
        let policy = policy_for(&[], 0, 0).unwrap();
        assert_eq!(policy.allowed, 0);
        assert!(!policy.allows(NET_OUTBOUND));
    }

    #[test]
    fn the_bits_match_the_kernel_program() {
        // These constants exist twice: here and in lsm/thalyx_lsm.bpf.c. No
        // compiler checks across that boundary, so this test reads the C and
        // compares. If it fails, the kernel is enforcing something other than
        // what was granted.
        let source = include_str!("../../../lsm/thalyx_lsm.bpf.c");

        // Compare definitions, not formatting. The C aligns its defines with
        // runs of spaces, and a test that fails on alignment teaches people to
        // ignore it — which would cost the one thing it is here to catch.
        let normalised: String = source
            .lines()
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n");

        for (name, value) in [
            ("THALYX_NET_OUTBOUND", NET_OUTBOUND),
            ("THALYX_FS_READ", FS_READ),
            ("THALYX_FS_WRITE", FS_WRITE),
        ] {
            let shift = value.trailing_zeros();
            let expected = format!("#define {name} (1 << {shift})");
            assert!(
                normalised.contains(&expected),
                "expected `{expected}` in the BPF program; the Rust and kernel \
                 definitions of {name} have drifted apart, which means the kernel \
                 would enforce something other than what was granted"
            );
        }
    }
}
