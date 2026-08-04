//! The filesystem a confined module sees.
//!
//! Until now the module had a mount namespace and the host's tree inside it:
//! the namespace isolated the *mount table*, not the files. What kept a module
//! out of `/etc/shadow` was `thalyx-lsm` and nothing else — real containment,
//! but one layer where the design calls for two.
//!
//! This builds the second. The module is pivoted into a root that contains its
//! own tree, the system libraries it needs to execute at all, and **exactly
//! the paths it was granted** — nothing else exists to be reached.
//!
//! ## The two layers say the same thing from opposite directions
//!
//! The LSM enforces the grants by refusing opens it was not told to allow. The
//! root filesystem enforces them by there being nothing else present. They are
//! derived from the same permissions and disagree about nothing, which is what
//! makes the pair worth having: a mistake in either one is still caught by the
//! other.
//!
//! ## Granted paths keep their names
//!
//! A grant on `/home/user/docs` appears at `/home/user/docs` inside. Only the
//! module's own tree moves, to `/module`, because its host path contains a
//! version number that nothing inside should have to know.

use crate::{Result, SandboxError};
use std::path::{Path, PathBuf};
use thalyx_manifest::Permission;

/// Where the module's own files appear inside its root.
pub const MODULE_ROOT: &str = "/module";

/// Where the old root is parked for the instant between `pivot_root` and
/// unmounting it. Inside the new root's tmpfs, so it never touches the host.
const OLD_ROOT: &str = "/.old-root";

/// The directory the new root is assembled on.
///
/// A dedicated path, and the choice is load-bearing. Assembling the root means
/// mounting a tmpfs over this directory, which **hides whatever was under it**
/// for the rest of the launch. The first attempt used `/tmp`, and the module
/// tree happened to live there: the tmpfs covered it and the bind that came
/// next could not find it. A granted path under `/tmp` would have failed the
/// same way, on someone else's machine, much later.
///
/// `/run` is where runtime state belongs, it is a tmpfs on any modern system,
/// and nothing a module is granted is plausibly under this exact path. What
/// persists on the host is one empty directory.
const ASSEMBLY: &str = "/run/thalyx/sandbox";

/// Read-only host paths a dynamically linked program needs to start at all.
///
/// Not a security boundary — it is most of a distribution. What keeps it
/// tolerable is that every one of them is mounted read-only, and that the LSM
/// still governs every open. Making it smaller means static module binaries,
/// which is a decision about how modules are built, not about the sandbox.
pub const SYSTEM_PATHS: [&str; 6] = ["/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc"];

/// Device nodes bound in individually, because a module needs a few and should
/// have no more. A whole `/dev` would include far too much.
pub const DEVICE_NODES: [&str; 5] = [
    "/dev/null",
    "/dev/zero",
    "/dev/full",
    "/dev/random",
    "/dev/urandom",
];

/// One thing bound into the module's root.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Bind {
    pub source: PathBuf,
    pub target: PathBuf,
    pub writable: bool,
    /// Whether this is a path the human granted, rather than a system path the
    /// module needs to start at all.
    ///
    /// Only granted paths are remapped for the module's user. The distinction
    /// is kept explicitly rather than inferred from writability, because a
    /// read grant needs remapping just as much as a write one.
    #[serde(default)]
    pub granted: bool,
}

/// What the module's root filesystem is made of.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RootFs {
    /// The module's own tree, mounted read-only at [`MODULE_ROOT`].
    pub module_dir: PathBuf,
    /// System paths, granted paths and device nodes.
    pub binds: Vec<Bind>,
    /// The user the module runs as, when its grants have to be remapped.
    ///
    /// A module running as its own uid cannot write to a directory belonging
    /// to someone else. Rather than change the owner on disk — Thalyx
    /// rewriting the human's filesystem to suit itself — the writable binds
    /// are idmapped, so the module sees them as its own and what it writes
    /// lands owned by whoever owns the directory. See [`crate::idmap`].
    pub uid: Option<u32>,
}

impl RootFs {
    /// Assemble the description of a root for a module and its grants.
    ///
    /// A granted path that does not exist is a refusal, not a shrug. The
    /// alternative is a module running with a grant the human confirmed and it
    /// cannot use — the same "promise the system cannot keep" this project
    /// refuses everywhere else, only mirrored.
    pub fn for_module(module_dir: &Path, permissions: &[Permission]) -> Result<Self> {
        Self::for_module_as(module_dir, permissions, None)
    }

