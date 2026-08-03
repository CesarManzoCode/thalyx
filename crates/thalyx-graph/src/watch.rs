//! Letting the kernel tell the index when it is still current.
//!
//! Checking freshness walks the entire tree, on every query. That is honest —
//! the filesystem is the truth and the index is a cache — and it does not
//! scale. `thalyx-watch` counts every filesystem mutation the kernel sees, so
//! the expensive question has a cheap answer: **if the count has not moved
//! since the index was built, nothing changed, and there is nothing to walk.**
//!
//! ## Why the fast path is off until it is proven
//!
//! That shortcut is only sound if the LSM's hooks catch *every* way a file can
//! change. They might not: a write through an already-open descriptor does not
//! pass through `inode_create` or `inode_rename`, and which hooks a kernel
//! exposes varies with its configuration. A hook set with a hole would make
//! the index answer "current" while a file underneath it had changed — and
//! `vault/04-Flujo-Canonico/Coherencia-Doble-Ruta.md` is explicit that an index
//! claiming to be current when it is not is worse than one that says it does
//! not know.
//!
//! So the shortcut is not asserted, it is **earned**. [`Trust::WalkAlways`] is
//! the default: the counter is used to explain, never to conclude.
//! [`Watcher::verify`] runs both answers side by side on a real machine and
//! reports whether they agreed. Only a machine where they have agreed should
//! move to [`Trust::Counter`].
//!
//! ## The asymmetry that matters
//!
//! The two ways the answers can disagree are not equally bad.
//!
//! - The counter says *changed*, the walk says *current*: harmless. Something
//!   moved elsewhere on the machine, or a file was written back identically.
//!   The index is merely more cautious than it needed to be.
//! - The counter says *current*, the walk says *changed*: **a coverage hole**.
//!   This is the one that makes the index lie, and it permanently breaks
//!   coverage when seen.

use crate::staleness::Freshness;
use crate::{Index, Result};

/// The kernel's running total of filesystem mutations.
///
/// A trait so the discipline around it can be tested without a kernel — which
/// is most of what this module is. The kernel-backed implementation lives in
/// `thalyx-watch`.
pub trait MutationCounter {
    /// Mutations observed since the watcher was loaded.
    ///
    /// Monotonic while loaded. It resets when the program is reloaded, and a
    /// decrease is how userspace notices that happened.
    fn total(&self) -> Result<u64>;

    /// Whether this counter's hook set is *claimed* to see every way a file
    /// can change.
    ///
    /// A claim, not a proof — which is why it is not enough on its own to
    /// enable the fast path. See [`Watcher::verify`].
    fn claims_complete_coverage(&self) -> bool {
        false
    }
}

/// So a caller can choose between counters at run time.
///
/// Whether a tree's mutations are counted on their own or machine-wide is
/// decided by the machine, not at compile time — and either way every rule in
/// this module applies unchanged, because scoping narrows what is counted, not
/// what may be concluded from it.
impl<T: MutationCounter + ?Sized> MutationCounter for Box<T> {
    fn total(&self) -> Result<u64> {
        (**self).total()
    }

    fn claims_complete_coverage(&self) -> bool {
        (**self).claims_complete_coverage()
    }
}

/// Whether the watcher can account for everything since the index was built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coverage {
    /// Unbroken since the build, which happened at this count.
    Unbroken { baseline: u64 },
    /// Something happened that the watcher cannot account for.
    Broken { reason: String },
}

impl Coverage {
    pub fn is_unbroken(&self) -> bool {
        matches!(self, Coverage::Unbroken { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Coverage::Unbroken { baseline } => {
                format!("unbroken since the index was built at count {baseline}")
            }
            Coverage::Broken { reason } => format!("BROKEN: {reason}"),
        }
    }
}

/// How much the counter is allowed to decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Trust {
    /// Walk the tree every time. The counter only explains the result.
    #[default]
    WalkAlways,
    /// Skip the walk when the counter has not moved.
    ///
    /// Only correct on a machine where [`Watcher::verify`] has agreed.
    Counter,
}

/// What a verification run found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    /// What the counter alone would have concluded.
    pub counter_said_current: bool,
    /// What walking the tree found.
    pub walk_said_current: bool,
    pub coverage: Coverage,
}

