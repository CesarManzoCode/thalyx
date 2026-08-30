//! What Cargo already knows about this workspace, asked of Cargo.
//!
//! Not parsed out of `Cargo.toml` by hand. A workspace is `[workspace]`
//! globs, inherited dependencies, renamed dependencies, optional features and
//! path dependencies that reach outside the tree, and every one of those is a
//! rule somebody has already implemented exactly once, correctly, in the tool
//! that owns the format. `Estrategia-de-Pruebas` rule 6 is about parsing
//! another tool's *output*; this is the version of it one level up — do not
//! parse another tool's **input** either, when the tool will tell you.
//!
//! `--no-deps` on purpose: the question here is which crates *of this tree*
//! depend on which, and resolving the registry graph would download and weigh
//! several thousand packages to answer a question about twenty-eight.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::{Result, RustError};

/// One crate of this workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub manifest: PathBuf,
    /// The directory the manifest is in — what "a file belongs to this
    /// package" means.
    pub root: PathBuf,
    /// The other packages **of this workspace** it depends on, by name. A
    /// registry dependency is not here: it cannot be edited, so it can never
    /// be the reason a check has to be re-run.
    pub depends_on: Vec<String>,
}

/// The workspace as Cargo describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub root: PathBuf,
    pub packages: Vec<Package>,
    /// Where Cargo would build. Excluded from every witness: build output is a
    /// consequence of the inputs, and folding it into the identity of the
    /// inputs would make every check invalidate the check that produced it.
    pub target_directory: PathBuf,
}

#[derive(Deserialize)]
struct RawMetadata {
    packages: Vec<RawPackage>,
    workspace_root: String,
    target_directory: String,
}

#[derive(Deserialize)]
struct RawPackage {
    name: String,
    version: String,
    manifest_path: String,
    dependencies: Vec<RawDependency>,
}

#[derive(Deserialize)]
struct RawDependency {
    name: String,
}

impl Workspace {
    /// Ask Cargo. Offline, because the answer is about this tree and a fetch
    /// would make a question about local structure fail on a train.
    pub fn read(root: &Path) -> Result<Self> {
        let manifest = root.join("Cargo.toml");
        if !manifest.is_file() {
            return Err(RustError::NotACargoWorkspace(root.display().to_string()));
        }
        let output = std::process::Command::new(cargo())
            .arg("metadata")
            .arg("--format-version")
            .arg("1")
            .arg("--no-deps")
            .arg("--offline")
            .arg("--manifest-path")
            .arg(&manifest)
            .output()
            .map_err(|error| RustError::NoCargo(error.to_string()))?;
        if !output.status.success() {
            return Err(RustError::Cargo(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Self::parse(&String::from_utf8_lossy(&output.stdout))
    }

    /// The same, from the bytes Cargo printed.
    ///
    /// Separate so that the parser is exercised against a **captured real
    /// sample** rather than only against a live Cargo — rule 6, in the shape
    /// that rule was written in: a fixture somebody invented proves the parser
    /// matches its author's idea of the format.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: RawMetadata =
            serde_json::from_str(text).map_err(|error| RustError::Cargo(error.to_string()))?;
        let names: std::collections::HashSet<&str> =
            raw.packages.iter().map(|p| p.name.as_str()).collect();
        let mut packages: Vec<Package> = raw
            .packages
            .iter()
            .map(|package| {
                let manifest = PathBuf::from(&package.manifest_path);
                let root = manifest
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| manifest.clone());
                let mut depends_on: Vec<String> = package
                    .dependencies
                    .iter()
                    .filter(|dependency| names.contains(dependency.name.as_str()))
                    .map(|dependency| dependency.name.clone())
                    .collect();
                depends_on.sort();
                depends_on.dedup();
                Package {
                    name: package.name.clone(),
                    version: package.version.clone(),
                    manifest,
                    root,
                    depends_on,
                }
            })
            .collect();
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Workspace {
            root: PathBuf::from(&raw.workspace_root),
            packages,
            target_directory: PathBuf::from(&raw.target_directory),
        })
    }

    pub fn package(&self, name: &str) -> Option<&Package> {
        self.packages.iter().find(|package| package.name == name)
    }

    /// Which package a file belongs to: the one whose directory is the longest
    /// prefix of it, which is what Cargo itself means by the question.
    ///
    /// The longest and not the first, because a workspace whose root is also a
    /// package contains every other package's files, and answering "the root"
    /// for all of them would make every change affect everything.
    pub fn package_of(&self, file: &Path) -> Option<&Package> {
        self.packages
            .iter()
            .filter(|package| file.starts_with(&package.root))
            .max_by_key(|package| package.root.as_os_str().len())
    }

    /// Every package that would have to be recompiled because one of these
    /// changed: the reverse dependency closure, inside this workspace.
    pub fn dependents_of(&self, names: &[String]) -> Vec<String> {
        let mut reached: std::collections::BTreeSet<String> = names.iter().cloned().collect();
        loop {
            let mut grew = false;
            for package in &self.packages {
                if reached.contains(&package.name) {
                    continue;
                }
                if package
                    .depends_on
                    .iter()
                    .any(|dependency| reached.contains(dependency))
                {
                    reached.insert(package.name.clone());
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        reached.into_iter().collect()
    }

    /// Every package these depend on, transitively, inside this workspace,
    /// including themselves.
    ///
    /// The other direction, and it is not the same question: dependents say
    /// *what has to be checked again*, and this says *what the answer depends
    /// on* — which is what a cache identity is made of.
    pub fn closure_of(&self, names: &[String]) -> Vec<String> {
        let mut reached: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut pending: Vec<String> = names.to_vec();
        while let Some(name) = pending.pop() {
            if !reached.insert(name.clone()) {
                continue;
            }
            if let Some(package) = self.package(&name) {
                pending.extend(package.depends_on.iter().cloned());
            }
        }
        reached.into_iter().collect()
    }
}

/// The `cargo` to run.
///
/// `CARGO` when this process was started by one, and otherwise the single
/// discovery in [`crate::toolchain`] — which is the only place that knows how
/// to find a toolchain installed by the person who typed `sudo`.
pub fn cargo() -> PathBuf {
    // `CARGO` first, because when this process *is* a `cargo` subcommand or a
    // build script Cargo has already said which one it is, and disagreeing
    // with it would run a second toolchain inside the first.
    std::env::var_os("CARGO")
        .map(PathBuf::from)
        .unwrap_or_else(crate::toolchain::cargo_command)
}
