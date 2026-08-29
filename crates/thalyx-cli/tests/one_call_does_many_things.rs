//! `hacer`, driven from a real session.
//!
//! `vault/03-Primitivas/Ejecucion-Transaccional.md`. The reasoning — what a
//! refused step does, what a failing check does, what authorises the rollback,
//! how much stays inside the machine — is covered in `crate::exec` against the
//! directory-backed fake, which is this project's standing split: policy that
//! can only be exercised on Btrfs is policy that is never exercised.
//!
//! What is here is the half that has to be driven through a session, because it
//! is about the line and not about the transaction: that the verb is reachable,
//! that a program arrives whole in both the spellings a caller will write it in,
//! and that a program which cannot be read is refused as **that** rather than as
//! something else.
//!
//! ## What this container cannot check, and where it is checked instead
//!
//! Btrfs. Every program here reaches `not_a_subvolume` and stops, which is the
//! correct refusal on a machine with no subvolume anywhere — and it is why
//! `not_a_subvolume` is the *success* condition of the first tests: it can only
//! be reached by the program having been read, so it is what says the line
//! arrived whole. `dev/verify.sh` stage 56 runs the same programs where the
//! boundary is a real snapshot.

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

fn answers(output: &Output, op: &str) -> serde_json::Value {
    let objects: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .replace('\r', "")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .collect();
    objects
        .iter()
        .find(|value| value["op"] == serde_json::json!(op))
        .unwrap_or_else(|| panic!("nothing answered `{op}`; got {objects:#?}"))
        .clone()
}

fn a_working_tree() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("a store");
    let work = root.path().join("work");
    std::fs::create_dir(&work).expect("the tree");
    std::fs::write(work.join("lib.rs"), "pub struct UidRegistry;\n").expect("a file");
    (root, work)
}

fn asked(root: &Path, at: &Path, line: &str, op: &str) -> serde_json::Value {
    let output = piped(
        root,
        &[
            "structured on",
            &format!("cd {}", at.display()),
            line,
            "salir",
        ],
    );
    answers(&output, op)
}

/// The program the tests below send, on one line, as a caller would write it.
const PROGRAM: &str = r#"{"label":"rename","steps":[{"verb":"edit","arguments":["lib.rs","sustituir","UidRegistry","UserRegistry"]}],"validate":[{"check":"text","text":"UidRegistry","expect":"none"}]}"#;

#[test]
fn a_program_typed_bare_arrives_whole() {
    // **The defect this test exists for, found by typing it.** A program is
    // JSON and JSON is made of double quotes, which is exactly what `words.rs`
    // takes off a word — so the first version of this verb answered
    // `unintelligible` to a perfectly good program, because Thalyx had eaten
    // every quote in it before the parser saw it.
    //
    // `not_a_subvolume` is the success condition: it comes from *after* the
    // program is read, so it is the proof that the line arrived whole. On a
    // machine with Btrfs this same line does the work — `dev/verify.sh` 56.
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, &format!("hacer {PROGRAM}"), "exec");

    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(
        answer["error"],
        serde_json::json!("not_a_subvolume"),
        "the program did not survive the line: {answer}"
    );
}

#[test]
fn a_program_in_single_quotes_arrives_whole_too() {
    // The spelling the bridge sends. `external::compose` puts every argument in
    // single quotes, and inside those the double quotes are literal — so this
    // is the shape every external agent's program has, and it must not be the
    // one that works by accident.
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, &format!("hacer '{PROGRAM}'"), "exec");

    assert_eq!(
        answer["error"],
        serde_json::json!("not_a_subvolume"),
        "{answer}"
    );
}