impl Verification {
    pub fn agreed(&self) -> bool {
        self.counter_said_current == self.walk_said_current
    }

    /// The disagreement that makes the index lie.
    ///
    /// Only one direction counts. The counter being too cautious costs a walk;
    /// the counter being too confident costs correctness.
    pub fn found_a_coverage_hole(&self) -> bool {
        self.counter_said_current && !self.walk_said_current
    }

    pub fn describe(&self) -> String {
        if self.found_a_coverage_hole() {
            return "COVERAGE HOLE: the counter said nothing had changed and the tree \
                    disagrees. Something can change a file without the kernel hooks \
                    seeing it, so the fast path would make the index lie."
                .to_string();
        }
        if !self.agreed() {
            return "counter was more cautious than the tree needed; harmless".to_string();
        }
        "counter and tree agreed".to_string()
    }
}

/// Ties an index to the kernel's mutation counter.
pub struct Watcher<C: MutationCounter> {
    counter: C,
    coverage: Coverage,
    trust: Trust,
}

impl<C: MutationCounter> Watcher<C> {
    /// Start watching, with no coverage yet.
    ///
    /// Coverage begins broken on purpose. Nothing has been observed, so
    /// nothing can be vouched for, and the first freshness check walks.
    pub fn new(counter: C) -> Self {
        Self {
            counter,
            coverage: Coverage::Broken {
                reason: "nothing observed yet".to_string(),
            },
            trust: Trust::WalkAlways,
        }
    }

    /// Resume from a baseline recorded by an earlier run.
    pub fn resuming_from(counter: C, baseline: u64) -> Self {
        let mut watcher = Self::new(counter);
        match watcher.counter.total() {
            // A count below the baseline means the program was reloaded and
            // started again from zero. Everything between the two is invisible.
            Ok(total) if total < baseline => {
                watcher.coverage = Coverage::Broken {
                    reason: format!(
                        "the counter went backwards ({total} < {baseline}); \
                         the watcher was reloaded and the gap cannot be recovered"
                    ),
                };
            }
            Ok(_) => watcher.coverage = Coverage::Unbroken { baseline },
            Err(error) => {
                watcher.coverage = Coverage::Broken {
                    reason: format!("the counter could not be read: {error}"),
                };
            }
        }
        watcher
    }

    pub fn with_trust(mut self, trust: Trust) -> Self {
        self.trust = trust;
        self
    }

    pub fn coverage(&self) -> &Coverage {
        &self.coverage
    }

    pub fn trust(&self) -> Trust {
        self.trust
    }

    /// Record that the index has just been rebuilt.
    ///
    /// This is the only thing that can repair broken coverage: after a full
    /// sweep the index matches the tree whatever was missed before.
    pub fn rebuilt(&mut self) -> Coverage {
        self.coverage = match self.counter.total() {
            Ok(baseline) => Coverage::Unbroken { baseline },
            Err(error) => Coverage::Broken {
                reason: format!("the counter could not be read: {error}"),
            },
        };
        self.coverage.clone()
    }

    /// The baseline to persist, if there is one worth persisting.
    pub fn baseline(&self) -> Option<u64> {
        match self.coverage {
            Coverage::Unbroken { baseline } => Some(baseline),
            Coverage::Broken { .. } => None,
        }
    }

    /// Whether the counter alone says the tree is untouched.
    ///
    /// `None` when it cannot say — no coverage, unreadable counter, or a
    /// counter that went backwards. Reading it can *break* coverage, which is
    /// why it takes `&mut self`.
    pub fn counter_says_current(&mut self) -> Option<bool> {
        let Coverage::Unbroken { baseline } = self.coverage else {
            return None;
        };

        match self.counter.total() {
            Ok(total) if total < baseline => {
                self.coverage = Coverage::Broken {
                    reason: format!(
                        "the counter went backwards ({total} < {baseline}); \
                         the watcher was reloaded"
                    ),
                };
                None
            }
            Ok(total) => Some(total == baseline),
            Err(error) => {
                self.coverage = Coverage::Broken {
                    reason: format!("the counter could not be read: {error}"),
                };
                None
            }
        }
    }

