//! Taking snapshots, and writing them down.
//!
//! The mechanics are `thalyx-snapshot`'s. What lives here is the part the
//! decree puts in the core: **the journal is written only by the core**, so an
//! operation that leaves something on disk is recorded by the same layer that
//! records installs and rollbacks, in the same file, with the same scope.
//!
//! `vault/04-Flujo-Canonico/Journal-y-Snapshots.md` is explicit that the
//! journal covers Thalyx's own operations and nothing else. A snapshot is one
//! of those. What the human did to the subvolume between two snapshots is not,
//! and nothing here may pretend otherwise.

use crate::store::Store;
use crate::{CoreError, Result};
use thalyx_journal::{Entry, Journal, Origin, Outcome};
use thalyx_snapshot::{Snapshot, Snapshots, Volumes, name_for};

/// Take a snapshot of a subvolume and record it.
///
/// The journal entry carries the snapshot's name in the field that exists for
/// it, so a later `restore` can be traced back to the moment it returns to.
pub fn take<V: Volumes>(
    store: &Store,
    snapshots: &Snapshots<V>,
    label: &str,
    request_id: &str,
) -> Result<Snapshot> {
    let journal = Journal::open(store.journal_path())?;
    let name = name_for(label, &thalyx_journal::now());

    let entry = |outcome: Outcome, snapshot: Option<String>| Entry {
        timestamp: thalyx_journal::now(),
        operation: "snapshot".to_string(),
        module_id: None,
        version: None,
        outcome,
        request_id: request_id.to_string(),
        origin: Origin::UserUtterance,
        snapshot,
        notes: vec![format!("of {}", snapshots.subvolume().display())],
    };

    match snapshots.take(&name) {
        Ok(snapshot) => {
            journal.append(&entry(Outcome::Success, Some(snapshot.name.clone())))?;
            Ok(snapshot)
        }
        Err(error) => {
            // Recorded as rejected rather than swallowed. A snapshot that was
            // asked for and did not happen is exactly what somebody will be
            // looking for after a restore turns out to have nothing to
            // restore to.
            let _ = journal.append(&entry(
                Outcome::Rejected {
                    reason: error.to_string(),
                },
                None,
            ));
            Err(CoreError::Snapshot(error))
        }
    }
}

/// Delete a snapshot and record that it is gone.
pub fn forget<V: Volumes>(
    store: &Store,
    snapshots: &Snapshots<V>,
    name: &str,
    request_id: &str,
) -> Result<()> {
    let journal = Journal::open(store.journal_path())?;
    let outcome = snapshots.forget(name);

    journal.append(&Entry {
        timestamp: thalyx_journal::now(),
        operation: "forget_snapshot".to_string(),
        module_id: None,
        version: None,
        outcome: match &outcome {
            Ok(()) => Outcome::Success,
            Err(error) => Outcome::Rejected {
                reason: error.to_string(),
            },
        },
        request_id: request_id.to_string(),
        origin: Origin::UserUtterance,
        snapshot: Some(name.to_string()),
        // The moment is gone and nothing can bring it back, so the record that
        // it was deliberate is the only thing left of it.
        notes: vec!["the moment it held cannot be recovered".to_string()],
    })?;

    outcome.map_err(CoreError::Snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_snapshot::directories::Directories;

    fn setup() -> (tempfile::TempDir, Store, Snapshots<Directories>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("store")).unwrap();
        let subvolume = dir.path().join("work");
        Directories::make_subvolume(&subvolume).unwrap();
        std::fs::write(subvolume.join("a.txt"), "one\n").unwrap();
        let snapshots = Snapshots::of(Directories, subvolume);
        (dir, store, snapshots)
    }

    fn entries(store: &Store) -> Vec<Entry> {
        Journal::open(store.journal_path())
            .unwrap()
            .entries()
            .unwrap()
    }

    #[test]
    fn a_snapshot_is_recorded_with_the_name_it_got() {
        // Without the name in the journal, a restore can be traced to a moment
        // nobody can identify afterwards.
        let (_dir, store, snapshots) = setup();
        let taken = take(&store, &snapshots, "before-upgrade", "req-1").unwrap();

        let last = entries(&store).pop().unwrap();
        assert_eq!(last.operation, "snapshot");
        assert_eq!(last.outcome, Outcome::Success);
        assert_eq!(last.snapshot.as_deref(), Some(taken.name.as_str()));
    }

    #[test]
    fn a_snapshot_that_did_not_happen_is_recorded_as_refused() {
        // The entry somebody will be looking for after a restore turns out to
        // have nothing to restore to.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("store")).unwrap();
        let plain = dir.path().join("not-a-subvolume");
        std::fs::create_dir_all(&plain).unwrap();
        let snapshots = Snapshots::of(Directories, plain);

        assert!(take(&store, &snapshots, "doomed", "req-1").is_err());

        let last = entries(&store).pop().unwrap();
        assert_eq!(last.operation, "snapshot");
        assert!(matches!(last.outcome, Outcome::Rejected { .. }));
        assert_eq!(last.snapshot, None);
    }

    #[test]
    fn forgetting_one_is_recorded_too() {
        let (_dir, store, snapshots) = setup();
        let taken = take(&store, &snapshots, "temporary", "req-1").unwrap();
        forget(&store, &snapshots, &taken.name, "req-2").unwrap();

        let last = entries(&store).pop().unwrap();
        assert_eq!(last.operation, "forget_snapshot");
        assert_eq!(last.outcome, Outcome::Success);
        assert_eq!(last.snapshot.as_deref(), Some(taken.name.as_str()));
    }

    #[test]
    fn two_snapshots_of_the_same_subvolume_do_not_collide() {
        // The name carries the moment, so taking one twice in a row is an
        // ordinary thing to do rather than an error the human has to work
        // around by inventing labels.
        let (_dir, store, snapshots) = setup();
        let first = take(&store, &snapshots, "checkpoint", "req-1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = take(&store, &snapshots, "checkpoint", "req-2").unwrap();

        assert_ne!(first.name, second.name);
        assert_eq!(snapshots.list().unwrap().len(), 2);
    }
}
