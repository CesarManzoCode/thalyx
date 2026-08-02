//! Making a granted directory writable by the module's own user.
//!
//! `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md` decrees one uid per module.
//! That buys isolation between modules and costs something specific: a module
//! running as uid 700000 cannot write to a directory belonging to the human,
//! however clearly the human confirmed the permission.
//!
//! Changing the directory's owner would fix it and is not acceptable — Thalyx
//! would be rewriting the human's filesystem to suit itself. An **idmapped
//! mount** fixes it without touching anything on disk: the bind the module sees
//! presents the human's files as owned by the module, and anything the module
//! writes lands on disk owned by the human.
//!
//! ## How the mapping is expressed, and which way round it goes
//!
//! The kernel takes the translation from a user namespace, and **the direction
//! is the opposite of the obvious reading**. A `uid_map` line is
//! `<inside> <outside> <count>`, and for an idmapped mount the kernel treats
//! the id *on disk* as the inside one and reports the outside one.
//!
//! So to make a directory owned by 1000 on disk appear as owned by 700000, the
//! map is:
//!
//! ```text
//! 1000 700000 1
//! ```
//!
//! Writing goes back the other way on its own: the module writes as 700000, the
//! mount translates it to 1000, and the file lands on disk owned by the human.
//!
//! The first version had this inverted. It did not fail — it mounted cleanly
//! and the directory showed up as owned by `nobody`, because the on-disk id was
//! not a valid *inside* id in that map. Which is the good failure: an id that
//! cannot be translated becomes nobody, and nobody can write nothing.
//!
//! Only that one id is mapped. Anything else in the directory shows up as
//! `nobody` too, and that is also right: the module was granted this path, not
//! everything that happens to live in it.
//!
//! ## Why there is a helper process
//!
//! A user namespace has to be *entered* to exist, and entering one costs the
//! caller its privileges — which the launcher still needs. So a short-lived
//! child enters one and waits; the launcher writes its map from outside, where
//! it is still root, and keeps a descriptor to the namespace. The namespace
//! outlives the child because the descriptor holds it open.

use crate::{Result, SandboxError};
use std::io::{BufRead, Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

/// Marker for the re-execution that creates a user namespace.
pub const USERNS_MARKER: &str = "__thalyx_sandbox_userns";

/// What the helper prints once it is inside the namespace.
const READY: &[u8] = b"ready\n";

/// The child half: enter a user namespace and wait to be mapped.
///
/// Being released is how this ends when everything worked, so it returns
/// success. An earlier version reported the release as an error and printed it
/// on the launcher's stderr — noise that read exactly like a failure, and did
/// in fact make a test believe the mount had been refused.
pub fn run_userns_helper() -> std::result::Result<u8, SandboxError> {
    thalyx_syscall::unshare(thalyx_syscall::CLONE_NEWUSER).map_err(|source| {
        SandboxError::NamespacesUnavailable {
            profile: "idmap".to_string(),
            source,
        }
    })?;

    // Announce, so the launcher writes the map only once the namespace exists.
    // Writing it earlier fails; racing on a sleep fails rarely, which is worse.
    let mut out = std::io::stdout();
    out.write_all(READY)
        .and_then(|()| out.flush())
        .map_err(|_| SandboxError::IdmapUnavailable {
            reason: "could not tell the launcher the namespace was ready".to_string(),
        })?;

    // Block until the launcher is done with us. It closes our input, and the
    // read returns zero. The namespace outlives this process on the descriptor
    // the launcher is holding.
    let mut ignored = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut ignored);
    Ok(0)
}

/// A user namespace that exists only to describe an id translation.
pub struct IdMapping {
    namespace: OwnedFd,
}

impl IdMapping {
    /// Map the ids a path carries on disk to the ids the module should see.
    ///
    /// Named for what happens rather than for the kernel's `inside`/`outside`,
    /// because those words point the opposite way to everyone's first guess.
    /// See the module docs.
    ///
    /// `helper` is the `thalyx` binary; it is re-executed to enter the
    /// namespace.
    pub fn translating(helper: &Path, on_disk: (u32, u32), seen_as: u32) -> Result<Self> {
        let mut child = std::process::Command::new(helper)
            .arg(USERNS_MARKER)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|source| SandboxError::Exec {
                program: helper.to_path_buf(),
                source,
            })?;

        let pid = child.id();
        let result = Self::map_and_capture(&mut child, pid, on_disk, seen_as);

        // Release the helper whatever happened. The namespace survives on the
        // descriptor alone.
        drop(child.stdin.take());
        let _ = child.wait();

