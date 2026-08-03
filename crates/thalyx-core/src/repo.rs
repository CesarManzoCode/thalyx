//! A local repository of `.thmod` bundles, and resolving a name to one of them.
//!
//! Implements `vault/04-Flujo-Canonico/Resolucion-de-Versiones.md`: **the
//! highest published version that satisfies the constraint and whose signature
//! validates.** No backtracking, no conflict resolution, no dependency graph —
//! Phase 1 forbids inter-module dependencies, and every legendary difficulty of
//! apt and npm comes from transitive ones. Without them the resolver stops
//! being a constraint solver and becomes an ordered comparison.
//!
//! Resolution is a sub-task without a contract of its own, per
//! `vault/04-Flujo-Canonico/Resolver-vs-Instalar.md`. It answers "which file",
//! and the install contract still governs what happens to that file.
//!
//! ## What resolving does not do
//!
//! It does not check the publisher key against the keystore. A signature that
//! validates only says the bytes match the key *inside the manifest*; whether
//! that key is the one this machine already trusts for this id is a TOFU
//! question, it is answered at install time, and answering it here as well
//! would put the same decision in two places that could disagree.
//!
//! What it does mean is that an unsigned or tampered bundle is never a
//! candidate — so a repository cannot steer a resolution by dropping in a
//! higher version it cannot sign.

use crate::bundle::Bundle;
use crate::{CoreError, Result};
use semver::{Version, VersionReq};
use std::path::{Path, PathBuf};

/// A bundle in a repository that parsed, and whose signature checks out.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub module_id: String,
    pub version: Version,
}

/// Why a bundle in the repository was not a candidate.
///
/// Kept and reported rather than discarded. "No version satisfies `^2`" and
/// "the only version that satisfies `^2` has a broken signature" send someone
/// to entirely different places, and a resolver that says only the first is
/// hiding the answer to the second.
#[derive(Debug, Clone)]
pub struct Rejected {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub candidates: Vec<Candidate>,
    pub rejected: Vec<Rejected>,
}

/// Read every `.thmod` in `dir`, keeping the ones that hold up.
///
/// A bundle that fails for any reason is recorded and skipped; one bad file in
/// a repository does not make the repository unusable.
pub fn scan(dir: &Path) -> Result<Scan> {
    let mut scan = Scan::default();

    let entries = std::fs::read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "thmod"))
        .collect();
    // Sorted so that two machines with the same repository resolve the same
    // way. Filesystem order is not an ordering anyone promised.
    paths.sort();

    for path in paths {
        match candidate_from(&path) {
            Ok(candidate) => scan.candidates.push(candidate),
            Err(reason) => scan.rejected.push(Rejected { path, reason }),
        }
    }

    Ok(scan)
}

fn candidate_from(path: &Path) -> std::result::Result<Candidate, String> {
    let bundle = Bundle::read(path).map_err(|e| e.to_string())?;

    // Before the version is even parsed. A bundle whose signature does not
    // check out is not a lower-priority candidate, it is not a candidate.
    bundle
        .manifest
        .verify_signature(&bundle.signature)
        .map_err(|_| "signature does not verify against the key in its manifest".to_string())?;

    let version = Version::parse(&bundle.manifest.version)
        .map_err(|e| format!("version `{}` is not semver: {e}", bundle.manifest.version))?;

    Ok(Candidate {
        path: path.to_path_buf(),
        module_id: bundle.manifest.id,
        version,
    })
}

