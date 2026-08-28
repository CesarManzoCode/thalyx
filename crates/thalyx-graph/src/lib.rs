//! The semantic index: files as nodes, dependencies as edges.
//!
//! One property governs this whole crate: **the index is a cache, and the
//! filesystem is the truth.** The double-route principle guarantees the human
//! can move files with plain POSIX tools, so an index that presented itself as
//! authoritative would be lying by construction.
//!
//! Every query therefore returns its answer *together with* whether the index
//! is current. Callers cannot accidentally read stale data believing it fresh,
//! because the type makes them handle the freshness alongside the rows.
//!
//! See `vault/03-Primitivas/FS-en-Grafo.md` and
//! `vault/04-Flujo-Canonico/Coherencia-Doble-Ruta.md`.

mod schema;
mod staleness;
pub mod watch;

pub use staleness::{Freshness, Staleness};
pub use watch::{Coverage, MutationCounter, Trust, Verification, Watcher};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thalyx_parser::{Language, ReferenceKind};

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("index database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("root `{0}` is not a directory")]
    RootNotADirectory(PathBuf),

    #[error(
        "the index database `{database}` sits inside the tree it indexes (`{root}`).\n  \
         The index would record its own files, and writing to it would immediately \
         invalidate the index it just wrote."
    )]
    DatabaseInsideTree { database: PathBuf, root: PathBuf },

    #[error(
        "`{root}` holds more than {ceiling} files worth indexing.\n  \
         Indexing it would take long enough that nobody would wait for the \
         answer. Name a smaller tree."
    )]
    TreeTooLarge { root: PathBuf, ceiling: usize },
}

/// How many files an index will take before refusing to build.
///
/// One number for every whole-tree walk in Thalyx, kept in `thalyx-files` where
/// the walk itself is. Two ceilings that had to agree would have drifted the
/// first time one of them was tuned.
pub use thalyx_files::CEILING;

/// How large a tree a query may rebuild the index for on its own.
///
/// Not the same number as [`CEILING`], and deliberately much smaller. `CEILING`
/// answers *is this tree worth indexing at all*; this answers *may a question
/// silently do it*, and the difference is that somebody is waiting for the
/// answer with no idea a rebuild is happening.
///
/// Picked from a measurement rather than from taste: indexing this repository —
/// 250 files, 5 400 declared names, 57 000 mentions — takes a little under half
/// a second, which is about 2 ms a file. Two thousand files is therefore a few
/// seconds, which is the most a question may cost before the caller should have
/// been told instead of made to wait. Above it the answer says the index is
/// stale, by how much, and that `index_build` is the thing to call — which is
/// the same conversation as before, held only where it is actually needed.
pub const AUTO_REFRESH_CEILING: usize = 2_000;

/// What a query did about an index that had fallen behind.
///
/// Four cases and not a boolean, because the three that are not "it was fine"
/// need different things from the caller: nothing, patience, or a call to
/// `index_build`. A boolean would have collapsed the last two into the one
/// answer a caller cannot act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refreshed {
    /// The index already matched the tree.
    NotNeeded,
    /// It did not, and now it does.
    Rebuilt {
        was: Staleness,
        took_ms: u128,
        report: BuildReport,
    },
    /// It did not, and rebuilding it is too much to do inside a question.
    Declined {
        estimated_files: usize,
        ceiling: usize,
        was: Staleness,
    },
    /// It did not, and the rebuild could not be done.
    Failed { was: Staleness, why: String },
}

impl Refreshed {
    /// The word a program matches on. Stable, never translated.
    pub fn word(&self) -> &'static str {
        match self {
            Refreshed::NotNeeded => "not_needed",
            Refreshed::Rebuilt { .. } => "rebuilt",
            Refreshed::Declined { .. } => "declined_too_large",
            Refreshed::Failed { .. } => "failed",
        }
    }

    /// One line saying what happened and, where there is one, what to do.
    pub fn describe(&self) -> String {
        match self {
            Refreshed::NotNeeded => "the index was already current".to_string(),
            Refreshed::Rebuilt {
                was,
                took_ms,
                report,
            } => format!(
                "the index was stale ({} added, {} modified, {} removed) and was rebuilt \
                 in {took_ms} ms: {} files, {} names",
                was.added.len(),
                was.modified.len(),
                was.removed.len(),
                report.files_indexed,
                report.symbols
            ),
            Refreshed::Declined {
                estimated_files,
                ceiling,
                was,
            } => format!(
                "the index is stale ({} files differ) and this tree holds about \
                 {estimated_files} files, more than the {ceiling} a question rebuilds on \
                 its own. Call `index_build` to rebuild it.",
                was.total()
            ),
            Refreshed::Failed { was, why } => format!(
                "the index is stale ({} files differ) and could not be rebuilt: {why}",
                was.total()
            ),
        }
    }
}

pub type Result<T> = std::result::Result<T, GraphError>;

/// A file in the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// Path relative to the indexed root, so the index survives being moved.
    pub path: String,
    pub language: Option<String>,
    pub size: u64,
    pub tags: Vec<String>,
}

/// A dependency from one file to another, or to something outside the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    /// The reference exactly as written in the source, or — for a [`Via::Symbol`]
    /// edge — the name that was used.
    pub raw_target: String,
    /// The file it resolves to, when it resolves inside the tree at all.
    pub to: Option<String>,
    pub line: usize,
    /// How the edge was learned. Never merged away: the two are different
    /// evidence and a caller that could not tell them apart would either
    /// distrust both or trust both.
    pub via: Via,
}

/// How an edge came to be known.
///
/// The distinction exists because of a defect found by running the system, on
/// 2026-08-28: asked what depends on `src/store.rs`, the index answered with the
/// two files that write `use crate::store::…` and missed a third that reaches
/// the same code as `server.store.save()`. `grep` found it. The index already
/// held the evidence — `save` is a declared name and the mention was recorded —
/// and simply never turned it into an edge.
///
/// So `dependencies` used to mean **imports**, while the word an agent reads it
/// as is *everything that would break*. Rather than rename the primitive into
/// something narrower, the missing half is now recorded, and each row says which
/// half it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// The file declared it: `use`, `mod`, `import`, `#include`, `require`.
    /// What the file says about itself, and true regardless of what else exists.
    Import,
    /// The file used a name that exactly one file in this tree declares.
    ///
    /// Weaker evidence than an import and deliberately so: it is a fact about
    /// the tree as a whole, and it stops being true if a second file starts
    /// declaring the same name. The uniqueness requirement is what keeps it
    /// precise — see [`Index::build`].
    Symbol,
}

impl Via {
    /// The word a program matches on. Stable, never translated.
    pub fn word(self) -> &'static str {
        match self {
            Via::Import => "import",
            Via::Symbol => "symbol",
        }
    }

    fn from_word(word: &str) -> Self {
        // Fails closed, rule 9: a row written by a version that does not exist
        // yet is reported as the weaker evidence, never as the stronger one.
        match word {
            "import" => Via::Import,
            _ => Via::Symbol,
        }
    }
}

