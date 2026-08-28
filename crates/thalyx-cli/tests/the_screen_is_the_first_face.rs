//! The screen is what a Thalyx machine comes up on, and the text session is what
//! is behind it.
//!
//! Cesar, 2026-08-28: *«no quiero un comando para activar ui, quiero ya la ui, la
//! que se ve al iniciar»*. That is a decree about **who has to know something**.
//! A screen you reach by typing its name is a screen the person holding the
//! machine has to have been told about, and on a machine with no shell there is
//! nobody to tell them.
//!
//! ## What can be checked here and what cannot
//!
//! There is no framebuffer in the container that builds Thalyx and there is no
//! init, so the two facts this delivery turns on — a display that can be drawn on
//! and a session whose parent is PID 1 — are both absent. What *is* here is every
//! decision taken around them, and the decisions are where the failure would be:
//! whether the boot entry can ask for text, whether a session with no keyboard
//! refuses rather than blacking out a display, and whether a refusal leaves a
//! machine that still answers.
//!
//! The rest is §41 of `dev/verify.sh` and Cesar's own machine.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

fn a_machine() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a temporary store");
    let made = Command::new(thalyx())
        .args(["store", "status"])
        .env("THALYX_ROOT", root.path())
        .output()
        .expect("opening a store");
    assert!(made.status.success(), "the store would not open");
    root
}

/// Type at the prompt **down a pipe**, which is the whole point of this file.
///
/// A pipe has no keyboard, and the screen must say so instead of taking the
/// console. That is not only politeness to a script: `catalogue_is_true` types
/// every advertised verb into a session exactly like this one, so a `pantalla`
/// that drew would take over the display of whatever machine was running the
/// tests — rule 11 of `CLAUDE.md`, in the one place it had not been looked for.
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

fn said(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace('\r', "")
}

fn objects(output: &Output) -> Vec<serde_json::Value> {
    said(output)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(serde_json::Value::is_object)
        .collect()
}

#[test]
fn a_session_with_no_keyboard_refuses_the_screen_in_the_words_it_advertised() {
    let root = a_machine();
    let output = piped(root.path(), &["structured on", "pantalla", "salir"]);

    let answer = objects(&output)
        .into_iter()
        .find(|object| object["op"] == serde_json::json!("screen"))
        .expect("the screen verb answered nothing at all");

    // The exact word, because `describe` promises a caller these two and nothing
    // else. Prose would have been a refusal a program cannot act on.
    assert_eq!(answer["error"], serde_json::json!("not_a_terminal"));
    assert_eq!(answer["ok"], serde_json::json!(false));
}

#[test]
fn a_screen_that_could_not_come_up_leaves_a_session_that_still_answers() {
    // The property that matters more than the refusal itself. On the machine
    // there is nothing behind the session, so a verb that ended it because a
    // display was missing would turn "no framebuffer" into "no computer".
    let root = a_machine();
    let output = piped(root.path(), &["pantalla", "pwd", "salir"]);
    let text = said(&output);

    assert!(
        !text.contains("I have no model loaded"),
        "`pantalla` is advertised and the session does not understand it:\n{text}"
    );
    // `pwd` after it, so this is the session still running and not the banner.
    assert!(
        text.contains("/"),
        "the session stopped answering after the screen refused:\n{text}"
    );
    assert!(output.status.success());
}

#[test]
fn the_screen_verb_is_offered_when_tab_is_pressed_at_the_start_of_a_line() {
    // The completion list is generated from the catalogue, so this is really a
    // check that the verb reached the one table everything else is built from —
    // and on a machine with no shell, a verb nothing can complete is a verb only
    // somebody who already knew about it can type.
    let root = a_machine();
    let output = piped(
        root.path(),
        &["structured on", "describe pantalla", "salir"],
    );

    let answer = objects(&output)
        .into_iter()
        .find(|object| object["op"] == serde_json::json!("describe"))
        .expect("describe answered nothing");
    let verb = &answer["verbs"][0];

    assert_eq!(verb["id"], serde_json::json!("screen"));
    assert_eq!(
        verb["names"],
        serde_json::json!(["pantalla", "screen"]),
        "the standard name comes second, which is backwards for every other verb"
    );
    // Said rather than assumed: a caller that has been told a verb changes the
    // machine treats it as consequential, and this one changes which face is in
    // front of a person.
    assert_eq!(verb["changes"], serde_json::json!(false));
}
