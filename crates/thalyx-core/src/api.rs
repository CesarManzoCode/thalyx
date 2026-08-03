//! Answering a module, with the manifest's permissions in hand.
//!
//! This is Thalyx's end of the channel decreed in
//! `vault/02-Arquitectura/API-Interna-de-Modulos.md`. The module asks; this
//! decides.
//!
//! ## Why this checks, when the sandbox already does
//!
//! The module runs inside a root that contains only what it was granted, and
//! the LSM refuses opens it was not told to allow. Neither of those protects
//! anything here, and the reason is worth stating plainly: **this code is not
//! inside the sandbox.** It runs as Thalyx, outside the module's namespaces,
//! outside its cgroup, and with Thalyx's own reach. A module that asks for a
//! path is asking *Thalyx* to open it.
//!
//! So the confinement is not a reason to relax; it is the reason this exists.
//! Every path a module names is checked against the grants before anything is
//! opened, and checked again after the kernel has resolved it, because the two
//! answers are not the same when a symlink is in the way.
//!
//! ## What a module cannot do through this
//!
//! Reach outside its grants by any spelling. Not with `..`, not with an
//! absolute path that merely starts with the right letters, and not with a
//! symlink planted inside a directory it may write. The last of those is the
//! one that would actually have worked, and it is the one this file exists for.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use thalyx_abi::{Denial, Failure, Grant, Handler, Identity, Level, Request, Response};

/// Thalyx, from a module's point of view.
pub struct ModuleApi {
    identity: Identity,
    /// Only the grants that name a path. `net` is enforced in the kernel by
    /// `thalyx-lsm` and has nothing to answer here — a module cannot even build
    /// a socket to be denied on, because `socket` is off the seccomp allowlist.
    paths: Vec<PathGrant>,
    /// What the module said to the human, in order, so a caller can show it.
    said: Vec<(Level, String)>,
}

struct PathGrant {
    /// As written in the manifest.
    root: PathBuf,
    /// As the kernel resolves it. Held separately because a grant naming a
    /// symlinked directory would otherwise never match the resolved form of
    /// anything inside it.
    resolved: Option<PathBuf>,
    action: String,
}

impl ModuleApi {
    pub fn for_module(
        manifest: &thalyx_manifest::Manifest,
        permissions: &[thalyx_manifest::Permission],
    ) -> Self {
        let paths = permissions
            .iter()
            .filter(|permission| permission.resource.starts_with('/'))
            .filter(|permission| permission.action == "read" || permission.action == "write")
            .map(|permission| {
                let root = PathBuf::from(&permission.resource);
                PathGrant {
                    resolved: std::fs::canonicalize(&root).ok(),
                    root,
                    action: permission.action.clone(),
                }
            })
            .collect();

        Self {
            identity: Identity {
                protocol: thalyx_abi::PROTOCOL_VERSION,
                module_id: manifest.id.clone(),
                version: manifest.version.clone(),
                grants: permissions
                    .iter()
                    .map(|permission| Grant {
                        resource: permission.resource.clone(),
                        action: permission.action.clone(),
                        expires_unix: None,
                    })
                    .collect(),
            },
            paths,
            said: Vec::new(),
        }
    }

    /// Everything the module told the human, in the order it said it.
    pub fn said(&self) -> &[(Level, String)] {
        &self.said
    }

    /// Turn a path a module named into one Thalyx is willing to open.
    ///
    /// Two checks, and both are needed. The first is on the name: it must be
    /// absolute, free of `..`, and under a grant. The second is on what the
    /// kernel says that name *is*, which is the only one that catches a symlink
    /// pointing out of the granted tree — and a module that may write inside
    /// its own grant can create one whenever it likes.
    fn permit(&self, named: &str, action: &str) -> Result<PathBuf, Denial> {
        let named = Path::new(named);
        if !named.is_absolute() {
            return Err(Denial::NotGranted);
        }

        // `..` is refused rather than resolved. Resolving it here would mean
        // this code and the kernel each computing what the path means, and the
        // gap between two answers to that question is where escapes live.
        if named
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(Denial::NotGranted);
        }