    /// The same, for a module that runs as a user of its own.
    pub fn for_module_as(
        module_dir: &Path,
        permissions: &[Permission],
        uid: Option<u32>,
    ) -> Result<Self> {
        let mut binds = Vec::new();

        for path in SYSTEM_PATHS {
            let source = PathBuf::from(path);
            // Absent is fine here: no machine has every one of them, and a
            // missing `/sbin` is not something anyone granted.
            if source.exists() {
                binds.push(Bind {
                    source: source.clone(),
                    target: source,
                    writable: false,
                    granted: false,
                });
            }
        }

        for path in DEVICE_NODES {
            let source = PathBuf::from(path);
            if source.exists() {
                binds.push(Bind {
                    source: source.clone(),
                    target: source,
                    writable: false,
                    granted: false,
                });
            }
        }

        for permission in permissions {
            let Some(path) = granted_path(permission) else {
                continue;
            };
            if !path.exists() {
                return Err(SandboxError::GrantedPathMissing {
                    path,
                    action: permission.action.clone(),
                });
            }

            let writable = permission.action == "write";
            match binds.iter_mut().find(|bind| bind.source == path) {
                // Two grants on one path: the wider one wins, because both
                // were confirmed.
                Some(existing) => existing.writable |= writable,
                None => binds.push(Bind {
                    source: path.clone(),
                    target: path,
                    writable,
                    granted: true,
                }),
            }
        }

        Ok(Self {
            module_dir: module_dir.to_path_buf(),
            binds,
            uid,
        })
    }

    /// Where the module's entrypoint ends up once the root has been pivoted.
    ///
    /// The launcher resolves the program against the host tree; inside, the
    /// same file is under [`MODULE_ROOT`]. Rewriting it here rather than at the
    /// call site keeps the one place that knows the module moved.
    pub fn program_inside(&self, host_program: &Path) -> Result<PathBuf> {
        let relative = host_program
            .strip_prefix(&self.module_dir)
            .map_err(|_| SandboxError::EntrypointEscapes(host_program.display().to_string()))?;
        Ok(Path::new(MODULE_ROOT).join(relative))
    }

    /// Build the root and pivot into it.
    ///
    /// Runs inside the module's mount namespace, before the seccomp filter, and
    /// before anything of the module's own has executed.
    pub fn pivot(&self) -> Result<()> {
        let assembly = Path::new(ASSEMBLY);
        create_dir(assembly)?;

        // A tmpfs over `/tmp`. Two things at once: somewhere to assemble the
        // root that is not on any real filesystem, and — since it becomes the
        // new root — a module whose writes to `/` go nowhere the host can see.
        mount_at(
            None,
            assembly,
            Some("tmpfs"),
            thalyx_syscall::MS_NOSUID | thalyx_syscall::MS_NODEV,
            Some("mode=0755"),
            "mount the tmpfs the module's root is built on",
        )?;

        create_dir(&assembly.join(OLD_ROOT.trim_start_matches('/')))?;

        // Order matters, and it is not obvious.
        //
        // Every mount that will *shadow* part of the new root has to be in
        // place before anything is bound underneath it. The writable `/tmp`
        // goes first for exactly this reason: mounting it after the binds
        // covered any granted path that happened to live under `/tmp`, and the
        // module got "no such file or directory" for something the human had
        // confirmed. It was found by a test whose granted path was a temporary
        // directory — which is to say, by luck.
        let tmp = assembly.join("tmp");
        create_dir(&tmp)?;
        mount_at(
            None,
            &tmp,
            Some("tmpfs"),
            thalyx_syscall::MS_NOSUID | thalyx_syscall::MS_NODEV,
            Some("mode=1777"),
            "mount the module's /tmp",
        )?;

        // `/proc` is mounted by the caller after the pivot: it has to be bound
        // to the module's PID namespace, and only a process inside that
        // namespace can do it. The directory has to exist first, and nothing
        // is ever bound underneath it.
        create_dir(&assembly.join("proc"))?;

        // The module's own tree.
        bind(
            &self.module_dir,
            &assembly.join(MODULE_ROOT.trim_start_matches('/')),
            false,
        )?;

        for entry in &self.binds {
            let target = assembly.join(entry.target.strip_prefix("/").unwrap_or(&entry.target));

            // A writable grant on a directory somebody else owns is the one
            // case a plain bind cannot deliver: the mount would be writable
            // and the module still would not be allowed. Remapping it is what
            // makes the permission the human confirmed actually work.
            match self.remapping_needed_for(entry)? {
                Some((owner_uid, owner_gid)) => bind_remapped(
                    &entry.source,
                    &target,
                    (owner_uid, owner_gid),
                    self.uid,
                    entry.writable,
                )?,
                None => bind(&entry.source, &target, entry.writable)?,
            }
        }

        let old_root = assembly.join(OLD_ROOT.trim_start_matches('/'));
        thalyx_syscall::pivot_root(assembly, &old_root).map_err(|source| {
            SandboxError::MountFailed {
                what: "pivot into the module's root".to_string(),
                source,
            }
        })?;

        // The cwd still points into the old root, which the module must not
        // keep a handle on.
        thalyx_syscall::chdir(Path::new("/")).map_err(|source| SandboxError::MountFailed {
            what: "move out of the old root".to_string(),
            source,
        })?;

        // Detach the host tree. Until this line the module could walk out
        // through `/.old-root`, so it is the step that makes the pivot mean
        // anything.
        thalyx_syscall::umount2(Path::new(OLD_ROOT), thalyx_syscall::MNT_DETACH).map_err(
            |source| SandboxError::MountFailed {
                what: "detach the host filesystem".to_string(),
                source,
            },
        )?;

        std::fs::remove_dir(OLD_ROOT).map_err(|source| SandboxError::io(OLD_ROOT, source))?;

        // Seal the root, and only now: the mount point for the old root had to
        // be removed first, and removing it needs the root still writable.
        //
        // Everything a module may write to is a mount of its own — `/tmp`, and
        // whatever it was granted for writing — so this takes away nothing
        // deliberate. What it takes away is a module filling its root tmpfs,
        // which is the cgroup's memory and therefore the machine's.
        mount_at(
            None,
            Path::new("/"),
            None,
            thalyx_syscall::MS_REMOUNT | thalyx_syscall::MS_RDONLY | thalyx_syscall::MS_BIND,
            None,
            "seal the module's root read-only",
        )?;

        Ok(())
    }
}

