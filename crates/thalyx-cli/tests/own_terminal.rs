//! Thalyx makes its own terminal.
//!
//! `TerminalConfirmer` refuses to confirm when stdin is not a terminal, because
//! silence is not consent. Anything that drives the session prompt therefore
//! has to supply a real one, and until 2026-08-04 that was `script(1)`.
//!
//! The dependency cost more than it looked like it would. Fedora ships `script`
//! in `util-linux-script`, a subpackage that is not installed by default — so on
//! the one machine that can actually verify Thalyx, stage 15 of `verify.sh`
//! skipped itself in its entirety and four of the six exit-criterion steps went
//! unchecked. The criterion that ends Phase 1 was not being tested because of a
//! package nobody had, and the skip said `NOT PROVEN` exactly as designed while
//! nobody acted on it.
//!
//! Rule 5 of `Estrategia-de-Pruebas.md`: the instrument includes the harness.
//! These tests cover the replacement, because a harness nothing checks is the
//! same hazard one step further back.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// Run `thalyx dev pty -- <argv>`, feeding it `input`.
fn through_pty(input: &str, argv: &[&str]) -> Output {
    let mut command = Command::new(thalyx());
    command.args(["dev", "pty", "--"]);
    command.args(argv);

    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("thalyx dev pty");

    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("feeding the child");

    child.wait_with_output().expect("waiting")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .replace('\r', "")
        .trim()
        .to_string()
}

#[test]
fn a_command_run_through_it_gets_a_terminal_and_one_run_without_it_does_not() {
    // Both halves in one test, because either alone would pass while the
    // property was absent. Without the first, the pty makes no terminal and
    // every prompt-driving check refuses for the harness's reason. Without the
    // second — the control — a test process that happened to already have a tty
    // on stdin would report success no matter what the pty did.
    let ask = "test -t 0 && echo TTY || echo NOT-A-TTY";

    let inside = through_pty("", &["sh", "-c", ask]);
    assert_eq!(
        stdout(&inside),
        "TTY",
        "thalyx dev pty did not supply a terminal: {}",
        String::from_utf8_lossy(&inside.stderr)
    );

    let outside = Command::new("sh")
        .args(["-c", ask])
        .stdin(Stdio::null())
        .output()
        .expect("the control");
    assert_eq!(
        stdout(&outside),
        "NOT-A-TTY",
        "the control already had a terminal, so the check above proves nothing"
    );
}

#[test]
fn what_is_written_to_it_arrives_on_the_child_s_stdin() {
    // A terminal the child cannot be spoken through would satisfy the test
    // above and drive no prompt at all.
    let output = through_pty("hola\n", &["sh", "-c", "read line; echo \"got:$line\""]);
    assert!(
        stdout(&output).contains("got:hola"),
        "the child never received what was written to its terminal: {}",
        stdout(&output)
    );
}

#[test]
fn the_child_s_exit_status_is_passed_through() {
    // `verify.sh` reads these. A pty that swallowed a non-zero status would
    // make a refusing session look like a succeeding one.
    let output = through_pty("", &["sh", "-c", "exit 7"]);
    assert_eq!(output.status.code(), Some(7));

    let clean = through_pty("", &["sh", "-c", "exit 0"]);
    assert_eq!(clean.status.code(), Some(0), "a clean exit must stay clean");
}

#[test]
fn a_child_that_writes_before_it_reads_does_not_deadlock() {
    // The shape of a prompt: it speaks first and waits second. A driver that
    // wrote all its input before reading any output would hang here, and the
    // symptom would be `verify.sh` stopping rather than failing.
    let output = through_pty(
        "answer\n",
        &["sh", "-c", "printf 'question? '; read a; echo \"[$a]\""],
    );

    let seen = stdout(&output);
    assert!(seen.contains("question?"), "did not see the prompt: {seen}");
    assert!(seen.contains("[answer]"), "did not see the answer: {seen}");
}

#[test]
fn the_end_of_a_terminal_is_not_reported_as_a_failure() {
    // Reading a pty after the last writer leaves returns `EIO`, which is the
    // normal end of the conversation and not a fault. Treating it as one would
    // make every successful run look broken — and the check that caught this
    // is simply that a trivial command exits zero with its output intact.
    let output = through_pty("", &["sh", "-c", "echo done"]);
    assert_eq!(stdout(&output), "done");
    assert!(
        output.status.success(),
        "a normal end of terminal was reported as a failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
