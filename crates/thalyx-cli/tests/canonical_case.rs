//! Level 3: the canonical case, end to end, through the real binary.
//!
//! The happy path is `vault/04-Flujo-Canonico/Caso-Instalar-Modulo.md`.
//! The rejection paths are the ones the threat model says must hold, each
//! exercised against a bundle built to violate exactly one of them.

mod harness;

use harness::Fixture;

#[test]
fn the_canonical_case_end_to_end() {
    let fixture = Fixture::new();

    // Install.
    let install = fixture.install();
    assert!(install.success(), "install failed: {}", install.stderr());
    assert!(install.stdout().contains("installed"));

    // The capability prompt is rendered by the core, banner included, and
    // lists every persistent permission from the manifest.
    assert!(
        install
            .stdout()
            .contains("Thalyx — capability authorisation")
    );
    assert!(install.stdout().contains("outbound network access"));
    assert!(
        install
            .stdout()
            .contains("read access to /home/user/projects")
    );

    // The module is current and its payload is in place.
    let store = fixture.store();
    assert_eq!(
        store.installed_version(Fixture::MODULE_ID).as_deref(),
        Some(Fixture::VERSION)
    );
    assert!(
        store
            .version_dir(Fixture::MODULE_ID, Fixture::VERSION)
            .join("bin/demo")
            .is_file()
    );

    // Permissions are in force.
    assert_eq!(fixture.effective_permissions().len(), 2);

    // The journal recorded the operation.
    let journal = fixture.run(&["journal"]);
    assert!(journal.stdout().contains("install_module"));
    assert!(journal.stdout().contains(Fixture::MODULE_ID));

    // The store is consistent: no leftovers, no orphans.
    let status = fixture.run(&["store", "status"]);
    assert!(status.stdout().contains("store is consistent"));

    // Removal revokes everything.
    let remove = fixture.run(&["module", "remove", Fixture::MODULE_ID]);
    assert!(remove.success(), "remove failed: {}", remove.stderr());
    assert!(!fixture.store().is_installed(Fixture::MODULE_ID));
    assert!(fixture.effective_permissions().is_empty());
}

#[test]
fn upgrading_replaces_the_current_version() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let upgrade = fixture.build_bundle("1.1.0");
    let result = fixture.run(&["module", "install", upgrade.to_str().unwrap(), "--yes"]);

    assert!(result.success(), "upgrade failed: {}", result.stderr());
    assert!(result.stdout().contains("upgraded from 1.0.0 to 1.1.0"));
    assert_eq!(
        fixture
            .store()
            .installed_version(Fixture::MODULE_ID)
            .as_deref(),
        Some("1.1.0")
    );
    assert!(fixture.store().orphaned_versions().unwrap().is_empty());
}

#[test]
fn a_tampered_artifact_is_refused_and_nothing_is_installed() {
    let fixture = Fixture::new();
    let tampered = fixture.tamper_with_artifact();

    let result = fixture.run(&["module", "install", tampered.to_str().unwrap(), "--yes"]);

    assert!(!result.success());
    assert!(
        result.stderr().contains("digest mismatch"),
        "expected a digest mismatch, got: {}",
        result.stderr()
    );
    assert!(!fixture.store().is_installed(Fixture::MODULE_ID));
    assert!(fixture.effective_permissions().is_empty());

    // The failed attempt is recorded, not erased.
    assert!(
        fixture
            .run(&["journal"])
            .stdout()
            .contains("digest mismatch")
    );
}

#[test]
fn a_manifest_signed_by_the_wrong_key_is_refused() {
    let fixture = Fixture::new();
    let forged = fixture.bundle_signed_by_a_different_key();

    let result = fixture.run(&["module", "install", forged.to_str().unwrap(), "--yes"]);

    assert!(!result.success());
    assert!(
        result.stderr().contains("signature"),
        "expected a signature rejection, got: {}",
        result.stderr()
    );
    assert!(!fixture.store().is_installed(Fixture::MODULE_ID));
}

#[test]
fn a_changed_publisher_key_for_a_known_module_is_refused() {
    // Adversary 3 of the threat model: publisher impersonation. Once an id has
    // been seen, a different key for it is a hard error, never a warning.
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let impostor = fixture.bundle_from_a_new_publisher("2.0.0");
    let result = fixture.run(&["module", "install", impostor.to_str().unwrap(), "--yes"]);

    assert!(!result.success());
    assert!(
        result
            .stderr()
            .contains("changed since it was first trusted"),
        "expected key pinning to refuse, got: {}",
        result.stderr()
    );

    // The originally installed version is untouched.
    assert_eq!(
        fixture
            .store()
            .installed_version(Fixture::MODULE_ID)
            .as_deref(),
        Some(Fixture::VERSION)
    );
}

#[test]
fn declining_the_capability_prompt_installs_nothing() {
    // No terminal is attached, so the confirmer refuses: silence is not consent.
    let fixture = Fixture::new();
    let result = fixture.run(&["module", "install", fixture.bundle().to_str().unwrap()]);

    assert!(!result.success());
    assert!(!fixture.store().is_installed(Fixture::MODULE_ID));
    assert!(fixture.effective_permissions().is_empty());
    assert!(
        fixture
            .run(&["journal"])
            .stdout()
            .contains("did not confirm")
    );
}

#[test]
fn reinstalling_the_same_version_is_refused() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let again = fixture.install();
    assert!(!again.success());
    assert!(again.stderr().contains("already installed"));
}

#[test]
fn removing_something_that_is_not_installed_is_an_error() {
    let fixture = Fixture::new();
    let result = fixture.run(&["module", "remove", "org.thalyx.absent"]);
    assert!(!result.success());
    assert!(result.stderr().contains("not installed"));
}

#[test]
fn inspect_reports_a_bundle_without_installing_it() {
    let fixture = Fixture::new();
    let result = fixture.run(&["dev", "inspect", fixture.bundle().to_str().unwrap()]);

    assert!(result.success());
    assert!(result.stdout().contains("signature    valid"));
    assert!(result.stdout().contains("digest matches"));
    assert!(!fixture.store().is_installed(Fixture::MODULE_ID));
}

#[test]
fn permissions_are_never_reported_as_in_force_for_an_uninstalled_module() {
    // Found by running the real thing, not by a test: after a crash between
    // the two renames, the permission records exist but the module is not
    // installed. The core gated them correctly; the CLI was reading the raw
    // registry and showing the human two live persistent grants that were not
    // real. Whatever a human is shown has to match what actually holds.
    use thalyx_core::fault::FaultPoint;

    let fixture = Fixture::new();
    fixture.install_with_fault(FaultPoint::MidCommit);

    assert!(!fixture.store().is_installed(Fixture::MODULE_ID));
    assert!(fixture.recorded_permissions() > 0, "records should exist");

    let output = fixture.run(&["permissions"]).stdout();
    assert!(
        output.contains("no permissions in force"),
        "expected nothing in force, got: {output}"
    );
    assert!(
        output.contains("inert"),
        "the inert records should be disclosed, not hidden: {output}"
    );
    assert!(
        !output.contains("outbound net (persistent)"),
        "an uninstalled module must not be shown as holding a grant: {output}"
    );

    // Cleaning clears the records too.
    assert!(fixture.run(&["store", "clean"]).success());
    assert_eq!(fixture.recorded_permissions(), 0);
}

#[test]
fn the_journal_states_its_own_scope() {
    // A journal that does not declare what it covers invites building
    // destructive operations on the assumption that it saw everything.
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let journal = fixture.run(&["journal"]);
    assert!(
        journal
            .stdout()
            .contains("not a complete record of what happened to the system")
    );
}
