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
//!
//! ## Why four of these ask the kernel a question before they run
//!
//! Because on 2026-08-27 this file armed Cesar's machine. `THALYX_ROOT` points
//! the *store* at a temporary directory and isolates a test from nothing else:
//! the guard is four bytes in bpffs, beside `KernelStore::DEFAULT_MAP`, and
//! that belongs to the machine running the suite. So under
//! `verify.sh` — as root, with `thalyx-lsm` attached — `negar` here did what
//! `negar` is for. The suite armed his kernel, the next test in this file read
//! «already enforcing» and failed, and §6 of `verify.sh` then measured a
//! kernel this script never asked for and said so.
//!
//! That is the second half of rule 5 and it had not been written down: the
//! harness is not only what asks the question, it is what the question is
//! asked *of*. A test that writes something machine-global has changed the
//! machine it is measuring, and left it changed for everything that runs
//! after.

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

/// Whether a `negar` typed here would move the guard of this machine.
///
/// Asked exactly as `crate::guard::set` asks it, and asked of the kernel:
/// that verb writes when the mode flag reads as something and refuses without
/// writing when it does not, so this is the same boundary and not a guess at
/// it. Deliberately not an existence check on the pin — bpffs is mode 700, and
/// a path test answers «missing» for a map that is there, which is the mistake
/// that once made this project's tooling read as disarmed while it was armed.
fn the_guard_of_this_machine_is_real() -> bool {
    use thalyx_permd::PolicyStore;
    would_switch_this_machine(&thalyx_permd::KernelStore::default_map().enforcement())
}

/// The decision, apart from the reading, so that something can check it.
///
/// The reading needs BPF and this container has none, so the half that can be
/// wrong with no kernel at all is the half that gets a test: an `Unreadable`
/// counted as a real guard would skip every test in this file on every machine
/// there is, and the file would go on printing NOT PROVEN for as long as
/// anybody let it — a skip nobody asked for looks exactly like a machine that
/// cannot do the check.
fn would_switch_this_machine(reading: &thalyx_permd::Enforcement) -> bool {
    !matches!(reading, thalyx_permd::Enforcement::Unreadable(_))
}

#[test]
fn a_flag_that_cannot_be_read_is_not_a_guard_these_tests_would_move() {
    use thalyx_permd::Enforcement;

    assert!(!would_switch_this_machine(&Enforcement::Unreadable(
        "there is no bpffs here".into()
    )));
    // Both of the other two, because the danger is the write and not the mode
    // it would write over: a machine already enforcing is still a machine
    // `negar` reaches.
    assert!(would_switch_this_machine(&Enforcement::Observing));
    assert!(would_switch_this_machine(&Enforcement::Enforcing));
}

/// Rule 3: a skip says it skipped, and says what went unproven.
///
/// With no `THALYX_REQUIRE_*` beside it, and that is not an oversight. Every
/// other skip in this project is a machine that can do *less* than the check
/// needs, and the variable exists so a machine that can do it is never quietly
/// let off. This one is the mirror: the machine can do *more*, and what is
/// missing is not a capability but the empty kernel those four tests are
/// about. A variable that turned this skip into a failure would demand that
/// the only machine that matters stop being able to enforce.
///
/// Where they are measured instead: §37 of `dev/verify.sh`, which arms the
/// machine on purpose, measures it with `bpftool` rather than with Thalyx, and
/// puts it back however the stage ended.
fn not_proven(claim: &str) {
    eprintln!("NOT PROVEN: this machine's kernel guard is real, so {claim}.");
    eprintln!("  Running it would arm this machine for real, and the next thing");
    eprintln!("  to run would be measuring a kernel nobody asked for. §37 of");
    eprintln!("  dev/verify.sh is where these verbs are checked on such a machine.");
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
    // Before the session is spawned, and that ordering is the whole fix: on a
    // machine whose guard is real this verb does not answer a refusal, it arms
    // the kernel.
    if the_guard_of_this_machine_is_real() {
        not_proven("`negar` would switch it instead of answering the refusal this reads");
        return;
    }

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

/// No precondition, and that is the point: this one runs on every machine.
///
/// `guard::set` turns the structured face away from `observar` **before** it
/// reads the kernel, deliberately — "a program may not ask to disarm this
/// machine" is a fact about the request and not about what happens to be
/// pinned. So this types the disarming verb on a machine that can be disarmed
/// and nothing moves, which is a stronger claim than the container can make
/// and the reason the skips above cost less than they look like they cost.
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
    // This one types `negar`, so it carries the same precondition as the test
    // it is the control for — and it has to: a control that ran on a different
    // machine than the thing it controls is not a control.
    if the_guard_of_this_machine_is_real() {
        not_proven("`negar` would come back as a switch, with no `error` to compare");
        return;
    }

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

/// Also unconditional, and for the reason a rehearsal exists: it reads the
/// flag and writes nothing, on any machine.
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
    // `deny` dispatches to the same place `negar` does, which is the claim —
    // and on a machine with a guard, proving it costs that machine's guard.
    // The Spanish half is proven there anyway: §37 types `negar` at the real
    // prompt and reads the flag back with `bpftool`.
    if the_guard_of_this_machine_is_real() {
        not_proven("typing `deny` to find out where it lands would arm the kernel");
        return;
    }

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
    // The baseline skips with the tests it is the baseline for. Left running
    // alone it would be a machine-wide claim nothing above depends on, and on
    // a machine merely observing it would pass while proving nothing about the
    // refusals — which is a baseline that has stopped being one.
    if the_guard_of_this_machine_is_real() {
        not_proven("the refusals it is the baseline for did not run either");
        return;
    }

    let root = store();
    let output = typed(root.path(), &["structured on", "estado", "salir"]);

    let said = answering(&output, "state");
    assert_ne!(
        said["enforcement"],
        serde_json::json!("enforcing"),
        "{said}"
    );
}
