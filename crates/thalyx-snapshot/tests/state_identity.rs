//! The exact identity of a workspace, and the two things that got it wrong.
//!
//! `vault/03-Primitivas/Identidad-de-Estado.md`. Until 2026-08-29 a rollback in
//! one call was authorised by a claim about **how many** files had been added
//! and how many modified. These tests are the counterexample that retired that
//! design, written as assertions so it cannot come back: a third party editing
//! a file the agent had already edited moves neither count, and the whole
//! protection was those two numbers.
//!
//! ## Why there is not a `sleep` in this file any more
//!
//! There was one, and it was the defect confessing. Two writes in a row from
//! one program can land inside a single filesystem timestamp tick, so the
//! version of these tests that compared metadata had to wait twenty
//! milliseconds between the agent's write and the third party's — otherwise it
//! failed intermittently, which is the worst way for a test to be wrong.
//!
//! But **that wait is the real case**. On Fedora, `dev/verify.sh` stage 55
//! deliberately does not sleep: the agent writes, takes the state, and a person
//! writes the same file immediately. A witness that needs the clock to have
//! moved cannot tell those two trees apart, and the person's work goes back to
//! the snapshot. So every write below is consecutive and none of them waits,
//! and the ones that would have been ambiguous under the old rules are marked.
//!
//! Nothing here needs Btrfs. A witness is a walk of an ordinary tree, which is
//! the point — the identity of a workspace must be answerable on the machine
//! that is asking, not only on the one filesystem.

use std::io::Write;
use std::path::Path;
use thalyx_snapshot::{Witness, difference, witness};

fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a directory");
    for (path, text) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("the parent");
        }
        std::fs::write(&full, text).expect("the file");
    }
    dir
}

/// Whether two states of one file could have been told apart by metadata alone.
///
/// Used to say out loud, in the tests that matter, that the case being asserted
/// really is the hard one on this machine — and not one that happened to cross
/// a timestamp tick and would therefore have passed under the rules this file
/// exists to retire.
fn metadata_alone_would_separate(path: &Path, before: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    let now = std::fs::symlink_metadata(path).expect("the file is still there");
    now.len() != before.len()
        || now.mtime_nsec() != before.mtime_nsec()
        || now.mtime() != before.mtime()
        || now.ctime_nsec() != before.ctime_nsec()
        || now.ctime() != before.ctime()
        || now.ino() != before.ino()
}

#[test]
fn a_tree_that_nobody_touched_has_the_same_witness_twice() {
    let dir = tree(&[
        ("src/main.rs", "fn main() {}\n"),
        ("Cargo.toml", "[package]\n"),
    ]);
    assert_eq!(witness(dir.path()).id, witness(dir.path()).id);
}

#[test]
fn two_trees_holding_different_bytes_do_not_share_a_witness() {
    let one = tree(&[("a.rs", "fn a() {}\n")]);
    let other = tree(&[("a.rs", "fn b() {}\n")]);
    assert_ne!(witness(one.path()).id, witness(other.path()).id);
}

#[test]
fn a_witness_says_which_rules_made_it() {
    // A witness from another version of Thalyx must be refusable on sight
    // rather than compared under rules it was not made under. Rule 9.
    let dir = tree(&[("a.rs", "fn a() {}\n")]);
    assert!(
        witness(dir.path())
            .id
            .starts_with(thalyx_snapshot::WITNESS_VERSION),
        "a witness has to name its own version"
    );
}

#[test]
fn writing_to_a_file_that_was_already_modified_moves_the_witness_and_not_the_counts() {
    // **The defect this whole mechanism exists for.** The tree is snapshotted;
    // the agent edits one file; a third party then edits *the same* file. The
    // counts are identical before and after that second edit, so a rollback
    // authorised by counts would proceed and take the third party's work.
    //
    // Same length, and no wait: this is stage 55's sequence, which is the one
    // that actually happens.
    let live = tree(&[("foo.rs", "original\n"), ("other.rs", "untouched\n")]);
    let snapshot = tree(&[("foo.rs", "original\n"), ("other.rs", "untouched\n")]);

    std::fs::write(live.path().join("foo.rs"), "what the agent wrote\n").expect("the write");
    let after_the_agent = difference(live.path(), snapshot.path());
    let agent_state = witness(live.path());

    std::fs::write(live.path().join("foo.rs"), "what the human wrote\n").expect("the write");
    let after_the_person = difference(live.path(), snapshot.path());
    let person_state = witness(live.path());

    assert_eq!(
        (after_the_agent.added_total, after_the_agent.modified_total),
        (
            after_the_person.added_total,
            after_the_person.modified_total
        ),
        "the counts are what the old protection was made of, and this is them \
         failing to notice — if this ever fails, the counterexample changed"
    );
    assert_ne!(
        agent_state.id, person_state.id,
        "a witness that cannot tell those two states apart is a witness that \
         authorises destroying the second one"
    );
}

