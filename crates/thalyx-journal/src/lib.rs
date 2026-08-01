//! The operation journal.
//!
//! Two properties this crate exists to enforce:
//!
//! 1. **Append-only.** Entries are written with `O_APPEND` and each write is
//!    flushed and fsynced before the call returns, so an entry that was
//!    reported as written survives a power loss.
//! 2. **Declared scope.** The journal records *only* operations Thalyx
//!    performed. It is not a complete record of what happened to the system,
//!    because the double-route principle guarantees the human can act without
//!    it. Anything that reads this journal must treat it accordingly.
//!
//! See `vault/04-Flujo-Canonico/Journal-y-Snapshots.md`.

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("journal entry {line} is corrupt: {source}")]
    Corrupt {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
}

/// What happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// **Not an outcome yet.** Written *before* the commit: "I am about to
    /// publish this". Every other variant is terminal and resolves it.
    ///
    /// This is what closes the gap where a crash immediately after the symlink
    /// swap left a module installed with no record that it ever happened. The
    /// intent is on disk before anything moves, so the worst case is an
    /// unresolved intent — which reconciliation settles against the disk —
    /// rather than a silent installation.
    Intended,
    /// The operation completed and was committed.
    Success,
    /// Rejected before anything physical happened. Nothing to undo.
    Rejected { reason: String },
    /// The artifact was produced but never published. Nothing to undo either,
    /// which is the whole point of build-then-commit.
    NotCommitted { reason: String },
    /// A non-critical step failed and the operation continued without it.
    Degraded { reason: String },
}

impl Outcome {
    /// Whether this outcome settles a request. `Intended` is the only one that
    /// does not.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Outcome::Intended)
    }
}

/// Where a contract field came from.
///
/// Recorded so that any executed action can be audited back to the source that
/// motivated it. See `vault/11-Seguridad/Marcado-de-Origen.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    UserUtterance,
    SystemState,
    UntrustedContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// RFC 3339, UTC.
    pub timestamp: String,
    pub operation: String,
    pub module_id: Option<String>,
    pub version: Option<String>,
    pub outcome: Outcome,
    pub request_id: String,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// An append-only journal backed by a JSON-lines file.
pub struct Journal {
    path: PathBuf,
}

impl Journal {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, JournalError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| JournalError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append an entry and make it durable before returning.
    ///
    /// The fsync is not optional: the journal is what a fault-injection test
    /// reads to decide whether an interrupted operation was recorded, so an
    /// entry sitting in a page cache would make those tests meaningless.
    pub fn append(&self, entry: &Entry) -> Result<(), JournalError> {
        let mut line = serde_json::to_string(entry).expect("entry is always serialisable");
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| JournalError::Io {
                path: self.path.clone(),
                source,
            })?;

        file.write_all(line.as_bytes())
            .map_err(|source| JournalError::Io {
                path: self.path.clone(),
                source,
            })?;
        file.sync_data().map_err(|source| JournalError::Io {
            path: self.path.clone(),
            source,
        })?;

        Ok(())
    }

    /// Intents that were never settled by a terminal entry.
    ///
    /// Each one is an operation the process announced and then died in the
    /// middle of. Whether it actually took effect is a question only the disk
    /// can answer, which is what reconciliation is for.
    pub fn unresolved_intents(&self) -> Result<Vec<Entry>, JournalError> {
        let entries = self.entries()?;

        let settled: std::collections::HashSet<&str> = entries
            .iter()
            .filter(|entry| entry.outcome.is_terminal())
            .map(|entry| entry.request_id.as_str())
            .collect();

        Ok(entries
            .iter()
            .filter(|entry| {
                entry.outcome == Outcome::Intended && !settled.contains(entry.request_id.as_str())
            })
            .cloned()
            .collect())
    }

    pub fn entries(&self) -> Result<Vec<Entry>, JournalError> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(JournalError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        };

        let mut entries = Vec::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(|source| JournalError::Io {
                path: self.path.clone(),
                source,
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let entry = serde_json::from_str(&line).map_err(|source| JournalError::Corrupt {
                line: index + 1,
                source,
            })?;
            entries.push(entry);
        }
        Ok(entries)
    }
}

/// Current time as an RFC 3339 string in UTC.
pub fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC 3339 formatting cannot fail for a valid timestamp")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(operation: &str, outcome: Outcome) -> Entry {
        Entry {
            timestamp: now(),
            operation: operation.to_string(),
            module_id: Some("org.publisher.demo".to_string()),
            version: Some("1.0.0".to_string()),
            outcome,
            request_id: "req-1".to_string(),
            origin: Origin::UserUtterance,
            snapshot: None,
            notes: vec![],
        }
    }

    #[test]
    fn appends_and_reads_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(dir.path().join("journal.jsonl")).unwrap();

        journal
            .append(&entry("install_module", Outcome::Success))
            .unwrap();
        journal
            .append(&entry(
                "install_module",
                Outcome::NotCommitted {
                    reason: "artifact digest mismatch".to_string(),
                },
            ))
            .unwrap();

        let entries = journal.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].outcome, Outcome::Success);
        assert!(matches!(entries[1].outcome, Outcome::NotCommitted { .. }));
    }

    #[test]
    fn missing_journal_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(dir.path().join("nothing-here.jsonl")).unwrap();
        assert!(journal.entries().unwrap().is_empty());
    }

    #[test]
    fn a_failed_attempt_is_recorded_not_erased() {
        // Build-then-commit means a failure leaves nothing to undo, but the
        // attempt must still be visible afterwards.
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(dir.path().join("journal.jsonl")).unwrap();
        journal
            .append(&entry(
                "install_module",
                Outcome::NotCommitted {
                    reason: "signature does not match".to_string(),
                },
            ))
            .unwrap();

        let entries = journal.entries().unwrap();
        assert_eq!(entries.len(), 1);
        match &entries[0].outcome {
            Outcome::NotCommitted { reason } => assert!(reason.contains("signature")),
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
}