    /// The index's freshness, using the counter as far as it is trusted.
    pub fn freshness(&mut self, index: &Index) -> Result<Freshness> {
        if self.trust == Trust::Counter
            && self.counter.claims_complete_coverage()
            && self.counter_says_current() == Some(true)
        {
            return Ok(Freshness::Current);
        }
        index.freshness()
    }

    /// Ask both, and report whether they agreed.
    ///
    /// The experiment that decides whether [`Trust::Counter`] is safe on this
    /// machine. A coverage hole breaks coverage permanently: once the counter
    /// has been caught being too confident, this run's baseline cannot be
    /// trusted again until a rebuild.
    pub fn verify(&mut self, index: &Index) -> Result<Verification> {
        let counter_said_current = self.counter_says_current();
        let walk_said_current = index.freshness()?.is_current();

        let verification = Verification {
            counter_said_current: counter_said_current.unwrap_or(false),
            walk_said_current,
            coverage: self.coverage.clone(),
        };

        if verification.found_a_coverage_hole() {
            self.coverage = Coverage::Broken {
                reason: "the counter claimed nothing had changed and the tree disagreed"
                    .to_string(),
            };
        }

        Ok(verification)
    }
}

/// A counter driven by hand, for tests.
#[derive(Debug, Default)]
pub struct MemoryCounter {
    total: std::sync::atomic::AtomicU64,
    readable: std::sync::atomic::AtomicBool,
    complete: bool,
}

impl MemoryCounter {
    pub fn new() -> Self {
        Self {
            total: std::sync::atomic::AtomicU64::new(0),
            readable: std::sync::atomic::AtomicBool::new(true),
            complete: true,
        }
    }

    /// A counter whose hook set does not claim to see everything.
    pub fn incomplete() -> Self {
        Self {
            complete: false,
            ..Self::new()
        }
    }

    pub fn bump(&self, by: u64) {
        self.total
            .fetch_add(by, std::sync::atomic::Ordering::Relaxed);
    }

