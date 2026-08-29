//! What a change reaches, and what a cached answer about it still covers.
//!
//! The tree is three crates: `chain-middle` uses `chain-base`, and
//! `chain-apart` uses neither. That shape is the whole point — the interesting
//! claims are all about `chain-apart`, the crate a change must **not** reach.
//! A fixture where everything depends on everything would pass every test here
//! with a function that returns the whole workspace.

mod support;

use std::path::Path;
use thalyx_rust::{Workspace, affected};

fn chain() -> (tempfile::TempDir, std::path::PathBuf, Workspace) {
    let (held, root) = support::tree("chain");
    let workspace = Workspace::read(&root).expect("cargo to describe the fixture");
    (held, root, workspace)
}

fn identity_of(workspace: &Workspace, packages: &[&str]) -> String {
    let names: Vec<String> = packages.iter().map(|name| name.to_string()).collect();
    affected::identity(workspace, &names, "a fixed toolchain").id
}

#[test]
fn the_parser_reads_what_cargo_really_prints() {
    // Rule 6, and it is here rather than only against a live Cargo because a
    // fixture somebody invented proves the parser matches its author's idea of
    // the format. This file was captured, verbatim, from
    // `cargo metadata --format-version 1 --no-deps` over `tests/trees/chain`.
    let sample = include_str!("samples/cargo-metadata-chain.json");
    let workspace = Workspace::parse(sample).expect("the captured sample");

    let names: Vec<&str> = workspace
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    assert_eq!(names, vec!["chain-apart", "chain-base", "chain-middle"]);
    assert_eq!(
        workspace
            .package("chain-middle")
            .expect("middle")
            .depends_on,
        vec!["chain-base".to_string()],
        "a path dependency inside the workspace is the edge this whole file is about"
    );
    assert!(
        workspace
            .package("chain-apart")
            .expect("apart")
            .depends_on
            .is_empty(),
        "chain-apart depends on nothing, and a parser that said otherwise would \
         make every test below pass for the wrong reason"
    );
}

#[test]
fn a_change_selects_the_crate_and_everything_above_it() {
    if !support::cargo_or_skip("that a change selects its dependents") {
        return;
    }
    let (_held, root, workspace) = chain();
    let reached = affected(&workspace, &root, &["base/src/lib.rs".to_string()]);

    assert_eq!(reached.changed, vec!["chain-base".to_string()]);
    assert_eq!(
        reached.selected,
        vec!["chain-base".to_string(), "chain-middle".to_string()],
        "compiling only the crate that changed proves nothing about the crate \
         that uses it, which is the entire reason this is not `package_of`"
    );
    assert!(
        !reached.selected.contains(&"chain-apart".to_string()),
        "chain-apart depends on nothing that changed and must not be compiled"
    );
    assert!(reached.unattributed.is_empty());
    assert!(!reached.whole_workspace);
}

#[test]
fn the_lockfile_reaches_every_crate() {
    if !support::cargo_or_skip("that a lockfile change reaches everything") {
        return;
    }
    let (_held, root, workspace) = chain();
    let reached = affected(&workspace, &root, &["Cargo.lock".to_string()]);
    assert!(reached.whole_workspace);
    assert_eq!(reached.selected.len(), 3);
}

#[test]
fn a_file_belonging_to_no_crate_is_named_rather_than_ignored() {
    if !support::cargo_or_skip("that an unattributable change says so") {
        return;
    }
    let (_held, root, workspace) = chain();
    let reached = affected(&workspace, &root, &["notes/README.md".to_string()]);
    assert_eq!(reached.unattributed, vec!["notes/README.md".to_string()]);
    assert!(
        reached.selected.is_empty(),
        "nothing is compiled, and the answer says which file it could not place \
         rather than reporting a clean check of nothing"
    );
}

