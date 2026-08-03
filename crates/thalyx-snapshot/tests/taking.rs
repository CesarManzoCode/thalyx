//! Everything about snapshots that is not Btrfs.
//!
//! Run against directories, because policy that can only be exercised on a
//! Btrfs filesystem is policy that is never exercised. The one thing these
//! cannot check is that Btrfs itself does what it says — that is
//! `THALYX_REQUIRE_BTRFS_TESTS=1` and `dev/verify.sh`, on a real filesystem.

use std::path::Path;
use thalyx_snapshot::directories::Directories;
use thalyx_snapshot::{SnapshotError, Snapshots, Volumes, name_for};

fn subvolume() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let subvolume = dir.path().join("work");
    Directories::make_subvolume(&subvolume).unwrap();
    std::fs::write(subvolume.join("notes.txt"), "as it was\n").unwrap();
    (dir, subvolume)
}

#[test]
fn a_snapshot_holds_what_the_subvolume_held() {
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);

    let taken = snapshots.take("2026-08-03T04-00-00Z-before").unwrap();
    std::fs::write(subvolume.join("notes.txt"), "changed since\n").unwrap();

    assert_eq!(
        std::fs::read_to_string(taken.path.join("notes.txt")).unwrap(),
        "as it was\n"
    );
}

#[test]
fn snapshots_live_beside_the_subvolume_and_not_inside_it() {
    // Inside, every snapshot would be part of the next one, and the tree would
    // grow by its own history every time anybody took one.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);

    let taken = snapshots.take("first").unwrap();
    assert!(
        !taken.path.starts_with(&subvolume),
        "the snapshot landed inside the subvolume: {}",
        taken.path.display()
    );
    assert_eq!(taken.path.parent().unwrap(), snapshots.directory());
}

#[test]
fn a_directory_that_is_not_a_subvolume_is_refused_rather_than_copied() {
    // A copy is not a snapshot. It is not atomic and it takes time
    // proportional to the data, so something that took twenty minutes is a
    // picture of twenty minutes rather than of an instant — and it would be
    // presented under the same name as the real thing.
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("just-a-directory");
    std::fs::create_dir_all(&plain).unwrap();

    let snapshots = Snapshots::of(Directories, &plain);
    assert!(matches!(
        snapshots.take("first"),
        Err(SnapshotError::NotASubvolume(_))
    ));
}

#[test]
fn taking_the_same_name_twice_refuses_instead_of_replacing() {
    // Silently replacing would destroy the older moment, which is the one
    // thing a snapshot exists to keep.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);

    snapshots.take("nightly").unwrap();
    assert!(matches!(
        snapshots.take("nightly"),
        Err(SnapshotError::AlreadyExists(_))
    ));
}

#[test]
fn a_name_that_would_escape_the_snapshot_directory_never_reaches_the_filesystem() {
    let (dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);

    assert!(snapshots.take("../escaped").is_err());
    assert!(
        !dir.path().join("escaped").exists(),
        "a snapshot was written outside the snapshot directory"
    );
}

#[test]
fn snapshots_come_back_in_the_order_they_were_taken() {
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);

    // Taken out of order on purpose, with labels whose alphabetical order is
    // the reverse of their chronological one.
    snapshots
        .take(&name_for("zzz", "2026-08-03T05:00:00Z"))
        .unwrap();
    snapshots
        .take(&name_for("aaa", "2026-08-03T03:00:00Z"))
        .unwrap();
    snapshots
        .take(&name_for("mmm", "2026-08-03T04:00:00Z"))
        .unwrap();

    let listed = snapshots.list().unwrap();
    let names: Vec<&str> = listed.iter().map(|s| s.name.as_str()).collect();

    assert!(names[0].ends_with("aaa"), "{names:?}");
    assert!(names[1].ends_with("mmm"), "{names:?}");
    assert!(names[2].ends_with("zzz"), "{names:?}");

    assert_eq!(snapshots.latest().unwrap().unwrap().name, names[2]);
}

