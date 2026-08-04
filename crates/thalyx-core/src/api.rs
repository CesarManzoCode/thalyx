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
//!
//! ## Why the containment check *is* the open
//!
//! The first version of this resolved the path with `canonicalize`, compared
//! the result against the grant, and then opened the resolved path. Every one
//! of those steps was right and the sequence was still wrong, because it is a
//! sequence: between the comparison and the open there is a moment, and a
//! module that may write inside its own grant can spend that moment replacing
//! the name it just asked about with a symlink to somewhere else. Thalyx would
//! then open the new target — with Thalyx's reach, not the module's, since
//! this code runs outside the sandbox on purpose.
//!
//! Nothing checked in userspace can close that. The check and the open have to
//! be the same syscall, so they are: `openat2` with `RESOLVE_BENEATH`, against
//! a descriptor for the granted directory opened once, at startup. The kernel
//! then refuses to resolve out of that directory *during* resolution — there
//! is no intermediate answer for anybody to invalidate.
//!
//! Holding the directory open has a second effect worth naming: the grant is
//! pinned to the directory the human authorised, not to its name. Renaming or
//! replacing that directory afterwards does not silently redirect the grant.
//!
//! ## What that costs, said plainly
//!
//! `RESOLVE_BENEATH` refuses **every absolute symlink**, including one that
//! would have landed inside the grant. A granted directory holding a link to
//! `/home/user/docs/notes.txt` — an absolute path naming a file inside the
//! same grant — is now refused where it used to be followed.
//!
//! That is a real narrowing and it is deliberate. An absolute symlink is
//! resolved against the host's root, and deciding whether it lands inside the
//! grant means resolving it in userspace first — which is the two-step check
//! this whole section exists to get rid of. Relative symlinks inside the grant
//! keep working, because the kernel can contain those itself.
//!
//! The direction of the loss is the one to accept: a module is refused
//! something it should have been allowed, which somebody notices and reports,
//! rather than allowed something it should have been refused, which nobody
//! notices at all.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::OwnedFd;
use std::path::{Component, Path, PathBuf};

use thalyx_abi::{Denial, Failure, Grant, Handler, Identity, Level, Request, Response};

/// How much a module may pile up on the human's behalf, per run.
///
/// A `Notify` is kept so the caller can show it when the module exits, which
/// means the module chooses how much memory Thalyx holds. The frame limit caps
/// one message; without a cap on the *count* a module could send a million of
/// them. Its own cgroup would not notice — the memory grows in Thalyx, which
/// inside the image is pid 1.
///
/// Past the limit, notices are counted and dropped rather than the channel
/// being torn down: a module that says too much is being tiresome, not hostile,
/// and killing its channel would lose the work it had already done.
pub const MAX_NOTICES: usize = 256;

/// And how much text, across all of them.
///
/// Separate from the count because 256 messages of a megabyte each is the same
/// attack with fewer steps.
pub const MAX_NOTICE_BYTES: usize = 64 * 1024;

/// Thalyx, from a module's point of view.
pub struct ModuleApi {
    identity: Identity,
    /// Only the grants that name a path. `net` is enforced in the kernel by
    /// `thalyx-lsm` and has nothing to answer here.
    paths: Vec<PathGrant>,
    /// What the module said to the human, in order, so a caller can show it.
    said: Vec<(Level, String)>,
    /// How many notices were refused after the ceiling, so the caller can say
    /// so rather than quietly showing a truncated list.
    dropped_notices: usize,
    notice_bytes: usize,
}

struct PathGrant {
    /// As written in the manifest, for matching the name a module asks about.
    root: PathBuf,
    /// The directory every request under this grant resolves beneath, held
    /// open from startup.
    ///
    /// For a grant on a directory this *is* that directory. For a grant on a
    /// single file it is the file's parent, because a file cannot be a
    /// resolution base — and then [`PathGrant::confined`] does the narrowing
    /// that `RESOLVE_BENEATH` alone would not, since beneath the parent lies
    /// every sibling the human did not grant.
    ///
    /// `None` when it could not be opened: a grant on a path that does not
    /// exist. That is not permission to guess. Without a descriptor there is
    /// nothing to resolve beneath, so every request under it is refused.
    base: Option<OwnedFd>,
    /// The path `base` was opened from, which is what a request is made
    /// relative to.
    base_path: PathBuf,
    /// Whether the grant names one file rather than a tree.
    single_file: bool,
    action: String,
}

