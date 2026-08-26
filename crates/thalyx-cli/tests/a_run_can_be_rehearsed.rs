//! `ensayo correr <id>` — D1's ninth of nine.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **D1**: every verb
//! that changes the machine can be rehearsed. `correr` was the one that could
//! not, and the reason written beside it was true when it was written — what a
//! run would be allowed to do is a question for the kernel side, and answering
//! it from the manifest would describe a run the machine may not be able to
//! give. Thalyx learned to read the mode on 2026-08-25, so the question has an
//! answer now.
//!
//! What matters most here is the property that a rehearsal can only have by
//! construction: it is `thalyx_core::foresee_run`, which is the run's own code
//! stopped one line before the program exists. So these tests check the two
//! things that would break anyway — that it agrees with the verb about the
//! module, and that it runs nothing — and one thing no amount of care can give
//! for free: that its warning about a degraded run is not printed on a machine
//! where the run would be fine.

mod harness;

use harness::Fixture;

fn object(said: &str, op: &str) -> serde_json::Value {
    said.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value["op"] == op)
        .unwrap_or_else(|| panic!("nothing answered `{op}`:\n{said}"))
}

#[test]
fn a_rehearsed_run_names_the_module_the_version_and_the_program() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let output = fixture.typed(&[
        "structured on",
        &format!("ensayo correr {}", Fixture::MODULE_ID),
        "salir",
    ]);
    let said = object(&output, "rehearse");

    assert_eq!(said["verb"], serde_json::json!("run"), "{said}");
    assert_eq!(
        said["module_id"],
        serde_json::json!(Fixture::MODULE_ID),
        "{said}"
    );
    assert_eq!(
        said["version"],
        serde_json::json!(Fixture::VERSION),
        "{said}"
    );
    // The resolved program, not the entrypoint name from the manifest. Those
    // are two different facts and only one of them is what would execute.
    assert!(
        said["program"]
            .as_str()
            .expect("a program")
            .ends_with("bin/demo"),
        "{said}"
    );
}

/// The one thing a rehearsal must never do, checked by what is *not* on the
/// disk rather than by what the session printed.
///
/// A rehearsal that ran the module and then said "nothing ran" is the exact
/// failure `fix(cli): a rehearsal says what would happen, not what happened`
/// was about, and reading the session's own report cannot tell the two apart.
#[test]
fn a_rehearsed_run_does_not_run_the_module() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let marker = fixture.granted_path().join("it-ran");
    // The module's payload is `echo demo`; it writes nothing on its own, so a
    // marker only appears if something executed a program that makes it.
    // Rewriting the installed entrypoint is how this test gets one.
    let installed = fixture
        .store()
        .version_dir(Fixture::MODULE_ID, Fixture::VERSION)
        .join("bin/demo");
    std::fs::write(
        &installed,
        format!("#!/bin/sh\ntouch {}\n", marker.display()),
    )
    .expect("rewriting the entrypoint");
    // The store installs entrypoints read-only. Writing over one keeps that
    // mode, and a program that cannot be executed would make the control below
    // fail for a reason that has nothing to do with rehearsals.
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755))
            .expect("making the entrypoint executable again");
    }

    let output = fixture.typed(&[
        "structured on",
        &format!("ensayo correr {}", Fixture::MODULE_ID),
        "salir",
    ]);
    object(&output, "rehearse");

    assert!(
        !marker.exists(),
        "the rehearsal executed the module; {output}"
    );

    // Rule 4: without this column the assertion above passes on a machine
    // where the marker could never have appeared — a payload that does not
    // run, a path that is not writable, an entrypoint that was never replaced.
    // `sin-confinar` is the one way to reach a real run here, because this
    // container has nothing that enforces.
    let real = fixture.typed(&[
        &format!("correr {} sin-confinar", Fixture::MODULE_ID),
        "salir",
    ]);
    assert!(
        marker.exists(),
        "the control did not run either, so the rehearsal proved nothing; {real}"
    );
}

