//! `negar` and `observar` at the real prompt.
//!
//! The decree is `vault/02-Arquitectura/Programas-Ajenos.md`. `crate::guard`
//! has unit tests over a `MemoryStore` that really flips, and they answer the
//! decision; these run the **session**, because rule 1 says every real defect
//! in this project came from running the system, and because the thing most
//! likely to be wrong here is not the decision but the wiring — a verb that
//! the catalogue advertises and the session does not understand looks fine in
//! every unit test there is.
//!
//! What this container can decide: that both verbs are reachable, that the
//! structured face is turned away from the one that disarms the machine, that
//! a rehearsal of either changes nothing, and that a machine with nothing
//! loaded is told so instead of being told it switched. What it cannot decide
//! is whether the four bytes reach the flag the hooks consult — there is no
//! BPF here — and that is stage 37 of `verify.sh`, on Cesar's machine.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

fn typed(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session");

    let mut script = String::new();
    for line in lines {
        script.push_str(line);
        script.push('\n');
    }
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("feeding the session");
    child.wait_with_output().expect("waiting for the session")
}

/// The one object carrying this `op`, or a failure that prints what did come
/// back.
///
/// By `op` rather than by position: the session prints a banner and a prompt,
/// and a test that took the last line would pass on a session that answered
/// nothing at all.
fn answering(output: &Output, op: &str) -> serde_json::Value {
    let said = String::from_utf8_lossy(&output.stdout);
    said.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value["op"] == op)
        .unwrap_or_else(|| panic!("nothing answered `{op}`:\n{said}"))
}

fn store() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a root");
    let output = Command::new(thalyx())
        .arg("init")
        .env("THALYX_ROOT", root.path())
        .output()
        .expect("init");
    assert!(output.status.success(), "init: {output:?}");
    root
}

#[test]
fn a_program_may_ask_the_machine_to_start_denying() {
    let root = store();
    let output = typed(root.path(), &["structured on", "negar", "salir"]);

    // There is no BPF here, so the answer is the refusal and not the switch.
    // What this asserts is that the verb exists, reaches `guard`, and answers
    // as itself — the wiring, which is the part unit tests cannot see.
    let said = answering(&output, "deny");
    assert_eq!(said["ok"], serde_json::json!(false), "{said}");
    assert_eq!(said["changed"], serde_json::json!(false), "{said}");
    // A2. The word has to name something that can be run where the message is
    // printed, which is what `make -C lsm enforce` was not.
    assert_eq!(
        said["remedy"],
        serde_json::json!("load_the_kernel_side"),
        "{said}"
    );
}

#[test]
fn a_program_may_not_ask_the_machine_to_stop_denying() {
    let root = store();
    let output = typed(root.path(), &["structured on", "observar", "salir"]);

    let said = answering(&output, "observe");
    assert_eq!(said["ok"], serde_json::json!(false), "{said}");
    assert_eq!(said["error"], serde_json::json!("needs_a_human"), "{said}");
    assert_eq!(
        said["remedy"],
        serde_json::json!("confirm_at_a_terminal"),
        "{said}"
    );
    assert_eq!(said["changed"], serde_json::json!(false), "{said}");
}

/// The control for the test above, and the reason it means anything.
///
/// Without it, a `guard` that refused every request for every reason would
/// look identical to one that refuses only the direction that disarms the
/// machine. The two verbs differ by exactly one word here — `needs_a_human`
/// against `unreadable` — and that word is the whole decision.
#[test]
fn the_two_directions_are_refused_for_different_reasons() {
    let root = store();
    let output = typed(
        root.path(),
        &["structured on", "negar", "observar", "salir"],
    );

    let arming = answering(&output, "deny");
    let disarming = answering(&output, "observe");

    assert_ne!(
        arming["error"], disarming["error"],
        "{arming} / {disarming}"
    );
    assert_eq!(disarming["error"], serde_json::json!("needs_a_human"));
    assert_ne!(arming["error"], serde_json::json!("needs_a_human"));
}

#[test]
fn a_rehearsal_answers_as_a_rehearsal_and_names_the_verb_it_stood_in_for() {
    let root = store();
    let output = typed(root.path(), &["structured on", "ensayo observar", "salir"]);

    // `describe` promises `rehearse` for `ensayo`. An answer that came back
    // under `observe` would be read by a caller as the machine having been
    // disarmed by a dry run.
    let said = answering(&output, "rehearse");
    assert_eq!(said["verb"], serde_json::json!("observe"), "{said}");
}

/// Both spellings, and not by comparing strings.
///
/// The catalogue is a claim about what the session understands; this is the
/// part of that claim these two verbs have to keep. An unknown verb answers
/// with a different `op` entirely, so finding this one is the assertion.
#[test]
fn the_english_spellings_reach_the_same_two_verbs() {
    let root = store();
    let output = typed(root.path(), &["structured on", "deny", "observe", "salir"]);

    answering(&output, "deny");
    answering(&output, "observe");
}

/// Rule 4: a denial test needs a baseline. This is it.
///
/// `estado` reads the same mode flag through the same store, so a machine
/// where these verbs refuse must be a machine whose mode does not read as
/// enforcing — otherwise the refusals above are about something else.
#[test]
fn the_machine_these_refusals_come_from_is_one_with_nothing_loaded() {
    let root = store();
    let output = typed(root.path(), &["structured on", "estado", "salir"]);

    let said = answering(&output, "state");
    assert_ne!(
        said["enforcement"],
        serde_json::json!("enforcing"),
        "{said}"
    );
}