/// A query result, inseparable from the index's freshness at the time.
///
/// Returning these together is deliberate: a caller that wants the rows has to
/// receive the caveat too. Making freshness a separate call would let it be
/// forgotten, which is exactly how a cache starts being mistaken for the truth.
#[derive(Debug, Clone)]
pub struct Answer<T> {
    pub rows: T,
    pub freshness: Freshness,
}

impl<T> Answer<T> {
    /// The rows, accepting explicitly that they may be out of date.
    pub fn regardless_of_freshness(self) -> T {
        self.rows
    }
}

/// Result of indexing a tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildReport {
    pub files_indexed: usize,
    pub files_parsed: usize,
    pub edges: usize,
    pub edges_resolved: usize,
    pub skipped: usize,
    /// Names this tree declares.
    pub symbols: usize,
    /// Places one of those names is used, not counting where it was declared.
    pub mentions: usize,
    /// Of `edges`, how many were learned from a uniquely-declared name rather
    /// than from an import. Reported because it is the number that says whether
    /// this tree is one where imports tell the whole story — and a caller that
    /// only saw the total could not tell a tree with none from a build that
    /// stopped recording them.
    pub edges_via_symbol: usize,
}

/// A name, and where it comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    /// `function`, `type` or `constant`.
    pub kind: String,
    pub path: String,
    pub line: usize,
}

/// A place a name is used.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Use {
    pub path: String,
    pub line: usize,
}

/// Everything the index knows about one name.
///
/// The two lists are kept apart rather than merged into "occurrences", because
/// *where this comes from* and *where this is used* are different questions and
/// a caller that got one list has to guess which rows answer which. That guess
/// is the ambiguity cost `Superficie-para-el-LLM.md` names third.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub definitions: Vec<Symbol>,
    pub uses: Vec<Use>,
}

pub struct Index {
    connection: Connection,
    root: PathBuf,
}

impl Index {
    /// Open, or create, the index for a tree.
    pub fn open(database: &Path, root: &Path) -> Result<Self> {
        if !root.is_dir() {
            return Err(GraphError::RootNotADirectory(root.to_path_buf()));
        }

        // Refuse to put the index inside what it indexes.
        //
        // A cache that is part of its own input can never be current: writing
        // it changes the tree, which makes it stale, which is only fixed by
        // writing it again. Checking here makes that structural rather than a
        // naming convention someone can get wrong — which is exactly how it
        // was got wrong the first time.
        let absolute_database = if database.is_absolute() {
            database.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(database))
                .unwrap_or_else(|_| database.to_path_buf())
        };
        if let Ok(canonical_root) = root.canonicalize()
            && absolute_database.starts_with(&canonical_root)
        {
            return Err(GraphError::DatabaseInsideTree {
                database: absolute_database,
                root: canonical_root,
            });
        }

