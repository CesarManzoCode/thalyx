//! `thalyx graph` — building and querying the semantic index.
//!
//! Every command that prints query results also prints the index's freshness.
//! Not as a courtesy: the index is a cache over a filesystem the human is free
//! to change without telling Thalyx, so an answer without that caveat would be
//! presented as more authoritative than it is.

use clap::Subcommand;
use std::path::{Path, PathBuf};
use thalyx_graph::{Coverage, Freshness, Index, MutationCounter, Trust, Watcher};
use thalyx_watch::{KernelCounter, Scoping, TreeCounter};

type Fallible = Result<(), Box<dyn std::error::Error>>;

#[derive(Subcommand)]
pub enum GraphCommand {
    /// Index a directory tree
    Build {
        /// Tree to index. Defaults to the current directory.
        #[arg(default_value = ".")]
        tree: PathBuf,
    },
    /// Report whether the index still matches the tree
    Status {
        #[arg(default_value = ".")]
        tree: PathBuf,
        /// List every changed path instead of counting them
        #[arg(long)]
        detail: bool,
    },
    /// What this file depends on
    Deps {
        path: String,
        #[arg(long, default_value = ".")]
        tree: PathBuf,
    },
    /// What depends on this file
    ///
    /// The question the semantic index exists for: no directory walk can
    /// answer it, because dependency is not a property of location.
    Dependents {
        path: String,
        #[arg(long, default_value = ".")]
        tree: PathBuf,
    },
    /// Attach a tag to a file
    Tag {
        path: String,
        tag: String,
        #[arg(long, default_value = ".")]
        tree: PathBuf,
    },
    /// Remove a tag from a file
    Untag {
        path: String,
        tag: String,
        #[arg(long, default_value = ".")]
        tree: PathBuf,
    },
    /// Check what the kernel's mutation counter says against the tree itself
    ///
    /// The experiment that decides whether the counter may ever be believed on
    /// this machine. It asks both and reports whether they agreed.
    Verify {
        #[arg(default_value = ".")]
        tree: PathBuf,
    },
    /// What the kernel's watcher can see, with no index involved
    ///
    /// Separate from `status` because "can this counter be believed" is a
    /// question about the machine, not about any one index. It is also the
    /// only way to watch the count move while doing something to a file,
    /// which is how the hook set gets checked against reality rather than
    /// against its own list of hooks.
    Watcher {
        /// Report this tree's own count rather than the machine's
        #[arg(long)]
        tree: Option<PathBuf>,
    },
    /// Let the counter decide freshness, or take that back
    ///
    /// The fast path is not asserted, it is earned: switching it on runs the
    /// verification first and refuses unless it agrees. With no flag, this
    /// reports where the index stands and what earned it.
    Trust {
        #[arg(default_value = ".")]
        tree: PathBuf,
        /// Skip the walk when the kernel says nothing changed
        #[arg(long, conflicts_with = "walk")]
        counter: bool,
        /// Go back to walking the tree on every check
        #[arg(long)]
        walk: bool,
    },
    /// Files carrying a tag
    Tagged {
        tag: String,
        #[arg(long, default_value = ".")]
        tree: PathBuf,
    },
}

