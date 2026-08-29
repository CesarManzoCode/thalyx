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
//! writable, and the delete takes away something `stat(2)` says was a subvolume.
//! Naming, ordering and every other decision stay where they were, exercised
//! against the directory fake on machines with no Btrfs at all.
//!
//! ## The check that measured the kernel instead of the object
//!
//! That last sentence used to read *the delete removes something `rmdir` cannot*,
//! and the test that stood behind it asked whether `remove_dir_all` failed. It
//! does not fail: since Linux 4.18 `rmdir(2)` takes an **empty** subvolume away
//! like any other directory, so the recursive remove unlinked the one file inside
//! the copy and then unlinked the subvolume, and an assertion written to say
//! *this is a subvolume* reported that it was not. It was one — the ioctl beside
//! it had already said so. The check was measuring the kernel's `rmdir` policy,
//! which is rule 5 of `Estrategia-de-Pruebas.md` and the same shape as
//! `chrt --other` measuring util-linux. What replaced it asks `stat(2)`.
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
//! ## The interference
//!
//! On 2026-08-29 `restoring_makes_a_writable_copy_and_deleting_takes_it_away_again`
//! failed on Cesar's machine and passed with `--test-threads=1`, which is the
//! shape of rule 11 and not of a Btrfs defect: the four tests below each made a
//! *differently named* subvolume, but [`thalyx_snapshot::Snapshots::directory`]
//! puts the snapshots beside the source — in **the parent** — so all four wrote
//! into one `THALYX_BTRFS_SCRATCH/.thalyx-snapshots`, took snapshots under names
//! that collided, and each tore that directory down while the others were still
//! working in it. A unique source name isolates nothing when the thing under
//! test derives its own location from the parent.
//!
//! What replaced it is [`Arena`]: a private plain directory per fixture, with the
//! source subvolume *inside* it, so `directory()` lands inside that arena too and
//! two fixtures share no path at all. Nothing is serialised — the product is
//! meant to hold several trees at once, and a test suite that can only hold one
//! is measuring the suite. And nothing is deleted that the arena did not make,
//! which is the other half: the old helper began by deleting the path it was
//! about to create, so a fixture's first act was to destroy whatever was there.
//!
//! ## The skip
//!
//! It needs a writable Btrfs filesystem, named by `THALYX_BTRFS_SCRATCH`. Where
//! there is none it says `NOT PROVEN` and `THALYX_REQUIRE_BTRFS_TESTS=1` turns the
//! skip into a failure — one variable for one requirement, rule 3.

use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use thalyx_snapshot::{Native, Snapshots, Volumes, name_for};

use std::sync::atomic::{AtomicU64, Ordering};

/// How many arenas this process has handed out.
///
/// The whole of what makes two arenas in one process distinct. The label cannot
/// be: two fixtures are allowed to want the same one, and the pid cannot either,
/// because `cargo test` runs this file's tests as threads of **one** process.
static HANDED_OUT: AtomicU64 = AtomicU64::new(0);

/// The name of one arena, and nothing on any filesystem.
///
/// Separated from making one so that *two arenas never collide* can be graded on
/// a machine with no Btrfs — the container this file is written in. It is the
/// claim the whole fixture rests on, and it should not be provable only where
/// everything else already is.
fn arena_name(label: &str) -> String {
    format!(
        "thalyx-native-{label}-{}-{}",
        std::process::id(),
        HANDED_OUT.fetch_add(1, Ordering::Relaxed)
    )
}

/// A Btrfs arena that belongs to one test: a private directory, and a subvolume
/// in it.
///
/// The directory is the point. [`thalyx_snapshot::Snapshots::directory`] puts
/// snapshots in the source's **parent**, so giving each test its own parent is
/// what stops two of them from meeting — in the snapshot directory, in the names
/// of the snapshots inside it, or in the writable copies a restore leaves there.
/// A private *source* name, which is what this file used to have, does none of
/// that.
struct Arena {
    root: PathBuf,
    source: PathBuf,
    torn_down: bool,
}

