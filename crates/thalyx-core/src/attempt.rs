//! The named attempt: begin, keep, abandon.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **D2**, and the
//! sentence [[Filosofia-Fundacional]] uses for the advantage no other operating
//! system has: *«intenta esto y si sale mal deshazlo»*.
//!
//! It is the fourth of the five costs — what an error costs, and whether it can
//! be taken back — and the decree is blunt about why that one changes behaviour
//! more than the others: in a system where everything is irreversible a rational
//! agent becomes timid, asks too much and tries too little, and that does not
//! read as prudence, it reads as incapacity. The cause is not in the model. It
//! is that the system offers no way to *attempt* anything.
//!
//! ## What is here and what is not
//!
//! Both pieces already existed — snapshots in [`crate::snapshots`] and the
//! journal — and what was missing was joining them and giving the result to
//! somebody who is not the core. So this module is the joining: the state that
//! says an attempt is open, and the four rules about it. The Btrfs mechanics
//! stay in `thalyx-snapshot` behind [`Volumes`], which is what lets everything
//! here be exercised on a filesystem that is not Btrfs.
//!
//! That split is the crate's own, written when the fake was: *«policy that can
//! only be exercised on a Btrfs filesystem is policy that is never exercised»*.
//! It applies exactly here. Whether a second attempt may be opened, what happens
//! when the snapshot an attempt names has been deleted, and whether an
//! interrupted abandon leaves a record are not Btrfs questions, and the answers
//! would be untested for months if they were treated as ones.
//!
//! ## The four rules, and the failure each one prevents
//!
//! 1. **One at a time.** A second attempt opened over the first makes
//!    `abandonar` ambiguous — back to which of them? — and the ambiguity is
//!    resolved at the worst possible moment, by a caller that is already having
//!    a bad day.
//! 2. **A record that cannot be read is not a record that is not there.** Rule
//!    10. A corrupt attempt file read as "nothing is open" would let a caller
//!    start a second attempt on top of a live snapshot and believe the first one
//!    never happened.
//! 3. **The attempt names its own subvolume.** Abandoning uses the tree the
//!    attempt was opened on, never wherever the session happens to be standing
//!    when the caller changes its mind.
//! 4. **An abandon that could not happen does not clear the attempt.** If the
//!    snapshot is gone the honest answer is that this attempt can no longer be
//!    abandoned — and the record stays, because a caller told "done" would go on
//!    believing the tree was returned.

use crate::store::Store;
use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thalyx_journal::{Entry, Journal, Origin, Outcome};
use thalyx_snapshot::{Restored, Snapshots, Volumes};

/// An attempt that has been opened and not yet settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Open {
    /// What the caller called it. Free text, for a person reading the journal.
    pub label: String,
    /// The snapshot the tree goes back to, by name.
    pub snapshot: String,
    /// The tree this attempt is about. Recorded rather than re-derived, so
    /// abandoning cannot be aimed at a different one by moving.
    pub subvolume: PathBuf,
    pub started_at: String,
    pub request_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AttemptError {
    #[error("an attempt is already open: `{0}`. Settle it before starting another")]
    AlreadyOpen(String),

    #[error("no attempt is open")]
    NoneOpen,

    /// The record exists and could not be understood. **Never** reported as no
    /// attempt: that is the one answer that would let a caller open a second one
    /// over a live snapshot.
    #[error("there is an attempt on record and it could not be read: {0}")]
    Unreadable(String),

    #[error("the snapshot `{0}` this attempt goes back to is not there, so it cannot be abandoned")]
    SnapshotGone(String),
}

/// Where the open attempt is written down.
///
/// In the store and not in the tree under test, for the reason the index has the
/// same rule: a record that lives inside what it describes is part of what an
/// abandon would revert, and abandoning would then delete the note saying an
/// abandon was in progress.
pub fn path(store: &Store) -> PathBuf {
    store.state_root().join("attempt.json")
}

/// The attempt that is open, or that none is.
///
/// Three outcomes and not two: open, none, and *there is one and it cannot be
/// read*. Rule 10, and the reason this returns a `Result<Option<_>>` rather than
/// an `Option`.
pub fn open(store: &Store) -> Result<Option<Open>> {
    let file = path(store);
    let raw = match std::fs::read_to_string(&file) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(CoreError::io(&file, error)),
    };
    match serde_json::from_str(&raw) {
        Ok(attempt) => Ok(Some(attempt)),
        Err(error) => Err(CoreError::Attempt(AttemptError::Unreadable(
            error.to_string(),
        ))),
    }
}