pub fn run(store_root: &Path, command: GraphCommand) -> Fallible {
    match command {
        GraphCommand::Build { tree } => {
            let tree = tree.canonicalize()?;
            let mut index = open(store_root, &tree)?;

            // Ask the kernel to count this tree's mutations on their own,
            // before the build rather than after: registration resets the
            // tree's count, and a reset after the build would throw away
            // everything that changed while it was running.
            let scoping = register(&tree);
            let report = index.build()?;

            // The counter's value at the moment the index matched the tree.
            // Recorded here because this is the only instant it is true.
            let mut watcher = Watcher::new(counter_for(&scoping));
            match watcher.rebuilt() {
                Coverage::Unbroken { baseline } => index.set_mutation_baseline(baseline)?,
                Coverage::Broken { .. } => index.clear_mutation_baseline()?,
            }

            println!("indexed {}", tree.display());
            println!("  {}", scoping.describe());
            println!(
                "  {} file(s), {} parsed",
                report.files_indexed, report.files_parsed
            );
            println!(
                "  {} dependenc(ies), {} resolved inside the tree",
                report.edges, report.edges_resolved
            );
            if report.skipped > 0 {
                println!("  {} skipped (unreadable or not text)", report.skipped);
            }
            let unresolved = report.edges - report.edges_resolved;
            if unresolved > 0 {
                println!();
                println!("{unresolved} reference(s) point outside the tree. They are kept as");
                println!("edges with no target rather than guessed at: an invented edge");
                println!("would make the graph confidently wrong.");
            }
            Ok(())
        }

        GraphCommand::Status { tree, detail } => {
            let tree = tree.canonicalize()?;
            let index = open(store_root, &tree)?;

            let scoping = thalyx_watch::scoping_for(&tree);
            println!("tree      {}", tree.display());
            println!("nodes     {}", index.node_count()?);
            println!("edges     {}", index.edge_count()?);
            print_coverage(&index, &scoping)?;

            // Through the watcher, so an index that earned the fast path
            // actually takes it. Asking the index directly would make the
            // whole earning mechanism decorative.
            let mut watcher = Watcher::new(counter_for(&scoping)).with_trust(index.trust()?);
            if let Some(baseline) = index.mutation_baseline()? {
                watcher = Watcher::resuming_from(counter_for(&scoping), baseline)
                    .with_trust(index.trust()?);
            }

            match watcher.freshness(&index)? {
                Freshness::Current => {
                    println!("freshness index is current");
                }
                Freshness::Stale(staleness) => {
                    println!(
                        "freshness {}",
                        Freshness::Stale(staleness.clone()).describe()
                    );
                    if detail {
                        for path in &staleness.added {
                            println!("          + {path}");
                        }
                        for path in &staleness.modified {
                            println!("          ~ {path}");
                        }
                        for path in &staleness.removed {
                            println!("          - {path}");
                        }
                        for path in &staleness.unreadable {
                            println!("          ? {path}");
                        }
                    }
                    println!();
                    println!("The filesystem is the truth; the index is a cache over it.");
                    println!("`thalyx graph build` brings it back into line.");
                }
            }
            Ok(())
        }

        GraphCommand::Watcher { tree } => {
            let counter = KernelCounter::default_map();

            // With a tree, the number that matters is that tree's. Reporting
            // the machine's total when a tree was named would answer a
            // different question with a number that looks like an answer.
            let scoped: Option<Box<dyn MutationCounter>> = match &tree {
                Some(tree) => {
                    let tree = tree.canonicalize()?;
                    let scoping = thalyx_watch::scoping_for(&tree);
                    println!("scope     {}", scoping.describe());
                    Some(counter_for(&scoping))
                }
                None => None,
            };

            match scoped
                .as_ref()
                .map_or_else(|| counter.total(), |c| c.total())
            {
                Ok(total) => println!("mutations {total}"),
                Err(_) => {
                    // Deliberately not a `mutations` line with a word where the
                    // number goes. Anything reading this for a count must come
                    // away with nothing, not with something that parses.
                    //
                    // And "not loaded" is only said when that is what happened.
                    // A map that is pinned and full of counts, which Thalyx
                    // failed to parse, once reported itself as an unattached
                    // watcher — sending the reader to reload something that had
                    // been working the whole time.
                    if counter.is_available() {
                        println!("watcher   attached, and Thalyx could not read its counter");
                        if let Some(error) = counter.unreadable() {
                            println!("  {error}");
                        }
                        println!();
                        println!("This is a defect in Thalyx, not a watcher that is missing.");
                        println!("The kernel is counting; the reader is what is wrong.");
                    } else {
                        println!("watcher   not loaded");
                        println!("  {}", counter.map().display());
                        println!();
                        println!("Every freshness check walks the whole tree until it is.");
                        println!("`make -C lsm load` attaches it.");
                    }
                    return Ok(());
                }
            }

            match counter.missing_hooks() {
                Ok(missing) if missing.is_empty() => {
                    println!(
                        "hooks     all {} attached",
                        thalyx_watch::REQUIRED_HOOKS.len()
                    );
                    println!();
                    println!("Nothing a process on this machine does to a file gets past all");
                    println!("of them — including a write through a descriptor that was");
                    println!("already open, which is the one that took the longest to cover.");
                    println!();
                    println!("Still machine-wide: it counts changes outside any indexed tree.");
                    println!("That costs walks that were not needed. It never hides one.");
                }
                Ok(missing) => {
                    println!(
                        "hooks     {} of {} attached",
                        thalyx_watch::REQUIRED_HOOKS.len() - missing.len(),
                        thalyx_watch::REQUIRED_HOOKS.len()
                    );
                    println!();
                    println!("MISSING — each of these is a way a file can change in silence:");
                    for hook in missing {
                        println!("  {hook}");
                    }
                    println!();
                    println!("The count cannot be believed while any of them is absent.");
                    println!("`make -C lsm hooks` shows which ones this kernel exposes.");
                }
                Err(error) => println!("hooks     could not be established: {error}"),
            }

            Ok(())
        }

        GraphCommand::Trust {
            tree,
            counter: earn,
            walk,
        } => {
            let tree = tree.canonicalize()?;
            let index = open(store_root, &tree)?;

            if walk {
                index.set_trust(Trust::WalkAlways, None)?;
                println!("this index walks the tree on every check.");
                println!("Slower, and true on any machine.");
                return Ok(());
            }

            if !earn {
                match index.trust()? {
                    Trust::Counter => {
                        println!("trust     the counter decides; the walk is skipped when it");
                        println!("          has not moved since the index was built");
                        match index.trust_earned()? {
                            Some(note) => println!("earned    {note}"),
                            None => {
                                println!("earned    (nothing recorded — it was not earned here)")
                            }
                        }
                    }
                    Trust::WalkAlways => {
                        println!("trust     the tree is walked on every check");
                        println!("          `thalyx graph trust --counter` earns the fast path");
                    }
                }
                return Ok(());
            }

            // Earning it. The verification is run here rather than trusted
            // from an earlier run: a machine that agreed once and has since
            // had its watcher reloaded, or its tree moved under a mount, is a
            // machine where the answer has changed.
            let scoping = thalyx_watch::scoping_for(&tree);
            let counter = counter_for(&scoping);

            if !counter.claims_complete_coverage() {
                println!("refused: the watcher does not cover every way a file can change.");
                println!("`thalyx graph watcher` names the hooks that are missing.");
                return Ok(());
            }

            let mut watcher = match index.mutation_baseline()? {
                Some(baseline) => Watcher::resuming_from(counter, baseline),
                None => Watcher::new(counter),
            };
            let verification = watcher.verify(&index)?;

            if verification.found_a_coverage_hole() {
                println!("refused: {}", verification.describe());
                return Ok(());
            }
            if !verification.coverage.is_unbroken() {
                println!("refused: coverage is {}", verification.coverage.describe());
                println!("`thalyx graph build` re-establishes it.");
                return Ok(());
            }

            let note = format!(
                "{} — {}; counter said {}, the tree said {}",
                thalyx_journal::now(),
                scoping.describe(),
                said(verification.counter_said_current),
                said(verification.walk_said_current)
            );
            index.set_trust(Trust::Counter, Some(&note))?;

            println!("earned. This index now skips the walk when the counter has not");
            println!("moved since it was built.");
            println!();
            println!("  {note}");
            println!();
            println!("It is given back on its own the moment the counter cannot account");
            println!("for everything: a reload, an unreadable map, a hook gone. Nothing");
            println!("has to notice and turn it off.");
            Ok(())
        }

        GraphCommand::Verify { tree } => {
            let tree = tree.canonicalize()?;
            let index = open(store_root, &tree)?;

            let counter = KernelCounter::default_map();
            let scoping = thalyx_watch::scoping_for(&tree);
            if !counter.is_available() {
                println!("the kernel mutation counter is NOT PRESENT");
                println!("  {}", counter.map().display());
                println!();
                println!("Nothing to verify: without it the index walks the tree on every");
                println!("query, which is correct and slow. `make -C lsm load` attaches it.");
                return Ok(());
            }

            println!("scope          {}", scoping.describe());
            let counter = counter_for(&scoping);
            let mut watcher = match index.mutation_baseline()? {
                Some(baseline) => Watcher::resuming_from(counter, baseline),
                None => Watcher::new(counter),
            };

            let verification = watcher.verify(&index)?;

            println!("counter said   {}", said(verification.counter_said_current));
            println!("the tree says  {}", said(verification.walk_said_current));
            println!("coverage       {}", verification.coverage.describe());
            println!();
            println!("{}", verification.describe());

            if verification.found_a_coverage_hole() {
                println!();
                println!("The counter must not be believed on this machine. Something can");
                println!("change a file without the kernel hooks seeing it, and the index");
                println!("would answer `current` for a tree that had moved on.");
                return Err("verification found a coverage hole".into());
            }
            Ok(())
        }

        GraphCommand::Deps { path, tree } => {
            let tree = tree.canonicalize()?;
            let index = open(store_root, &tree)?;
            let answer = index.dependencies_of(&path)?;

            print_freshness(&answer.freshness);
            if answer.rows.is_empty() {
                println!("{path} declares no dependencies");
                return Ok(());
            }
            println!("{path} depends on:");
            for edge in &answer.rows {
                match &edge.to {
                    Some(target) => println!("  {target}  (line {})", edge.line),
                    None => println!(
                        "  {}  (line {}, outside the tree)",
                        edge.raw_target, edge.line
                    ),
                }
            }
            Ok(())
        }

        GraphCommand::Dependents { path, tree } => {
            let tree = tree.canonicalize()?;
            let index = open(store_root, &tree)?;
            let answer = index.dependents_of(&path)?;

            print_freshness(&answer.freshness);
            if answer.rows.is_empty() {
                println!("nothing in the tree depends on {path}");
                return Ok(());
            }
            println!("depends on {path}:");
            for edge in &answer.rows {
                println!("  {}  (line {})", edge.from, edge.line);
            }
            Ok(())
        }

        GraphCommand::Tag { path, tag, tree } => {
            let tree = tree.canonicalize()?;
            let index = open(store_root, &tree)?;
            index.tag(&path, &tag)?;
            println!("tagged {path} as {tag}");
            Ok(())
        }

        GraphCommand::Untag { path, tag, tree } => {
            let tree = tree.canonicalize()?;
            let index = open(store_root, &tree)?;
            index.untag(&path, &tag)?;
            println!("removed tag {tag} from {path}");
            Ok(())
        }

        GraphCommand::Tagged { tag, tree } => {
            let tree = tree.canonicalize()?;
            let index = open(store_root, &tree)?;
            let answer = index.tagged(&tag)?;

            print_freshness(&answer.freshness);
            if answer.rows.is_empty() {
                println!("nothing tagged {tag}");
                return Ok(());
            }
            for path in &answer.rows {
                println!("  {path}");
            }
            Ok(())
        }
    }
}