impl Arena {
    /// Make one, or nothing where there is no Btrfs to make it on.
    ///
    /// With the ioctl and not with `btrfs subvolume create`: this file has to be
    /// able to run on the machine it was written for, and that machine is the one
    /// with no btrfs-progs on it.
    fn make(label: &str) -> Option<Arena> {
        let base = PathBuf::from(std::env::var("THALYX_BTRFS_SCRATCH").ok()?);
        let root = base.join(arena_name(label));

        // `create_dir` and not `create_dir_all`, and nothing is deleted first:
        // an arena either made its own root or there is no arena. The helper this
        // replaced opened by deleting the path it wanted, which is how one test
        // came to tear down the tree another was measuring.
        std::fs::create_dir(&root).ok()?;

        // Where the scratch is not on Btrfs the ioctl says `ENOTTY`, and the root
        // just made would otherwise stay behind — a skip that litters is a skip
        // that will be blamed for the next run's debris.
        let made = std::fs::File::open(&root).ok().and_then(|directory| {
            thalyx_syscall::btrfs_subvolume_create(directory.as_fd(), "source").ok()
        });
        if made.is_none() {
            let _ = std::fs::remove_dir(&root);
            return None;
        }

        Some(Arena {
            source: root.join("source"),
            root,
            torn_down: false,
        })
    }

    /// The subvolume under test.
    fn source(&self) -> &Path {
        &self.source
    }

    /// Everything this arena owns, snapshot directory included.
    fn root(&self) -> &Path {
        &self.root
    }

    fn snapshots(&self) -> Snapshots<Native> {
        Snapshots::of(Native, &self.source)
    }

    /// Take away what this arena made, and say so if something would not go.
    ///
    /// Asserting rather than printing: a subvolume left behind is debris the next
    /// run cannot `rmdir`, and a fixture that quietly fails to clean up is how a
    /// suite starts depending on the order it ran in.
    fn clean(&mut self) {
        let failures = self.tear_down();
        assert!(
            failures.is_empty(),
            "the fixture could not take back what it made: {}",
            failures.join("; ")
        );
    }

    /// The removal itself, idempotent, reporting rather than panicking.
    ///
    /// Only paths under [`Arena::root`], which is the ownership rule: a shared
    /// directory is never removed here, because the arena that would remove it
    /// cannot know whether another one is still inside.
    fn tear_down(&mut self) -> Vec<String> {
        if self.torn_down {
            return Vec::new();
        }
        self.torn_down = true;

        let mut failures = Vec::new();
        let snapshots = self.snapshots().directory();

        // Each snapshot by the ioctl, because `remove_dir_all` cannot take one
        // away: it starts by unlinking the files inside, and a read-only
        // subvolume refuses that. What is left over is swept by the recursive
        // remove at the end.
        if let Ok(entries) = std::fs::read_dir(&snapshots) {
            for entry in entries.flatten() {
                let path = entry.path();
                if Native.is_subvolume(&path).unwrap_or(false)
                    && let Err(error) = Native.delete(&path)
                {
                    failures.push(format!("{}: {error}", path.display()));
                }
            }
        }

        if let Err(error) = Native.delete(&self.source) {
            failures.push(format!("{}: {error}", self.source.display()));
        }
        if let Err(error) = std::fs::remove_dir_all(&self.root) {
            failures.push(format!("{}: {error}", self.root.display()));
        }

        failures
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        // The path where a test failed: it never reached its `clean()`, and what
        // it leaves behind is a subvolume. Reported and not asserted, because a
        // panic while unwinding aborts the process and takes the real failure's
        // message with it.
        for failure in self.tear_down() {
            eprintln!("the fixture could not take back what it made: {failure}");
        }
    }
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

/// `BTRFS_FIRST_FREE_OBJECTID`, from `include/uapi/linux/btrfs_tree.h`.
///
/// The inode number the root of every Btrfs subvolume has. `thalyx-syscall`
/// declines to *implement* `is_subvolume` with this number, for a stated reason —
/// it is true of Btrfs today and it is not an interface anybody promised. Grading
/// with it is the opposite situation: what a test needs is a source that is not
/// the one under test, and this one is not.
const BTRFS_FIRST_FREE_OBJECTID: u64 = 256;

/// What `stat(2)` calls a path: its inode number and its device.
///
/// The independent witness, and independence is the whole of it.
/// [`thalyx_snapshot::Native::is_subvolume`] asks `BTRFS_IOC_SUBVOL_GETFLAGS`, and
/// so does [`flags`] above, so a test that graded a subvolume by either of them
/// would be checking one ioctl against itself. These two numbers come from
/// `stat(2)`: on Btrfs the root of a subvolume is inode
/// [`BTRFS_FIRST_FREE_OBJECTID`], and the kernel hands every subvolume its own
/// anonymous device, so its `st_dev` is not the one of the directory holding it.
/// That pair is what `libbtrfsutil`'s `btrfs_util_is_subvolume` looks at, which is
/// why it is a second source rather than a guess invented here.
fn names(path: &Path) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;

