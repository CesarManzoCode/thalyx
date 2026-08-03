//! `thalyx rollback`, through the real binary.
//!
//! `vault/04-Flujo-Canonico/Rollback-vs-Restore.md` decrees the narrow, cheap
//! operation: it takes back what Thalyx published and touches nothing else.
//! Every test here checks the disk and the registry afterwards rather than the
//! command's own report of what it did — asking the system whether it worked
//! proves nothing, which is the rule the isolation tests were built on.

mod harness;

use harness::Fixture;

#[test]
fn undoing_an_install_takes_the_module_and_its_permissions_back() {
    let fixture = Fixture::new();

    let install = fixture.install();
    assert!(install.success(), "install failed: {}", install.stderr());
    assert!(fixture.store().is_installed(Fixture::MODULE_ID));
    assert!(
        fixture.recorded_permissions() > 0,
        "the fixture module is supposed to be granted something"
    );

    let rollback = fixture.run(&["rollback"]);
    assert!(rollback.success(), "rollback failed: {}", rollback.stderr());

    // The disk, not the report.
    assert!(
        !fixture.store().is_installed(Fixture::MODULE_ID),
        "the module is still installed after being rolled back"
    );
    assert_eq!(
        fixture.recorded_permissions(),
        0,
        "a permission outlived the module it was granted to"
    );
}

#[test]
fn a_dry_run_says_what_would_happen_and_changes_nothing() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let preview = fixture.run(&["rollback", "--dry-run"]);
    assert!(preview.success(), "{}", preview.stderr());
    assert!(
        preview.stdout().contains("undo install_module"),
        "the preview does not say what it would undo: {}",
        preview.stdout()
    );
    assert!(preview.stdout().contains("nothing was undone"));

    assert!(
        fixture.store().is_installed(Fixture::MODULE_ID),
        "--dry-run removed the module"
    );
    assert!(fixture.recorded_permissions() > 0);
}

#[test]
fn nothing_to_undo_is_said_plainly_rather_than_failing_obscurely() {
    // A fresh store. Build-then-commit means an empty journal is the normal
    // state, not an error condition, and the message has to read that way.
    let fixture = Fixture::new();

    let rollback = fixture.run(&["rollback"]);
    assert!(!rollback.success());
    assert!(
        rollback.stderr().contains("nothing to roll back"),
        "{}",
        rollback.stderr()
    );
}

#[test]
fn a_failed_install_leaves_nothing_to_roll_back() {
    // The property build-then-commit exists for, checked from the other end:
    // after a rejected install there is no commit in the journal, so rollback
    // has nothing to find. If this ever starts finding one, something was
    // published that should not have been.
    let fixture = Fixture::new();

    let tampered = fixture.tamper_with_artifact();
    let install = fixture.install_bundle_at(&tampered);
    assert!(!install.success(), "a tampered bundle installed");

    let rollback = fixture.run(&["rollback"]);
    assert!(!rollback.success());
    assert!(
        rollback.stderr().contains("nothing to roll back"),
        "{}",
        rollback.stderr()
    );
}

#[test]
fn rolling_back_twice_refuses_the_second_time() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    assert!(fixture.run(&["rollback"]).success());

    let again = fixture.run(&["rollback"]);
    assert!(
        !again.success(),
        "the second rollback claimed to do something"
    );
    // The reason has to be the specific one: with the module gone, the entry
    // that published it is still the most recent commit in the journal.
    assert!(
        again.stderr().contains("already gone"),
        "{}",
        again.stderr()
    );
}

#[test]
fn an_upgrade_is_not_deleted_by_undoing_the_install_before_it() {
    // The failure the whole design is shaped around. The journal's most recent
    // commit published 1.0.0; the disk holds 1.1.0. Undoing "the install"
    // against the journal alone would delete a version the human still wants.
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let newer = fixture.build_bundle("1.1.0");
    assert!(
        fixture
            .run(&["module", "remove", Fixture::MODULE_ID])
            .success()
    );
    assert!(fixture.install_bundle_at(&newer).success());

    // Now roll back by naming the original request, which published 1.0.0.
    let store = fixture.store();
    let journal = thalyx_journal::Journal::open(store.journal_path()).unwrap();
    let original = journal
        .entries()
        .unwrap()
        .into_iter()
        .find(|entry| {
            entry.operation == "install_module"
                && entry.version.as_deref() == Some("1.0.0")
                && entry.outcome == thalyx_journal::Outcome::Success
        })
        .expect("the first install is in the journal");

    let rollback = fixture.run(&["rollback", "--request", &original.request_id]);
    assert!(
        !rollback.success(),
        "it undid an entry that no longer holds"
    );
    assert!(
        rollback.stderr().contains("1.1.0"),
        "the refusal does not name what is actually installed: {}",
        rollback.stderr()
    );
    assert_eq!(
        fixture
            .store()
            .installed_version(Fixture::MODULE_ID)
            .as_deref(),
        Some("1.1.0"),
        "the newer version was deleted"
    );
}

#[test]
fn the_journal_records_the_rollback_as_its_own_operation() {
    // An audit that cannot tell "removed because unwanted" from "undone
    // because it should not have happened" has lost the distinction the two
    // commands exist to make.
    let fixture = Fixture::new();
    assert!(fixture.install().success());
    assert!(fixture.run(&["rollback"]).success());

    let shown = fixture.run(&["journal"]);
    assert!(shown.success(), "{}", shown.stderr());
    assert!(
        shown.stdout().contains("rollback"),
        "the journal does not show the rollback: {}",
        shown.stdout()
    );
}
