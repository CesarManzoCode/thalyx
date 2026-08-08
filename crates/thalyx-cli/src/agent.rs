//! `thalyx agent` — say what you want, and see exactly what would be done.
//!
//! Two subcommands, and the split between them is the point. `plan` produces a
//! contract and stops; `do` produces the same contract and hands it to the core,
//! which confirms it through the trusted path before anything happens.
//!
//! Neither of them is a shortcut around the human. See
//! `vault/09-Notas-Tecnicas/Agente-Minimo.md`.

use crate::agent_model::{self, ModelCommand};
use crate::render;
use clap::Subcommand;
use std::path::PathBuf;
use thalyx_agent::recollection::Context;
use thalyx_agent::{ForeignText, Model, Path as AgentPath, Segment, Transcript, UnconfiguredModel};
use thalyx_contract::Caller;
use thalyx_core::Store;

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The model to reason with, whatever the machine has.
///
/// A machine with no model configured gets [`UnconfiguredModel`], which says so
/// and resolves nothing — and everything the rules cover keeps working, which is
/// `Principio-Doble-Ruta.md` being the thing that makes a missing model
/// survivable rather than fatal.
///
/// Boxed because the two are different types and the caller does not care which
/// one it got. Which one it was is visible in the output either way: the
/// unconfigured one produces an error naming itself.
fn model_for(store: &Store) -> Result<Box<dyn Model>, Box<dyn std::error::Error>> {
    match agent_model::configured(store)? {
        Some(settings) => Ok(Box::new(settings.model()?)),
        None => Ok(Box::new(UnconfiguredModel)),
    }
}

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
        /// Let the model act on this task even after reading the text above.
        ///
        /// Off by default and never remembered: a conclusion drawn while
        /// reading someone else's document is one that document had a chance to
        /// shape. This does NOT let that text choose what to install — a module
        /// named only there is still refused.
        #[arg(long)]
        foreign_may_act: bool,
        /// Reason with what the agent remembers about this task
        #[arg(long)]
        task: Option<String>,
    },

    /// What the agent remembers about a task, and what it can no longer confirm
    Recall {
        /// The task name it was recorded under
        task: String,
    },

    /// Choose which of the four tiers this machine runs, and check it
    #[command(subcommand)]
    Model(ModelCommand),

    /// Print the GBNF grammar every inference is constrained by
    ///
    /// So that the same inference can be repeated in a terminal with
    /// `--grammar-file` and no Thalyx in the way.
    Grammar,

    /// Measure the configured tier: intent, arguments, abstention, latency, RAM
    Bench {
        /// A suite of cases. Without one, the suite built into Thalyx is used.
        #[arg(long)]
        cases: Option<PathBuf>,
        /// Leave every case's prompt, grammar and command under this directory
        ///
        /// One directory per inference, named after that prompt's marker. A
        /// twenty-case suite therefore leaves twenty of them, which is the
        /// point: the marker is new every invocation, so a case that answered
        /// strangely cannot be rebuilt afterwards from the suite alone.
        #[arg(long)]
        keep_prompt: Option<PathBuf>,
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
        /// Let the model act on this task even after reading the text above.
        ///
        /// Off by default and never remembered: a conclusion drawn while
        /// reading someone else's document is one that document had a chance to
        /// shape. This does NOT let that text choose what to install — a module
        /// named only there is still refused.
        #[arg(long)]
        foreign_may_act: bool,
    },
}

pub fn run(store: &Store, command: AgentCommand, request_id: &str) -> Fallible {
    match command {
        AgentCommand::Plan {
            utterance,
            repo,
            foreign,
            foreign_may_act,
            task,
        } => plan(
            store,
            Planning {
                utterance: &utterance,
                repo: repo.as_deref(),
                foreign: &foreign,
                task: task.as_deref(),
                policy: policy(foreign_may_act),
                request_id,
            },
        ),

        AgentCommand::Recall { task } => recall(store, &task, ""),

        AgentCommand::Model(command) => agent_model::model(store, command),

        AgentCommand::Grammar => {
            print!("{}", thalyx_agent::grammar::gbnf());
            Ok(())
        }

        AgentCommand::Bench { cases, keep_prompt } => {
            agent_model::bench(store, cases.as_deref(), keep_prompt, request_id)
        }

        AgentCommand::Do {
            utterance,
            repo,
            foreign,
            yes,
            task,
            foreign_may_act,
        } => act(
            store,
            Doing {
                utterance: &utterance,
                repo: &repo,
                foreign: &foreign,
                yes,
                task: task.as_deref(),
                policy: policy(foreign_may_act),
                request_id,
            },
        ),
    }
}

/// The concession is per invocation and never stored, which is what "per task
/// and never global" means in a command-line shape.
fn policy(may_act: bool) -> ForeignText {
    if may_act {
        ForeignText::MayActThisTask
    } else {
        ForeignText::NeverActs
    }
}

pub(crate) fn memory_path(store: &Store) -> PathBuf {
    store.root().join("state").join("memory.db")
}

