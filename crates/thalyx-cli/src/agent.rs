//! `thalyx agent` — say what you want, and see exactly what would be done.
//!
//! Two subcommands, and the split between them is the point. `plan` produces a
//! contract and stops; `do` produces the same contract and hands it to the core,
//! which confirms it through the trusted path before anything happens.
//!
//! Neither of them is a shortcut around the human. See
//! `vault/09-Notas-Tecnicas/Agente-Minimo.md`.

use crate::render;
use clap::Subcommand;
use std::path::PathBuf;
use thalyx_agent::{Path as AgentPath, Segment, Transcript, UnconfiguredModel};
use thalyx_contract::Caller;
use thalyx_core::Store;

type Fallible = Result<(), Box<dyn std::error::Error>>;

#[derive(Subcommand)]
pub enum AgentCommand {
    /// Turn a sentence into a contract and print it, without doing anything
    Plan {
        /// What you want, in your own words
        utterance: String,
        /// A directory of .thmod bundles to resolve the module against
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Text Thalyx did not get from you — a fetched page, someone's README.
        ///
        /// Passing it here is what makes it untrusted, and the point of the
        /// flag is to make that demonstrable from a terminal.
        #[arg(long)]
        foreign: Vec<String>,
    },

    /// Turn a sentence into a contract and carry it out
    Do {
        utterance: String,
        #[arg(long)]
        repo: PathBuf,
        #[arg(long)]
        foreign: Vec<String>,
        /// Confirm capabilities without prompting. For scripts and tests.
        #[arg(long)]
        yes: bool,
        /// Remember this under a task name, so it survives a reboot.
        ///
        /// Step 6 of the Phase 1 exit criterion is that the machine can be
        /// restarted and the agent still knows what the task was.
        #[arg(long)]
        task: Option<String>,
    },
}

pub fn run(store: &Store, command: AgentCommand, request_id: &str) -> Fallible {
    match command {
        AgentCommand::Plan {
            utterance,
            repo,
            foreign,
        } => plan(&utterance, repo.as_deref(), &foreign, request_id),

        AgentCommand::Do {
            utterance,
            repo,
            foreign,
            yes,
            task,
        } => act(
            store,
            &utterance,
            &repo,
            &foreign,
            yes,
            task.as_deref(),
            request_id,
        ),
    }
}

fn transcript(utterance: &str, foreign: &[String]) -> Transcript {
    let mut transcript = Transcript::new().with(Segment::typed(utterance));
    for text in foreign {
        transcript = transcript.with(Segment::foreign(text));
    }
    transcript
}

fn caller(request_id: &str) -> Caller {
    Caller {
        module_id: "thalyx-agent".to_string(),
        request_id: request_id.to_string(),
    }
}

fn describe_path(path: AgentPath) -> &'static str {
    match path {
        AgentPath::Rules => "the rules, with no model involved",
        AgentPath::Model => "a model",
    }
}

fn plan(
    utterance: &str,
    repo: Option<&std::path::Path>,
    foreign: &[String],
    request_id: &str,
) -> Fallible {
    let transcript = transcript(utterance, foreign);
    let plan = thalyx_agent::plan(&transcript, &UnconfiguredModel, caller(request_id))?;

    println!("understood by: {}", describe_path(plan.path));
    println!();
    println!("{}", plan.contract.to_json());
    println!();

    // Provenance printed as its own block rather than left inside the JSON.
    // It is the field that decides whether the core will look at any of the
    // others, so burying it in a structure someone skims is the wrong place.
    println!("provenance:");
    for (field, origin) in plan.contract.origins.iter() {
        println!("  {field:<12} {origin}");
    }

    if let Some(repo) = repo {
        println!();
        println!("resolution against {}:", repo.display());
        for target in &plan.contract.targets {
            match thalyx_core::repo::resolve(repo, target, plan.contract.constraint.as_deref()) {
                Ok(found) => println!("  {target} → {} ({})", found.path.display(), found.version),
                Err(error) => println!("  {target} → {error}"),
            }
        }
    }

    println!();
    println!("Nothing was done. `thalyx agent do` carries it out, and asks first.");
    Ok(())
}

fn act(
    store: &Store,
    utterance: &str,
    repo: &std::path::Path,
    foreign: &[String],
    yes: bool,
    task: Option<&str>,
    request_id: &str,
) -> Fallible {
    let transcript = transcript(utterance, foreign);
    let plan = thalyx_agent::plan(&transcript, &UnconfiguredModel, caller(request_id))?;

    println!("understood by: {}", describe_path(plan.path));

    // One target, because the minimal agent installs one module. Resolution is
    // a sub-task without a contract of its own — see Resolver-vs-Instalar — so
    // it happens here rather than becoming a second thing to authorise.
    let target = plan
        .contract
        .targets
        .first()
        .ok_or("the contract names nothing to install")?;
    let resolved = thalyx_core::repo::resolve(repo, target, plan.contract.constraint.as_deref())?;
    println!(
        "resolved: {} {} from {}",
        target,
        resolved.version,
        resolved.path.display()
    );

    // The contract goes to the core with its provenance intact. The core
    // re-checks it, and would refuse it here just as readily as it would refuse
    // one that arrived from anywhere else.
    let request = thalyx_core::InstallRequest {
        bundle_path: &resolved.path,
        contract: plan.contract,
    };
    let mut prompt = render::TerminalConfirmer::new(yes);
    let outcome = thalyx_core::install(store, request, &mut prompt)?;

    println!();
    println!("{} {} installed", outcome.module_id, outcome.version);
    println!(
        "  {} file(s), {} permission(s) now in force",
        outcome.files.len(),
        outcome.granted
    );

    // After the install, never before. A memory written first would record an
    // installation that then failed, and a memory of something that did not
    // happen is worse than no memory at all.
    if let Some(task) = task {
        // The `current` link and not the version directory: it is the single
        // point that decides whether a module is installed at all, so removing
        // the module, or upgrading past this version, both make the memory stop
        // being assertable — which is exactly what it should say in either case.
        let installed_at = store.current_link(&outcome.module_id);
        thalyx_agent::recollection::record_install(
            &store.root().join("state").join("memory.db"),
            task,
            utterance,
            &outcome.module_id,
            &outcome.version,
            &installed_at,
        )?;
        println!();
        println!("remembered under task `{task}`.");
        println!("  `thalyx memory recall {task}` reads it back, after a reboot too.");
    }

    Ok(())
}
