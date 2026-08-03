//! How much an index is allowed to let the counter decide, and how it
//! stops being allowed.
//!
//! `vault/03-Primitivas/FS-en-Grafo.md`: the shortcut is not asserted, it is
//! earned. These check the part that outlives a process — a setting on disk
//! that survives until something takes it back — and the one property that
//! matters more than any of it: nothing has to remember to turn it off.

use thalyx_graph::{Index, MutationCounter, Result, Trust, Watcher};

/// A counter under the test's control, so coverage can be broken on purpose.
struct Fake {
    total: std::cell::Cell<u64>,
    complete: bool,
    readable: std::cell::Cell<bool>,
}

impl Fake {
    fn new() -> Self {
        Self {
            total: std::cell::Cell::new(100),
            complete: true,
            readable: std::cell::Cell::new(true),
        }
    }

    fn with_a_hole(mut self) -> Self {
        self.complete = false;
        self
    }
}

impl MutationCounter for Fake {
    fn total(&self) -> Result<u64> {
        if !self.readable.get() {
            return Err(thalyx_graph::GraphError::Io {
                path: std::path::PathBuf::from("/sys/fs/bpf/thalyx/maps/x"),
                source: std::io::Error::other("the map went away"),
            });
        }
        Ok(self.total.get())
    }

    fn claims_complete_coverage(&self) -> bool {
        self.complete
    }
}

fn index() -> (tempfile::TempDir, Index) {
    let dir = tempfile::tempdir().unwrap();
    let tree = dir.path().join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(tree.join("a.rs"), "fn main() {}\n").unwrap();

    let mut index = Index::open(&dir.path().join("index.db"), &tree).unwrap();
    index.build().unwrap();
    (dir, index)
}

#[test]
fn an_index_walks_until_something_says_otherwise() {
    // The default has to be the safe one. A shortcut that was on until
    // somebody turned it off would be wrong on every machine it was never
    // verified against, which is most of them.
    let (_dir, index) = index();
    assert_eq!(index.trust().unwrap(), Trust::WalkAlways);
    assert_eq!(index.trust_earned().unwrap(), None);
}

#[test]
fn the_setting_and_what_earned_it_survive_being_reopened() {
    let dir = tempfile::tempdir().unwrap();
    let tree = dir.path().join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    let database = dir.path().join("index.db");

    {
        let index = Index::open(&database, &tree).unwrap();
        index
            .set_trust(Trust::Counter, Some("verified on 2026-08-03"))
            .unwrap();
    }

    let index = Index::open(&database, &tree).unwrap();
    assert_eq!(index.trust().unwrap(), Trust::Counter);
    assert_eq!(
        index.trust_earned().unwrap().as_deref(),
        Some("verified on 2026-08-03")
    );
}

#[test]
fn giving_it_back_clears_what_earned_it() {
    // A note explaining why the fast path is on, left behind on an index that
    // now walks, is a reason for a thing that is not happening.
    let (_dir, index) = index();
    index
        .set_trust(Trust::Counter, Some("earned here"))
        .unwrap();
    index.set_trust(Trust::WalkAlways, None).unwrap();

    assert_eq!(index.trust().unwrap(), Trust::WalkAlways);
    assert_eq!(index.trust_earned().unwrap(), None);
}

#[test]
fn a_trusted_index_skips_the_walk_when_the_counter_has_not_moved() {
    let (_dir, index) = index();
    let counter = Fake::new();
    let baseline = counter.total.get();

    let mut watcher = Watcher::resuming_from(counter, baseline).with_trust(Trust::Counter);
    assert!(watcher.freshness(&index).unwrap().is_current());
}

#[test]
fn the_shortcut_is_given_back_on_its_own_when_the_counter_stops_answering() {
    // The property that makes the setting safe to persist: nothing has to
    // notice and turn it off. A watcher that was reloaded, a map that went
    // away, a hook that is gone — each one lands back on the walk without any
    // code deciding to.
    let (dir, index) = index();
    let counter = Fake::new();
    let baseline = counter.total.get();
    counter.readable.set(false);

    let mut watcher = Watcher::resuming_from(counter, baseline).with_trust(Trust::Counter);

    // The tree really has changed, and only the walk can see it.
    std::fs::write(dir.path().join("tree").join("b.rs"), "fn other() {}\n").unwrap();
    assert!(
        !watcher.freshness(&index).unwrap().is_current(),
        "an unreadable counter let the index answer `current` about a changed tree"
    );
}

#[test]
fn a_counter_that_admits_a_hole_is_not_trusted_however_the_setting_reads() {
    // Two keys, and the setting is only one of them. An index carrying
    // `Trust::Counter` from a machine where it was earned must not take the
    // shortcut on a machine whose hook set is incomplete.
    let (dir, index) = index();
    let counter = Fake::new().with_a_hole();
    let baseline = counter.total.get();

    let mut watcher = Watcher::resuming_from(counter, baseline).with_trust(Trust::Counter);

    std::fs::write(dir.path().join("tree").join("b.rs"), "fn other() {}\n").unwrap();
    assert!(
        !watcher.freshness(&index).unwrap().is_current(),
        "the shortcut fired on a counter that says it misses things"
    );
}

#[test]
fn a_counter_that_moved_falls_back_to_the_walk_and_finds_the_truth() {
    let (dir, index) = index();
    let counter = Fake::new();
    let baseline = counter.total.get();

    // Something happened, and the tree really did change.
    counter.total.set(baseline + 7);
    let mut watcher = Watcher::resuming_from(counter, baseline).with_trust(Trust::Counter);

    std::fs::write(dir.path().join("tree").join("b.rs"), "fn other() {}\n").unwrap();
    assert!(!watcher.freshness(&index).unwrap().is_current());
}

#[test]
fn a_counter_that_moved_for_something_elsewhere_costs_only_a_walk() {
    // The harmless direction, and it has to stay harmless: the count moved,
    // the tree did not, and the answer is still correct — just arrived the
    // slow way.
    let (_dir, index) = index();
    let counter = Fake::new();
    let baseline = counter.total.get();
    counter.total.set(baseline + 7);

    let mut watcher = Watcher::resuming_from(counter, baseline).with_trust(Trust::Counter);
    assert!(watcher.freshness(&index).unwrap().is_current());
}
