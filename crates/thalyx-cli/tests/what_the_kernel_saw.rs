//! `cambios`, driven from a real session, on whichever machine this is.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **B3**. The
//! protocol under this — where a record starts, what makes one unfinished, what
//! happens at the wrap — is covered exhaustively in `thalyx_watch::ring` against
//! byte arrays, which is a fake that models the property under test exactly:
//! the kernel's side of that contract *is* the byte layout.
//!
//! What is here is the half a session has to answer. The claim is one sentence
//! and it holds on every machine: **the watcher not being loaded is not the same
//! fact as nothing having changed.** A verb that answered "no changes" on a
//! machine with no watcher would hand every caller the one conclusion this whole
//! feature exists to stop them reaching.
//!
//! ## Why these tests ask the kernel what state it is in
//!
//! They used to assume. The file said *«on a machine with no watcher»*, the
//! tests were named for it, and the assumption was true on every machine anybody
//! had run them on — this container cannot load BPF at all, so the refusal was
//! the only answer that could ever come back and the tests passed for years of
//! commits without the assumption being false once.
//!
//! On 2026-08-23 Cesar ran them on hardware with the watcher **loaded**, exactly
//! as the instructions for that run told him to. `cambios` answered correctly,
//! with real records read from a real kernel ring — and both tests failed,
//! saying Thalyx was wrong.
//!
//! That is rule 5 of `Estrategia-de-Pruebas.md` and it is the second kind of it:
//! **a test that infers its own precondition**. So neither test infers one now.
//! Each asks the filesystem whether the ring is pinned — a question that does
//! not go through the verb under test, because a test that asked `cambios`
//! whether the watcher was loaded and then checked `cambios` against the answer
//! would agree with itself on any machine.
//!
//! And **neither branch is a skip.** Both are real assertions about the same
//! sentence, so this file proves something wherever it runs: without the watcher
//! that the refusal cannot be read as an empty answer, and with it that the
//! answer never claims `not_loaded` and says the three things a ring cannot give.
//!
//! `dev/verify.sh` stage 27 is the other half, on a machine that has BPF.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Whether this machine has the watcher's ring pinned.
///
/// Asked of the filesystem, not of Thalyx. The pin either exists or it does
/// not, and that is a fact about the kernel that the verb under test has no
/// part in — which is the whole point: a test that established its precondition
/// by asking the thing it is testing would agree with itself on any machine,
/// including a broken one.
fn the_watcher_is_loaded() -> bool {
    Path::new(thalyx_watch::ring::DEFAULT_RING).exists()
}

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
fn the_watcher_being_absent_is_never_reported_as_nothing_having_changed() {
    let root = tempfile::tempdir().expect("a store");
    let output = piped(root.path(), &["structured on", "cambios", "salir"]);
    let answer = answer_to(&objects(&output), "changes");

    if the_watcher_is_loaded() {
        // The half only a machine with BPF can check, and it is not the weaker
        // one: these are real records drained from a real kernel ring.
        assert_eq!(
            answer["ok"],
            serde_json::json!(true),
            "the ring is pinned and `cambios` still refused: {answer}"
        );
        assert_ne!(
            answer["error"],
            serde_json::json!("not_loaded"),
            "the ring is pinned and the answer says it is not: {answer}"
        );
        // The three things a ring cannot give, said rather than implied. A
        // caller that treated this as a re-readable history would be wrong
        // quietly, which is the failure that costs an index its correctness.
        assert_eq!(answer["is_a_history"], serde_json::json!(false), "{answer}");
        assert_eq!(
            answer["consumed_by_reading"],
            serde_json::json!(true),
            "{answer}"
        );
        assert_eq!(answer["names_paths"], serde_json::json!(false), "{answer}");
        // And the rows are a list, even when the kernel had nothing to give —
        // which is the case a caller must be able to tell from a refusal.
        assert!(
            answer["mutations"].is_array(),
            "a loaded watcher must answer with rows, empty or not: {answer}"
        );
        return;
    }

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

    if the_watcher_is_loaded() {
        // The refusal must not be printed: on this machine it would be a lie,
        // and it is the exact sentence that would send somebody to load a
        // watcher that is already loaded.
        assert!(
            !said.contains("not the same as nothing having changed"),
            "the watcher is loaded and the person was given the refusal:\n{said}"
        );
        // And they are told the thing a ring cannot give. **Either** sentence,
        // because a loaded watcher has two ordinary outcomes and they say it
        // differently: a queue with records ends with "never which file", and an
        // empty one says it is not a history of the machine. Asserting only the
        // first would fail on a quiet machine — which is a test failing for the
        // machine's mood rather than for anything being wrong, and this file has
        // already cost one of those.
        assert!(
            said.contains("never which file") || said.contains("not a history"),
            "a person was told neither what a record cannot say nor that reading \
             emptied the queue:\n{said}"
        );
        return;
    }

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
