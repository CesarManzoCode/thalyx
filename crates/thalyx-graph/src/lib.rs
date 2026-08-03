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
    /// The reference exactly as written in the source.
    pub raw_target: String,
    /// The file it resolves to, when it resolves inside the tree at all.
    pub to: Option<String>,
    pub line: usize,
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

        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_ignored(e.path()))
        {
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
        }
        files.sort_by(|a, b| a.1.cmp(&b.1));

        // Two passes: every node has to exist before edges can resolve against
        // them, otherwise resolution would depend on directory traversal order.
        let mut pending_edges = Vec::new();

        for (absolute, relative) in &files {
            let metadata = match std::fs::metadata(absolute) {
                Ok(metadata) => metadata,
                Err(_) => {
                    report.skipped += 1;
                    continue;
                }
            };
            let language = Language::from_path(absolute);

            transaction.execute(
                "INSERT OR REPLACE INTO nodes (path, language, size, mtime_ns) \
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    relative,
                    language.map(|l| l.name()),
                    metadata.len() as i64,
                    staleness::mtime_nanos(&metadata),
                ],
            )?;
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

        for (from, reference) in pending_edges {
            let resolved = resolve(&from, &reference.target, reference.kind, &known);
            transaction.execute(
                "INSERT INTO edges (from_path, raw_target, to_path, line) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![from, reference.target, resolved, reference.line as i64],
            )?;
            report.edges += 1;
            if resolved.is_some() {
                report.edges_resolved += 1;
            }
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
        let mut seen = std::collections::HashSet::new();
        let rows = answer
            .rows
            .into_iter()
            .filter(|edge| seen.insert(edge.from.clone()))
            .collect();
        Ok(Answer {
            rows,
            freshness: answer.freshness,
        })
    }

    fn edges_where(&self, condition: &str, parameter: &str) -> Result<Answer<Vec<Edge>>> {
        let freshness = self.freshness()?;
        let sql = format!(
            "SELECT from_path, raw_target, to_path, line FROM edges WHERE {condition} \
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
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Answer { rows, freshness })
    }

    /// Attach a tag to a file.
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

/// Directories never worth indexing.
///
/// Build outputs and version control internals would swamp the graph with
/// nodes no one asks about, and `.git` alone can be larger than the project.
fn is_ignored(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some(".git" | "target" | "node_modules" | ".venv" | "__pycache__" | "dist" | "build")
    )
}

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