/// The highest version of `module_id` satisfying `constraint`.
///
/// `constraint` absent means any version, which is what a human typing a bare
/// name asks for.
pub fn resolve(dir: &Path, module_id: &str, constraint: Option<&str>) -> Result<Candidate> {
    let requirement = match constraint {
        Some(text) => VersionReq::parse(text).map_err(|e| CoreError::UnresolvableModule {
            module_id: module_id.to_string(),
            constraint: text.to_string(),
            reason: format!("the constraint is not valid semver: {e}"),
        })?,
        None => VersionReq::STAR,
    };

    let scan = scan(dir)?;
    let named: Vec<&Candidate> = scan
        .candidates
        .iter()
        .filter(|c| c.module_id == module_id)
        .collect();

    if named.is_empty() {
        // Distinguishing the two cases matters: a repository holding a bundle
        // for this id that failed to verify is a different situation from one
        // that never had it, and only one of them is worth investigating.
        let broken: Vec<&Rejected> = scan
            .rejected
            .iter()
            .filter(|r| {
                r.path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().contains(module_id))
            })
            .collect();

        let reason = if broken.is_empty() {
            "no bundle in this repository publishes it".to_string()
        } else {
            format!(
                "{} bundle(s) name it and none of them held up: {}",
                broken.len(),
                broken
                    .iter()
                    .map(|r| r.reason.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        };

        return Err(CoreError::UnresolvableModule {
            module_id: module_id.to_string(),
            constraint: constraint.unwrap_or("*").to_string(),
            reason,
        });
    }

    named
        .iter()
        .filter(|c| requirement.matches(&c.version))
        .max_by(|a, b| a.version.cmp(&b.version))
        .map(|c| (*c).clone())
        .ok_or_else(|| {
            let mut available: Vec<String> = named.iter().map(|c| c.version.to_string()).collect();
            available.sort();
            CoreError::UnresolvableModule {
                module_id: module_id.to_string(),
                constraint: constraint.unwrap_or("*").to_string(),
                reason: format!("published versions are {}", available.join(", ")),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a signed bundle for `id` at `version`, in `dir`.
    fn publish(dir: &Path, id: &str, version: &str) -> PathBuf {
        crate::test_support::write_bundle(dir, id, version, true)
    }

    /// The same, but signed with a key that does not match the manifest.
    fn publish_broken(dir: &Path, id: &str, version: &str) -> PathBuf {
        crate::test_support::write_bundle(dir, id, version, false)
    }

    #[test]
    fn the_highest_version_that_satisfies_the_constraint_wins() {
        let dir = tempfile::tempdir().unwrap();
        publish(dir.path(), "dev.thalyx.demo", "1.0.0");
        publish(dir.path(), "dev.thalyx.demo", "1.4.2");
        publish(dir.path(), "dev.thalyx.demo", "2.0.0");

        let resolved = resolve(dir.path(), "dev.thalyx.demo", Some("^1.0")).unwrap();
        assert_eq!(resolved.version, Version::parse("1.4.2").unwrap());
    }

    #[test]
    fn no_constraint_means_the_highest_there_is() {
        let dir = tempfile::tempdir().unwrap();
        publish(dir.path(), "dev.thalyx.demo", "1.0.0");
        publish(dir.path(), "dev.thalyx.demo", "2.1.0");

        let resolved = resolve(dir.path(), "dev.thalyx.demo", None).unwrap();
        assert_eq!(resolved.version, Version::parse("2.1.0").unwrap());
    }

    #[test]
    fn a_bundle_whose_signature_does_not_verify_is_not_a_candidate() {
        let dir = tempfile::tempdir().unwrap();
        publish(dir.path(), "dev.thalyx.demo", "1.0.0");
        publish_broken(dir.path(), "dev.thalyx.demo", "9.9.9");

        let resolved = resolve(dir.path(), "dev.thalyx.demo", None).unwrap();
        assert_eq!(
            resolved.version,
            Version::parse("1.0.0").unwrap(),
            "a repository must not be able to steer a resolution by dropping in \
             a higher version it cannot sign"
        );
    }

    #[test]
    fn a_repository_with_one_bad_file_still_resolves_the_others() {
        let dir = tempfile::tempdir().unwrap();
        publish(dir.path(), "dev.thalyx.demo", "1.0.0");
        std::fs::write(dir.path().join("garbage.thmod"), b"not a tar at all").unwrap();

        let scan = scan(dir.path()).unwrap();
        assert_eq!(scan.candidates.len(), 1);
        assert_eq!(scan.rejected.len(), 1);
        assert!(resolve(dir.path(), "dev.thalyx.demo", None).is_ok());
    }

    #[test]
    fn a_module_nobody_published_says_so_rather_than_guessing() {
        let dir = tempfile::tempdir().unwrap();
        publish(dir.path(), "dev.thalyx.demo", "1.0.0");

        match resolve(dir.path(), "dev.thalyx.absent", None) {
            Err(CoreError::UnresolvableModule { reason, .. }) => {
                assert!(reason.contains("no bundle"), "unhelpful reason: {reason}");
            }
            other => panic!("expected an unresolvable error, got {other:?}"),
        }
    }

    #[test]
    fn a_constraint_nothing_satisfies_reports_what_is_published() {
        let dir = tempfile::tempdir().unwrap();
        publish(dir.path(), "dev.thalyx.demo", "1.0.0");
        publish(dir.path(), "dev.thalyx.demo", "1.2.0");

        match resolve(dir.path(), "dev.thalyx.demo", Some("^3")) {
            Err(CoreError::UnresolvableModule { reason, .. }) => {
                assert!(
                    reason.contains("1.0.0") && reason.contains("1.2.0"),
                    "{reason}"
                );
            }
            other => panic!("expected an unresolvable error, got {other:?}"),
        }
    }

    #[test]
    fn scanning_is_ordered_so_two_machines_agree() {
        let dir = tempfile::tempdir().unwrap();
        for version in ["3.0.0", "1.0.0", "2.0.0"] {
            publish(dir.path(), "dev.thalyx.demo", version);
        }
        let first = scan(dir.path()).unwrap();
        let second = scan(dir.path()).unwrap();
        let names =
            |s: &Scan| -> Vec<PathBuf> { s.candidates.iter().map(|c| c.path.clone()).collect() };
        assert_eq!(names(&first), names(&second));
    }
}
