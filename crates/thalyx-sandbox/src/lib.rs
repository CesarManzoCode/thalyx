//! `thalyx-sandbox` — running module code under a policy the kernel enforces.
//!
//! This crate is **outside the TCB**. It contains module code; it is not
//! trusted by anything. See `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md` and
//! `vault/11-Seguridad/Modelo-de-Amenaza.md`.
//!
//! ## What it closes
//!
//! `thalyx-permd` can write a policy into the kernel, but until now nothing
//! called it: a module was installed with its permissions recorded, and then
//! ran with none of them enforced. The registry said one thing and the machine
//! did another, which is the failure mode the whole project is arranged to
//! avoid.
//!
//! ## The ordering rule
//!
//! **The policy is in the kernel before the process is in the cgroup, and the
//! process is in the cgroup before the module's first instruction runs.**
//!
//! Both halves matter, and for different reasons:
//!
//! - The LSM fails *open* for a cgroup it has no entry for — it has to, or
//!   every process on the machine would be denied everything. So a process
//!   that enters its cgroup before the policy is written is unconfined for as
//!   long as that takes.
//! - A process cannot join a cgroup before it exists. The gap between `fork`
//!   and the join is unavoidable; what is avoidable is running module code
//!   inside it. See [`launch`].
//!
//! Teardown reverses it, and the reason is sharper than symmetry: a cgroup id
//! is an inode number, and inode numbers are reused. A map entry that outlived
//! its directory would silently become the policy of whatever cgroup is
//! created next. **The policy is withdrawn before the cgroup is removed.**
//!
//! ## What is not here yet
//!
//! The `module_standard` profile decrees namespaces, a seccomp allowlist and
//! cgroup resource limits. None of those are implemented. This crate delivers
//! the identity and the launch ordering the kernel policy hangs on, and
//! nothing more — a module launched through it is contained by `thalyx-lsm`
//! and by nothing else.

pub mod cgroup;
pub mod launch;

pub use cgroup::Cgroup;
pub use launch::{EnterRequest, enter_and_exec, parse_enter};

use std::path::{Path, PathBuf};
use thalyx_manifest::Permission;
use thalyx_permd::PolicyStore;

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("no cgroup2 filesystem is mounted; module confinement is not available")]
    NoCgroup2,

    #[error(
        "`{0}` is not on a cgroup2 filesystem.\n  \
         Writing a process there would report success and confine nothing."
    )]
    NotCgroup2(PathBuf),

    #[error("cgroup `{0}` does not exist")]
    NoSuchCgroup(PathBuf),

    #[error("`{0}` cannot be used as a cgroup name")]
    UnusableName(String),

    #[error(
        "process {pid} wrote itself into `{cgroup}` but is not a member.\n  \
         Refusing to run the module: it would be unconfined."
    )]
    JoinNotEffective { cgroup: PathBuf, pid: u32 },

    #[error(
        "entrypoint `{0}` points outside the module tree.\n  \
         It would run with the module's permissions without being the module's code."
    )]
    EntrypointEscapes(String),

    #[error("entrypoint `{0}` does not exist")]
    NoSuchEntrypoint(PathBuf),

    #[error("`{module_id}` declares no entrypoint named `{entrypoint}`")]
    NoSuchEntrypointName {
        module_id: String,
        entrypoint: String,
    },

    #[error("could not execute `{program}`: {source}")]
    Exec {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Permd(#[from] thalyx_permd::PermdError),
}

impl SandboxError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        SandboxError::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, SandboxError>;

/// A cgroup with a policy in the kernel, ready for a module to be launched in.
///
/// Holding one of these is the proof that the ordering rule was followed: it
/// cannot be constructed without the policy already having been written.
pub struct Confinement<'a> {
    cgroup: Cgroup,
    policy_store: &'a dyn PolicyStore,
    cgroup_id: u64,
    applied: thalyx_permd::Policy,
}

impl<'a> Confinement<'a> {
    /// Create the module's cgroup and put its policy in the kernel.
    ///
    /// In that order, and never the reverse: the policy is keyed on the
    /// cgroup's inode, so the directory has to exist before there is anything
    /// to key on. Nothing is placed in the cgroup here, so the window between
    /// the two steps contains no module code.
    ///
    /// If any granted permission cannot be expressed as kernel policy, this
    /// fails and the cgroup is removed again. Running the module anyway would
    /// mean it holds a subset of what the human confirmed, and the human would
    /// never be told which subset.
    pub fn establish(
        policy_store: &'a dyn PolicyStore,
        parent: &Path,
        name: &str,
        permissions: &[Permission],
        now_ns: u64,
        jit_lifetime_ns: u64,
    ) -> Result<Self> {
        let cgroup = Cgroup::ensure(parent, name)?;
        let cgroup_id = cgroup.id()?;

        let applied = match thalyx_permd::apply(
            policy_store,
            cgroup_id,
            permissions,
            now_ns,
            jit_lifetime_ns,
        ) {
            Ok(policy) => policy,
            Err(error) => {
                // Leave nothing behind that a later run could mistake for a
                // live confinement — but only if we are the only ones here.
                if cgroup.is_empty().unwrap_or(false) {
                    let _ = cgroup.remove();
                }
                return Err(error.into());
            }
        };

        Ok(Self {
            cgroup,
            policy_store,
            cgroup_id,
            applied,
        })
    }

