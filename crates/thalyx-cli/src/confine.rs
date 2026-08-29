//! A workspace the kernel enforces, rather than a name userspace checked.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md`. An external agent works
//! inside one directory and may not reach outside it, and until 2026-08-28 that
//! boundary was made of `std::fs::canonicalize`: resolve the name the agent
//! gave, compare the answer against the workspace, then let the verb open **the
//! original name** all over again.
//!
//! That is the exact sequence `crate::api`'s module surface was rewritten to
//! stop using, and for the exact reason written there: it is a sequence, and
//! between the comparison and the open there is a moment. Anything that can
//! write inside the workspace can spend that moment turning a directory into a
//! symlink pointing somewhere else, and Thalyx — which is not inside anybody's
//! sandbox — then opens the new target with Thalyx's own reach.
//!
//! It was not a theoretical window. The test that found it swaps `src` between
//! a real directory and a link to another tree while an agent reads
//! `src/main.rs` in a loop; before this module existed, 57 of 4000 reads came
//! back with the contents of a file outside the workspace.
//!
//! ## What replaces it
//!
//! The same primitive the module surface uses, so that this system has one
//! answer to this question and not two: `openat2` with `RESOLVE_BENEATH`,
//! against a descriptor for the workspace opened once when the session opened.
//! The kernel refuses to resolve out of that directory *during* resolution, so
//! there is no intermediate answer for anybody to invalidate — the check and
//! the open are the same syscall.
//!
//! `RESOLVE_NO_SYMLINKS` is deliberately **not** used. A project with a
//! relative symlink in it is an ordinary project, and `RESOLVE_BENEATH`
//! already contains where such a link can land: a link pointing out is refused
//! by the kernel, a link pointing in resolves to something inside. Refusing all
//! of them would narrow the workspace for no gain in containment.
//!
//! ## Why the verbs still see a path
//!
//! Every verb in this session takes a path and opens it by name, and rewriting
//! all of them to take descriptors would be a second filesystem layer living
//! beside the first — which is what `CLAUDE.md` means by not building a
//! parallel API.
//!
//! So an [`Anchored`] holds the descriptor the kernel resolved and hands out
//! `/proc/self/fd/N` as the name to open. The kernel resolves that to the very
//! inode this call proved was inside the workspace, whatever has happened to
//! the names since. The path the *answer* carries is unchanged — the caller
//! keeps the workspace-relative path it already had and uses the anchored one
//! only for the open, which is two arguments in the same function rather than a
//! new shape for every verb.
//!
//! ## What is still open, said plainly
//!
//! A path being **created** does not exist yet, so there is nothing to anchor.
//! [`Confinement::anchor_parent`] pins the parent directory and hands back
//! `/proc/self/fd/N/leaf`: the parent is an inode nothing can redirect, and the
//! last component is one lookup inside it. A symlink planted at that last
//! component between the refusal-check and the create would still be followed.
//! It is narrower than what it replaces by every directory on the path, and it
//! is written down here rather than left as a difference between what this
//! module claims and what it does.
//!
//! For an unconfined session — the person's own — everything here is the
//! identity function. Nothing about the human's session changes, which is the
//! point: the boundary belongs to the caller that has one.

use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The workspace an external session may not leave.
#[derive(Debug)]
pub struct Confinement {
    /// Opened once, when the session opened. Holding it pins the *directory*
    /// rather than its name: renaming or replacing the workspace afterwards
    /// does not silently redirect the session into somewhere else.
    root: OwnedFd,
    /// The workspace's own path, with links resolved. Only ever used to work
    /// out what part of an absolute path is relative to the root — never as
    /// the thing containment is decided by.
    real: PathBuf,
}

/// A path the kernel has resolved inside the workspace, held open.
///
/// The descriptor lives as long as this value does, and [`Anchored::path`] is
/// only valid while it does — which is why it borrows.
#[derive(Debug)]
pub struct Anchored {
    /// `None` for an unconfined session, where the path is its own anchor.
    _held: Option<OwnedFd>,
    open_as: PathBuf,
}

impl Anchored {
    /// The name to open. Not the name to print: that is the caller's original.
    pub fn path(&self) -> &Path {
        &self.open_as
    }

    /// The identity anchor, for a session with no workspace.
    pub fn wherever(path: &Path) -> Self {
        Self {
            _held: None,
            open_as: path.to_path_buf(),
        }
    }
}

