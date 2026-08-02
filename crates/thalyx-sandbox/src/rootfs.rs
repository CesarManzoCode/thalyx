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
}

/// What the module's root filesystem is made of.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RootFs {
    /// The module's own tree, mounted read-only at [`MODULE_ROOT`].
    pub module_dir: PathBuf,
    /// System paths, granted paths and device nodes.
    pub binds: Vec<Bind>,
}

impl RootFs {
    /// Assemble the description of a root for a module and its grants.
    ///
    /// A granted path that does not exist is a refusal, not a shrug. The
    /// alternative is a module running with a grant the human confirmed and it
    /// cannot use — the same "promise the system cannot keep" this project
    /// refuses everywhere else, only mirrored.
    pub fn for_module(module_dir: &Path, permissions: &[Permission]) -> Result<Self> {
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
                }),
            }
        }

        Ok(Self {
            module_dir: module_dir.to_path_buf(),
            binds,
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
            bind(&entry.source, &target, entry.writable)?;
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
    let metadata =
        std::fs::metadata(source).map_err(|source_error| SandboxError::io(source, source_error))?;

    if metadata.is_dir() {
        create_dir(target)?;
    } else {
        if let Some(parent) = target.parent() {
            create_dir(parent)?;
        }
        std::fs::File::create(target).map_err(|e| SandboxError::io(target, e))?;
    }

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
}
