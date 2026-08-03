//! What a fact was recorded against, and whether it still holds.
//!
//! `vault/03-Primitivas/Memoria-Persistente.md`: *cada hecho guarda el estado
//! del índice en el momento de registrarlo. Si ese estado ya no se sostiene el
//! hecho se marca como no verificado. No se borra: dejar de ser comprobable no
//! es lo mismo que ser falso.*
//!
//! ## Why it is not a fingerprint of the whole tree
//!
//! The obvious reading — hash the index, compare later — makes every fact
//! unverified the moment anything anywhere changes, which is within seconds on
//! a machine anybody is using. A memory where everything is always doubtful is
//! the same as no memory at all.
//!
//! So a fact is witnessed against **the specific paths it is about**. Editing
//! an unrelated file leaves it verified; editing the file it describes does
//! not. And the report says which paths moved, because "something changed"
//! that cannot say what is barely better than silence.

use std::path::Path;

/// One path a fact depends on, as it was when the fact was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessedPath {
    pub path: String,
    pub size: u64,
    pub mtime_ns: i64,
    /// Whether the path existed at all. A fact can legitimately be about
    /// something's *absence*, and "it is still missing" is a real check.
    pub existed: bool,
}

impl WitnessedPath {
    /// Record a path as it is now.
    pub fn observe(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(metadata) => Self {
                path: path.display().to_string(),
                size: metadata.len(),
                mtime_ns: mtime_nanos(&metadata),
                existed: true,
            },
            Err(_) => Self {
                path: path.display().to_string(),
                size: 0,
                mtime_ns: 0,
                existed: false,
            },
        }
    }

    /// Whether the path is still as it was.
    fn still_holds(&self) -> bool {
        match std::fs::metadata(&self.path) {
            Ok(metadata) => {
                self.existed
                    && self.size == metadata.len()
                    && self.mtime_ns == mtime_nanos(&metadata)
            }
            Err(_) => !self.existed,
        }
    }
}

/// Everything a fact was recorded against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Witness {
    pub paths: Vec<WitnessedPath>,
}

impl Witness {
    pub fn nothing() -> Self {
        Self::default()
    }

    /// Witness a fact against the paths it is about.
    pub fn over<I, P>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        Self {
            paths: paths
                .into_iter()
                .map(|path| WitnessedPath::observe(path.as_ref()))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Re-check every path, now.
    pub fn standing(&self) -> Standing {
        if self.paths.is_empty() {
            return Standing::Unwitnessed;
        }

        let moved: Vec<String> = self
            .paths
            .iter()
            .filter(|path| !path.still_holds())
            .map(|path| path.path.clone())
            .collect();

        if moved.is_empty() {
            Standing::Verified
        } else {
            Standing::Unverified { moved }
        }
    }
}

/// How much weight a fact can still carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Everything it was recorded against is still as it was.
    Verified,
    /// Something it depends on moved. The fact is not deleted and not called
    /// false — it is called uncheckable, and the paths are named.
    Unverified { moved: Vec<String> },
    /// It was recorded with nothing to check against.
    ///
    /// Distinct from `Verified` on purpose. A fact nobody can contradict is
    /// not a fact anybody has confirmed, and presenting the two the same way
    /// is how an agent ends up sounding certain about something it never
    /// checked.
    Unwitnessed,
}

impl Standing {
    pub fn is_verified(&self) -> bool {
        matches!(self, Standing::Verified)
    }

    /// How this should be said out loud.
    pub fn describe(&self) -> String {
        match self {
            Standing::Verified => "still checks out".to_string(),
            Standing::Unwitnessed => "recorded with nothing to check it against".to_string(),
            Standing::Unverified { moved } => format!(
                "NO LONGER VERIFIABLE — {} changed since this was recorded",
                moved.join(", ")
            ),
        }
    }
}

pub(crate) fn mtime_nanos(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos() as i64)
        // No usable mtime means the file cannot be told apart from a changed
        // one. Zero compares unequal to any real value, so the fact fails to
        // verify rather than passing on a technicality.
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fact_about_an_untouched_file_still_checks_out() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("subject");
        std::fs::write(&file, "as it was").unwrap();

        let witness = Witness::over([&file]);
        assert_eq!(witness.standing(), Standing::Verified);
    }

    #[test]
    fn editing_the_file_a_fact_is_about_makes_it_unverifiable_and_says_which() {
        // Not false. Uncheckable. The distinction is the whole point: an agent
        // that deleted the record would lose what it knew, and one that kept
        // asserting it would be confidently wrong.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("subject");
        std::fs::write(&file, "before").unwrap();

        let witness = Witness::over([&file]);
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&file, "after it changed").unwrap();

        match witness.standing() {
            Standing::Unverified { moved } => {
                assert_eq!(moved, vec![file.display().to_string()]);
            }
            other => panic!("expected the fact to stop verifying, got {other:?}"),
        }
    }

    #[test]
    fn editing_an_unrelated_file_leaves_the_fact_alone() {
        // The reason a witness is not a fingerprint of the whole tree. If it
        // were, every fact would be doubtful within seconds of anyone using
        // the machine, and a memory where everything is doubtful is no memory.
        let dir = tempfile::tempdir().unwrap();
        let subject = dir.path().join("subject");
        let elsewhere = dir.path().join("elsewhere");
        std::fs::write(&subject, "the fact is about this").unwrap();
        std::fs::write(&elsewhere, "and not this").unwrap();

        let witness = Witness::over([&subject]);
        std::fs::write(&elsewhere, "changed, and it does not matter").unwrap();

        assert_eq!(witness.standing(), Standing::Verified);
    }

    #[test]
    fn a_fact_about_something_missing_verifies_while_it_stays_missing() {
        // "The module is not installed" is a fact, and it is checkable.
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("never-existed");

        let witness = Witness::over([&absent]);
        assert_eq!(witness.standing(), Standing::Verified);

        std::fs::write(&absent, "it exists now").unwrap();
        assert!(!witness.standing().is_verified());
    }

    #[test]
    fn deleting_the_file_a_fact_is_about_makes_it_unverifiable() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("subject");
        std::fs::write(&file, "here for now").unwrap();

        let witness = Witness::over([&file]);
        std::fs::remove_file(&file).unwrap();

        assert!(matches!(witness.standing(), Standing::Unverified { .. }));
    }

    #[test]
    fn a_fact_with_nothing_to_check_is_not_the_same_as_a_verified_one() {
        // A fact nobody can contradict is not a fact anybody confirmed.
        // Collapsing the two is how an agent ends up sounding certain about
        // something it never checked.
        let witness = Witness::nothing();
        assert_eq!(witness.standing(), Standing::Unwitnessed);
        assert!(!witness.standing().is_verified());
    }

    #[test]
    fn the_description_names_what_moved() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("subject");
        std::fs::write(&file, "a").unwrap();
        let witness = Witness::over([&file]);
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&file, "bb").unwrap();

        let described = witness.standing().describe();
        assert!(described.contains("NO LONGER VERIFIABLE"), "{described}");
        assert!(described.contains("subject"), "{described}");
    }
}