impl RootFs {
    /// Whether this bind has to be remapped, and from which owner.
    ///
    /// Every granted path whose owner is not the module's user, readable or
    /// writable. Reads need it as much as writes do: a directory the human
    /// keeps at mode 0700 is unreadable to uid 700000 however clearly the
    /// grant was confirmed.
    ///
    /// The system paths are not remapped — they are world-readable by design,
    /// and remapping each one would cost a helper process and a namespace to
    /// change nothing.
    fn remapping_needed_for(&self, entry: &Bind) -> Result<Option<(u32, u32)>> {
        let Some(uid) = self.uid else {
            return Ok(None);
        };
        if !entry.granted {
            return Ok(None);
        }

        let (owner_uid, owner_gid) = crate::idmap::owner_of(&entry.source)?;
        Ok((owner_uid != uid).then_some((owner_uid, owner_gid)))
    }
}

/// Bind a path with its ownership translated to the module's user.
fn bind_remapped(
    source: &Path,
    target: &Path,
    on_disk: (u32, u32),
    uid: Option<u32>,
    writable: bool,
) -> Result<()> {
    let uid = uid.expect("only called when the module has a user of its own");

    // The same rule as a plain bind, and it used to be a bare `create_dir`.
    // See `create_target_like`: a directory here over a file source is an
    // `EINVAL` from `move_mount` with nothing in it to say what was wrong.
    create_target_like(source, target)?;

    let helper = std::env::current_exe().map_err(|source_error| SandboxError::Exec {
        program: PathBuf::from("<current executable>"),
        source: source_error,
    })?;

    let mapping = crate::idmap::IdMapping::translating(&helper, on_disk, uid)?;
    crate::idmap::bind_idmapped(source, target, &mapping)?;

    // A read grant is still a read grant. Remapping makes the module the
    // apparent owner, which would otherwise hand it write access the human
    // never confirmed.
    if !writable {
        mount_at(
            None,
            target,
            None,
            thalyx_syscall::MS_BIND
                | thalyx_syscall::MS_REC
                | thalyx_syscall::MS_REMOUNT
                | thalyx_syscall::MS_RDONLY,
            None,
            &format!("make the remapped {} read-only", target.display()),
        )?;
    }

    Ok(())
}

/// The path a permission grants, if it grants one.
fn granted_path(permission: &Permission) -> Option<PathBuf> {
    if !permission.resource.starts_with('/') {
        return None;
    }
    match permission.action.as_str() {
        "read" | "write" => Some(PathBuf::from(&permission.resource)),
        _ => None,
    }
}

