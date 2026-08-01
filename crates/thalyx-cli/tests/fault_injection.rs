//! Level 2 tests: fault injection over the commit.
//!
//! These are the tests that turn "the commit is atomic" from a claim into
//! evidence. Each one spawns the real `thalyx` binary, kills it at a named
//! point with `SIGABRT` — no unwinding, no destructors, no flush — and then
//! checks the store against a single invariant:
//!
//! > **Published or not published. Never halfway.**
//!
//! Concretely: `modules/<id>/current` resolves to a complete version directory,
//! or the module is not installed at all. Nothing in between, and no permission
//! in force for a module that is not installed.
//!
//! See `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md`.

mod harness;

use harness::Fixture;
use thalyx_core::fault::FaultPoint;

/// After every interruption, this must hold.
fn assert_invariant(fixture: &Fixture, context: &str) {
    let store = fixture.store();

    match store.installed_version(Fixture::MODULE_ID) {
        Some(version) => {
            // Claimed installed: the payload must actually be there.
            let dir = store.version_dir(Fixture::MODULE_ID, &version);
            assert!(
                dir.join("bin/demo").is_file(),
                "{context}: `current` points at {version} but the payload is missing — \
                 this is a half-published module"
            );
        }
        None => {
            // Claimed not installed: nothing may be in force for it.
            let permissions = fixture.effective_permissions();
            assert!(
                permissions.is_empty(),
                "{context}: module is not installed but holds {} permission(s) — \
                 an orphan grant",
                permissions.len()
            );
        }
    }
}

#[test]
fn interrupting_before_anything_is_written_leaves_nothing_installed() {
    let fixture = Fixture::new();
    let status = fixture.install_with_fault(FaultPoint::PostVerify);

    assert!(status.aborted(), "expected the process to abort");
    assert_invariant(&fixture, "post-verify");
    assert!(!fixture.store().is_installed(Fixture::MODULE_ID));
    assert!(fixture.effective_permissions().is_empty());
}

#[test]
fn interrupting_after_staging_leaves_nothing_installed() {
    let fixture = Fixture::new();
    let status = fixture.install_with_fault(FaultPoint::PostStage);

    assert!(status.aborted());
    assert_invariant(&fixture, "post-stage");
    assert!(!fixture.store().is_installed(Fixture::MODULE_ID));

    // The staged tree is left behind, and that is fine: nothing points at it.
    // It is reclaimable space, not a half-installed module.
    let leftovers = std::fs::read_dir(fixture.store().staging_root())
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert!(leftovers > 0, "expected an inert staging leftover");
}

/// The one that matters.
#[test]
fn interrupting_between_the_two_renames_leaves_the_module_uninstalled() {
    let fixture = Fixture::new();
    let status = fixture.install_with_fault(FaultPoint::MidCommit);

    assert!(status.aborted());
    assert_invariant(&fixture, "mid-commit");

    let store = fixture.store();

    // The version directory is already in its final location...
    assert!(
        store
            .version_dir(Fixture::MODULE_ID, Fixture::VERSION)
            .is_dir(),
        "expected the version directory to have been renamed into place"
    );

    // ...but `current` never swung, so the module is not installed.
    assert!(
        !store.is_installed(Fixture::MODULE_ID),
        "a version directory without a `current` link must not count as installed"
    );

    // And it is reported as an orphan rather than silently ignored.
    assert_eq!(
        store.orphaned_versions().unwrap(),
        vec![(Fixture::MODULE_ID.to_string(), Fixture::VERSION.to_string())]
    );

    // No permission is in force, even though the grant record was written
    // before the commit: effectiveness is gated on being current.
    assert!(fixture.effective_permissions().is_empty());
}

#[test]
fn interrupting_after_the_commit_leaves_the_module_installed_and_usable() {
    let fixture = Fixture::new();
    let status = fixture.install_with_fault(FaultPoint::PostCommit);

    assert!(status.aborted());
    assert_invariant(&fixture, "post-commit");

    // Past the symlink swap the installation stands on its own, permissions
    // included, without needing any of the steps that never ran.
    assert_eq!(
        fixture
            .store()
            .installed_version(Fixture::MODULE_ID)
            .as_deref(),
        Some(Fixture::VERSION)
    );
    assert_eq!(fixture.effective_permissions().len(), 2);
}

#[test]
fn the_invariant_holds_at_every_fault_point() {
    for point in FaultPoint::ALL {
        let fixture = Fixture::new();
        let status = fixture.install_with_fault(point);
        assert!(status.aborted(), "{point}: expected an abort");
        assert_invariant(&fixture, &point.to_string());
    }
}

