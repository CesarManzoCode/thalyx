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
