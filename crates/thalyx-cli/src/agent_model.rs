//! `thalyx agent model`, `grammar` and `bench` — choosing a tier and measuring it.
//!
//! Implements the half of `vault/02-Arquitectura/Gamas-de-Modelo.md` that is not
//! the inference itself: the tier is a decision the user takes about their own
//! hardware, and the bench is what replaces the decree's estimated numbers with
//! measured ones.
//!
//! ## Why nothing here downloads anything
//!
//! A system that fetched several gigabytes on the user's behalf would be taking
//! the tier decision for them, which is the one thing the decree exists to leave
//! with them. So `model` records a path to a file a human put there, and says
//! so when the file is not there.
//!
//! ## What the bench measures, and the sentence it prints because of it
//!
//! Utility. The decree is explicit that the bench **cannot** conclude that one
//! tier is safe and another is not — all four produce valid contracts by
//! construction, and all four stay subject to the trusted path and to the
//! core's validation. A table of per-tier numbers with no such line under it
//! invites exactly the reading the decree forbids, so the line is printed every
//! time rather than left in a document.
//!
//! The injection case is deliberately *not* a bench case for the same reason.
//! It lives in `dev/verify.sh`, where it is an assertion that passes or fails
//! rather than a score one tier can have more of.

use clap::Subcommand;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thalyx_agent::config::Settings;
use thalyx_agent::{ForeignText, Model, Segment, Tier, Transcript};
use thalyx_contract::Caller;
use thalyx_core::Store;

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The suite used when nobody names another one.
const DEFAULT_SUITE: &str = include_str!("bench_suite.toml");

pub(crate) fn settings_path(store: &Store) -> PathBuf {
    store.state_root().join("agent-model.toml")
}

/// The model the agent should use, or `None` when there is none configured.
///
/// Returning `None` rather than an error is the double route being real: a
/// Thalyx with no model is a Thalyx a human can still drive completely, and
/// treating that as a broken state would make the supported case look like
/// damage.
pub(crate) fn configured(store: &Store) -> Result<Option<Settings>, Box<dyn std::error::Error>> {
    Ok(Settings::load(&settings_path(store))?)
}

#[derive(Subcommand)]
pub enum ModelCommand {
    /// Show the tiers, and which one is configured
    Show,

    /// Record a tier and the weights file it runs
    ///
    /// Thalyx does not download the weights. Put the GGUF file on the machine
    /// and name it here; the size and digest are recorded from the file itself.
    Use {
        /// ligera, media, alta or maxima
        tier: String,
        /// The GGUF file
        #[arg(long)]
        weights: PathBuf,
        /// The llama.cpp binary. A bare name is looked up on PATH.
        ///
        /// `llama-completion`, not `llama-cli`: since llama.cpp split its
        /// tools, `llama-cli` is the interactive chat frontend and answers a
        /// prompt file by opening a session on it.
        #[arg(long, default_value = thalyx_agent::llama::COMPLETION_BINARY)]
        binary: PathBuf,
    },

    /// Forget the configured model, leaving the machine with none
    Forget,

    /// Run one inference and report what it cost, without touching the store
    Check {
        /// What to ask. Something the rules cannot resolve, or the model is
        /// never reached and this checks nothing.
        #[arg(default_value = "dev.thalyx.demo, ese")]
        utterance: String,
    },
}

pub fn model(store: &Store, command: ModelCommand) -> Fallible {
    match command {
        ModelCommand::Show => show(store),
        ModelCommand::Use {
            tier,
            weights,
            binary,
        } => choose(store, &tier, &weights, &binary),
        ModelCommand::Forget => forget(store),
        ModelCommand::Check { utterance } => check(store, &utterance),
    }
}

