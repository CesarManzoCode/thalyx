//! `thalyx-memory` — what the agent remembers, and how much of it still holds.
//!
//! See `vault/03-Primitivas/Memoria-Persistente.md`. Two things it decrees
//! shape everything here.
//!
//! ## Two layers that cannot be mixed up
//!
//! **Facts** are what happened: a module was installed, a permission was
//! confirmed. **Notes** are what the agent inferred: a possible next step, a
//! reading of the situation. Both are worth keeping — an agent that stored
//! only facts would burn compute re-deriving context it had already worked out
//! — but they must never be presented as the same kind of thing.
//!
//! That separation is in the types rather than in a field. [`Recollection`]
//! hands back facts and notes in two different places, so producing a sentence
//! that mixes them takes a deliberate act rather than a slip. It is the same
//! technique as `Answer<T>` in the graph, where the rows cannot be had without
//! the freshness.
//!
//! ## A fact stops being checkable, and never becomes false
//!
//! The human can change anything without telling the agent — that is the
//! double-route principle. So a fact recorded yesterday may no longer be
//! verifiable today. It is marked [`Standing::Unverified`], with the paths
//! that moved, and it is **not deleted**: no longer checkable is not the same
//! as untrue, and an agent that quietly dropped the record would lose what it
//! knew rather than knowing it less certainly.
//!
//! ## Notes are discardable, facts are not
//!
//! There is [`Memory::forget_notes`] and there is no way to delete a fact.
//! Inference is the agent's; the record of what happened is the human's.

pub mod embed;
mod schema;
pub mod witness;

pub use embed::{Embedder, Embedding, LexicalEmbedder};
pub use witness::{Standing, Witness, WitnessedPath};

use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("memory database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error(
        "this memory was written with the `{stored}` embedder and is being read with \
         `{offered}`.\n  \
         The stored vectors mean nothing to a different embedder, and searching anyway \
         would return confident nonsense."
    )]
    DifferentEmbedder { stored: String, offered: String },
}

pub type Result<T> = std::result::Result<T, MemoryError>;

/// Which of the two layers a record belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// What happened. Verifiable, and never deleted.
    Fact,
    /// What the agent worked out. Discardable.
    Note,
}

impl Layer {
    fn as_str(self) -> &'static str {
        match self {
            Layer::Fact => "fact",
            Layer::Note => "note",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "fact" => Some(Layer::Fact),
            "note" => Some(Layer::Note),
            _ => None,
        }
    }
}

/// Something the agent recorded.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub id: i64,
    pub task: String,
    pub layer: Layer,
    pub text: String,
    /// Seconds since the Unix epoch.
    pub recorded_at: i64,
}

/// A fact, with whether it can still be checked.
#[derive(Debug, Clone, PartialEq)]
pub struct RecalledFact {
    pub record: Record,
    pub standing: Standing,
}

/// What the agent remembers about a task.
///
/// Facts and notes are separate fields, not a list with a tag. A caller that
/// wants to say "you installed X" and "maybe do Y" has to reach into two
/// different places to do it, which is exactly the friction the decree asks
/// for.
#[derive(Debug, Clone, Default)]
pub struct Recollection {
    pub facts: Vec<RecalledFact>,
    pub notes: Vec<Record>,
}

impl Recollection {
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty() && self.notes.is_empty()
    }

    /// Facts that still check out.
    pub fn verified(&self) -> impl Iterator<Item = &RecalledFact> {
        self.facts.iter().filter(|fact| fact.standing.is_verified())
    }

    /// Facts that no longer can be checked. Not wrong — unconfirmable.
    pub fn no_longer_verifiable(&self) -> impl Iterator<Item = &RecalledFact> {
        self.facts
            .iter()
            .filter(|fact| matches!(fact.standing, Standing::Unverified { .. }))
    }
}

/// A search result, carrying what kind of matching produced it.
///
/// The same shape as the graph's `Answer<T>`: rows cannot be had without the
/// caveat. Here the caveat is whether the embedder understood the text or only
/// counted its words.
#[derive(Debug, Clone)]
pub struct Recall {
    pub hits: Vec<Hit>,
    /// False when the matches are word overlap rather than meaning.
    pub semantic: bool,
    pub embedder: String,
}