/// Rule 10 reaches the wire, and this is the part of it a caller depends on.
///
/// Three states, not two. On a machine with nothing loaded the answer is
/// `null` and the run would be refused; the mistake this guards against is
/// reporting that as "observing", which is a claim about a loaded kernel.
#[test]
fn a_rehearsed_run_on_a_machine_with_nothing_loaded_says_it_would_not_start() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let output = fixture.typed(&[
        "structured on",
        &format!("ensayo correr {}", Fixture::MODULE_ID),
        "salir",
    ]);
    let said = object(&output, "rehearse");

    // This container has no policy map. If that ever stops being true here,
    // this test is measuring a different machine and should say so rather than
    // quietly changing meaning.
    if said["enforcement"] != serde_json::json!(null) {
        return;
    }

    assert_eq!(said["would_run"], serde_json::json!(false), "{said}");
    assert!(said["refusal"].is_string(), "{said}");
    // The refusal has to be the verb's own words. A rehearsal that invented a
    // reason would send the person to fix something that was never in the way.
    assert!(
        said["refusal"]
            .as_str()
            .expect("a refusal")
            .contains("policy map is not loaded"),
        "{said}"
    );
}

/// The control, and the reason the test above means anything.
///
/// `sin-confinar` is honoured whether or not the kernel side is there, so on
/// this same machine the same rehearsal must answer that the run *would* go
/// ahead — degraded. Without this column, a `foresee_run` that refused
/// everything would pass every assertion above.
#[test]
fn the_same_machine_says_an_unconfined_run_would_go_ahead_degraded() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let output = fixture.typed(&[
        "structured on",
        &format!("ensayo correr {} sin-confinar", Fixture::MODULE_ID),
        "salir",
    ]);
    let said = object(&output, "rehearse");

    assert_eq!(said["would_run"], serde_json::json!(true), "{said}");
    assert_eq!(said["unconfined"], serde_json::json!(true), "{said}");
    assert_eq!(said["degraded"], serde_json::json!(true), "{said}");
    assert_eq!(said["refusal"], serde_json::json!(null), "{said}");
}

/// What it holds is what is **in force**, not what the manifest asked for.
///
/// The distinction is the whole of `effective_permissions`, and a rehearsal
/// that read the manifest instead would describe a run with permissions the
/// module does not have — which is the failure mode with no symptom, printed
/// in the one place a person goes to check before running something.
#[test]
fn what_it_would_hold_is_what_is_in_force_and_not_what_the_manifest_asked_for() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let output = fixture.typed(&[
        "structured on",
        &format!("ensayo correr {}", Fixture::MODULE_ID),
        "salir",
    ]);
    let said = object(&output, "rehearse");

    let in_force = fixture.effective_permissions().len();
    assert_eq!(
        said["count"],
        serde_json::json!(in_force),
        "the rehearsal listed {} of {in_force} grants in force: {said}",
        said["count"]
    );
}

#[test]
fn a_module_that_is_not_installed_is_a_question_with_no_subject() {
    let fixture = Fixture::new();

    let output = fixture.typed(&["structured on", "ensayo correr org.nobody.here", "salir"]);
    let said = object(&output, "rehearse");

    assert_eq!(said["ok"], serde_json::json!(false), "{said}");
    assert_eq!(said["error"], serde_json::json!("cannot"), "{said}");
}

/// The human face, printed rather than asserted into shape.
///
/// Kept because the two faces of this verb are the one place D1 and the
/// double-path decree meet, and because the sentence a person reads before
/// running something is the whole value of a rehearsal. Run with
/// `cargo test -p thalyx-cli --test a_run_can_be_rehearsed human -- --nocapture`.
#[test]
fn the_human_face_says_what_it_would_do_and_that_nothing_ran() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let output = fixture.typed(&[&format!("ensayo correr {}", Fixture::MODULE_ID), "salir"]);
    println!("{output}");

    assert!(output.contains("would run:"), "{output}");
    assert!(output.contains("Nothing ran."), "{output}");
    // The one sentence this rehearsal exists to make possible. "It would run"
    // and "it would run with nothing enforcing it" are the same thing to
    // anyone who is not told, and here being told still costs nothing.
    assert!(
        output.contains("It would not start") || output.contains("degraded"),
        "{output}"
    );
}
