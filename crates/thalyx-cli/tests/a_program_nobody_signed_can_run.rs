//! `ejecutar` — G1, and what a foreign program can actually see.
//!
//! `vault/02-Arquitectura/Programas-Ajenos.md`. Every assertion here asks the
//! **confined program** what it can reach and compares it with what this test
//! process can reach, which is rule 2 of `Estrategia-de-Pruebas.md`: asking
//! Thalyx whether it confined something would prove nothing, and the class of
//! bug this project keeps finding is the system reporting success for work it
//! did not do.
//!
//! The runs need a cgroup2 mount with the `memory` and `pids` controllers
//! delegated. Where they are missing these print `NOT PROVEN`, say they did not
//! run, and `THALYX_REQUIRE_CONTROLLER_TESTS=1` turns that into a failure. The
//! one test that needs none of it — that a machine with nothing to enforce
//! refuses rather than running the program anyway — runs everywhere.

use std::path::{Path, PathBuf};
use thalyx_core::{ForeignRequest, Store};
use thalyx_manifest::{Permission, PermissionKind};

/// A program nobody signed: a shell script in a directory of its own.
///
/// Its own directory matters. It becomes the tree mounted at `/module` inside
/// the pivot, so a script written into a directory shared with the test's other
/// fixtures would be handing the program those fixtures too — and the test that
/// says it cannot reach them would be testing the fixture layout.
struct Guest {
    home: tempfile::TempDir,
}

impl Guest {
    fn saying(script: &str) -> Self {
        let home = tempfile::tempdir().expect("temp dir");
        let program = home.path().join("guest");
        std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();

        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { home }
    }

    fn program(&self) -> PathBuf {
        self.home.path().join("guest")
    }
}

fn store() -> (tempfile::TempDir, Store) {
    let root = tempfile::tempdir().expect("temp dir");
    let store = Store::open(root.path()).expect("store");
    (root, store)
}

fn grant(path: &Path, action: &str) -> Permission {
    Permission {
        resource: path.display().to_string(),
        action: action.to_string(),
        kind: PermissionKind::Jit,
    }
}

fn request<'a>(program: &'a Path, grants: Vec<Permission>) -> ForeignRequest<'a> {
    ForeignRequest {
        program,
        args: Vec::new(),
        grants,
        // The helper is the binary under test: it re-executes itself into the
        // cgroup and only then becomes the guest.
        helper: PathBuf::from(env!("CARGO_BIN_EXE_thalyx")),
        request_id: "test-request".to_string(),
        profile: thalyx_sandbox::profile::MODULE_STANDARD,
    }
}

/// Whether this machine can hand down what the profile needs.
///
/// Said as `NOT PROVEN` rather than passed over, and keyed on the controllers
/// because that is the thing that is missing — a development container has a
/// cgroup2 mount and no delegation, and reporting "no cgroup2" there would send
/// somebody looking for the wrong thing.
fn ran_or_not_proven(error: &thalyx_core::CoreError, what: &str) -> bool {
    let said = error.to_string();
    if !said.contains("controller") {
        return false;
    }

    eprintln!("NOT PROVEN: this machine does not delegate the cgroup controllers, so {what}");
    eprintln!("  {said}");
    eprintln!("  This test did not run. It did not pass.");
    assert!(
        std::env::var_os("THALYX_REQUIRE_CONTROLLER_TESTS").is_none(),
        "{said}"
    );
    true
}

#[test]
fn a_machine_with_nothing_to_enforce_refuses_rather_than_running_it_anyway() {
    // The claim that separates this verb from `correr`. A module has
    // `sin-confinar`, and it earns it: a human read that manifest and its
    // publisher answered for it. Nobody answered for this, so the fallback does
    // not exist — and a verb that ran the program anyway, with a warning, would
    // be exactly the "did something other than what it was told" this project
    // keeps arranging against.
    let (_root, store) = store();
    let guest = Guest::saying("echo I should never have run");
    let program = guest.program();

    let nothing = thalyx_permd::MemoryStore::unavailable();
    let error = thalyx_core::run_foreign(&store, &nothing, request(&program, Vec::new()))
        .expect_err("a machine that can enforce nothing must not run an unsigned program");

    let said = error.to_string();
    assert!(said.contains("policy map is not loaded"), "{said}");
    // And it must not offer a way round itself. `correr`'s message ends by
    // naming the `--unconfined` flag; this one says the mode does not exist,
    // which is the opposite sentence and has to stay the opposite sentence.
    assert!(!said.contains("--unconfined"), "{said}");
    assert!(said.contains("no unconfined mode"), "{said}");
}

