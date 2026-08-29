//! The exact identity of a workspace, and the case counting got wrong.
//!
//! `vault/03-Primitivas/Identidad-de-Estado.md`. Until 2026-08-29 a rollback in
//! one call was authorised by a claim about **how many** files had been added
//! and how many modified. These tests are the counterexample that retired that
//! design, written as assertions so it cannot come back: a third party editing
//! a file the agent had already edited moves neither count, and the whole
//! protection was those two numbers.
//!
//! Nothing here needs Btrfs. A witness is a walk of an ordinary tree, which is
//! the point — the identity of a workspace must be answerable on the machine
//! that is asking, not only on the one filesystem.

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

/// Write, and make sure the write is visible to a timestamp comparison.
///
/// The sleep is the honest part of this file. A filesystem's timestamp
/// granularity is coarser than two writes in a row from the same program, so a
/// test that wrote twice without waiting would be measuring the clock rather
/// than the witness — and it would fail intermittently, which is the worst way
/// for a test to be wrong. `write_later` states the assumption out loud instead
/// of relying on the machine being slow.
fn write_later(path: &Path, text: &str) {
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(path, text).expect("the write");
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
    let live = tree(&[("foo.rs", "original\n"), ("other.rs", "untouched\n")]);
    let snapshot = tree(&[("foo.rs", "original\n"), ("other.rs", "untouched\n")]);

    write_later(&live.path().join("foo.rs"), "what the agent wrote\n");
    let after_the_agent = difference(live.path(), snapshot.path());
    let agent_state = witness(live.path());

    write_later(&live.path().join("foo.rs"), "what the person wrote\n");
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
fn a_write_that_keeps_the_length_still_moves_the_witness() {
    // Same number of bytes, so `size` is no help; the modification time and the
    // change time are what carry it.
    let dir = tree(&[("foo.rs", "aaaa\n")]);
    let before = witness(dir.path());
    write_later(&dir.path().join("foo.rs"), "bbbb\n");
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
    // sizes, the same times, the same inodes. The witness is an identity of the
    // state and not a counter of events, so it comes back.
    assert_eq!(empty_handed.id, witness(dir.path()).id);
}

#[test]
fn a_witness_counts_the_files_it_weighed() {
    let dir = tree(&[("a.rs", "one\n"), ("deep/b.rs", "two\n")]);
    let taken = witness(dir.path());
    assert_eq!(taken.files, 2);
    assert!(taken.is_complete());
}

#[test]
fn a_tree_with_a_hole_in_it_never_matches_anything_including_itself() {
    // Rule 9 and rule 10 together. A directory that could not be opened is not
    // a directory that is empty, and a witness made over one must not be able
    // to authorise replacing the tree it did not finish reading.
    let incomplete = Witness {
        id: "w1-whatever".to_string(),
        files: 3,
        unreadable: 1,
    };
    assert!(!incomplete.is_complete());
    assert!(!incomplete.matches("w1-whatever"));
}

#[test]
fn the_difference_and_the_witness_come_from_the_same_instant() {
    // The pair is one walk, so a caller cannot be handed a plan for one state
    // and a witness for another — which is the race the witness exists to
    // close, reappearing inside the thing that closes it.
    let live = tree(&[("foo.rs", "original\n")]);
    let snapshot = tree(&[("foo.rs", "original\n")]);
    write_later(&live.path().join("foo.rs"), "changed\n");

    let (difference, taken) = thalyx_snapshot::difference_and_witness(live.path(), snapshot.path());
    assert_eq!(difference.modified_total, 1);
    assert_eq!(taken.id, witness(live.path()).id);
}
