//! Sandbox profiles: what `module_standard` actually means.
//!
//! `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md` decrees the profile as a
//! named set of isolation measures, declared in the contract and applied by
//! the core — never chosen by the module.
//!
//! A profile is data, on purpose. It is the thing a reviewer reads to know
//! what a module can reach, and it should be readable without following the
//! code that applies it.

use crate::limits::Limits;
use crate::seccomp::Allowlist;
use crate::{Result, SandboxError};
use thalyx_manifest::Permission;

/// The namespaces a profile detaches into.
///
/// A struct of booleans rather than a bitmask so that reading it says what it
/// means. The mask is derived once, in [`Namespaces::flags`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Namespaces {
    pub mount: bool,
    pub pid: bool,
    pub ipc: bool,
    pub uts: bool,
    pub network: bool,
}

impl Namespaces {
    pub const NONE: Self = Self {
        mount: false,
        pid: false,
        ipc: false,
        uts: false,
        network: false,
    };

    pub fn any(&self) -> bool {
        *self != Self::NONE
    }

    /// Recover a set from a flag mask.
    ///
    /// The launcher passes the mask across the re-execution rather than having
    /// the child derive it again. Deriving it twice is how a module granted
    /// outbound network ended up in an empty network namespace anyway: the
    /// parent adjusted the profile, the child re-resolved it from the name,
    /// and the two disagreed with nothing to notice.
    pub fn from_flags(flags: i32) -> Self {
        Self {
            mount: flags & thalyx_syscall::CLONE_NEWNS != 0,
            pid: flags & thalyx_syscall::CLONE_NEWPID != 0,
            ipc: flags & thalyx_syscall::CLONE_NEWIPC != 0,
            uts: flags & thalyx_syscall::CLONE_NEWUTS != 0,
            network: flags & thalyx_syscall::CLONE_NEWNET != 0,
        }
    }

    pub fn flags(&self) -> i32 {
        let mut flags = 0;
        if self.mount {
            flags |= thalyx_syscall::CLONE_NEWNS;
        }
        if self.pid {
            flags |= thalyx_syscall::CLONE_NEWPID;
        }
        if self.ipc {
            flags |= thalyx_syscall::CLONE_NEWIPC;
        }
        if self.uts {
            flags |= thalyx_syscall::CLONE_NEWUTS;
        }
        if self.network {
            flags |= thalyx_syscall::CLONE_NEWNET;
        }
        flags
    }
}

/// A named set of isolation measures.
#[derive(Debug, Clone)]
pub struct Profile {
    pub name: &'static str,
    pub namespaces: Namespaces,
    pub limits: Limits,
    pub seccomp: Option<Allowlist>,
    /// Whether the module drops to a user of its own before it runs.
    ///
    /// The uid itself comes from the core, which owns the assignment. A
    /// profile only says whether the drop happens at all.
    pub own_user: bool,
    /// Whether the module is pivoted into a root of its own.
    ///
    /// Separate from `namespaces.mount`, because a mount namespace on its own
    /// isolates the mount *table* and not the files. Having them as one flag
    /// invited exactly the confusion that let the sandbox ship with a mount
    /// namespace and the whole host tree inside it.
    pub pivot_root: bool,
    /// What the module sees as the hostname, inside its UTS namespace.
    ///
    /// Fixed rather than inherited: the real hostname is information about the
    /// machine that no module needs, and plenty of software keys behaviour off
    /// it.
    pub hostname: &'static str,
}

/// The profile every module runs under.
pub const MODULE_STANDARD: &str = "module_standard";

/// cgroup identity only — no namespaces, no filter, no limits.
///
/// It exists so the launch ordering can be tested on its own, on machines
/// where namespaces or controllers are unavailable, and so that a confined run
/// that goes wrong can be bisected.
///
/// It **is** reachable: `thalyx module run --profile diagnostic` accepts it.
/// Hiding it would not make it safer — anyone who can invoke that already runs
/// as the user Thalyx runs as. What keeps it honest is [`Profile::isolates`]:
/// a run under a profile that isolates nothing is announced and journalled as
/// degraded, exactly like `--unconfined`.
pub const DIAGNOSTIC: &str = "diagnostic";

/// Look up a profile by the name a contract declared.
///
/// An unknown name is an error rather than a fallback to something safe. A
/// fallback would mean a typo silently changed what a module is allowed to
/// reach, which is the class of mistake nobody notices.
pub fn resolve(name: &str) -> Result<Profile> {
    match name {
        MODULE_STANDARD => Ok(module_standard()),
        DIAGNOSTIC => Ok(diagnostic()),
        other => Err(SandboxError::UnknownProfile(other.to_string())),
    }
}