    pub fn cgroup(&self) -> &Cgroup {
        &self.cgroup
    }

    pub fn cgroup_id(&self) -> u64 {
        self.cgroup_id
    }

    /// The policy actually written to the kernel.
    pub fn policy(&self) -> thalyx_permd::Policy {
        self.applied
    }

    /// Start a module inside this confinement.
    pub fn spawn(
        &self,
        helper: &Path,
        program: &Path,
        args: &[std::ffi::OsString],
    ) -> Result<std::process::Child> {
        launch::spawn(helper, &self.cgroup, program, args)
    }

    /// Withdraw the policy and remove the cgroup, in that order.
    ///
    /// Does nothing if the cgroup still has members: another instance of the
    /// same module is running, and stripping its permissions mid-flight would
    /// deny it operations the human confirmed. Returns whether the
    /// confinement was actually torn down.
    pub fn release(self) -> Result<bool> {
        if !self.cgroup.is_empty()? {
            return Ok(false);
        }

        // Order is not cosmetic. The cgroup id is the directory's inode number
        // and inode numbers are reused; an entry left in the map after its
        // directory is gone would become the policy of whatever cgroup the
        // kernel allocates that inode to next.
        thalyx_permd::revoke(self.policy_store, self.cgroup_id)?;
        self.cgroup.remove()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cgroup::tests::fake_cgroup2;
    use thalyx_manifest::PermissionKind;
    use thalyx_permd::MemoryStore;

    /// A parent with the module's cgroup already in place.
    ///
    /// The fake cannot model the kernel populating a freshly created cgroup,
    /// so the child is laid out up front and `ensure` takes its reuse path.
    /// Creation and removal against a real mount are covered by
    /// `tests/real_cgroup.rs`.
    fn fake_hierarchy(root: &Path, name: &str) -> PathBuf {
        let parent = root.join("cgroup2");
        fake_cgroup2(&parent);
        fake_cgroup2(&parent.join(name));
        parent
    }

    fn permission(resource: &str, action: &str) -> Permission {
        Permission {
            resource: resource.to_string(),
            action: action.to_string(),
            kind: PermissionKind::Persistent,
        }
    }

    #[test]
    fn the_policy_is_in_the_kernel_before_anything_can_join() {
        let dir = tempfile::tempdir().unwrap();
        let parent = fake_hierarchy(dir.path(), "org.thalyx.demo");
        let store = MemoryStore::new();

        let confinement = Confinement::establish(
            &store,
            &parent,
            "org.thalyx.demo",
            &[permission("net", "outbound")],
            0,
            0,
        )
        .unwrap();

        // The cgroup exists, the policy is written, and nothing is in it yet.
        assert!(confinement.cgroup().path().is_dir());
        assert!(confinement.cgroup().is_empty().unwrap());
        assert_eq!(
            store.get(confinement.cgroup_id()).unwrap(),
            Some(confinement.policy())
        );
        assert!(confinement.policy().allows(thalyx_permd::NET_OUTBOUND));
    }

    #[test]
    fn a_permission_the_kernel_cannot_express_starts_no_confinement() {
        // The module must not run at all. Running it with the subset that did
        // translate would mean it holds permissions the human never saw, and
        // the LSM fails open on an unknown cgroup — so a half-established
        // confinement looks confined and is free.
        let dir = tempfile::tempdir().unwrap();
        let parent = fake_hierarchy(dir.path(), "org.thalyx.demo");
        let store = MemoryStore::new();

        let result = Confinement::establish(
            &store,
            &parent,
            "org.thalyx.demo",
            &[permission("net", "outbound"), permission("camera", "read")],
            0,
            0,
        );

        assert!(matches!(result, Err(SandboxError::Permd(_))));

        // Not even the part that could be expressed. Half a policy is the one
        // outcome worse than none.
        let id = thalyx_permd::cgroup_id(&parent.join("org.thalyx.demo")).unwrap();
        assert_eq!(store.get(id).unwrap(), None);
    }

    #[test]
    fn releasing_while_another_instance_runs_leaves_the_policy_alone() {
        // One cgroup per module means the second instance shares the first's
        // policy. Tearing it down when the first exits would deny the second
        // operations the human confirmed, at an arbitrary moment.
        let dir = tempfile::tempdir().unwrap();
        let parent = fake_hierarchy(dir.path(), "org.thalyx.demo");
        let store = MemoryStore::new();

        let confinement = Confinement::establish(
            &store,
            &parent,
            "org.thalyx.demo",
            &[permission("net", "outbound")],
            0,
            0,
        )
        .unwrap();
        let id = confinement.cgroup_id();
        confinement.cgroup().join(999).unwrap();

        assert!(!confinement.release().unwrap());
        assert!(store.get(id).unwrap().is_some());
        assert!(parent.join("org.thalyx.demo").is_dir());
    }
}
