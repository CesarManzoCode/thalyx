//! `restore` — returning a subvolume to a moment, destroying what came after.
//!
//! `vault/04-Flujo-Canonico/Rollback-vs-Restore.md` is unambiguous about what
//! this is, and it is the reason it does not share a name with `rollback`:
//!
//! > Ámbito amplio: todo lo que haya en ese subvolumen. **Destructivo.** Puede
//! > eliminar trabajo del usuario posterior al snapshot.
//!
//! and it names two requirements. The state check from decree 4 of
//! `Coherencia-Doble-Ruta`: **if there are changes Thalyx did not originate, it
//! stops.** And explicit confirmation by the trusted path, showing the diff of
//! what will be lost.
//!
//! Both are here, and the first one needs saying carefully, because "it stops"
//! and "it shows you what you would lose and asks" sound like they contradict
//! each other. They do not. Drift is the normal case — the whole point of the
//! adoption demonstration is a human undoing *their own* work, which is drift
//! by definition. What the decree forbids is proceeding **without the human
//! having been told**. So it stops, shows exactly what it found, and goes no
//! further unless somebody who saw that says yes.
//!
//! ## The intent is on disk before anything moves
//!
//! The same discipline as installing. The swap itself is a single
//! `RENAME_EXCHANGE` where the filesystem supports one, so there is no half
//! state to recover from — but where it falls back to two renames there is a
//! window, and an unresolved intent naming both paths is the difference
//! between a recoverable interruption and a tree that is simply gone.

use crate::store::Store;
use crate::{CoreError, Result};
use thalyx_journal::{Entry, Journal, Origin, Outcome};
use thalyx_snapshot::{Difference, Restored, Snapshots, Volumes};

/// What a restore would do, worked out before anything is asked.
#[derive(Debug, Clone)]
pub struct Plan {
    pub snapshot: String,
    pub subvolume: std::path::PathBuf,
    /// What the live tree has that the snapshot does not, and the reverse.
    pub difference: Difference,
    /// Exactly which state of the live tree this plan was made against.
    ///
    /// Carried in the plan rather than taken separately, because a plan and a
    /// witness of two different instants are worse than no witness at all: they
    /// look like a checked pair and are not one. See
    /// `thalyx_snapshot::difference_and_witness`.
    pub state: thalyx_snapshot::Witness,
}

impl Plan {
    /// Whether anything would actually change.
    pub fn is_a_no_op(&self) -> bool {
        self.difference.is_empty()
    }

    /// The one sentence a human has to weigh.
    ///
    /// Files created since the snapshot are the ones with no older version to
    /// return to: a restore does not revert them, it deletes them. Everything
    /// else is recoverable from the tree this restore keeps aside, and that
    /// difference decides whether the answer should be yes.
    pub fn what_it_costs(&self) -> String {
        if self.is_a_no_op() {
            return "nothing differs from the snapshot; restoring would change nothing".to_string();
        }

        let mut parts = Vec::new();
        if self.difference.added_total > 0 {
            parts.push(format!(
                "{} file(s) created since then would be DELETED",
                self.difference.added_total
            ));
        }
        if self.difference.modified_total > 0 {
            parts.push(format!(
                "{} file(s) would go back to their older contents",
                self.difference.modified_total
            ));
        }
        if self.difference.removed_total > 0 {
            parts.push(format!(
                "{} file(s) deleted since then would come back",
                self.difference.removed_total
            ));
        }
        if !self.difference.unreadable.is_empty() {
            parts.push(format!(
                "{} path(s) could not be compared",
                self.difference.unreadable.len()
            ));
        }
        parts.join("\n")
    }
}

/// Work out what returning to a snapshot would cost.
///
/// Nothing is touched. This exists so the confirmation can be about the real
/// tree in front of the human rather than about restores in general.
pub fn plan<V: Volumes>(snapshots: &Snapshots<V>, name: &str) -> Result<Plan> {
    let snapshot = snapshots.find(name)?;
    let (difference, state) =
        thalyx_snapshot::difference_and_witness(snapshots.subvolume(), &snapshot.path);
    Ok(Plan {
        snapshot: snapshot.name,
        subvolume: snapshots.subvolume().to_path_buf(),
        difference,
        state,
    })
}

/// Carry out a restore.
///
/// **The caller must already have asked.** This function does not prompt: the
/// trusted path belongs to the layer that can talk to a terminal, and a core
/// that prompted would be a core that could be made to prompt by something
/// other than a human.
pub fn apply<V: Volumes>(
    store: &Store,
    snapshots: &Snapshots<V>,
    plan: &Plan,
    request_id: &str,
) -> Result<Restored> {
    // The global lock. A restore replaces a whole subvolume; an install
    // committing into it halfway through would be published into a tree that
    // is about to be replaced, and would vanish with no record of why.
    let lock = store.lock()?;
    apply_holding_the_lock(store, snapshots, plan, request_id, &lock, &|| Ok(()))
}

