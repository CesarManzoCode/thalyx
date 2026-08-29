//! The four turns a question used to cost when the tree had moved on.
//!
//! In the first real run of an external agent, 2026-08-28, Claude asked the
//! index about a tree it had just edited, got an answer that did not match what
//! it had done, worked out from the `fresh` field that the index was behind,
//! called `thalyx_state`, called `thalyx_index`, and asked its question again.
//!
//! Nothing there was a bug. The index said exactly what it knew and said it was
//! stale, which is the decreed honesty of [[FS-en-Grafo]]. What was wrong is
//! that acting on it was left to the most expensive participant in the loop.
//!
//! These tests pin the new arrangement and, just as importantly, its edges: the
//! honesty rule does not move, a rebuild that would be expensive is declined
//! rather than done quietly, and nothing ever reports `current` on the strength
//! of having tried.

use thalyx_graph::{Freshness, Index, Refreshed};

fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let full = dir.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, contents).unwrap();
    }
    dir
}

#[test]
fn a_question_asked_after_an_edit_answers_about_the_edit() {
    let dir = tree(&[
        ("src/lib.rs", "pub mod store;\n"),
        ("src/store.rs", "pub fn save() {}\n"),
    ]);
    let mut index = Index::in_memory(dir.path()).unwrap();
    index.build().unwrap();

    // The double route: a file appears without Thalyx being told.
    std::fs::write(
        dir.path().join("src/caller.rs"),
        "pub fn go() {\n    crate::store::save();\n}\n",
    )
    .unwrap();

    let outcome = index.refresh_if_stale().unwrap();
    assert!(
        matches!(outcome, Refreshed::Rebuilt { .. }),
        "a stale index of three files must repair itself, and said {}",
        outcome.word()
    );

    let dependents = index.dependents_of("src/store.rs").unwrap();
    assert!(
        dependents.freshness.is_current(),
        "the answer after a repair is about the tree as it is"
    );
    assert!(
        dependents
            .rows
            .iter()
            .any(|edge| edge.from == "src/caller.rs"),
        "the file written a moment ago is a dependent and the answer must say so"
    );
}

#[test]
fn an_index_that_was_already_current_is_not_rebuilt() {
    // The other half of the bargain. A rebuild on every question would make the
    // cheap path cost what the expensive one costs, which is the whole reason
    // there is an index and not a walk.
    let dir = tree(&[("src/lib.rs", "pub fn one() {}\n")]);
    let mut index = Index::in_memory(dir.path()).unwrap();
    index.build().unwrap();

    assert_eq!(index.refresh_if_stale().unwrap(), Refreshed::NotNeeded);
    assert_eq!(index.refresh_if_stale().unwrap(), Refreshed::NotNeeded);
}

#[test]
fn a_tree_too_big_to_rebuild_inside_a_question_is_told_so_and_not_made_to_wait() {
    let dir = tree(&[
        ("src/lib.rs", "pub mod a;\n"),
        ("src/a.rs", "pub fn one() {}\n"),
    ]);
    let mut index = Index::in_memory(dir.path()).unwrap();
    index.build().unwrap();

    std::fs::write(dir.path().join("src/b.rs"), "pub fn two() {}\n").unwrap();

    match index.refresh_if_stale_within(1).unwrap() {
        Refreshed::Declined {
            estimated_files,
            ceiling,
            was,
        } => {
            assert_eq!(ceiling, 1);
            assert!(
                estimated_files > 1,
                "{estimated_files} files were estimated"
            );
            assert_eq!(was.added, vec!["src/b.rs"]);
        }
        other => panic!(
            "a tree over the ceiling must be declined, and said {}",
            other.word()
        ),
    }

    // Declined is not repaired, and the answer still says so. This is the rule
    // that must survive the convenience: an index that reported `current`
    // because a refresh was attempted would be the cache mistaken for the truth
    // that `Coherencia-Doble-Ruta.md` forbids.
    let answer = index.dependents_of("src/a.rs").unwrap();
    assert!(!answer.freshness.is_current());
}

#[test]
fn what_was_declined_says_what_to_call_instead() {
    // A refusal a caller cannot act on is a refusal that costs a turn and buys
    // nothing — which is the turn this whole change exists to delete.
    let dir = tree(&[("src/lib.rs", "pub fn one() {}\n")]);
    let mut index = Index::in_memory(dir.path()).unwrap();
    index.build().unwrap();
    std::fs::write(dir.path().join("src/b.rs"), "pub fn two() {}\n").unwrap();

    let outcome = index.refresh_if_stale_within(1).unwrap();
    assert_eq!(outcome.word(), "declined_too_large");
    assert!(
        outcome.describe().contains("index_build"),
        "the refusal must name the thing to call: {}",
        outcome.describe()
    );
}

#[test]
fn a_repair_reports_what_it_repaired() {
    let dir = tree(&[("src/lib.rs", "pub fn one() {}\n")]);
    let mut index = Index::in_memory(dir.path()).unwrap();
    index.build().unwrap();

    std::fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn one() {}\npub fn two() {}\n",
    )
    .unwrap();

    match index.refresh_if_stale().unwrap() {
        Refreshed::Rebuilt { was, report, .. } => {
            assert_eq!(was.modified, vec!["src/lib.rs"]);
            assert_eq!(report.symbols, 2);
        }
        other => panic!("expected a rebuild, got {}", other.word()),
    }
    assert!(matches!(index.freshness().unwrap(), Freshness::Current));
}
