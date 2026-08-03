//! `rollback` — undoing a commit Thalyx made.
//!
//! `vault/04-Flujo-Canonico/Rollback-vs-Restore.md` decrees two operations
//! under two names, and forbids one clever command that decides between them:
//!
//! - **`rollback`** undoes something Thalyx published. Narrow, cheap,
//!   guaranteed by the commit architecture, and unable to destroy the human's
//!   work — so it needs no extra confirmation.
//! - **`restore`** returns a Btrfs subvolume to a snapshot. Wide, destructive,
//!   and gated behind the drift check and the trusted path.
//!
//! This is the first one. It touches nothing the human made, only what Thalyx
//! itself put on disk, and that boundary is what makes it safe to run without
//! asking. Nothing here may ever grow the ability to remove a file Thalyx did
//! not publish; the moment it could, it would need `restore`'s confirmation
//! and would be the confusion the decree exists to prevent.
//!
//! ## Why so much of this is refusing
//!
//! Build-then-commit already means most failures leave nothing to undo. What
//! remains is the case where the commit *succeeded* and is now being reversed,
//! and there the risk is not the removal — it is reversing an entry that no
//! longer describes the world. If the module was upgraded after the entry, the
//! version on disk is not the one the entry published, and undoing "the
//! install" would silently delete a later one the human still wants.
//!
//! So the plan is worked out against the disk, not against the journal alone.
//! The journal says what Thalyx did; only the store says what is there now,
//! and the journal is explicitly not a complete record of what happened to the
//! system. Every disagreement between the two is a refusal with the reason
//! named, never a best guess.

use crate::store::Store;
use crate::{CoreError, Result, commit};
use thalyx_journal::{Entry, Journal, Origin, Outcome};

/// The one operation that publishes something, and so the one with anything
/// to undo.
const REVERSIBLE: &str = "install_module";

/// What rolling back would do, worked out before anything moves.
///
/// Produced separately from applying it so the human can be told what is about
/// to happen in the same terms it will happen in. The decree says rollback
/// needs no confirmation; it does not say it may be silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The request being undone, not the request doing the undoing.
    pub request_id: String,
    pub operation: String,
    pub module_id: String,
    pub version: String,
    /// Permissions that stop being effective. Revoked with the module, because
    /// a permission outliving what it was granted to is the orphaned grant
    /// `Permisos-JIT` forbids.
    pub permissions_revoked: usize,
    /// The user id retired. Never handed to another module afterwards.
    pub uid_retired: Option<u32>,
}

impl Plan {
    pub fn describe(&self) -> String {
        format!(
            "undo {} of {} {}",
            self.operation, self.module_id, self.version
        )
    }
}

/// Work out what rolling back would undo.
///
/// With `request` given, that exact commit. Without it, the most recent commit
/// that can still be undone — which is not the same as the most recent entry,
/// because rejected attempts and removals are in the journal too and none of
/// them left anything behind.
pub fn plan(store: &Store, request: Option<&str>) -> Result<Plan> {
    let journal = Journal::open(store.journal_path())?;
    let entries = journal.entries()?;

    let entry = match request {
        Some(id) => entries
            .iter()
            .rev()
            .find(|entry| entry.request_id == id)
            .ok_or_else(|| CoreError::NoSuchRequest {
                request_id: id.to_string(),
            })?,
        None => entries
            .iter()
            .rev()
            .find(|entry| is_reversible(entry))
            .ok_or(CoreError::NothingToRollBack)?,
    };

    // Asked for by name, an entry that cannot be undone has to say which of
    // the several reasons applies. Silently walking back to an older entry
    // would undo something the human did not name.
    if !is_reversible(entry) {
        return Err(refusal(entry));
    }

    let module_id = entry
        .module_id
        .clone()
        .ok_or_else(|| CoreError::NotReversible {
            operation: entry.operation.clone(),
            reason: "the entry names no module".to_string(),
        })?;
    let version = entry
        .version
        .clone()
        .ok_or_else(|| CoreError::NotReversible {
            operation: entry.operation.clone(),
            reason: "the entry names no version".to_string(),
        })?;

    // Against the disk, not against the journal. The journal records only what
    // Thalyx did, and the human is free to have done something else.
    let installed =
        store
            .installed_version(&module_id)
            .ok_or_else(|| CoreError::AlreadyUndone {
                module_id: module_id.clone(),
                version: version.clone(),
            })?;

    if installed != version {
        return Err(CoreError::RollbackSuperseded {
            module_id,
            published: version,
            installed,
        });
    }

    let registry = crate::permissions::Registry::load(store.permissions_path())?;
    let uids = crate::uids::UidRegistry::load(store.uids_path())?;

    Ok(Plan {
        request_id: entry.request_id.clone(),
        operation: entry.operation.clone(),
        permissions_revoked: registry.effective(&module_id).len(),
        uid_retired: uids.assigned(&module_id),
        module_id,
        version,
    })
}