impl PathGrant {
    /// Whether `path` is inside what this grant actually covers.
    ///
    /// The check `RESOLVE_BENEATH` cannot make on its own. When the grant is a
    /// whole directory the kernel's containment is the whole answer; when it
    /// is one file, the base is that file's parent and the kernel would just
    /// as happily resolve the file next to it.
    fn confined(&self, path: &Path) -> bool {
        if self.single_file {
            path == self.root
        } else {
            under(path, &self.root)
        }
    }
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

                // A grant on a file resolves beneath the file's parent, since
                // `openat2` needs a directory to start from. Deciding it once,
                // here, rather than per request: the answer is a property of
                // the grant, and re-deciding it every time would be one more
                // thing that could change between two requests.
                let single_file = root.is_file();
                let base_path = if single_file {
                    root.parent().unwrap_or(&root).to_path_buf()
                } else {
                    root.clone()
                };

                PathGrant {
                    // Opened once, here, and never by name again. Every later
                    // request resolves beneath this descriptor.
                    base: open_directory(&base_path),
                    base_path,
                    single_file,
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
            dropped_notices: 0,
            notice_bytes: 0,
        }
    }

    /// Everything the module told the human, in the order it said it.
    pub fn said(&self) -> &[(Level, String)] {
        &self.said
    }

    /// How many notices were refused for being past the ceiling.
    ///
    /// Reported rather than swallowed. A list that silently stopped growing
    /// looks exactly like a module that stopped talking, and those are
    /// different events.
    pub fn dropped_notices(&self) -> usize {
        self.dropped_notices
    }

    /// Find the grant a named path falls under, and open the path beneath it.
    ///
    /// The name is checked first — absolute, free of `..`, under a grant — and
    /// then handed to the kernel as a path *relative to the granted directory*
    /// with `RESOLVE_BENEATH`. The first check decides which grant applies; the
    /// second is the one that cannot be raced, because it is the open.
    fn open_permitted(
        &self,
        named: &str,
        action: &str,
        flags: i32,
        mode: u32,
    ) -> Result<std::result::Result<std::fs::File, std::io::Error>, Denial> {
        let path = Path::new(named);
        if !path.is_absolute() {
            return Err(Denial::NotGranted);
        }

        // `..` is refused rather than resolved. `RESOLVE_BENEATH` would refuse
        // it too, but refusing here keeps the answer the same on every kernel
        // and makes the reason legible in the denial.
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(Denial::NotGranted);
        }

        let mut wrong_action = false;
        for grant in &self.paths {
            if !grant.confined(path) {
                continue;
            }
            if grant.action != action && !(grant.action == "write" && action == "read") {
                // A write grant carries read: a module that may replace a file
                // and cannot look at it first is a module that can only
                // clobber. The reverse does not hold.
                wrong_action = true;
                continue;
            }

            // A grant whose base could not be opened holds nothing. There is
            // no root to resolve beneath, and inventing one is the guess this
            // whole module refuses to make.
            let Some(base) = &grant.base else {
                return Err(Denial::NotGranted);
            };

            // Relative to the base, because that is what `openat2` resolves
            // against. An empty remainder means the request names the base
            // itself, which only happens for a grant on a directory a module
            // asked to open as a file — and there is nothing to read there.
            let relative = match path.strip_prefix(&grant.base_path) {
                Ok(rest) if rest.as_os_str().is_empty() => Path::new("."),
                Ok(rest) => rest,
                // Cannot happen: `confined` already established containment.
                // Refused rather than unwrapped, because a panic in the code
                // answering a module is a denial of service a module can ask
                // for.
                Err(_) => return Err(Denial::NotGranted),
            };

            use std::os::fd::AsFd;
            return Ok(thalyx_syscall::open_beneath(
                base.as_fd(),
                relative,
                flags,
                mode,
                thalyx_syscall::RESOLVE_BENEATH | thalyx_syscall::RESOLVE_NO_MAGICLINKS,
            ));
        }

        if wrong_action {
            return Err(Denial::WrongAction);
        }
        Err(Denial::NotGranted)
    }
}

