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
//! 5. **Each of the three is one transition, under the global lock.** Added
//!    2026-08-28, when an audit pointed out that rule 1 was checked and not
//!    enforced. `begin` read the record, took a snapshot and wrote the record,
//!    with nothing between the read and the write to stop a second client doing
//!    the same — so two clients arriving together both saw "nothing is open",
//!    both snapshotted, and the second overwrote the first's record. The first
//!    attempt's snapshot then existed with nothing naming it: unabandonable,
//!    and invisible except as disk that never comes back. `keep` and `abandon`
//!    had the mirror of it — two clients planning against the same attempt and
//!    both carrying it out, restoring one snapshot twice.
//!
//!    The lock is [`Store::lock`], the same `flock` every other multi-file
//!    contract in this crate takes; there is no second locking scheme here.
//!    Because `flock` attaches to an open file description and is therefore not
//!    reentrant, [`crate::restore::apply_holding_the_lock`] exists so that
//!    `abandon` can restore without waiting for itself.
//!
//!    And the record is published the way every other state file in this crate
//!    is: written to a unique temporary in the same directory, `fsync`ed,
//!    `rename`d over, and the directory `fsync`ed after. `std::fs::write`,
//!    which is what this used, can leave a half-written `attempt.json` after a
//!    crash — which rule 2 turns into "there is an attempt and it cannot be
//!    read", correctly but permanently: the machine can no longer begin an
//!    attempt and no longer abandon the one it thinks it has.

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

    /// The attempt this call was planned against is no longer the one on
    /// record. Somebody else settled it between the plan and the carrying out.
    ///
    /// Reported rather than carried out anyway, which would restore a snapshot
    /// a second time — over a tree that the first abandon had already returned
    /// and that somebody may have started working in again.
    #[error(
        "the attempt this was planned against has been settled already; \
         the one on record now is `{0}`"
    )]
    Superseded(String),

    /// The workspace is not in the state the caller authorised destroying.
    ///
    /// The whole of what makes a rollback in one call safe. A caller states
    /// which state it believes the tree is in; if anything at all has been
    /// written since — by a person, by another agent, by a build — the state it
    /// named is no longer the state that would be destroyed, and this is the
    /// refusal rather than the destruction.
    #[error(
        "the workspace has been written to since this rollback was authorised: \
         it was `{expected}` and it is `{found}` now. Nothing was changed"
    )]
    WorkspaceMoved { expected: String, found: String },

    /// The tree could not be read everywhere, so no exact claim about it can be
    /// checked. Rule 9: the cautious answer, never the fast one.
    #[error(
        "{unreadable} path(s) under the workspace could not be read, so what a \
         rollback would destroy cannot be established exactly. Nothing was changed"
    )]
    WorkspaceUnreadable { unreadable: usize },
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

/// Write the record atomically and durably.
///
/// [`crate::keystore::save_json`] and not `std::fs::write`, which is what this
/// was: a crash in the middle of that leaves a file that is half of one record
/// and half of another, and rule 2 above then reports — correctly — that there
/// is an attempt on record and it cannot be read. Correctly and permanently:
/// nothing can begin an attempt after that, and nothing can abandon the one the
/// machine believes it has. This is the primitive holding a promise of
/// recovery, so it gets the same publication every other state file here gets.
fn write(store: &Store, attempt: &Open) -> Result<()> {
    crate::keystore::save_json(&path(store), attempt)
}

