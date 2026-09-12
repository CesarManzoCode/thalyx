//! VersionedState: the boundary a transaction is carried out inside.
//!
//! `vault/03-Primitivas/Ejecucion-Transaccional.md` describes the transaction;
//! this is what it needs from the machine, and each method exists because
//! `exec::carry_out` made that exact call before there was a boundary to make it
//! through.
//!
//! ## The one method that is new: `candidate`
//!
//! On `linux-current` a verdict is about the live tree at the moment a checker
//! looked, and the commit keeps whatever the tree holds when the snapshot is let
//! go. Nothing names *which* tree a check was about, and the backend says so by
//! answering `None`.
//!
//! The managed model cannot work like that and should not pretend to: a
//! validation is a claim about an immutable version (Thalyx-Kernel ADR-003), and
//! a publication publishes a version, not a directory. So a backend that has
//! candidates answers with the content identity of the frozen tree, and the
//! transaction refuses to publish a candidate on the strength of a verdict about
//! a different one. On a backend that answers `None` that rule is vacuous, which
//! is what keeps `linux-current` exactly what it was.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// How many paths of each kind are listed before the counts take over.
///
/// The same bound `thalyx_snapshot::Difference::SHOWN` has, so that what a
/// program sees from `changed()` is the same shape on every backend.
pub const SHOWN: usize = 20;

/// What a workspace gained, lost and changed since the boundary opened.
///
/// Field for field what `thalyx_snapshot::Difference` is, because it is what a
/// program reads and what the evidence records; a backend that changed its shape
/// would change what Thalyx says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
    pub added_total: usize,
    pub modified_total: usize,
    pub removed_total: usize,
    /// Paths that could not be read. Counted as differences by every reader:
    /// what cannot be compared must not be reported as identical.
    pub unreadable: Vec<String>,
}

impl Changes {
    pub fn count(&self) -> usize {
        self.added_total + self.modified_total + self.removed_total
    }

    pub(crate) fn add(&mut self, path: &str) {
        self.added_total += 1;
        if self.added.len() < SHOWN {
            self.added.push(path.to_string());
        }
    }

    pub(crate) fn modify(&mut self, path: &str) {
        self.modified_total += 1;
        if self.modified.len() < SHOWN {
            self.modified.push(path.to_string());
        }
    }

    pub(crate) fn remove(&mut self, path: &str) {
        self.removed_total += 1;
        if self.removed.len() < SHOWN {
            self.removed.push(path.to_string());
        }
    }
}

/// A boundary that has been opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// What a rollback goes back to, named the way this backend names it: a
    /// snapshot on `linux-current`, a generation on the managed model.
    pub base: String,
    /// The exact identity of the tree when the boundary opened, when every path
    /// of it could be read.
    pub start_state: Option<String>,
}

/// What the transaction decided, as the record of the decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// A version that did not come from a transaction: the tree as it was found
    /// when a managed store first saw it. Never validated, and says so.
    Seed,
    /// Every step went through and every last verdict held, about this candidate.
    Commit,
    /// The caller asked to keep a failure. Named apart from a commit, as
    /// `kept_after_failure` is named apart from `committed`.
    KeepAfterFailure,
    /// Something did not hold, and the work is put back.
    Rollback,
    /// It worked, and the caller asked for the tree back anyway.
    RestoreSuccess,
}

/// One verdict as it gates a decision: which check, what it said, and which
/// candidate it was about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundVerdict {
    pub key: String,
    pub check: String,
    pub verdict: String,
    pub candidate: Option<String>,
}

/// Everything a keep or an abandon is about, handed to the backend whole.
///
/// `linux-current` has nowhere to put it and ignores it; the managed model makes
/// it an immutable object and names it in the same durable record as the root it
/// decided, which is the "receipt and policy in the same publication" rule of
/// Thalyx-Kernel ADR-003.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub transaction: String,
    pub label: String,
    pub decision: Decision,
    pub succeeded: bool,
    pub checks: Vec<BoundVerdict>,
    /// A digest of the program source, when there was one: the receipt says
    /// what was asked for without carrying it.
    pub program: Option<String>,
}

/// What keeping produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Kept {
    /// The generation a publication created, on a backend that has generations.
    pub generation: Option<u64>,
    pub root: Option<String>,
}

/// Why an abandon did not happen, in the two places it can fail.
///
/// Two and not one because Thalyx has always said them differently: a boundary
/// that could not even be planned is still open and nothing was attempted; one
/// that was planned and refused at the last look is still open because the tree
/// moved, and the remedy is different.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbandonFailure {
    /// Nothing was attempted: what abandoning would mean could not be worked out.
    Unplanned(String),
    /// It was attempted and refused, or failed part way. Nothing was put back.
    NotPutBack(String),
}

/// The boundary.
pub trait VersionedState {
    /// The directory the work's requests act on.
    ///
    /// Known before [`VersionedState::open`], because the semantic provider's
    /// accounting is keyed by it and is read before anything runs.
    fn workspace(&self) -> &Path;

    /// Open the boundary. Nothing may have been written before this returns.
    fn open(&mut self, label: &str, request_id: &str) -> Result<Opened, String>;

    /// What the workspace shows changed since `open`. Observed, never
    /// remembered.
    fn changed(&mut self) -> Changes;

    /// The identity of the version a verdict taken now would be about.
    ///
    /// `Ok(None)` on a backend whose validations are about a mutable tree. An
    /// error is a backend that has candidates and could not freeze one — which
    /// is never a candidate that nothing is wrong with.
    fn candidate(&mut self) -> Result<Option<String>, String>;

    /// Keep the work: commit it, or keep a failure the caller asked to keep.
    fn keep(&mut self, receipt: &Receipt) -> Result<Kept, String>;

    /// Put the work back, authorised by the state this backend last observed.
    fn abandon(&mut self, receipt: &Receipt) -> Result<(), AbandonFailure>;

    /// The exact identity of what the tree is after settling.
    fn end_state(&mut self) -> Option<String>;

    /// What this backend did, for the evidence. Never read to decide anything.
    fn describe(&self) -> serde_json::Value;
}