fn show(store: &Store) -> Fallible {
    println!("The four tiers. One family, so a bench result is about size and");
    println!("nothing else. Sizes are estimates until `thalyx agent bench` runs.");
    println!();
    println!(
        "  {:<8} {:<30} {:>10} {:>10}",
        "tier", "model", "disk", "ram"
    );
    for tier in Tier::ALL {
        println!(
            "  {:<8} {:<30} {:>10} {:>10}",
            tier.name(),
            tier.model(),
            tier.disk().to_string(),
            tier.ram().to_string(),
        );
    }
    println!();

    match configured(store)? {
        None => {
            println!("Nothing configured. The rules still resolve everything they");
            println!("resolved before — a machine with no model is a machine you can");
            println!("still use for all of it, just not by describing things loosely.");
            println!();
            println!("  thalyx agent model use media --weights <file.gguf>");
        }
        Some(settings) => {
            println!(
                "Configured: {} ▪ {}",
                settings.tier,
                settings.binary.display()
            );
            println!("  weights  {}", settings.weights.display());
            println!(
                "  measured {} bytes ▪ {}",
                settings.weights_bytes, settings.weights_digest
            );
            println!("  flags    {}", settings.extra_args.join(" "));
            match settings.model() {
                Ok(model) => match model.preflight() {
                    Ok(()) => println!("  ready"),
                    Err(error) => println!("  NOT READY: {error}"),
                },
                Err(error) => println!("  NOT READY: {error}"),
            }
            // A store configured before 2026-08-08 names the interactive tool,
            // because that was the default. Saying so here means it is found by
            // looking rather than by an inference failing in a way that reads
            // like the model's fault.
            if let Some(warning) = wrong_tool_warning(&settings.binary) {
                println!();
                println!("{warning}");
            }
        }
    }
    Ok(())
}

fn choose(store: &Store, tier: &str, weights: &Path, binary: &Path) -> Fallible {
    let tier = Tier::parse(tier)
        .ok_or_else(|| format!("`{tier}` is not a tier. They are: ligera, media, alta, maxima"))?;

    println!("reading {} to record what it is...", weights.display());
    let settings = Settings::record(tier, weights, binary)?;
    let path = settings_path(store);
    settings.save(&path)?;

    println!();
    println!("tier      {tier} ▪ {}", tier.model());
    println!("weights   {}", settings.weights.display());
    println!(
        "measured  {} bytes, against an estimate of {}",
        settings.weights_bytes,
        tier.disk()
    );
    println!("digest    {}", settings.weights_digest);
    println!("recorded  {}", path.display());
    println!();

    // Said now rather than at the first sentence somebody types. A tier that
    // turns out to be unusable an hour later is one they will have built plans
    // on top of.
    match settings.model()?.preflight() {
        Ok(()) => {
            if let Some(warning) = wrong_tool_warning(&settings.binary) {
                println!("{warning}");
                println!();
            }
            println!("`thalyx agent model check` runs one inference against it.");
        }
        Err(error) => {
            println!("Recorded, but not usable yet: {error}");
            if let Some(hint) = missing_completion_tool_hint(&settings.binary) {
                println!();
                println!("{hint}");
            }
        }
    }
    Ok(())
}

/// Whether a configured binary is llama.cpp's interactive frontend.
///
/// Said at the moment of configuring rather than left for the first inference,
/// because the failure it produces is a clean exit with a banner — the shape of
/// mistake somebody spends an evening blaming on the model. The check is on the
/// file name because that is all that is knowable without running it, and it is
/// a warning rather than a refusal: somebody may have a build where that name
/// is the completion tool, and `run` checks the contract itself either way.
fn wrong_tool_warning(binary: &Path) -> Option<String> {
    let name = binary.file_name()?.to_str()?;
    if name != thalyx_agent::llama::INTERACTIVE_BINARY {
        return None;
    }
    Some(format!(
        "WARNING: `{name}` is llama.cpp's interactive chat frontend since the\n\
         tools were split. Handed a prompt file it opens a session on it instead\n\
         of completing it, and exits cleanly having answered nothing. The\n\
         one-shot tool is `{}` — re-run this with\n  --binary {}",
        thalyx_agent::llama::COMPLETION_BINARY,
        thalyx_agent::llama::COMPLETION_BINARY,
    ))
}

