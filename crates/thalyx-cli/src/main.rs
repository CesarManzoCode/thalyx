//! The `thalyx` command-line interface.
//!
//! Phase 1 has no agent, so this is the only way in — and by the double-route
//! principle it must stay a complete way in even once the agent exists.
//! Everything the agent will be able to do, a human can do here first.

mod dev;
mod enforce;
mod graph;
mod memory;
mod render;
mod run;

use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use thalyx_contract::{Contract, Operation, Origin, Origins};
use thalyx_core::Store;

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

    /// Undo something Thalyx published
    ///
    /// Narrow and cheap: it takes back what Thalyx itself put on disk and
    /// touches nothing you made, which is why it does not ask before running.
    /// It is not the command that returns the filesystem to a snapshot — that
    /// one is destructive and has its own name, `restore`.
    Rollback {
        /// Undo this exact request rather than the most recent commit
        #[arg(long)]
        request: Option<String>,
        /// Say what would be undone, and do nothing
        #[arg(long)]
        dry_run: bool,
    },

    /// What the agent remembers between sessions
    #[command(subcommand)]
    Memory(memory::MemoryCommand),

    /// Push granted permissions into the kernel, and see what is enforced
    #[command(subcommand)]
    Enforce(enforce::EnforceCommand),

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
    /// Run an installed module under the permissions it was granted
    Run {
        module_id: String,
        /// Sandbox profile to confine it with
        #[arg(long, default_value = thalyx_sandbox::profile::MODULE_STANDARD)]
        profile: String,
        /// Which of the module's declared entrypoints to start
        #[arg(long, default_value = thalyx_core::run::DEFAULT_ENTRYPOINT)]
        entrypoint: String,
        /// Run with no confinement at all, and have the journal say so
        #[arg(long)]
        unconfined: bool,
        /// Arguments passed through to the module
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
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
    // Checked before any argument parsing, and deliberately not a clap
    // subcommand. This is the internal re-execution that puts a process into a
    // module's cgroup before it becomes the module — see
    // `thalyx_sandbox::launch`. It has to be recognised positionally, ahead of
    // anything that could interpret the module's own arguments, and it has no
    // business appearing in `--help`.
    if let Some(stage) = thalyx_sandbox::parse_stage(std::env::args_os()) {
        return match thalyx_sandbox::run_stage(&stage) {
            Ok(code) => ExitCode::from(code),
            // Failing here means the module does not run at all, which is the
            // point: running it less confined than the profile says would be
            // the outcome the whole mechanism exists to prevent.
            Err(error) => {
                eprintln!("thalyx: {error}");
                ExitCode::FAILURE
            }
        };
    }

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
        Command::Rollback { request, dry_run } => {
            let store = Store::open(&root)?;
            let plan = thalyx_core::rollback::plan(&store, request.as_deref())?;

            // Said before it happens, in the same words either way. The decree
            // says rollback needs no confirmation because it cannot destroy
            // the human's work; it does not say it may act silently.
            println!("{}", plan.describe());
            println!("  published by request {}", plan.request_id);
            if plan.permissions_revoked > 0 {
                println!(
                    "  {} permission(s) stop being effective",
                    plan.permissions_revoked
                );
            }
            if let Some(uid) = plan.uid_retired {
                println!("  user {uid} is retired, and never handed to another module");
            }
            println!();
            println!("Nothing outside what Thalyx published is touched.");

            if dry_run {
                println!();
                println!("--dry-run: nothing was undone.");
                return Ok(());
            }

            thalyx_core::rollback::apply(&store, &plan, &new_request_id())?;
            println!();
            println!("undone.");
            Ok(())
        }
        Command::Memory(command) => {
            Store::open(&root)?;
            memory::run(&root, command)
        }
        Command::Enforce(command) => enforce::run(&root, command),
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
            // The CLI is the human's own route, so every effectful field comes
            // from what they typed. The agent's contracts will look the same
            // but carry different provenance — and that is exactly the
            // distinction the core checks.
            let request = thalyx_core::InstallRequest {
                bundle_path: &bundle,
                contract: install_contract(&bundle),
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
            println!(
                "  runs as user {}, which is this module's and no other's",
                outcome.uid
            );
            Ok(())
        }
        ModuleCommand::Run {
            module_id,
            profile,
            entrypoint,
            unconfined,
            args,
        } => run::run(
            root,
            &module_id,
            &profile,
            &entrypoint,
            args,
            unconfined,
            new_request_id(),
        ),
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

/// Build the contract for a human-initiated install.
///
/// Permissions are left empty on purpose: the CLI is not claiming to know what
/// the module wants. The manifest is the authority, and the core confirms its
/// full set regardless of what the contract mentions.
fn install_contract(bundle: &std::path::Path) -> Contract {
    let mut origins = Origins::new();
    origins
        .set("operation", Origin::UserUtterance)
        .set("targets", Origin::UserUtterance)
        .set("constraint", Origin::UserUtterance)
        .set("permissions", Origin::SystemState);

    Contract {
        version: thalyx_contract::SUPPORTED_VERSION.to_string(),
        operation: Operation::InstallModule,
        targets: vec![bundle.display().to_string()],
        constraint: None,
        permissions: Vec::new(),
        requires_confirmation: true,
        sandbox_profile: Some("module_standard".to_string()),
        rollback: Default::default(),
        caller: thalyx_contract::Caller {
            module_id: "thalyx-cli".to_string(),
            request_id: new_request_id(),
        },
        origins,
    }
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