/// Bind one path into the assembling root.
///
/// Read-only takes two calls. A single `mount(MS_BIND | MS_RDONLY)` silently
/// ignores the read-only flag — the bind inherits the source's writability, and
/// nothing reports a problem. It is the classic way a container ends up with a
/// writable `/usr` that everyone believes is read-only.
fn bind(source: &Path, target: &Path, writable: bool) -> Result<()> {
    create_target_like(source, target)?;

    mount_at(
        Some(source),
        target,
        None,
        thalyx_syscall::MS_BIND | thalyx_syscall::MS_REC,
        None,
        &format!("bind {}", source.display()),
    )?;

    if !writable {
        mount_at(
            Some(source),
            target,
            None,
            thalyx_syscall::MS_BIND
                | thalyx_syscall::MS_REC
                | thalyx_syscall::MS_REMOUNT
                | thalyx_syscall::MS_RDONLY,
            None,
            &format!("make {} read-only", target.display()),
        )?;
    }

    Ok(())
}

fn mount_at(
    source: Option<&Path>,
    target: &Path,
    fstype: Option<&str>,
    flags: u64,
    data: Option<&str>,
    what: &str,
) -> Result<()> {
    thalyx_syscall::mount(source, target, fstype, flags, data).map_err(|source| {
        SandboxError::MountFailed {
            what: what.to_string(),
            source,
        }
    })
}

fn create_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|source| SandboxError::io(path, source))
}