#[test]
fn a_crate_that_changed_invalidates_what_was_checked_about_it() {
    if !support::cargo_or_skip("that a relevant change invalidates a cached check") {
        return;
    }
    let (_held, root, workspace) = chain();
    let before = identity_of(&workspace, &["chain-base"]);

    let file = root.join("base").join("src").join("lib.rs");
    std::fs::write(&file, "pub fn ground() -> u32 {\n    2\n}\n").expect("the write");

    assert_ne!(
        before,
        identity_of(&workspace, &["chain-base"]),
        "the crate's own source changed and its cached check would still be reused"
    );
}

#[test]
fn a_dependency_that_changed_invalidates_what_was_checked_above_it() {
    if !support::cargo_or_skip("that a dependency change invalidates a dependent's check") {
        return;
    }
    let (_held, root, workspace) = chain();
    let before = identity_of(&workspace, &["chain-middle"]);

    let file = root.join("base").join("src").join("lib.rs");
    std::fs::write(&file, "pub fn ground() -> u32 {\n    2\n}\n").expect("the write");

    assert_ne!(
        before,
        identity_of(&workspace, &["chain-middle"]),
        "chain-middle compiles chain-base, so a check of it cannot survive a \
         change to chain-base. This is the direction that must be the closure \
         and not the dependents"
    );
}

#[test]
fn a_change_somewhere_unrelated_leaves_a_check_standing() {
    if !support::cargo_or_skip("that an unrelated change does not invalidate") {
        return;
    }
    let (_held, root, workspace) = chain();
    let before = identity_of(&workspace, &["chain-apart"]);

    let file = root.join("base").join("src").join("lib.rs");
    std::fs::write(&file, "pub fn ground() -> u32 {\n    2\n}\n").expect("the write");

    assert_eq!(
        before,
        identity_of(&workspace, &["chain-apart"]),
        "chain-apart does not depend on chain-base, so its check is still true. \
         A whole-tree witness would fail here, which is exactly why this \
         identity is scoped"
    );
}

#[test]
fn the_same_bytes_under_a_different_toolchain_are_a_different_answer() {
    if !support::cargo_or_skip("that the toolchain is part of a check's identity") {
        return;
    }
    let (_held, _root, workspace) = chain();
    // Rule 12, as an identity: a build with a different configuration is a
    // different system, and five ioctl casts went through 189 checks proving it.
    assert_ne!(
        affected::identity(&workspace, &["chain-base".to_string()], "rustc 1.90").id,
        affected::identity(&workspace, &["chain-base".to_string()], "rustc 1.94").id
    );
}

#[test]
fn a_tree_restored_byte_for_byte_keeps_what_was_checked_about_it() {
    if !support::cargo_or_skip("that a rollback does not empty the cache") {
        return;
    }
    let (_held, root, workspace) = chain();
    let file = root.join("base").join("src").join("lib.rs");
    let original = std::fs::read_to_string(&file).expect("the file");
    let before = identity_of(&workspace, &["chain-base"]);

    std::fs::write(&file, "pub fn ground() -> u32 {\n    2\n}\n").expect("the write");
    assert_ne!(before, identity_of(&workspace, &["chain-base"]));

    // What `intento abandonar` does: the same bytes back, with every timestamp
    // and inode new. An identity made of mtimes would call this a different
    // tree — which is the mistake of 2026-08-29, and the reason this witness is
    // made of contents alone.
    std::fs::write(&file, &original).expect("the restore");
    assert_eq!(
        before,
        identity_of(&workspace, &["chain-base"]),
        "a rollback emptied the validation cache, so every reverted attempt \
         would pay to compile the tree it started from"
    );
}

#[test]
fn a_file_is_attributed_to_the_innermost_crate_that_contains_it() {
    if !support::cargo_or_skip("that nesting attributes a file to the nearest manifest") {
        return;
    }
    let (_held, root, workspace) = chain();
    let inner = root.join("middle").join("src").join("lib.rs");
    assert_eq!(
        workspace
            .package_of(&inner)
            .map(|package| package.name.as_str()),
        Some("chain-middle")
    );
    assert!(
        workspace.package_of(Path::new("/etc/passwd")).is_none(),
        "a path outside the workspace belongs to no package of it"
    );
}