fn print_freshness(freshness: &Freshness) {
    if !freshness.is_current() {
        println!("[{}]", freshness.describe());
        println!();
    }
}

/// One index per tree, kept in the store rather than inside the tree itself.
///
/// Putting it in the tree would make the index a file the index has to index,
/// and would litter a project the user did not ask Thalyx to modify.
fn open(store_root: &Path, tree: &Path) -> Result<Index, Box<dyn std::error::Error>> {
    let key = tree
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let database = store_root
        .join("state")
        .join("graph")
        .join(format!("{key}.db"));
    Ok(Index::open(&database, tree)?)
}

/// Tell the kernel to count this tree on its own, and report what it will do.
///
/// Failing to register is not an error. It means no watcher is loaded, or no
/// permission to write the map — and the answer to both is the machine-wide
/// count, which is less useful and never wrong.
fn register(tree: &Path) -> Scoping {
    let scoping = thalyx_watch::scoping_for(tree);
    let Scoping::Tree(root) = &scoping else {
        return scoping;
    };

    match KernelCounter::default_map().watch(root) {
        Ok(()) => scoping,
        Err(error) => Scoping::Machine {
            reason: format!("the kernel could not be told to watch it ({error})"),
        },
    }
}

/// The counter the scoping calls for.
///
/// Boxed because which one it is depends on the machine. Everything the
/// watcher concludes is the same either way: scoping narrows what is counted,
/// not what may be concluded from it.
fn counter_for(scoping: &Scoping) -> Box<dyn MutationCounter> {
    match scoping {
        Scoping::Tree(root) => Box::new(TreeCounter::new(KernelCounter::default_map(), *root)),
        Scoping::Machine { .. } => Box::new(KernelCounter::default_map()),
    }
}