/// Make the mount point a bind of `source` can be attached to.
///
/// A directory for a directory and an empty file for a file, and the kernel is
/// strict about it. `fs/namespace.c`, `do_move_mount`:
///
/// ```c
/// if (d_is_dir(new_path->dentry) != d_is_dir(old_path->dentry))
///     goto out;   /* -EINVAL */
/// ```
///
/// `mount(2)` refuses the same mismatch with `ENOTDIR`, which at least names
/// the problem. The new mount API says `EINVAL` and nothing else.
///
/// This exists as one function because it was two. `bind` handled both kinds
/// and `bind_remapped` created a directory unconditionally, so a **granted
/// path that is a single file** — which every test in this repository happened
/// not to have, and which the greeter has — came apart at the last syscall of
/// the remapped bind, on the machine's own console:
///
/// ```text
/// could not attach the remapped mount at
/// /run/thalyx/sandbox/opt/thalyx/data/greeter/notes.txt: Invalid argument
/// ```
///
/// Two pieces of code that must agree about the same kernel rule, kept apart,
/// stopped agreeing. Now there is one.
fn create_target_like(source: &Path, target: &Path) -> Result<()> {
    let metadata =
        std::fs::metadata(source).map_err(|source_error| SandboxError::io(source, source_error))?;

    if metadata.is_dir() {
        return create_dir(target);
    }

    if let Some(parent) = target.parent() {
        create_dir(parent)?;
    }
    // Truncating an existing one is fine and never loses anything: this is a
    // mount point inside the tmpfs the root is assembled on, and whatever is
    // in it is about to be covered by the bind.
    std::fs::File::create(target).map_err(|source| SandboxError::io(target, source))?;
    Ok(())
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
    fn a_root_contains_the_system_paths_that_exist_and_no_others() {
        let root = RootFs::for_module(Path::new("/opt/thalyx/modules/x/1.0.0"), &[]).unwrap();

        for bind in &root.binds {
            assert!(
                bind.source.exists(),
                "{} was bound and is not there",
                bind.source.display()
            );
            assert!(!bind.writable, "nothing unasked-for should be writable");
        }
        assert!(root.binds.iter().any(|b| b.source == Path::new("/usr")));
    }

    #[test]
    fn a_granted_path_is_bound_with_the_writability_it_was_granted() {
        let dir = tempfile::tempdir().unwrap();
        let readable = dir.path().join("docs");
        let writable = dir.path().join("out");
        std::fs::create_dir(&readable).unwrap();
        std::fs::create_dir(&writable).unwrap();

        let root = RootFs::for_module(
            Path::new("/opt/thalyx/modules/x/1.0.0"),
            &[
                permission(readable.to_str().unwrap(), "read"),
                permission(writable.to_str().unwrap(), "write"),
            ],
        )
        .unwrap();

        let read_bind = root.binds.iter().find(|b| b.source == readable).unwrap();
        assert!(!read_bind.writable);
        let write_bind = root.binds.iter().find(|b| b.source == writable).unwrap();
        assert!(write_bind.writable);
    }

    #[test]
    fn two_grants_on_one_path_end_up_as_wide_as_the_wider_one() {
        // Both were confirmed by the human. Binding read-only because the read
        // grant was seen first would deny something they said yes to.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared");
        std::fs::create_dir(&path).unwrap();
        let as_str = path.to_str().unwrap();

        let root = RootFs::for_module(
            Path::new("/opt/thalyx/modules/x/1.0.0"),
            &[permission(as_str, "read"), permission(as_str, "write")],
        )
        .unwrap();

        let binds: Vec<_> = root.binds.iter().filter(|b| b.source == path).collect();
        assert_eq!(binds.len(), 1, "the path should be bound once");
        assert!(binds[0].writable);
    }

    #[test]
    fn a_grant_on_a_path_that_does_not_exist_is_refused() {
        // The module would run holding a grant it cannot use, and nothing would
        // say so. Refusing names the path the human has to fix.
        let error = RootFs::for_module(
            Path::new("/opt/thalyx/modules/x/1.0.0"),
            &[permission("/definitely/not/here", "read")],
        )
        .unwrap_err();

        assert!(matches!(error, SandboxError::GrantedPathMissing { .. }));
        assert!(error.to_string().contains("/definitely/not/here"));
    }

    #[test]
    fn a_network_permission_binds_nothing() {
        let root = RootFs::for_module(
            Path::new("/opt/thalyx/modules/x/1.0.0"),
            &[permission("net", "outbound")],
        )
        .unwrap();
        assert!(!root.binds.iter().any(|b| b.source == Path::new("net")));
    }

    #[test]
    fn the_entrypoint_is_rewritten_to_where_the_module_actually_is() {
        let root = RootFs::for_module(Path::new("/opt/thalyx/modules/x/1.0.0"), &[]).unwrap();
        assert_eq!(
            root.program_inside(Path::new("/opt/thalyx/modules/x/1.0.0/bin/demo"))
                .unwrap(),
            Path::new("/module/bin/demo")
        );
    }

    #[test]
    fn a_program_outside_the_module_tree_has_no_place_inside_and_is_refused() {
        let root = RootFs::for_module(Path::new("/opt/thalyx/modules/x/1.0.0"), &[]).unwrap();
        assert!(matches!(
            root.program_inside(Path::new("/bin/sh")),
            Err(SandboxError::EntrypointEscapes(_))
        ));
    }

    #[test]
    fn a_root_survives_the_trip_through_the_re_execution() {
        let root = RootFs::for_module(Path::new("/opt/thalyx/modules/x/1.0.0"), &[]).unwrap();
        let encoded = serde_json::to_string(&root).unwrap();
        let decoded: RootFs = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, root);
    }

    /// A mount point for a file is a file, and the kernel will not take a
    /// directory instead.
    ///
    /// `do_move_mount` refuses the mismatch with a bare `EINVAL`, so this is
    /// the difference between a granted file working and a message that names
    /// a syscall and nothing about the cause. The remapped bind got it wrong
    /// for as long as the remapped bind existed, and nothing noticed because
    /// every granted path anyone had tested was a directory.
    #[test]
    fn a_mount_point_for_a_file_is_a_file_and_one_for_a_directory_is_a_directory() {
        let scratch = tempfile::tempdir().unwrap();

        let file_source = scratch.path().join("notes.txt");
        std::fs::write(&file_source, "granted\n").unwrap();
        let file_target = scratch.path().join("assembly/data/notes.txt");
        create_target_like(&file_source, &file_target).unwrap();
        assert!(
            file_target.is_file(),
            "a file source got a directory mount point, which move_mount refuses"
        );

        let dir_source = scratch.path().join("docs");
        std::fs::create_dir(&dir_source).unwrap();
        let dir_target = scratch.path().join("assembly/docs");
        create_target_like(&dir_source, &dir_target).unwrap();
        assert!(dir_target.is_dir(), "a directory source got a file");
    }

    /// The parents are made, so a grant several directories deep works.
    ///
    /// `/opt/thalyx/data/greeter/notes.txt` is four levels below the assembly
    /// root and none of them exist in it.
    #[test]
    fn the_directories_above_a_granted_file_are_made_on_the_way_to_it() {
        let scratch = tempfile::tempdir().unwrap();
        let source = scratch.path().join("notes.txt");
        std::fs::write(&source, "granted\n").unwrap();

        let target = scratch
            .path()
            .join("assembly/opt/thalyx/data/greeter/notes.txt");
        create_target_like(&source, &target).unwrap();
        assert!(target.is_file());
    }

    /// A source that is not there is an error, not an empty mount point.
    ///
    /// Without this the bind would be attempted against nothing and fail one
    /// syscall later, naming the mount instead of the missing path.
    #[test]
    fn a_source_that_is_not_there_is_refused_before_anything_is_created() {
        let scratch = tempfile::tempdir().unwrap();
        let target = scratch.path().join("assembly/absent");
        assert!(create_target_like(&scratch.path().join("absent"), &target).is_err());
        assert!(!target.exists());
    }
}