/// Carry out a plan.
///
/// The same three things `remove` does, in the same order and for the same
/// reasons — unpublish, revoke, retire — because leaving a module's
/// permissions or its user id behind is the same defect whichever command
/// removed it. What differs is the journal entry: an operation undone and one
/// that was never wanted read differently to anyone auditing later.
pub fn apply(store: &Store, plan: &Plan, request_id: &str) -> Result<()> {
    let journal = Journal::open(store.journal_path())?;

    // Re-checked here, not trusted from the plan. Anything could have happened
    // between working it out and applying it, and the cost of being wrong is
    // deleting a version somebody installed in between.
    match store.installed_version(&plan.module_id) {
        Some(installed) if installed == plan.version => {}
        Some(installed) => {
            return Err(CoreError::RollbackSuperseded {
                module_id: plan.module_id.clone(),
                published: plan.version.clone(),
                installed,
            });
        }
        None => {
            return Err(CoreError::AlreadyUndone {
                module_id: plan.module_id.clone(),
                version: plan.version.clone(),
            });
        }
    }

    let outcome = commit::unpublish(store, &plan.module_id);

    let entry = |outcome: Outcome, notes: Vec<String>| Entry {
        timestamp: thalyx_journal::now(),
        operation: "rollback".to_string(),
        module_id: Some(plan.module_id.clone()),
        version: Some(plan.version.clone()),
        outcome,
        request_id: request_id.to_string(),
        origin: Origin::UserUtterance,
        snapshot: None,
        notes,
    };

    match outcome {
        Ok(_) => {
            let mut registry = crate::permissions::Registry::load(store.permissions_path())?;
            registry.revoke_all(&plan.module_id)?;

            let mut uids = crate::uids::UidRegistry::load(store.uids_path())?;
            uids.retire(&plan.module_id)?;

            journal.append(&entry(
                Outcome::Success,
                vec![
                    format!("undid request {}", plan.request_id),
                    "all permissions revoked".to_string(),
                ],
            ))?;
            Ok(())
        }
        Err(error) => {
            let _ = journal.append(&entry(
                Outcome::Rejected {
                    reason: error.to_string(),
                },
                vec![format!("tried to undo request {}", plan.request_id)],
            ));
            Err(error)
        }
    }
}

/// Whether an entry left something on disk that can be taken back.
fn is_reversible(entry: &Entry) -> bool {
    entry.operation == REVERSIBLE && matches!(entry.outcome, Outcome::Success)
}