/// Why a path could not be anchored, in the words the boundary reports.
#[derive(Debug)]
pub enum NotAnchored {
    /// The kernel refused to resolve it inside the workspace. `EXDEV` is what
    /// `RESOLVE_BENEATH` answers for a path or a symlink that leaves the base.
    Outside,
    /// It is not there. Distinguished from [`NotAnchored::Outside`] because
    /// rule 10 says a failure to read is not a failure to exist, and here the
    /// two lead to different remedies.
    Absent,
    Unreadable(std::io::Error),
}

impl Confinement {
    /// Pin a workspace, by opening it.
    ///
    /// The path is canonicalised once, here, and never again. That is not the
    /// containment check — the descriptor is — it is how an absolute path the
    /// caller names is turned into something relative to the root.
    pub fn of(workspace: &Path) -> std::io::Result<Arc<Self>> {
        let real = std::fs::canonicalize(workspace)?;
        // `File::open` on a directory yields a descriptor `openat2` accepts as
        // a resolution base. The standard library does not expose `O_PATH`;
        // nothing ever reads through this one, so the difference is not
        // reachable from outside this module.
        let root = OwnedFd::from(std::fs::File::open(&real)?);
        Ok(Arc::new(Self { root, real }))
    }

    /// The workspace's own path, with every link already resolved.
    ///
    /// Never the thing containment is decided by — that is the descriptor — and
    /// only ever the answer to "which workspace is this session's".
    pub fn root(&self) -> &Path {
        &self.real
    }

    /// Resolve an absolute path inside the workspace, and hold it open.
    pub fn anchor(&self, path: &Path) -> Result<Anchored, NotAnchored> {
        let relative = self.relative(path)?;
        // `O_PATH` is what a descriptor used only for resolution wants, and it
        // is the one flag that lets this anchor a directory, a file and a
        // symlink's target alike without caring which it is. Reopening through
        // `/proc/self/fd` is what turns it back into something readable.
        let held = thalyx_syscall::open_beneath(
            self.root.as_fd(),
            &relative,
            thalyx_syscall::O_PATH | thalyx_syscall::O_CLOEXEC,
            0,
            thalyx_syscall::RESOLVE_BENEATH | thalyx_syscall::RESOLVE_NO_MAGICLINKS,
        )
        .map_err(classify)?;

        let fd = std::os::fd::AsRawFd::as_raw_fd(&held);
        Ok(Anchored {
            open_as: PathBuf::from(format!("/proc/self/fd/{fd}")),
            _held: Some(OwnedFd::from(held)),
        })
    }

    /// The same, for a path that is about to be created.
    ///
    /// The parent is anchored and the last component is appended to it, so the
    /// only lookup that is not against a pinned inode is that one. See the
    /// module's "what is still open".
    pub fn anchor_parent(&self, path: &Path) -> Result<Anchored, NotAnchored> {
        let (parent, leaf) = match (path.parent(), path.file_name()) {
            (Some(parent), Some(leaf)) => (parent, leaf),
            // A path with no parent is `/`, which is never inside a workspace.
            _ => return Err(NotAnchored::Outside),
        };
        let anchored = self.anchor(parent)?;
        Ok(Anchored {
            open_as: anchored.open_as.join(leaf),
            _held: anchored._held,
        })
    }

    /// What the caller's absolute path is, seen from the root descriptor.
    fn relative(&self, path: &Path) -> Result<PathBuf, NotAnchored> {
        // `strip_prefix` compares components rather than characters, so
        // `/home/projects-old` is not inside `/home/projects`.
        match path.strip_prefix(&self.real) {
            // The root itself. `openat2` rejects an empty path, and `.` is the
            // spelling that means the base.
            Ok(rest) if rest.as_os_str().is_empty() => Ok(PathBuf::from(".")),
            Ok(rest) => Ok(rest.to_path_buf()),
            // Not lexically inside. The kernel would refuse it too, but there
            // is nothing to hand it: `openat2` resolves relative names, and an
            // absolute one is refused by `RESOLVE_BENEATH` without saying why.
            Err(_) => Err(NotAnchored::Outside),
        }
    }
}

impl NotAnchored {
    /// The refusal a verb reports, in the vocabulary every verb already speaks.
    ///
    /// `Outside` becomes "is not there" and not a message naming the workspace,
    /// and that is deliberate: the boundary above this one
    /// (`external::inside`) is what explains containment to an agent, with the
    /// remedy attached. This one is reached only when a path passed that check
    /// and then stopped being inside — a race, or a link — and telling the
    /// caller *how* it got out would describe a filesystem it may not see.
    pub fn about(self, path: &Path) -> thalyx_files::FileError {
        match self {
            Self::Outside | Self::Absent => thalyx_files::FileError::Absent(path.to_path_buf()),
            Self::Unreadable(error) => thalyx_files::FileError::Unreadable {
                path: path.to_path_buf(),
                detail: error.to_string(),
            },
        }
    }
}