        if let Some(parent) = database.parent() {
            std::fs::create_dir_all(parent).map_err(|source| GraphError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let connection = Connection::open(database)?;
        schema::apply(&connection)?;

        Ok(Self {
            connection,
            root: root.to_path_buf(),
        })
    }

    /// In-memory index, for tests.
    pub fn in_memory(root: &Path) -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        schema::apply(&connection)?;
        Ok(Self {
            connection,
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Rebuild the whole index from the tree.
    ///
    /// A full sweep rather than an incremental update: deterministic, easy to
    /// reason about, and fast enough for a project of the size Phase 1
    /// targets. The event-driven path that the LSM will feed is an
    /// optimisation on top, not a replacement — this stays the reconciliation
    /// that runs at boot and on demand.
    pub fn build(&mut self) -> Result<BuildReport> {
        let mut report = BuildReport::default();
        let transaction = self.connection.transaction()?;

        transaction.execute("DELETE FROM edges", [])?;
        transaction.execute("DELETE FROM nodes", [])?;
        transaction.execute("DELETE FROM symbols", [])?;
        transaction.execute("DELETE FROM mentions", [])?;

        let mut files = Vec::new();
        for entry in walk(&self.root) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    report.skipped += 1;
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(relative) = entry.path().strip_prefix(&self.root) else {
                continue;
            };
            files.push((
                entry.path().to_path_buf(),
                relative.to_string_lossy().into_owned(),
            ));
            // Refused here rather than after the walk, so a tree of a million
            // files costs a moment and not a minute. The count that comes back
            // is therefore "more than the ceiling" and never a total — which is
            // all a caller needs to do the one useful thing, name something
            // smaller, and is the only number this stopped early enough to know.
            //
            // The transaction has already emptied the tables and has not been
            // committed; dropping it here rolls that back, so a refused rebuild
            // leaves the index that was there exactly as it was.
            if files.len() > CEILING {
                return Err(GraphError::TreeTooLarge {
                    root: self.root.clone(),
                    ceiling: CEILING,
                });
            }
        }
        files.sort_by(|a, b| a.1.cmp(&b.1));

        // Two passes: every node has to exist before edges can resolve against
        // them, otherwise resolution would depend on directory traversal order.
        let mut pending_edges = Vec::new();
        // Every name this tree declares, and exactly where. The first decides
        // which identifiers are worth recording at all; the second keeps a
        // declaration from being counted as a use of itself.
        let mut defined: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut declared_at: std::collections::HashSet<(String, String, usize)> =
            std::collections::HashSet::new();
        // Which file declares each name **and lets another file see it** — or
        // `None` once a second file does the same.
        //
        // The two conditions are the whole precision guard for symbol edges,
        // and both were paid for.
        //
        // *Exactly one* file: a name only one file declares can be pointed at
        // without a compiler, because a use of it resolves there or resolves
        // nowhere in this tree. Two declarations make it a guess, and a guess
        // is what an index must not put in a dependency list.
        //
        // *Exported*: the first version had only the rule above, and asked what
        // depends on `thalyx-snapshot/src/lib.rs` it answered with thirty-three
        // files. That crate declares `fn place` and `fn relative` — both
        // private, both ordinary words — so every file in the repository
        // holding a `let relative = …` was reported as a dependent. A private
        // name cannot be referred to from another file: that is not a heuristic
        // about the code, it is the language saying the edge is impossible.
        // Found by running the index over this repository and reading the rows,
        // which is where the other three precision defects came from too.
        let mut declared_uniquely_in: std::collections::HashMap<String, Option<String>> =
            std::collections::HashMap::new();
        // Every name each file declares, at any visibility. A file that has its
        // own private `validate_name` and calls it was being reported as
        // depending on the one other crate that happens to declare a public
        // one — because only exported declarations are candidates, so the
        // private one next door was not there to make the name ambiguous. A
        // file calling something it declares itself is never reaching outward.
        let mut declares: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();

        for (absolute, relative) in &files {
            let metadata = match std::fs::metadata(absolute) {
                Ok(metadata) => metadata,
                Err(_) => {
                    report.skipped += 1;
                    continue;
                }
            };
            let language = Language::from_path(absolute);

            transaction
                .prepare_cached(
                    "INSERT OR REPLACE INTO nodes (path, language, size, mtime_ns) \
                     VALUES (?1, ?2, ?3, ?4)",
                )?
                .execute(rusqlite::params![
                    relative,
                    language.map(|l| l.name()),
                    metadata.len() as i64,
                    staleness::mtime_nanos(&metadata),
                ])?;
            report.files_indexed += 1;

            let Some(language) = language else { continue };
            let Ok(source) = std::fs::read_to_string(absolute) else {
                // Unreadable or non-UTF-8: it stays a node with no edges rather
                // than being dropped, so the graph does not silently lose files.
                report.skipped += 1;
                continue;
            };

            report.files_parsed += 1;
            for reference in thalyx_parser::parse(language, &source) {
                pending_edges.push((relative.clone(), reference));
            }

            for found in thalyx_parser::definitions(language, &source) {
                // `prepare_cached` rather than `execute`, here and for the
                // mentions below, because `execute` compiles the statement
                // again for every row and these two are the rows there are tens
                // of thousands of. Priced by swapping just these two back:
                // `crates/` — 305 files, 5 524 names, 58 390 mentions — takes
                // 588.2 ms with `execute` and 480.4 ms with this, best of seven
                // in release. It is not a microoptimisation looking for a
                // reason; it is what pays for most of what the symbol edges
                // below cost.
                transaction
                    .prepare_cached(
                        "INSERT INTO symbols (name, kind, path, line) VALUES (?1, ?2, ?3, ?4)",
                    )?
                    .execute(rusqlite::params![
                        found.name,
                        found.kind.word(),
                        relative,
                        found.line as i64
                    ])?;
                report.symbols += 1;
                defined.insert(found.name.clone());
                if found.exported {
                    match declared_uniquely_in.entry(found.name.clone()) {
                        std::collections::hash_map::Entry::Vacant(slot) => {
                            slot.insert(Some(relative.clone()));
                        }
                        std::collections::hash_map::Entry::Occupied(mut slot) => {
                            // A name declared twice *in the same file* — a Rust
                            // `fn` and a `const` of the same name, or two `impl`
                            // blocks — is still one place to look. It is only a
                            // second *file* that makes the name ambiguous.
                            if slot.get().as_deref() != Some(relative.as_str()) {
                                slot.insert(None);
                            }
                        }
                    }
                }
                declares.insert((relative.clone(), found.name.clone()));
                declared_at.insert((relative.clone(), found.name, found.line));
            }
        }

        // The second read of every file, and the reason it is a second read
        // rather than a second use of what pass one held: recording which names
        // are *used* needs to know which names this tree *defines*, and that set
        // is only complete once every file has been seen. Keeping every
        // identifier of every file in memory until then would make indexing a
        // large tree a memory question, which is a worse trade than reading the
        // files again out of the page cache.
        // One row per (file, name), because a file that calls `save` in nine
        // places depends on where `save` lives once. Nine rows would make a
        // dependent count meaningless, which is the same reason the import
        // edges are deduped by (file, target).
        let mut symbol_edges: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut pending_symbol_edges: Vec<(String, String, String, usize)> = Vec::new();

        for (absolute, relative) in &files {
            let Some(language) = Language::from_path(absolute) else {
                continue;
            };
            let Ok(source) = std::fs::read_to_string(absolute) else {
                continue;
            };

            // Both questions over one scan of the file. Asked separately they
            // scrub every file twice, which measured at 53 ms on `crates/` of
            // this repository — 533.7 ms against 480.4 ms.
            //
            // The second is what this file makes up for itself. A name it binds
            // is its own binding, and its own binding shadows anything outside —
            // so a symbol edge for it would be about a name nobody is using.
            let (mentioned, bound_here) =
                thalyx_parser::identifiers_and_bindings(language, &source);

            let mut seen_here = std::collections::HashSet::new();
            for (name, line) in mentioned {
                if !defined.contains(&name) {
                    continue;
                }
                // The line a name is declared on mentions it too, and counting
                // that as a use would make every symbol look like it has one
                // more caller than it has — including the ones that have none,
                // which is the answer somebody deletes code on.
                if declared_at.contains(&(relative.clone(), name.clone(), line)) {
                    continue;
                }
                // Once per line. A name used three times on one line is one
                // place to look, and three rows would cost a caller three times
                // the tokens to learn the same thing.
                if !seen_here.insert((name.clone(), line)) {
                    continue;
                }
                transaction
                    .prepare_cached("INSERT INTO mentions (name, path, line) VALUES (?1, ?2, ?3)")?
                    .execute(rusqlite::params![name, relative, line as i64])?;
                report.mentions += 1;

                // The half of "what depends on this" that imports cannot see.
                //
                // Found by running the system: `server.store.save()` in a third
                // file made that file a dependent of `src/store.rs`, and asked
                // what depended on `store.rs` the index named only the two files
                // that wrote `use crate::store::…`. The evidence was already
                // here — this loop had just recorded the mention — and nothing
                // turned it into an edge.
                //
                // Only for a name exactly one file declares, and never back at
                // the file that declares it. Both conditions are precision and
                // not tidiness: without the first the edge is a guess, and
                // without the second every file would depend on itself.
                if let Some(Some(declared_in)) = declared_uniquely_in.get(&name)
                    && declared_in != relative
                    && !declares.contains(&(relative.clone(), name.clone()))
                    && !bound_here.contains(&name)
                    && symbol_edges.insert((relative.clone(), name.clone()))
                {
                    pending_symbol_edges.push((relative.clone(), name, declared_in.clone(), line));
                }
            }
        }

        let known: std::collections::HashSet<&str> = files
            .iter()
            .map(|(_, relative)| relative.as_str())
            .collect();

        // One edge per (file, target), keeping the first line it appeared on.
        // The parser honestly reports every occurrence; the graph is about
        // dependencies, and a file importing the same module twice depends on
        // it once. Without this, "who depends on auth" would count duplicates.
        let mut seen_edges = std::collections::HashSet::new();
        let pending_edges: Vec<_> = pending_edges
            .into_iter()
            .filter(|(from, reference)| seen_edges.insert((from.clone(), reference.target.clone())))
            .collect();

        // Which file-to-file dependencies the imports already state. A symbol
        // edge that lands on one of these says nothing new — the file declared
        // the dependency itself, which is the stronger evidence — and every such
        // row would be a second line about the same fact in an answer whose
        // whole purpose is to cost less than reading the files.
        let mut stated_by_import: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();

        for (from, reference) in pending_edges {
            let resolved = resolve(&from, &reference.target, reference.kind, &known);
            if let Some(target) = &resolved {
                stated_by_import.insert((from.clone(), target.clone()));
            }
            transaction
                .prepare_cached(
                    "INSERT INTO edges (from_path, raw_target, to_path, line, via) \
                     VALUES (?1, ?2, ?3, ?4, 'import')",
                )?
                .execute(rusqlite::params![
                    from,
                    reference.target,
                    resolved,
                    reference.line as i64
                ])?;
            report.edges += 1;
            if resolved.is_some() {
                report.edges_resolved += 1;
            }
        }

        // What is left is exactly the interesting set: a file that reaches
        // another file's code without ever naming that file. Field access, a
        // method on a type it was handed, a re-export it went through, a trait
        // bound — the cases an import list cannot show and a caller used to have
        // to find with `grep`.
        for (from, name, to, line) in pending_symbol_edges {
            if stated_by_import.contains(&(from.clone(), to.clone())) {
                continue;
            }
            transaction
                .prepare_cached(
                    "INSERT INTO edges (from_path, raw_target, to_path, line, via) \
                     VALUES (?1, ?2, ?3, ?4, 'symbol')",
                )?
                .execute(rusqlite::params![from, name, to, line as i64])?;
            report.edges += 1;
            report.edges_resolved += 1;
            report.edges_via_symbol += 1;
        }

        transaction.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('built_at_files', ?1)",
            [report.files_indexed as i64],
        )?;

        transaction.commit()?;
        Ok(report)
    }

    /// Whether the index still matches the tree.
    pub fn freshness(&self) -> Result<Freshness> {
        staleness::check(&self.connection, &self.root)
    }

    /// Bring the index up to date if it is behind, when that is cheap enough.
    ///
    /// ## The turn this exists to delete
    ///
    /// Found in the first real run of an external agent, 2026-08-28. Asked a
    /// question about a tree it had just changed, Claude got an answer that did
    /// not match what it had done, worked out that the answer carried
    /// `fresh: stale`, called `thalyx_state`, called `thalyx_index`, and asked
    /// its question again. Four turns, three of them spent on the index's
    /// bookkeeping rather than on the task.
    ///
    /// Every one of those turns was the caller doing something Thalyx could
    /// have done itself, and doing it with a model's attention — the most
    /// expensive thing in the loop. The freshness field was never the problem;
    /// *making the caller act on it* was.
    ///
    /// ## What is still not decided here
    ///
    /// The honesty rule of [[FS-en-Grafo]] does not move: the answer still
    /// carries what the freshness was, what was done about it, and what it is
    /// now. A rebuild that was declined or that failed says so, in a field, and
    /// the rows come back anyway with the stale label they had. Nothing here
    /// ever reports `current` on the strength of having tried.
    pub fn refresh_if_stale(&mut self) -> Result<Refreshed> {
        self.refresh_if_stale_within(AUTO_REFRESH_CEILING)
    }

    /// The same, with the ceiling named.
    ///
    /// It exists so the ceiling can be tested for what it does rather than for
    /// what it is: a test that had to build two thousand files to see a refusal
    /// would take longer than the refusal exists to prevent, and would be
    /// measuring the temporary directory.
    pub fn refresh_if_stale_within(&mut self, ceiling: usize) -> Result<Refreshed> {
        let before = self.freshness()?;
        let Freshness::Stale(staleness) = before else {
            return Ok(Refreshed::NotNeeded);
        };

        // How big the tree is, from the two numbers already in hand: what the
        // index holds, plus what appeared since. Cheap on purpose — asking the
        // filesystem again would be a second walk to decide whether to do a
        // third, and the walk is most of what a rebuild costs.
        let estimated_files = self.node_count()? + staleness.added.len();
        if estimated_files > ceiling {
            return Ok(Refreshed::Declined {
                estimated_files,
                ceiling,
                was: staleness,
            });
        }

        let began = std::time::Instant::now();
        match self.build() {
            Ok(report) => Ok(Refreshed::Rebuilt {
                was: staleness,
                took_ms: began.elapsed().as_millis(),
                report,
            }),
            // A rebuild that could not happen is not a tree that did not change.
            // Rule 10, in the place where confusing the two would have the
            // caller looking for a filesystem problem that is really a
            // read-only store.
            Err(error) => Ok(Refreshed::Failed {
                was: staleness,
                why: error.to_string(),
            }),
        }
    }

    /// Every node, with the index's freshness attached.
    pub fn nodes(&self) -> Result<Answer<Vec<Node>>> {
        let freshness = self.freshness()?;
        let mut statement = self
            .connection
            .prepare("SELECT path, language, size FROM nodes ORDER BY path")?;
        let rows = statement
            .query_map([], |row| {
                Ok(Node {
                    path: row.get(0)?,
                    language: row.get(1)?,
                    size: row.get::<_, i64>(2)? as u64,
                    tags: Vec::new(),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut rows = rows;
        for node in &mut rows {
            node.tags = self.tags_of(&node.path)?;
        }
        Ok(Answer { rows, freshness })
    }

    /// What this file depends on.
    pub fn dependencies_of(&self, path: &str) -> Result<Answer<Vec<Edge>>> {
        self.edges_where("from_path = ?1", path)
    }

    /// What depends on this file.
    ///
    /// The query the whole primitive exists for: "give me everything that
    /// depends on the auth module" is not answerable by walking directories.
    ///
    /// One row per dependent file, not per reference. A file that names the
    /// same module twice — `pub mod store;` and `pub use store::Store;` —
    /// depends on it once, and listing it twice would make a dependent count
    /// meaningless.
    pub fn dependents_of(&self, path: &str) -> Result<Answer<Vec<Edge>>> {
        let answer = self.edges_where("to_path = ?1", path)?;

        // One row per dependent, and when a file has both kinds of evidence the
        // import is the row that survives. Keeping whichever the sort happened
        // to reach first would make a file that plainly writes `use
        // crate::store::Store` report itself as a symbol-level guess, which is
        // the weaker claim about the stronger fact.
        let mut best: std::collections::BTreeMap<String, Edge> = std::collections::BTreeMap::new();
        for edge in answer.rows {
            match best.entry(edge.from.clone()) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(edge);
                }
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    if slot.get().via == Via::Symbol && edge.via == Via::Import {
                        slot.insert(edge);
                    }
                }
            }
        }

        Ok(Answer {
            rows: best.into_values().collect(),
            freshness: answer.freshness,
        })
    }

    fn edges_where(&self, condition: &str, parameter: &str) -> Result<Answer<Vec<Edge>>> {
        let freshness = self.freshness()?;
        let sql = format!(
            "SELECT from_path, raw_target, to_path, line, via FROM edges WHERE {condition} \
             ORDER BY from_path, line"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement
            .query_map([parameter], |row| {
                Ok(Edge {
                    from: row.get(0)?,
                    raw_target: row.get(1)?,
                    to: row.get(2)?,
                    line: row.get::<_, i64>(3)? as usize,
                    via: Via::from_word(&row.get::<_, String>(4)?),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Answer { rows, freshness })
    }

    /// Attach a tag to a file.
    /// Everything the index knows about one name.
    ///
    /// `Superficie-para-el-LLM.md`, punto **C2**. The two lists arrive together
    /// because they are one thought — *where does this come from and who uses
    /// it* — and a caller that had to ask twice would pay two round trips for a
    /// question it asked once.
    ///
    /// Exact and case-sensitive. A search that also matched `Login` and
    /// `login_user` would be a different service: it would answer a question
    /// nobody asked with rows the caller then has to filter, which is the
    /// context cost this exists to lower rather than move.
    pub fn symbol(&self, name: &str) -> Result<Answer<Found>> {
        let freshness = self.freshness()?;

        let mut statement = self.connection.prepare(
            "SELECT name, kind, path, line FROM symbols WHERE name = ?1 \
             ORDER BY path, line",
        )?;
        let definitions = statement
            .query_map([name], |row| {
                Ok(Symbol {
                    name: row.get(0)?,
                    kind: row.get(1)?,
                    path: row.get(2)?,
                    line: row.get::<_, i64>(3)? as usize,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut statement = self
            .connection
            .prepare("SELECT path, line FROM mentions WHERE name = ?1 ORDER BY path, line")?;
        let uses = statement
            .query_map([name], |row| {
                Ok(Use {
                    path: row.get(0)?,
                    line: row.get::<_, i64>(1)? as usize,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Answer {
            rows: Found { definitions, uses },
            freshness,
        })
    }

    /// How many names this tree declares, for a caller checking the index is
    /// worth asking.
    pub fn symbol_count(&self) -> Result<usize> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| {
                row.get::<_, i64>(0)
            })? as usize)
    }

    pub fn tag(&self, path: &str, tag: &str) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO tags (path, tag) VALUES (?1, ?2)",
            rusqlite::params![path, tag],
        )?;
        Ok(())
    }

    pub fn untag(&self, path: &str, tag: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM tags WHERE path = ?1 AND tag = ?2",
            rusqlite::params![path, tag],
        )?;
        Ok(())
    }

    fn tags_of(&self, path: &str) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT tag FROM tags WHERE path = ?1 ORDER BY tag")?;
        let rows = statement
            .query_map([path], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Files carrying a tag — the semantic query the primitive promises.
    pub fn tagged(&self, tag: &str) -> Result<Answer<Vec<String>>> {
        let freshness = self.freshness()?;
        let mut statement = self
            .connection
            .prepare("SELECT path FROM tags WHERE tag = ?1 ORDER BY path")?;
        let rows = statement
            .query_map([tag], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Answer { rows, freshness })
    }

    /// Remember the mutation count the index was last built at.
    ///
    /// Persisted so a one-shot command can pick up where the previous one left
    /// off. Without it every invocation would start with broken coverage and
    /// the counter could never say anything.
    pub fn set_mutation_baseline(&self, baseline: u64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO meta (key, value) VALUES ('mutation_baseline', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [baseline.to_string()],
        )?;
        Ok(())
    }

    /// The recorded baseline, if one was ever written.
    ///
    /// A value that cannot be parsed is treated as absent rather than as zero.
    /// Zero would be a baseline the watcher could vouch for, and a corrupt
    /// field must never turn into a claim.
    pub fn mutation_baseline(&self) -> Result<Option<u64>> {
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'mutation_baseline'",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(stored.and_then(|value| value.parse().ok()))
    }

    /// Record how much this index is allowed to let the counter decide.
    ///
    /// Persisted per index rather than globally, because it is a fact about
    /// one tree on one machine: the same counter can be scoped to one tree and
    /// machine-wide for another, and the verification that earned it was run
    /// against a specific tree.
    ///
    /// `earned` is what the verification found. Stored beside the setting so
    /// the answer to "why is the fast path on here" is on disk instead of in
    /// somebody's memory.
    pub fn set_trust(&self, trust: crate::watch::Trust, earned: Option<&str>) -> Result<()> {
        let value = match trust {
            crate::watch::Trust::Counter => "counter",
            crate::watch::Trust::WalkAlways => "walk",
        };
        self.connection.execute(
            "INSERT INTO meta (key, value) VALUES ('trust', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [value],
        )?;
        match earned {
            Some(note) => self.connection.execute(
                "INSERT INTO meta (key, value) VALUES ('trust_earned', ?1)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [note],
            )?,
            None => self
                .connection
                .execute("DELETE FROM meta WHERE key = 'trust_earned'", [])?,
        };
        Ok(())
    }

    /// How much the counter is allowed to decide for this index.
    ///
    /// Anything not recognised — absent, corrupt, written by a future version
    /// — reads as [`crate::watch::Trust::WalkAlways`]. The shortcut is the
    /// dangerous answer, so it is never the one a damaged field can produce.
    pub fn trust(&self) -> Result<crate::watch::Trust> {
        let stored: Option<String> = self
            .connection
            .query_row("SELECT value FROM meta WHERE key = 'trust'", [], |row| {
                row.get(0)
            })
            .ok();
        Ok(match stored.as_deref() {
            Some("counter") => crate::watch::Trust::Counter,
            _ => crate::watch::Trust::WalkAlways,
        })
    }

    /// What the verification found when the fast path was switched on.
    pub fn trust_earned(&self) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'trust_earned'",
                [],
                |row| row.get(0),
            )
            .ok())
    }

    /// Forget the baseline, so the next run starts with no coverage.
    pub fn clear_mutation_baseline(&self) -> Result<()> {
        self.connection
            .execute("DELETE FROM meta WHERE key = 'mutation_baseline'", [])?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn connection_for_tests(&self) -> &Connection {
        &self.connection
    }

    pub fn node_count(&self) -> Result<usize> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get::<_, i64>(0))?
            as usize)
    }

    pub fn edge_count(&self) -> Result<usize> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get::<_, i64>(0))?
            as usize)
    }
}