fn said(current: bool) -> &'static str {
    if current {
        "nothing changed"
    } else {
        "something changed"
    }
}

/// What the kernel counter can and cannot say about this index.
///
/// Printed with the rest of the status because a shortcut that is off, and a
/// shortcut that is on and wrong, look identical from the outside otherwise.
fn print_coverage(index: &Index, scoping: &Scoping) -> Fallible {
    let counter = KernelCounter::default_map();

    if !counter.is_available() {
        println!("watcher   not loaded; every freshness check walks the whole tree");
        return Ok(());
    }

    println!("scope     {}", scoping.describe());

    // The scoped counter, so the number printed is the number the freshness
    // check will compare against. Printing the machine-wide total beside a
    // baseline taken from the tree's own count would show a difference of
    // millions and mean nothing.
    match index.trust()? {
        Trust::Counter => println!("trust     the counter decides; the walk is skipped"),
        Trust::WalkAlways => println!("trust     the tree is walked on every check"),
    }

    let total = match counter_for(scoping).total() {
        Ok(total) => total,
        Err(error) => {
            println!("watcher   present, and this tree's count is unavailable: {error}");
            return Ok(());
        }
    };

    match index.mutation_baseline()? {
        Some(baseline) => println!(
            "watcher   {} mutation(s) seen, {} since this index was built",
            total,
            total.saturating_sub(baseline)
        ),
        None => println!("watcher   {total} mutation(s) seen; no baseline for this index"),
    }

    // Which hooks, by name. "Coverage is incomplete" is not actionable; "the
    // hook that sees writes through an open descriptor is not attached" says
    // what changed in silence and what to do about it.
    match counter.missing_hooks() {
        Ok(missing) if missing.is_empty() => {
            println!(
                "          all {} hooks attached: nothing on this machine can change a",
                thalyx_watch::REQUIRED_HOOKS.len()
            );
            println!("          file without the count moving");
            // The caveat depends on the scoping, and printing it either way
            // would go stale the moment the other case became true.
            if matches!(scoping, Scoping::Machine { .. }) {
                println!("          it counts changes outside this tree too — cautious, never");
                println!("          blind, and the shortcut will rarely fire");
            }
        }
        Ok(missing) => {
            println!(
                "          NOT BELIEVABLE — {} hook(s) missing:",
                missing.len()
            );
            println!("          {}", missing.join(", "));
            println!("          each one is a way a file can change without the count moving");
        }
        Err(error) => {
            println!("          coverage could not be established: {error}");
        }
    }

    Ok(())
}