    let about = std::fs::metadata(path).expect("stat(2) has something to say about it");
    (about.ino(), about.dev())
}

#[test]
fn the_kernel_is_asked_whether_something_is_a_subvolume_and_no_binary_is_run() {
    let Some(mut arena) = Arena::make("is-subvolume") else {
        return unproven("the_kernel_is_asked_whether_something_is_a_subvolume");
    };
    let subvolume = arena.source().to_path_buf();

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

    arena.clean();

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
    let Some(mut arena) = Arena::make("read-only") else {
        return unproven("a_native_snapshot_is_read_only");
    };
    let subvolume = arena.source().to_path_buf();

    let snapshots = arena.snapshots();
    std::fs::write(subvolume.join("notes.txt"), "as it was\n").expect("a file to snapshot");

    let taken = snapshots
        .take(&name_for("native", "2026-08-28T04:00:00Z"))
        .expect("the snapshot");

    std::fs::write(subvolume.join("notes.txt"), "changed since\n").expect("changing it");

    let held = std::fs::read_to_string(taken.path.join("notes.txt")).expect("reading the snapshot");
    let reported = flags(&taken.path);
    let refused = std::fs::write(taken.path.join("notes.txt"), "tampered").is_err();
    let is_one = Native.is_subvolume(&taken.path);

    arena.clean();

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
    let Some(mut arena) = Arena::make("restoring") else {
        return unproven("restoring_makes_a_writable_copy");
    };
    let subvolume = arena.source().to_path_buf();

    let snapshots = arena.snapshots();
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

    // That the thing being deleted is a subvolume, asked of `stat(2)` and of the
    // `btrfs` command — never of the ioctl the code under test uses, and never of
    // whether some other way of removing it failed.
    let holder = names(&snapshots.directory());
    let witness = names(&copy);

    // The negative control, made in the same directory on the same filesystem: an
    // ordinary directory must not carry the witness. Without it a witness that
    // said *subvolume* about everything would pass.
    let ordinary = snapshots.directory().join("an-ordinary-directory");
    std::fs::create_dir_all(&ordinary).expect("a directory beside the copy");
    let control = names(&ordinary);

    // And the second opinion where btrfs-progs is installed, asked about the same
    // two paths, the way `the_kernel_is_asked_whether_something_is_a_subvolume`
    // asks it.
    let command = thalyx_snapshot::Btrfs::new();
    let agreed = match (command.is_subvolume(&copy), command.is_subvolume(&ordinary)) {
        (Ok(true), Ok(false)) => "yes",
        _ => "no btrfs-progs here to ask",
    };
    eprintln!("the `btrfs` command was asked the same question and agreed: {agreed}");
    let _ = std::fs::remove_dir_all(&ordinary);

    let deleted = Native.delete(&copy);
    let gone = !copy.exists();

    arena.clean();

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
    assert_eq!(
        witness.0, BTRFS_FIRST_FREE_OBJECTID,
        "the copy a restore swaps in is inode {} and the root of a subvolume is \
         inode {BTRFS_FIRST_FREE_OBJECTID}, so this test is not about a subvolume",
        witness.0
    );
    assert_ne!(
        witness.1, holder.1,
        "the copy a restore swaps in shares device {:#x} with the directory holding \
         it, and the kernel gives every subvolume its own, so this test is not \
         about a subvolume",
        witness.1
    );
    assert_ne!(
        control.0, BTRFS_FIRST_FREE_OBJECTID,
        "an ordinary directory came back as inode {BTRFS_FIRST_FREE_OBJECTID}, so \
         the witness above says `subvolume` about everything and proves nothing"
    );
    assert_eq!(
        control.1, holder.1,
        "an ordinary directory has device {:#x} and the directory holding it has \
         {:#x}, so the witness above says `subvolume` about everything",
        control.1, holder.1
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
    let Some(mut arena) = Arena::make("not-a-subvolume") else {
        return unproven("taking_a_snapshot_of_something_that_is_not_a_subvolume");
    };

    let plain = arena.source().join("an-ordinary-directory");
    std::fs::create_dir_all(&plain).expect("a directory inside it");
    std::fs::write(plain.join("notes.txt"), "as it was\n").expect("something in it");

    let refused = Snapshots::of(Native, &plain).take("2026-08-28T06-00-00Z-native");
    let outcome = format!("{refused:?}");

    arena.clean();

    assert!(
        matches!(
            refused,
            Err(thalyx_snapshot::SnapshotError::NotASubvolume(_))
        ),
        "a plain directory was snapshotted instead of refused: {outcome}"
    );
}

/// Two arenas never collide, graded where there is no Btrfs at all.
///
/// The claim every test above rests on, and it is deliberately not the same
/// claim as the one below: this one is about the naming, so it runs everywhere —
/// including the container, which has no Btrfs and therefore proves nothing else
/// in this file. A label is not what separates two arenas, and neither is the
/// pid: `cargo test` runs this file as threads of one process, and two fixtures
/// asking for the same label is exactly what the test below does on purpose.
#[test]
fn two_arenas_asked_for_under_one_label_are_never_given_the_same_name() {
    let first = arena_name("the-same-label");
    let second = arena_name("the-same-label");

    assert_ne!(
        first, second,
        "two arenas were given one name, so a fixture would adopt another's tree"
    );
    assert!(
        first.contains("the-same-label") && first.contains(&std::process::id().to_string()),
        "the name says nothing about who made it, which is what debris is read by: {first}"
    );
}

/// Cleaning one arena leaves the other whole, on a real Btrfs.
///
/// The control for the interference this file's fixture exists to stop, and it
/// is deterministic rather than timed: no threads, no sleeps, no waiting to see
/// whether two tests happen to overlap. Both arenas ask for **the same label**
/// and both snapshots are taken under **the same name** — the exact collision
/// that a shared `.thalyx-snapshots` turned into a failure — and then one arena
/// is torn down while the other is still holding its snapshot. Under the old
/// fixture the two names were one path and this could not even be written.
#[test]
fn cleaning_one_arena_leaves_the_other_arenas_snapshot_untouched() {
    let (Some(mut first), Some(mut second)) =
        (Arena::make("side-by-side"), Arena::make("side-by-side"))
    else {
        return unproven("cleaning_one_arena_leaves_the_other_arenas_snapshot_untouched");
    };

    let name = name_for("native", "2026-08-29T04:00:00Z");
    std::fs::write(first.source().join("notes.txt"), "the first\n").expect("a file to snapshot");
    std::fs::write(second.source().join("notes.txt"), "the second\n").expect("a file to snapshot");

    let one = first.snapshots().take(&name).expect("the first snapshot");
    let two = second.snapshots().take(&name).expect("the second snapshot");

    let separate = first.snapshots().directory() != second.snapshots().directory();
    // Neither arena may sit inside the other, or tearing one down by its root
    // would take the other with it however different their names are.
    let nested = first.snapshots().directory().starts_with(second.root())
        || second.snapshots().directory().starts_with(first.root());

    first.clean();

    let survived = std::fs::read_to_string(two.path.join("notes.txt"));
    let held = second.snapshots().list().map(|taken| taken.len());
    let gone = !one.path.exists() && !first.root().exists();

    second.clean();

    assert!(
        separate,
        "two arenas share a snapshot directory, so one test's `clean` is another's teardown"
    );
    assert!(!nested, "one arena is inside the other");
    assert_eq!(
        survived.expect("the other arena's snapshot is still readable"),
        "the second\n",
        "cleaning one arena changed what the other's snapshot holds"
    );
    assert_eq!(
        held.expect("the other arena's snapshot directory is still there"),
        1,
        "cleaning one arena emptied the other's snapshot directory"
    );
    assert!(
        gone,
        "the arena that was cleaned is still on disk, so cleanup is not ownership-based \
         in the direction that matters either"
    );
}
