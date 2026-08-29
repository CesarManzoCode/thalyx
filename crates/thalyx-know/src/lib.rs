//! What this machine knows about a tree, kept between the sessions that ask.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md` says the point of every
//! primitive here is to move work off the model's context and onto the machine.
//! The index did that for one question. This does it for **the answers
//! themselves**: a session that resolved a symbol, listed a workspace's
//! packages or compiled a crate has learned something the next session
//! otherwise pays for again, and the cost of paying again is not the CPU — it
//! is the round trip and the context that comes with it.
//!
//! ## The whole design is one rule
//!
//! **A remembered answer carries the identity of the state it was derived
//! from.** Recalling it compares that identity against the state now and says
//! one of three things:
//!
//! - `Current` — the inputs are byte for byte what they were.
//! - `Stale` — they are not, and here is the answer anyway, marked.
//! - `Unknown` — nothing was ever remembered about this.
//!
//! There is no fourth answer and no way to get the value without the standing,
//! which is the point: `Estrategia-de-Pruebas` rule 9 says the corrupt case must
//! produce the cautious answer, and a cache whose `get` returns a bare value is
//! a cache that hands out a stale answer to every caller who forgot to ask.
//!
//! ## What this is not
//!
//! It is not a database service, not a build cache and not a second index.
//! One SQLite file per tree, one table, three verbs. Nothing here knows what
//! Rust is: the *kinds* of fact and the meaning of their witnesses belong to
//! whoever remembers them — see `thalyx-rust` for the first such provider.
//!
//! `false miss = slower, false hit = wrong`, so every doubt resolves towards
//! the miss: an unreadable input makes a witness incomplete, and an incomplete
//! witness matches nothing at all.

pub mod witness;

pub use witness::{Over, Witness, witness, woven};

use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum KnowError {
    #[error("the knowledge store could not be opened: {0}")]
    Open(String),
    #[error("the knowledge store could not be read: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, KnowError>;

/// Whether a remembered answer still describes the tree it was derived from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// The inputs are exactly what they were when this was learned.
    Current,
    /// They are not. Both identities are carried so an answer can say so.
    Stale { was: String, now: String },
    /// Nothing was remembered about this.
    Unknown,
}

impl Standing {
    /// The word a machine-facing answer matches on.
    pub fn word(&self) -> &'static str {
        match self {
            Standing::Current => "current",
            Standing::Stale { .. } => "stale",
            Standing::Unknown => "unknown",
        }
    }

    pub fn is_current(&self) -> bool {
        matches!(self, Standing::Current)
    }
}

/// A remembered answer and everything needed to decide whether to believe it.
#[derive(Debug, Clone)]
pub struct Held {
    pub value: String,
    pub standing: Standing,
    /// Who learned it — `rust-analyzer`, `cargo`, `index`. Reported to the
    /// caller, because "exact, from a compiler" and "guessed, from a scan" are
    /// different facts and a surface that conflated them would be lying by
    /// omission.
    pub source: String,
    /// Seconds since the epoch. For display only: nothing here decides
    /// anything from a clock, which is the mistake of 2026-08-29.
    pub recorded_at: i64,
}

/// Everything one tree's machine has learned about it.
pub struct Knowledge {
    connection: Connection,
}

