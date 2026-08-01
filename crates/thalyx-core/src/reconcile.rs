//! Reconciling unresolved intents against the disk.
//!
//! The journal is written around the commit, not inside it: an intent goes down
//! before anything moves, and a terminal entry after. A process that dies in
//! between leaves an intent with no outcome.
//!
//! That is not a lost operation — it is a **question**, and the disk has the
//! answer. Reconciliation asks it: is the version the intent named the current
//! one? If yes, the commit happened and the entry that was never written is
//! written now. If no, there was no commit.
//!
//! This is what closes the gap where a crash right after the symlink swap left
//! a module installed with no record that it ever happened.
//!
//! See `vault/04-Flujo-Canonico/Fase-Commit-Atomico.md`.

use crate::store::Store;
use crate::{CoreError, Result};
use thalyx_journal::{Entry, Journal, Outcome};

/// What reconciliation concluded about one unresolved intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub request_id: String,
    pub module_id: String,
    pub version: String,
    /// `true` when the disk shows the commit did happen.
    pub committed: bool,
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.committed {
            write!(
                f,
                "{} {} — the commit had happened; recorded as successful",
                self.module_id, self.version
            )
        } else {
            write!(
                f,
                "{} {} — no commit had happened; recorded as not committed",
                self.module_id, self.version
            )
        }
    }
}

/// Settle every unresolved intent against the current state of the disk.
///
/// Idempotent: running it twice settles nothing the second time, because the
/// first run wrote the terminal entries.
pub fn reconcile(store: &Store) -> Result<Vec<Resolution>> {
    let journal = Journal::open(store.journal_path())?;
    let pending = journal.unresolved_intents()?;
    let mut resolutions = Vec::new();

    for intent in pending {
        let (Some(module_id), Some(version)) = (intent.module_id.clone(), intent.version.clone())
        else {
            // An intent without a subject cannot be checked against anything.
            // Settle it so it stops being reported, and say why.
            journal.append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: intent.operation.clone(),
                module_id: intent.module_id.clone(),
                version: intent.version.clone(),
                outcome: Outcome::NotCommitted {
                    reason: "unresolved intent carried no module to check".to_string(),
                },
                request_id: intent.request_id.clone(),
                origin: intent.origin,
                snapshot: None,
                notes: vec!["settled by reconciliation".to_string()],
            })?;
            continue;
        };

        // The disk is the authority, not the journal.
        let committed = store.installed_version(&module_id).as_deref() == Some(version.as_str());

        journal.append(&Entry {
            timestamp: thalyx_journal::now(),
            operation: intent.operation.clone(),
            module_id: Some(module_id.clone()),
            version: Some(version.clone()),
            outcome: if committed {
                Outcome::Success
            } else {
                Outcome::NotCommitted {
                    reason: "interrupted before the commit completed".to_string(),
                }
            },
            request_id: intent.request_id.clone(),
            origin: intent.origin,
            snapshot: None,
            notes: vec![format!(
                "settled by reconciliation; the operation was interrupted at {}",
                intent.timestamp
            )],
        })?;

        resolutions.push(Resolution {
            request_id: intent.request_id,
            module_id,
            version,
            committed,
        });
    }

    Ok(resolutions)
}

/// Record the intent to publish, before anything moves.
pub(crate) fn record_intent(
    journal: &Journal,
    operation: &str,
    module_id: &str,
    version: &str,
    request_id: &str,
    origin: thalyx_journal::Origin,
) -> Result<()> {
    journal
        .append(&Entry {
            timestamp: thalyx_journal::now(),
            operation: operation.to_string(),
            module_id: Some(module_id.to_string()),
            version: Some(version.to_string()),
            outcome: Outcome::Intended,
            request_id: request_id.to_string(),
            origin,
            snapshot: None,
            notes: vec![],
        })
        .map_err(CoreError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_journal::Origin;

    fn journal_of(store: &Store) -> Journal {
        Journal::open(store.journal_path()).unwrap()
    }

    fn stage_and_publish(store: &Store, id: &str, version: &str) {
        let dir = store.new_staging_dir().unwrap();
        std::fs::write(dir.join("payload"), "x").unwrap();
        crate::commit::publish(store, &dir, id, version).unwrap();
    }

    #[test]
    fn an_intent_for_a_commit_that_happened_becomes_a_success() {
        // The exact gap this closes: the symlink swung, then the process died
        // before the journal entry was written.
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();
        let journal = journal_of(&store);

        record_intent(
            &journal,
            "install_module",
            "org.demo.thing",
            "1.0.0",
            "req-1",
            Origin::UserUtterance,
        )
        .unwrap();
        stage_and_publish(&store, "org.demo.thing", "1.0.0");

        let resolutions = reconcile(&store).unwrap();

        assert_eq!(resolutions.len(), 1);
        assert!(resolutions[0].committed);
        assert!(journal.unresolved_intents().unwrap().is_empty());
        assert!(
            journal
                .entries()
                .unwrap()
                .iter()
                .any(|e| e.outcome == Outcome::Success)
        );
    }

    #[test]
    fn an_intent_for_a_commit_that_did_not_happen_becomes_not_committed() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();
        let journal = journal_of(&store);

        record_intent(
            &journal,
            "install_module",
            "org.demo.thing",
            "1.0.0",
            "req-1",
            Origin::UserUtterance,
        )
        .unwrap();

        let resolutions = reconcile(&store).unwrap();

        assert_eq!(resolutions.len(), 1);
        assert!(!resolutions[0].committed);
        assert!(journal.unresolved_intents().unwrap().is_empty());
    }

    #[test]
    fn an_intent_already_settled_is_not_reconciled_again() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();
        let journal = journal_of(&store);

        record_intent(
            &journal,
            "install_module",
            "org.demo.thing",
            "1.0.0",
            "req-1",
            Origin::UserUtterance,
        )
        .unwrap();
        journal
            .append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: "install_module".to_string(),
                module_id: Some("org.demo.thing".to_string()),
                version: Some("1.0.0".to_string()),
                outcome: Outcome::Success,
                request_id: "req-1".to_string(),
                origin: Origin::UserUtterance,
                snapshot: None,
                notes: vec![],
            })
            .unwrap();

        assert!(reconcile(&store).unwrap().is_empty());
    }

    #[test]
    fn reconciliation_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();
        let journal = journal_of(&store);

        record_intent(
            &journal,
            "install_module",
            "org.demo.thing",
            "1.0.0",
            "req-1",
            Origin::UserUtterance,
        )
        .unwrap();

        assert_eq!(reconcile(&store).unwrap().len(), 1);
        assert_eq!(reconcile(&store).unwrap().len(), 0);
        assert_eq!(reconcile(&store).unwrap().len(), 0);
    }

    #[test]
    fn intents_from_different_requests_are_settled_independently() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path()).unwrap();
        let journal = journal_of(&store);

        record_intent(
            &journal,
            "install_module",
            "org.demo.a",
            "1.0.0",
            "req-a",
            Origin::UserUtterance,
        )
        .unwrap();
        record_intent(
            &journal,
            "install_module",
            "org.demo.b",
            "2.0.0",
            "req-b",
            Origin::UserUtterance,
        )
        .unwrap();
        stage_and_publish(&store, "org.demo.a", "1.0.0");

        let mut resolutions = reconcile(&store).unwrap();
        resolutions.sort_by(|a, b| a.module_id.cmp(&b.module_id));

        assert_eq!(resolutions.len(), 2);
        assert!(resolutions[0].committed, "a was published");
        assert!(!resolutions[1].committed, "b was not");
    }
}
