//! Knowing when the index is lying.
//!
//! The human can move, edit and delete files without going through Thalyx —
//! that is the double-route principle, and it is not negotiable. So the index
//! *will* fall behind, and the only question is whether it knows.
//!
//! This module answers that by comparing what the index recorded against what
//! the tree looks like now: size and modification time per file, plus files
//! that appeared or vanished. No content is read, so the check stays cheap
//! enough to run on every query.
//!
//! It **fails closed**. Anything it cannot determine — an unreadable
//! directory, a file it cannot stat — counts as stale. An index that says
//! "current" when it is not is worse than one that says "I don't know".
//!
//! See `vault/04-Flujo-Canonico/Coherencia-Doble-Ruta.md`.

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

/// What changed in the tree since the index was built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Staleness {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
    /// Paths the check could not read. Their presence alone makes the index
    /// stale, because "unknown" is not "unchanged".
    pub unreadable: Vec<String>,
}

impl Staleness {
    pub fn total(&self) -> usize {
        self.added.len() + self.modified.len() + self.removed.len() + self.unreadable.len()
    }
}

/// Whether the index still matches the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    /// Every indexed file is where it was, unchanged, and nothing new appeared.
    Current,
    /// The tree moved on. The detail says how.
    Stale(Staleness),
}

impl Freshness {
    pub fn is_current(&self) -> bool {
        matches!(self, Freshness::Current)
    }

    /// A one-line summary, for anything that displays query results.
    pub fn describe(&self) -> String {
        match self {
            Freshness::Current => "index is current".to_string(),
            Freshness::Stale(staleness) => {
                let mut parts = Vec::new();
                if !staleness.added.is_empty() {
                    parts.push(format!("{} added", staleness.added.len()));
                }
                if !staleness.modified.is_empty() {
                    parts.push(format!("{} modified", staleness.modified.len()));
                }
                if !staleness.removed.is_empty() {
                    parts.push(format!("{} removed", staleness.removed.len()));
                }
                if !staleness.unreadable.is_empty() {
                    parts.push(format!("{} unreadable", staleness.unreadable.len()));
                }
                format!("index is STALE ({})", parts.join(", "))
            }
        }
    }
}

pub(crate) fn mtime_nanos(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos() as i64)
        // No usable mtime means we cannot tell whether the file changed. Zero
        // is recorded, and the comparison will treat any real value as a
        // change — failing closed rather than assuming nothing happened.
        .unwrap_or(0)
}

pub(crate) fn check(connection: &Connection, root: &Path) -> crate::Result<Freshness> {
    let mut indexed: HashMap<String, (i64, i64)> = HashMap::new();
    {
        let mut statement = connection.prepare("SELECT path, size, mtime_ns FROM nodes")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (path, size, mtime) = row?;
            indexed.insert(path, (size, mtime));
        }
    }

    let mut staleness = Staleness::default();
    let mut seen = std::collections::HashSet::new();

    for entry in crate::walk(root) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                staleness.unreadable.push(
                    error
                        .path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<unknown>".to_string()),
                );
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().into_owned();
        seen.insert(relative.clone());

        let Ok(metadata) = entry.metadata() else {
            staleness.unreadable.push(relative);
            continue;
        };

        match indexed.get(&relative) {
            None => staleness.added.push(relative),
            Some((size, mtime)) => {
                if *size != metadata.len() as i64 || *mtime != mtime_nanos(&metadata) {
                    staleness.modified.push(relative);
                }
            }
        }
    }

    for path in indexed.keys() {
        if !seen.contains(path) {
            staleness.removed.push(path.clone());
        }
    }

    staleness.added.sort();
    staleness.modified.sort();
    staleness.removed.sort();
    staleness.unreadable.sort();

    Ok(if staleness.total() == 0 {
        Freshness::Current
    } else {
        Freshness::Stale(staleness)
    })
}

#[cfg(test)]
mod tests {
    use crate::Index;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, contents) in files {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, contents).unwrap();
        }
        dir
    }

    #[test]
    fn a_freshly_built_index_is_current() {
        let dir = tree(&[("src/main.rs", "use crate::util;\n"), ("src/util.rs", "\n")]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();

        assert!(index.freshness().unwrap().is_current());
    }

    #[test]
    fn a_new_file_makes_the_index_stale() {
        // The plain double-route case: the human created a file with an editor,
        // never telling Thalyx.
        let dir = tree(&[("src/main.rs", "\n")]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();

        std::fs::write(dir.path().join("src/new.rs"), "\n").unwrap();

        match index.freshness().unwrap() {
            super::Freshness::Stale(staleness) => {
                assert_eq!(staleness.added, vec!["src/new.rs"]);
            }
            super::Freshness::Current => panic!("a new file must make the index stale"),
        }
    }

    #[test]
    fn an_edited_file_makes_the_index_stale() {
        let dir = tree(&[("src/main.rs", "use crate::a;\n")]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();

        std::fs::write(
            dir.path().join("src/main.rs"),
            "use crate::b;\nuse crate::c;\n",
        )
        .unwrap();

        match index.freshness().unwrap() {
            super::Freshness::Stale(staleness) => {
                assert_eq!(staleness.modified, vec!["src/main.rs"]);
            }
            super::Freshness::Current => panic!("an edit must make the index stale"),
        }
    }

    #[test]
    fn a_deleted_file_makes_the_index_stale() {
        let dir = tree(&[("src/main.rs", "\n"), ("src/gone.rs", "\n")]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();

        std::fs::remove_file(dir.path().join("src/gone.rs")).unwrap();

        match index.freshness().unwrap() {
            super::Freshness::Stale(staleness) => {
                assert_eq!(staleness.removed, vec!["src/gone.rs"]);
            }
            super::Freshness::Current => panic!("a deletion must make the index stale"),
        }
    }

    #[test]
    fn rebuilding_makes_it_current_again() {
        let dir = tree(&[("src/main.rs", "\n")]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();

        std::fs::write(dir.path().join("src/new.rs"), "\n").unwrap();
        assert!(!index.freshness().unwrap().is_current());

        index.build().unwrap();
        assert!(index.freshness().unwrap().is_current());
    }

    #[test]
    fn every_query_carries_the_freshness_with_it() {
        // The property the whole design rests on: a caller cannot get rows
        // without also being handed the caveat.
        let dir = tree(&[("src/main.rs", "use crate::util;\n"), ("src/util.rs", "\n")]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();

        std::fs::write(dir.path().join("src/util.rs"), "// changed\n").unwrap();

        let answer = index.dependencies_of("src/main.rs").unwrap();
        assert!(!answer.freshness.is_current());
        assert!(answer.freshness.describe().contains("STALE"));
        // The rows are still there — stale is not useless, it is *labelled*.
        assert_eq!(answer.rows.len(), 1);
    }

    #[test]
    fn the_description_says_what_changed() {
        let dir = tree(&[("a.rs", "\n"), ("b.rs", "\n")]);
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();

        std::fs::write(dir.path().join("c.rs"), "\n").unwrap();
        std::fs::remove_file(dir.path().join("b.rs")).unwrap();

        let description = index.freshness().unwrap().describe();
        assert!(description.contains("1 added"), "{description}");
        assert!(description.contains("1 removed"), "{description}");
    }
}
