//! `thalyx graph` — building and querying the semantic index.
//!
//! Every command that prints query results also prints the index's freshness.
//! Not as a courtesy: the index is a cache over a filesystem the human is free
//! to change without telling Thalyx, so an answer without that caveat would be
//! presented as more authoritative than it is.

use clap::Subcommand;
use std::path::{Path, PathBuf};
use thalyx_graph::{Freshness, Index};

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
