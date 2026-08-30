//! What the diagnosis says when the semantic provider dies before it answers.
//!
//! Written on 2026-08-30, from physical evidence on Fedora that could not be
//! read. Stage 58 reported `analyzer_starts=1` and then
//! `rust-analyzer did not answer: initialize: the server stopped`; `ausearch -m
//! SECCOMP` over the exact seconds of the run showed no SECCOMP, no AVC and no
//! other kill. So the machine held one sentence about the death, and that
//! sentence — *«the server stopped»* — is rule 10's failure shape: it says the
//! reading failed and nothing about what happened. A process killed by the
//! filter, a process that could not find its toolchain, and a process that
//! panicked all close a pipe.
//!
//! These tests do not reproduce that death — nothing here can, this container
//! cannot confine anything. They establish the **instrument**: that when a
//! server dies before or during `initialize`, the refusal carries how the
//! process ended and what it wrote on the way out, bounded.
//!
//! The stand-in is a shell that behaves the way the property under test needs —
//! rule 8. It is asked to die by `SIGSYS`, which is not an approximation of a
//! seccomp kill: a process killed by the filter dies of exactly that signal.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use thalyx_rust::analyzer::{Analyzer, Launching, Spawn, Started};

/// A server that is not one: a shell told what to do instead of speaking LSP.
///
/// It reads one line first, so that Thalyx's `initialize` is written to a
/// process that is still alive. Without that the death is a race between
/// `EPIPE` on the write and the channel closing on the read, and a test that
/// raced would prove the diagnosis on whichever side won that day.
struct DiesLike(&'static str);

impl Spawn for DiesLike {
    fn start(&self, _asked: Launching<'_>) -> thalyx_rust::Result<Started> {
        let child = Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("read _ignored; {}", self.0))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("a shell to stand in for a server");
        Ok(Started {
            child,
            release: None,
            how: "a stand-in".to_string(),
            confined: false,
        })
    }
}

/// Start one against a directory that need not be a workspace: nothing here
/// gets as far as reading it.
fn refusal_from(dies_like: &'static str) -> String {
    let root = PathBuf::from(".");
    let error = Analyzer::start(
        &root,
        Path::new("/nonexistent-the-spawner-decides"),
        None,
        &[],
        &[],
        &DiesLike(dies_like),
    )
    .err()
    .expect("a server that dies cannot have started");
    error.to_string()
}

#[test]
fn a_server_killed_by_a_signal_names_the_signal() {
    let said = refusal_from("kill -SYS $$");
    assert!(
        said.contains("killed by signal 31") && said.contains("SIGSYS"),
        "a process killed by the signal the seccomp filter uses must be \
         reported as that, and this said: {said}"
    );
}

#[test]
fn a_server_that_exits_names_its_status() {
    let said = refusal_from("exit 101");
    assert!(
        said.contains("exited with status 101"),
        "a server that exited must be told apart from one that was killed, \
         and this said: {said}"
    );
    assert!(
        !said.contains("killed by signal"),
        "an ordinary exit reported as a signal is the confusion this exists \
         to end: {said}"
    );
}

#[test]
fn what_the_server_wrote_before_dying_is_in_the_refusal() {
    // The sentence that matters is always the last one, which is why the tail
    // is kept rather than the head: a real server logs its way through an
    // indexing pass before it says why it is going.
    let said = refusal_from(
        "echo 'indexing, nothing to see' >&2; \
         echo 'thread panicked: could not find the toolchain' >&2; exit 1",
    );
    assert!(
        said.contains("could not find the toolchain"),
        "the reason a server gave for dying must survive to the refusal, and \
         this said: {said}"
    );
}

#[test]
fn a_server_that_said_nothing_is_reported_as_having_said_nothing() {
    // Rule 10 again, one level down: "it wrote nothing" and "nothing was read"
    // are different facts, and a diagnosis that omitted the sentence entirely
    // would leave the reader unable to tell which happened.
    let said = refusal_from("exit 3");
    assert!(
        said.contains("wrote nothing to stderr"),
        "silence is a finding and must be stated, and this said: {said}"
    );
}

#[test]
fn a_server_that_writes_without_stopping_does_not_fill_this_process() {
    // The half of this that is not diagnosis: the confined path has always had
    // a stderr pipe and nothing ever drained it, so a chatty server would block
    // on a full buffer — a hang that looks like an unresponsive server. Draining
    // it is what makes keeping it safe, and the tail is what keeps the draining
    // bounded. A megabyte, which is far past any buffer this could accumulate in.
    let said = refusal_from("yes 'a log line nobody will ever read' | head -c 1000000 >&2; exit 7");
    assert!(
        said.contains("exited with status 7"),
        "a server that wrote a megabyte and then exited must still be waited \
         on and reported, and this said: {said}"
    );
    assert!(
        said.contains("the last 4096 of 1000000 bytes"),
        "the quoted tail must be bounded and must say how much was dropped, \
         and this said: {}",
        &said[..said.len().min(400)]
    );
}