/// The same restore, for a caller that already holds the global lock.
///
/// `flock` attaches to an open file description and is not reentrant: a caller
/// that took the lock and then called [`apply`] would open a second description
/// on the same file and wait for itself, forever. So the lock is a parameter
/// and the type is the proof — there is no way to reach this without one, and
/// no way to reach it while holding nothing.
///
/// [`crate::attempt::abandon`] is the caller that needs it. Abandoning is
/// "check the attempt on record is still the one I planned against, restore,
/// clear the record", and those three have to be one transition or two clients
/// abandoning at once restore the same snapshot twice.
/// The last question, asked with the swap already built and nothing else left
/// to do.
///
/// A rollback authorised by a state has to compare that state against the tree
/// at the instant of the destruction, and "instant" is only worth the word if
/// almost nothing can happen after it. Passing the question in rather than
/// asking it before the call is what lets the expensive half of a restore —
/// opening the journal, writing the intent, making the writable copy of the
/// snapshot — happen on the harmless side of it.
pub(crate) type LastLook<'a> = &'a dyn Fn() -> Result<()>;

pub(crate) fn apply_holding_the_lock<V: Volumes>(
    store: &Store,
    snapshots: &Snapshots<V>,
    plan: &Plan,
    request_id: &str,
    _lock: &crate::store::ContractLock,
    before_the_swap: LastLook<'_>,
) -> Result<Restored> {
    let journal = Journal::open(store.journal_path())?;

    let entry = |outcome: Outcome, notes: Vec<String>| Entry {
        timestamp: thalyx_journal::now(),
        operation: "restore".to_string(),
        module_id: None,
        version: None,
        outcome,
        request_id: request_id.to_string(),
        origin: Origin::UserUtterance,
        snapshot: Some(plan.snapshot.clone()),
        notes,
    };

    // Written before anything moves. An interruption then leaves an intent
    // naming the subvolume and the snapshot, which is recoverable; without it
    // an interrupted fallback path leaves a tree that is simply missing and
    // nothing that says where it went.
    journal.append(&entry(
        Outcome::Intended,
        vec![
            format!("returning {} to it", plan.subvolume.display()),
            format!(
                "{} file(s) created since then will be deleted",
                plan.difference.added_total
            ),
        ],
    ))?;

    // Built, and not committed. Nothing has moved yet: the writable copy sits
    // beside the subvolume under a name nothing points at.
    let prepared = match snapshots.prepare_restore(&plan.snapshot, &thalyx_journal::now()) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = journal.append(&entry(
                Outcome::Rejected {
                    reason: error.to_string(),
                },
                vec![format!("{} is unchanged", plan.subvolume.display())],
            ));
            return Err(CoreError::Snapshot(error));
        }
    };

    // The last look at the live tree, with the swap already built. What it
    // refuses, it refuses having destroyed nothing — the staging copy goes with
    // the `Prepared`.
    if let Err(refused) = before_the_swap() {
        let _ = journal.append(&entry(
            Outcome::Rejected {
                reason: refused.to_string(),
            },
            vec![format!("{} is unchanged", plan.subvolume.display())],
        ));
        prepared.discard();
        return Err(refused);
    }

    match prepared.commit() {
        Ok(restored) => {
            let mut notes = vec![format!(
                "what was replaced is kept as {}",
                restored.replaced_kept_as
            )];
            // Recorded, because the two are different guarantees and an audit
            // that cannot tell them apart cannot say whether an interruption
            // was survivable.
            notes.push(
                if restored.atomic {
                    "swapped atomically; there was no instant with no tree"
                } else {
                    "swapped with two renames; this filesystem has no atomic exchange, \
                     so there was a window with no tree in place"
                }
                .to_string(),
            );

            journal.append(&entry(Outcome::Success, notes))?;
            Ok(restored)
        }
        Err(error) => {
            let _ = journal.append(&entry(
                Outcome::Rejected {
                    reason: error.to_string(),
                },
                vec![format!("{} is unchanged", plan.subvolume.display())],
            ));
            Err(CoreError::Snapshot(error))
        }
    }
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
        std::fs::write(subvolume.join("a.txt"), "original\n").unwrap();
        (dir, store, Snapshots::of(Directories, subvolume))
    }

    fn entries(store: &Store) -> Vec<Entry> {
        Journal::open(store.journal_path())
            .unwrap()
            .entries()
            .unwrap()
    }

    #[test]
    fn a_plan_says_what_would_be_deleted_and_what_merely_reverted() {
        let (_dir, _store, snapshots) = setup();
        let taken = snapshots.take("before").unwrap();

        std::fs::write(snapshots.subvolume().join("new.txt"), "since\n").unwrap();
        std::fs::write(snapshots.subvolume().join("a.txt"), "edited since\n").unwrap();

        let plan = plan(&snapshots, &taken.name).unwrap();
        let cost = plan.what_it_costs();

        assert!(cost.contains("DELETED"), "{cost}");
        assert!(cost.contains("older contents"), "{cost}");
        assert_eq!(plan.difference.lost_outright(), 1);
    }

    #[test]
    fn a_restore_that_would_change_nothing_says_so_instead_of_listing_nothing() {
        // "Restoring would change nothing" and an empty list of changes read
        // very differently to somebody deciding whether to answer yes.
        let (_dir, _store, snapshots) = setup();
        let taken = snapshots.take("before").unwrap();

        let plan = plan(&snapshots, &taken.name).unwrap();
        assert!(plan.is_a_no_op());
        assert!(plan.what_it_costs().contains("would change nothing"));
    }

    #[test]
    fn the_intent_is_on_disk_before_the_tree_moves() {
        // An interruption during the fallback path leaves a tree that is
        // simply missing. The intent is what turns that into something
        // recoverable rather than something inexplicable.
        let (_dir, store, snapshots) = setup();
        let taken = snapshots.take("before").unwrap();
        std::fs::write(snapshots.subvolume().join("new.txt"), "since\n").unwrap();

        let plan = plan(&snapshots, &taken.name).unwrap();
        apply(&store, &snapshots, &plan, "req-1").unwrap();

        let recorded = entries(&store);
        let intent = recorded
            .iter()
            .find(|entry| entry.operation == "restore" && entry.outcome == Outcome::Intended)
            .expect("no intent was written");

        assert_eq!(intent.snapshot.as_deref(), Some(taken.name.as_str()));
        assert!(
            intent
                .notes
                .iter()
                .any(|note| note.contains(&snapshots.subvolume().display().to_string())),
            "the intent does not name the subvolume: {:?}",
            intent.notes
        );

        // And it is resolved by the entry that follows it.
        let last = recorded.last().unwrap();
        assert_eq!(last.operation, "restore");
        assert_eq!(last.outcome, Outcome::Success);
    }

    #[test]
    fn the_journal_records_whether_the_swap_was_atomic() {
        // Two different guarantees. An audit that cannot tell them apart
        // cannot say whether an interruption would have been survivable.
        let (_dir, store, snapshots) = setup();
        let taken = snapshots.take("before").unwrap();

        let plan = plan(&snapshots, &taken.name).unwrap();
        apply(&store, &snapshots, &plan, "req-1").unwrap();

        let last = entries(&store).pop().unwrap();
        assert!(
            last.notes.iter().any(|note| note.contains("swapped")),
            "{:?}",
            last.notes
        );
    }

    #[test]
    fn a_restore_records_where_the_replaced_tree_went() {
        // The record is the only thing standing between "this destroyed your
        // work" and "this destroyed your work and here is where it went".
        let (_dir, store, snapshots) = setup();
        let taken = snapshots.take("before").unwrap();
        std::fs::write(snapshots.subvolume().join("new.txt"), "since\n").unwrap();

        let plan = plan(&snapshots, &taken.name).unwrap();
        let restored = apply(&store, &snapshots, &plan, "req-1").unwrap();

        let last = entries(&store).pop().unwrap();
        assert!(
            last.notes
                .iter()
                .any(|note| note.contains(&restored.replaced_kept_as)),
            "{:?}",
            last.notes
        );

        let kept = snapshots.directory().join(&restored.replaced_kept_as);
        assert!(kept.join("new.txt").exists());
    }

    #[test]
    fn a_restore_that_could_not_happen_says_the_tree_is_unchanged() {
        let (_dir, store, snapshots) = setup();
        let plan = Plan {
            snapshot: "never-taken".to_string(),
            subvolume: snapshots.subvolume().to_path_buf(),
            difference: Difference::default(),
            state: thalyx_snapshot::witness(snapshots.subvolume()),
        };

        assert!(apply(&store, &snapshots, &plan, "req-1").is_err());

        let last = entries(&store).pop().unwrap();
        assert!(matches!(last.outcome, Outcome::Rejected { .. }));
        assert!(
            last.notes.iter().any(|note| note.contains("unchanged")),
            "{:?}",
            last.notes
        );
        assert!(snapshots.subvolume().join("a.txt").exists());
    }
}