        let mut wrong_action = false;
        for grant in &self.paths {
            if !under(named, &grant.root) {
                continue;
            }
            if grant.action != action && !(grant.action == "write" && action == "read") {
                // A write grant carries read: a module that may replace a file
                // and cannot look at it first is a module that can only
                // clobber. The reverse does not hold.
                wrong_action = true;
                continue;
            }

            // What the kernel makes of it. For a write the file may not exist
            // yet, so the parent is what gets resolved — the directory has to
            // be inside the grant either way, and a symlinked parent is the
            // same escape by a different door.
            let resolved = match std::fs::canonicalize(named) {
                Ok(real) => Some(real),
                Err(_) => named
                    .parent()
                    .and_then(|parent| std::fs::canonicalize(parent).ok())
                    .map(|parent| match named.file_name() {
                        Some(name) => parent.join(name),
                        None => parent,
                    }),
            };

            match (resolved, &grant.resolved) {
                // Both sides resolved: this is the check that matters.
                (Some(real), Some(root)) if under(&real, root) => return Ok(real),
                (Some(_), Some(_)) => return Err(Denial::NotGranted),
                // Either the target or the grant could not be resolved. That is
                // not permission to guess: without both, there is no way to
                // know whether the name leaves the tree, and the cautious
                // answer is the only honest one.
                _ => return Err(Denial::NotGranted),
            }
        }

        if wrong_action {
            return Err(Denial::WrongAction);
        }
        Err(Denial::NotGranted)
    }
}

/// Is `path` inside `root`, counting components rather than characters?
///
/// `starts_with` on the strings would put `/home/user/projects-old` inside a
/// grant for `/home/user/projects`, which is a different directory belonging to
/// somebody who never agreed to anything.
fn under(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

impl Handler for ModuleApi {
    fn handle(&mut self, request: Request) -> Response {
        match request {
            Request::Identify => Response::Identity(self.identity.clone()),

            Request::ReadFile { path, offset, len } => {
                if len > thalyx_abi::MAX_READ {
                    return Response::Failed {
                        kind: Failure::TooLarge,
                        detail: format!(
                            "{len} bytes asked for; the channel carries at most {}",
                            thalyx_abi::MAX_READ
                        ),
                    };
                }
                let real = match self.permit(&path, "read") {
                    Ok(real) => real,
                    Err(reason) => return Response::Denied { reason },
                };

                let mut file = match std::fs::File::open(&real) {
                    Ok(file) => file,
                    Err(error) => return failure_from(error, &path),
                };
                if let Err(error) = file.seek(SeekFrom::Start(offset)) {
                    return failure_from(error, &path);
                }

                let mut bytes = vec![0u8; len as usize];
                let mut filled = 0;
                while filled < bytes.len() {
                    match file.read(&mut bytes[filled..]) {
                        Ok(0) => break,
                        Ok(count) => filled += count,
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(error) => return failure_from(error, &path),
                    }
                }
                bytes.truncate(filled);

                // End of file, established from the file's own length rather
                // than from a short read. A read can come back short for
                // reasons that have nothing to do with the end, and a module
                // that stopped on one would silently truncate what it read.
                let eof = match file.metadata() {
                    Ok(metadata) => offset.saturating_add(filled as u64) >= metadata.len(),
                    // Unknown is not "yes". Saying the file ended when nobody
                    // checked is the one answer that loses data quietly.
                    Err(_) => false,
                };

                Response::Contents { bytes, eof }
            }

            Request::WriteFile {
                path,
                offset,
                bytes,
            } => {
                let real = match self.permit(&path, "write") {
                    Ok(real) => real,
                    Err(reason) => return Response::Denied { reason },
                };

                let mut file = match std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&real)
                {
                    Ok(file) => file,
                    Err(error) => return failure_from(error, &path),
                };
                if let Err(error) = file.seek(SeekFrom::Start(offset)) {
                    return failure_from(error, &path);
                }
                match file.write_all(&bytes) {
                    Ok(()) => Response::Written {
                        bytes: bytes.len() as u32,
                    },
                    Err(error) => failure_from(error, &path),
                }
            }

            Request::Notify { level, text } => {
                self.said.push((level, text));
                Response::Noted
            }
        }
    }
}