/// Forget the record, and make the forgetting survive a power cut.
///
/// The `fsync` of the directory is the same half `write_durably` does after its
/// rename, and it matters in the same way: without it the unlink is atomic with
/// respect to readers and not with respect to power, so a machine could come
/// back up believing an attempt is open over a snapshot that was already let
/// go — and rule 4 would then refuse every abandon for a reason that is not
/// true any more.
fn clear(store: &Store) -> Result<()> {
    let file = path(store);
    match std::fs::remove_file(&file) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(CoreError::io(&file, error)),
    }
    let parent = store.state_root();
    let dir = std::fs::File::open(&parent).map_err(|error| CoreError::io(&parent, error))?;
    dir.sync_all()
        .map_err(|error| CoreError::io(&parent, error))
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
///
/// All three steps — read the record, take the snapshot, write the record — are
/// under the global lock, which is rule 5. Checking that nothing is open and
/// then writing as if that were still true is not a check, and the two clients
/// it lets through do not collide noisily: the second one's record wins and the
/// first one's snapshot is left with nothing naming it.
pub fn begin<V: Volumes>(
    store: &Store,
    snapshots: &Snapshots<V>,
    label: &str,
    request_id: &str,
) -> Result<Open> {
    let _lock = store.lock()?;

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
    let _lock = store.lock()?;

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

/// How a caller earned the right to have a tree replaced.
///
/// Two shapes and not a boolean, because the two are authorised by different
/// things and the difference is the whole of what was wrong with the design
/// this replaced.
///
/// [`Authorised::ByAHuman`] is `Camino-Confiable`: somebody was shown what
/// would be lost and said yes. What they saw was the tree in front of them, and
/// their answer covers whatever it holds when they give it — a person cannot
/// state a digest and should never be asked to.
///
/// [`Authorised::ByState`] is a program saying the same thing in the only way a
/// program can mean it: **this exact state, and no other.** It is stronger than
/// the human's yes and not weaker, which is why it is allowed to be one call
/// where the human's is two.
#[derive(Debug, Clone, Copy)]
pub enum Authorised<'a> {
    /// Somebody was asked and answered. The tree is replaced as it stands.
    ByAHuman,
    /// A caller named the state it is authorising the destruction of. Checked
    /// against the tree under the lock, and refused if anything has moved.
    ByState(&'a str),
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
    authorised: Authorised<'_>,
    request_id: &str,
) -> Result<Restored> {
    // One transition: check the record, restore, clear the record. The plan and
    // the confirmation were made outside this lock — they have to be, because
    // the confirmation is a person at a terminal and holding the machine's
    // global lock while somebody reads a question is how a machine stops
    // answering. So what could have changed in between is re-read here.
    let lock = store.lock()?;

    // Rule 4's neighbour, and the reason `Superseded` exists. Two clients that
    // both planned against the same attempt would otherwise both carry it out:
    // the second restore lands on a tree the first one already returned, and
    // silently deletes whatever was written in it since.
    match open(store)? {
        None => return Err(CoreError::Attempt(AttemptError::NoneOpen)),
        Some(on_record) if &on_record != attempt => {
            return Err(CoreError::Attempt(AttemptError::Superseded(
                on_record.label,
            )));
        }
        Some(_) => {}
    }

    // And the tree itself, re-read inside the lock and **as the last thing
    // before it is replaced** — not here, but in the restore, with the writable
    // copy already built and the exchange the only step left.
    //
    // Not against the plan's witness, which was taken outside the lock and is
    // therefore a statement about a moment that has already passed. What is
    // compared is the state the *caller* authorised destroying against the
    // state the tree is in at the instant of destruction, with as little as
    // possible able to run in between — which is the only place that comparison
    // means anything. A check made before taking the lock would be the same
    // shape of defect as the `canonicalize`-then-open sequence `crate::api` was
    // rewritten to stop using: a comparison and an action with a moment between
    // them.
    //
    // ## What the window is, since it is not zero
    //
    // The walk is not instantaneous and the exchange is not part of it, so
    // there is an interval — one walk of the tree, plus one `renameat2` — in
    // which a write by somebody who does not take Thalyx's lock lands after the
    // answer. Nothing short of freezing the filesystem closes that, and the
    // honest statement of what is guaranteed is therefore two sentences and not
    // one:
    //
    //   - a write that **completed before the check** is never lost: the
    //     witness moved, the claim stops matching, and this refuses;
    //   - a write that lands inside the window is **not destroyed**, only
    //     displaced: the tree that was live is kept, and `replaced_kept_as`
    //     names where. `Rollback-vs-Restore` requires that of every restore,
    //     and it is what makes the residual race survivable rather than silent.
    //
    // Thalyx's own clients cannot be in that window at all — they queue on
    // `Store::lock`, which this holds.
    let last_look = || -> Result<()> {
        let Authorised::ByState(claimed) = authorised else {
            return Ok(());
        };
        let now = thalyx_snapshot::witness(&attempt.subvolume);
        if !now.is_complete() {
            return Err(CoreError::Attempt(AttemptError::WorkspaceUnreadable {
                unreadable: now.unreadable,
            }));
        }
        if !now.matches(claimed) {
            return Err(CoreError::Attempt(AttemptError::WorkspaceMoved {
                expected: claimed.to_string(),
                found: now.id,
            }));
        }
        Ok(())
    };

    let restored = crate::restore::apply_holding_the_lock(
        store, snapshots, plan, request_id, &lock, &last_look,
    )?;

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
        std::fs::write(tree.join("before.txt"), "what was there before").expect("a file");
        (base, store, tree)
    }

    fn snapshots_of(tree: &Path) -> Snapshots<Directories> {
        Snapshots::of(Directories, tree)
    }

    /// Write, with nothing waiting for the clock.
    ///
    /// There used to be a twenty-millisecond sleep here, and it was the defect
    /// confessing: a witness made of timestamps cannot separate two writes
    /// inside one filesystem tick, so the tests had to wait for a tick. **The
    /// case that does not wait is the real one** — on Fedora, `dev/verify.sh`
    /// stage 55 writes, takes the state, and writes again immediately, which is
    /// what an agent and a person sharing a tree actually do.
    ///
    /// Since 2026-08-29 the witness covers what each file holds, so no wait is
    /// needed and none is taken. Every write below is the same length as the
    /// one before it, too, so `size` cannot be doing the work either.
    fn write_now(path: &Path, text: &str) {
        std::fs::write(path, text).expect("the write");
    }

    #[test]
    fn a_rollback_that_names_the_state_the_tree_is_in_goes_ahead_in_one_call() {
        // The positive control, and without it the test below cannot be read: a
        // rule that refused every rollback would pass the negative one and
        // break the feature entirely.
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        let attempt = begin(&store, &snapshots, "rename", "r1").expect("an attempt");

        write_now(&tree.join("before.txt"), "what the agent wrote!");

        let (_, plan) = what_abandoning_costs(&store, &snapshots).expect("a plan");
        abandon(
            &store,
            &snapshots,
            &attempt,
            &plan,
            Authorised::ByState(&plan.state.id),
            "r2",
        )
        .expect("the tree was exactly what the caller said it was");

        assert_eq!(
            std::fs::read_to_string(tree.join("before.txt")).unwrap(),
            "what was there before",
            "the rollback did not happen"
        );
        assert!(open(&store).unwrap().is_none(), "the attempt is still open");
    }

    #[test]
    fn a_write_to_a_file_the_agent_had_already_written_stops_the_rollback() {
        // **The defect this mechanism was built for**, end to end. The counts
        // are identical before and after the third party's write — one modified
        // file either way — so the protection that preceded this one let the
        // rollback through and replaced their work with the snapshot.
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        let attempt = begin(&store, &snapshots, "rename", "r1").expect("an attempt");

        write_now(&tree.join("before.txt"), "what the agent wrote!");
        let (_, plan) = what_abandoning_costs(&store, &snapshots).expect("a plan");
        let state_the_agent_saw = plan.state.id.clone();

        // Somebody else, in the same file, while the attempt is open.
        write_now(&tree.join("before.txt"), "what the person wrote");

        let refused = abandon(
            &store,
            &snapshots,
            &attempt,
            &plan,
            Authorised::ByState(&state_the_agent_saw),
            "r2",
        );

        assert!(
            matches!(
                refused,
                Err(CoreError::Attempt(AttemptError::WorkspaceMoved { .. }))
            ),
            "a stale state authorised a destruction: {refused:?}"
        );
        assert_eq!(
            std::fs::read_to_string(tree.join("before.txt")).unwrap(),
            "what the person wrote",
            "the third party's work was destroyed"
        );
        // And the attempt is still open, because an abandon that did not happen
        // must not be recorded as one — rule 4 of this module.
        assert!(open(&store).unwrap().is_some());
    }

    #[test]
    fn the_counts_alone_would_not_have_noticed_that_write() {
        // The other half of the test above, and the reason it is a defect
        // rather than a preference. If this ever fails, the counterexample has
        // changed and the whole argument for a witness needs re-reading.
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        begin(&store, &snapshots, "rename", "r1").expect("an attempt");

        write_now(&tree.join("before.txt"), "what the agent wrote!");
        let (_, agents) = what_abandoning_costs(&store, &snapshots).expect("a plan");

        write_now(&tree.join("before.txt"), "what the person wrote");
        let (_, after) = what_abandoning_costs(&store, &snapshots).expect("a plan");

        assert_eq!(
            (
                agents.difference.added_total,
                agents.difference.modified_total
            ),
            (
                after.difference.added_total,
                after.difference.modified_total
            ),
            "the counts moved, so this is no longer the case that fooled them"
        );
        assert_ne!(agents.state.id, after.state.id);
    }

    #[test]
    fn a_tree_that_could_not_be_read_everywhere_authorises_nothing() {
        // Rules 9 and 10 on the one path where nobody is asked twice. A file
        // that can be stat'd and not read is a file nobody has compared, and a
        // digest over the part that could be read is not an identity of
        // anything. The refusal has its own word, because a caller told
        // `workspace_moved` would go looking for somebody else's edit.
        let (_base, store, tree) = a_machine();
        if running_as_root() {
            println!("NOT PROVEN: running as root, where a mode cannot make a file unreadable");
            return;
        }
        let snapshots = snapshots_of(&tree);
        let attempt = begin(&store, &snapshots, "rename", "r1").expect("an attempt");

        write_now(&tree.join("before.txt"), "what the agent wrote!");
        let (_, plan) = what_abandoning_costs(&store, &snapshots).expect("a plan");
        let stated = plan.state.id.clone();

        // A hole appears after the plan was made. Whatever the caller states,
        // there is now no exact identity of this tree.
        std::fs::write(tree.join("locked.txt"), "unreadable from here on").expect("a file");
        std::fs::set_permissions(
            tree.join("locked.txt"),
            std::os::unix::fs::PermissionsExt::from_mode(0o000),
        )
        .expect("the mode");

        let refused = abandon(
            &store,
            &snapshots,
            &attempt,
            &plan,
            Authorised::ByState(&stated),
            "r2",
        );

        assert!(
            matches!(
                refused,
                Err(CoreError::Attempt(AttemptError::WorkspaceUnreadable { .. }))
            ),
            "a tree with a hole in it authorised a destruction: {refused:?}"
        );
        assert_eq!(
            std::fs::read_to_string(tree.join("before.txt")).unwrap(),
            "what the agent wrote!",
            "the tree was replaced anyway"
        );
        assert!(open(&store).unwrap().is_some());
    }

    #[test]
    fn a_refused_rollback_leaves_nothing_half_built_beside_the_subvolume() {
        // The restore is now built *before* the last look at the tree, so that
        // as little as possible sits between the answer and the swap. That
        // ordering has a cost of its own and this is it: when the look refuses,
        // a writable copy of the snapshot already exists, and leaving it there
        // would turn every refused rollback into a tree nobody asked for.
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        let attempt = begin(&store, &snapshots, "rename", "r1").expect("an attempt");

        write_now(&tree.join("before.txt"), "what the agent wrote!");
        let (_, plan) = what_abandoning_costs(&store, &snapshots).expect("a plan");
        let stated = plan.state.id.clone();
        write_now(&tree.join("before.txt"), "what the person wrote");

        let refused = abandon(
            &store,
            &snapshots,
            &attempt,
            &plan,
            Authorised::ByState(&stated),
            "r2",
        );
        assert!(refused.is_err());

        let leftovers: Vec<String> = std::fs::read_dir(snapshots.directory())
            .expect("the snapshot directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".restoring-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "a refused rollback left {leftovers:?} beside the subvolume"
        );
    }

    /// Whether a mode can still make a file unreadable to this process.
    ///
    /// No `unsafe` outside `thalyx-syscall`, and no dependency worth adding for
    /// one number: the effective uid is in `/proc/self/status`.
    fn running_as_root() -> bool {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status
                    .lines()
                    .find(|line| line.starts_with("Uid:"))
                    .map(|line| line.split_whitespace().nth(1) == Some("0"))
            })
            .unwrap_or(false)
    }

    #[test]
    fn a_human_who_said_yes_is_not_asked_for_a_digest() {
        // `Camino-Confiable`: a person was shown what would be lost and
        // answered about the tree in front of them. Requiring them to state a
        // digest would be requiring something no person can produce, and the
        // two-step path is the escape hatch every caller written before today
        // still uses.
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);
        let attempt = begin(&store, &snapshots, "rename", "r1").expect("an attempt");
        write_now(&tree.join("before.txt"), "changed");

        let (_, plan) = what_abandoning_costs(&store, &snapshots).expect("a plan");
        abandon(
            &store,
            &snapshots,
            &attempt,
            &plan,
            Authorised::ByAHuman,
            "r2",
        )
        .expect("a human's yes still settles an attempt");
        assert_eq!(
            std::fs::read_to_string(tree.join("before.txt")).unwrap(),
            "what was there before"
        );
    }

    /// Two clients arriving at the same moment, on one real store.
    ///
    /// Threads and not a mock, because the thing under test is `flock`, and a
    /// mock of `flock` would be a mock that grants the property it is meant to
    /// prove — rule 8.
    ///
    /// `Concurrencia.md` warns that a thread is the wrong instrument for a
    /// `flock` test, and it is right about the case it is describing: a thread
    /// that *shares* an open file description is let straight through. That is
    /// not this. Each thread here calls `Store::open` and then `Store::lock`,
    /// and `lock` opens `state/lock` itself — two descriptions, which is what
    /// two processes have. The falsification is on record: with the lock taken
    /// out of `begin`, this test fails every run.
    ///
    /// The barrier is what makes it a race rather than a sequence. Without it
    /// the first thread finishes before the second starts and the test passes
    /// on a machine where the bug is still there.
    #[test]
    fn two_clients_beginning_at_the_same_moment_open_exactly_one_attempt() {
        use std::sync::{Arc, Barrier};

        let (_base, store, tree) = a_machine();
        let root = store.root().to_path_buf();
        let both = Arc::new(Barrier::new(2));

        let outcomes: Vec<_> = ["one", "two"]
            .into_iter()
            .map(|label| {
                let (root, tree, both) = (root.clone(), tree.clone(), Arc::clone(&both));
                std::thread::spawn(move || {
                    let store = Store::open(root).expect("a store");
                    let snapshots = snapshots_of(&tree);
                    both.wait();
                    begin(&store, &snapshots, label, label).map(|open| open.label)
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().expect("no panic"))
            .collect();

        let won: Vec<_> = outcomes.iter().filter(|result| result.is_ok()).collect();
        assert_eq!(won.len(), 1, "both clients opened an attempt: {outcomes:?}");
        assert!(
            outcomes.iter().any(|result| matches!(
                result,
                Err(CoreError::Attempt(AttemptError::AlreadyOpen(_)))
            )),
            "the loser was told something other than that an attempt is open: {outcomes:?}"
        );

        // And the half that a "one of them failed" assertion would miss: the
        // record on disk names the winner, and there is exactly one snapshot.
        // Two clients that both snapshotted and one of whose records was
        // overwritten leave a snapshot nothing can ever abandon.
        let on_record = open(&store).unwrap().expect("an attempt is open");
        assert_eq!(
            Some(&on_record.label),
            won[0].as_ref().ok(),
            "the record does not name the client that won"
        );
        let snapshots = snapshots_of(&tree);
        assert_eq!(
            snapshots.list().unwrap().len(),
            1,
            "a snapshot was taken by a client that did not get to record it"
        );
    }

    /// The mirror of it: two clients that both planned against the same attempt.
    ///
    /// The plan and the confirmation happen outside the lock on purpose — the
    /// confirmation is a person reading a question — so this is the window that
    /// cannot be closed by holding the lock longer, and has to be closed by
    /// re-reading the record inside it. Carrying both out would restore the
    /// same snapshot twice, the second time over a tree the first abandon had
    /// already returned and that somebody may have written in since.
    #[test]
    fn two_clients_abandoning_the_same_attempt_carry_it_out_once() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);

        let opened = begin(&store, &snapshots, "refactor", "r1").unwrap();
        std::fs::write(tree.join("after.txt"), "made during the attempt").unwrap();

        // Both plans are made while the attempt is open, which is what two
        // clients that each asked "what would this cost" would hold.
        let (first, first_plan) = what_abandoning_costs(&store, &snapshots).unwrap();
        let (second, second_plan) = what_abandoning_costs(&store, &snapshots).unwrap();
        assert_eq!(first, opened);
        assert_eq!(second, opened);

        abandon(
            &store,
            &snapshots,
            &first,
            &first_plan,
            Authorised::ByAHuman,
            "r2",
        )
        .unwrap();
        assert!(
            !tree.join("after.txt").exists(),
            "the first abandon did nothing"
        );

        // Somebody starts working again in the returned tree.
        std::fs::write(tree.join("after.txt"), "written after the abandon").unwrap();

        let twice = abandon(
            &store,
            &snapshots,
            &second,
            &second_plan,
            Authorised::ByAHuman,
            "r3",
        );
        assert!(
            matches!(twice, Err(CoreError::Attempt(AttemptError::NoneOpen))),
            "a second abandon of a settled attempt was carried out: {twice:?}"
        );
        assert_eq!(
            std::fs::read_to_string(tree.join("after.txt")).unwrap(),
            "written after the abandon",
            "the second abandon deleted work done after the first one finished"
        );
    }

    /// And the same window with a *different* attempt on record, which is the
    /// case `NoneOpen` cannot catch: begin, abandon, begin again, and the stale
    /// plan from the first one arrives.
    #[test]
    fn an_abandon_planned_against_a_settled_attempt_does_not_hit_the_next_one() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);

        let first = begin(&store, &snapshots, "one", "r1").unwrap();
        let (stale, stale_plan) = what_abandoning_costs(&store, &snapshots).unwrap();
        assert_eq!(stale, first);
        keep(&store, &snapshots, "r2").unwrap();

        begin(&store, &snapshots, "two", "r3").unwrap();
        std::fs::write(tree.join("during the second.txt"), "work").unwrap();

        let wrong = abandon(
            &store,
            &snapshots,
            &stale,
            &stale_plan,
            Authorised::ByAHuman,
            "r4",
        );
        assert!(
            matches!(wrong, Err(CoreError::Attempt(AttemptError::Superseded(ref label))) if label == "two"),
            "a stale plan reached the attempt that came after it: {wrong:?}"
        );
        assert!(
            tree.join("during the second.txt").exists(),
            "the second attempt's work was undone by the first attempt's plan"
        );
    }

    /// A record left half-written by a crash.
    ///
    /// Rule 2 already said this must never read as "nothing is open", and it
    /// did not. What this adds is the other half: that the machine cannot then
    /// begin a *new* attempt on top of it either. An unreadable record is a
    /// snapshot that exists with a name nothing can produce, and beginning over
    /// it would strand it for good.
    #[test]
    fn a_half_written_record_stops_everything_rather_than_reading_as_nothing() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);

        begin(&store, &snapshots, "refactor", "r1").unwrap();
        let file = path(&store);
        let whole = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, &whole[..whole.len() / 2]).unwrap();

        assert!(matches!(
            open(&store),
            Err(CoreError::Attempt(AttemptError::Unreadable(_)))
        ));
        assert!(
            matches!(
                begin(&store, &snapshots, "another", "r2"),
                Err(CoreError::Attempt(AttemptError::Unreadable(_)))
            ),
            "a new attempt was opened over a record nobody could read"
        );
        assert!(matches!(
            keep(&store, &snapshots, "r3"),
            Err(CoreError::Attempt(AttemptError::Unreadable(_)))
        ));
    }

    /// That the record is published rather than written in place.
    ///
    /// This does not prove durability against a power cut — nothing in a test
    /// process can, and a mock that claimed to would be the fake rule 8 forbids.
    /// What it proves is the mechanism that durability rests on: the bytes go
    /// to a temporary in the same directory and arrive under the real name by
    /// `rename`, so the only two states a reader can see are the old record and
    /// the new one. The `fsync`s are `keystore::write_durably`'s and are tested
    /// where they live.
    #[test]
    fn the_record_is_published_and_never_written_in_place() {
        let (_base, store, tree) = a_machine();
        let snapshots = snapshots_of(&tree);

        begin(&store, &snapshots, "refactor", "r1").unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(store.state_root())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "the temporary the record was staged in is still there: {leftovers:?}"
        );

        // And the record that landed is whole, which is the property the
        // temporary buys: `std::fs::write` truncates first, so a reader between
        // the truncate and the write finds a file that parses as nothing.
        assert!(open(&store).unwrap().is_some());
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
        abandon(
            &store,
            &snapshots,
            &attempt,
            &plan,
            Authorised::ByAHuman,
            "r1",
        )
        .unwrap();

        // The whole sentence of the decree, checked on the disk rather than in
        // the report: *intenta esto y si sale mal deshazlo*.
        assert_eq!(
            std::fs::read_to_string(tree.join("before.txt")).unwrap(),
            "what was there before"
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