#[test]
fn two_writes_of_the_same_length_in_a_row_are_two_states_with_no_wait_between_them() {
    // The requirement, stated on its own: same file, same size, consecutive
    // writes, and nothing may depend on the clock having ticked. This is the
    // test the twenty-millisecond sleep was hiding.
    let dir = tree(&[("shared.txt", "..............\n")]);
    let file = dir.path().join("shared.txt");

    std::fs::write(&file, "what the agent wrote\n").expect("the write");
    let before = witness(dir.path());
    let metadata = std::fs::symlink_metadata(&file).expect("the file");

    std::fs::write(&file, "what the human wrote\n").expect("the write");
    let after = witness(dir.path());

    assert_ne!(
        before.id, after.id,
        "two consecutive same-length writes to one file produced one state"
    );

    // Said out loud rather than assumed. If this machine happens to separate
    // them by metadata the assertion above is still true but weaker, and a
    // reader deserves to know which of the two they just proved.
    if !metadata_alone_would_separate(&file, &metadata) {
        println!(
            "and metadata alone could not have separated them on this machine: \
             the old witness would have authorised destroying the second state"
        );
    }
}

#[test]
fn a_write_through_a_descriptor_that_was_already_open_moves_the_witness() {
    // The case a counter of syscalls has to work to catch and a walk gets for
    // nothing: the descriptor was opened before the state was ever taken, so
    // nothing about opening it can have been observed. The bytes change, the
    // length does not, and the write goes through the old descriptor.
    let dir = tree(&[("shared.txt", "aaaaaaaaaaaaaaaaaaaa\n")]);
    let file = dir.path().join("shared.txt");

    let mut already_open = std::fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .expect("a descriptor opened before the state was taken");

    let before = witness(dir.path());
    let metadata = std::fs::symlink_metadata(&file).expect("the file");

    already_open
        .write_all(b"bbbbbbbbbbbbbbbbbbbb\n")
        .expect("the write");
    already_open.flush().expect("the flush");

    assert_ne!(
        before.id,
        witness(dir.path()).id,
        "a write through an open descriptor left the state looking untouched"
    );
    if !metadata_alone_would_separate(&file, &metadata) {
        println!("and metadata alone could not have separated them on this machine");
    }
}

#[test]
fn a_write_outside_the_tree_does_not_move_its_witness() {
    // The other half, and the one a machine-wide counter cannot give without
    // being scoped first: the identity is of *this* workspace. Somebody
    // building in another directory must not stop a rollback here.
    let dir = tree(&[("a.rs", "one\n")]);
    let elsewhere = tree(&[("b.rs", "two\n")]);

    let before = witness(dir.path());
    std::fs::write(elsewhere.path().join("b.rs"), "rewritten entirely\n").expect("the write");
    std::fs::write(elsewhere.path().join("c.rs"), "and a new file\n").expect("the write");

    assert_eq!(
        before.id,
        witness(dir.path()).id,
        "work in another tree moved this tree's identity"
    );
}

#[test]
fn a_write_that_keeps_the_length_still_moves_the_witness() {
    // Same number of bytes, so `size` is no help, and no wait, so the
    // timestamps may be no help either. What carries it is the contents.
    let dir = tree(&[("foo.rs", "aaaa\n")]);
    let before = witness(dir.path());
    std::fs::write(dir.path().join("foo.rs"), "bbbb\n").expect("the write");
    assert_ne!(before.id, witness(dir.path()).id);
}

#[test]
fn a_file_appearing_and_a_file_vanishing_both_move_the_witness() {
    let dir = tree(&[("a.rs", "one\n")]);
    let empty_handed = witness(dir.path());

    std::fs::write(dir.path().join("b.rs"), "two\n").expect("the second file");
    let with_two = witness(dir.path());
    assert_ne!(empty_handed.id, with_two.id);

    std::fs::remove_file(dir.path().join("b.rs")).expect("the removal");
    // Back to a tree holding exactly what it held: the same files, the same
    // sizes, the same times, the same inodes, the same bytes. The witness is an
    // identity of the state and not a counter of events, so it comes back.
    assert_eq!(empty_handed.id, witness(dir.path()).id);
}

#[test]
fn a_witness_counts_the_files_it_weighed_and_the_bytes_it_read() {
    let dir = tree(&[("a.rs", "one\n"), ("deep/b.rs", "two\n")]);
    let taken = witness(dir.path());
    assert_eq!(taken.files, 2);
    assert_eq!(taken.bytes, 8, "four bytes each, and it says what it cost");
    assert!(taken.is_complete());
}

