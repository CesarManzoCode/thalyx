//! When the index answers a question the compiler was supposed to answer, the
//! answer says **why** the compiler did not.
//!
//! ## The failure this file exists to stop
//!
//! 2026-09-05, on a real Thalyx VM with `negar` armed. The first `context` of
//! the boot came back
//!
//! ```text
//! { "source": "index", "resolution": "matched" }
//! ```
//!
//! and the second one, minutes later and in the same boot, came back
//! `rust-analyzer`, `resolution: one`, twelve uses. Both answers were true
//! about who had answered them, and neither said one word about why the first
//! had not reached the compiler — because `gather()` matched
//! `Ok(None) | Err(_)` and dropped the error on the floor.
//!
//! "There is no analyzer on this machine", "the analyzer was still loading the
//! workspace", "cargo could not describe the tree" and "the analyzer died on a
//! syscall" are four different machines and one diagnosis each, and from
//! outside they were the same two words. Rule 10 in
//! `Estrategia-de-Pruebas.md`: a failure to read is not a failure to exist, and
//! the answer has to say which one happened.
//!
//! **The fallback itself is unchanged and is meant to be.** A machine that
//! cannot resolve a name still answers from the index — that is what keeps the
//! programming face alive on a machine with no rust-analyzer. What is asserted
//! here is that it can no longer be quiet about it.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// A separate process, because rule 11: descriptors 0, 1 and 2 belong to the
/// process and `cargo test` runs a binary's tests as threads inside one.
fn piped(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session");
    let mut typed = String::new();
    for line in lines {
        typed.push_str(line);
        typed.push('\n');
    }
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(typed.as_bytes())
        .expect("feeding the session");
    child.wait_with_output().expect("waiting for the session")
}

fn answer(output: &Output, op: &str) -> serde_json::Value {
    String::from_utf8_lossy(&output.stdout)
        .replace('\r', "")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .find(|value| value["op"] == serde_json::json!(op))
        .unwrap_or_else(|| {
            panic!(
                "nothing answered `{op}`:\n{}",
                String::from_utf8_lossy(&output.stdout)
            )
        })
}

/// A tree with Rust in it and **no `Cargo.toml`**, which is a semantic provider
/// that cannot start on any machine — with or without rust-analyzer, with or
/// without a kernel that denies. The point is not this particular cause: it is
/// that whatever the cause was, it arrives in the answer.
fn a_tree_no_provider_can_load() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("a store");
    let tree = root.path().join("tree");
    std::fs::create_dir_all(tree.join("src")).expect("the tree");
    std::fs::write(
        tree.join("src").join("keystore.rs"),
        "pub struct Keystore;\n\npub fn open() -> Keystore {\n    Keystore\n}\n",
    )
    .expect("a file");
    (root, tree)
}

#[test]
fn a_context_that_fell_back_to_the_index_carries_the_providers_own_words() {
    let (root, tree) = a_tree_no_provider_can_load();
    let said = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", tree.display()),
            "indexar",
            "contexto Keystore",
            "salir",
        ],
    );
    let answer = answer(&said, "context");

    // ── the baseline: the fallback still answers ────────────────────────────
    //
    // Without this, an answer that had stopped being produced at all and an
    // answer that fell back loudly would be the same test result.
    assert_eq!(answer["ok"], serde_json::json!(true), "{answer}");
    assert_eq!(answer["source"], serde_json::json!("index"), "{answer}");
    assert_eq!(
        answer["resolution"],
        serde_json::json!("matched"),
        "{answer}"
    );
    assert_eq!(
        answer["entries"][0]["name"],
        serde_json::json!("Keystore"),
        "the fallback stopped answering, which is a different change than the \
         one under test: {answer}"
    );

    // ── the claim ───────────────────────────────────────────────────────────
    let why = answer["analyzer_error"].as_str().unwrap_or_else(|| {
        panic!(
            "the answer came from the index and does not say why the semantic \
             provider did not answer; this is the field that exists so a cold \
             first query can be diagnosed at all: {answer}"
        )
    });
    assert!(
        why.contains("cargo") || why.contains("rust-analyzer"),
        "the reason has to name what stopped the provider, and `{why}` names \
         nothing that could be acted on"
    );
    // Carried in `detail` as well, because that is the field a person reads
    // and the one every existing harness already prints. A diagnosis that
    // only lives under a key nobody dumps is a diagnosis nobody reads.
    let detail = answer["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains(why),
        "`detail` has to carry the same sentence, verbatim: detail={detail:?}, \
         analyzer_error={why:?}"
    );
}
