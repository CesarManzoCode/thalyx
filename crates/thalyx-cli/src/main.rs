//! The `thalyx` command-line interface.
//!
//! Phase 1 has no agent, so this is the only way in — and by the double-route
//! principle it must stay a complete way in even once the agent exists.
//! Everything the agent will be able to do, a human can do here first.

mod dev;
mod graph;
mod render;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use thalyx_core::Store;
use thalyx_journal::Origin;

#[derive(Parser)]
#[command(
    name = "thalyx",
    version,
    about = "Thalyx — an operating system where AI is a first-class citizen",
    long_about = None
)]
struct Cli {
    /// Store root. Defaults to $THALYX_ROOT, then /opt/thalyx.
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install, list and remove modules
    #[command(subcommand)]
    Module(ModuleCommand),

    /// Build and query the semantic index
    #[command(subcommand)]
    Graph(graph::GraphCommand),

    /// Read the operation journal
    Journal {
        /// Show only the most recent N entries
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },

    /// Show granted permissions
    Permissions,

    /// Inspect and repair the store
    #[command(subcommand)]
    Store(StoreCommand),

    /// Packaging tools for module publishers
    #[command(subcommand)]
    Dev(dev::DevCommand),
}

#[derive(Subcommand)]
enum ModuleCommand {
    /// Install a module from a .thmod bundle
    Install {
        bundle: PathBuf,
        /// Confirm capabilities without prompting. For scripts and tests.
        #[arg(long)]
        yes: bool,
    },
    /// List installed modules
    List,
    /// Remove a module and revoke its permissions
    Remove { module_id: String },
}

#[derive(Subcommand)]
enum StoreCommand {
    /// Report store consistency
    Status,
    /// Delete leftover staging directories and orphaned versions
    Clean,
    /// Settle unresolved intents against what is actually on disk
    Reconcile,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("thalyx: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let root = cli.root.unwrap_or_else(default_root);

    match cli.command {
        Command::Module(command) => run_module(&root, command),
        Command::Graph(command) => {
            Store::open(&root)?;
            graph::run(&root, command)
        }
        Command::Journal { limit } => {
            let store = Store::open(&root)?;
            render::journal(&store, limit)
        }
        Command::Permissions => {
            let store = Store::open(&root)?;
            render::permissions(&store)
        }
        Command::Store(StoreCommand::Status) => {
            let store = Store::open(&root)?;
            render::store_status(&store)
        }
        Command::Store(StoreCommand::Reconcile) => {
            let store = Store::open(&root)?;
            let resolutions = thalyx_core::reconcile::reconcile(&store)?;
            if resolutions.is_empty() {
                println!("nothing to reconcile");
            } else {
                for resolution in resolutions {
                    println!("{resolution}");
                }
            }
            Ok(())
        }
        Command::Store(StoreCommand::Clean) => {
            let store = Store::open(&root)?;
            let staging = store.clean_staging()?;
            println!("removed {staging} leftover staging director(ies)");

            for (id, version) in store.orphaned_versions()? {
                let dir = store.version_dir(&id, &version);
                std::fs::remove_dir_all(&dir)?;
                println!("removed orphaned version {id} {version}");
            }

            // Permission records for modules that are not installed grant
            // nothing, but leaving them around invites someone to read the
            // file directly and believe otherwise.
            let mut registry = thalyx_core::permissions::Registry::load(store.permissions_path())?;
            let inert: Vec<String> = registry
                .all()
                .map(|(id, _)| id.clone())
                .filter(|id| !store.is_installed(id))
                .collect();
            for id in inert {
                registry.revoke_all(&id)?;
                println!("cleared inert permission record for {id}");
            }

            Ok(())
        }
        Command::Dev(command) => dev::run(command),
    }
}

fn run_module(
    root: &std::path::Path,
    command: ModuleCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(root)?;

    match command {
        ModuleCommand::Install { bundle, yes } => {
            let request = thalyx_core::InstallRequest {
                bundle_path: &bundle,
                request_id: new_request_id(),
                origin: Origin::UserUtterance,
            };

            let mut prompt = render::TerminalConfirmer::new(yes);
            let outcome = thalyx_core::install(&store, request, &mut prompt)?;

            println!();
            if let Some(previous) = &outcome.replaced {
                println!(
                    "{} upgraded from {} to {}",
                    outcome.module_id, previous, outcome.version
                );
            } else {
                println!("{} {} installed", outcome.module_id, outcome.version);
            }
            println!(
                "  {} file(s), {} permission(s) now in force",
                outcome.files.len(),
                outcome.granted
            );
            Ok(())
        }
        ModuleCommand::List => render::module_list(&store),
        ModuleCommand::Remove { module_id } => {
            let version = thalyx_core::remove(&store, &module_id, &new_request_id())?;
            println!("{module_id} {version} removed, permissions revoked");
            Ok(())
        }
    }
}

fn default_root() -> PathBuf {
    std::env::var_os("THALYX_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/thalyx"))
}

/// Ties an operation to its journal entry and its pending permission grants.
fn new_request_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("req-{nanos:x}")
}