impl Recall {
    /// A sentence to print beside the results, always.
    pub fn describe(&self) -> String {
        if self.semantic {
            format!("matched by meaning ({})", self.embedder)
        } else {
            format!(
                "matched by shared words, not by meaning ({}). Two records saying the \
                 same thing differently will not find each other.",
                self.embedder
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub record: Record,
    pub similarity: f32,
    /// Present for facts, absent for notes.
    pub standing: Option<Standing>,
}

/// The agent's memory across sessions.
pub struct Memory {
    connection: Connection,
}

impl Memory {
    pub fn open(path: &Path, embedder: &dyn Embedder) -> Result<Self> {
        let connection = Connection::open(path)?;
        Self::prepare(connection, embedder)
    }

    pub fn in_memory(embedder: &dyn Embedder) -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        Self::prepare(connection, embedder)
    }

    fn prepare(connection: Connection, embedder: &dyn Embedder) -> Result<Self> {
        schema::apply(&connection)?;

        // Vectors from one embedder mean nothing to another. Recording which
        // one wrote them turns "the results are subtly wrong" into an error.
        let stored: Option<String> = connection
            .query_row("SELECT value FROM meta WHERE key = 'embedder'", [], |row| {
                row.get(0)
            })
            .ok();

        match stored {
            Some(stored) if stored != embedder.name() => {
                return Err(MemoryError::DifferentEmbedder {
                    stored,
                    offered: embedder.name().to_string(),
                });
            }
            Some(_) => {}
            None => {
                connection.execute(
                    "INSERT INTO meta (key, value) VALUES ('embedder', ?1)",
                    [embedder.name()],
                )?;
            }
        }

        Ok(Self { connection })
    }

    /// Record something that happened, and what it was checked against.
    pub fn remember_fact(
        &self,
        task: &str,
        text: &str,
        witness: &Witness,
        embedder: &dyn Embedder,
    ) -> Result<i64> {
        let id = self.insert(task, Layer::Fact, text, embedder)?;

        for path in &witness.paths {
            self.connection.execute(
                "INSERT INTO witnessed (record, path, size, mtime_ns, existed)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    id,
                    path.path,
                    path.size as i64,
                    path.mtime_ns,
                    path.existed as i64
                ],
            )?;
        }

        Ok(id)
    }

    /// Record something the agent worked out. Discardable by design.
    pub fn note(&self, task: &str, text: &str, embedder: &dyn Embedder) -> Result<i64> {
        self.insert(task, Layer::Note, text, embedder)
    }

    fn insert(&self, task: &str, layer: Layer, text: &str, embedder: &dyn Embedder) -> Result<i64> {
        let vector = embedder.embed(text).to_bytes();
        self.connection.execute(
            "INSERT INTO records (task, layer, text, recorded_at, vector)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![task, layer.as_str(), text, now(), vector],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Everything remembered about a task, with each fact re-checked now.
    pub fn recall(&self, task: &str) -> Result<Recollection> {
        let mut statement = self.connection.prepare(
            "SELECT id, task, layer, text, recorded_at FROM records
             WHERE task = ?1 ORDER BY recorded_at, id",
        )?;

        let rows = statement.query_map([task], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;

        let mut collected = Vec::new();
        for row in rows {
            collected.push(row?);
        }

        let mut recollection = Recollection::default();
        for (id, task, layer, text, recorded_at) in collected {
            let Some(layer) = Layer::parse(&layer) else {
                continue;
            };
            let record = Record {
                id,
                task,
                layer,
                text,
                recorded_at,
            };

            match layer {
                Layer::Note => recollection.notes.push(record),
                Layer::Fact => {
                    let standing = self.witness_of(id)?.standing();
                    recollection.facts.push(RecalledFact { record, standing });
                }
            }
        }

        Ok(recollection)
    }

    /// Find records that look like this text.
    ///
    /// Exact search over every vector. See `embed.rs` for why it is not
    /// approximate, and why the answer says what kind of matching it was.
    pub fn search(&self, query: &str, limit: usize, embedder: &dyn Embedder) -> Result<Recall> {
        let wanted = embedder.embed(query);

        let mut statement = self
            .connection
            .prepare("SELECT id, task, layer, text, recorded_at, vector FROM records")?;

        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })?;

        let mut collected = Vec::new();
        for row in rows {
            collected.push(row?);
        }

        let mut hits = Vec::new();
        for (id, task, layer, text, recorded_at, vector) in collected {
            let (Some(layer), Some(vector)) =
                (Layer::parse(&layer), Embedding::from_bytes(&vector))
            else {
                continue;
            };

            let similarity = wanted.similarity(&vector);
            // Zero means nothing in common, or vectors that cannot be compared
            // at all. Neither is a match, and returning them padded the list
            // with noise ranked above nothing.
            if similarity <= 0.0 {
                continue;
            }

            let standing = match layer {
                Layer::Fact => Some(self.witness_of(id)?.standing()),
                Layer::Note => None,
            };

            hits.push(Hit {
                record: Record {
                    id,
                    task,
                    layer,
                    text,
                    recorded_at,
                },
                similarity,
                standing,
            });
        }

        hits.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
                // A stable tie-break, so two runs over the same data agree.
                .then(a.record.id.cmp(&b.record.id))
        });
        hits.truncate(limit);

        Ok(Recall {
            hits,
            semantic: embedder.is_semantic(),
            embedder: embedder.name().to_string(),
        })
    }

    /// Drop the agent's inferences for a task. Facts are untouched.
    pub fn forget_notes(&self, task: &str) -> Result<usize> {
        Ok(self.connection.execute(
            "DELETE FROM records WHERE task = ?1 AND layer = 'note'",
            [task],
        )?)
    }

    /// Every task with something remembered about it.
    pub fn tasks(&self) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT DISTINCT task FROM records ORDER BY task")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn witness_of(&self, record: i64) -> Result<Witness> {
        let mut statement = self
            .connection
            .prepare("SELECT path, size, mtime_ns, existed FROM witnessed WHERE record = ?1")?;
        let rows = statement.query_map([record], |row| {
            Ok(WitnessedPath {
                path: row.get(0)?,
                size: row.get::<_, i64>(1)? as u64,
                mtime_ns: row.get(2)?,
                existed: row.get::<_, i64>(3)? != 0,
            })
        })?;
        Ok(Witness {
            paths: rows.collect::<rusqlite::Result<Vec<_>>>()?,
        })
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
