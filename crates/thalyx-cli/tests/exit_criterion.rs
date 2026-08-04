//! The steps that end Phase 1, driven at the machine's own prompt.
//!
//! `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md` closes the phase
//! when a person outside the project, following only the README, boots the
//! image, installs a signed module from a local repository, confirms its
//! permissions on the trusted path, reverts it, powers the machine off, and
//! comes back to one that still knows what it was asked.
//!
//! Four of those six happen at the session prompt with no shell behind it, and
//! none of that half needs a kernel with BPF, a Btrfs filesystem or a delegated
//! cgroup. It needs a terminal — `TerminalConfirmer` refuses to confirm without
//! one, because silence is not consent — and Thalyx now makes its own.
//!
//! ## Why this is a test and not only a stage of `verify.sh`
//!
//! It was only a stage, and that is how it came to be unchecked. Stage 15
//! needed `script(1)`, which Fedora ships in a subpackage nobody installs, so
//! on 2026-08-04 the whole thing skipped itself and said `NOT PROVEN`. The
//! criterion that ends the phase was being verified by one command that one
//! person ran by hand on one machine, and that command silently stopped
//! verifying it.
//!
//! The parts that genuinely need hardware — booting the image, the kernel
//! enforcing, a real reboot — stay in `verify.sh`, where a machine that cannot
//! answer says so. What does not need hardware belongs here, where it runs on
//! every change.
//!
//! ## What every assertion here is careful about
//!
//! Asking the *session* whether it did something proves nothing; each step is
//! checked against the store on disk, from outside the session that made it.

mod harness;

use harness::Fixture;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// Type `lines` at the session prompt, and keep everything it said.
///
/// Through `thalyx dev pty`, because the confirmer refuses a stdin that is not
/// a terminal. Each call is a **separate process** with nothing carried over,
/// which is what makes the memory checks mean anything: a later session reads
/// back what an earlier one wrote, with no state in between but the disk.
fn at_the_prompt(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .args(["dev", "pty", "--", thalyx(), "session"])
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session, on a terminal of Thalyx's own making");

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

fn said(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).replace('\r', "");
    text.push_str(&String::from_utf8_lossy(&output.stderr).replace('\r', ""));
    text
}

/// A store holding a signed bundle in its repository, and nothing installed.
///
/// The module arrives **uninstalled**, on purpose: a machine that booted with
/// it already in place makes step 2 impossible to perform and step 3 never
/// happen at all.
fn machine_with_a_bundle_to_install(fixture: &Fixture) -> std::path::PathBuf {
    let root = fixture.root();
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).expect("the repository");

    let bundle = fixture.build_bundle("1.0.0");
    std::fs::copy(&bundle, repo.join("demo-1.0.0.thmod")).expect("putting it in the repository");
    root
}

#[test]
fn step_2_a_signed_module_installs_from_a_local_repository_at_the_prompt() {
    let fixture = Fixture::new();
    let root = machine_with_a_bundle_to_install(&fixture);

    let listing = at_the_prompt(&root, &["disponibles", "salir"]);
    assert!(
        said(&listing).contains(Fixture::MODULE_ID),
        "the machine did not list what its repository holds:\n{}",
        said(&listing)
    );

    let install = at_the_prompt(
        &root,
        &[&format!("instalar {}", Fixture::MODULE_ID), "y", "salir"],
    );
    assert!(
        said(&install).contains("installed"),
        "the module did not install from the prompt:\n{}",
        said(&install)
    );

    // Against the disk, from outside the session that claimed it.
    assert_eq!(
        fixture
            .store()
            .installed_version(Fixture::MODULE_ID)
            .as_deref(),
        Some("1.0.0"),
        "the session reported an install that is not on disk"
    );
}

#[test]
fn step_3_the_permission_is_shown_and_identified_as_thalyx_s_before_it_is_granted() {
    let fixture = Fixture::new();
    let root = machine_with_a_bundle_to_install(&fixture);

    let install = at_the_prompt(
        &root,
        &[&format!("instalar {}", Fixture::MODULE_ID), "y", "salir"],
    );
    let seen = said(&install);

    // Three things, because any one alone is weak: the banner that says the
    // question is Thalyx's rather than a module's, the permission itself, and
    // the question. A flow that installed first and listed afterwards would
    // satisfy a looser check.
    assert!(
        seen.contains("Thalyx — capability authorisation"),
        "the prompt was not identified as Thalyx's:\n{seen}"
    );
    assert!(
        seen.contains("outbound network access"),
        "the network permission was never shown:\n{seen}"
    );
    assert!(seen.contains("Confirm?"), "nobody was asked:\n{seen}");

    // The permission the human was shown has to be the whole permission. A
    // granted path that renders truncated is a grant the human cannot tell
    // from a different one under the same parent.
    let granted = fixture.granted_path();
    let leaf = granted
        .file_name()
        .and_then(|name| name.to_str())
        .expect("the granted directory has a name");
    assert!(
        seen.contains(leaf),
        "the granted path was shown without its final component, so it cannot \
         be told from any sibling:\n{seen}"
    );
}