/// Map a written reference onto a file in the tree, if it names one.
///
/// Honest about its limits: it resolves what can be resolved from the text and
/// the file list, and returns `None` otherwise. An unresolved edge is kept with
/// `to = None`, because "this file references numpy" is real information even
/// though numpy is not in the tree. Guessing would put edges in the graph that
/// no execution follows, which is worse than admitting the reference is
/// external.
fn resolve(
    from: &str,
    target: &str,
    kind: ReferenceKind,
    known: &std::collections::HashSet<&str>,
) -> Option<String> {
    let directory = Path::new(from)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    // `use crate::{A, B}` names the crate root itself.
    if matches!(target, "crate" | "self" | "super") {
        for root_file in ["lib.rs", "main.rs", "mod.rs"] {
            let candidate = join(&directory, root_file);
            if known.contains(candidate.as_str()) {
                return Some(candidate);
            }
        }
        return None;
    }

    // Split the reference into the segments it actually names. `crate::` and
    // `self::` are position markers, not path components.
    let cleaned = target
        .trim_start_matches("./")
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .replace("::", "/")
        .replace('.', "/");
    let reference: Vec<&str> = cleaned.split('/').filter(|s| !s.is_empty()).collect();
    if reference.is_empty() {
        return None;
    }

    // Where the reference might be rooted: next to the importing file, or at
    // the top of the tree.
    let bases: Vec<String> = match kind {
        ReferenceKind::Relative => {
            let mut bases = vec![directory.clone()];
            // `../lib/x` walks up from the importing file's directory.
            if target.starts_with("..") {
                bases.insert(0, join(&directory, target));
            }
            bases.push(String::new());
            bases
        }
        _ => vec![directory.clone(), String::new()],
    };

    let extensions = ["", ".rs", ".py", ".js", ".ts", ".go", ".c", ".h"];

    for base in bases {
        // Try the whole reference, then drop trailing segments — but never
        // all of them.
        //
        // The tail of an import path is usually an *item*, not a file:
        // `use crate::keystore::Keystore` has to match `keystore.rs`, so some
        // shortening is required. But shortening must stop at the reference
        // itself: if it were allowed to eat into the base directory too,
        // `std::path::Path` would happily "resolve" to the importing crate's
        // own `lib.rs`, which is worse than not resolving at all.
        for length in (1..=reference.len()).rev() {
            let prefix = join(&base, &reference[..length].join("/"));
            let Some(prefix) = normalise(Path::new(&prefix)) else {
                continue;
            };

            for extension in extensions {
                let with_extension = format!("{prefix}{extension}");
                if known.contains(with_extension.as_str()) {
                    return Some(with_extension);
                }
            }
            // A directory-shaped module: `foo/mod.rs`, `foo/__init__.py`.
            for entry in ["/mod.rs", "/__init__.py", "/index.js", "/lib.rs"] {
                let nested = format!("{prefix}{entry}");
                if known.contains(nested.as_str()) {
                    return Some(nested);
                }
            }
        }
    }

    None
}