#[test]
fn a_symbolic_link_is_weighed_by_where_it_points_and_not_by_what_is_there() {
    // Following it would weigh some other tree's file — possibly one outside
    // the workspace entirely, which would make this workspace's identity move
    // when somebody edited a file that is not in it.
    let dir = tree(&[("real.txt", "contents\n")]);
    let outside = tree(&[("target.txt", "elsewhere\n")]);
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(outside.path().join("target.txt"), &link).expect("the link");

    let before = witness(dir.path());
    std::fs::write(outside.path().join("target.txt"), "rewritten\n").expect("the write");
    assert_eq!(
        before.id,
        witness(dir.path()).id,
        "editing what a link points at moved the tree the link is in"
    );

    std::fs::remove_file(&link).expect("the removal");
    std::os::unix::fs::symlink(outside.path().join("other.txt"), &link).expect("the link");
    assert_ne!(
        before.id,
        witness(dir.path()).id,
        "a link that points somewhere else is a different tree"
    );
}

#[test]
fn a_tree_with_a_hole_in_it_never_matches_anything_including_itself() {
    // Rule 9 and rule 10 together. A directory that could not be opened is not
    // a directory that is empty, and a witness made over one must not be able
    // to authorise replacing the tree it did not finish reading.
    let incomplete = Witness {
        id: "w2-whatever".to_string(),
        files: 3,
        unreadable: 1,
        bytes: 0,
    };
    assert!(!incomplete.is_complete());
    assert!(!incomplete.matches("w2-whatever"));
}

#[test]
fn a_file_whose_bytes_cannot_be_read_makes_the_witness_incomplete() {
    // The new way a walk can have a hole in it, and it has to fail closed the
    // same way the old ones did. A file that can be stat'd and not read is a
    // file nobody has compared.
    if nix_is_root() {
        // Root reads anything, so the mode says nothing here. Rule 3: a check
        // that could not be made says so instead of passing.
        println!("NOT PROVEN: running as root, where a mode cannot make a file unreadable");
        return;
    }
    let dir = tree(&[("open.txt", "readable\n"), ("shut.txt", "secret\n")]);
    let shut = dir.path().join("shut.txt");
    std::fs::set_permissions(&shut, std::os::unix::fs::PermissionsExt::from_mode(0o000))
        .expect("the mode");

    let taken = witness(dir.path());
    assert_eq!(taken.unreadable, 1, "the unreadable file was not counted");
    assert!(!taken.is_complete());
    assert!(
        !taken.matches(&taken.id),
        "a witness with a hole in it matched itself, which is an authorisation"
    );

    // And it is still one of the files: a file nobody could read is not a file
    // that is not there, which is the difference rule 10 is about.
    assert_eq!(taken.files, 2);
}

fn nix_is_root() -> bool {
    // No `unsafe` outside `thalyx-syscall`, and no dependency worth adding for
    // one number: the effective uid is in `/proc/self/status`.
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("Uid:"))
                .map(|line| line.split_whitespace().nth(1) == Some("0"))
        })
        .unwrap_or(false)
}

#[test]
fn the_difference_and_the_witness_come_from_the_same_instant() {
    // The pair is one walk, so a caller cannot be handed a plan for one state
    // and a witness for another — which is the race the witness exists to
    // close, reappearing inside the thing that closes it.
    let live = tree(&[("foo.rs", "original\n")]);
    let snapshot = tree(&[("foo.rs", "original\n")]);
    std::fs::write(
        live.path().join("foo.rs"),
        "changed, and by a different length\n",
    )
    .expect("the write");

    let (difference, taken) = thalyx_snapshot::difference_and_witness(live.path(), snapshot.path());
    assert_eq!(difference.modified_total, 1);
    assert_eq!(taken.id, witness(live.path()).id);
}

#[test]
fn the_count_of_modified_files_can_understate_where_the_witness_cannot() {
    // Said as an assertion because it is a real limit and a reader has to be
    // able to find it. `Difference` — the summary a human is shown before
    // answering — compares by size and modification time, which is what makes a
    // confirmation cost one walk of each tree instead of a read of both. Two
    // writes of the same length inside one filesystem tick can therefore be
    // *counted* as no change.
    //
    // The identity is not made of that, and that is the whole point: what
    // authorises a destruction is the witness, which reads what the file holds.
    // The summary can be one file short; the authorisation cannot be one state
    // wrong. `vault/03-Primitivas/Identidad-de-Estado.md` records why the
    // summary is not being made exact today and what it would cost.
    let live = tree(&[("foo.rs", "original\n")]);
    let snapshot = tree(&[("foo.rs", "original\n")]);

    let before = witness(live.path());
    let metadata = std::fs::symlink_metadata(live.path().join("foo.rs")).expect("the file");
    std::fs::write(live.path().join("foo.rs"), "replaced\n").expect("the write");

    assert_ne!(
        before.id,
        witness(live.path()).id,
        "the identity missed a same-length rewrite, which is the defect itself"
    );
    if !metadata_alone_would_separate(&live.path().join("foo.rs"), &metadata) {
        assert_eq!(
            difference(live.path(), snapshot.path()).modified_total,
            0,
            "if this starts noticing, the summary has become exact and this              note should be retired rather than left saying otherwise"
        );
    }
}