#[test]
fn step_3_control_answering_no_installs_nothing_and_remembers_nothing() {
    // Without this, a session that installed whatever the human answered would
    // pass every other check here. The refusal has to be what stops it, not
    // the absence of an opportunity.
    let fixture = Fixture::new();
    let root = machine_with_a_bundle_to_install(&fixture);

    let refused = at_the_prompt(
        &root,
        &[&format!("instalar {}", Fixture::MODULE_ID), "n", "salir"],
    );
    assert!(
        said(&refused).contains("did not confirm"),
        "a refusal was not reported as one:\n{}",
        said(&refused)
    );

    assert!(
        !fixture.store().is_installed(Fixture::MODULE_ID),
        "answering no installed the module anyway"
    );

    // And nothing was remembered either. The record is written after the commit
    // and never before, so somebody who said no does not come back to a machine
    // that remembers them saying yes — which a memory written from the
    // *request* rather than from the act would do.
    let memory = at_the_prompt(&root, &["recuerdos", "salir"]);
    assert!(
        said(&memory).contains("nothing recorded"),
        "the machine remembered an install that never happened:\n{}",
        said(&memory)
    );
}

#[test]
fn step_4_the_installation_can_be_reverted_from_the_prompt() {
    let fixture = Fixture::new();
    let root = machine_with_a_bundle_to_install(&fixture);

    at_the_prompt(
        &root,
        &[&format!("instalar {}", Fixture::MODULE_ID), "y", "salir"],
    );
    assert!(
        fixture.store().is_installed(Fixture::MODULE_ID),
        "nothing was installed, so there is nothing to revert and this proves nothing"
    );

    let reverted = at_the_prompt(&root, &["revertir", "salir"]);
    assert!(
        !fixture.store().is_installed(Fixture::MODULE_ID),
        "`revertir` left the module installed:\n{}",
        said(&reverted)
    );

    // And what it held goes with it. A permission outliving the module it was
    // granted to is the orphaned grant `Permisos-JIT` forbids.
    assert!(
        fixture.effective_permissions().is_empty(),
        "the module is gone and its permissions are still in force"
    );
}

#[test]
fn step_6_a_later_session_says_what_was_asked_and_rechecks_what_it_did() {
    // The step with the least behind it, and the one whose shape matters most.
    //
    // Every `at_the_prompt` is a separate process holding nothing, so this is a
    // session that installed nothing reading back what another one did. That is
    // the mechanism a reboot exercises; what a reboot adds is the kernel going
    // away, and what makes *that* survivable is the record being a file. Stage
    // 16 of `verify.sh` does the reboot.
    let fixture = Fixture::new();
    let root = machine_with_a_bundle_to_install(&fixture);

    // The baseline. Without it, a `recuerdos` that printed a fixed paragraph
    // would satisfy everything below, and step 6 would be theatre in the one
    // place the criterion looks.
    let before = at_the_prompt(&root, &["recuerdos", "salir"]);
    assert!(
        said(&before).contains("nothing recorded"),
        "a machine that has done nothing claimed a memory:\n{}",
        said(&before)
    );

    at_the_prompt(
        &root,
        &[&format!("instalar {}", Fixture::MODULE_ID), "y", "salir"],
    );

    let after = at_the_prompt(&root, &["recuerdos", "salir"]);
    let remembered = said(&after);
    assert!(
        remembered.contains(Fixture::MODULE_ID),
        "a new session did not know what the previous one was asked:\n{remembered}"
    );

    // And it is re-checked rather than replayed: the machine goes and looks,
    // and says the installation still stands.
    assert!(
        !remembered.contains("no longer confirm"),
        "the machine cannot confirm an install that is still on disk:\n{remembered}"
    );
}

#[test]
fn step_6_the_memory_is_rechecked_against_the_disk_and_not_replayed() {
    // The half that makes step 6 worth anything without a model. After
    // `revertir`, the machine still remembers being asked — no file can make it
    // false that somebody said something — and it can no longer stand behind
    // what it did, **on its own**, with nobody having told it the module left.
    //
    // A memory that replayed what it was told would say the install still holds.
    let fixture = Fixture::new();
    let root = machine_with_a_bundle_to_install(&fixture);

    at_the_prompt(
        &root,
        &[&format!("instalar {}", Fixture::MODULE_ID), "y", "salir"],
    );
    at_the_prompt(&root, &["revertir", "salir"]);
    assert!(
        !fixture.store().is_installed(Fixture::MODULE_ID),
        "the revert did not happen, so this proves nothing"
    );

    let after = at_the_prompt(&root, &["recuerdos", "salir"]);
    let remembered = said(&after);

    assert!(
        remembered.contains(Fixture::MODULE_ID),
        "the request itself was forgotten; no file can make it false that \
         somebody asked:\n{remembered}"
    );
    assert!(
        remembered.contains("no longer confirm"),
        "the machine still asserts an installation that was undone, so its \
         memory is a replay rather than a re-check:\n{remembered}"
    );
}
