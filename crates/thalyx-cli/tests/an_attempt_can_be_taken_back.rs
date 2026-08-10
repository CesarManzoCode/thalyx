//! `intento`, driven from a real session.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **D2**, and the
//! sentence [[Filosofia-Fundacional]] uses for the advantage no other operating
//! system has: *«intenta esto y si sale mal deshazlo»*.
//!
//! ## What these prove and what they cannot
//!
//! The reasoning — which attempt is open, what a second one does, what an
//! abandon aims at, what happens when the snapshot is gone — is covered in
//! `thalyx_core::attempt` against the directory fake, which is the crate's own
//! split: *policy that can only be exercised on a Btrfs filesystem is policy
//! that is never exercised*.
//!
//! What is here is the half that has to be driven through a session: that the
//! verb is reachable, that both faces answer, and that on a filesystem with no
//! subvolumes it **refuses**. That last one is not a lesser test. A copy of a
//! directory is not a snapshot — not atomic, and proportional to the data — so
//! an implementation that quietly fell back to copying would pass every check
//! that only asked whether `intento empezar` reported success, and would hand a
//! caller a way back that is not there. This container has no Btrfs at all,
//! which makes it exactly the machine that can prove the refusal.
//!
//! On Cesar's machine, `dev/verify.sh` stage 26 runs the other half.

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

fn asked(root: &Path, at: &Path, line: &str) -> serde_json::Value {
    let output = piped(
        root,
        &[
            "structured on",
            &format!("cd {}", at.display()),
            line,
            "salir",
        ],
    );
    answer_to(&objects(&output), "attempt")
}

fn a_working_tree() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("a store");
    let work = root.path().join("work");
    std::fs::create_dir(&work).expect("the tree");
    std::fs::write(work.join("uno.txt"), "one").expect("a file");
    (root, work)
}

#[test]
fn a_machine_with_no_attempt_open_says_so_without_being_asked_twice() {
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, "intento");

    assert_eq!(answer["ok"], serde_json::json!(true));
    // `false` and not an absent key. A caller that had to infer "no attempt"
    // from a missing field is one that never wrote the branch.
    assert_eq!(answer["open"], serde_json::json!(false));
}

#[test]
fn without_a_subvolume_it_refuses_instead_of_copying_a_directory() {
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, "intento empezar refactor");

    // The claim: a copy is not a snapshot. It is not atomic, it takes time
    // proportional to the data, and something that took twenty minutes is a
    // picture of twenty minutes rather than of an instant. An implementation
    // that fell back to one would hand a caller a way back that is not there —
    // and the caller would find out only when it needed it.
    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("not_a_subvolume"));
    assert!(
        answer["message"]
            .as_str()
            .unwrap()
            .contains("no attempt was started"),
        "the refusal does not say nothing was started: {answer}"
    );
}

#[test]
fn a_refusal_to_start_leaves_nothing_open_behind_it() {
    let (root, work) = a_working_tree();
    let output = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "intento empezar refactor",
            "intento",
            "salir",
        ],
    );
    let all = objects(&output);
    let status = all
        .iter()
        .rfind(|value| value["op"] == serde_json::json!("attempt"))
        .expect("a status");

    // The half that would be worse than the refusal itself: an attempt written
    // down for a snapshot that was never taken is one that can never be
    // abandoned and blocks every following one.
    assert_eq!(status["open"], serde_json::json!(false), "{status}");
}

#[test]
fn settling_something_that_was_never_started_says_which_rather_than_succeeding() {
    let (root, work) = a_working_tree();

    for line in ["intento confirmar", "intento abandonar"] {
        let answer = asked(root.path(), &work, line);
        assert_eq!(answer["ok"], serde_json::json!(false), "{line}: {answer}");
        assert_eq!(
            answer["error"],
            serde_json::json!("none_open"),
            "{line}: {answer}"
        );
    }
}

#[test]
fn a_word_that_is_not_one_of_the_three_is_named_rather_than_guessed_at() {
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, "intento borrar-todo");

    // Guessing which of three consequential words was meant is not a service
    // anybody wants from the verb that can replace a whole subvolume.
    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("unknown_argument"));
}

#[test]
fn a_person_is_told_how_to_start_one_rather_than_only_that_none_is_open() {
    let (root, work) = a_working_tree();
    let output = piped(
        root.path(),
        &[&format!("cd {}", work.display()), "intento", "salir"],
    );
    let said = String::from_utf8_lossy(&output.stdout);

    assert!(
        said.contains("No attempt is open"),
        "the person was not told the state:\n{said}"
    );
    // The half `Principio-Doble-Ruta` is about: on the image there is no second
    // terminal and no manual, so a verb whose way in is not on screen is a verb
    // the person does not have.
    assert!(
        said.contains("intento empezar"),
        "the person was not told how to start one:\n{said}"
    );
}

#[test]
fn rehearsing_it_sends_the_caller_to_the_verb_that_already_answers_that() {
    let (root, work) = a_working_tree();
    let output = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "ensayo intento",
            "salir",
        ],
    );
    let answer = answer_to(&objects(&output), "rehearse");

    // A2 applied to a rehearsal: `intento` alone already says what abandoning
    // would cost, so refusing without naming that would send a caller looking
    // for something it already has.
    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("ask_attempt_itself"));
}
