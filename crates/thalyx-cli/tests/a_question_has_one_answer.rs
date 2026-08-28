//! One question, two faces, and the same answer means the same thing on both.
//!
//! `crates/thalyx-cli/src/ask.rs` is why this file exists. The eight places in
//! Thalyx that stop and ask a human used to write the asking out by hand, and
//! two things followed from that which no test could see:
//!
//! 1. **They drifted.** `intento abandonar` took `si` and `sí`; the verb that
//!    takes the kernel guard off the whole machine took neither. Nobody decided
//!    that — it is what five hand-written lines become after being written five
//!    times, and it is exactly what `Principio-Doble-Ruta.md` forbids.
//! 2. **None of them worked on the display.** Under the screen descriptor 0 is
//!    `/dev/null`, so every one of them found no terminal and refused. On the
//!    face the machine boots into, `instalar`, `ejecutar`, `observar` and
//!    `instalar-en` could be read about and not finished.
//!
//! What is tested here is the half a container can answer. The other half —
//! the question drawn on `/dev/fb0` and answered on a real keyboard — is stage
//! 42 of `dev/verify.sh` and needs his hardware, because there is no display
//! here to draw one on.
//!
//! Every claim below has its control beside it, which is rule 4: without one, a
//! refusal and a verb that never ran look the same from outside.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// Type at a session that is on a **real terminal**, made by Thalyx itself.
///
/// `dev pty` and not a pipe, because a pipe is the other half of what this file
/// measures: the confirmer refuses a stdin that is not a terminal, and a test
/// that only ever piped could never tell a working yes from a refused one.
fn on_a_terminal(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .args(["dev", "pty", "--", thalyx(), "--root"])
        .arg(root)
        .arg("session")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("a session on a terminal of Thalyx's own making");

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
        .expect("typing at the prompt");

    child.wait_with_output().expect("waiting for the session")
}

/// The same lines, down a pipe, which is a session with nobody watching it.
fn down_a_pipe(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .args(["--root"])
        .arg(root)
        .arg("session")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("a piped session");

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
    let mut text = String::from_utf8_lossy(&output.stdout).replace('\r', "");
    text.push_str(&String::from_utf8_lossy(&output.stderr).replace('\r', ""));
    text
}

fn a_machine() -> tempfile::TempDir {
    tempfile::tempdir().expect("a store of this test's own")
}

/// What `ejecutar` prints once it is past the confirmation, on a machine with
/// no kernel policy map — which is every machine but the image and his.
///
/// Captured from a real run rather than invented, which is rule 6: a fixture
/// written by hand proves the assertion matches somebody's model of the output.
/// This sentence is the marker that the answer was taken as a **yes**, because
/// it is printed by the code on the far side of the question and by nothing
/// else.
const PAST_THE_QUESTION: &str = "the kernel policy map is not loaded";
/// And what it prints when the answer was taken as a no.
const REFUSED: &str = "  Not run.";

/// The claim the whole seam exists for.
///
/// `sí` is what a person types on a machine whose every sentence is in Spanish.
/// Before `ask.rs` this verb took `y` and `yes` and nothing else, so the answer
/// a Spanish-speaking person gives to a Spanish question was a no.
#[test]
fn the_answer_a_person_gives_in_the_language_of_the_question_is_a_yes() {
    let machine = a_machine();
    for answer in ["sí", "si", "s", "y", "yes", "SÍ", "  sí  "] {
        let session = on_a_terminal(machine.path(), &["ejecutar /bin/echo x", answer, "salir"]);
        let seen = said(&session);
        assert!(
            seen.contains(PAST_THE_QUESTION),
            "{answer:?} was not taken as a yes:\n{seen}"
        );
        assert!(
            !seen.contains(REFUSED),
            "{answer:?} was taken as a yes and as a no at once:\n{seen}"
        );
    }
}