/// Open a directory for resolution only, or answer that it could not be opened.
///
/// `O_PATH` because nothing is ever read or written through this descriptor —
/// it exists to be the root that `openat2` resolves beneath. A grant on a file
/// rather than a directory opens too, and `RESOLVE_BENEATH` then permits only
/// the file itself, which is exactly the grant the human gave.
fn open_directory(path: &Path) -> Option<OwnedFd> {
    // `File::open` on a directory works and yields a descriptor `openat2`
    // accepts as a resolution base. `O_PATH` would be the tighter choice and
    // the standard library does not expose it; nothing ever reads through this
    // descriptor, so the difference is not one a module can reach.
    //
    // The conversion is safe — `File` owns the descriptor and hands ownership
    // over — which matters here, because this crate forbids `unsafe` and the
    // raw-descriptor spelling of the same thing would need it.
    std::fs::File::open(path).ok().map(OwnedFd::from)
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
                let opened = match self.open_permitted(&path, "read", O_RDONLY, NO_MODE) {
                    Ok(opened) => opened,
                    Err(reason) => return Response::Denied { reason },
                };
                let mut file = match opened {
                    Ok(file) => file,
                    Err(error) => return answer_for(error, &path),
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
                let opened = match self.open_permitted(&path, "write", O_WRONLY | O_CREAT, 0o600) {
                    Ok(opened) => opened,
                    Err(reason) => return Response::Denied { reason },
                };
                let mut file = match opened {
                    Ok(file) => file,
                    Err(error) => return answer_for(error, &path),
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

            // Bounded, because this is the one request that makes Thalyx hold
            // something on the module's say-so.
            //
            // Every other request is answered and forgotten. A notice is kept
            // until the run ends so the human can be shown it, which means an
            // unbounded stream of them is an unbounded allocation in the
            // process that is doing the confining — and in the image, in pid 1.
            // The frame limit caps one message at a megabyte and says nothing
            // about how many.
            //
            // Refused rather than fatal: still `Noted`, because the module did
            // nothing wrong by being verbose and tearing down its channel would
            // lose whatever it had already accomplished. The count is reported
            // separately so nobody mistakes a truncated list for a quiet module.
            Request::Notify { level, text } => {
                if self.said.len() >= MAX_NOTICES
                    || self.notice_bytes.saturating_add(text.len()) > MAX_NOTICE_BYTES
                {
                    self.dropped_notices += 1;
                    return Response::Noted;
                }
                self.notice_bytes += text.len();
                self.said.push((level, text));
                Response::Noted
            }
        }
    }
}

/// Open flags, spelled out rather than pulled from `libc`.
///
/// This crate does not depend on the C library and should not: the point of
/// `thalyx-syscall` is that the rest of the workspace does not talk to it
/// directly. These three numbers are ABI on every Linux architecture Thalyx
/// targets.
///
/// No `O_TRUNC` on the write path: a write at an offset must not discard what
/// is past it.
const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 0o1;
const O_CREAT: i32 = 0o100;

/// The mode argument for an open that cannot create anything.
///
/// It must be zero, and that is not a convention — `openat2` rejects a
/// non-zero mode without `O_CREAT` with `EINVAL`, where the older `openat`
/// silently ignored it. The stricter check is the whole reason to prefer the
/// newer call, so it is worth being caught by it once.
const NO_MODE: u32 = 0;

