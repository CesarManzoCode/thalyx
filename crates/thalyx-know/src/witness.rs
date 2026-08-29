//! The identity of the state an answer was derived from.
//!
//! `vault/03-Primitivas/Ejecucion-Transaccional.md` decided this shape for the
//! transaction, and 2026-08-29 decided its rules: **a witness made of
//! timestamps is not an identity**, because the last step of a reversible task
//! is *put everything back*, and putting everything back changes every
//! timestamp while restoring every byte. So this one is made of contents and
//! nothing else.
//!
//! ## Why not `thalyx_snapshot::witness`
//!
//! That one exists to authorise a destruction, so it folds in mtime, ctime and
//! inode — anything that could possibly differ, because a false *match* there
//! destroys somebody's work. This one exists to authorise **reuse of a
//! result**, and it has two different requirements:
//!
//! 1. It is **scoped**. A cached `cargo check` of one package must survive a
//!    change in a package that one does not depend on, and a whole-tree
//!    identity cannot express that.
//! 2. It is **content-only**. A tree restored byte for byte is the same tree,
//!    and a validation of it is still valid; an identity that said otherwise
//!    would turn every rollback into a cold cache.
//!
//! Both witnesses fail closed the same way: a path that could not be read makes
//! the witness incomplete, and an incomplete witness matches nothing, including
//! itself. A failure to read is not a failure to exist — rule 10 — so the count
//! is carried rather than folded away.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The rules this witness is made under, named inside the witness itself.
///
/// A string from another version is refused on sight rather than compared under
/// rules it was not made under. `k1`: sha256 of contents, path-ordered.
pub const WITNESS_VERSION: &str = "k1";

/// What a set of files contains, as one comparable string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Witness {
    pub id: String,
    /// How many files went into it. Reported, never used to decide.
    pub files: usize,
    /// How many paths could not be read. Any at all makes it authorise nothing.
    pub unreadable: usize,
    /// How many bytes were hashed. What this identity cost to compute.
    pub bytes: u64,
}

impl Witness {
    pub fn is_complete(&self) -> bool {
        self.unreadable == 0
    }

    /// Whether this witness and that string describe the same contents.
    ///
    /// Incomplete witnesses match nothing, including themselves: an answer
    /// derived from a tree part of which nobody read is an answer nobody can
    /// say is still true.
    pub fn matches(&self, claimed: &str) -> bool {
        self.is_complete() && self.id == claimed
    }
}

/// What to look at, and what to ignore, when taking a witness.
///
/// A struct rather than four positional arguments because every caller sets
/// three of the four the same way and the fourth is the interesting one, and a
/// call site reading `witness(&dirs, &[".rs"], &["target"], &[])` is a call
/// site nobody can check at a glance.
pub struct Over<'a> {
    /// The directories and single files to walk. A file is taken as itself.
    pub roots: &'a [PathBuf],
    /// A file counts when its name ends with one of these. Empty means all.
    pub suffixes: &'a [&'a str],
    /// Directory names never descended into, at any depth.
    pub skip: &'a [&'a str],
}

/// The witness of exactly what [`Over`] names, as it is right now.
///
/// Ordered by path so that two walks of the same contents in different
/// directory order agree, and nul-separated so that a file named `a\nb` cannot
/// be spelled as two files — a separator that can appear in what it separates
/// is not a separator.
pub fn witness(over: &Over<'_>) -> Witness {
    let mut found: std::collections::BTreeMap<String, [u8; 32]> = std::collections::BTreeMap::new();
    let mut unreadable: Vec<String> = Vec::new();
    let mut bytes = 0u64;

    for root in over.roots {
        let walk = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !entry.file_type().is_dir()
                    || entry
                        .file_name()
                        .to_str()
                        .is_none_or(|name| !over.skip.contains(&name))
            });
        for entry in walk {
            let entry = match entry {
                Ok(entry) => entry,
                // **Rule 10, and it is the whole rule.** A path that is not
                // there is not a path nobody could read: a caller naming
                // `Cargo.lock` among the things a check depends on is right to
                // name it, and a workspace that has not got one yet is a
                // workspace with no lockfile — not a tree part of which is a
                // mystery. Counting it as unreadable made every witness
                // incomplete, and an incomplete witness matches nothing, so the
                // validation cache silently never hit. Found by a test that
                // asserted a hit and got a compiler.
                Err(error)
                    if error
                        .io_error()
                        .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
                {
                    continue;
                }
                Err(error) => {
                    unreadable.push(
                        error
                            .path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "<unknown>".to_string()),
                    );
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !wanted(path, over.suffixes) {
                continue;
            }
            match std::fs::read(path) {
                Ok(contents) => {
                    bytes = bytes.saturating_add(contents.len() as u64);
                    let digest: [u8; 32] = Sha256::digest(&contents).into();
                    found.insert(path.display().to_string(), digest);
                }
                // A file that was walked and then could not be opened is a
                // real failure to read, and is said as one — unlike a name
                // that was never there.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => unreadable.push(path.display().to_string()),
            }
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(WITNESS_VERSION.as_bytes());
    hasher.update(b"\n");
    for (path, digest) in &found {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(digest);
        hasher.update(b"\n");
    }
    // In the digest and not only in the count, so two sets unreadable at
    // different places never hash the same.
    unreadable.sort();
    for path in &unreadable {
        hasher.update(b"?");
        hasher.update(path.as_bytes());
        hasher.update(b"\n");
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Witness {
        id: format!("{WITNESS_VERSION}-{}", hex::encode(digest)),
        files: found.len(),
        unreadable: unreadable.len(),
        bytes,
    }
}

fn wanted(path: &Path, suffixes: &[&str]) -> bool {
    if suffixes.is_empty() {
        return true;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    suffixes.iter().any(|suffix| name.ends_with(suffix))
}

/// One identity out of several, for an answer that depends on more than one
/// thing — a validation whose inputs are a package, its dependencies, and the
/// toolchain that would run it.
///
/// Order matters and is the caller's: the parts are folded in as given, so a
/// caller that wants order not to matter sorts them first. Making that the
/// caller's decision is deliberate — a helper that always sorted would make
/// `[a, b]` and `[b, a]` the same identity even where they are not.
pub fn woven(parts: &[&Witness]) -> Witness {
    let mut hasher = Sha256::new();
    hasher.update(WITNESS_VERSION.as_bytes());
    hasher.update(b"+\n");
    let mut files = 0;
    let mut unreadable = 0;
    let mut bytes = 0u64;
    for part in parts {
        hasher.update(part.id.as_bytes());
        hasher.update(b"\n");
        files += part.files;
        unreadable += part.unreadable;
        bytes = bytes.saturating_add(part.bytes);
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Witness {
        id: format!("{WITNESS_VERSION}-{}", hex::encode(digest)),
        files,
        unreadable,
        bytes,
    }
}

/// A witness of a string rather than of a tree — a toolchain version, a set of
/// flags, anything an answer depends on that is not a file.
pub fn of_text(text: &str) -> Witness {
    let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    Witness {
        id: format!("{WITNESS_VERSION}-{}", hex::encode(digest)),
        files: 0,
        unreadable: 0,
        bytes: text.len() as u64,
    }
}