/// What to say when the completion tool is absent and the chat one is there.
///
/// Naming the thing that *is* installed is the whole value: "not found" on a
/// machine that visibly has llama.cpp reads as Thalyx being wrong about the
/// path, and sends somebody looking for a typo that is not there.
fn missing_completion_tool_hint(binary: &Path) -> Option<String> {
    let name = binary.file_name()?.to_str()?;
    if name != thalyx_agent::llama::COMPLETION_BINARY {
        return None;
    }
    let interactive = thalyx_agent::llama::INTERACTIVE_BINARY;
    if !on_path(interactive) {
        return None;
    }
    Some(format!(
        "`{interactive}` IS installed, and it is not a substitute: it is the\n\
         interactive chat frontend. `{name}` is built from the same tree —\n\
         `cmake --build build --target {name}` — and is the tool that completes\n\
         a prompt once and exits."
    ))
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

fn forget(store: &Store) -> Fallible {
    let path = settings_path(store);
    match std::fs::remove_file(&path) {
        Ok(()) => println!("forgotten. The rules still resolve what they always did."),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("there was nothing configured.")
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn check(store: &Store, utterance: &str) -> Fallible {
    let settings = configured(store)?.ok_or("no model is configured; `thalyx agent model use`")?;
    let model = settings.model()?;

    let transcript = Transcript::new().with(Segment::typed(utterance));
    if matches!(
        thalyx_agent::router::route(&transcript),
        thalyx_agent::Route::Resolved { .. }
    ) {
        // Rule 4 in miniature: a check the model never took part in would pass
        // on a machine with no llama.cpp at all.
        return Err(format!(
            "the rules resolve {utterance:?} on their own, so this would not reach \
             the model. Ask something they cannot."
        )
        .into());
    }

    println!("tier    {} ▪ {}", settings.tier, settings.weights.display());
    println!("asking  {utterance:?}");
    println!();

    let run = model.run(&transcript)?;
    println!(
        "answer    {}",
        if run.answer.is_empty() {
            "(nothing)"
        } else {
            &run.answer
        }
    );
    println!("latency   {:.2}s", run.latency.as_secs_f64());
    println!("peak rss  {}", describe_rss(run.peak_rss));
    println!();

    match thalyx_agent::Proposal::parse(&run.answer) {
        Ok(proposal) => println!("parsed as: {proposal:?}"),
        Err(error) => {
            println!("did NOT parse: {error}");
            println!();
            println!("If llama.cpp took the grammar, this cannot happen — so the");
            println!("first thing to check is that --grammar-file was accepted.");
            println!("`thalyx agent grammar` prints the grammar to try by hand.");
        }
    }
    Ok(())
}

/// A memory figure that was never sampled says so.
///
/// Rule 10: printing `0 B` for a process that finished between two looks would
/// be a failure to read, reported as a very small measurement.
fn describe_rss(bytes: Option<u64>) -> String {
    match bytes {
        // The unit moves with the figure. Fixed at GB, a real measurement of a
        // few megabytes prints as `0.00 GB`, which is the one string that reads
        // exactly like the "never sampled" case this function exists to keep
        // apart from it. Found by running it, not by reading it.
        Some(bytes) if bytes >= 1_000_000_000 => format!("{:.2} GB", bytes as f64 / 1e9),
        Some(bytes) if bytes >= 1_000_000 => format!("{:.0} MB", bytes as f64 / 1e6),
        Some(bytes) => format!("{bytes} B"),
        None => "not sampled (the process finished too fast to look at)".to_string(),
    }
}

// ------------------------------------------------------------------ the bench

#[derive(serde::Deserialize)]
struct Suite {
    #[serde(rename = "case")]
    cases: Vec<Case>,
}

#[derive(serde::Deserialize)]
struct Case {
    name: String,
    utterance: String,
    #[serde(default)]
    context: Vec<String>,
    /// A module id, or the word `abstain`.
    expect: String,
    #[serde(default)]
    constraint: Option<String>,
}

impl Case {
    fn wants_abstention(&self) -> bool {
        self.expect == "abstain"
    }

    fn transcript(&self) -> Transcript {
        let mut transcript = Transcript::new();
        for line in &self.context {
            transcript = transcript.with(Segment::thalyx(line));
        }
        transcript.with(Segment::typed(&self.utterance))
    }
}

/// A model that keeps what its last run cost.
///
/// `thalyx_agent::plan` hands back a contract, not a [`thalyx_agent::Run`] — it
/// is the agent's answer and the cost of getting it is none of its business.
/// The bench does need the cost, and the two honest ways to get it are a clock
/// wrapped around `plan` or this. A clock measures the assembler and the
/// attribution as well as the inference, which on the light tier is a
/// measurable share of a small number.
struct Recording<'a> {
    inner: &'a thalyx_agent::LlamaModel,
    last: std::sync::Mutex<Option<thalyx_agent::Run>>,
}

impl<'a> Recording<'a> {
    fn around(inner: &'a thalyx_agent::LlamaModel) -> Recording<'a> {
        Recording {
            inner,
            last: std::sync::Mutex::new(None),
        }
    }

    fn take(&self) -> Option<thalyx_agent::Run> {
        self.last
            .lock()
            .expect("no thread panicked holding this")
            .take()
    }
}

impl Model for Recording<'_> {
    fn propose(&self, transcript: &Transcript) -> Result<String, thalyx_agent::ModelError> {
        let run = self.inner.run(transcript)?;
        let answer = run.answer.clone();
        *self.last.lock().expect("no thread panicked holding this") = Some(run);
        Ok(answer)
    }
}

/// What one case did.
enum Outcome {
    /// A contract naming exactly what was wanted.
    Right,
    /// A contract naming something else, or one where abstention was wanted.
    Wrong(String),
    /// No contract, which is right for an abstention case and wrong otherwise.
    Abstained,
}

pub fn bench(store: &Store, cases: Option<&Path>, request_id: &str) -> Fallible {
    let settings = configured(store)?.ok_or(
        "no model is configured, so there is nothing to measure. \
         `thalyx agent model use <tier> --weights <file>`",
    )?;
    let model = settings.model()?;
    model.preflight()?;

    let text = match cases {
        Some(path) => std::fs::read_to_string(path)?,
        None => DEFAULT_SUITE.to_string(),
    };
    let suite: Suite = toml::from_str(&text)?;
    if suite.cases.is_empty() {
        return Err("the suite has no cases in it".into());
    }

    println!(
        "tier     {} ▪ {}",
        settings.tier,
        settings.weights.display()
    );
    println!(
        "weights  {} bytes ▪ {}",
        settings.weights_bytes, settings.weights_digest
    );
    println!("cases    {}", suite.cases.len());
    println!();

    let mut intent_right = 0usize;
    let mut arguments_right = 0usize;
    let mut abstention_right = 0usize;
    let mut abstention_total = 0usize;
    let mut invented = 0usize;
    let mut latencies: Vec<Duration> = Vec::new();
    let mut peak_rss: Option<u64> = None;

    for case in &suite.cases {
        let transcript = case.transcript();

        // A case the rules answer measures the rules, and the rules are the
        // same on every tier — a suite of those would report all four as
        // perfect. Refused rather than skipped: a silent skip is how a suite
        // ends up measuring nothing at all.
        if matches!(
            thalyx_agent::router::route(&transcript),
            thalyx_agent::Route::Resolved { .. }
        ) {
            return Err(format!(
                "case {:?} is resolved by the rules, so it never reaches the model \
                 and would score the same on every tier",
                case.name
            )
            .into());
        }

        let recording = Recording::around(&model);
        let plan = thalyx_agent::plan(
            &transcript,
            &recording,
            ForeignText::NeverActs,
            Caller {
                module_id: "thalyx-agent".to_string(),
                request_id: request_id.to_string(),
            },
        );

        // From the run itself, not from a clock around `plan`, and not from a
        // second inference. A second call would double the wait and report the
        // memory of a run other than the one that produced the answer.
        if let Some(run) = recording.take() {
            latencies.push(run.latency);
            if let Some(bytes) = run.peak_rss {
                peak_rss = Some(peak_rss.map_or(bytes, |seen: u64| seen.max(bytes)));
            }
        }

        let outcome = match &plan {
            Ok(plan) => match plan.contract.targets.first() {
                Some(target) if *target == case.expect => Outcome::Right,
                Some(target) => Outcome::Wrong(target.clone()),
                None => Outcome::Abstained,
            },
            Err(_) => Outcome::Abstained,
        };

        if case.wants_abstention() {
            abstention_total += 1;
        }

        let mark = match (&outcome, case.wants_abstention()) {
            (Outcome::Abstained, true) => {
                abstention_right += 1;
                intent_right += 1;
                arguments_right += 1;
                "ok  "
            }
            (Outcome::Abstained, false) => "MISS",
            (Outcome::Right, false) => {
                intent_right += 1;
                let constraint = plan
                    .as_ref()
                    .ok()
                    .and_then(|p| p.contract.constraint.clone());
                let wanted = case.constraint.as_deref();
                if wanted.is_none_or(|w| constraint.as_deref().is_some_and(|c| c.contains(w))) {
                    arguments_right += 1;
                    "ok  "
                } else {
                    "arg "
                }
            }
            (Outcome::Right, true) | (Outcome::Wrong(_), true) => {
                invented += 1;
                "INV "
            }
            (Outcome::Wrong(_), false) => {
                intent_right += 1;
                "WRONG"
            }
        };

        let said = match &outcome {
            Outcome::Right => case.expect.clone(),
            Outcome::Wrong(target) => target.clone(),
            Outcome::Abstained => "(abstained)".to_string(),
        };
        println!("  {mark} {:<50} → {said}", truncate(&case.name, 50));
    }

    let total = suite.cases.len();
    let acting = total - abstention_total;
    latencies.sort_unstable();
    if latencies.is_empty() {
        return Err("no case reached the model, so there is nothing to report".into());
    }

    println!();
    println!("{} of {total} cases", settings.tier);
    println!("  intent      {intent_right}/{total}");
    println!("  arguments   {arguments_right}/{total}");
    if abstention_total > 0 {
        println!("  abstention  {abstention_right}/{abstention_total}  ({invented} invented)");
    }
    if acting > 0 {
        println!("  acting      {acting} case(s) where a contract was the right answer");
    }
    println!(
        "  latency     median {:.2}s ▪ worst {:.2}s",
        latencies[latencies.len() / 2].as_secs_f64(),
        latencies.last().copied().unwrap_or_default().as_secs_f64()
    );
    println!("  peak rss    {}", describe_rss(peak_rss));
    println!();
    println!("This measures utility, not safety. All four tiers produce valid");
    println!("contracts by construction and all four stay behind the trusted path,");
    println!("so nothing here can say one tier is safer than another.");
    println!();
    println!(
        "Real figures for the vault table: {} bytes on disk.",
        settings.weights_bytes
    );

    Ok(())
}

fn truncate(text: &str, at: usize) -> String {
    if text.chars().count() <= at {
        return text.to_string();
    }
    text.chars().take(at.saturating_sub(1)).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_suite_parses() {
        let suite: Suite = toml::from_str(DEFAULT_SUITE).expect("the shipped suite is readable");
        assert!(suite.cases.len() >= 5);
    }

    #[test]
    fn every_case_in_the_default_suite_actually_reaches_the_model() {
        // The property the bench refuses to run without. A case the rules
        // answer scores the same on all four tiers, so a suite of those would
        // report every tier as perfect and measure nothing.
        let suite: Suite = toml::from_str(DEFAULT_SUITE).unwrap();
        for case in suite.cases {
            assert!(
                matches!(
                    thalyx_agent::router::route(&case.transcript()),
                    thalyx_agent::Route::AskTheModel
                ),
                "case {:?} is answered by the rules",
                case.name
            );
        }
    }

    #[test]
    fn every_id_a_case_expects_appears_in_that_case_s_own_text() {
        // Attribution refuses a value that appears on no channel, so a case
        // whose answer is not somewhere in its own material is a case about the
        // core rather than about the tier — it would score zero on every tier
        // and the number would mean nothing.
        let suite: Suite = toml::from_str(DEFAULT_SUITE).unwrap();
        for case in suite.cases {
            if case.wants_abstention() {
                continue;
            }
            let material = format!("{} {}", case.utterance, case.context.join(" "));
            assert!(
                material.contains(&case.expect),
                "case {:?} expects {} and never mentions it",
                case.name,
                case.expect
            );
        }
    }

    #[test]
    fn an_abstention_case_names_no_module_at_all() {
        // If it named one, abstaining would be the wrong answer and the case
        // would be scoring the opposite of what it says it scores.
        let suite: Suite = toml::from_str(DEFAULT_SUITE).unwrap();
        for case in suite.cases.iter().filter(|c| c.wants_abstention()) {
            let mentions_an_id = case
                .utterance
                .split_whitespace()
                .chain(case.context.iter().flat_map(|c| c.split_whitespace()))
                .any(|token| {
                    let token =
                        token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
                    token.split('.').count() >= 3 && token.starts_with(char::is_alphabetic)
                });
            assert!(
                !mentions_an_id || case.name.contains("ruled out"),
                "abstention case {:?} names a module, so abstaining is not clearly right",
                case.name
            );
        }
    }

    #[test]
    fn memory_that_was_never_sampled_does_not_print_as_zero() {
        // The failure this guards is not "None prints wrong" — it is that a
        // real small measurement used to print `0.00 GB`, which reads exactly
        // like nothing having been measured. Two different facts must not share
        // a rendering.
        assert!(describe_rss(None).contains("not sampled"));
        assert_eq!(describe_rss(Some(2_000_000_000)), "2.00 GB");
        assert_eq!(describe_rss(Some(2_400_000)), "2 MB");
        assert_eq!(describe_rss(Some(4096)), "4096 B");
        assert!(!describe_rss(Some(4096)).starts_with('0'));
    }
}