/// Turn a failed *open* into the answer that says what actually happened.
///
/// The distinction this makes, and why it is not cosmetic: when the kernel
/// refuses an open because resolution left the granted directory, that is a
/// **denial**, not a failure. It is the same event as Thalyx refusing the name
/// itself — the kernel simply caught it, at the only moment it could be caught
/// without a race — and reporting it as "unreadable" would tell a module its
/// disk was broken when what happened is that it tried to escape.
///
/// `EXDEV` is what `RESOLVE_BENEATH` returns for a path or symlink that leaves
/// the base. `ELOOP` is what `RESOLVE_NO_MAGICLINKS` returns for a magic link,
/// and for an ordinary symlink loop, which is also not something to allow.
fn answer_for(error: std::io::Error, path: &str) -> Response {
    const EXDEV: i32 = 18;
    const ELOOP: i32 = 40;

    match error.raw_os_error() {
        Some(EXDEV) | Some(ELOOP) => Response::Denied {
            reason: Denial::NotGranted,
        },
        _ => failure_from(error, path),
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
    fn a_relative_symlink_that_stays_inside_the_grant_still_works() {
        // The control. Without it, refusing every symlink would pass the test
        // above and break something legitimate — and the two look identical
        // from the outside.
        //
        // Relative, and that is the whole content of the next test: the kernel
        // can contain a relative link by itself, so this one is followed.
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::write(granted.join("real.txt"), b"inside").expect("a file");
        std::os::unix::fs::symlink("real.txt", granted.join("link")).expect("the symlink");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::ReadFile {
            path: granted.join("link").to_str().unwrap().to_string(),
            offset: 0,
            len: 128,
        });

        match answer {
            Response::Contents { bytes, .. } => assert_eq!(bytes, b"inside"),
            other => panic!("a relative symlink inside the grant was refused: {other:?}"),
        }
    }

    #[test]
    #[cfg(unix)]
    fn an_absolute_symlink_is_refused_even_when_it_points_inside_the_grant() {
        // Not a bug being pinned down — a cost being named.
        //
        // `RESOLVE_BENEATH` refuses every absolute symlink, because an absolute
        // path is resolved against the host root and the kernel will not guess
        // that this particular one happens to land back inside. Working out
        // that it does would mean resolving it in userspace first, which is
        // exactly the two-step check that made this file racy.
        //
        // So this is refused, and it is the *legitimate* case being refused.
        // Written down as a test so that the day somebody wonders why their
        // link stopped working, the answer is in the suite rather than in
        // somebody's memory.
        let home = tempfile::tempdir().expect("a temporary directory");
        let granted = home.path().join("granted");
        std::fs::create_dir(&granted).expect("the granted directory");
        std::fs::write(granted.join("real.txt"), b"inside").expect("a file");
        std::os::unix::fs::symlink(granted.join("real.txt"), granted.join("absolute"))
            .expect("the symlink");

        let mut api = api(vec![(granted.to_str().unwrap(), "read")]);
        let answer = api.handle(Request::ReadFile {
            path: granted.join("absolute").to_str().unwrap().to_string(),
            offset: 0,
            len: 128,
        });

        assert_eq!(
            answer,
            Response::Denied {
                reason: Denial::NotGranted
            },
            "an absolute symlink is refused as a denial, not reported as a broken disk"
        );
    }

    #[test]
    fn a_module_cannot_make_thalyx_hold_an_unbounded_amount_of_its_talking() {
        // The module chooses how many notices it sends; Thalyx keeps them all
        // until the run ends. Without a ceiling that is an allocation the
        // module controls, growing inside the process doing the confining —
        // which in the image is pid 1, and its own cgroup limit never sees it.
        let mut api = api(vec![]);

        for index in 0..(MAX_NOTICES * 2) {
            assert_eq!(
                api.handle(Request::Notify {
                    level: Level::Info,
                    text: format!("notice {index}"),
                }),
                Response::Noted,
                "a module past the ceiling is refused, not disconnected"
            );
        }

        assert!(api.said().len() <= MAX_NOTICES);
        assert!(
            api.dropped_notices() > 0,
            "the notices past the ceiling have to be counted, or a truncated \
             list is indistinguishable from a module that went quiet"
        );
    }

    #[test]
    fn a_few_enormous_notices_are_capped_as_well_as_many_small_ones() {
        // The control for the test above. A ceiling on the count alone is the
        // same attack with fewer, fatter messages.
        let mut api = api(vec![]);
        let fat = "x".repeat(MAX_NOTICE_BYTES / 4);

        for _ in 0..16 {
            api.handle(Request::Notify {
                level: Level::Info,
                text: fat.clone(),
            });
        }

        let held: usize = api.said().iter().map(|(_, text)| text.len()).sum();
        assert!(
            held <= MAX_NOTICE_BYTES,
            "held {held} bytes, past the {MAX_NOTICE_BYTES} ceiling"
        );
        assert!(api.dropped_notices() > 0);
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