#[test]
fn a_program_that_cannot_be_read_is_refused_as_that_and_not_as_something_else() {
    // Rule 10 at the level of a word: a caller told `not_a_subvolume` about a
    // malformed program would go looking for a filesystem, and a caller told
    // `unintelligible` about a good one would go looking for a typo. Each of
    // these is refused before anything is snapshotted.
    let (root, work) = a_working_tree();
    for wrong in [
        r#"{"steps":}"#,
        r#"{"validate":[]}"#,
        r#"{"steps":[]}"#,
        r#"{"steps":[{"verb":"exec","arguments":[]}]}"#,
        r#"{"steps":[{"verb":"attempt","arguments":["abandonar","si"]}]}"#,
    ] {
        let answer = asked(root.path(), &work, &format!("hacer {wrong}"), "exec");
        assert_eq!(answer["ok"], serde_json::json!(false), "{wrong}: {answer}");
        assert_eq!(
            answer["error"],
            serde_json::json!("unintelligible"),
            "`{wrong}` was not refused as a program this verb cannot read: {answer}"
        );
    }
}

#[test]
fn asking_for_a_program_with_nothing_after_it_says_what_one_looks_like() {
    // Punto A2: an error that only says what went wrong costs a whole cycle of
    // guessing. This one carries the shape.
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, "hacer", "exec");
    assert_eq!(answer["error"], serde_json::json!("nothing_asked"));
    assert!(
        answer["message"]
            .as_str()
            .is_some_and(|message| message.contains("steps")),
        "{answer}"
    );
}

#[test]
fn a_handle_that_is_not_the_shape_of_one_never_becomes_a_path() {
    // A handle arrives from outside, and `../../etc/passwd` is a string like
    // any other. Refused on its shape, before it is joined onto a directory —
    // and refused as *that*, so a caller can tell "I made that up" from "it is
    // not here any more".
    let (root, work) = a_working_tree();
    for wrong in ["../../etc/passwd", "a/b"] {
        let answer = asked(
            root.path(),
            &work,
            &format!("evidencia {wrong}"),
            "evidence",
        );
        assert_eq!(answer["ok"], serde_json::json!(false), "{wrong}");
        assert_eq!(
            answer["error"],
            serde_json::json!("not_a_handle"),
            "`{wrong}` was looked up rather than refused: {answer}"
        );
    }

    let answer = asked(root.path(), &work, "evidencia t-never-happened", "evidence");
    assert_eq!(answer["error"], serde_json::json!("absent"), "{answer}");
}

#[test]
fn asking_for_one_step_of_a_run_is_a_call_the_boundary_lets_through() {
    // The check this was written for. `evidencia <id> paso=N` is the whole of
    // the progressive disclosure — the answer is small and this is how the
    // detail is fetched — and it goes through the external agent's argument
    // check on the way. An earlier version guarded that argument as a window
    // flag, and the boundary refused `paso=` as a word it did not know: the
    // second half of the compression would have been unreachable for exactly
    // the caller it exists for, while working perfectly at a prompt.
    //
    // `absent` is the success condition: it comes from after the argument was
    // read, so it says the call arrived rather than being turned away.
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, "evidencia t-nothing paso=2", "evidence");
    assert_eq!(answer["error"], serde_json::json!("absent"), "{answer}");

    // And a word this verb does not know is refused as that, rather than
    // quietly dropped into "the whole run" — which a caller would read as this
    // run having one step.
    let confused = asked(
        root.path(),
        &work,
        "evidencia t-nothing pasos=2",
        "evidence",
    );
    assert_eq!(
        confused["error"],
        serde_json::json!("unknown_argument"),
        "{confused}"
    );
}

#[test]
fn the_verb_is_in_the_catalogue_the_machine_hands_out() {
    // A verb an agent cannot discover is a verb that does not exist for it, and
    // `describe` is the only place it can look.
    let (root, _work) = a_working_tree();
    let output = piped(root.path(), &["structured on", "describe hacer", "salir"]);
    let answer = answers(&output, "describe");
    assert_eq!(answer["count"], serde_json::json!(1));
    assert_eq!(answer["verbs"][0]["id"], serde_json::json!("exec"));
    // It changes the machine, and a caller reads that field to decide whether
    // to be careful.
    assert_eq!(answer["verbs"][0]["changes"], serde_json::json!(true));
}
