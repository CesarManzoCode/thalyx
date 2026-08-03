//! Returning a subvolume to a moment, and knowing what that costs first.
//!
//! `vault/04-Flujo-Canonico/Rollback-vs-Restore.md` calls this the destructive
//! one. What is checked here is that it destroys exactly what it says it will,
//! that the human could have known beforehand, and that what it replaced is
//! still somewhere.

use thalyx_snapshot::directories::Directories;
use thalyx_snapshot::{Snapshots, difference};

fn subvolume() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let subvolume = dir.path().join("work");
    Directories::make_subvolume(&subvolume).unwrap();
    std::fs::write(subvolume.join("kept.txt"), "original\n").unwrap();
    std::fs::write(subvolume.join("doomed.txt"), "also original\n").unwrap();
    (dir, subvolume)
}

#[test]
fn a_restore_returns_the_contents_the_snapshot_held() {
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);
    let taken = snapshots.take("before").unwrap();

    std::fs::write(subvolume.join("kept.txt"), "edited since\n").unwrap();
    snapshots
        .restore(&taken.name, "2026-08-03T04:00:00Z")
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(subvolume.join("kept.txt")).unwrap(),
        "original\n"
    );
}

#[test]
fn work_created_since_the_snapshot_is_destroyed_which_is_the_whole_point() {
    // Not a defect to be softened. `restore` exists so a human can undo their
    // own work, and a version that quietly kept new files would leave the tree
    // in a state matching neither moment.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);
    let taken = snapshots.take("before").unwrap();

    std::fs::write(subvolume.join("written-after.txt"), "new work\n").unwrap();
    snapshots
        .restore(&taken.name, "2026-08-03T04:00:00Z")
        .unwrap();

    assert!(!subvolume.join("written-after.txt").exists());
}

#[test]
fn what_was_replaced_is_kept_and_named() {
    // A restore is destructive by design, and keeping what it replaced costs
    // nothing on Btrfs. It turns "this destroys your work" into "this destroys
    // your work and here is where it went".
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);
    let taken = snapshots.take("before").unwrap();

    std::fs::write(subvolume.join("written-after.txt"), "new work\n").unwrap();
    let restored = snapshots
        .restore(&taken.name, "2026-08-03T04:00:00Z")
        .unwrap();

    let kept = snapshots.directory().join(&restored.replaced_kept_as);
    assert_eq!(
        std::fs::read_to_string(kept.join("written-after.txt")).unwrap(),
        "new work\n",
        "the work the restore destroyed is not anywhere"
    );
}

#[test]
fn the_snapshot_survives_being_restored_from() {
    // Moving the snapshot into place would consume the moment it records: a
    // restore that could only be done once, and that silently destroyed the
    // thing it restored from.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);
    let taken = snapshots.take("before").unwrap();

    std::fs::write(subvolume.join("kept.txt"), "edited\n").unwrap();
    snapshots
        .restore(&taken.name, "2026-08-03T04:00:00Z")
        .unwrap();

    assert!(snapshots.find(&taken.name).is_ok(), "the snapshot is gone");

    // And again, from the same one.
    std::fs::write(subvolume.join("kept.txt"), "edited again\n").unwrap();
    snapshots
        .restore(&taken.name, "2026-08-03T05:00:00Z")
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(subvolume.join("kept.txt")).unwrap(),
        "original\n"
    );
}

#[test]
fn restoring_a_name_that_is_not_a_snapshot_touches_nothing() {
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);

    assert!(
        snapshots
            .restore("never-taken", "2026-08-03T04:00:00Z")
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(subvolume.join("kept.txt")).unwrap(),
        "original\n"
    );
}

#[test]
fn the_difference_names_what_would_be_lost_and_what_would_be_reverted() {
    // The two are not the same, and the distinction is the one the human most
    // needs before answering: a file created since the snapshot has no older
    // version to go back to, so restoring does not revert it, it deletes it.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);
    let taken = snapshots.take("before").unwrap();

    std::fs::write(subvolume.join("brand-new.txt"), "created since\n").unwrap();
    std::fs::write(subvolume.join("kept.txt"), "edited since, longer\n").unwrap();
    std::fs::remove_file(subvolume.join("doomed.txt")).unwrap();

    let diff = difference(&subvolume, &taken.path);

    assert_eq!(diff.added, ["brand-new.txt"], "{diff:?}");
    assert_eq!(diff.modified, ["kept.txt"], "{diff:?}");
    assert_eq!(diff.removed, ["doomed.txt"], "{diff:?}");
    assert_eq!(diff.lost_outright(), 1);
}

#[test]
fn an_untouched_subvolume_differs_from_its_snapshot_in_nothing() {
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);
    let taken = snapshots.take("before").unwrap();

    let diff = difference(&subvolume, &taken.path);
    assert!(diff.is_empty(), "{diff:?}");
}

#[test]
fn the_snapshot_directory_is_not_compared_against_itself() {
    // It lives beside the subvolume, but a caller is free to point this at a
    // tree that contains it, and every snapshot would then read as work that
    // a restore would destroy.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);
    let first = snapshots.take("first").unwrap();
    snapshots.take("second").unwrap();

    let diff = difference(&subvolume, &first.path);
    assert!(
        !diff.added.iter().any(|path| path.contains("second")),
        "a snapshot was reported as work a restore would destroy: {diff:?}"
    );
}

#[test]
fn the_listing_is_bounded_and_the_counts_are_not() {
    // A confirmation nobody can read is a confirmation nobody gives
    // meaningfully. The count still has to be true.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);
    let taken = snapshots.take("before").unwrap();

    let many = thalyx_snapshot::Difference::SHOWN * 3;
    for n in 0..many {
        std::fs::write(subvolume.join(format!("file-{n:04}.txt")), "new\n").unwrap();
    }

    let diff = difference(&subvolume, &taken.path);
    assert_eq!(diff.added.len(), thalyx_snapshot::Difference::SHOWN);
    assert_eq!(diff.added_total, many);
    assert_eq!(diff.lost_outright(), many);
}

#[test]
fn a_file_rewritten_with_the_same_length_is_still_a_difference() {
    // Same size, different content. Comparing sizes alone would call this
    // identical and the human would be told a restore costs nothing.
    let (_dir, subvolume) = subvolume();
    let snapshots = Snapshots::of(Directories, &subvolume);
    let taken = snapshots.take("before").unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(subvolume.join("kept.txt"), "0riginal\n").unwrap();

    let diff = difference(&subvolume, &taken.path);
    assert_eq!(diff.modified, ["kept.txt"], "{diff:?}");
}
