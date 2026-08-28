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
//! ## The `module_standard` profile
//!
//! On top of the cgroup, a confined module gets what
//! `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md` decrees: mount, PID, IPC and
//! UTS namespaces, an empty network namespace unless it was granted outbound
//! network, a seccomp allowlist, and cgroup resource limits. See [`profile`].
//!
//! ## What is still not here
//!
//! **The user namespace for the module itself.** The decree names it and this
//! does not implement it. A user namespace that mapped root to root would
//! satisfy the letter of the decree and isolate nothing, and this project does
//! not ship theatre and call it protection.
//!
//! What blocked it was a policy question — which uid a module runs as, and who
//! owns the files in the store — and that question has since been answered
//! elsewhere: `thalyx-core`'s `uids` hands each module a uid that is never
//! recycled (it is not a dependency of this crate — the caller passes the uid
//! down in [`LaunchSpec`]), [`launch`] descends to it and re-reads the
//! effective uid before executing anything, and [`idmap`] holds a user
//! namespace open purely to describe the id translation for granted paths.
//! **So a module does not run as
//! the uid Thalyx runs as** — this paragraph said it did long after it stopped
//! being true, which is the failure mode a status comment has: it keeps
//! claiming the state of the world on the day it was written.
//!
//! What is still missing is narrower than it was: an id map of the module's
//! own, rather than the uid drop plus idmapped mounts that stand in for one.

pub mod cgroup;
pub mod idmap;
pub mod launch;
pub mod limits;
pub mod profile;
pub mod rootfs;
pub mod seccomp;