/// The `module_standard` profile as decreed.
pub fn module_standard() -> Profile {
    Profile {
        name: MODULE_STANDARD,
        namespaces: Namespaces {
            mount: true,
            pid: true,
            ipc: true,
            uts: true,
            // Denied by default; lifted only by an explicit grant. See
            // `for_permissions`.
            network: true,
        },
        limits: Limits {
            // Chosen to be generous rather than tuned. A module that dies at a
            // limit nobody picked deliberately is a worse failure than one
            // that uses more memory than it should, and the number is a policy
            // knob, not an architectural decision.
            memory_max: Some(1 << 30), // 1 GiB
            pids_max: Some(512),
            // Uncapped on purpose. The mechanism is here and tested; picking a
            // fraction of the machine for every module on it is a decision
            // about how Thalyx feels to use, not one about how it is built.
            cpu_max: None,
        },
        seccomp: Some(crate::seccomp::module_standard()),
        pivot_root: true,
        own_user: true,
        hostname: "thalyx-module",
    }
}

fn diagnostic() -> Profile {
    Profile {
        name: DIAGNOSTIC,
        namespaces: Namespaces::NONE,
        limits: Limits::default(),
        seccomp: None,
        pivot_root: false,
        own_user: false,
        hostname: "thalyx-module",
    }
}

impl Profile {
    /// Adjust the profile for what the module was actually granted.
    ///
    /// Only one thing varies today, and it varies in two places at once: the
    /// network namespace and the seccomp filter. An empty netns is the
    /// strongest possible denial — there is no route, no address, nothing to
    /// connect through — but a module that *was* granted outbound network has
    /// to be able to use it, and Phase 1 does not build veth pairs. So the
    /// namespace is dropped for those modules and `thalyx-lsm` enforces the
    /// grant instead.
    ///
    /// The asymmetry is deliberate. A module with no network permission gets
    /// two independent denials; a module with one gets the enforcement it was
    /// granted under. Neither ends up with less than it should.
    ///
    /// ## Why the filter has to move too
    ///
    /// This used to adjust only the namespace, and the result was the one
    /// arrangement that is worse than either alternative: the grant removed
    /// the network namespace, and the filter went on refusing `socket`
    /// unconditionally. So a module granted `net/outbound` could not open a
    /// connection — and had been given the host's network namespace in
    /// exchange for nothing. The permission cost isolation and delivered no
    /// capability, and every test passed, because the LSM test proved the hook
    /// denies and no test ever asked whether a *granted* module could connect.
    ///
    /// Both halves move together now, from the same grant, in one place.
    pub fn for_permissions(mut self, permissions: &[Permission]) -> Self {
        if grants_network(permissions) {
            self.namespaces.network = false;
            self.seccomp = self
                .seccomp
                .map(|allowlist| allowlist.allow_all(crate::seccomp::outbound_network()));
        }
        self
    }

    /// Whether this profile isolates at all, beyond the cgroup.
    pub fn isolates(&self) -> bool {
        self.namespaces.any() || self.seccomp.is_some() || !self.limits.is_empty()
    }

    /// A one-line summary for the operator.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();

        let mut namespaces = Vec::new();
        for (enabled, name) in [
            (self.namespaces.mount, "mount"),
            (self.namespaces.pid, "pid"),
            (self.namespaces.ipc, "ipc"),
            (self.namespaces.uts, "uts"),
            (self.namespaces.network, "net"),
        ] {
            if enabled {
                namespaces.push(name);
            }
        }
        if namespaces.is_empty() {
            parts.push("no namespaces".to_string());
        } else {
            parts.push(format!("namespaces: {}", namespaces.join("+")));
        }

        parts.push(if self.pivot_root {
            "own root filesystem".to_string()
        } else {
            "the host filesystem".to_string()
        });

        parts.push(if self.own_user {
            "own unprivileged user".to_string()
        } else {
            "the user Thalyx runs as".to_string()
        });

        match &self.seccomp {
            Some(allowlist) => parts.push(format!("seccomp: {} syscalls allowed", allowlist.len())),
            None => parts.push("no seccomp filter".to_string()),
        }

        if self.limits.is_empty() {
            parts.push("no resource limits".to_string());
        } else {
            let mut limits = Vec::new();
            if let Some(bytes) = self.limits.memory_max {
                limits.push(format!("memory {}MiB", bytes / (1 << 20)));
            }
            if let Some(count) = self.limits.pids_max {
                limits.push(format!("pids {count}"));
            }
            if let Some(cpu) = self.limits.cpu_max {
                limits.push(format!("cpu {}/{}", cpu.quota_us, cpu.period_us));
            }
            parts.push(limits.join(", "));
        }

        parts.join("; ")
    }
}