#[test]
fn an_interrupted_install_can_be_retried_and_succeeds() {
    // Recovery matters as much as consistency: a store left by a crash has to
    // be usable, not just intact.
    for point in FaultPoint::ALL {
        let fixture = Fixture::new();
        fixture.install_with_fault(point);

        let retry = fixture.install();
        assert!(
            retry.success() || retry.stderr().contains("already installed"),
            "{point}: retry failed: {}",
            retry.stderr()
        );
        assert_eq!(
            fixture
                .store()
                .installed_version(Fixture::MODULE_ID)
                .as_deref(),
            Some(Fixture::VERSION),
            "{point}: module should be installed after the retry"
        );
        assert_eq!(fixture.effective_permissions().len(), 2);
    }
}

#[test]
fn a_commit_interrupted_before_its_journal_entry_is_recovered_by_reconciliation() {
    // The gap this closes: dying right after the symlink swap used to leave the
    // module installed and functional with no record that it ever happened.
    // The intent is written before anything moves, so what survives is an
    // unresolved question rather than a silent installation.
    let fixture = Fixture::new();
    let status = fixture.install_with_fault(FaultPoint::PostCommit);
    assert!(status.aborted());

    // Installed, and the journal knows something was attempted.
    assert!(fixture.store().is_installed(Fixture::MODULE_ID));
    let before = fixture.run(&["store", "status"]).stdout();
    assert!(
        before.contains("intents   1 unresolved"),
        "expected one unresolved intent, got: {before}"
    );

    // The disk answers the question.
    let reconcile = fixture.run(&["store", "reconcile"]);
    assert!(reconcile.success());
    assert!(
        reconcile.stdout().contains("the commit had happened"),
        "reconciliation should conclude the commit happened: {}",
        reconcile.stdout()
    );

    // The installation is now recorded, and nothing is left hanging.
    let journal = fixture.run(&["journal"]).stdout();
    assert!(journal.contains("settled by reconciliation"));
    assert!(
        fixture
            .run(&["store", "status"])
            .stdout()
            .contains("intents   0 unresolved")
    );
}

#[test]
fn an_intent_for_a_commit_that_never_happened_is_settled_as_not_committed() {
    let fixture = Fixture::new();
    let status = fixture.install_with_fault(FaultPoint::MidCommit);
    assert!(status.aborted());

    // The intent went down before the commit was attempted at all.
    assert!(!fixture.store().is_installed(Fixture::MODULE_ID));

    let reconcile = fixture.run(&["store", "reconcile"]);
    assert!(reconcile.success());
    assert!(
        reconcile.stdout().contains("no commit had happened"),
        "expected the disk to say there was no commit: {}",
        reconcile.stdout()
    );
    assert_invariant(&fixture, "reconciled mid-commit");
}

#[test]
fn an_interrupted_install_still_pins_the_publisher_key() {
    // Found by running the real thing after the intent log landed: `module
    // list` said "publisher unpinned" for a module that was installed. Pinning
    // used to happen after the commit, so a crash in between left the id with
    // no key — and the next bundle offered for it, signed by anyone, would
    // have been accepted as a first sighting. That is adversary 3 of the
    // threat model, opened by an interruption rather than by an attack.
    let fixture = Fixture::new();
    fixture.install_with_fault(FaultPoint::PostCommit);
    assert!(fixture.store().is_installed(Fixture::MODULE_ID));

    let listing = fixture.run(&["module", "list"]).stdout();
    assert!(
        !listing.contains("unpinned"),
        "an installed module must have its publisher key pinned: {listing}"
    );

    // And the pin actually holds against a different key.
    let impostor = fixture.bundle_from_a_new_publisher("2.0.0");
    let result = fixture.run(&["module", "install", impostor.to_str().unwrap(), "--yes"]);
    assert!(
        result
            .stderr()
            .contains("changed since it was first trusted"),
        "key rotation should be refused after an interrupted install: {}",
        result.stderr()
    );
}

#[test]
fn installing_settles_anything_a_previous_crash_left_hanging() {
    // A user who just retries should never have to know reconciliation exists.
    let fixture = Fixture::new();
    fixture.install_with_fault(FaultPoint::PostCommit);

    // The retry is refused as already installed, but it still settles the
    // journal on its way through.
    let retry = fixture.install();
    assert!(retry.stderr().contains("already installed"));
    assert!(
        fixture
            .run(&["store", "status"])
            .stdout()
            .contains("intents   0 unresolved")
    );
}

#[test]
fn an_interrupted_upgrade_leaves_the_previous_version_serving() {
    // The strongest form of the invariant: an interrupted upgrade must not
    // just avoid corruption, it must leave the machine working.
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let upgrade = fixture.build_bundle("1.1.0");
    let status = fixture.install_bundle_with_fault(&upgrade, FaultPoint::MidCommit);
    assert!(status.aborted());

    assert_eq!(
        fixture
            .store()
            .installed_version(Fixture::MODULE_ID)
            .as_deref(),
        Some(Fixture::VERSION),
        "the previous version must still be the current one"
    );
    assert_eq!(fixture.effective_permissions().len(), 2);
    assert_invariant(&fixture, "interrupted upgrade");
}
