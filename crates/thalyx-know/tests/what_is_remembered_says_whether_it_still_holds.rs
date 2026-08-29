//! The one rule this crate has, tested as the three answers it can give.

use std::path::PathBuf;
use thalyx_know::{Knowledge, Over, Standing, witness};

fn tree() -> (tempfile::TempDir, PathBuf) {
    let held = tempfile::tempdir().expect("a directory");
    let root = held.path().to_path_buf();
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::write(root.join("src").join("a.rs"), "fn a() {}\n").expect("a");
    std::fs::write(root.join("src").join("b.rs"), "fn b() {}\n").expect("b");
    std::fs::write(root.join("notes.md"), "not code\n").expect("notes");
    (held, root)
}

fn over(roots: &[PathBuf]) -> thalyx_know::Witness {
    witness(&Over {
        roots,
        suffixes: &[".rs"],
        skip: &["target"],
    })
}

#[test]
fn nothing_remembered_is_unknown_and_not_a_guess() {
    let store = Knowledge::in_memory().expect("a store");
    let (_held, root) = tree();
    assert!(
        store
            .recall("kind", "never seen", &over(&[root]))
            .expect("a recall")
            .is_none()
    );
}

#[test]
fn what_was_learned_about_an_unchanged_tree_is_current() {
    let store = Knowledge::in_memory().expect("a store");
    let (_held, root) = tree();
    let identity = over(std::slice::from_ref(&root));
    store
        .remember("kind", "key", &identity, "a test", "the answer")
        .expect("a write");

    let held = store
        .recall("kind", "key", &over(&[root]))
        .expect("a recall")
        .expect("something");
    assert_eq!(held.standing, Standing::Current);
    assert_eq!(held.value, "the answer");
    assert_eq!(held.source, "a test");
}

#[test]
fn a_changed_input_makes_it_stale_and_never_current() {
    let store = Knowledge::in_memory().expect("a store");
    let (_held, root) = tree();
    let identity = over(std::slice::from_ref(&root));
    store
        .remember("kind", "key", &identity, "a test", "the answer")
        .expect("a write");

    std::fs::write(root.join("src").join("a.rs"), "fn a() { let x = 1; }\n").expect("a change");
    let now = over(std::slice::from_ref(&root));

    let held = store
        .recall("kind", "key", &now)
        .expect("a recall")
        .expect("something");
    assert!(matches!(held.standing, Standing::Stale { .. }));
    assert_eq!(
        held.value, "the answer",
        "the value is still handed over — marked. A cache that hid it would \
         cost a caller the ability to say `this is what I knew, and it moved`"
    );
    assert!(
        store
            .recall_current("kind", "key", &now)
            .expect("a recall")
            .is_none(),
        "the one thing that must never happen"
    );
}

#[test]
fn a_file_the_witness_does_not_cover_changes_nothing() {
    let store = Knowledge::in_memory().expect("a store");
    let (_held, root) = tree();
    let identity = over(std::slice::from_ref(&root));
    store
        .remember("kind", "key", &identity, "a test", "the answer")
        .expect("a write");

    std::fs::write(root.join("notes.md"), "still not code, but different\n").expect("a change");
    assert!(
        store
            .recall_current("kind", "key", &over(&[root]))
            .expect("a recall")
            .is_some(),
        "the witness covers `.rs` and a markdown file is not one. Invalidating \
         on it would be a false miss on every edit to a README"
    );
}

#[test]
fn the_same_bytes_written_again_are_the_same_tree() {
    let (_held, root) = tree();
    let before = over(std::slice::from_ref(&root));
    let text = std::fs::read_to_string(root.join("src").join("a.rs")).expect("a");
    std::fs::write(root.join("src").join("a.rs"), "temporarily different\n").expect("a change");
    std::fs::write(root.join("src").join("a.rs"), &text).expect("a restore");
    assert_eq!(
        before.id,
        over(std::slice::from_ref(&root)).id,
        "a witness made of timestamps would call a rollback a new tree, which \
         is the mistake this version exists to not repeat"
    );
}

