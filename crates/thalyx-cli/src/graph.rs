//! `thalyx graph` — building and querying the semantic index.
//!
//! Every command that prints query results also prints the index's freshness.
//! Not as a courtesy: the index is a cache over a filesystem the human is free
//! to change without telling Thalyx, so an answer without that caveat would be
//! presented as more authoritative than it is.

use clap::Subcommand;
use std::path::{Path, PathBuf};
use thalyx_graph::{Coverage, Freshness, Index, MutationCounter, Watcher};
use thalyx_watch::KernelCounter;

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
            let report = index.build()?;

            // The counter's value at the moment the index matched the tree.
            // Recorded here because this is the only instant it is true.
            let counter = KernelCounter::default_map();
            let mut watcher = Watcher::new(counter);
            match watcher.rebuilt() {
                Coverage::Unbroken { baseline } => index.set_mutation_baseline(baseline)?,
                Coverage::Broken { .. } => index.clear_mutation_baseline()?,
            }

            println!("indexed {}", tree.display());
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

            println!("tree      {}", tree.display());
            println!("nodes     {}", index.node_count()?);
            println!("edges     {}", index.edge_count()?);
            print_coverage(&index)?;

            match index.freshness()? {
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

        GraphCommand::Verify { tree } => {
            let tree = tree.canonicalize()?;
            let index = open(store_root, &tree)?;

            let counter = KernelCounter::default_map();
            if !counter.is_available() {
                println!("the kernel mutation counter is NOT PRESENT");
                println!("  {}", counter.map().display());
                println!();
                println!("Nothing to verify: without it the index walks the tree on every");
                println!("query, which is correct and slow. `make -C lsm load` attaches it.");
                return Ok(());
            }

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
fn print_coverage(index: &Index) -> Fallible {
    let counter = KernelCounter::default_map();

    if !counter.is_available() {
        println!("watcher   not loaded; every freshness check walks the whole tree");
        return Ok(());
    }

    let total = match counter.total() {
        Ok(total) => total,
        Err(error) => {
            println!("watcher   present but unreadable: {error}");
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

    if !counter.claims_complete_coverage() {
        println!("          the count is machine-wide and its hooks miss writes through");
        println!("          an open descriptor, so it is reported and never believed");
    }

    Ok(())
}
