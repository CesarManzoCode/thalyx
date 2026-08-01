//! A hostile contract is refused before it causes any work.
//!
//! The provenance check runs first, ahead of opening the bundle. These tests
//! prove it by pointing the request at a path that does not exist: if the
//! contract were examined after the bundle, the failure would be an I/O error
//! instead of a provenance rejection.
//!
//! Ordering is a security property here, not an optimisation. Anything the
//! core does before deciding a contract is admissible is work an attacker got
//! for free.

use std::path::Path;
use thalyx_contract::{Caller, Contract, Operation, Origin, Origins};
use thalyx_core::install::AllowAll;
use thalyx_core::{CoreError, InstallRequest, Store};

fn contract_with(origins: Origins) -> Contract {
    Contract {
        version: "1.0".to_string(),
        operation: Operation::InstallModule,
        targets: vec!["org.publisher.pyassist".to_string()],
        constraint: Some("^2.3".to_string()),
        permissions: Vec::new(),
        requires_confirmation: true,
        sandbox_profile: Some("module_standard".to_string()),
        rollback: Default::default(),
        caller: Caller {
            module_id: "thalyx-agent".to_string(),
            request_id: "req-hostile".to_string(),
        },
        origins,
    }
}

fn trusted_origins() -> Origins {
    let mut origins = Origins::new();
    origins
        .set("operation", Origin::UserUtterance)
        .set("targets", Origin::UserUtterance)
        .set("constraint", Origin::SystemState)
        .set("permissions", Origin::SystemState);
    origins
}

/// The temp directory is returned rather than leaked: holding it keeps the
/// store alive for the caller's assertions, and dropping it cleans up.
fn attempt(
    contract: Contract,
) -> (
    tempfile::TempDir,
    Store,
    thalyx_core::Result<thalyx_core::InstallOutcome>,
) {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = Store::open(directory.path().join("store")).expect("store");

    // A path that certainly does not exist. Reaching it at all means the
    // contract was accepted, which is the failure this test looks for.
    let result = thalyx_core::install(
        &store,
        InstallRequest {
            bundle_path: Path::new("/nonexistent/never-opened.thmod"),
            contract,
        },
        &mut AllowAll,
    );

    (directory, store, result)
}

#[test]
fn a_target_from_untrusted_content_is_refused_before_the_bundle_is_opened() {
    // The attack in full: a repository description talks the agent into naming
    // a different module. The contract is well-formed and passes every
    // syntactic check. Only its provenance gives it away.
    let mut origins = trusted_origins();
    origins.set("targets", Origin::UntrustedContent);

    let (_dir, _store, result) = attempt(contract_with(origins));

    match result {
        Err(CoreError::Contract(error)) => {
            let message = error.to_string();
            assert!(
                message.contains("untrusted content"),
                "expected a provenance rejection, got: {message}"
            );
        }
        Err(other) => panic!(
            "the contract should have been refused before any I/O, but the core got as far as: {other}"
        ),
        Ok(_) => panic!("a contract sourced from untrusted content was accepted"),
    }
}

#[test]
fn every_effectful_field_is_refused_from_untrusted_content() {
    for field in thalyx_contract::EFFECTFUL_FIELDS {
        let mut origins = trusted_origins();
        origins.set(field, Origin::UntrustedContent);

        let (_dir, _store, result) = attempt(contract_with(origins));

        assert!(
            matches!(result, Err(CoreError::Contract(_))),
            "`{field}` sourced from untrusted content must be refused, got {result:?}"
        );
    }
}

#[test]
fn a_missing_origin_is_refused_rather_than_assumed_trusted() {
    let mut origins = Origins::new();
    origins
        .set("operation", Origin::UserUtterance)
        .set("constraint", Origin::SystemState)
        .set("permissions", Origin::SystemState);
    // `targets` deliberately absent.

    let (_dir, _store, result) = attempt(contract_with(origins));

    match result {
        Err(CoreError::Contract(error)) => {
            assert!(
                error.to_string().contains("declares no origin"),
                "got: {error}"
            );
        }
        other => panic!("a field with no declared provenance must be refused, got {other:?}"),
    }
}

#[test]
fn a_contract_for_the_wrong_operation_is_refused() {
    let mut contract = contract_with(trusted_origins());
    contract.operation = Operation::DeleteFiles;

    let (_dir, _store, result) = attempt(contract);
    assert!(
        matches!(result, Err(CoreError::MalformedBundle(_))),
        "install must not execute a contract authorising something else, got {result:?}"
    );
}

#[test]
fn a_trusted_contract_gets_far_enough_to_fail_on_the_missing_bundle() {
    // The control. Without it, the tests above would pass even if the core
    // rejected every contract for some unrelated reason.
    let (_dir, _store, result) = attempt(contract_with(trusted_origins()));

    match result {
        Err(CoreError::Io { .. }) => {} // reached the bundle, as it should
        other => panic!(
            "a well-formed trusted contract should have been accepted and then failed on the absent bundle, got {other:?}"
        ),
    }
}

#[test]
fn the_journal_records_a_refused_contract() {
    // A rejection is still an event. With build-then-commit there is nothing
    // to undo, but an attempt that leaves no trace is an attempt nobody can
    // audit.
    let mut origins = trusted_origins();
    origins.set("targets", Origin::UntrustedContent);

    let (_dir, store, _result) = attempt(contract_with(origins));

    let journal = thalyx_journal::Journal::open(store.journal_path()).expect("journal");
    let entries = journal.entries().expect("entries");

    assert_eq!(entries.len(), 1, "the refusal should have been recorded");
    assert_eq!(entries[0].request_id, "req-hostile");
    assert_eq!(
        entries[0].origin,
        thalyx_journal::Origin::UntrustedContent,
        "the journal records the least trusted origin in the contract"
    );
}