#[test]
fn an_incomplete_witness_authorises_nothing() {
    let unreadable = thalyx_know::Witness {
        id: "k1-whatever".to_string(),
        files: 3,
        unreadable: 1,
        bytes: 0,
    };
    assert!(!unreadable.is_complete());
    assert!(
        !unreadable.matches("k1-whatever"),
        "a witness of a tree part of which nobody read must not match even \
         itself: a failure to read is not a failure to exist"
    );

    let store = Knowledge::in_memory().expect("a store");
    store
        .remember("kind", "key", &unreadable, "a test", "the answer")
        .expect("a write");
    assert!(
        store.keys("kind").expect("keys").is_empty(),
        "an entry that can never be current is a permanent miss wearing the \
         costume of a cache"
    );
}

#[test]
fn a_file_nested_deeper_is_still_covered() {
    let (_held, root) = tree();
    // Named as what it asserts. It was written as a test about unreadable
    // paths and it is not one: this suite runs as root, root can read
    // anything, and the test would have been measuring the uid rather than the
    // witness — the same mistake as `chrt --other` measuring util-linux.
    let nested = root.join("src").join("deeper");
    std::fs::create_dir(&nested).expect("a directory");
    std::fs::write(nested.join("c.rs"), "fn c() {}\n").expect("c");
    let found = over(std::slice::from_ref(&root));
    assert_eq!(found.files, 3, "two at the top and one nested");
    assert!(found.is_complete());
}

#[test]
fn woven_identities_differ_when_any_part_does() {
    let one = thalyx_know::witness::of_text("rustc 1.90");
    let two = thalyx_know::witness::of_text("rustc 1.94");
    let base = thalyx_know::witness::of_text("the same sources");
    assert_ne!(
        thalyx_know::woven(&[&base, &one]).id,
        thalyx_know::woven(&[&base, &two]).id
    );
    assert_eq!(
        thalyx_know::woven(&[&base, &one]).id,
        thalyx_know::woven(&[&base, &one]).id
    );
}

#[test]
fn what_is_held_can_be_counted_without_printing_any_of_it() {
    let store = Knowledge::in_memory().expect("a store");
    let identity = thalyx_know::witness::of_text("anything");
    for name in ["one", "two", "three"] {
        store
            .remember("symbol", name, &identity, "a test", "value")
            .expect("a write");
    }
    store
        .remember("validation", "a check", &identity, "a test", "value")
        .expect("a write");
    assert_eq!(
        store.counts().expect("counts"),
        vec![("symbol".to_string(), 3), ("validation".to_string(), 1)]
    );
    assert_eq!(store.forget_kind("symbol").expect("a forget"), 3);
    assert_eq!(
        store.counts().expect("counts"),
        vec![("validation".to_string(), 1)]
    );
}

#[test]
fn a_name_that_is_not_there_is_absence_and_not_a_failure_to_read() {
    // The defect this test exists for. `Cargo.lock` is a legitimate thing to
    // name among a check's inputs, and a workspace that has not got one yet is
    // a workspace with no lockfile. Counting the name as *unreadable* made
    // every witness incomplete, and an incomplete witness matches nothing — so
    // the validation cache never hit once, silently, and the compiler ran every
    // time. Rule 10: a failure to read is not a failure to exist, and this is
    // the same sentence read in the other direction.
    let (_held, root) = tree();
    let with_a_ghost = witness(&Over {
        roots: &[root.join("src"), root.join("Cargo.lock")],
        suffixes: &[".rs"],
        skip: &[],
    });
    assert!(
        with_a_ghost.is_complete(),
        "naming a file that does not exist made the witness authorise nothing"
    );
    assert_eq!(with_a_ghost.files, 2);

    // And when it appears, it is a different set of contents.
    std::fs::write(root.join("Cargo.lock"), "version = 4\n").expect("a lockfile");
    let with_it = witness(&Over {
        roots: &[root.join("src"), root.join("Cargo.lock")],
        suffixes: &[".rs", "Cargo.lock"],
        skip: &[],
    });
    assert_ne!(with_a_ghost.id, with_it.id);
}