impl Knowledge {
    /// Open, creating the file and the table if they are not there.
    pub fn open(database: &Path) -> Result<Self> {
        if let Some(parent) = database.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| KnowError::Open(format!("{}: {error}", parent.display())))?;
        }
        let connection = Connection::open(database)?;
        apply(&connection)?;
        Ok(Self { connection })
    }

    /// A store that exists only for the life of this process. Tests, and the
    /// case where the store's disk is not writable — a machine that cannot
    /// remember still has to be able to answer.
    pub fn in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        apply(&connection)?;
        Ok(Self { connection })
    }

    /// Learn something, replacing whatever was known about the same key.
    ///
    /// One row per `(kind, key)` and not a history: this is a cache, and a
    /// cache that keeps every version it has ever held is a log nobody reads
    /// that grows without bound. The journal is where history lives.
    pub fn remember(
        &self,
        kind: &str,
        key: &str,
        witness: &Witness,
        source: &str,
        value: &str,
    ) -> Result<()> {
        // An incomplete witness is refused rather than stored. Storing it would
        // create a row that can never be `Current` — dead weight that looks
        // like a cache entry and is really a permanent miss.
        if !witness.is_complete() {
            return Ok(());
        }
        self.connection.execute(
            "INSERT INTO facts (kind, key, witness, source, value, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (kind, key) DO UPDATE SET
                witness = excluded.witness,
                source = excluded.source,
                value = excluded.value,
                recorded_at = excluded.recorded_at",
            rusqlite::params![kind, key, witness.id, source, value, now()],
        )?;
        Ok(())
    }

    /// What is known about a key, and whether it still holds.
    ///
    /// The standing is computed here and not by the caller, because a caller
    /// that compares witnesses itself is a caller that can forget to.
    pub fn recall(&self, kind: &str, key: &str, against: &Witness) -> Result<Option<Held>> {
        let mut statement = self.connection.prepare(
            "SELECT witness, source, value, recorded_at FROM facts WHERE kind = ?1 AND key = ?2",
        )?;
        let mut rows = statement.query(rusqlite::params![kind, key])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let was: String = row.get(0)?;
        let standing = if against.matches(&was) {
            Standing::Current
        } else {
            Standing::Stale {
                was,
                now: against.id.clone(),
            }
        };
        Ok(Some(Held {
            value: row.get(2)?,
            standing,
            source: row.get(1)?,
            recorded_at: row.get(3)?,
        }))
    }

    /// The answer only if it is still true, which is what a cache is asked.
    ///
    /// Spelled as its own call rather than left to every caller to write out of
    /// [`recall`], so that "reuse this" is one expression that cannot
    /// accidentally reuse a stale one.
    pub fn recall_current(&self, kind: &str, key: &str, against: &Witness) -> Result<Option<Held>> {
        Ok(self
            .recall(kind, key, against)?
            .filter(|held| held.standing.is_current()))
    }

    /// Forget one fact. Used when something is known to be wrong rather than
    /// merely old — a witness cannot express "this answer was a bug".
    pub fn forget(&self, kind: &str, key: &str) -> Result<usize> {
        Ok(self.connection.execute(
            "DELETE FROM facts WHERE kind = ?1 AND key = ?2",
            rusqlite::params![kind, key],
        )?)
    }

    /// Forget everything of one kind.
    pub fn forget_kind(&self, kind: &str) -> Result<usize> {
        Ok(self
            .connection
            .execute("DELETE FROM facts WHERE kind = ?1", rusqlite::params![kind])?)
    }

    /// How many facts of each kind are held, for an answer that reports what
    /// the machine is carrying without printing any of it.
    pub fn counts(&self) -> Result<Vec<(String, usize)>> {
        let mut statement = self
            .connection
            .prepare("SELECT kind, COUNT(*) FROM facts GROUP BY kind ORDER BY kind")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;
        let mut counts = Vec::new();
        for row in rows {
            counts.push(row?);
        }
        Ok(counts)
    }

    /// Every key of one kind, whatever its standing. For a surface that lists
    /// what is known without deciding anything about it.
    pub fn keys(&self, kind: &str) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT key FROM facts WHERE kind = ?1 ORDER BY key")?;
        let rows = statement.query_map(rusqlite::params![kind], |row| row.get::<_, String>(0))?;
        let mut keys = Vec::new();
        for row in rows {
            keys.push(row?);
        }
        Ok(keys)
    }
}

fn apply(connection: &Connection) -> rusqlite::Result<()> {
    // WAL for the same reason the index uses it: a reader is never blocked by a
    // writer, and a reader that sees the previous contents is reading something
    // whose standing it is about to be told.
    let _ = connection.pragma_update(None, "journal_mode", "WAL");
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS facts (
            -- What sort of answer this is: `packages`, `symbol`, `validation`.
            -- Owned by whoever remembers it; nothing here interprets it.
            kind        TEXT NOT NULL,
            key         TEXT NOT NULL,
            -- The identity of the state this was derived from. The whole
            -- mechanism: without it a row is a guess with a timestamp on it.
            witness     TEXT NOT NULL,
            source      TEXT NOT NULL,
            value       TEXT NOT NULL,
            recorded_at INTEGER NOT NULL,
            PRIMARY KEY (kind, key)
        );
        "#,
    )
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}
