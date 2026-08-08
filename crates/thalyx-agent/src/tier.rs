//! The four tiers, and the difference between a number somebody measured and a
//! number somebody guessed.
//!
//! Implements the table in `vault/02-Arquitectura/Gamas-de-Modelo.md`. One
//! family, four sizes, chosen by the user according to their hardware — because
//! anchoring a single model would put a RAM requirement in front of anyone who
//! wants to try Thalyx, and a requirement that excludes ordinary machines is not
//! a performance detail.
//!
//! ## Why the sizes are typed as estimates
//!
//! The decree says the sizes "are approximate until the bench measures them; the
//! table is corrected with the real figures, not left with the estimated ones".
//! A `u64` of bytes cannot say which of those two it is, and a field that cannot
//! say it will eventually be read as measured by somebody who was not there when
//! it was written. So the estimate is [`Estimate`], it prints with a `~`, and
//! `thalyx agent bench` is what replaces it — see [`Tier::disk`].
//!
//! Nothing here downloads anything. The weights are a file the human puts on the
//! machine and names with `thalyx agent model`, which is also the only place a
//! path to them is ever recorded.

use std::fmt;

/// One of the four sizes a user can choose between.
///
/// The order is smallest to largest and [`Tier::ALL`] relies on it, because a
/// listing that jumps around is a listing somebody has to read twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Light,
    Medium,
    High,
    Max,
}

/// A figure nobody has measured yet.
///
/// Exists so that `Tier::disk()` cannot be mistaken for a fact. When the bench
/// runs on real weights it reports the real byte count, and that is what gets
/// written back into the vault table — the estimate is never quietly promoted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Estimate(pub u64);

impl fmt::Display for Estimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "~{:.1} GB", self.0 as f64 / 1e9)
    }
}

impl Tier {
    pub const ALL: [Tier; 4] = [Tier::Light, Tier::Medium, Tier::High, Tier::Max];

    /// The name the user types, in the language the user speaks to Thalyx in.
    ///
    /// Spanish because `CLAUDE.md` puts the human-facing side of the project in
    /// Spanish and this is one — it is what somebody types after `thalyx agent
    /// model`. Everything the code says about it stays in English.
    pub const fn name(self) -> &'static str {
        match self {
            Tier::Light => "ligera",
            Tier::Medium => "media",
            Tier::High => "alta",
            Tier::Max => "maxima",
        }
    }

    /// The model this tier is, verbatim from the decree.
    ///
    /// One family across all four, so that the only thing varying between them
    /// is size and a bench result is attributable to it. Changing family is
    /// changing all four at once — a single tier swapped out would destroy the
    /// comparability the whole decree exists for.
    pub const fn model(self) -> &'static str {
        match self {
            Tier::Light => "Qwen2.5-1.5B-Instruct-Q4_K_M",
            Tier::Medium => "Qwen2.5-3B-Instruct-Q4_K_M",
            Tier::High => "Qwen2.5-7B-Instruct-Q4_K_M",
            Tier::Max => "Qwen2.5-14B-Instruct-Q4_K_M",
        }
    }

    /// What the weights file is expected to weigh. **Estimated, never measured.**
    pub const fn disk(self) -> Estimate {
        Estimate(match self {
            Tier::Light => 1_100_000_000,
            Tier::Medium => 2_000_000_000,
            Tier::High => 4_700_000_000,
            Tier::Max => 9_000_000_000,
        })
    }

    /// The RAM the decree says the tier asks for, in bytes.
    ///
    /// Also unmeasured, and the bench reports resident set size against it. The
    /// interesting failure is a tier that fits the file on disk and does not fit
    /// in memory, which looks like a crash of Thalyx and is not one.
    pub const fn ram(self) -> Estimate {
        Estimate(match self {
            Tier::Light => 4_000_000_000,
            Tier::Medium => 8_000_000_000,
            Tier::High => 16_000_000_000,
            Tier::Max => 32_000_000_000,
        })
    }

    pub fn parse(name: &str) -> Option<Tier> {
        // Unicode-aware, not ASCII-aware: `MÁXIMA` has to fold to `máxima`, and
        // `to_ascii_lowercase` leaves the accented letter capital — so the tier
        // would be selectable in lower case and not in upper, which nobody
        // would think to report as a bug.
        let name = name.trim().to_lowercase();
        Tier::ALL.into_iter().find(|tier| {
            tier.name() == name
                // `máxima` is how anyone would actually type it. Accepting only
                // the unaccented spelling would make the largest tier the one
                // the user cannot select by writing its name correctly.
                || (*tier == Tier::Max && name == "máxima")
        })
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tier_can_be_typed_back_in() {
        for tier in Tier::ALL {
            assert_eq!(
                Tier::parse(tier.name()),
                Some(tier),
                "{tier} prints a name that does not select it"
            );
        }
    }

    #[test]
    fn the_largest_tier_answers_to_the_spelling_with_the_accent() {
        // Somebody typing Spanish types `máxima`. A tier only reachable by
        // misspelling it is a tier that is not really on offer.
        assert_eq!(Tier::parse("máxima"), Some(Tier::Max));
        assert_eq!(Tier::parse("maxima"), Some(Tier::Max));
        assert_eq!(Tier::parse("  MÁXIMA "), Some(Tier::Max));
    }

    #[test]
    fn a_name_that_is_not_a_tier_selects_nothing() {
        for name in ["", "grande", "qwen", "7b", "light"] {
            assert_eq!(Tier::parse(name), None, "{name:?} selected a tier");
        }
    }

    #[test]
    fn the_four_tiers_are_four_distinct_models_of_one_family() {
        let models: Vec<&str> = Tier::ALL.iter().map(|t| t.model()).collect();
        let mut unique = models.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 4, "two tiers name the same weights");
        assert!(
            models.iter().all(|m| m.starts_with("Qwen2.5-")),
            "one family across all four is what makes a bench result attributable \
             to size rather than to the prompt suiting one family better: {models:?}"
        );
    }

    #[test]
    fn the_tiers_get_bigger_in_the_order_they_are_listed() {
        // A listing out of order is one somebody reads twice, and `ALL` is what
        // `thalyx agent model` prints.
        for pair in Tier::ALL.windows(2) {
            assert!(
                pair[0].disk().0 < pair[1].disk().0 && pair[0].ram().0 < pair[1].ram().0,
                "{} is not smaller than {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn an_estimate_prints_as_one_rather_than_as_a_measurement() {
        assert_eq!(Estimate(2_000_000_000).to_string(), "~2.0 GB");
    }
}