#[test]
fn the_order_does_not_come_from_the_files_modification_times() {
    // A snapshot's mtime is the *source's* mtime, not the moment it was taken.
    // Ordering by it would put snapshots in the wrong order and look right,
    // and "restore the latest" would silently mean a different one.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);

    let older = snapshots
        .take(&name_for("x", "2026-08-01T00:00:00Z"))
        .unwrap();
    let newer = snapshots
        .take(&name_for("y", "2026-08-09T00:00:00Z"))
        .unwrap();

    // Make the older one the most recently touched.
    std::fs::write(older.path.join("touched"), "now").unwrap();

    assert_eq!(snapshots.latest().unwrap().unwrap().name, newer.name);
}

#[test]
fn a_subvolume_with_no_snapshots_lists_nothing_rather_than_failing() {
    // No snapshot directory yet is the normal state of a fresh subvolume, not
    // an error condition.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);

    assert!(snapshots.list().unwrap().is_empty());
    assert!(snapshots.latest().unwrap().is_none());
}

#[test]
fn forgetting_a_snapshot_leaves_the_subvolume_alone() {
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);

    snapshots.take("first").unwrap();
    snapshots.forget("first").unwrap();

    assert!(snapshots.list().unwrap().is_empty());
    assert!(
        subvolume.join("notes.txt").exists(),
        "forgetting a snapshot removed the live tree"
    );
}

#[test]
fn forgetting_by_a_name_that_is_not_a_snapshot_deletes_nothing() {
    // The name is looked up among the snapshots first, so a caller cannot hand
    // this something that resolves to a subvolume somewhere else.
    let (dir, subvolume) = subvolume();
    let elsewhere = dir.path().join("important");
    Directories::make_subvolume(&elsewhere).unwrap();

    let snapshots = Snapshots::of(Directories, &subvolume);
    assert!(matches!(
        snapshots.forget("../important"),
        Err(SnapshotError::NoSuchSnapshot(_))
    ));
    assert!(
        elsewhere.exists(),
        "a subvolume outside the snapshots was deleted"
    );
}

/// Btrfs itself, on a real filesystem.
///
/// Skipped where there is no Btrfs, and it says so — an `ok` that exercised
/// nothing is indistinguishable from one that exercised everything.
/// `THALYX_REQUIRE_BTRFS_TESTS=1` turns the skip into a failure.
#[test]
fn btrfs_takes_a_real_snapshot() {
    let Some(subvolume) = btrfs_scratch() else {
        let required = std::env::var("THALYX_REQUIRE_BTRFS_TESTS").is_ok();
        assert!(
            !required,
            "THALYX_REQUIRE_BTRFS_TESTS is set and no Btrfs subvolume could be made"
        );
        eprintln!(
            "NOT PROVEN: btrfs_takes_a_real_snapshot did not run. It needs a writable \
             Btrfs filesystem (THALYX_BTRFS_SCRATCH=<path on btrfs>) and btrfs-progs."
        );
        return;
    };

    let btrfs = thalyx_snapshot::Btrfs::new();
    let snapshots = Snapshots::of(btrfs, &subvolume);

    std::fs::write(subvolume.join("notes.txt"), "as it was\n").unwrap();
    let taken = snapshots
        .take(&name_for("test", "2026-08-03T04:00:00Z"))
        .unwrap();

    std::fs::write(subvolume.join("notes.txt"), "changed since\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(taken.path.join("notes.txt")).unwrap(),
        "as it was\n",
        "the snapshot moved with the subvolume, so it is not a snapshot"
    );

    // A read-only snapshot has to actually be read-only, or it is a second
    // working copy that will drift from the moment it claims to be.
    assert!(
        std::fs::write(taken.path.join("notes.txt"), "tampered").is_err(),
        "the snapshot was writable"
    );

    snapshots.forget(&taken.name).unwrap();
    let _ = snapshots.volumes().delete(&subvolume);
    let _ = std::fs::remove_dir_all(snapshots.directory());
}

/// A throwaway subvolume on a real Btrfs filesystem, or nothing.
fn btrfs_scratch() -> Option<std::path::PathBuf> {
    let base = std::env::var("THALYX_BTRFS_SCRATCH").ok()?;
    let base = Path::new(&base);

    let subvolume = base.join(format!("thalyx-test-{}", std::process::id()));
    let btrfs = thalyx_snapshot::Btrfs::new();
    let _ = btrfs.delete(&subvolume);

    let made = std::process::Command::new("btrfs")
        .args(["subvolume", "create"])
        .arg(&subvolume)
        .output()
        .ok()?;

    made.status.success().then_some(subvolume)
}
