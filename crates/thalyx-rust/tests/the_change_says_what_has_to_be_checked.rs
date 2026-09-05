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

// ── the state a check is an answer about ─────────────────────────────────────
//
// The claims below are all one defect, found on 2026-09-05 by the vertical
// stage of `verify.sh`: a second request over bytes the machine had *just*
// compiled compiled them again. The identity was taken once, before Cargo ran,
// and the verdict was filed under it — and Cargo, on a workspace with no
// lockfile, writes one while it works.

/// A `cargo check` of this tree, offline, building outside it.
///
/// Outside because a `target/` inside the tree would be inside the witness's
/// walk, and then this file would be measuring its own build directory.
fn cargo_check(root: &Path, build_into: &Path) -> bool {
    std::process::Command::new(thalyx_rust::metadata::cargo())
        .arg("check")
        .arg("--offline")
        .arg("--target-dir")
        .arg(build_into)
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn the_input_that_moves_under_a_first_check_is_the_lockfile_cargo_writes() {
    if !support::cargo_or_skip("which input of a check's identity Cargo moves under it") {
        return;
    }
    let (held, root, workspace) = chain();
    // The fixture ships a lockfile, which is why nothing here ever caught this:
    // the tree `verify.sh` builds for the vertical stage has none, and neither
    // does any workspace anybody has just written.
    std::fs::remove_file(root.join("Cargo.lock")).expect("the fixture's lockfile");

    let before = identity_of(&workspace, &["chain-middle"]);
    // Taken apart rather than asserted as one number: an identity that differs
    // says *that* something moved and never *what*, and this test exists to
    // name the input. The code and the manifests are one witness, the lockfile
    // is the other, and only the second one is allowed to have moved.
    let code = |root: &Path| {
        thalyx_know::witness(&thalyx_know::Over {
            roots: std::slice::from_ref(&root.to_path_buf()),
            suffixes: &[".rs", "Cargo.toml"],
            skip: thalyx_rust::affected::NOT_SOURCE,
        })
        .id
    };
    let code_before = code(&root);
    assert!(!root.join("Cargo.lock").exists());

    assert!(
        cargo_check(&root, &held.path().join("build")),
        "the fixture has no dependency to fetch, so an offline check of it \
         compiles or the machine cannot compile at all"
    );

    assert!(
        root.join("Cargo.lock").exists(),
        "the whole diagnosis rests on Cargo materialising this file, so it is \
         asserted rather than assumed"
    );
    assert_eq!(
        code_before,
        code(&root),
        "not one byte of code or manifest moved under the compiler; if this \
         ever fails, the cause below is not the cause any more"
    );
    assert_ne!(
        before,
        identity_of(&workspace, &["chain-middle"]),
        "the identity a verdict is filed under changed while the verdict was \
         being produced, and the lockfile is the only thing that moved"
    );
}

#[test]
fn a_check_is_remembered_under_the_state_the_next_question_will_ask_about() {
    if !support::cargo_or_skip("that a check of a lockless tree is reusable at all") {
        return;
    }
    let (held, root, workspace) = chain();
    std::fs::remove_file(root.join("Cargo.lock")).expect("the fixture's lockfile");
    let build_into = held.path().join("build");

    let mut runs = 0;
    let ran = affected::steady(
        || {
            Some(affected::identity(
                &workspace,
                &["chain-middle".to_string()],
                "a fixed toolchain",
            ))
        },
        || {
            runs += 1;
            cargo_check(&root, &build_into)
        },
    );
    assert!(ran.outcome, "the check itself passed");

    let over = ran.over.expect("a state the verdict is an answer about");
    assert_eq!(
        ran.moved, 1,
        "the lockfile appeared under the first run and nothing moved under the second"
    );
    assert_eq!(
        runs, 2,
        "the settling costs one incremental re-check, once per tree"
    );
    assert_eq!(
        over.id,
        identity_of(&workspace, &["chain-middle"]),
        "the verdict must be filed under the tree that is really there, because \
         that is the one the next request asks about"
    );
    assert_ne!(
        over.id,
        ran.from.expect("the state it started from").id,
        "and that is not the state the check started from — filing it there is \
         the defect: every second request over the same bytes missed and ran a \
         compiler the machine did not need"
    );
}

#[test]
fn a_tree_that_will_not_hold_still_is_reported_and_never_remembered() {
    // Rule 8: the fake models the property under test — something writing to
    // the tree while the compiler reads it, which is exactly what Cargo does
    // once and what a second process would do forever. If this ever answers
    // with an identity, a verdict gets filed under bytes no compiler read.
    if !support::cargo_or_skip("what happens when a tree moves under every run") {
        return;
    }
    let (_held, root, workspace) = chain();
    let mut runs = 0;
    let ran = affected::steady(
        || {
            Some(affected::identity(
                &workspace,
                &["chain-middle".to_string()],
                "a fixed toolchain",
            ))
        },
        || {
            runs += 1;
            std::fs::write(
                root.join("middle")
                    .join("src")
                    .join(format!("wrote{runs}.rs")),
                "pub fn nobody_asked() {}\n",
            )
            .expect("the write");
        },
    );
    assert_eq!(
        runs, 2,
        "one retry, because the settling of a real tree is a one-off"
    );
    assert_eq!(ran.moved, 2);
    assert!(
        ran.over.is_none(),
        "a result whose inputs nobody can name is used, reported and never \
         remembered: false miss = slower, false hit = wrong"
    );
    // And the evidence still says which way it moved. A check that reported
    // nothing but "it did not settle" would leave the next person doing from
    // the outside what this pair of witnesses does from the inside.
    let (from, at) = (ran.from.expect("the first"), ran.at.expect("the last"));
    assert_eq!(
        at.files,
        from.files + 2,
        "two files appeared under the two runs, and the witnesses name that"
    );
}