    /// Simulate the watcher being reloaded: the count starts again.
    pub fn reload(&self) {
        self.total.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn make_unreadable(&self) {
        self.readable
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl MutationCounter for MemoryCounter {
    fn total(&self) -> Result<u64> {
        if !self.readable.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(crate::GraphError::Io {
                path: std::path::PathBuf::from("<memory counter>"),
                source: std::io::Error::other("made unreadable for a test"),
            });
        }
        Ok(self.total.load(std::sync::atomic::Ordering::Relaxed))
    }

    fn claims_complete_coverage(&self) -> bool {
        self.complete
    }
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

    fn built(dir: &tempfile::TempDir) -> Index {
        let mut index = Index::in_memory(dir.path()).unwrap();
        index.build().unwrap();
        index
    }

    #[test]
    fn coverage_starts_broken_because_nothing_has_been_observed() {
        // A watcher that vouched for the past before it existed would be the
        // whole bug in one line.
        let watcher = Watcher::new(MemoryCounter::new());
        assert!(!watcher.coverage().is_unbroken());
        assert!(watcher.baseline().is_none());
    }

    #[test]
    fn rebuilding_is_what_repairs_coverage() {
        let mut watcher = Watcher::new(MemoryCounter::new());
        assert!(!watcher.coverage().is_unbroken());

        assert!(watcher.rebuilt().is_unbroken());
        assert_eq!(watcher.baseline(), Some(0));
    }

    #[test]
    fn the_fast_path_is_off_until_it_is_asked_for() {
        // Default trust walks. A shortcut that turned itself on would be a
        // correctness decision nobody made.
        let dir = tree(&[("a.rs", "\n")]);
        let index = built(&dir);
        let mut watcher = Watcher::new(MemoryCounter::new());
        watcher.rebuilt();

        assert_eq!(watcher.trust(), Trust::WalkAlways);

        // Change the tree without touching the counter — exactly the state a
        // hook hole would produce.
        std::fs::write(dir.path().join("b.rs"), "\n").unwrap();

        assert!(
            !watcher.freshness(&index).unwrap().is_current(),
            "the default must not believe a counter it has not verified"
        );
    }

    #[test]
    fn a_counter_that_has_not_moved_skips_the_walk_once_trusted() {
        let dir = tree(&[("a.rs", "\n")]);
        let index = built(&dir);
        let mut watcher = Watcher::new(MemoryCounter::new()).with_trust(Trust::Counter);
        watcher.rebuilt();

        assert!(watcher.freshness(&index).unwrap().is_current());
        assert_eq!(watcher.counter_says_current(), Some(true));
    }

    #[test]
    fn a_counter_that_moved_falls_back_to_the_walk() {
        let dir = tree(&[("a.rs", "\n")]);
        let index = built(&dir);
        let counter = MemoryCounter::new();
        let mut watcher = Watcher::new(counter).with_trust(Trust::Counter);
        watcher.rebuilt();

        std::fs::write(dir.path().join("b.rs"), "\n").unwrap();
        // The kernel would have counted that.
        watcher.counter.bump(1);

        let freshness = watcher.freshness(&index).unwrap();
        assert!(!freshness.is_current());
        // And the walk produced the detail, which the counter never could.
        assert!(freshness.describe().contains("1 added"));
    }

    #[test]
    fn a_counter_that_went_backwards_breaks_coverage() {
        // The watcher was reloaded and started from zero. Everything between
        // the old count and now happened where nobody was looking, and no
        // amount of counting afterwards recovers it.
        let dir = tree(&[("a.rs", "\n")]);
        let index = built(&dir);
        let counter = MemoryCounter::new();
        counter.bump(500);

        let mut watcher = Watcher::new(counter).with_trust(Trust::Counter);
        watcher.rebuilt();
        assert!(watcher.coverage().is_unbroken());

        watcher.counter.reload();
        // A file changed during the gap, which is exactly what a reload hides.
        std::fs::write(dir.path().join("b.rs"), "\n").unwrap();

        assert_eq!(watcher.counter_says_current(), None);
        assert!(!watcher.coverage().is_unbroken());
        assert!(watcher.coverage().describe().contains("backwards"));

        // After the reload the reloaded counter reads zero, which equals its
        // own fresh baseline — so a watcher that had not noticed would answer
        // "current". The answer has to come from the tree instead.
        assert!(
            !watcher.freshness(&index).unwrap().is_current(),
            "a reloaded counter must not be able to vouch for the gap"
        );
    }

    #[test]
    fn resuming_from_a_baseline_the_counter_cannot_support_starts_broken() {
        let counter = MemoryCounter::new();
        counter.bump(3);

        // A baseline from before a reload.
        let watcher = Watcher::resuming_from(counter, 900);
        assert!(!watcher.coverage().is_unbroken());
        assert!(watcher.coverage().describe().contains("reloaded"));
    }

    #[test]
    fn resuming_from_a_baseline_the_counter_still_supports_keeps_coverage() {
        let counter = MemoryCounter::new();
        counter.bump(1200);

        let watcher = Watcher::resuming_from(counter, 900);
        assert!(watcher.coverage().is_unbroken());
        assert_eq!(watcher.baseline(), Some(900));
    }

    #[test]
    fn an_unreadable_counter_breaks_coverage_rather_than_being_assumed_still() {
        // Fails closed, like everything else here. "I could not read it" must
        // never become "nothing changed".
        let dir = tree(&[("a.rs", "\n")]);
        let index = built(&dir);
        let counter = MemoryCounter::new();
        let mut watcher = Watcher::new(counter).with_trust(Trust::Counter);
        watcher.rebuilt();

        watcher.counter.make_unreadable();

        assert_eq!(watcher.counter_says_current(), None);
        assert!(!watcher.coverage().is_unbroken());
        // And the answer still comes from the tree.
        assert!(watcher.freshness(&index).unwrap().is_current());
    }

    #[test]
    fn a_counter_that_does_not_claim_full_coverage_never_gets_the_fast_path() {
        // Its hook set is known to miss things. Trusting it would be trusting
        // a claim nobody made.
        let dir = tree(&[("a.rs", "\n")]);
        let index = built(&dir);
        let mut watcher = Watcher::new(MemoryCounter::incomplete()).with_trust(Trust::Counter);
        watcher.rebuilt();

        std::fs::write(dir.path().join("b.rs"), "\n").unwrap();

        assert!(
            !watcher.freshness(&index).unwrap().is_current(),
            "a counter that admits it misses things must not be trusted"
        );
    }

    #[test]
    fn verification_agrees_when_nothing_has_happened() {
        let dir = tree(&[("a.rs", "\n")]);
        let index = built(&dir);
        let mut watcher = Watcher::new(MemoryCounter::new());
        watcher.rebuilt();

        let verification = watcher.verify(&index).unwrap();
        assert!(verification.agreed());
        assert!(!verification.found_a_coverage_hole());
        assert!(watcher.coverage().is_unbroken());
    }

    #[test]
    fn verification_names_a_coverage_hole_and_breaks_coverage_for_good() {
        // The tree changed and the kernel did not count it — a write through
        // an open descriptor, or a hook this kernel does not expose. This is
        // the whole reason the fast path has to be earned.
        let dir = tree(&[("a.rs", "\n")]);
        let index = built(&dir);
        let mut watcher = Watcher::new(MemoryCounter::new());
        watcher.rebuilt();

        std::fs::write(dir.path().join("b.rs"), "\n").unwrap();
        // The counter stays where it was: that is the hole.

        let verification = watcher.verify(&index).unwrap();
        assert!(!verification.agreed());
        assert!(verification.found_a_coverage_hole());
        assert!(verification.describe().contains("COVERAGE HOLE"));
        assert!(
            !watcher.coverage().is_unbroken(),
            "a proven hole must not leave coverage intact"
        );
    }

    #[test]
    fn a_counter_that_is_merely_too_cautious_is_not_a_hole() {
        // Something changed elsewhere on the machine, outside this tree. The
        // index pays for a walk it did not need, and nothing is wrong.
        let dir = tree(&[("a.rs", "\n")]);
        let index = built(&dir);
        let counter = MemoryCounter::new();
        let mut watcher = Watcher::new(counter);
        watcher.rebuilt();
        watcher.counter.bump(7);

        let verification = watcher.verify(&index).unwrap();
        assert!(!verification.agreed());
        assert!(!verification.found_a_coverage_hole());
        assert!(verification.describe().contains("cautious"));
        assert!(
            watcher.coverage().is_unbroken(),
            "being cautious must not cost coverage"
        );
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn a_baseline_survives_between_runs() {
        let dir = tempfile::tempdir().unwrap();
        let index = Index::in_memory(dir.path()).unwrap();

        assert_eq!(index.mutation_baseline().unwrap(), None);
        index.set_mutation_baseline(4321).unwrap();
        assert_eq!(index.mutation_baseline().unwrap(), Some(4321));

        index.set_mutation_baseline(9999).unwrap();
        assert_eq!(index.mutation_baseline().unwrap(), Some(9999));

        index.clear_mutation_baseline().unwrap();
        assert_eq!(index.mutation_baseline().unwrap(), None);
    }

    #[test]
    fn a_corrupt_trust_setting_reads_as_walking_rather_than_as_the_shortcut() {
        // The shortcut is the dangerous answer, so it must never be what a
        // damaged field — or a version of Thalyx that does not exist yet —
        // can produce. Fail closed, in the direction that costs a walk.
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        let index = Index::open(&dir.path().join("index.db"), &tree).unwrap();

        index.set_trust(Trust::Counter, None).unwrap();
        assert_eq!(index.trust().unwrap(), Trust::Counter);

        index
            .connection_for_tests()
            .execute(
                "UPDATE meta SET value = 'whatever-comes-next' WHERE key = 'trust'",
                [],
            )
            .unwrap();

        assert_eq!(index.trust().unwrap(), Trust::WalkAlways);
    }

    #[test]
    fn a_baseline_that_cannot_be_read_is_absent_rather_than_zero() {
        // Zero is a baseline the watcher would vouch for. A corrupt field must
        // never turn into a claim about what has happened since.
        let dir = tempfile::tempdir().unwrap();
        let index = Index::in_memory(dir.path()).unwrap();
        index.set_mutation_baseline(10).unwrap();

        index
            .connection_for_tests()
            .execute(
                "UPDATE meta SET value = 'not a number' WHERE key = 'mutation_baseline'",
                [],
            )
            .unwrap();

        assert_eq!(index.mutation_baseline().unwrap(), None);
    }
}