fn join(base: &str, rest: &str) -> String {
    if base.is_empty() {
        rest.to_string()
    } else if rest.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{rest}")
    }
}

/// Collapse `.` and `..` textually. Returns `None` if the path escapes the root.
fn normalise(path: &Path) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_str()?),
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// The walk, in one place, because everything that walks has to agree exactly.
///
/// It lives in `thalyx-files` since 2026-08-23, when `encontrar` and
/// `contenido` became the third and fourth things that walk a tree. The reason
/// it is one function is unchanged and is written where it now lives: two walks
/// that disagree about which files belong make every index stale the moment it
/// is written, and it reads as a staleness bug rather than as two walks.
pub(crate) use thalyx_files::walk;

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, contents) in files {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, contents).unwrap();
        }
        dir
    }

    fn indexed(dir: &tempfile::TempDir) -> Index {
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();
        index
    }

    #[test]
    fn a_file_that_reaches_the_code_through_a_field_is_a_dependent() {
        // The defect, verbatim. `handler.rs` imports `server.rs` and never
        // names `store.rs`, and it calls `Store::persist` through a field of
        // `Server`. Asked what depends on `store.rs`, the index used to answer
        // with the two files that write `use crate::store::…`; `grep persist`
        // on Linux found the third. The mention was already in the index —
        // nothing turned it into an edge.
        let dir = tree(&[
            (
                "src/store.rs",
                "pub struct Store;\nimpl Store {\n    pub fn persist(&self) {}\n}\n",
            ),
            (
                "src/server.rs",
                "use crate::store::Store;\npub struct Server {\n    pub store: Store,\n}\n",
            ),
            (
                "src/handler.rs",
                "use crate::server::Server;\npub fn handle(s: &Server) {\n    s.store.persist();\n}\n",
            ),
        ]);
        let index = indexed(&dir);

        let dependent = index
            .dependents_of("src/store.rs")
            .unwrap()
            .rows
            .into_iter()
            .find(|edge| edge.from == "src/handler.rs")
            .expect("the file that calls `persist` depends on the file that declares it");

        assert_eq!(dependent.via, Via::Symbol);
        assert_eq!(dependent.raw_target, "persist");
        assert_eq!(dependent.line, 3);
    }

    #[test]
    fn a_name_two_files_declare_never_becomes_an_edge() {
        // The precision guard, and the reason the rule is *exactly one* file.
        // With two declarations a use of the name could be either, and an edge
        // to one of them would be a coin toss presented as a fact — which is
        // worse than the missing row this whole change exists to add.
        let dir = tree(&[
            ("src/disk.rs", "pub fn flush() {}\n"),
            ("src/cache.rs", "pub fn flush() {}\n"),
            ("src/caller.rs", "pub fn stop() {\n    flush();\n}\n"),
        ]);
        let index = indexed(&dir);

        for ambiguous in ["src/disk.rs", "src/cache.rs"] {
            assert!(
                index.dependents_of(ambiguous).unwrap().rows.is_empty(),
                "`{ambiguous}` got a dependent from a name two files declare"
            );
        }

        // And the index still says what it knows: two declarations and one use.
        // Refusing to draw the edge is not refusing to answer.
        let found = index.symbol("flush").unwrap().rows;
        assert_eq!(found.definitions.len(), 2);
        assert_eq!(found.uses.len(), 1);
    }

    #[test]
    fn a_symbol_edge_is_not_drawn_where_an_import_already_says_it() {
        // Both rows would be about the same dependency, and the answer this
        // whole primitive exists to make cheaper would be paying twice for it.
        let dir = tree(&[
            (
                "src/thing.rs",
                "pub struct Thing;\npub fn make() -> Thing { Thing }\n",
            ),
            (
                "src/user.rs",
                "use crate::thing::Thing;\npub fn go() -> Thing {\n    crate::thing::make()\n}\n",
            ),
        ]);
        let index = indexed(&dir);

        let rows = index.dependencies_of("src/user.rs").unwrap().rows;
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].via, Via::Import);
        assert_eq!(rows[0].to.as_deref(), Some("src/thing.rs"));
    }

    // ─────────────────────────────────────────── symbols, which is punto C2

    #[test]
    fn a_name_is_found_where_it_is_defined_and_where_it_is_used() {
        let dir = tree(&[
            ("src/auth.rs", "pub fn login() {}\n"),
            ("src/one.rs", "use crate::auth;\nfn a() { login(); }\n"),
            ("src/two.rs", "use crate::auth;\nfn b() { login(); }\n"),
        ]);
        let index = indexed(&dir);
        let found = index.symbol("login").unwrap().regardless_of_freshness();

        // The whole claim of C2 in one answer: one row saying where it comes
        // from, two saying who uses it. The `grep` that answers the same
        // question returns three lines of which one is the definition and the
        // caller has to work out which.
        assert_eq!(found.definitions.len(), 1);
        assert_eq!(found.definitions[0].path, "src/auth.rs");
        assert_eq!(found.definitions[0].kind, "function");
        assert_eq!(found.definitions[0].line, 1);

        let where_used: Vec<&str> = found.uses.iter().map(|u| u.path.as_str()).collect();
        assert_eq!(where_used, vec!["src/one.rs", "src/two.rs"]);
    }

    #[test]
    fn the_line_a_name_is_declared_on_is_not_counted_as_a_use_of_it() {
        // The failure this prevents is the one somebody deletes code on: every
        // symbol looking like it has one more caller than it has, including the
        // ones that have none.
        let dir = tree(&[("src/lonely.rs", "pub fn unused() {}\n")]);
        let index = indexed(&dir);
        let found = index.symbol("unused").unwrap().regardless_of_freshness();

        assert_eq!(found.definitions.len(), 1);
        assert!(
            found.uses.is_empty(),
            "a definition counted itself as a use: {:?}",
            found.uses
        );
    }

    #[test]
    fn a_mention_inside_a_comment_is_not_a_use() {
        // The difference from `grep`, stated as a test. `grep -r login` cannot
        // tell these apart, and the caller pays for the rows and then pays
        // again to work out which are real.
        let dir = tree(&[
            ("src/auth.rs", "pub fn login() {}\n"),
            ("src/talk.rs", "// login is handled elsewhere\nfn c() {}\n"),
        ]);
        let index = indexed(&dir);
        let found = index.symbol("login").unwrap().regardless_of_freshness();

        assert!(
            found.uses.is_empty(),
            "a comment counted as a use: {found:?}"
        );
    }

    #[test]
    fn a_name_nothing_defines_is_not_recorded_as_a_mention_of_anything() {
        // Only names the tree declares are kept. Without that the table is
        // mostly vocabulary — `println`, `let`, `self` — and an index that is
        // mostly vocabulary is one nobody can afford to keep.
        let dir = tree(&[("src/a.rs", "pub fn f() { println!(\"x\"); }\n")]);
        let index = indexed(&dir);

        let found = index.symbol("println").unwrap().regardless_of_freshness();
        assert!(found.definitions.is_empty());
        assert!(found.uses.is_empty());
    }

    #[test]
    fn the_search_is_exact_rather_than_helpfully_wider() {
        let dir = tree(&[(
            "src/a.rs",
            "pub fn login() {}\npub fn login_user() {}\npub fn Login() {}\n",
        )]);
        let index = indexed(&dir);
        let found = index.symbol("login").unwrap().regardless_of_freshness();

        // A wider match would answer a question nobody asked with rows the
        // caller then has to filter — which moves the context cost rather than
        // lowering it.
        assert_eq!(found.definitions.len(), 1);
        assert_eq!(found.definitions[0].line, 1);
    }

    #[test]
    fn a_rebuild_does_not_leave_the_symbols_of_the_previous_one_behind() {
        let dir = tree(&[("src/a.rs", "pub fn gone() {}\n")]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();
        assert_eq!(
            index
                .symbol("gone")
                .unwrap()
                .regardless_of_freshness()
                .definitions
                .len(),
            1
        );

        std::fs::write(dir.path().join("src/a.rs"), "pub fn stayed() {}\n").unwrap();
        index.build().unwrap();

        // A stale row here is worse than a missing one: it sends somebody to a
        // line where the function is not, and nothing in the answer says so.
        let found = index.symbol("gone").unwrap().regardless_of_freshness();
        assert!(found.definitions.is_empty(), "{found:?}");
        assert_eq!(
            index
                .symbol("stayed")
                .unwrap()
                .regardless_of_freshness()
                .definitions
                .len(),
            1
        );
    }

    #[test]
    fn a_symbol_answer_carries_the_freshness_like_every_other_one() {
        let dir = tree(&[("src/a.rs", "pub fn f() {}\n")]);
        let index = indexed(&dir);
        // The decreed rule of `FS-en-Grafo`: the rows and the caveat are one
        // object, because separating them is how a cache starts being mistaken
        // for the truth.
        assert!(index.symbol("f").unwrap().freshness.is_current());
    }

    #[test]
    fn indexes_files_as_nodes() {
        let dir = tree(&[
            ("src/main.rs", "use crate::util;\n"),
            ("src/util.rs", "pub fn f() {}\n"),
            ("README.md", "hello\n"),
        ]);
        let index = indexed(&dir);

        assert_eq!(index.node_count().unwrap(), 3);
        let nodes = index.nodes().unwrap().regardless_of_freshness();
        let readme = nodes.iter().find(|n| n.path == "README.md").unwrap();
        assert_eq!(readme.language, None, "unknown types are still indexed");
    }

    #[test]
    fn resolves_dependencies_inside_the_tree() {
        let dir = tree(&[
            ("src/main.rs", "use crate::util;\n"),
            ("src/util.rs", "pub fn f() {}\n"),
        ]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("src/main.rs")
            .unwrap()
            .regardless_of_freshness();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to.as_deref(), Some("src/util.rs"));
    }

    #[test]
    fn answers_who_depends_on_this() {
        // The query the primitive exists for, and the one no directory walk
        // can answer.
        let dir = tree(&[
            ("src/auth.rs", "pub fn login() {}\n"),
            ("src/api.rs", "use crate::auth;\n"),
            ("src/web.rs", "use crate::auth;\n"),
            ("src/unrelated.rs", "use std::fs;\n"),
        ]);
        let index = indexed(&dir);

        let dependents = index
            .dependents_of("src/auth.rs")
            .unwrap()
            .regardless_of_freshness();
        let sources: Vec<&str> = dependents.iter().map(|e| e.from.as_str()).collect();

        assert_eq!(sources.len(), 2);
        assert!(sources.contains(&"src/api.rs"));
        assert!(sources.contains(&"src/web.rs"));
    }

    #[test]
    fn a_file_that_names_a_module_twice_is_one_dependent() {
        let dir = tree(&[
            ("src/store.rs", "pub struct Store;\n"),
            ("src/lib.rs", "pub mod store;\npub use store::Store;\n"),
        ]);
        let index = indexed(&dir);

        let dependents = index
            .dependents_of("src/store.rs")
            .unwrap()
            .regardless_of_freshness();
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0].from, "src/lib.rs");
    }

    #[test]
    fn external_references_are_kept_unresolved_not_invented() {
        let dir = tree(&[("app.py", "import numpy\nimport os\n")]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("app.py")
            .unwrap()
            .regardless_of_freshness();
        assert_eq!(edges.len(), 2);
        assert!(
            edges.iter().all(|e| e.to.is_none()),
            "references outside the tree must stay unresolved rather than be guessed at"
        );
        assert!(edges.iter().any(|e| e.raw_target == "numpy"));
    }

    #[test]
    fn resolves_python_package_directories() {
        let dir = tree(&[
            ("app/__init__.py", "\n"),
            ("app/models.py", "\n"),
            ("main.py", "from app.models import User\n"),
        ]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("main.py")
            .unwrap()
            .regardless_of_freshness();
        assert_eq!(edges[0].to.as_deref(), Some("app/models.py"));
    }

    #[test]
    fn resolves_javascript_relative_imports() {
        let dir = tree(&[
            ("src/index.js", "import { x } from './utils';\n"),
            ("src/utils.js", "export const x = 1;\n"),
        ]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("src/index.js")
            .unwrap()
            .regardless_of_freshness();
        assert_eq!(edges[0].to.as_deref(), Some("src/utils.js"));
    }

    #[test]
    fn resolves_c_local_includes() {
        let dir = tree(&[
            ("src/main.c", "#include \"config.h\"\n#include <stdio.h>\n"),
            ("src/config.h", "#define X 1\n"),
        ]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("src/main.c")
            .unwrap()
            .regardless_of_freshness();
        let local = edges.iter().find(|e| e.raw_target == "config.h").unwrap();
        let system = edges.iter().find(|e| e.raw_target == "stdio.h").unwrap();
        assert_eq!(local.to.as_deref(), Some("src/config.h"));
        assert_eq!(system.to, None);
    }

    #[test]
    fn ignores_build_output_and_version_control() {
        let dir = tree(&[
            ("src/main.rs", "\n"),
            ("target/debug/junk.rs", "\n"),
            (".git/config", "\n"),
            ("node_modules/pkg/index.js", "\n"),
        ]);
        let index = indexed(&dir);
        assert_eq!(index.node_count().unwrap(), 1);
    }

    #[test]
    fn a_hidden_directory_is_where_a_machine_keeps_its_own_things_and_is_not_read() {
        // The three that were actually walked into on Cesar's machine, plus a
        // control: the same names without the dot are ordinary directories and
        // must still be read, or this rule would have hidden his source.
        let dir = tree(&[
            ("src/main.rs", "\n"),
            (".cargo/registry/src/serde/lib.rs", "\n"),
            (".rustup/toolchains/stable/lib/rustlib/src/x.rs", "\n"),
            (".cache/something/y.rs", "\n"),
            ("cargo/mine.rs", "\n"),
        ]);
        let index = indexed(&dir);
        assert_eq!(index.node_count().unwrap(), 2);
    }

    #[test]
    fn a_tree_named_on_purpose_is_read_even_though_its_own_name_is_hidden() {
        // The rule is about descending, not about what was asked for. Somebody
        // who names `~/.config` has named it; answering "nothing here" about a
        // directory full of files would be the rule eating its own argument.
        let dir = tree(&[(".config/app/main.rs", "\n")]);
        let mut index = Index::in_memory(&dir.path().join(".config")).unwrap();
        index.build().unwrap();
        assert_eq!(index.node_count().unwrap(), 1);
    }

    #[test]
    fn a_tree_too_big_to_wait_for_is_refused_and_the_index_that_was_there_survives() {
        let dir = tree(&[("src/main.rs", "fn a() {}\n")]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();
        assert_eq!(index.node_count().unwrap(), 1);

        // Past the ceiling by one file, so this is a test of the boundary and
        // not of a number far away from it.
        let big = dir.path().join("many");
        std::fs::create_dir_all(&big).unwrap();
        for n in 0..=CEILING {
            std::fs::write(big.join(format!("f{n:07}.txt")), "").unwrap();
        }

        let refused = index.build().unwrap_err();
        assert!(
            matches!(refused, GraphError::TreeTooLarge { .. }),
            "a tree past the ceiling was reported as {refused:?}"
        );
        // The control, and the reason the refusal happens inside a transaction:
        // a rebuild that refused must not be a rebuild that emptied the index.
        assert_eq!(index.node_count().unwrap(), 1);
    }

    #[test]
    fn tags_are_queryable() {
        let dir = tree(&[("src/auth.rs", "\n"), ("src/api.rs", "\n")]);
        let index = indexed(&dir);

        index.tag("src/auth.rs", "auth-core").unwrap();
        index.tag("src/api.rs", "auth-core").unwrap();
        index.tag("src/api.rs", "public").unwrap();

        let tagged = index.tagged("auth-core").unwrap().regardless_of_freshness();
        assert_eq!(tagged, vec!["src/api.rs", "src/auth.rs"]);

        index.untag("src/api.rs", "auth-core").unwrap();
        assert_eq!(
            index.tagged("auth-core").unwrap().regardless_of_freshness(),
            vec!["src/auth.rs"]
        );
    }

    #[test]
    fn rebuilding_does_not_duplicate() {
        let dir = tree(&[("src/main.rs", "use crate::util;\n"), ("src/util.rs", "\n")]);
        let mut index = indexed(&dir);

        let first = index.build().unwrap();
        let second = index.build().unwrap();
        assert_eq!(first, second);
        assert_eq!(index.edge_count().unwrap(), 1);
    }

    #[test]
    fn resolves_import_paths_that_name_an_item_not_a_file() {
        // `use crate::keystore::Keystore` names a type in its last segment.
        // Matching only the full path resolved nothing, which silently lost
        // most of the real edges in a Rust project.
        let dir = tree(&[
            ("src/keystore.rs", "pub struct Keystore;\n"),
            ("src/main.rs", "use crate::keystore::Keystore;\n"),
        ]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("src/main.rs")
            .unwrap()
            .regardless_of_freshness();
        assert_eq!(edges[0].to.as_deref(), Some("src/keystore.rs"));
    }

    #[test]
    fn does_not_resolve_external_crates_to_local_files() {
        // Over-resolution is worse than under-resolution: an edge the graph
        // invents is a lie, while a missing one is only an omission. Shortening
        // an import path must never eat past the reference into the base
        // directory, or `std::path::Path` "resolves" to the importing crate's
        // own lib.rs.
        let dir = tree(&[
            ("src/lib.rs", "pub mod util;\n"),
            (
                "src/util.rs",
                "use std::path::Path;\nuse serde::Serialize;\n",
            ),
        ]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("src/util.rs")
            .unwrap()
            .regardless_of_freshness();

        for edge in &edges {
            assert_eq!(
                edge.to, None,
                "`{}` is an external crate and must stay unresolved",
                edge.raw_target
            );
        }
    }

    #[test]
    fn a_bare_crate_reference_resolves_to_the_crate_root() {
        let dir = tree(&[
            ("src/lib.rs", "pub struct Thing;\n"),
            ("src/util.rs", "use crate::{Thing};\n"),
        ]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("src/util.rs")
            .unwrap()
            .regardless_of_freshness();
        assert_eq!(edges[0].to.as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn resolves_deeply_qualified_python_imports() {
        let dir = tree(&[
            ("app/models.py", "class User: pass\n"),
            ("main.py", "from app.models.User import x\n"),
        ]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("main.py")
            .unwrap()
            .regardless_of_freshness();
        assert_eq!(edges[0].to.as_deref(), Some("app/models.py"));
    }

    #[test]
    fn the_index_refuses_to_live_inside_the_tree_it_indexes() {
        // A cache that is part of its own input can never be current: writing
        // it makes it stale. This was a real bug, caused by a CLI argument
        // name collision, and it is now impossible by construction.
        let dir = tree(&[("src/main.rs", "\n")]);
        let inside = dir.path().join("state/index.db");

        assert!(matches!(
            Index::open(&inside, dir.path()),
            Err(GraphError::DatabaseInsideTree { .. })
        ));

        let outside = tempfile::tempdir().unwrap();
        assert!(Index::open(&outside.path().join("index.db"), dir.path()).is_ok());
    }

    #[test]
    fn a_module_imported_twice_is_one_dependency() {
        let dir = tree(&[
            ("src/main.rs", "use crate::util;\nuse crate::util;\n"),
            ("src/util.rs", "\n"),
        ]);
        let index = indexed(&dir);

        let edges = index
            .dependencies_of("src/main.rs")
            .unwrap()
            .regardless_of_freshness();
        assert_eq!(edges.len(), 1, "duplicate imports are one dependency");
        assert_eq!(edges[0].line, 1, "the first occurrence is kept");
    }

    #[test]
    fn build_report_counts_what_it_did() {
        let dir = tree(&[
            ("src/main.rs", "use crate::util;\nuse serde::Serialize;\n"),
            ("src/util.rs", "\n"),
            ("README.md", "\n"),
        ]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        let report = index.build().unwrap();

        assert_eq!(report.files_indexed, 3);
        assert_eq!(report.files_parsed, 2, "README.md has no known language");
        assert_eq!(report.edges, 2);
        assert_eq!(report.edges_resolved, 1, "serde is outside the tree");
    }
}