fn grants_network(permissions: &[Permission]) -> bool {
    permissions
        .iter()
        .any(|p| p.resource == "net" && p.action == "outbound")
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_manifest::PermissionKind;

    fn permission(resource: &str, action: &str) -> Permission {
        Permission {
            resource: resource.to_string(),
            action: action.to_string(),
            kind: PermissionKind::Persistent,
        }
    }

    #[test]
    fn an_unknown_profile_is_an_error_not_a_fallback() {
        // A fallback would let a typo in a contract silently change what a
        // module can reach. Nobody would ever notice.
        assert!(matches!(
            resolve("moduel_standard"),
            Err(SandboxError::UnknownProfile(_))
        ));
        assert!(resolve(MODULE_STANDARD).is_ok());
    }

    #[test]
    fn a_module_without_network_permission_gets_an_empty_network_namespace() {
        let profile = module_standard().for_permissions(&[permission("/home/user", "read")]);
        assert!(profile.namespaces.network);
        assert!(profile.namespaces.flags() & thalyx_syscall::CLONE_NEWNET != 0);
    }

    #[test]
    fn a_module_granted_network_keeps_the_host_network_and_is_held_by_the_lsm() {
        // Not a weakening: it is the difference between a module that must not
        // reach the network and one the human said may. Phase 1 has no veth
        // pair to give the second a namespace it can still use.
        let profile = module_standard().for_permissions(&[permission("net", "outbound")]);
        assert!(!profile.namespaces.network);
        assert_eq!(profile.namespaces.flags() & thalyx_syscall::CLONE_NEWNET, 0);

        // Everything else survives.
        assert!(profile.namespaces.mount);
        assert!(profile.namespaces.pid);
        assert!(profile.seccomp.is_some());
    }

    #[test]
    fn a_granted_module_can_actually_build_a_socket_and_an_ungranted_one_cannot() {
        // The pair of claims the grant is supposed to mean. Written as one
        // test because either half alone is a state Thalyx was actually in:
        //
        // - Without the first, `net/outbound` drops the network namespace and
        //   the filter still refuses `socket`, so the grant costs a layer of
        //   isolation and hands back nothing. That is exactly what shipped.
        // - Without the second, every module can build a socket and the whole
        //   denial rests on the LSM, which is one layer where the decree asks
        //   for two.
        let ungranted = module_standard().for_permissions(&[permission("/home/user", "read")]);
        let granted = module_standard().for_permissions(&[permission("net", "outbound")]);

        for syscall in [libc::SYS_socket, libc::SYS_connect] {
            assert!(
                !ungranted
                    .seccomp
                    .as_ref()
                    .expect("the standard profile filters")
                    .contains(syscall),
                "syscall {syscall} is allowed for a module that was granted no network"
            );
            assert!(
                granted
                    .seccomp
                    .as_ref()
                    .expect("the standard profile filters")
                    .contains(syscall),
                "syscall {syscall} is denied to a module the human granted the network to, \
                 so the grant does nothing but remove its network namespace"
            );
        }
    }

    #[test]
    fn a_network_grant_does_not_quietly_hand_over_anything_else() {
        // The control on the widening. `for_permissions` adds a named handful
        // and must not become the place where the allowlist grows generally —
        // in particular not `bind`, which is inbound, a permission nothing
        // grants.
        let granted = module_standard().for_permissions(&[permission("net", "outbound")]);
        let filter = granted.seccomp.as_ref().expect("a filter");

        for syscall in [
            libc::SYS_bind,
            libc::SYS_listen,
            libc::SYS_accept,
            libc::SYS_ptrace,
            libc::SYS_mount,
            libc::SYS_unshare,
        ] {
            assert!(
                !filter.contains(syscall),
                "a network grant let {syscall} through"
            );
        }
    }

    #[test]
    fn the_standard_profile_asks_for_every_measure_the_decree_names() {
        let profile = module_standard();
        assert!(profile.namespaces.mount, "mount namespace");
        assert!(profile.namespaces.pid, "pid namespace");
        assert!(profile.namespaces.ipc, "ipc namespace");
        assert!(profile.namespaces.uts, "uts namespace");
        assert!(profile.namespaces.network, "network denied by default");
        assert!(profile.seccomp.is_some(), "seccomp allowlist");
        assert!(profile.limits.memory_max.is_some(), "memory.max");
        assert!(profile.limits.pids_max.is_some(), "pids.max");
        assert!(profile.pivot_root, "a root filesystem of its own");
        assert!(profile.own_user, "a user of its own");
        assert!(profile.isolates());
    }

    #[test]
    fn the_diagnostic_profile_admits_that_it_isolates_nothing() {
        let profile = diagnostic();
        assert!(!profile.isolates());
        assert!(profile.describe().contains("no namespaces"));
        assert!(profile.describe().contains("no seccomp"));
    }

    #[test]
    fn a_flag_mask_round_trips_through_the_re_execution() {
        // What the parent decided has to be exactly what the child applies.
        for namespaces in [
            module_standard().namespaces,
            module_standard()
                .for_permissions(&[permission("net", "outbound")])
                .namespaces,
            Namespaces::NONE,
        ] {
            assert_eq!(Namespaces::from_flags(namespaces.flags()), namespaces);
        }
    }

    #[test]
    fn the_flags_match_the_namespaces_that_are_on() {
        let none = Namespaces::NONE;
        assert_eq!(none.flags(), 0);
        assert!(!none.any());

        let only_pid = Namespaces {
            pid: true,
            ..Namespaces::NONE
        };
        assert_eq!(only_pid.flags(), thalyx_syscall::CLONE_NEWPID);
        assert!(only_pid.any());
    }
}
