//! A rename answers with what rust-analyzer already told it, and no more.
//!
//! `vault/03-Primitivas/Semantica-Compilada.md`. The verb has always answered
//! *which* files it changed. It has never answered **how much** of each one it
//! changed — although it knew: rust-analyzer replies to a rename with a
//! `WorkspaceEdit`, every file and every range inside it, and applying that
//! collapsed the ranges into one new string per file before anybody looked.
//!
//! So a caller that wanted to know a rename had touched one file in three
//! places and another in one had exactly two ways to find out, and both were
//! worse than the answer it had already been given: ask again, or search the
//! tree textually for the name — which is the thing this entire path exists to
//! be better than, and which is wrong wherever the new name was already present
//! for some other reason.
//!
//! ## The fixture, and why it is shaped like that
//!
//! `counted` declares `Flywheel` and uses it a different number of times in
//! each of three files: three, two, one. A tree where every file held one
//! occurrence would let "one per file", or the file count under another name,
//! pass as a per-file edit count. The name is invented for this file and is
//! deliberately nothing the benchmark uses — a regression written over the
//! bank's own symbol is a regression that passes because the bank passes.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

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

fn counted() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("thalyx-rust")
        .join("tests")
        .join("trees")
        .join("counted");
    let held = tempfile::tempdir().expect("a temporary directory");
    let tree = held.path().join("counted");
    copy(&source, &tree);
    let store = held.path().join("store");
    std::fs::create_dir_all(&store).expect("a store");
    (held, tree, store)
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the destination");
    for entry in std::fs::read_dir(from).expect("the fixture") {
        let entry = entry.expect("an entry");
        // Whatever a check left behind is not the fixture.
        if entry.file_name() == "target" {
            continue;
        }
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

#[test]
fn a_rename_says_how_many_places_in_each_file_it_rewrote() {
    if !analyzer_or_skip("that a rename reports its edits per file") {
        return;
    }
    let (_held, tree, store) = counted();

    let output = piped(
        &store,
        &[
            "structured on".to_string(),
            format!("cd {}", tree.display()),
            "renombrar-simbolo Flywheel Rotor".to_string(),
            "salir".to_string(),
        ],
    );
    let said = objects(&output);
    let answer = answer_to(&said, "rename");
    assert_eq!(answer["ok"], serde_json::json!(true), "{answer:#}");
    assert_eq!(answer["source"], serde_json::json!("rust-analyzer"));

    let by_file = answer["edits_by_file"]
        .as_array()
        .unwrap_or_else(|| panic!("no `edits_by_file`: {answer:#}"));

    // Sorted here and not asserted in the machine's order: the order the machine
    // answers in is rust-analyzer's, and pinning it here would make this a test
    // about the language server's iteration rather than about the counts.
    let mut seen: Vec<(String, u64)> = by_file
        .iter()
        .map(|entry| {
            (
                entry["path"].as_str().expect("a path").to_string(),
                entry["edits"].as_u64().expect("a count"),
            )
        })
        .collect();
    seen.sort();

    assert_eq!(
        seen,
        vec![
            ("src/hub.rs".to_string(), 3),
            ("src/rim.rs".to_string(), 1),
            ("src/spoke.rs".to_string(), 2),
        ],
        "{answer:#}"
    );

    // The counts are per file and are not the file count wearing a hat.
    assert_eq!(answer["files_changed"], serde_json::json!(3), "{answer:#}");
    assert_eq!(answer["edits"], serde_json::json!(6), "{answer:#}");

    // And every file named in `files` is accounted for in `edits_by_file`, so
    // the two can never come to disagree about what was touched.
    let named: Vec<&str> = answer["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    let counted: Vec<&str> = by_file
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .collect();
    assert_eq!(named, counted, "{answer:#}");

    // The bytes, because the answer is a claim about them. Rule 1: a count that
    // was right about a rename that did not happen is not a count.
    let hub = std::fs::read_to_string(tree.join("src/hub.rs")).expect("hub.rs");
    assert!(hub.contains("pub struct Rotor;"), "{hub}");
    assert!(!hub.contains("Flywheel"), "{hub}");
    let rim = std::fs::read_to_string(tree.join("src/rim.rs")).expect("rim.rs");
    assert!(rim.contains("use crate::hub::Rotor as Wheel;"), "{rim}");
}

#[test]
fn a_rename_given_a_name_says_where_the_definition_is_and_one_given_a_place_does_not() {
    if !analyzer_or_skip("that a rename only claims a definition it really resolved") {
        return;
    }

    // Given the name. The verb reached the place *through* the symbol's own
    // declaration, so it knows, and says.
    let (_held, tree, store) = counted();
    let said = objects(&piped(
        &store,
        &[
            "structured on".to_string(),
            format!("cd {}", tree.display()),
            "renombrar-simbolo Flywheel Rotor".to_string(),
            "salir".to_string(),
        ],
    ));
    let answer = answer_to(&said, "rename");
    assert_eq!(
        answer["definition"],
        serde_json::json!("src/hub.rs:1:12"),
        "{answer:#}"
    );
    assert_eq!(answer["resolved_at"], answer["definition"], "{answer:#}");

    // Given a place — and a place that is a plain *use* of the name, not its
    // declaration and not an import. rust-analyzer resolves it to the same
    // symbol and rewrites all three files exactly as it did above, so the work
    // is identical and only the knowledge is not: nothing on this path went
    // through the declaration, so `definition` is absent rather than a guess.
    // A caller can tell "it is here" from "nobody asked", which is rule 10
    // applied to one field.
    //
    // An import site would be a different operation and is not the case under
    // test: pointing at `use crate::hub::Flywheel;` makes rust-analyzer add a
    // local alias to that one file instead of renaming the symbol, which is
    // correct and is its own reason not to claim a definition for a place
    // somebody pointed at.
    let (_held2, tree2, store2) = counted();
    let said = objects(&piped(
        &store2,
        &[
            "structured on".to_string(),
            format!("cd {}", tree2.display()),
            // `pub fn hold() -> Flywheel {` — the name starts at column 18.
            "renombrar-simbolo src/spoke.rs:3:18 Rotor".to_string(),
            "salir".to_string(),
        ],
    ));
    let answer = answer_to(&said, "rename");
    assert_eq!(answer["ok"], serde_json::json!(true), "{answer:#}");
    assert!(
        answer.get("definition").is_none(),
        "the verb claimed a definition it never resolved: {answer:#}"
    );
    // The control: it really did the same work, so the missing field is about
    // what was *known* and not about a rename that did less.
    assert_eq!(answer["files_changed"], serde_json::json!(3), "{answer:#}");
    assert_eq!(answer["edits"], serde_json::json!(6), "{answer:#}");
    assert!(
        answer["edits_by_file"]
            .as_array()
            .is_some_and(|by| by.len() == 3),
        "{answer:#}"
    );
}

#[test]
fn a_rename_that_refused_says_nothing_changed_and_nothing_did() {
    // The control beside both tests above: a per-file count that appeared on a
    // refusal, or a definition claimed for a name that resolved to nothing,
    // would be the verb answering about work it did not do.
    let (_held, tree, store) = counted();
    let before = std::fs::read_to_string(tree.join("src/hub.rs")).expect("hub.rs");

    let said = objects(&piped(
        &store,
        &[
            "structured on".to_string(),
            format!("cd {}", tree.display()),
            "renombrar-simbolo NothingDeclaresThis Rotor".to_string(),
            "salir".to_string(),
        ],
    ));
    let answer = answer_to(&said, "rename");
    assert_eq!(answer["ok"], serde_json::json!(false), "{answer:#}");
    assert!(answer.get("definition").is_none(), "{answer:#}");
    assert_eq!(
        std::fs::read_to_string(tree.join("src/hub.rs")).expect("hub.rs"),
        before,
        "a refusal wrote to the tree"
    );
}
