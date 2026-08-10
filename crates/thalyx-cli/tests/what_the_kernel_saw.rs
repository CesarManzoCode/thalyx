//! `cambios`, driven from a real session on a machine with no watcher.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **B3**. The
//! protocol under this — where a record starts, what makes one unfinished, what
//! happens at the wrap — is covered exhaustively in `thalyx_watch::ring` against
//! byte arrays, which is a fake that models the property under test exactly:
//! the kernel's side of that contract *is* the byte layout.
//!
//! What is here is the half a session has to answer, and on this machine that
//! is the refusal. It is not a lesser test. The distinction it checks is the one
//! that decides where somebody goes next: **the watcher not being loaded is not
//! the same fact as nothing having changed**, and a verb that answered "no
//! changes" on a machine with no watcher would hand every caller the one
//! conclusion this whole feature exists to stop them reaching.
//!
//! `dev/verify.sh` stage 27 is the other half, on a machine that has BPF.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

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

fn objects(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .replace('\r', "")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(|value| value.is_object())
        .collect()
}

fn answer_to(objects: &[serde_json::Value], op: &str) -> serde_json::Value {
    objects
        .iter()
        .find(|value| value["op"] == serde_json::json!(op))
        .unwrap_or_else(|| panic!("nothing answered `{op}`; got {objects:#?}"))
        .clone()
}

#[test]
fn with_no_watcher_loaded_it_says_so_and_never_says_nothing_changed() {
    let root = tempfile::tempdir().expect("a store");
    let output = piped(root.path(), &["structured on", "cambios", "salir"]);
    let answer = answer_to(&objects(&output), "changes");

    // The whole point of the distinction. "The watcher is not loaded" is a thing
    // to go and fix; "nothing has changed" is a conclusion about the machine.
    // Reporting the second when the first is true is how a caller decides its
    // index is current on a machine it cannot see at all.
    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("not_loaded"));
    assert!(
        answer["message"]
            .as_str()
            .unwrap()
            .contains("not the same as nothing having changed"),
        "the refusal lets itself be read as an empty answer: {answer}"
    );
    // And no rows at all, so a caller cannot read a count of zero off it.
    assert!(answer["mutations"].is_null(), "{answer}");
}

#[test]
fn a_person_asking_gets_the_same_distinction_in_a_sentence() {
    let root = tempfile::tempdir().expect("a store");
    let output = piped(root.path(), &["cambios", "salir"]);
    let said = String::from_utf8_lossy(&output.stdout);

    assert!(
        said.contains("not the same as nothing having changed"),
        "the person was left to read a refusal as an empty answer:\n{said}"
    );
}

#[test]
fn the_verb_is_advertised_with_what_it_cannot_do() {
    // A1 binds the catalogue to reality by running it, and what `describe` says
    // about this verb is the only warning a caller gets before it asks twice and
    // wonders why the second answer is empty.
    let root = tempfile::tempdir().expect("a store");
    let output = piped(root.path(), &["structured on", "describe", "salir"]);
    let answer = answer_to(&objects(&output), "describe");

    let verb = answer["verbs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|verb| verb["id"] == serde_json::json!("changes"))
        .unwrap_or_else(|| panic!("`changes` is not in the catalogue: {answer}"));

    assert!(
        verb["summary"].as_str().unwrap().contains("empties"),
        "the catalogue does not warn that reading consumes: {verb}"
    );
    assert!(
        verb["errors"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("not_loaded")),
        "the caller cannot write the branch it will hit first: {verb}"
    );
}
