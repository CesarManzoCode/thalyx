//! The ambiguity contract, at the surface a model actually sees.
//!
//! `vault/03-Primitivas/Semantica-Compilada.md`. `thalyx-rust` has its own
//! tests for the resolution itself; this is the other half — that the two verbs
//! which *act* on a resolution behave the way the decree says, and above all
//! that the one which writes files does not write any.
//!
//! The tree is three crates declaring `Config`, plus one name declared once.
//! The second is not decoration: without it, a machine that answered
//! "ambiguous" to everything would pass every assertion below while having
//! broken the whole programming face.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// Type lines into a session down a plain pipe and keep the objects.
fn piped(store: &Path, lines: &[String]) -> Output {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", store)
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

fn objects(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .replace('\r', "")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(|value| value.is_object())
        .collect()
}

fn answer_to<'a>(objects: &'a [serde_json::Value], op: &str) -> &'a serde_json::Value {
    objects
        .iter()
        .find(|value| value["op"] == serde_json::json!(op))
        .unwrap_or_else(|| panic!("nothing answered `{op}`; got {objects:#?}"))
}

/// The fixture, copied somewhere writable, plus a store of its own.
fn three_configs() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("thalyx-rust")
        .join("tests")
        .join("trees")
        .join("three-configs");
    let held = tempfile::tempdir().expect("a temporary directory");
    let tree = held.path().join("three-configs");
    copy(&source, &tree);
    let store = held.path().join("store");
    std::fs::create_dir_all(&store).expect("a store");
    (held, tree, store)
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

/// Rule 3: one variable per requirement, and a skip that says it skipped.
fn analyzer_or_skip(what: &str) -> bool {
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

/// Every source file of the tree and its bytes, for the byte-for-byte column.
fn everything(tree: &Path) -> Vec<(String, String)> {
    let mut all = Vec::new();
    for package in ["alpha", "beta", "gamma"] {
        let path = tree.join(package).join("src").join("lib.rs");
        all.push((
            format!("{package}/src/lib.rs"),
            std::fs::read_to_string(&path).expect("a source file"),
        ));
    }
    all
}

#[test]
fn contexto_says_a_name_means_three_things_and_hands_over_three_handles() {
    if !analyzer_or_skip("that an ambiguous name is answered as ambiguous at the surface") {
        return;
    }
    let (_held, tree, store) = three_configs();

    let output = piped(
        &store,
        &[
            "structured on".to_string(),
            format!("cd {}", tree.display()),
            "contexto Config".to_string(),
            "salir".to_string(),
        ],
    );
    let said = objects(&output);
    let answer = answer_to(&said, "context");

    assert_eq!(
        answer["resolution"],
        serde_json::json!("ambiguous"),
        "{answer:#}"
    );
    assert_eq!(answer["source"], serde_json::json!("rust-analyzer"));
    let entries = answer["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 3, "{answer:#}");

    // Each entry says which crate it is in and carries the identity that
    // resolves it. That is what makes the refusal actionable rather than
    // merely correct: the next call is one the caller can write.
    let mut crates: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry["crate"].as_str())
        .collect();
    crates.sort_unstable();
    assert_eq!(crates, ["alpha", "beta", "gamma"], "{answer:#}");
    for entry in entries {
        let at = entry["at"].as_str().expect("an `at`");
        assert!(at.contains("src/lib.rs:"), "{at}");
    }
}

#[test]
fn contexto_still_resolves_a_name_that_means_one_thing() {
    // **The control.** A machine that called everything ambiguous would pass
    // the test above, and this is the only thing that notices.
    if !analyzer_or_skip("that a unique name still resolves at the surface") {
        return;
    }
    let (_held, tree, store) = three_configs();

    let output = piped(
        &store,
        &[
            "structured on".to_string(),
            format!("cd {}", tree.display()),
            "contexto Unmistakable".to_string(),
            "salir".to_string(),
        ],
    );
    let answer = objects(&output);
    let answer = answer_to(&answer, "context");

    assert_eq!(answer["resolution"], serde_json::json!("one"), "{answer:#}");
    let entries = answer["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1, "{answer:#}");
    assert_eq!(entries[0]["crate"], serde_json::json!("gamma"));
    assert_eq!(entries[0]["kind"], serde_json::json!("struct"));
}

#[test]
fn a_rename_against_three_candidates_writes_nothing_at_all() {
    // **The assertion this contract exists for.** Before 2026-08-30 this call
    // renamed one of the three — chosen by whatever order rust-analyzer listed
    // its workspace symbols in — across every file that used it, and answered
    // `source: rust-analyzer`, which is true.
    //
    // The bytes are the claim. `files_changed: 0` is what the machine says;
    // the three files being identical is what happened.
    if !analyzer_or_skip("that an ambiguous rename refuses before writing") {
        return;
    }
    let (_held, tree, store) = three_configs();
    let before = everything(&tree);

    let output = piped(
        &store,
        &[
            "structured on".to_string(),
            format!("cd {}", tree.display()),
            "renombrar-simbolo Config Settings".to_string(),
            "salir".to_string(),
        ],
    );
    let said = objects(&output);
    let answer = answer_to(&said, "rename");

    assert_eq!(answer["ok"], serde_json::json!(false), "{answer:#}");
    assert_eq!(
        answer["error"],
        serde_json::json!("ambiguous"),
        "{answer:#}"
    );
    assert_eq!(answer["files_changed"], serde_json::json!(0), "{answer:#}");
    let candidates = answer["candidates"].as_array().expect("candidates");
    assert_eq!(candidates.len(), 3, "{answer:#}");

    for (name, was) in before {
        let now = std::fs::read_to_string(tree.join(&name)).expect(&name);
        assert_eq!(now, was, "{name} was changed by a rename that refused");
        assert!(
            !now.contains("Settings"),
            "{name} carries the new name after a refusal"
        );
    }
}

#[test]
fn a_rename_of_the_name_that_means_one_thing_goes_through() {
    // The control for the refusal, and the one that would catch a
    // `place` that had started refusing everything: the same verb, the same
    // tree, the same call shape, on the name that is unique.
    if !analyzer_or_skip("that an unambiguous rename still happens") {
        return;
    }
    let (_held, tree, store) = three_configs();

    let output = piped(
        &store,
        &[
            "structured on".to_string(),
            format!("cd {}", tree.display()),
            "renombrar-simbolo Unmistakable OnlyOne".to_string(),
            "salir".to_string(),
        ],
    );
    let said = objects(&output);
    let answer = answer_to(&said, "rename");

    assert_eq!(answer["ok"], serde_json::json!(true), "{answer:#}");
    let gamma = std::fs::read_to_string(tree.join("gamma/src/lib.rs")).expect("gamma");
    assert!(gamma.contains("pub struct OnlyOne"), "{gamma}");
    assert!(
        !gamma.contains("pub struct Unmistakable"),
        "the old declaration is still there: {gamma}"
    );
    // And the three `Config`s are untouched, because they were never the
    // question.
    assert!(gamma.contains("pub struct Config"), "{gamma}");
}
