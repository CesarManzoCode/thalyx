//! The atomic commit.
//!
//! Publication is two `rename(2)` calls, both atomic, in this order:
//!
//! 1. `.staging/<uuid>` → `modules/<id>/<version>`
//!    The destination does not exist yet, so there is no `ENOTEMPTY`.
//! 2. `modules/<id>/.current.tmp` → `modules/<id>/current`
//!    `rename` over an existing symlink replaces it atomically.
//!
//! **The module becomes installed at the instant of step 2, not before.**
//!
//! Why not simply rename the directory over the old one: `rename` fails with
//! `ENOTEMPTY` when the destination is a non-empty directory, so upgrading
//! 2.3.0 to 2.3.1 could never be a single directory rename. The symlink
//! indirection solves that and the "which version is live" question at once.
//!
//! See `vault/04-Flujo-Canonico/Fase-Commit-Atomico.md`.

use crate::fault::{self, FaultPoint};
use crate::store::Store;
use crate::{CoreError, Result};
use std::path::Path;

/// Publish a staged version and make it current.
///
/// If the process dies between the two renames, the version directory exists
/// but `current` still points where it did before. The store reports the module
/// as not installed, which is the invariant the fault-injection tests check.
pub fn publish(store: &Store, staging: &Path, id: &str, version: &str) -> Result<()> {
    let module_root = store.module_root(id);
    std::fs::create_dir_all(&module_root).map_err(|e| CoreError::io(&module_root, e))?;

    let final_dir = store.version_dir(id, version);

    // Clear an orphan left by a previously interrupted commit.
    //
    // Without this, retrying after a crash between the two renames fails with
    // `ENOTEMPTY`: the version directory is already in place but `current`
    // never swung to it. Removing it is safe precisely because it is an
    // orphan — nothing points at it, so nothing can be using it. A directory
    // that *is* current is never reached here; installing over a live version
    // is refused earlier as `AlreadyInstalled`.
    //
    // Found by the level 2 recovery test, not by design review.
    if final_dir.exists() && store.installed_version(id).as_deref() != Some(version) {
        std::fs::remove_dir_all(&final_dir).map_err(|e| CoreError::io(&final_dir, e))?;
    }

    std::fs::rename(staging, &final_dir).map_err(|e| CoreError::io(&final_dir, e))?;

    // The window the whole design is about.
    fault::checkpoint(FaultPoint::MidCommit)?;

    swap_current_link(store, id, version)?;

    // Make the directory entries durable, so a power loss after we return
    // cannot lose the publication we just reported as done.
    sync_dir(&module_root)?;

    Ok(())
}

/// Atomically point `current` at `version`.
///
/// Creating the temporary link and renaming it is what makes this atomic:
/// there is no instant where `current` is missing.
fn swap_current_link(store: &Store, id: &str, version: &str) -> Result<()> {
    let module_root = store.module_root(id);
    let temporary = module_root.join(".current.tmp");
    let current = store.current_link(id);

    // A leftover from an interrupted run would make symlink() fail with EEXIST.
    let _ = std::fs::remove_file(&temporary);

    std::os::unix::fs::symlink(version, &temporary).map_err(|e| CoreError::io(&temporary, e))?;
    std::fs::rename(&temporary, &current).map_err(|e| CoreError::io(&current, e))?;

    Ok(())
}

/// Remove a module: drop `current` first, then the version directory.
///
/// The order matters. Unlinking `current` is the atomic step that makes the
/// module uninstalled; deleting the directory afterwards is just reclaiming
/// space. Interrupted halfway, this leaves an orphaned version directory —
/// inert, and reported by [`Store::orphaned_versions`].
pub fn unpublish(store: &Store, id: &str) -> Result<String> {
    let version = store
        .installed_version(id)
        .ok_or_else(|| CoreError::NotInstalled {
            module_id: id.to_string(),
        })?;

    let current = store.current_link(id);
    std::fs::remove_file(&current).map_err(|e| CoreError::io(&current, e))?;
    sync_dir(&store.module_root(id))?;

    let version_dir = store.version_dir(id, &version);
    if version_dir.is_dir() {
        std::fs::remove_dir_all(&version_dir).map_err(|e| CoreError::io(&version_dir, e))?;
    }

    let module_root = store.module_root(id);
    if std::fs::read_dir(&module_root)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
    {
        let _ = std::fs::remove_dir(&module_root);
    }

    Ok(version)
}

/// fsync a directory so its entries survive a crash.
///
/// Renaming a file is atomic with respect to other readers, but the directory
/// entry itself is not durable until the directory is synced.
fn sync_dir(path: &Path) -> Result<()> {
    let dir = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
    dir.sync_all().map_err(|e| CoreError::io(path, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged(store: &Store, contents: &str) -> std::path::PathBuf {
        let dir = store.new_staging_dir().unwrap();
        std::fs::write(dir.join("payload"), contents).unwrap();
        dir
    }

    #[test]
    fn publishing_makes_a_module_current() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();

        publish(&store, &staged(&store, "v1"), "org.demo.thing", "1.0.0").unwrap();

        assert_eq!(
            store.installed_version("org.demo.thing").as_deref(),
            Some("1.0.0")
        );
        assert!(store.orphaned_versions().unwrap().is_empty());
    }

    #[test]
    fn upgrading_over_an_existing_version_works() {
        // The case a plain directory rename could never handle: the
        // destination module directory is already non-empty.
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();

        publish(&store, &staged(&store, "v1"), "org.demo.thing", "1.0.0").unwrap();
        publish(&store, &staged(&store, "v2"), "org.demo.thing", "1.0.1").unwrap();

        assert_eq!(
            store.installed_version("org.demo.thing").as_deref(),
            Some("1.0.1")
        );
        let payload =
            std::fs::read_to_string(store.version_dir("org.demo.thing", "1.0.1").join("payload"))
                .unwrap();
        assert_eq!(payload, "v2");
    }

    #[test]
    fn a_version_directory_without_a_current_link_is_not_installed() {
        // Exactly the state a crash between the two renames leaves behind.
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();

        let staging = staged(&store, "v1");
        std::fs::create_dir_all(store.module_root("org.demo.thing")).unwrap();
        std::fs::rename(&staging, store.version_dir("org.demo.thing", "1.0.0")).unwrap();

        assert!(!store.is_installed("org.demo.thing"));
        assert_eq!(
            store.orphaned_versions().unwrap(),
            vec![("org.demo.thing".to_string(), "1.0.0".to_string())]
        );
    }

    #[test]
    fn unpublishing_removes_the_module() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();

        publish(&store, &staged(&store, "v1"), "org.demo.thing", "1.0.0").unwrap();
        let removed = unpublish(&store, "org.demo.thing").unwrap();

        assert_eq!(removed, "1.0.0");
        assert!(!store.is_installed("org.demo.thing"));
        assert!(store.orphaned_versions().unwrap().is_empty());
    }

    #[test]
    fn unpublishing_something_absent_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();
        assert!(matches!(
            unpublish(&store, "org.demo.absent"),
            Err(CoreError::NotInstalled { .. })
        ));
    }
}