/// The control, and without it the test above proves only that the session did
/// not crash: a confirmer that said yes to everything would pass it.
#[test]
fn anything_that_is_not_an_answer_is_a_no_and_the_program_never_starts() {
    let machine = a_machine();
    for answer in ["n", "no", "quizá", "yy", "", "sim", "ssí"] {
        let session = on_a_terminal(machine.path(), &["ejecutar /bin/echo x", answer, "salir"]);
        let seen = said(&session);
        assert!(
            seen.contains(REFUSED),
            "{answer:?} was not refused:\n{seen}"
        );
        assert!(
            !seen.contains(PAST_THE_QUESTION),
            "{answer:?} got past the question:\n{seen}"
        );
    }
}

/// Silence is not consent, and it still is not.
///
/// The check that used to be written out at each of the eight sites now lives in
/// one place, and moving a rule is the classic way to lose it.
#[test]
fn a_session_nobody_is_watching_cannot_authorise_anything() {
    let machine = a_machine();
    let session = down_a_pipe(machine.path(), &["ejecutar /bin/echo x", "sí", "salir"]);
    let seen = said(&session);

    assert!(
        seen.contains("There is no terminal to confirm on"),
        "a pipe was allowed to confirm, or refused for the wrong reason:\n{seen}"
    );
    assert!(
        !seen.contains(PAST_THE_QUESTION),
        "a pipe that typed `sí` ran the program:\n{seen}"
    );
    // And the `sí` on the next line was not quietly read as something else —
    // it fell through to the prompt, where it is not a verb.
    assert!(
        !seen.contains(REFUSED),
        "the pipe was asked the question at all:\n{seen}"
    );
}

/// The change that makes the display possible, stated as a property.
///
/// The eight sites used to check for a terminal **before** printing what the
/// question is about. Under the screen that ordering is fatal: the context is
/// the confirmation — `Camino-Confiable.md` says a question drawn without what
/// was read from the thing itself is not a confirmation — and it is built from
/// what the verb has printed. A refusal issued before the context exists leaves
/// nothing to draw.
///
/// So the order is now: say what this is, then ask, then refuse if there is
/// nobody to ask. Visible from a pipe, which is the only face a container has.
#[test]
fn a_verb_says_what_it_is_about_before_it_says_there_is_nobody_to_ask() {
    let machine = a_machine();
    let seen = said(&down_a_pipe(
        machine.path(),
        &["ejecutar /bin/echo x", "salir"],
    ));

    let context = seen
        .find("Nobody signed it")
        .unwrap_or_else(|| panic!("the context was never printed at all:\n{seen}"));
    let refusal = seen
        .find("There is no terminal to confirm on")
        .unwrap_or_else(|| panic!("the refusal is gone:\n{seen}"));

    assert!(
        context < refusal,
        "the refusal came before the thing it is refusing, so a display would \
         have nothing to draw:\n{seen}"
    );
}

// ---------------------------------------------------------------------------
// What is deliberately **not** tested here, and why.
// ---------------------------------------------------------------------------
//
// `instalar-en` and `thalyx install` ask with `Accepts::Exactly` — the path of
// the disk, typed out, never a yes. That the comparison refuses a `sí` is
// proven by the unit tests in `ask.rs`. That those two verbs are wired to that
// shape of it is **not** proven here, and it cannot be, because the only way to
// reach their question is to name a disk the verb agrees to erase.
//
// A first version of this file tried anyway. It named a device that does not
// exist and asserted that `sí` did not authorise anything — and it passed,
// because `instalar-en` refuses an unopenable device before it asks. A vacuous
// pass, the class `Estrategia-de-Pruebas.md` catalogues twice, caught only by
// reading the output instead of the exit status.
//
// The fix is not a better disk to name. Pointing a disk-erasing verb at a real
// disk and typing `sí` at it is a test that erases the machine the day the
// thing it is testing is broken — which is the one day it runs differently.
// `THALYX_ROOT` does not isolate a disk; nothing does. Rule 11.
//
// So the claim lives in `dev/verify.sh`, on a loop device the script already
// knows how to make and destroy, where the disk being erased is a file the
// script created.