pub use cgroup::Cgroup;
pub use launch::{LaunchSpec, Stage, parse_stage, run_stage};
pub use limits::Limits;
pub use profile::Profile;
pub use rootfs::RootFs;

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

    /// Told apart from the write that puts a process in, and this is the whole
    /// reason the variant exists.
    ///
    /// Both used to report `I/O error at <cgroup>/cgroup.procs`, and on
    /// 2026-08-25 that one sentence was the only evidence of a failure whose
    /// two candidate causes needed opposite fixes. The write happens while the
    /// task is still outside; the read happens after it is in, and is
    /// therefore the first thing the cgroup's own policy is ever asked about.
    #[error(
        "`{cgroup}` could not be read back after joining it: {source}\n  \
         The write that puts a process in happens from outside the cgroup and \
         this read happens from inside, so a denial here is the new policy \
         answering, not a permission on the directory."
    )]
    MembershipUnreadable {
        cgroup: PathBuf,
        #[source]
        source: std::io::Error,
    },

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

    #[error("`{0}` is not a sandbox profile Thalyx knows")]
    UnknownProfile(String),

    #[error(
        "`{path}` was granted for {action} and is not there.\n  \
         Refusing to run: the module would hold a permission it cannot use, and \
         nothing would say so. Create the path, or remove the grant."
    )]
    GrantedPathMissing { path: PathBuf, action: String },

    #[error("the launch specification could not be {direction}: {source}")]
    Spec {
        direction: &'static str,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "could not detach into the namespaces `{profile}` requires: {source}\n  \
         Refusing to run the module: it would be isolated by less than the profile says."
    )]
    NamespacesUnavailable {
        profile: String,
        #[source]
        source: std::io::Error,
    },

    #[error("could not {what}: {source}")]
    MountFailed {
        what: String,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "could not become user {uid}: {source}\n  \
         Refusing to run the module: it would run as whatever Thalyx runs as."
    )]
    UserNotDropped {
        uid: u32,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "asked to become user {wanted} and the process is still {actual}.\n  \
         Refusing to run the module: every step after this looks the same either way."
    )]
    UserNotEffective { wanted: u32, actual: u32 },

    #[error("could not remap ownership for the module's user: {reason}")]
    Idmap { reason: String },

    #[error(
        "idmapped mounts are not available: {reason}\n  \
         A granted write path owned by someone else cannot be made usable without them."
    )]
    IdmapUnavailable { reason: String },

    #[error("could not set the hostname inside the UTS namespace: {source}")]
    HostnameNotSet {
        #[source]
        source: std::io::Error,
    },

    /// The module's channel to Thalyx could not be put where the module looks
    /// for it.
    ///
    /// Fatal rather than degraded, and that is the whole point: a module that
    /// started without its channel would run with no way to reach the system,
    /// discover it on its first request, and report the absence as though
    /// Thalyx had refused it.
    #[error("could not place the module's channel on descriptor {number}: {source}")]
    ChannelNotPlaced {
        number: i32,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Seccomp(#[from] crate::seccomp::SeccompError),

    #[error(
        "`{cgroup}` cannot hand down the controller(s) {missing:?} that this profile needs.\n  \
         It has: {available:?}\n  \
         Without them the limits would not apply, and the module would look bounded and not be."
    )]
    ControllersUnavailable {
        cgroup: PathBuf,
        missing: Vec<String>,
        available: Vec<String>,
    },

    #[error(
        "could not set {limit}={value} on `{cgroup}`: {source}\n  \
         Refusing to run the module unbounded."
    )]
    LimitNotApplied {
        limit: String,
        value: String,
        cgroup: PathBuf,
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
    profile: Profile,
    permissions: Vec<Permission>,
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
        profile: Profile,
        permissions: &[Permission],
        now_ns: u64,
        jit_lifetime_ns: u64,
    ) -> Result<Self> {
        let profile = profile.for_permissions(permissions);

        // Before the cgroup exists, so a parent that cannot hand down what the
        // profile needs fails without leaving anything behind.
        limits::delegate(parent, &profile.limits.controllers())?;

        let cgroup = Cgroup::ensure(parent, name)?;
        let cgroup_id = cgroup.id()?;

        // Limits go on before the policy and long before anything joins. A
        // module is never briefly unbounded, and a limit that will not apply
        // is discovered while there is still nothing to clean up but an empty
        // cgroup.
        if let Err(error) = profile.limits.apply(cgroup.path()) {
            if cgroup.is_empty().unwrap_or(false) {
                let _ = cgroup.remove();
            }
            return Err(error);
        }

        let applied = match thalyx_permd::apply(
            policy_store,
            cgroup_id,
            permissions,
            now_ns,
            jit_lifetime_ns,
            // Read of what this program can see, always. Not a grant — see
            // `CONFINED_FLOOR`. Given to the policy and never to the profile,
            // which is the distinction that matters: the permissions below
            // also decide what `RootFs` bind-mounts, and a floor expressed as
            // a permission on `/` would mount the host's whole filesystem into
            // the sandbox.
            thalyx_permd::CONFINED_FLOOR,
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
            profile,
            permissions: permissions.to_vec(),
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

    /// The profile in force, after it was adjusted for what was granted.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Start a module inside this confinement.
    ///
    /// The root filesystem is built here rather than by the caller, from the
    /// same permissions the policy was written from. Two mechanisms, one
    /// source: the LSM refuses opens it was not told to allow, and the root
    /// contains nothing else to open.
    ///
    /// `channel` is the module's end of its socket to Thalyx. The caller keeps
    /// ownership — this only borrows it long enough to let it survive `exec`
    /// and to record which number it is on. `None` starts a program with no
    /// channel, which is what the sandbox's own tests need and what no real
    /// module should ever get: see [`launch::LaunchSpec::channel_fd`].
    pub fn spawn(
        &self,
        helper: &Path,
        module_dir: &Path,
        program: &Path,
        uid: Option<u32>,
        args: &[std::ffi::OsString],
        channel: Option<std::os::fd::BorrowedFd<'_>>,
    ) -> Result<std::process::Child> {
        let uid = if self.profile.own_user { uid } else { None };
        let rootfs = if self.profile.pivot_root {
            Some(rootfs::RootFs::for_module_as(
                module_dir,
                &self.permissions,
                uid,
            )?)
        } else {
            None
        };

        // Rust marks everything it opens close-on-exec, which is right for
        // every descriptor except this one: the channel exists precisely to
        // outlive two `exec`s. Cleared here rather than at creation so that the
        // window where it could leak into an unrelated child is as short as the
        // code allows.
        let channel_fd = match channel {
            Some(fd) => {
                use std::os::fd::AsRawFd;
                thalyx_syscall::clear_cloexec(fd).map_err(|source| {
                    SandboxError::ChannelNotPlaced {
                        number: fd.as_raw_fd(),
                        source,
                    }
                })?;
                Some(fd.as_raw_fd())
            }
            None => None,
        };

        let spec = launch::LaunchSpec {
            cgroup: self.cgroup.path().to_path_buf(),
            profile: self.profile.name.to_string(),
            namespaces: self.profile.namespaces,
            rootfs,
            program: program.to_path_buf(),
            uid,
            channel_fd,
        };

        launch::spawn(helper, &spec, args)
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
            crate::profile::resolve(crate::profile::DIAGNOSTIC).unwrap(),
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
            crate::profile::resolve(crate::profile::DIAGNOSTIC).unwrap(),
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
            crate::profile::resolve(crate::profile::DIAGNOSTIC).unwrap(),
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