fn classify(error: std::io::Error) -> NotAnchored {
    match error.raw_os_error() {
        // What `RESOLVE_BENEATH` answers for a path, or a symlink, that leaves
        // the base — including an absolute symlink that would have landed
        // inside, which is the narrowing `api.rs` documents and accepts.
        Some(thalyx_syscall::EXDEV) => NotAnchored::Outside,
        // `RESOLVE_NO_MAGICLINKS`, and also an ordinary symlink loop. Both are
        // paths that do not name a thing, and neither is "outside".
        Some(thalyx_syscall::ELOOP) => NotAnchored::Outside,
        Some(thalyx_syscall::ENOENT) | Some(thalyx_syscall::ENOTDIR) => NotAnchored::Absent,
        _ => NotAnchored::Unreadable(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_workspace() -> (tempfile::TempDir, Arc<Confinement>) {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("src")).expect("src");
        std::fs::write(root.path().join("src/main.rs"), "inside\n").expect("write");
        let confinement = Confinement::of(root.path()).expect("confinement");
        (root, confinement)
    }

    #[test]
    fn an_anchored_path_reads_the_file_it_named() {
        let (root, workspace) = a_workspace();
        let anchored = workspace
            .anchor(&root.path().canonicalize().unwrap().join("src/main.rs"))
            .expect("anchored");
        assert_eq!(
            std::fs::read_to_string(anchored.path()).unwrap(),
            "inside\n"
        );
    }

    /// The whole point, and it is not the same claim as "a symlink is refused":
    /// what is proved here is that the descriptor keeps naming the file it
    /// resolved **after** the name has been made to mean something else.
    #[test]
    fn an_anchor_survives_the_directory_under_it_becoming_a_link_somewhere_else() {
        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir_all(outside.path().join("src")).unwrap();
        std::fs::write(outside.path().join("src/main.rs"), "SECRET\n").unwrap();

        let (root, workspace) = a_workspace();
        let real = root.path().canonicalize().unwrap();
        let anchored = workspace
            .anchor(&real.join("src/main.rs"))
            .expect("anchored");

        std::fs::rename(real.join("src"), real.join("src.moved")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("src"), real.join("src")).unwrap();

        assert_eq!(
            std::fs::read_to_string(anchored.path()).unwrap(),
            "inside\n",
            "the anchor followed the name instead of holding the file"
        );
    }

    #[test]
    fn a_link_out_of_the_workspace_is_refused_by_the_kernel_and_not_by_a_comparison() {
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret"), "SECRET\n").unwrap();

        let (root, workspace) = a_workspace();
        let real = root.path().canonicalize().unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), real.join("escape")).unwrap();

        assert!(matches!(
            workspace.anchor(&real.join("escape")),
            Err(NotAnchored::Outside)
        ));
    }

    #[test]
    fn a_relative_link_that_stays_inside_still_works() {
        let (root, workspace) = a_workspace();
        let real = root.path().canonicalize().unwrap();
        std::os::unix::fs::symlink("src/main.rs", real.join("shortcut")).unwrap();

        let anchored = workspace.anchor(&real.join("shortcut")).expect("anchored");
        assert_eq!(
            std::fs::read_to_string(anchored.path()).unwrap(),
            "inside\n"
        );
    }

    #[test]
    fn a_path_that_is_not_there_says_so_rather_than_saying_outside() {
        let (root, workspace) = a_workspace();
        let real = root.path().canonicalize().unwrap();
        assert!(matches!(
            workspace.anchor(&real.join("src/nothing.rs")),
            Err(NotAnchored::Absent)
        ));
    }

    #[test]
    fn a_file_about_to_be_made_is_anchored_by_its_parent() {
        let (root, workspace) = a_workspace();
        let real = root.path().canonicalize().unwrap();
        let anchored = workspace
            .anchor_parent(&real.join("src/new.rs"))
            .expect("anchored");
        std::fs::write(anchored.path(), "made\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(real.join("src/new.rs")).unwrap(),
            "made\n"
        );
    }

    #[test]
    fn a_path_outside_the_workspace_is_refused_before_the_kernel_is_asked() {
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret"), "SECRET\n").unwrap();
        let (_root, workspace) = a_workspace();
        assert!(matches!(
            workspace.anchor(&outside.path().join("secret")),
            Err(NotAnchored::Outside)
        ));
    }
}