/// Assemble everything the agent is working from, in trust order.
///
/// What Thalyx remembers goes in first because it is the oldest and the least
/// specific; what the human just typed goes last because it is the request. The
/// order is for a reader — attribution does not care about position, only about
/// which channel a value turns up on.
fn transcript(context: Context, utterance: &str, foreign: &[String]) -> Transcript {
    let mut transcript = Transcript::new();
    for segment in context.segments {
        transcript = transcript.with(segment);
    }
    transcript = transcript.with(Segment::typed(utterance));
    for text in foreign {
        transcript = transcript.with(Segment::foreign(text));
    }
    transcript
}

/// The agent reading its own memory back, in its own voice.
///
/// `thalyx memory recall` shows the same records as a database would. This says
/// what they mean for the task: what is still true, and what it has stopped
/// being able to stand behind. A memory nobody consults is a log.
///
/// `indent` exists because the session inside the machine prints the same thing
/// two spaces in. It is a parameter rather than a second copy of these
/// paragraphs: step 6 of the exit criterion is checked by reading this text
/// after a reboot, and two versions of it would eventually say different things
/// about the same memory, on the two routes `Principio-Doble-Ruta.md` requires
/// to agree.
pub(crate) fn recall(store: &Store, task: &str, indent: &str) -> Fallible {
    let context = thalyx_agent::recollection::context(&memory_path(store), task)?;

    if context.is_empty() {
        println!("{indent}I have nothing recorded under `{task}`.");
        return Ok(());
    }

    if !context.said.is_empty() {
        println!("{indent}About `{task}`, you told me:");
        for text in &context.said {
            println!("{indent}  · {text}");
        }
    }

    if !context.holds.is_empty() {
        if !context.said.is_empty() {
            println!();
        }
        println!("{indent}And this still checks out:");
        for text in &context.holds {
            println!("{indent}  ✓ {text}");
        }
    }

    if !context.unconfirmable.is_empty() {
        println!();
        println!("{indent}And this I remember but can no longer confirm:");
        for text in &context.unconfirmable {
            println!("{indent}  ? {text}");
        }
        println!();
        // It used to say the thing had changed "without going through Thalyx",
        // which was true while the only route to this state was somebody
        // editing a file behind us. `revertir` at the session prompt reaches it
        // too, and that is Thalyx doing exactly what it was asked — so the
        // sentence became a confident account of a cause this code cannot see.
        // A memory that names the wrong reason is worse than one that names
        // none: it sends a person looking for an intruder they will not find.
        println!("{indent}Not wrong — unconfirmable. What it described is no longer");
        println!("{indent}what it was, so I will not act on it. Undoing the install");
        println!("{indent}does this, and so does a file changing behind my back; I");
        println!("{indent}cannot tell which from here. Name it yourself and I will");
        println!("{indent}act, which is the point of you never needing me.");
    }

    Ok(())
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

/// One `agent plan`, gathered for the same reason [`Doing`] is.
struct Planning<'a> {
    utterance: &'a str,
    repo: Option<&'a std::path::Path>,
    foreign: &'a [String],
    task: Option<&'a str>,
    policy: ForeignText,
    request_id: &'a str,
}

fn plan(store: &Store, planning: Planning<'_>) -> Fallible {
    let Planning {
        utterance,
        repo,
        foreign,
        task,
        policy,
        request_id,
    } = planning;

    let context = match task {
        Some(task) => thalyx_agent::recollection::context(&memory_path(store), task)?,
        None => Context::default(),
    };
    if !context.segments.is_empty() {
        println!(
            "working from {} thing(s) I remember",
            context.segments.len()
        );
    }
    let transcript = transcript(context, utterance, foreign);
    let model = model_for(store)?;
    let plan = thalyx_agent::plan(&transcript, model.as_ref(), policy, caller(request_id))?;

    println!("understood by: {}", describe_path(plan.path));
    if policy == ForeignText::MayActThisTask {
        println!(
            "note: you allowed the model to act after reading foreign text, for \
             this one command.\n      It still cannot choose what to install."
        );
    }
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

/// One `agent do`, gathered so the shape of the request is one thing rather
/// than eight positional arguments that can be swapped without the compiler
/// noticing.
struct Doing<'a> {
    utterance: &'a str,
    repo: &'a std::path::Path,
    foreign: &'a [String],
    yes: bool,
    task: Option<&'a str>,
    policy: ForeignText,
    request_id: &'a str,
}

fn act(store: &Store, doing: Doing<'_>) -> Fallible {
    let Doing {
        utterance,
        repo,
        foreign,
        yes,
        task,
        policy,
        request_id,
    } = doing;

    let context = match task {
        Some(task) => thalyx_agent::recollection::context(&memory_path(store), task)?,
        None => Context::default(),
    };
    let transcript = transcript(context, utterance, foreign);
    let model = model_for(store)?;
    let plan = thalyx_agent::plan(&transcript, model.as_ref(), policy, caller(request_id))?;

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