#[test]
fn a_kernel_that_is_only_watching_does_not_get_to_run_a_guest() {
    // `make -C lsm load` lands in observe mode on purpose, so this is the
    // state a machine is in right after the human does the thing the previous
    // refusal told them to do. Until 2026-08-25 the run went ahead there: the
    // policy map opened, `is_available()` said yes, and every denial the guest
    // earned would have been written to a log and not applied.
    let (_root, store) = store();
    let guest = Guest::saying("echo I should never have run");
    let program = guest.program();

    let watching = thalyx_permd::MemoryStore::observing();
    let error = thalyx_core::run_foreign(&store, &watching, request(&program, Vec::new()))
        .expect_err("a kernel that denies nothing must not run an unsigned program");

    let said = error.to_string();
    assert!(said.contains("only observing"), "{said}");
    // The remedy has to be the one that fixes *this*. Sending the human back
    // to `make -C lsm load` is what the other refusal says, and following it
    // here would leave them exactly where they started.
    assert!(said.contains("make -C lsm enforce"), "{said}");
    assert!(!said.contains("--unconfined"), "{said}");
}

#[test]
fn a_kernel_whose_mode_could_not_be_read_does_not_get_to_run_a_guest() {
    // Rules 9 and 10 together: an unread answer is not a "no", and it is
    // certainly not a "yes". The dangerous default here is the fast one.
    let (_root, store) = store();
    let guest = Guest::saying("echo I should never have run");
    let program = guest.program();

    let mute = thalyx_permd::MemoryStore::mode_unreadable("the pin is something else");
    let error = thalyx_core::run_foreign(&store, &mute, request(&program, Vec::new()))
        .expect_err("an unread mode must not be taken for enforcement");

    let said = error.to_string();
    assert!(said.contains("could not be read"), "{said}");
    // What went wrong, carried through rather than flattened into "failed".
    assert!(said.contains("the pin is something else"), "{said}");
}

#[test]
fn an_enforcing_kernel_is_never_refused_for_its_mode() {
    // Rule 4's control column, and it runs on every machine. Without it, a
    // mode check that refused unconditionally would pass both tests above and
    // look exactly like one that works — while `ejecutar` had quietly stopped
    // being able to run anything at all.
    //
    // This does not assert the run succeeds: on a machine with no delegated
    // controllers it cannot. It asserts the refusal, whatever it is, is not
    // about the mode.
    let (_root, store) = store();
    let guest = Guest::saying("echo hello");
    let program = guest.program();

    let enforcing = thalyx_permd::MemoryStore::new();
    if let Err(error) = thalyx_core::run_foreign(&store, &enforcing, request(&program, Vec::new()))
    {
        let said = error.to_string();
        assert!(!said.contains("only observing"), "{said}");
        assert!(!said.contains("could not be read"), "{said}");
    }
}

#[test]
fn the_refusal_a_human_reads_never_counts_zero_of_anything() {
    // The sentence Cesar was handed the first time he ran the verb:
    // «none of the 0 thing(s) this was granted would be enforced». A run with
    // no grants is the ordinary case — it is what `ejecutar <ruta>` with no
    // words after it does — so the ordinary case was the broken one.
    let (_root, store) = store();
    let guest = Guest::saying("echo nothing");
    let program = guest.program();

    let nothing = thalyx_permd::MemoryStore::unavailable();
    let error = thalyx_core::run_foreign(&store, &nothing, request(&program, Vec::new()))
        .expect_err("a machine that can enforce nothing must not run an unsigned program");

    let said = error.to_string();
    assert!(!said.contains(" 0 "), "{said}");
    assert!(!said.contains("thing(s)"), "{said}");
    // And it still says what is wrong and what fixes it.
    assert!(said.contains("policy map is not loaded"), "{said}");
    assert!(said.contains("make -C lsm load"), "{said}");
}

#[test]
fn a_refused_run_is_in_the_journal_and_is_not_called_a_module() {
    // `Marcado-de-Origen` at this layer: what a program nobody signed did has
    // to be separable from what Thalyx did by reading the record, not by
    // remembering. A refusal is part of that record — a refusal that left no
    // trace would be a trusted path nobody could audit.
    let (_root, store) = store();
    let guest = Guest::saying("echo nothing");
    let program = guest.program();

    let nothing = thalyx_permd::MemoryStore::unavailable();
    let _ = thalyx_core::run_foreign(&store, &nothing, request(&program, Vec::new()));

    let written = std::fs::read_to_string(store.journal_path()).expect("a journal");
    assert!(
        written.contains("\"operation\":\"run_foreign\""),
        "the journal did not name this run: {written}"
    );
    assert!(
        !written.contains("run_module"),
        "a guest was recorded as a module: {written}"
    );
    assert!(
        written.contains(&program.display().to_string()),
        "the record does not say which program: {written}"
    );
}

