//! The four operations `intento` needs, against a real Btrfs and no `btrfs` binary.
//!
//! ## The defect
//!
//! `make -C image agent` creates the agent's workspace as a real subvolume, and
//! inside the running machine `thalyx_attempt` answered `not_a_subvolume`.
//! [`thalyx_snapshot::Btrfs`] asks by running `btrfs subvolume show`, and
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md` puts the kernel and one
//! program in the image — so the spawn failed, and `is_subvolume` has no way to
//! say *I could not ask*. A missing binary was reported as a fact about the
//! filesystem, which is rule 10 of `Estrategia-de-Pruebas.md` from the wrong side.
//!
//! ## What is graded here, and what is not
//!
//! Only the boundary [`thalyx_snapshot::Native`] is: the kernel answers about a
//! subvolume that a `btrfs` binary made, the snapshot it takes is read-only *by
//! the flag the kernel reports* and not merely by a write failing, the restore is
//! writable, and the delete removes something `rmdir` cannot. Naming, ordering and
//! every other decision stay where they were, exercised against the directory fake
//! on machines with no Btrfs at all.
//!
//! Two things are here on purpose that a smaller test would leave out.
//!
//! **The second opinion.** Where btrfs-progs is installed, [`thalyx_snapshot::Btrfs`]
//! is asked the same question about the same path, and the two must agree. A
//! native `is_subvolume` that answered `true` for every directory would otherwise
//! pass everything below it.
//!
//! **The control.** A path that is *not* a subvolume must come back `false` rather
//! than as an error, and a plain directory on a filesystem that is not Btrfs at
//! all must come back `false` too — the two ways of not being one, which the
//! kernel distinguishes with `EINVAL` and `ENOTTY`. Without them a backend that
//! failed closed on everything would look identical to one that works.
//!
//! ## The skip
//!
//! It needs a writable Btrfs filesystem, named by `THALYX_BTRFS_SCRATCH`. Where
//! there is none it says `NOT PROVEN` and `THALYX_REQUIRE_BTRFS_TESTS=1` turns the
//! skip into a failure — one variable for one requirement, rule 3.

use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use thalyx_snapshot::{Native, Snapshots, Volumes, name_for};

/// A throwaway subvolume on a real Btrfs filesystem, or nothing.
///
/// Made with the ioctl and not with `btrfs subvolume create`, which is not
/// tidiness: this file has to be able to run on the machine it was written for,
/// and that machine is the one with no btrfs-progs on it.
fn scratch(label: &str) -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var("THALYX_BTRFS_SCRATCH").ok()?);
    // The caller's label is in the name, and rule 11 is why: `cargo test` runs
    // the four tests in this file as threads of **one** process, so a name made
    // of the pid alone is one subvolume shared by all of them — and this
    // function starts by deleting it. Each one would then be tearing down the
    // tree the others were measuring.
    let name = format!("thalyx-native-{label}-{}", std::process::id());
    let subvolume = base.join(&name);

    let _ = Native.delete(&subvolume);
    let directory = std::fs::File::open(&base).ok()?;
    thalyx_syscall::btrfs_subvolume_create(directory.as_fd(), &name).ok()?;
    Some(subvolume)
}

/// What to say when there is nowhere to run.
fn unproven(what: &str) {
    assert!(
        std::env::var("THALYX_REQUIRE_BTRFS_TESTS").is_err(),
        "THALYX_REQUIRE_BTRFS_TESTS is set and no Btrfs subvolume could be made"
    );
    eprintln!(
        "NOT PROVEN: {what} did not run. It needs a writable Btrfs filesystem \
         (THALYX_BTRFS_SCRATCH=<path on btrfs>)."
    );
}

/// The flags the kernel reports for a subvolume, which is how read-only is checked.
///
/// Not by trying to write to it. A write that fails proves a write failed — the
/// mount could be read-only, the directory could be unwritable — and the property
/// under test is that Thalyx asked for `BTRFS_SUBVOL_RDONLY` and the kernel took
/// it. That is a different sentence, and only this one is about the flag.
fn flags(path: &Path) -> u64 {
    let directory = std::fs::File::open(path).expect("the subvolume opens");
    thalyx_syscall::btrfs_subvolume_flags(directory.as_fd()).expect("it is a subvolume")
}

fn clean(subvolume: &Path) {
    let snapshots = Snapshots::of(Native, subvolume);
    if let Ok(taken) = snapshots.list() {
        for snapshot in taken {
            let _ = Native.delete(&snapshot.path);
        }
    }
    if let Ok(entries) = std::fs::read_dir(snapshots.directory()) {
        for entry in entries.flatten() {
            let _ = Native.delete(&entry.path());
        }
    }
    let _ = std::fs::remove_dir_all(snapshots.directory());
    let _ = Native.delete(subvolume);
}

#[test]
fn the_kernel_is_asked_whether_something_is_a_subvolume_and_no_binary_is_run() {
    let Some(subvolume) = scratch("is-subvolume") else {
        return unproven("the_kernel_is_asked_whether_something_is_a_subvolume");
    };

    let plain = subvolume.join("an-ordinary-directory");
    std::fs::create_dir_all(&plain).expect("a directory inside it");

    let native = Native;
    let yes = native.is_subvolume(&subvolume);
    let no = native.is_subvolume(&plain);

    // The control that is not on Btrfs at all: `ENOTTY` rather than `EINVAL`, and
    // the same plain `false`. This is the one the container can be wrong about.
    let elsewhere = tempfile::tempdir().expect("somewhere that is not Btrfs");
    let off_btrfs = native.is_subvolume(elsewhere.path());

    // And a path with nothing at it: an answer, not a failure to read one.
    let missing = native.is_subvolume(&subvolume.join("no-such-thing"));

    // The second opinion, where there is one. The two backends are asked the same
    // question about the same paths, because a native answer of `true` for
    // everything would pass every assertion in this file on its own.
    let command = thalyx_snapshot::Btrfs::new();
    let agreed = match (
        command.is_subvolume(&subvolume),
        command.is_subvolume(&plain),
    ) {
        (Ok(true), Ok(false)) => "yes",
        _ => "no btrfs-progs here to ask",
    };

    clean(&subvolume);

    assert!(
        yes.expect("the kernel answered"),
        "a real subvolume was not recognised, which is the bug this file is about"
    );
    assert!(
        !no.expect("the kernel answered"),
        "an ordinary directory inside a subvolume was called a subvolume"
    );
    assert!(
        !off_btrfs.expect("ENOTTY is an answer"),
        "a directory on a filesystem that is not Btrfs was called a subvolume"
    );
    assert!(
        !missing.expect("a missing path is an answer"),
        "a path with nothing at it was called a subvolume"
    );
    eprintln!("the `btrfs` command was asked the same question and agreed: {agreed}");
}

#[test]
fn a_native_snapshot_is_read_only_by_the_flag_the_kernel_reports() {
    let Some(subvolume) = scratch("read-only") else {
        return unproven("a_native_snapshot_is_read_only");
    };

    let snapshots = Snapshots::of(Native, &subvolume);
    std::fs::write(subvolume.join("notes.txt"), "as it was\n").expect("a file to snapshot");

    let taken = snapshots
        .take(&name_for("native", "2026-08-28T04:00:00Z"))
        .expect("the snapshot");

    std::fs::write(subvolume.join("notes.txt"), "changed since\n").expect("changing it");

    let held = std::fs::read_to_string(taken.path.join("notes.txt")).expect("reading the snapshot");
    let reported = flags(&taken.path);
    let refused = std::fs::write(taken.path.join("notes.txt"), "tampered").is_err();
    let is_one = Native.is_subvolume(&taken.path);

    clean(&subvolume);

    assert_eq!(
        held, "as it was\n",
        "the snapshot moved with the subvolume, so it is not a snapshot"
    );
    assert_eq!(
        reported & thalyx_syscall::BTRFS_SUBVOL_RDONLY,
        thalyx_syscall::BTRFS_SUBVOL_RDONLY,
        "the kernel reports flags {reported:#x}, so RDONLY was not asked for in the \
         ioctl that created it"
    );
    assert!(refused, "the snapshot was writable");
    assert!(
        is_one.expect("the kernel answered"),
        "a snapshot is a subvolume"
    );
}

#[test]
fn restoring_makes_a_writable_copy_and_deleting_takes_it_away_again() {
    let Some(subvolume) = scratch("restoring") else {
        return unproven("restoring_makes_a_writable_copy");
    };

    let snapshots = Snapshots::of(Native, &subvolume);
    std::fs::write(subvolume.join("notes.txt"), "as it was\n").expect("a file to snapshot");
    let taken = snapshots
        .take(&name_for("native", "2026-08-28T05:00:00Z"))
        .expect("the snapshot");

    // What `restore` does before it swaps: a writable copy, and never the snapshot
    // itself. A restore that moved the snapshot into place would consume the
    // moment it records.
    let copy = snapshots.directory().join("a-writable-copy");
    Native
        .restore_from(&taken.path, &copy)
        .expect("the writable copy");

    let copied = std::fs::read_to_string(copy.join("notes.txt")).expect("reading the copy");
    let reported = flags(&copy);
    let written = std::fs::write(copy.join("notes.txt"), "and it can be written\n").is_ok();

    // `rmdir` will not have a subvolume, and neither will `remove_dir_all` — which
    // is the whole reason `delete` is an ioctl and not a call to `std::fs`.
    let by_hand = std::fs::remove_dir_all(&copy).is_err();
    let deleted = Native.delete(&copy);
    let gone = !copy.exists();

    clean(&subvolume);

    assert_eq!(copied, "as it was\n", "the copy is not of the snapshot");
    assert_eq!(
        reported & thalyx_syscall::BTRFS_SUBVOL_RDONLY,
        0,
        "the copy a restore swaps in came back read-only (flags {reported:#x}), so \
         the tree would be unwritable afterwards"
    );
    assert!(
        written,
        "the copy a restore swaps in could not be written to"
    );
    assert!(
        by_hand,
        "an ordinary recursive remove took the subvolume away, so this test is not \
         about a subvolume"
    );
    deleted.expect("the kernel deleted the subvolume");
    assert!(gone, "the subvolume is still there after being deleted");
}

#[test]
fn taking_a_snapshot_of_something_that_is_not_a_subvolume_refuses_rather_than_copying() {
    // The refusal, on the machine where the alternative would have worked. On a
    // Btrfs filesystem a plain directory is snapshottable-looking in every way
    // except the one that matters, and a backend that quietly copied it would
    // hand back something `intento abandonar` could not put back atomically.
    let Some(subvolume) = scratch("not-a-subvolume") else {
        return unproven("taking_a_snapshot_of_something_that_is_not_a_subvolume");
    };

    let plain = subvolume.join("an-ordinary-directory");
    std::fs::create_dir_all(&plain).expect("a directory inside it");
    std::fs::write(plain.join("notes.txt"), "as it was\n").expect("something in it");

    let refused = Snapshots::of(Native, &plain).take("2026-08-28T06-00-00Z-native");
    let outcome = format!("{refused:?}");

    clean(&subvolume);

    assert!(
        matches!(
            refused,
            Err(thalyx_snapshot::SnapshotError::NotASubvolume(_))
        ),
        "a plain directory was snapshotted instead of refused: {outcome}"
    );
}
