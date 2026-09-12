//! Every message a managed client and its service exchange.
//!
//! JSON, because both ends of the first transport are Rust and the messages are
//! small next to what a transaction costs, and because a person debugging a
//! store can read its conversation. Object bytes travel hex-encoded; what that
//! costs is visible in [`crate::transport::Carried`], which is where a later
//! binary encoding would have to justify itself.

use crate::authority::RequestId;
use serde::{Deserialize, Serialize};

/// The format a store speaks. A client that is answered with another one stops.
pub const FORMAT: &str = "thalyx-managed-local-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// A file's bytes.
    Bytes,
    /// An encoded tree: `crate::content::Manifest::encode`.
    Tree,
    /// A decision and what it was based on.
    Receipt,
    /// A run's whole record.
    Evidence,
}

impl ObjectKind {
    pub fn word(self) -> &'static str {
        match self {
            ObjectKind::Bytes => "bytes",
            ObjectKind::Tree => "tree",
            ObjectKind::Receipt => "receipt",
            ObjectKind::Evidence => "evidence",
        }
    }

    pub const ALL: [ObjectKind; 4] = [
        ObjectKind::Bytes,
        ObjectKind::Tree,
        ObjectKind::Receipt,
        ObjectKind::Evidence,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Hello,
    /// The published generation and its root.
    Published,
    /// Every generation this store has published, oldest first.
    History,
    /// Which of these `bytes` objects the store does not have.
    Missing {
        digests: Vec<String>,
    },
    Put {
        kind: ObjectKind,
        hex: String,
    },
    Get {
        digest: String,
    },
    /// The next sequence this principal may use.
    Sequence {
        principal: String,
    },
    /// Publish the first version of a store that has none.
    Seed {
        request: RequestId,
        root: String,
        receipt: String,
    },
    /// Open private work against a generation.
    Fork {
        request: RequestId,
        transaction: String,
        generation: u64,
    },
    /// Publish a candidate, if the generation is still the one the work forked.
    Publish {
        request: RequestId,
        transaction: String,
        expected_generation: u64,
        candidate: String,
        receipt: String,
    },
    /// What became of a request whose reply was never read.
    Result {
        request: RequestId,
    },
    /// Close private work without publishing it.
    Abandon {
        request: RequestId,
        transaction: String,
        candidate: Option<String>,
        receipt: String,
    },
    /// Name a kept evidence object by the transaction it records.
    KeepEvidence {
        transaction: String,
        digest: String,
    },
    FindEvidence {
        transaction: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Reply {
    Hello {
        format: String,
        store: String,
        epoch: u64,
    },
    Version {
        generation: u64,
        root: Option<String>,
    },
    History {
        roots: Vec<(u64, String)>,
    },
    Missing {
        digests: Vec<String>,
    },
    Stored {
        digest: String,
    },
    Object {
        kind: ObjectKind,
        hex: String,
    },
    Next {
        sequence: u64,
    },
    /// Durable: the log holds the commit.
    Committed {
        generation: u64,
        root: String,
    },
    Forked {
        generation: u64,
    },
    Abandoned,
    /// The generation moved on. Durable as an abort of this request.
    Stale {
        expected: u64,
        current: u64,
    },
    /// The request was resolved without effect. Durable.
    Aborted {
        reason: String,
    },
    /// Nothing is known about that request, which is not the same as it having
    /// been aborted.
    Unknown,
    Recorded,
    Found {
        digest: String,
    },
    Absent,
    Refused {
        word: String,
        message: String,
    },
}

pub fn encode<T: Serialize>(message: &T) -> Vec<u8> {
    serde_json::to_vec(message).expect("a managed message is always JSON")
}

pub fn decode<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|error| format!("not a managed message: {error}"))
}