#[test]
fn a_program_nobody_signed_runs_and_its_exit_code_comes_back() {
    // G1 itself, in one line: something that was never installed, never signed
    // and never had a manifest ran, and Thalyx knows how it ended.
    let (_root, store) = store();
    let guest = Guest::saying("echo I am a guest\nexit 7");
    let program = guest.program();

    let policies = thalyx_permd::MemoryStore::new();
    let outcome = match thalyx_core::run_foreign(&store, &policies, request(&program, Vec::new())) {
        Ok(outcome) => outcome,
        Err(error) => {
            assert!(
                ran_or_not_proven(&error, "no guest was launched"),
                "{error}"
            );
            return;
        }
    };

    assert_eq!(outcome.exit_code, Some(7), "{:?}", outcome.wrote);
    assert_eq!(outcome.wrote.stdout.trim(), "I am a guest");
    assert!(outcome.isolated, "a guest ran without being isolated");
    assert!(
        outcome.uid.is_some(),
        "a guest ran as Thalyx rather than as a user of its own"
    );
    // Its own directory came with it, and that is what makes the program
    // reachable at all inside a root it does not otherwise appear in.
    assert!(outcome.name.starts_with("foreign."), "{}", outcome.name);
}

#[test]
fn a_guest_reaches_what_was_granted_and_nothing_beside_it() {
    // Baseline and control in one run, so nobody can read them apart: the same
    // program, in the same launch, is asked about a path that was granted and a
    // path that was not. Without the first, "it cannot see anything" would also
    // be true of a program that never started.
    let (_root, store) = store();

    let visible = tempfile::tempdir().unwrap();
    std::fs::write(visible.path().join("note"), "granted content\n").unwrap();

    let hidden = tempfile::tempdir().unwrap();
    std::fs::write(hidden.path().join("secret"), "never granted\n").unwrap();

    let guest = Guest::saying(&format!(
        "cat {granted}/note; [ -e {ungranted}/secret ] && echo REACHABLE || echo absent",
        granted = visible.path().display(),
        ungranted = hidden.path().display(),
    ));
    let program = guest.program();

    let policies = thalyx_permd::MemoryStore::new();
    let outcome = match thalyx_core::run_foreign(
        &store,
        &policies,
        request(&program, vec![grant(visible.path(), "read")]),
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            assert!(
                ran_or_not_proven(&error, "no guest was launched"),
                "{error}"
            );
            return;
        }
    };

    let saw = outcome.wrote.stdout;
    assert!(
        saw.contains("granted content"),
        "the granted path was not reachable: {saw} {}",
        outcome.wrote.stderr
    );
    assert!(
        saw.contains("absent"),
        "the guest reached a path nobody granted: {saw}"
    );
}

#[test]
fn a_path_granted_for_reading_cannot_be_written_by_the_guest() {
    // The grain of a grant. `read` and `write` are different words on the line
    // a human confirmed, and a system where they produce the same access has
    // made the human's answer meaningless.
    let (_root, store) = store();

    let target = tempfile::tempdir().unwrap();
    let guest = Guest::saying(&format!(
        "touch {}/written 2>/dev/null && echo WRITABLE || echo read-only",
        target.path().display()
    ));
    let program = guest.program();

    let policies = thalyx_permd::MemoryStore::new();
    let outcome = match thalyx_core::run_foreign(
        &store,
        &policies,
        request(&program, vec![grant(target.path(), "read")]),
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            assert!(
                ran_or_not_proven(&error, "no guest was launched"),
                "{error}"
            );
            return;
        }
    };

    assert!(
        outcome.wrote.stdout.contains("read-only"),
        "a read grant let the guest write: {}",
        outcome.wrote.stdout
    );
    // And on the host, which is the half that cannot be faked from inside.
    assert!(!target.path().join("written").exists());
}

#[test]
fn two_runs_of_the_same_program_are_the_same_user() {
    // What the uid keyed on the path buys, and the reason it is keyed on the
    // path at all: what the guest wrote yesterday is still its own today. A
    // fresh user per run would leave a trail of files owned by nobody who will
    // ever come back.
    let (_root, store) = store();
    let guest = Guest::saying("true");
    let program = guest.program();

    let policies = thalyx_permd::MemoryStore::new();
    let first = match thalyx_core::run_foreign(&store, &policies, request(&program, Vec::new())) {
        Ok(outcome) => outcome,
        Err(error) => {
            assert!(
                ran_or_not_proven(&error, "no guest was launched"),
                "{error}"
            );
            return;
        }
    };
    let second = thalyx_core::run_foreign(&store, &policies, request(&program, Vec::new()))
        .expect("the second run");

    assert_eq!(first.uid, second.uid);
    assert_eq!(first.name, second.name);
}