/// Turn an I/O error into the answer that says which thing went wrong.
///
/// Not there, versus there and unreadable. Collapsing them would leave a module
/// unable to tell a typo from a broken disk.
fn failure_from(error: std::io::Error, path: &str) -> Response {
    let kind = match error.kind() {
        std::io::ErrorKind::NotFound => Failure::NotFound,
        std::io::ErrorKind::PermissionDenied => Failure::Unreadable,
        _ => Failure::Unreadable,
    };
    Response::Failed {
        kind,
        detail: format!("{path}: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_manifest::{Permission, PermissionKind};

    fn manifest() -> thalyx_manifest::Manifest {
        thalyx_manifest::Manifest::parse(
            r#"
format_version = 1
id = "dev.thalyx.demo"
name = "Demo"
version = "1.0.0"
description = "for tests"
license = "GPL-3.0-or-later"
publisher_key = "ed25519:3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29"
distribution = "prebuilt"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 1

[requires]
thalyx = "^1.0"

[entrypoints]
run = "bin/demo"
"#,
        )
        .expect("a manifest the tests can lean on")
    }

    fn api(grants: Vec<(&str, &str)>) -> ModuleApi {
        let permissions: Vec<Permission> = grants
            .into_iter()
            .map(|(resource, action)| Permission {
                resource: resource.to_string(),
                action: action.to_string(),
                kind: PermissionKind::Persistent,
            })
            .collect();
        ModuleApi::for_module(&manifest(), &permissions)
    }

    #[test]
    fn a_module_is_told_who_it_is_and_what_it_may_do() {
        let mut api = api(vec![("/tmp", "read")]);
        match api.handle(Request::Identify) {
            Response::Identity(identity) => {
                assert_eq!(identity.module_id, "dev.thalyx.demo");
                assert_eq!(identity.grants.len(), 1);
            }
            other => panic!("expected an identity, got {other:?}"),
        }
    }

    #[test]
    fn a_granted_file_can_be_read() {
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::write(granted.join("notes.txt"), b"the vault is the authority")
            .expect("a file to read");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::ReadFile {
            path: granted.join("notes.txt").to_str().unwrap().to_string(),
            offset: 0,
            len: 128,
        });

        match answer {
            Response::Contents { bytes, eof } => {
                assert_eq!(bytes, b"the vault is the authority");
                assert!(eof, "the whole file was read, so it ended");
            }
            other => panic!("expected contents, got {other:?}"),
        }
    }

    #[test]
    fn a_file_outside_every_grant_is_refused() {
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        let secret = home.path().join("secret.txt");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::write(&secret, b"not for the module").expect("a file to protect");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::ReadFile {
            path: secret.to_str().unwrap().to_string(),
            offset: 0,
            len: 128,
        });

        assert_eq!(
            answer,
            Response::Denied {
                reason: Denial::NotGranted
            }
        );
    }

    #[test]
    fn a_sibling_directory_sharing_a_prefix_is_not_inside_the_grant() {
        // `/x/projects-old` starts with `/x/projects` as text and is a
        // different directory. A string comparison here would hand a module
        // somebody else's files.
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("projects");
        let sibling = home.path().join("projects-old");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::create_dir(&sibling).expect("the sibling");
        std::fs::write(sibling.join("secret.txt"), b"not yours").expect("a file");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::ReadFile {
            path: sibling.join("secret.txt").to_str().unwrap().to_string(),
            offset: 0,
            len: 16,
        });

        assert_eq!(
            answer,
            Response::Denied {
                reason: Denial::NotGranted
            }
        );
    }

    #[test]
    fn a_path_climbing_out_with_dotdot_is_refused() {
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::write(home.path().join("secret.txt"), b"not yours").expect("a file");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::ReadFile {
            path: granted.join("../secret.txt").to_str().unwrap().to_string(),
            offset: 0,
            len: 16,
        });

        assert_eq!(
            answer,
            Response::Denied {
                reason: Denial::NotGranted
            }
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_planted_inside_the_grant_does_not_reach_outside_it() {
        // The one that would actually have worked. Thalyx is not inside the
        // sandbox, so it opens with its own reach; a module that may write in
        // its granted directory can leave a symlink there and then ask Thalyx
        // to follow it.
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        let secret = home.path().join("secret.txt");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::write(&secret, b"not for the module").expect("a file to protect");
        std::os::unix::fs::symlink(&secret, granted.join("escape")).expect("the symlink");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::ReadFile {
            path: granted.join("escape").to_str().unwrap().to_string(),
            offset: 0,
            len: 128,
        });

        assert_eq!(
            answer,
            Response::Denied {
                reason: Denial::NotGranted
            },
            "a symlink out of the granted tree was followed"
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_that_stays_inside_the_grant_still_works() {
        // The control. Without it, refusing every symlink would pass the test
        // above and break something legitimate — and the two look identical
        // from the outside.
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::write(granted.join("real.txt"), b"inside").expect("a file");
        std::os::unix::fs::symlink(granted.join("real.txt"), granted.join("link"))
            .expect("the symlink");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::ReadFile {
            path: granted.join("link").to_str().unwrap().to_string(),
            offset: 0,
            len: 128,
        });

        match answer {
            Response::Contents { bytes, .. } => assert_eq!(bytes, b"inside"),
            other => panic!("a symlink inside the grant was refused: {other:?}"),
        }
    }

    #[test]
    fn a_read_grant_does_not_carry_writing() {
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::write(granted.join("notes.txt"), b"original").expect("a file");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::WriteFile {
            path: granted.join("notes.txt").to_str().unwrap().to_string(),
            offset: 0,
            bytes: b"replaced".to_vec(),
        });

        assert_eq!(
            answer,
            Response::Denied {
                reason: Denial::WrongAction
            }
        );
        assert_eq!(
            std::fs::read(granted.join("notes.txt")).expect("reading back"),
            b"original",
            "the file changed despite the refusal"
        );
    }

    #[test]
    fn a_write_grant_carries_reading_because_replacing_blind_is_not_writing() {
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::write(granted.join("log.txt"), b"so far").expect("a file");

        let mut api = api(vec![(granted.to_str().unwrap(), "write")]);
        match api.handle(Request::ReadFile {
            path: granted.join("log.txt").to_str().unwrap().to_string(),
            offset: 0,
            len: 32,
        }) {
            Response::Contents { bytes, .. } => assert_eq!(bytes, b"so far"),
            other => panic!("expected contents, got {other:?}"),
        }
    }

    #[test]
    fn a_granted_write_lands_on_the_disk() {
        // Asking the system whether it worked proves nothing: the file is read
        // back from outside the API.
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        std::fs::create_dir(&granted).expect("the granted directory");

        let mut api = api(vec![(granted.to_str().unwrap(), "write")]);
        let answer = api.handle(Request::WriteFile {
            path: granted.join("out.txt").to_str().unwrap().to_string(),
            offset: 0,
            bytes: b"written by a module".to_vec(),
        });

        assert_eq!(answer, Response::Written { bytes: 19 });
        assert_eq!(
            std::fs::read(granted.join("out.txt")).expect("reading it back"),
            b"written by a module"
        );
    }

    #[test]
    fn a_permitted_file_that_is_missing_fails_rather_than_being_denied() {
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        std::fs::create_dir(&granted).expect("the granted directory");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::ReadFile {
            path: granted.join("absent.txt").to_str().unwrap().to_string(),
            offset: 0,
            len: 16,
        });

        // The name is inside the grant, so this is not a refusal — it is a
        // file that is not there, and the two are different facts.
        match answer {
            Response::Failed { kind, .. } => assert_eq!(kind, Failure::NotFound),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn a_module_with_no_grants_at_all_can_still_say_who_it_is() {
        let mut api = api(vec![]);
        assert!(matches!(
            api.handle(Request::Identify),
            Response::Identity(_)
        ));
        assert_eq!(
            api.handle(Request::ReadFile {
                path: "/etc/shadow".to_string(),
                offset: 0,
                len: 1,
            }),
            Response::Denied {
                reason: Denial::NotGranted
            }
        );
    }

    #[test]
    fn what_a_module_says_to_the_human_is_kept_in_order() {
        let mut api = api(vec![]);
        api.handle(Request::Notify {
            level: Level::Info,
            text: "starting".to_string(),
        });
        api.handle(Request::Notify {
            level: Level::Warning,
            text: "nearly done".to_string(),
        });

        assert_eq!(
            api.said(),
            &[
                (Level::Info, "starting".to_string()),
                (Level::Warning, "nearly done".to_string()),
            ]
        );
    }
}