fn write(store: &Store, attempt: &Open) -> Result<()> {
    let file = path(store);
    let raw = serde_json::to_string_pretty(attempt).expect("an attempt serialises");
    std::fs::write(&file, raw).map_err(|error| CoreError::io(&file, error))
}

fn clear(store: &Store) -> Result<()> {
    let file = path(store);
    match std::fs::remove_file(&file) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CoreError::io(&file, error)),
    }
}

fn journal_entry(operation: &str, outcome: Outcome, attempt: &Open, notes: Vec<String>) -> Entry {
    Entry {
        timestamp: thalyx_journal::now(),
        operation: operation.to_string(),
        module_id: None,
        version: None,
        outcome,
        request_id: attempt.request_id.clone(),
        origin: Origin::UserUtterance,
        snapshot: Some(attempt.snapshot.clone()),
        notes,
    }
}

/// Open an attempt on a subvolume.
///
/// The snapshot is taken **before** the record is written. The other order would
/// leave, on an interruption, a record naming a snapshot that does not exist —
/// an attempt that cannot be abandoned and says nothing about why.
pub fn begin<V: Volumes>(
    store: &Store,
    snapshots: &Snapshots<V>,
    label: &str,
    request_id: &str,
) -> Result<Open> {
    if let Some(already) = open(store)? {
        return Err(CoreError::Attempt(AttemptError::AlreadyOpen(already.label)));
    }

    let taken = crate::snapshots::take(store, snapshots, label, request_id)?;
    let attempt = Open {
        label: label.to_string(),
        snapshot: taken.name,
        subvolume: snapshots.subvolume().to_path_buf(),
        started_at: thalyx_journal::now(),
        request_id: request_id.to_string(),
    };
    write(store, &attempt)?;

    let journal = Journal::open(store.journal_path())?;
    journal.append(&journal_entry(
        "attempt_begin",
        Outcome::Success,
        &attempt,
        vec![format!("on {}", attempt.subvolume.display())],
    ))?;

    Ok(attempt)
}

/// Keep everything that was done, and let the snapshot go.
///
/// Nothing on the live tree is touched, which is why this needs no confirmation
/// and `abandon` does. What it costs is the ability to change one's mind, and
/// that is said rather than assumed.
pub fn keep<V: Volumes>(store: &Store, snapshots: &Snapshots<V>, request_id: &str) -> Result<Open> {
    let attempt = open(store)?.ok_or(CoreError::Attempt(AttemptError::NoneOpen))?;

    // The record is cleared even if the snapshot could not be deleted, and that
    // asymmetry with `abandon` is deliberate: a leftover snapshot costs disk and
    // nothing else, while an attempt that cannot be settled blocks every
    // following one. The failure is journalled, so the leftover is findable.
    let forgotten = crate::snapshots::forget(store, snapshots, &attempt.snapshot, request_id);
    clear(store)?;

    let journal = Journal::open(store.journal_path())?;
    journal.append(&journal_entry(
        "attempt_keep",
        match &forgotten {
            Ok(()) => Outcome::Success,
            Err(error) => Outcome::Degraded {
                reason: format!("the snapshot could not be deleted: {error}"),
            },
        },
        &attempt,
        vec![format!("on {}", attempt.subvolume.display())],
    ))?;

    Ok(attempt)
}

/// What abandoning would cost, worked out without abandoning anything.
///
/// The same shape a restore's confirmation uses, because it *is* a restore —
/// and the caller must see it before it can be carried out, whichever face is
/// asking. `Camino-Confiable` is not relaxed because the thing being undone was
/// an agent's own work: the tree is shared, and a person may have written in it
/// while the attempt was open.
pub fn what_abandoning_costs<V: Volumes>(
    store: &Store,
    snapshots: &Snapshots<V>,
) -> Result<(Open, crate::restore::Plan)> {
    let attempt = open(store)?.ok_or(CoreError::Attempt(AttemptError::NoneOpen))?;
    let plan = crate::restore::plan(snapshots, &attempt.snapshot)
        .map_err(|_| CoreError::Attempt(AttemptError::SnapshotGone(attempt.snapshot.clone())))?;
    Ok((attempt, plan))
}

