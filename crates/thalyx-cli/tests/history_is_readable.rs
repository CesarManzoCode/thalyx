//! The journal, asked from inside a session instead of from a subcommand.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **F2**: *«qué se
//! hizo aquí y por qué» contestado por el sistema y no reconstruido de la
//! conversación*. It has been written since [[Journal-y-Snapshots]] and read by
//! exactly one thing, `thalyx journal`, which is a subcommand and not something
//! a caller living in a session can reach.
//!
//! ## Why the entries here are made by installing something
//!
//! Rule 1: every real defect came from running the system. A test that wrote
//! journal lines by hand would prove that this file can read the shape its
//! author believes the journal has — which is the same mistake as a parser
//! tested only against its author's fixtures, and this project has made that one
//! twice. So the history under test is produced by a real install through the
//! real binary, and what is read back is whatever that actually wrote.

mod harness;

use harness::Fixture;
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

fn history(root: &Path, line: &str) -> serde_json::Value {
    let output = piped(root, &["structured on", line, "salir"]);
    answer_to(&objects(&output), "history")
}

#[test]
fn what_the_machine_did_can_be_asked_from_inside_a_session() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let answer = history(&fixture.root(), "historia");

    assert_eq!(answer["ok"], serde_json::json!(true));
    let entries = answer["entries"].as_array().unwrap();
    assert!(!entries.is_empty(), "the install left no history: {answer}");

    // The operation that was actually performed, named by the system rather
    // than remembered by whoever was watching.
    let installs: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|entry| entry["operation"] == serde_json::json!("install_module"))
        .collect();
    assert!(!installs.is_empty(), "no install in the history: {answer}");
    assert_eq!(installs[0]["module"], serde_json::json!(Fixture::MODULE_ID));
}

#[test]
fn the_answer_says_it_is_not_a_record_of_everything_that_happened() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let answer = history(&fixture.root(), "historia");

    // The caveat the human face has printed in two lines since it was written,
    // as a field. A caller that read this as "everything that happened here"
    // would conclude that nothing else did — and a person with a shell can move
    // a file without anything in here knowing.
    assert_eq!(
        answer["covers"],
        serde_json::json!("operations_thalyx_performed")
    );
    assert_eq!(
        answer["complete_record_of_the_machine"],
        serde_json::json!(false)
    );
}

#[test]
fn an_intent_is_marked_as_not_having_settled_anything() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let answer = history(&fixture.root(), "historia");
    let entries = answer["entries"].as_array().unwrap();

    // `Journal-y-Snapshots` writes the intent *before* the commit, so a crash
    // in between leaves an unresolved intent rather than a silent install. A
    // caller that could not tell an intent from an outcome would report an
    // interrupted operation as done.
    let intents: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|entry| entry["outcome"] == serde_json::json!("intended"))
        .collect();
    assert!(
        !intents.is_empty(),
        "the install wrote no intent, so this test proves nothing: {answer}"
    );
    for intent in intents {
        assert_eq!(intent["settled"], serde_json::json!(false));
    }
    // And the control: the entry that did settle says so.
    let settled: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|entry| entry["outcome"] == serde_json::json!("success"))
        .collect();
    assert!(!settled.is_empty(), "{answer}");
    for entry in settled {
        assert_eq!(entry["settled"], serde_json::json!(true));
    }
}

#[test]
fn the_newest_is_first_so_what_just_happened_is_not_at_the_end() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());
    assert!(fixture.run(&["rollback"]).success());

    let answer = history(&fixture.root(), "historia");
    let entries = answer["entries"].as_array().unwrap();

    // The order an agent asking "what just happened" needs. Oldest first would
    // make the answer to that question the one row it has to page furthest to
    // reach, on the machine where the answer matters most.
    assert_eq!(
        entries[0]["operation"],
        serde_json::json!("rollback"),
        "the most recent operation is not first: {answer}"
    );
}

#[test]
fn the_history_is_paged_with_the_same_words_as_every_other_long_answer() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());
    assert!(fixture.run(&["rollback"]).success());

    let first = history(&fixture.root(), "historia limite=1");
    assert_eq!(first["sent"], serde_json::json!(1));
    assert!(first["total"].as_u64().unwrap() > 1, "{first}");
    assert_eq!(first["more"], serde_json::json!(true));

    let token = first["cursor"].as_str().expect("a cursor").to_string();
    let second = history(
        &fixture.root(),
        &format!("historia limite=1 cursor={token}"),
    );

    // The second page is the next one and not the same one, which is the whole
    // reason a cursor is worth having.
    assert_ne!(second["entries"][0], first["entries"][0], "{second}");
    assert_eq!(second["before"], serde_json::json!(1));
}

#[test]
fn a_machine_nothing_has_been_done_to_says_so_rather_than_failing() {
    let root = tempfile::tempdir().expect("a store");
    let answer = history(root.path(), "historia");

    // Rule 10 from the other side: an empty history is a fact about the
    // machine, and a caller that got a refusal here would think the journal
    // could not be read.
    assert_eq!(answer["ok"], serde_json::json!(true));
    assert_eq!(answer["total"], serde_json::json!(0));
    assert_eq!(answer["entries"], serde_json::json!([]));
}

#[test]
fn a_person_gets_the_same_history_in_sentences_with_the_caveat_in_it() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let output = piped(&fixture.root(), &["historia", "salir"]);
    let said = String::from_utf8_lossy(&output.stdout);

    assert!(
        said.contains("install_module"),
        "the person was not told what was done:\n{said}"
    );
    // The half of `Principio-Doble-Ruta` that is not about the model: the
    // caveat is not a machine-face field that a person does without.
    assert!(
        said.contains("not everything that happened"),
        "the person was not told what this does not cover:\n{said}"
    );
}
