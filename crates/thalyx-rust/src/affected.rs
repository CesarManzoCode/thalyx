//! Which crates a change reaches, and the identity of everything that decides
//! it.
//!
//! `vault/03-Primitivas/Ejecucion-Transaccional.md` gave `hacer` a Rust check
//! that compiled *the packages the changed files are in*. That is the easy
//! half and it is wrong on the case that matters: change a type in
//! `thalyx-core` and compiling `thalyx-core` proves nothing about the twelve
//! crates that use it. What a reviewer would run is the reverse dependency
//! closure, and it is mechanically derivable, so the model should never have to
//! decide it.
//!
//! ## The two directions, which are different questions
//!
//! - **Dependents** — *what has to be compiled again.* Upwards from the change.
//! - **Closure** — *what the answer depends on.* Downwards from the selection,
//!   and what the cache identity is made of.
//!
//! Confusing them makes a cache that either never hits or hits when it must
//! not. `false miss = slower, false hit = wrong`.

use crate::metadata::Workspace;
use std::path::{Path, PathBuf};
use thalyx_know::{Over, Witness, witness, woven};

/// Directory names never walked when taking a witness of source.
///
/// `target` because it is the *output*: folding a build's product into the
/// identity of its inputs makes every check invalidate itself. `.git` because
/// it changes on every commit and describes nothing the compiler reads.
pub const NOT_SOURCE: &[&str] = &["target", ".git", ".jj"];

/// What a Rust check reads: the code, and the manifests that say how it is
/// built. `Cargo.lock` is in it because a resolved version is an input to the
/// compilation even though no `.rs` file mentions it.
pub const SOURCE: &[&str] = &[".rs", "Cargo.toml", "Cargo.lock"];

/// Which packages a set of changed files selects for validation, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affected {
    /// The packages the changed files are actually inside.
    pub changed: Vec<String>,
    /// Those and everything that depends on them: what gets compiled.
    pub selected: Vec<String>,
    /// Changed files that belong to no package. Reported and never ignored: a
    /// file nobody can attribute is a reason a check might not cover the
    /// change, and rule 3 says a gap says its own name.
    pub unattributed: Vec<String>,
    /// Whether something changed that reaches every crate — the workspace
    /// manifest, or the lockfile.
    pub whole_workspace: bool,
    /// One sentence a caller can relay without reading any of the above.
    pub why: String,
}

/// Work out what a change reaches.
///
/// `changed` are paths relative to `root`, which is what
/// `thalyx_snapshot::Difference` produces.
pub fn affected(workspace: &Workspace, root: &Path, changed: &[String]) -> Affected {
    let mut direct: Vec<String> = Vec::new();
    let mut unattributed: Vec<String> = Vec::new();
    let mut whole_workspace = false;

    for name in changed {
        let path = root.join(name);
        // A lockfile or the workspace manifest changes how everything is
        // built, so nothing below it can be argued to be unaffected.
        if name == "Cargo.lock" || name == "Cargo.toml" {
            whole_workspace = true;
            continue;
        }
        match workspace.package_of(&path) {
            Some(package) => {
                if !direct.contains(&package.name) {
                    direct.push(package.name.clone());
                }
            }
            None => unattributed.push(name.clone()),
        }
    }
    direct.sort();

    let selected = if whole_workspace {
        workspace
            .packages
            .iter()
            .map(|package| package.name.clone())
            .collect()
    } else {
        workspace.dependents_of(&direct)
    };

    let why = if whole_workspace {
        format!(
            "the workspace manifest or lockfile changed, so all {} packages are checked",
            selected.len()
        )
    } else if direct.is_empty() {
        "no changed file belongs to a Cargo package".to_string()
    } else {
        format!(
            "{} changed ({}), and {} package(s) depend on {}",
            plural(direct.len(), "package"),
            direct.join(", "),
            selected.len() - direct.len(),
            if direct.len() == 1 { "it" } else { "them" }
        )
    };

    Affected {
        changed: direct,
        selected,
        unattributed,
        whole_workspace,
        why,
    }
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("1 {word}")
    } else {
        format!("{count} {word}s")
    }
}

/// The identity of everything a check of these packages reads.
///
/// The **closure** and not the selection: compiling `thalyx-cli` reads
/// `thalyx-core`, so a change to `thalyx-core` must invalidate the cached check
/// of `thalyx-cli`. And nothing else: a change to `thalyx-screen`, which
/// `thalyx-know` does not depend on, must not — which is the whole reason this
/// is scoped rather than a witness of the tree.
pub fn identity(workspace: &Workspace, packages: &[String], toolchain: &str) -> Witness {
    let mut roots: Vec<PathBuf> = Vec::new();
    for name in workspace.closure_of(packages) {
        if let Some(package) = workspace.package(&name) {
            roots.push(package.root.clone());
        }
    }
    // The workspace manifest and lockfile are read by every compilation, and
    // they live above every package directory.
    roots.push(workspace.root.join("Cargo.toml"));
    roots.push(workspace.root.join("Cargo.lock"));
    roots.sort();
    roots.dedup();

    let source = witness(&Over {
        roots: &roots,
        suffixes: SOURCE,
        skip: NOT_SOURCE,
    });
    // The toolchain is an input like any other: the same bytes compiled by a
    // different rustc is a different answer, and rule 12 is the whole story of
    // what it costs to assume otherwise.
    woven(&[&source, &thalyx_know::witness::of_text(toolchain)])
}

/// The identity of every source file of the workspace.
///
/// What a semantic answer depends on, and deliberately coarse: rust-analyzer
/// resolves a name using whatever it can see, so nothing narrower can be
/// *proved* to be enough. A false miss costs a re-query; a false hit gives a
/// frontier model a symbol location that moved.
pub fn source_identity(workspace: &Workspace) -> Witness {
    witness(&Over {
        roots: std::slice::from_ref(&workspace.root),
        suffixes: SOURCE,
        skip: NOT_SOURCE,
    })
}