/// Put the tree back and close the attempt.
///
/// **The caller must already have asked.** Like [`crate::restore::apply`], and
/// for the same reason: the trusted path belongs to the layer that can talk to a
/// terminal, and a core that prompted could be made to prompt by something that
/// is not a human.
pub fn abandon<V: Volumes>(
    store: &Store,
    snapshots: &Snapshots<V>,
    attempt: &Open,
    plan: &crate::restore::Plan,
    request_id: &str,
) -> Result<Restored> {
    let restored = crate::restore::apply(store, snapshots, plan, request_id)?;

    // Only now. An abandon that failed leaves the attempt open on purpose — a
    // caller told the attempt was settled would go on believing the tree was
    // returned, which is the one wrong belief this whole feature exists to
    // prevent somebody holding.
    clear(store)?;

    let journal = Journal::open(store.journal_path())?;
    journal.append(&journal_entry(
        "attempt_abandon",
        Outcome::Success,
        attempt,
        vec![
            format!(
                "{} returned to {}",
                attempt.subvolume.display(),
                attempt.snapshot
            ),
            format!(
                "{} file(s) made since then were deleted",
                plan.difference.added_total
            ),
        ],
    ))?;

    Ok(restored)
}

/// The subvolume an open attempt is about, for a caller that has to point
/// [`Snapshots`] at the right tree before it can ask anything else.
pub fn subvolume_of(attempt: &Open) -> &Path {
    &attempt.subvolume
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_snapshot::directories::Directories;

    /// A store and a subvolume the fake will accept.
    ///
    /// The fake copies where Btrfs shares blocks, so it is neither atomic nor
    /// cheap and must never be mistaken for Btrfs. What it models is what these
    /// tests are about — which attempt is open, what a second one does, what an
    /// abandon aims at — and rule 8 is satisfied by that being the property
    /// under test rather than by the fake being a good emulator.
    fn a_machine() -> (tempfile::TempDir, Store, PathBuf) {
        let base = tempfile::tempdir().expect("a temp dir");
        let store = Store::open(base.path().join("store")).expect("a store");
        let tree = base.path().join("work");
        Directories::make_subvolume(&tree).expect("a subvolume");
        std::fs::write(tree.join("before.txt"), "one").expect("a file");
        (base, store, tree)
    }

    fn snapshots_of(tree: &Path) -> Snapshots<Directories> {
        Snapshots::of(Directories, tree)
    }

    #[test]
    fn nothing_is_open_on_a_machine_where_nothing_was_attempted() {
        let (_base, store, _tree) = a_machine();
        assert_eq!(open(&store).unwrap(), None);
    }

    #[test]
    fn an_attempt_can_be_opened_and_is_still_open_after_the_process_that_opened_it() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);

        begin(&store, &snapshots, "refactor", "r1").unwrap();

        // Read back through the file rather than kept in memory: an agent that
        // opens an attempt, works, and comes back in another session is the
        // ordinary case, not the exotic one.
        let reopened = Store::open(store.root()).unwrap();
        let found = open(&reopened).unwrap().expect("the attempt survived");
        assert_eq!(found.label, "refactor");
        assert_eq!(found.subvolume, tree);
    }

    #[test]
    fn a_second_attempt_is_refused_rather_than_nested() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        begin(&store, &snapshots, "first", "r1").unwrap();

        // Nesting makes `abandonar` ambiguous — back to which one? — and the
        // ambiguity would be resolved by a caller that is already having a bad
        // day.
        let second = begin(&store, &snapshots, "second", "r2");
        assert!(
            matches!(
                second,
                Err(CoreError::Attempt(AttemptError::AlreadyOpen(ref label))) if label == "first"
            ),
            "a second attempt was allowed: {second:?}"
        );

        // And the first one is untouched, which is the half that matters: a
        // refusal that damaged what it refused to replace would be worse than
        // allowing it.
        assert_eq!(open(&store).unwrap().unwrap().label, "first");
    }

    #[test]
    fn abandoning_puts_back_what_was_changed_and_deletes_what_was_made() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        begin(&store, &snapshots, "try", "r1").unwrap();

        std::fs::write(tree.join("before.txt"), "changed").unwrap();
        std::fs::write(tree.join("new.txt"), "made during the attempt").unwrap();

        let (attempt, plan) = what_abandoning_costs(&store, &snapshots).unwrap();
        abandon(&store, &snapshots, &attempt, &plan, "r1").unwrap();

        // The whole sentence of the decree, checked on the disk rather than in
        // the report: *intenta esto y si sale mal deshazlo*.
        assert_eq!(
            std::fs::read_to_string(tree.join("before.txt")).unwrap(),
            "one"
        );
        assert!(
            !tree.join("new.txt").exists(),
            "a file made during the attempt survived it"
        );
        assert_eq!(open(&store).unwrap(), None);
    }

    #[test]
    fn keeping_leaves_the_work_alone_and_closes_the_attempt() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        begin(&store, &snapshots, "try", "r1").unwrap();
        std::fs::write(tree.join("new.txt"), "kept").unwrap();

        keep(&store, &snapshots, "r1").unwrap();

        // The control for the test above. Without it, an implementation that
        // reverted on both paths would pass the abandon test and be useless.
        assert!(tree.join("new.txt").exists(), "keeping reverted the work");
        assert_eq!(open(&store).unwrap(), None);
    }

    #[test]
    fn what_abandoning_costs_is_answerable_without_abandoning_anything() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        begin(&store, &snapshots, "try", "r1").unwrap();
        std::fs::write(tree.join("new.txt"), "made").unwrap();

        let (_, plan) = what_abandoning_costs(&store, &snapshots).unwrap();
        assert_eq!(plan.difference.lost_outright(), 1);

        // Nothing moved. A "what would this cost" that costs something is not
        // one anybody can afford to ask.
        assert!(tree.join("new.txt").exists());
        assert!(open(&store).unwrap().is_some());
    }

    #[test]
    fn an_unreadable_record_is_never_reported_as_no_attempt() {
        let (_base, store, _tree) = a_machine();
        std::fs::write(path(&store), "{ this is not json").unwrap();

        // Rule 10, and the sharpest case of it in this module: read as "nothing
        // open", a corrupt record lets a caller open a second attempt over a
        // live snapshot and believe the first never happened.
        let found = open(&store);
        assert!(
            matches!(found, Err(CoreError::Attempt(AttemptError::Unreadable(_)))),
            "a corrupt attempt read as: {found:?}"
        );
    }

    #[test]
    fn an_attempt_whose_snapshot_is_gone_says_so_and_stays_open() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        let attempt = begin(&store, &snapshots, "try", "r1").unwrap();

        snapshots.forget(&attempt.snapshot).unwrap();

        let asked = what_abandoning_costs(&store, &snapshots);
        assert!(
            matches!(
                asked,
                Err(CoreError::Attempt(AttemptError::SnapshotGone(_)))
            ),
            "{asked:?}"
        );
        // Still open, deliberately. A caller told the attempt was settled would
        // go on believing the tree had been returned, which is the one wrong
        // belief this feature exists to stop anybody holding.
        assert!(open(&store).unwrap().is_some());
    }

    #[test]
    fn settling_nothing_says_nothing_is_open_rather_than_succeeding_quietly() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);

        assert!(matches!(
            keep(&store, &snapshots, "r1"),
            Err(CoreError::Attempt(AttemptError::NoneOpen))
        ));
        assert!(matches!(
            what_abandoning_costs(&store, &snapshots),
            Err(CoreError::Attempt(AttemptError::NoneOpen))
        ));
    }

    #[test]
    fn every_step_of_an_attempt_is_in_the_journal() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        begin(&store, &snapshots, "try", "r1").unwrap();
        keep(&store, &snapshots, "r1").unwrap();

        let entries = Journal::open(store.journal_path())
            .unwrap()
            .entries()
            .unwrap();
        let operations: Vec<&str> = entries.iter().map(|e| e.operation.as_str()).collect();

        // Without this, an attempt is a thing that happened to the machine with
        // no record of it — and `historia` would answer a question about the
        // most consequential operation Thalyx has with silence.
        assert!(operations.contains(&"attempt_begin"), "{operations:?}");
        assert!(operations.contains(&"attempt_keep"), "{operations:?}");
    }

    #[test]
    fn abandoning_aims_at_the_tree_the_attempt_was_opened_on() {
        let (base, store, tree) = a_machine();
        let elsewhere = base.path().join("elsewhere");
        Directories::make_subvolume(&elsewhere).unwrap();

        let attempt = begin(&store, &snapshots_of(&tree), "try", "r1").unwrap();

        // The record names the tree, so a caller that has moved cannot abandon
        // the wrong one by standing somewhere else.
        assert_eq!(subvolume_of(&attempt), tree);
        assert_ne!(subvolume_of(&attempt), elsewhere);
    }
}
