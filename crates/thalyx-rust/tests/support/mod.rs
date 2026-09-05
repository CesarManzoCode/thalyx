//! What the tests in this crate need from the machine, and how they say so
//! when the machine has not got it.
//!
//! `Estrategia-de-Pruebas` rule 3: a test that skips says it skipped, prints
//! `NOT PROVEN`, and there is **one environment variable per requirement** —
//! never one for all of them, because then the only way to demand what a
//! machine has is to demand what it has not.

// Each test binary uses some of these and not others, and a module shared by
// two binaries is compiled once per binary — so whichever half one of them does
// not call is dead code *in that binary*. Silenced here rather than by
// splitting the module in two, which would be two copies of the skip rule.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Copy a fixture tree somewhere writable and answer with the copy.
///
/// Copied rather than used in place because half these tests change files, and
/// a test that edits its own fixture is a test that passes once.
pub fn tree(name: &str) -> (tempfile::TempDir, PathBuf) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("trees")
        .join(name);
    let held = tempfile::tempdir().expect("a temporary directory");
    let root = held.path().join(name);
    copy(&source, &root);
    (held, root)
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the destination");
    for entry in std::fs::read_dir(from).expect("the fixture") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("a copy");
        }
    }
}

/// Whether rust-analyzer is here, saying so when it is not.
///
/// `THALYX_REQUIRE_RUST_ANALYZER=1` turns the skip into a failure, which is how
/// a machine that has one demands that it be used.
pub fn analyzer_or_skip(what: &str) -> bool {
    if thalyx_rust::analyzer::find().is_some() {
        return true;
    }
    let message = format!(
        "NOT PROVEN: {what} — there is no rust-analyzer on this machine. \
         Set THALYX_REQUIRE_RUST_ANALYZER=1 to make this a failure."
    );
    if std::env::var("THALYX_REQUIRE_RUST_ANALYZER").as_deref() == Ok("1") {
        panic!("{message}");
    }
    eprintln!("{message}");
    false
}

/// The same for Cargo, which is a separate requirement and so a separate word.
pub fn cargo_or_skip(what: &str) -> bool {
    let works = std::process::Command::new(thalyx_rust::metadata::cargo())
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if works {
        return true;
    }
    let message = format!(
        "NOT PROVEN: {what} — there is no cargo on this machine. \
         Set THALYX_REQUIRE_CARGO=1 to make this a failure."
    );
    if std::env::var("THALYX_REQUIRE_CARGO").as_deref() == Ok("1") {
        panic!("{message}");
    }
    eprintln!("{message}");
    false
}

/// A copy of `dev/rust-corpus`, which is the workspace the machine's own Rust
/// is verified against.
///
/// The same tree as `dev/verify-agent-rust.sh` and not a second one: the
/// booted machine and the unit tests have to be able to disagree about the
/// answer, which they cannot do if they are asked about different corpora.
/// Copied because a rename really rewrites files.
pub fn corpus() -> (tempfile::TempDir, PathBuf) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/rust-corpus")
        .canonicalize()
        .expect("dev/rust-corpus");
    let held = tempfile::tempdir().expect("a temporary directory");
    let root = held.path().join("rust-corpus");
    copy(&source, &root);
    (held, root)
}

/// Whether a test that deliberately runs an unconfined analyzer may run.
///
/// `THALYX_REQUIRE_CONFINED_ANALYZER=1` is a demand about the machine, and on
/// 2026-08-30 it was typed on `verify.sh`'s command line and stayed in the
/// environment of `cargo test --workspace` — rule 5, where the harness *is*
/// the environment. A test whose whole subject is the environment a provider
/// is handed cannot also be the test that proves confinement, so it says so
/// and stops rather than quietly starting an unconfined server under a
/// variable that forbids one.
pub fn unconfined_or_skip(what: &str) -> bool {
    if std::env::var("THALYX_REQUIRE_CONFINED_ANALYZER").as_deref() != Ok("1") {
        return true;
    }
    eprintln!(
        "NOT PROVEN: {what} — THALYX_REQUIRE_CONFINED_ANALYZER=1 and this check \
         starts an ordinary process on purpose. It measures the environment a \
         toolchain's children are handed, not the confinement around them."
    );
    false
}