/// Why a named entry cannot be undone.
///
/// Every branch is a different thing to tell the human, and collapsing them
/// into "cannot roll back that" would hide the one case that is good news:
/// build-then-commit meaning there was never anything to undo.
fn refusal(entry: &Entry) -> CoreError {
    let reason = match (&entry.outcome, entry.operation.as_str()) {
        (Outcome::Intended, _) => {
            "it was never resolved; `thalyx store status` reconciles unfinished intents"
        }
        (Outcome::Rejected { .. }, _) => "it was refused, so nothing was published",
        (Outcome::NotCommitted { .. }, _) => {
            "the artifact was built but never published — which is build-then-commit \
             working, and leaves nothing to undo"
        }
        (_, "remove_module") => {
            "a removal cannot be undone: the version's files are gone, and rollback \
             only takes back what Thalyx put on disk. Reinstall the module instead"
        }
        (_, "rollback") => "undoing a rollback is a reinstall, not a rollback",
        _ => {
            "only an installation publishes anything, so only an installation \
              has something to take back"
        }
    };

    CoreError::NotReversible {
        operation: entry.operation.clone(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_journal::Origin;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    fn record(store: &Store, operation: &str, module: &str, version: &str, outcome: Outcome) {
        let journal = Journal::open(store.journal_path()).unwrap();
        journal
            .append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: operation.to_string(),
                module_id: Some(module.to_string()),
                version: Some(version.to_string()),
                outcome,
                request_id: format!("req-{operation}-{module}-{version}"),
                origin: Origin::UserUtterance,
                snapshot: None,
                notes: vec![],
            })
            .unwrap();
    }

    /// Put a version on disk the way a commit would leave it.
    fn publish(store: &Store, module: &str, version: &str) {
        let staging = store.new_staging_dir().unwrap();
        std::fs::write(staging.join("entrypoint"), "#!/bin/sh\n").unwrap();
        commit::publish(store, &staging, module, version).unwrap();
    }

    #[test]
    fn an_empty_journal_has_nothing_to_undo() {
        let (_dir, store) = store();
        assert!(matches!(
            plan(&store, None),
            Err(CoreError::NothingToRollBack)
        ));
    }

    #[test]
    fn a_committed_install_is_what_gets_undone() {
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        let plan = plan(&store, None).unwrap();
        assert_eq!(plan.module_id, "org.example.tool");
        assert_eq!(plan.version, "1.0.0");
    }

    #[test]
    fn rejected_and_uncommitted_attempts_are_skipped_not_undone() {
        // Build-then-commit means these left nothing behind. Rolling one back
        // would look for a version that was never published — and picking the
        // most recent *entry* rather than the most recent *commit* is exactly
        // how that mistake gets made.
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );
        record(
            &store,
            "install_module",
            "org.example.other",
            "2.0.0",
            Outcome::Rejected {
                reason: "signature".to_string(),
            },
        );
        record(
            &store,
            "install_module",
            "org.example.third",
            "3.0.0",
            Outcome::NotCommitted {
                reason: "digest".to_string(),
            },
        );

        let plan = plan(&store, None).unwrap();
        assert_eq!(plan.module_id, "org.example.tool");
    }

    #[test]
    fn a_removal_names_why_it_cannot_be_undone() {
        // The bytes are gone. Saying so beats a generic refusal, because the
        // thing the human wants next is "reinstall it", and nothing else here
        // would tell them that.
        let (_dir, store) = store();
        record(
            &store,
            "remove_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        match plan(&store, Some("req-remove_module-org.example.tool-1.0.0")) {
            Err(CoreError::NotReversible { reason, .. }) => {
                assert!(reason.contains("Reinstall"), "{reason}");
            }
            other => panic!("expected a refusal naming the reason, got {other:?}"),
        }
    }

    #[test]
    fn naming_a_rejected_entry_refuses_instead_of_undoing_an_older_one() {
        // The dangerous shortcut: falling back to the last reversible entry
        // when the named one is not. The human named a request; undoing a
        // different one is not a smaller version of what they asked for.
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );
        record(
            &store,
            "install_module",
            "org.example.other",
            "2.0.0",
            Outcome::Rejected {
                reason: "signature".to_string(),
            },
        );

        assert!(matches!(
            plan(&store, Some("req-install_module-org.example.other-2.0.0")),
            Err(CoreError::NotReversible { .. })
        ));
        assert!(store.is_installed("org.example.tool"));
    }

    #[test]
    fn an_unknown_request_is_refused_rather_than_resolved_to_the_latest() {
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        assert!(matches!(
            plan(&store, Some("a-request-that-never-existed")),
            Err(CoreError::NoSuchRequest { .. })
        ));
    }

    #[test]
    fn a_module_no_longer_installed_reports_that_rather_than_failing_obscurely() {
        let (_dir, store) = store();
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        assert!(matches!(
            plan(&store, None),
            Err(CoreError::AlreadyUndone { .. })
        ));
    }

    #[test]
    fn a_version_installed_after_the_entry_is_never_deleted_by_undoing_it() {
        // The failure this whole module is shaped around. The journal says
        // Thalyx installed 1.0.0; the disk says 2.0.0 is what is there now.
        // Undoing "the install" would delete a version the human still wants,
        // and the journal alone cannot tell the difference.
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "2.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        match plan(&store, None) {
            Err(CoreError::RollbackSuperseded {
                published,
                installed,
                ..
            }) => {
                assert_eq!(published, "1.0.0");
                assert_eq!(installed, "2.0.0");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert!(store.is_installed("org.example.tool"));
    }

    #[test]
    fn applying_a_plan_takes_the_module_back_off_disk() {
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        let plan = plan(&store, None).unwrap();
        apply(&store, &plan, "req-rollback").unwrap();

        assert!(!store.is_installed("org.example.tool"));
    }

    #[test]
    fn a_rollback_is_recorded_as_one_and_names_what_it_undid() {
        // An audit that cannot tell "removed because unwanted" from "undone
        // because it should not have happened" has lost the distinction the
        // two commands exist to make.
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        let plan = plan(&store, None).unwrap();
        let undone = plan.request_id.clone();
        apply(&store, &plan, "req-rollback").unwrap();

        let entries = Journal::open(store.journal_path())
            .unwrap()
            .entries()
            .unwrap();
        let last = entries.last().unwrap();

        assert_eq!(last.operation, "rollback");
        assert_eq!(last.outcome, Outcome::Success);
        assert!(
            last.notes.iter().any(|note| note.contains(&undone)),
            "the entry does not say which request it undid: {:?}",
            last.notes
        );
    }

    #[test]
    fn rolling_back_twice_refuses_the_second_time() {
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        let first = plan(&store, None).unwrap();
        apply(&store, &first, "req-rollback-1").unwrap();

        // The stale plan is deliberately reused: a caller that worked one out
        // and then lost a race must not be able to delete something with it.
        assert!(matches!(
            apply(&store, &first, "req-rollback-2"),
            Err(CoreError::AlreadyUndone { .. })
        ));
    }

    #[test]
    fn a_plan_applied_after_an_upgrade_refuses_rather_than_deleting_the_upgrade() {
        // The same race as above, in the direction that loses data: the plan
        // was worked out for 1.0.0 and by the time it runs the disk holds
        // 2.0.0. Trusting the plan here would delete the newer one.
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        let stale = plan(&store, None).unwrap();

        commit::unpublish(&store, "org.example.tool").unwrap();
        publish(&store, "org.example.tool", "2.0.0");

        assert!(matches!(
            apply(&store, &stale, "req-rollback"),
            Err(CoreError::RollbackSuperseded { .. })
        ));
        assert_eq!(
            store.installed_version("org.example.tool").as_deref(),
            Some("2.0.0")
        );
    }

    #[test]
    fn undoing_an_install_revokes_what_it_was_granted() {
        // A permission outliving the module it was granted to is the orphaned
        // grant Permisos-JIT forbids, and it does not stop being one because
        // the module left by this door rather than the other.
        use thalyx_manifest::{Permission, PermissionKind};

        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        let mut registry = crate::permissions::Registry::load(store.permissions_path()).unwrap();
        registry
            .make_effective(&crate::permissions::PendingGrants::new(
                "org.example.tool",
                "req-install",
                vec![Permission {
                    resource: "network:example.org:443".to_string(),
                    action: "connect".to_string(),
                    kind: PermissionKind::Persistent,
                }],
            ))
            .unwrap();

        let plan = plan(&store, None).unwrap();
        assert_eq!(plan.permissions_revoked, 1);

        apply(&store, &plan, "req-rollback").unwrap();

        let registry = crate::permissions::Registry::load(store.permissions_path()).unwrap();
        assert!(registry.effective("org.example.tool").is_empty());
    }

    #[test]
    fn undoing_an_install_retires_the_user_id_rather_than_freeing_it() {
        // The uid may own files in places Thalyx does not track. Handing the
        // number to a different module later would give it all of them.
        let (_dir, store) = store();
        publish(&store, "org.example.tool", "1.0.0");
        record(
            &store,
            "install_module",
            "org.example.tool",
            "1.0.0",
            Outcome::Success,
        );

        let mut uids = crate::uids::UidRegistry::load(store.uids_path()).unwrap();
        let assigned = uids.assign("org.example.tool").unwrap();

        let plan = plan(&store, None).unwrap();
        assert_eq!(plan.uid_retired, Some(assigned));

        apply(&store, &plan, "req-rollback").unwrap();

        let mut uids = crate::uids::UidRegistry::load(store.uids_path()).unwrap();
        assert_eq!(uids.assigned("org.example.tool"), None);
        assert_ne!(
            uids.assign("org.example.other").unwrap(),
            assigned,
            "a retired uid was handed to another module"
        );
    }
}