        result
    }

    fn map_and_capture(
        child: &mut std::process::Child,
        pid: u32,
        on_disk: (u32, u32),
        seen_as: u32,
    ) -> Result<Self> {
        let stdout = child.stdout.take().ok_or_else(|| SandboxError::Idmap {
            reason: "the helper had no output to wait on".to_string(),
        })?;

        let mut line = String::new();
        std::io::BufReader::new(stdout)
            .read_line(&mut line)
            .map_err(|source| SandboxError::io("<user namespace helper>", source))?;

        if line.as_bytes() != READY {
            return Err(SandboxError::Idmap {
                reason: format!(
                    "the helper said `{}` instead of announcing itself",
                    line.trim()
                ),
            });
        }

        // `setgroups` is denied first. It is only strictly required when the
        // writer is unprivileged, and writing it regardless costs nothing and
        // removes a difference in behaviour between running as root and not.
        let _ = std::fs::write(format!("/proc/{pid}/setgroups"), "deny");

        // The group map comes from the directory's own group, not from its
        // owner. They are usually the same number and the one time they are
        // not is the time a file lands unwritable for a reason nobody can see.
        let (on_disk_uid, on_disk_gid) = on_disk;
        for (file, on_disk_id) in [("gid_map", on_disk_gid), ("uid_map", on_disk_uid)] {
            // `<inside> <outside>`, and the id on disk is the inside one.
            let mapping = format!("{on_disk_id} {seen_as} 1");
            let path = format!("/proc/{pid}/{file}");
            std::fs::write(&path, &mapping).map_err(|source| SandboxError::Idmap {
                reason: format!("could not write {file} (`{mapping}`): {source}"),
            })?;
        }

        let namespace = std::fs::File::open(format!("/proc/{pid}/ns/user"))
            .map_err(|source| SandboxError::Idmap {
                reason: format!("could not hold the namespace open: {source}"),
            })?
            .into();

        Ok(Self { namespace })
    }

    pub fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.namespace.as_fd()
    }
}

/// Bind `source` at `target` with ownership translated through `mapping`.
///
/// The mount is cloned detached, remapped, and only then attached. There is no
/// instant at which it is visible in the tree presenting the wrong owner.
pub fn bind_idmapped(source: &Path, target: &Path, mapping: &IdMapping) -> Result<()> {
    use std::os::fd::AsRawFd;

    let tree = thalyx_syscall::open_tree(
        source,
        thalyx_syscall::OPEN_TREE_CLONE | thalyx_syscall::AT_RECURSIVE | libc::O_CLOEXEC as u32,
    )
    .map_err(|source_error| SandboxError::Idmap {
        reason: format!(
            "could not clone the mount at {}: {source_error}",
            source.display()
        ),
    })?;

    let attr = thalyx_syscall::MountAttr {
        attr_set: thalyx_syscall::MOUNT_ATTR_IDMAP,
        attr_clr: 0,
        propagation: 0,
        userns_fd: mapping.as_fd().as_raw_fd() as u64,
    };

    thalyx_syscall::mount_setattr(
        tree.as_fd(),
        thalyx_syscall::AT_EMPTY_PATH_U32 | thalyx_syscall::AT_RECURSIVE,
        &attr,
    )
    .map_err(|source_error| SandboxError::Idmap {
        reason: format!(
            "the kernel refused to remap {}: {source_error}\n  \
             The filesystem underneath may not support idmapped mounts.",
            source.display()
        ),
    })?;

    thalyx_syscall::move_mount(tree.as_fd(), target).map_err(|source_error| SandboxError::Idmap {
        reason: format!(
            "could not attach the remapped mount at {}: {source_error}",
            target.display()
        ),
    })
}

/// The owner of a path, for deciding whether a mapping is needed at all.
pub fn owner_of(path: &Path) -> Result<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).map_err(|source| SandboxError::io(path, source))?;
    Ok((metadata.uid(), metadata.gid()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_of_a_path_is_read_from_the_filesystem() {
        // Both halves, because the group is what the mapping uses for
        // `gid_map` and it is not always the same number as the owner.
        let dir = tempfile::tempdir().unwrap();
        let (uid, gid) = owner_of(dir.path()).unwrap();

        assert_eq!(uid, thalyx_syscall::effective_uid());

        use std::os::unix::fs::MetadataExt;
        assert_eq!(gid, std::fs::metadata(dir.path()).unwrap().gid());
    }

    #[test]
    fn a_path_that_is_not_there_is_an_error_rather_than_a_guess() {
        assert!(owner_of(Path::new("/definitely/not/here")).is_err());
    }
}
