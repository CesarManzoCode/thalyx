//! The real model: llama.cpp, as a process.
//!
//! `vault/02-Arquitectura/Gamas-de-Modelo.md` decrees invoking it rather than
//! linking it, for three reasons that this file is shaped by:
//!
//! - **No build dependency.** Nothing here needs a C++ toolchain, so somebody
//!   who only wants the CLI does not pay for a piece they will not use.
//! - **Every step inspectable by hand.** `thalyx agent grammar` prints the exact
//!   grammar, and with [`Invocation::keep_prompt`] set, every run leaves its
//!   prompt, its grammar and the command that ran them on disk — so a strange
//!   answer can be taken to a terminal with no Thalyx in the way. A fault only
//!   observable from inside the process that caused it costs twice as much to
//!   find. What none of that buys is the same answer twice; the section below
//!   is why this bullet no longer claims it.
//! - **The model is outside the process.** It is outside the TCB by
//!   `vault/11-Seguridad/Modelo-de-Amenaza.md`; running it in another process
//!   makes that boundary one the operating system enforces rather than one the
//!   design asserts.
//!
//! ## A fixed seed is not a reproducible run — measured 2026-08-08
//!
//! [`Invocation`] pins `--seed 1` and `--temp 0`, so the *sampler* is
//! deterministic. That was read here as "the run is reproducible", and it is
//! not, because the sampler is not the only input:
//!
//! - [`crate::prompt::Prompt::render`] mints a fresh random marker on every
//!   invocation, so **the prompt is different bytes every time**. Determinism
//!   given the same input is not reproducibility when the input changes.
//! - The `-f` path [`Invocation::command_line`] names used to live in a
//!   `tempfile::tempdir()` removed when the run ends, so the file the command
//!   named was gone before anybody could paste the line. Worse, nothing outside
//!   a test ever called `command_line` — the bullet above described a feature
//!   that had never run. [`Invocation::keep_prompt`] is the fix Cesar chose:
//!   with a path, the prompt, the grammar and the command stay on disk, and
//!   *that* run can be repeated exactly, marker and all.
//!
//! None of that needed a machine to notice — `prompt::tests::
//! a_marker_is_never_reused_between_two_renders` has asserted the marker
//! changes since the day it was written. A test in this crate and a doc comment
//! in this crate said opposite things, and the doc comment was believed because
//! nothing ever asked the two to agree.
//!
//! What did need a machine: the light tier, run twice through `thalyx agent
//! bench` with the same weights, the same suite and the same machine, moved
//! **two cases of twenty, in opposite directions** — one from no-measurement to
//! a rejected id, one from a rejected id to the right answer. So every per-tier
//! fraction in `vault/02-Arquitectura/Gamas-de-Modelo.md` carries movement of
//! that size, and a two-case gap between two tiers is not a difference between
//! them.
//!
//! Making the marker *derivable* would buy back reproducibility across runs,
//! and it is deliberately not done: a marker foreign text cannot guess is the
//! whole reason [`crate::prompt`] randomises it. Keeping the prompt costs
//! nothing of the sort, and it is also the honest half of the trade — the
//! run-to-run movement it leaves in place is real, and a benchmark that hides
//! it reports one sample of a distribution with the confidence of a
//! measurement.
//!
//! ## What has run, and what still has not — revised 2026-08-08
//!
//! Two runs on Cesar's Fedora, against `llama.cpp b1-3653e6d` and
//! Qwen2.5-3B-Instruct-Q4_K_M:
//!
//! 1. With `llama-cli`, which no longer completes anything — see the section
//!    below.
//! 2. With `llama-completion`, which **loaded the weights and printed a
//!    well-formed proposal.** Thalyx refused it, because the answer's *end* had
//!    never been defined; see [`crate::proposal::Proposal::completion_in`].
//!
//! **Proven against the real tool:** every flag [`LlamaModel::spawn`] passes is
//! accepted, the weights load, the prompt is echoed with the marker intact, and
//! a proposal comes back inside the timeout.
//!
//! **Not proven, and worth keeping apart from the above:** that
//! `--grammar-file` is what constrained that answer. A 3B model asked for JSON
//! may well produce JSON unaided, so an accepted flag and an applied grammar
//! look identical from here — telling them apart needs an utterance the model
//! would answer with prose if it were allowed to. Nor is any per-tier number
//! proven. The container has neither llama.cpp nor a route to the weights, so
//! the workspace tests cannot close either. `dev/verify.sh` says so.
//!
//! ## Which binary, and why it is not `llama-cli` — revised 2026-08-08
//!
//! It was `llama-cli`, and the first run against a real llama.cpp
//! (`b1-3653e6d`, Qwen2.5-3B) showed why that is wrong. llama.cpp has split its
//! tools: `llama-cli` is now an **interactive chat frontend** built on the
//! server, with conversation control — regenerate, roll back, `/exit`, `/regen`,
//! `/clear` — and the old one-shot completion tool lives on as
//! **`llama-completion`**, which is what this file wants and what carries `-f`,
//! `--grammar-file`, `-n`, `--seed` and `--temp` unchanged.
//!
//! Handed `-f`, the new `llama-cli` opens a session on the file instead of
//! completing it. It does not fail: it loads the weights, prints a banner, takes
//! the closed stdin as end of input and exits cleanly. So the wrong tool looks
//! like a working tool that gave a bad answer.
//!
//! That is why [`LlamaError::NotOneShot`] exists. The contract this file needs
//! is not "a program called llama-something" — it is **feed a prompt, apply a
//! grammar, print a completion, exit** — and the contract is now checked rather
//! than assumed. See [`LlamaModel::run`].
//!
//! ## The version-dependent part, kept where it can be edited
//!
//! Flags come and go between llama.cpp releases. The ones this file passes
//! itself — `-m`, `-f`, `-n`, `--seed`, `--temp`, `--grammar-file` — have been
//! stable across the split. The ones that have not are in
//! [`Invocation::extra_args`], which lives in the config file rather than in
//! this source, so a build that rejects one is fixed by editing a line instead
//! of by rebuilding Thalyx. If llama.cpp refuses a flag it says so on stderr,
//! and [`LlamaError::Exited`] carries that text out verbatim.
//!
//! `--no-display-prompt` used to be among them and has been **removed on
//! purpose**: the echoed prompt carries the marker that proves the prompt was
//! read at all, and suppressing it destroys the evidence the contract check
//! depends on. See `prompt.rs`.
//!
//! Stdin is closed rather than being given a flag. A tool that wants to chat
//! reads end-of-input at once and stops, so the failure is a quick wrong answer
//! rather than a hang — and a hang is the one failure that looks like nothing at
//! all.

use crate::model::{Model, ModelError};
use crate::prompt::Prompt;
use crate::transcript::Transcript;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// More than this from a grammar-constrained model is a runaway.
///
/// Four times what `proposal.rs` will accept, on purpose: the point is to have
/// enough of the output in hand to say *what* went wrong, rather than to cut it
/// off at the boundary and report a truncation as a malformed answer.
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// How often the peak-memory sampler looks.
const RSS_SAMPLE: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum LlamaError {
    #[error("{0} is not installed, or is not on PATH")]
    NotInstalled(PathBuf),

    #[error("the weights are not at {0}")]
    WeightsMissing(PathBuf),

    #[error("could not start {binary}: {source}")]
    Spawn {
        binary: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("no answer within {}s; the process was killed", .0.as_secs())]
    TimedOut(Duration),

    #[error("llama.cpp exited with {status}. It said:\n{stderr}")]
    Exited { status: String, stderr: String },

    #[error("llama.cpp produced more than {MAX_OUTPUT_BYTES} bytes and was stopped")]
    Runaway,

    #[error(
        "{} ran and exited cleanly, but never put the prompt through a \
         completion: what it printed does not contain the prompt at all.\n\
         \n\
         This is what llama.cpp's `llama-cli` does since the tools were split — \
         it is an interactive chat frontend now, and it answers `-f` by opening \
         a session on the file rather than completing it. The one-shot tool is \
         `llama-completion`.\n\
         \n\
         Point Thalyx at it:\n    \
         thalyx agent model use <tier> --weights <file> --binary llama-completion\n\
         \n\
         The first 400 bytes of what it printed instead:\n{sample}",
        .binary.display()
    )]
    NotOneShot { binary: PathBuf, sample: String },

    #[error(
        "{} completed the prompt, but what follows it is not a proposal, which \
         means --grammar-file was not in force. A grammar-constrained \
         completion cannot produce anything else — so this is the tool ignoring \
         the grammar, not the model answering badly.\n\
         \n\
         Thalyx reads the first complete JSON value after the prompt and \
         ignores whatever the tool prints after it, so a trailing marker such \
         as `[end of text]` is not what this is.\n\
         \n\
         It answered:\n{answer}",
        .binary.display()
    )]
    GrammarNotInForce { binary: PathBuf, answer: String },

    #[error(
        "the model began the object the grammar describes and ran out of tokens \
         before closing it, at the {predict}-token cap.\n\
         \n\
         This is the grammar working, not failing: the answer opens exactly \
         where the grammar says it must. What it ran out of was budget.\n\
         \n\
         The grammar does not bound how long a module id may be, so a model \
         that cannot find a legal way to answer will spend the whole cap inside \
         one string. If the request was ordinary, that is what happened, and a \
         larger `predict` will not make the answer right — it will make it \
         longer.\n\
         \n\
         What it got to:\n{answer}"
    )]
    Truncated { predict: u32, answer: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<LlamaError> for ModelError {
    fn from(error: LlamaError) -> Self {
        ModelError::Failed(error.to_string())
    }
}

/// Everything needed to run one inference, and nothing about what it is for.
#[derive(Debug, Clone)]
pub struct Invocation {
    /// The llama.cpp binary. A bare name is looked up on `PATH`.
    pub binary: PathBuf,
    /// The GGUF file. Thalyx never downloads this; a human puts it there.
    pub weights: PathBuf,
    /// The flags that move between llama.cpp releases. See the module docs.
    pub extra_args: Vec<String>,
    /// Token cap. A grammar-constrained proposal is far under this; the cap is
    /// for the case where the grammar is not doing what it is supposed to.
    pub predict: u32,
    /// Fixed, so the sampler is deterministic. Note what that is not: the
    /// prompt carries a marker that changes every invocation, so two runs of
    /// the same question are two different questions. See the module docs.
    pub seed: u64,
    pub timeout: Duration,
    /// Where to leave the prompt, the grammar and the command that ran them.
    ///
    /// [`None`] puts them in a scratch directory that is removed when the run
    /// ends, which is right for a bench of twenty cases and wrong for a person
    /// trying to see what was actually asked. With a path, the files stay and
    /// *that* run can be repeated by hand — the marker inside the kept prompt
    /// is the one that ran, which is the part a re-render cannot give back.
    pub keep_prompt: Option<PathBuf>,
}

/// The llama.cpp tool that honours the one-shot completion contract.
///
/// Not `llama-cli`, which is the interactive chat frontend since the tools were
/// split. See the module docs.
pub const COMPLETION_BINARY: &str = "llama-completion";

/// The tool it used to be, kept by name so the failure can say so.
pub const INTERACTIVE_BINARY: &str = "llama-cli";

impl Invocation {
    /// The defaults, which are the flags that have been stable across the split.
    pub fn new(binary: impl Into<PathBuf>, weights: impl Into<PathBuf>) -> Invocation {
        Invocation {
            binary: binary.into(),
            weights: weights.into(),
            // `--no-display-prompt` is deliberately not here. See the module
            // docs: the echo carries the proof that the prompt was read.
            extra_args: vec!["-no-cnv".to_string()],
            predict: 256,
            seed: 1,
            timeout: Duration::from_secs(180),
            keep_prompt: None,
        }
    }

    /// The command as somebody would type it, for reproducing a run by hand.
    ///
    /// Takes the paths it would write rather than inventing names, so what is
    /// written is what was run. `grammar_file` is [`None`] for the free arm of
    /// the grammar probe, which is the one run that passes no `--grammar-file`
    /// — printing the flag there would describe the other arm.
    pub fn command_line(&self, prompt_file: &Path, grammar_file: Option<&Path>) -> String {
        let mut parts = vec![self.binary.display().to_string()];
        let mut push = |flag: &str, value: String| {
            parts.push(flag.to_string());
            parts.push(value);
        };
        push("-m", self.weights.display().to_string());
        push("-f", prompt_file.display().to_string());
        if let Some(grammar_file) = grammar_file {
            push("--grammar-file", grammar_file.display().to_string());
        }
        push("-n", self.predict.to_string());
        push("--seed", self.seed.to_string());
        push("--temp", "0".to_string());
        parts.extend(self.extra_args.iter().cloned());
        parts.join(" ")
    }
}

/// One completed inference, with what it cost.
///
/// The cost travels with the answer because `Gamas-de-Modelo.md` asks the bench
/// for latency and resident memory per tier, and measuring them anywhere other
/// than around the process that spent them means measuring something else.
#[derive(Debug, Clone)]
pub struct Run {
    pub answer: String,
    pub latency: Duration,
    /// Peak resident set size in bytes, sampled from `/proc`.
    ///
    /// `None` when no sample was ever taken — a process that finished between
    /// two looks. Reporting `0` there would be a measurement of nothing
    /// presented as a measurement of a small thing, which is rule 10: a failure
    /// to read is not a failure to exist.
    pub peak_rss: Option<u64>,
}

/// Whether a run is handed `--grammar-file`.
///
/// Only the probes ever pass [`Constrained::No`]. It is a private argument
/// rather than a field on [`Invocation`] on purpose: an unconstrained inference
/// is not a mode Thalyx supports, and a setting for it in the config file would
/// be one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Constrained {
    Yes,
    No,
}

/// Which arm of a probe a run belongs to.
///
/// Carries [`Constrained`] rather than sitting beside it, so an arm cannot be
/// labelled one way and run the other — which is the failure that makes saved
/// evidence describe the wrong inference, and it has already happened once here
/// for a different reason.
///
/// Two of the three arms run unconstrained and they are **not the same
/// question**: [`Arm::Free`] is the object prompt with the flag removed, and
/// [`Arm::Prose`] is a different prompt entirely. Naming only the flag would
/// leave two directories called `-free` per case, distinguishable only by
/// reading the prompts inside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    WithGrammar,
    Free,
    Prose,
}

impl Arm {
    fn constrained(self) -> Constrained {
        match self {
            Arm::WithGrammar => Constrained::Yes,
            Arm::Free | Arm::Prose => Constrained::No,
        }
    }

    /// What this arm is called in a kept-evidence directory name.
    fn suffix(self) -> &'static str {
        match self {
            Arm::WithGrammar => "-with-grammar",
            Arm::Free => "-free",
            Arm::Prose => "-prose",
        }
    }
}

/// Where a run's prompt and grammar sit while llama.cpp reads them.
///
/// Two shapes because they answer to different people. [`Scratch::Discarded`]
/// is gone the moment the run ends, which is what a bench of twenty cases
/// wants; [`Scratch::Kept`] is what somebody wants who is trying to see what
/// was actually asked, and it exists because the prompt carries a marker that
/// no later render will produce again. See [`Invocation::keep_prompt`].
enum Scratch {
    Discarded(tempfile::TempDir),
    Kept(PathBuf),
}

impl Scratch {
    /// `under` is where a disposable directory is made, when the engine can
    /// only read from somewhere in particular.
    ///
    /// It exists because the module engine reads the prompt from inside a
    /// sandbox: the only paths that exist in there are the ones the human
    /// granted, and the system temporary directory is not one of them. A
    /// prompt written to `/tmp` is a prompt the engine is told to read and
    /// cannot see — which arrives as "the tool never completed the prompt",
    /// naming llama.cpp for a mistake made here.
    fn open(
        keep: Option<&Path>,
        under: Option<&Path>,
        named: &str,
    ) -> Result<Scratch, std::io::Error> {
        match keep {
            None => match under {
                None => Ok(Scratch::Discarded(tempfile::tempdir()?)),
                Some(root) => {
                    std::fs::create_dir_all(root)?;
                    let dir = tempfile::tempdir_in(root)?;
                    // `tempdir_in` makes it 0700, owned by whoever is running
                    // Thalyx — root. A module runs as a user of its own
                    // (`module_standard` gives it one), so a 0700 directory is
                    // one it cannot enter: the bind is there, the file is
                    // there, and `open` says EACCES. What comes back from that
                    // is llama.cpp failing to read the prompt, which reads as
                    // the engine's fault and is this line's.
                    make_readable(dir.path(), 0o755)?;
                    Ok(Scratch::Discarded(dir))
                }
            },
            Some(root) => {
                let dir = root.join(named);
                std::fs::create_dir_all(&dir)?;
                Ok(Scratch::Kept(dir))
            }
        }
    }

    fn path(&self) -> &Path {
        match self {
            Scratch::Discarded(dir) => dir.path(),
            Scratch::Kept(dir) => dir,
        }
    }
}

/// Let a program that is not this one read it.
///
/// Only ever used on the scratch a confined engine has to read, and only there:
/// widening anything else would be handing out access nobody asked for.
#[cfg(unix)]
fn make_readable(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn make_readable(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

/// How many tokens the probe is given.
///
/// Small because only the first character decides the outcome, and because a
/// constrained model that was asked for something illegal does not give up: on
/// the first real run it filled 256 tokens with a module id spelling out
/// `banana_module_1234…`, hunting for a legal way to say the word. The grammar
/// does not bound the length of an id — see `grammar.rs` — so the budget is the
/// only thing that ends that, and there is no reason to pay for all of it.
const PROBE_PREDICT: u32 = 48;

/// What [`LlamaModel::grammar_check`] found.
///
/// Three outcomes and not two. `Inconclusive` is the one that has to exist:
/// rule 3 says a check that could not be made must say so rather than counting
/// as a pass, and a probe both arms answer the same way has not measured
/// anything.
///
/// Both arms travel with every outcome, including the failure. The first
/// version of this dropped the control arm from `NotInForce` on the grounds
/// that the failure was already definitive — and then reported a false failure
/// with half the evidence missing, which is the one situation where the other
/// half is worth most.
#[derive(Debug, Clone)]
pub enum GrammarCheck {
    /// Constrained it could not open with the word; left alone it did.
    ///
    /// Both halves of that sentence are checked. The second one used to be
    /// inferred from the free arm not opening an object, which is how a tier
    /// whose model answered the free probe with nothing at all was reported as
    /// proof that the grammar stopped it from speaking.
    InForce {
        constrained: String,
        unconstrained: String,
    },
    /// Constrained it said the word anyway, which the grammar forbids.
    NotInForce {
        constrained: String,
        unconstrained: String,
    },
    /// The probe could not tell the two arms apart, and says which way.
    Inconclusive {
        why: &'static str,
        constrained: String,
        unconstrained: String,
    },
}

/// Everything one inference needs, with its two files already on disk.
///
/// The whole argument vector is derived from this in one place, so the command
/// a person can paste into a terminal and the command an engine is actually
/// handed cannot drift apart. That drift is not hypothetical here: the free arm
/// of the grammar probe differs from the constrained one by exactly one flag,
/// and a second place building the arguments is a second place to get that
/// wrong.
#[derive(Debug, Clone, Copy)]
pub struct EngineCall<'a> {
    pub weights: &'a Path,
    pub prompt_file: &'a Path,
    /// [`None`] only for the free arm of the grammar probe. See [`Constrained`].
    pub grammar_file: Option<&'a Path>,
    pub predict: u32,
    pub seed: u64,
    pub extra_args: &'a [String],
    pub timeout: Duration,
}

impl EngineCall<'_> {
    /// The arguments, in the order every launcher passes them.
    pub fn args(&self) -> Vec<std::ffi::OsString> {
        let mut args: Vec<std::ffi::OsString> = vec![
            "-m".into(),
            self.weights.into(),
            "-f".into(),
            self.prompt_file.into(),
        ];
        if let Some(grammar) = self.grammar_file {
            args.push("--grammar-file".into());
            args.push(grammar.into());
        }
        args.push("-n".into());
        args.push(self.predict.to_string().into());
        args.push("--seed".into());
        args.push(self.seed.to_string().into());
        args.push("--temp".into());
        args.push("0".into());
        args.extend(self.extra_args.iter().map(Into::into));
        args
    }
}

/// What an engine printed, and what it cost.
#[derive(Debug, Default)]
pub struct EngineRun {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// How it ended, when it did not end well. [`None`] is a clean exit.
    pub failed: Option<String>,
    /// Peak resident set size, when the launcher was in a position to sample it.
    pub peak_rss: Option<u64>,
}

/// Whatever actually runs llama.cpp.
///
/// The seam Cesar decreed on 2026-08-28: **the engine is a module**, not a
/// program found on `PATH`. Everything this crate knows about inference — the
/// prompt, the marker, the grammar, where an answer stops, what a broken one
/// looks like — is above this line and unchanged by which side of it runs.
/// Below it there are two implementations and they answer one question
/// differently: who starts the process.
///
/// - [`ProcessEngine`] starts it here, with [`std::process::Command`]. It is
///   what `thalyx agent bench` uses on a development machine, where there is a
///   llama.cpp on `PATH` and no store to install anything into.
/// - The machine's engine runs it as an installed, signed module under
///   `module_standard`, through `thalyx_core::run`. It lives in the CLI,
///   because that is where the store and the sandbox are, and because a crate
///   the model's output passes through must not be able to start confined
///   processes.
///
/// The trait is deliberately narrow: an argument vector in, bytes out. An
/// engine that could see the prompt's structure would be an engine that could
/// be wrong about it in a second place.
pub trait Engine: std::fmt::Debug + Send + Sync {
    /// What to name in an error. A binary path, or a module id.
    fn describe(&self) -> PathBuf;

    /// Whether the engine is there at all, without loading any weights.
    fn preflight(&self) -> Result<(), LlamaError>;

    /// Where the prompt and grammar must be written for this engine to read
    /// them. [`None`] means anywhere.
    fn scratch_root(&self) -> Option<PathBuf> {
        None
    }

    fn complete(&self, call: EngineCall<'_>) -> Result<EngineRun, LlamaError>;
}

/// llama.cpp as a child process of this one.
#[derive(Debug, Clone)]
pub struct ProcessEngine {
    /// A bare name is looked up on `PATH`.
    pub binary: PathBuf,
}

impl ProcessEngine {
    pub fn new(binary: impl Into<PathBuf>) -> ProcessEngine {
        ProcessEngine {
            binary: binary.into(),
        }
    }

    /// Wait for the process, killing it if it outstays the deadline.
    ///
    /// A token cap already bounds a model that will not stop generating. This
    /// bounds the other one — a process that is not generating anything and is
    /// also not exiting, which produces no output to be suspicious of.
    fn wait_or_kill(
        &self,
        child: &mut Child,
        started: Instant,
        timeout: Duration,
    ) -> Result<std::process::ExitStatus, LlamaError> {
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if started.elapsed() > timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(LlamaError::TimedOut(timeout));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Engine for ProcessEngine {
    fn describe(&self) -> PathBuf {
        self.binary.clone()
    }

    fn preflight(&self) -> Result<(), LlamaError> {
        if !resolves_to_a_program(&self.binary) {
            return Err(LlamaError::NotInstalled(self.binary.clone()));
        }
        Ok(())
    }

    fn complete(&self, call: EngineCall<'_>) -> Result<EngineRun, LlamaError> {
        let started = Instant::now();
        let mut command = Command::new(&self.binary);
        command
            .args(call.args())
            // Closed, not inherited. See the module docs: an inherited stdin is
            // how llama-cli decides there is a person to chat with.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|source| LlamaError::Spawn {
            binary: self.binary.clone(),
            source,
        })?;

        let peak = Arc::new(AtomicU64::new(0));
        let sampler = sample_peak_rss(child.id(), Arc::clone(&peak));
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());

        let status = self.wait_or_kill(&mut child, started, call.timeout)?;

        // Joined after the wait, never before: a reader thread ends when its
        // pipe closes, and the pipe closes when the child goes.
        let out = stdout.join().unwrap_or_default();
        let err = stderr.join().unwrap_or_default();
        sampler.stop();

        Ok(EngineRun {
            stdout: out,
            stderr: err,
            failed: (!status.success()).then(|| status.to_string()),
            peak_rss: match peak.load(Ordering::Relaxed) {
                0 => None,
                bytes => Some(bytes),
            },
        })
    }
}

/// The model the decree describes.
#[derive(Debug, Clone)]
pub struct LlamaModel {
    invocation: Invocation,
    /// What starts llama.cpp. See [`Engine`].
    engine: Arc<dyn Engine>,
}

impl LlamaModel {
    /// A model that starts llama.cpp here, as a child process.
    pub fn new(invocation: Invocation) -> LlamaModel {
        let engine = Arc::new(ProcessEngine::new(invocation.binary.clone()));
        LlamaModel { invocation, engine }
    }

    /// The same model, run by something else — in practice the engine module.
    ///
    /// Everything about what a good answer is stays where it was. Only who
    /// starts the process changes, which is the whole point of the seam.
    pub fn through(mut self, engine: Arc<dyn Engine>) -> LlamaModel {
        self.engine = engine;
        self
    }

    pub fn engine(&self) -> &dyn Engine {
        self.engine.as_ref()
    }

    pub fn invocation(&self) -> &Invocation {
        &self.invocation
    }

    /// Leave every run's prompt, grammar and command line under `dir`.
    ///
    /// Deliberately not a field in the config file. Keeping prompts is what
    /// somebody does while looking into one strange answer, and a setting that
    /// survives reboots would fill a disk on the machine of somebody who set it
    /// once and forgot. [`None`] restores the disposable default, so a caller
    /// can pass an `Option` straight through from a flag.
    pub fn keeping_prompt(mut self, dir: Option<PathBuf>) -> LlamaModel {
        self.invocation.keep_prompt = dir;
        self
    }

    /// Check what can be checked without spending a minute loading weights.
    ///
    /// Separate from [`LlamaModel::run`] so that `thalyx agent model` can say
    /// "that file is not there" the moment somebody configures it, rather than
    /// the first time they ask the agent something.
    pub fn preflight(&self) -> Result<(), LlamaError> {
        if !self.invocation.weights.is_file() {
            return Err(LlamaError::WeightsMissing(self.invocation.weights.clone()));
        }
        self.engine.preflight()
    }

    /// Run one inference and report what it cost.
    pub fn run(&self, transcript: &Transcript) -> Result<Run, LlamaError> {
        let run = self.complete(&Prompt::render(transcript), Arm::WithGrammar)?;

        // The prompt was completed, so cut the completion out of what came
        // back. The marker said where the answer begins; the grammar says where
        // it ends, and llama.cpp prints ` [end of text]` past that point.
        // Taking the whole region instead is what turned a correct proposal
        // into an accusation — see `Proposal::completion_in`.
        //
        // Silence is left alone: a tool that completed to nothing is a quiet
        // model, not a broken tool, and `Proposal::parse` has a word for it.
        if run.answer.is_empty() {
            return Ok(run);
        }

        if let Some(completion) = crate::proposal::Proposal::completion_in(&run.answer)
            && crate::proposal::Proposal::parse(completion).is_ok()
        {
            return Ok(Run {
                answer: completion.to_string(),
                ..run
            });
        }

        // It did not parse, and there are two ways to get here that want
        // opposite things done about them. The first character tells them
        // apart, because it is the one thing the grammar fixes absolutely.
        //
        // Found on Cesar's machine by the grammar probe, which had this same
        // bug: told to emit a word the grammar forbids, the model spent every
        // token it had spelling a legal id and hit the cap mid-string. That
        // answer is unparseable *and* maximally obedient, and calling it a
        // broken grammar sends whoever reads it to audit llama.cpp.
        if run
            .answer
            .trim_start()
            .starts_with(crate::grammar::ROOT_FIRST_CHAR)
        {
            return Err(LlamaError::Truncated {
                predict: self.invocation.predict,
                answer: sample_of(&run.answer),
            });
        }

        // It did not even open where `root` opens. A constrained decode cannot
        // do that, so the grammar is not being applied.
        Err(LlamaError::GrammarNotInForce {
            binary: self.engine.describe(),
            answer: sample_of(&run.answer),
        })
    }

    /// Answer one transcript three ways, for [`crate::grammar_effect`].
    ///
    /// - **with the grammar** — what Thalyx actually ships
    /// - **without it**, same rendered prompt, marker and all, so the two differ
    ///   in exactly one flag. A second render would carry a new marker and make
    ///   them two different questions
    /// - **in prose**, [`Prompt::in_prose`], which asks for no object and names
    ///   no operation
    ///
    /// The third arm was added 2026-08-09, after the first two answered their
    /// question and could not answer the next one. They showed the grammar is
    /// not what stops this model declining — it invents with the flag and
    /// without it — and then stopped, because the prompt they share asks for a
    /// JSON object whose first field is an operation. Neither arm was ever free
    /// of that. See [`Prompt::in_prose`].
    ///
    /// This is the only way out of this crate to an unconstrained answer, and it
    /// is deliberately one call: `Gamas-de-Modelo.md` decrees that every
    /// inference runs grammar-constrained, so a method handing back a free
    /// answer on its own would be that decree with an opt-out. Nothing here can
    /// become a [`crate::Proposal`] — all three arms come back as text.
    #[allow(clippy::type_complexity)]
    pub fn three_ways(
        &self,
        transcript: &Transcript,
    ) -> (
        Result<String, LlamaError>,
        Result<String, LlamaError>,
        Result<String, LlamaError>,
    ) {
        let asking_for_an_object = Prompt::render(transcript);
        let asking_in_prose = Prompt::in_prose(transcript);

        // Each arm's failure is carried out on its own. A single `Result` would
        // lose the arms that worked, and which of them failed is most of the
        // diagnosis: an unconstrained arm that runs away is a different fact
        // from a constrained one that does.
        (
            self.complete(&asking_for_an_object, Arm::WithGrammar)
                .map(|run| run.answer),
            self.complete(&asking_for_an_object, Arm::Free)
                .map(|run| run.answer),
            self.complete(&asking_in_prose, Arm::Prose)
                .map(|run| run.answer),
        )
    }

    /// Ask the model for something the grammar forbids, with and without it.
    ///
    /// The one thing `run` cannot establish. See [`Prompt::probe`] for why the
    /// probe is shaped the way it is, and [`GrammarCheck`] for what each of the
    /// three outcomes means.
    ///
    /// Two runs of the *same* prompt so that `--grammar-file` is the only thing
    /// that differs between them. A check whose two arms differ in two things
    /// cannot say which one moved the result.
    pub fn grammar_check(&self) -> Result<GrammarCheck, LlamaError> {
        let probe = Prompt::probe();

        // A short budget, and it is the probe's whole cost. Only the first
        // character decides this, and a constrained model that cannot say what
        // it was asked for will otherwise spend every token it is given hunting
        // for a legal way to comply — which is what happened on the first real
        // run, at 256 tokens of a module id spelling out `banana_module_…`.
        let mut brief = self.clone();
        brief.invocation.predict = PROBE_PREDICT;

        let constrained = brief.complete(&probe, Arm::WithGrammar)?.answer;
        let unconstrained = brief.complete(&probe, Arm::Free)?.answer;

        let obeys_root = |answer: &str| {
            answer
                .trim_start()
                .starts_with(crate::grammar::ROOT_FIRST_CHAR)
        };
        let says_word = |answer: &str| answer.trim_start().starts_with(crate::prompt::PROBE_WORD);

        // Read at the first character, not by parsing. See `ROOT_FIRST_CHAR`:
        // an answer cut off by the token cap does not parse and is *maximally*
        // obedient, and the version of this that asked "did it parse" reported
        // exactly that case as a broken grammar.
        Ok(if says_word(&constrained) {
            // Definitive, and the only definitive failure available: the
            // grammar cannot put that character first, so it was not applied.
            GrammarCheck::NotInForce {
                constrained,
                unconstrained,
            }
        } else if !obeys_root(&constrained) {
            // Neither the forbidden word nor an object. Rule 10 — say which
            // thing happened rather than picking the confident reading.
            GrammarCheck::Inconclusive {
                why: "the constrained answer is neither the word nor an object, \
                      so this probe cannot say what shaped it",
                constrained,
                unconstrained,
            }
        } else if obeys_root(&unconstrained) {
            // Both arms opened an object. The grammar may be working perfectly;
            // this probe cannot see it, because the model would have answered
            // that way anyway. Rule 4: without the control arm, a grammar doing
            // nothing and a grammar working look the same.
            GrammarCheck::Inconclusive {
                why: "this model opens an object with or without the grammar, so \
                      the two arms cannot be told apart",
                constrained,
                unconstrained,
            }
        } else if says_word(&unconstrained) {
            GrammarCheck::InForce {
                constrained,
                unconstrained,
            }
        } else {
            // The control arm has to be *asserted*, not left over. This branch
            // used to be the `else`, so `InForce` was reached whenever the free
            // arm merely failed to open an object — and the verdict printed
            // underneath it says "left alone it did [say the word]", which
            // nothing had checked.
            //
            // Found on the light tier, 2026-08-08: the 1.5B answered the free
            // probe with an immediate end-of-generation, so its control arm read
            // ` [end of text]` and the probe declared PROVEN over evidence that
            // showed the model saying nothing at all. Rule 4 — a denial and an
            // operation that never happened look identical without a control —
            // and the reason it survived is rule 8: every stand-in here said the
            // word when handed no grammar, so no fake ever modelled a model that
            // stays quiet.
            //
            // What can still be said from such a run is smaller than PROVEN and
            // is not this probe's claim: the flag changed the output. Whether
            // the *word* was what the grammar stopped is unmeasured, because the
            // model never showed it would say it.
            GrammarCheck::Inconclusive {
                why: "the unconstrained arm did not say the word either, so \
                      nothing here shows the grammar is what stopped it",
                constrained,
                unconstrained,
            }
        })
    }

    /// Spawn the tool, wait for it, and take what follows the marker.
    ///
    /// Everything both callers share, and nothing about what a good answer is.
    /// `constrained` is not a setting: [`Constrained::No`] exists for the
    /// grammar probe alone, since `Gamas-de-Modelo.md` decrees that every
    /// inference runs grammar-constrained, and a config knob for turning that
    /// off would be a decree with an opt-out.
    fn complete(&self, prompt: &Prompt, arm: Arm) -> Result<Run, LlamaError> {
        let constrained = arm.constrained();
        self.preflight()?;

        // Named after the marker, which is unique per invocation: a bench of
        // twenty cases leaves twenty directories instead of overwriting one,
        // and the name is a string the run's own output already shows.
        //
        // The arm is in the name because the marker is not enough. Both probes
        // — `grammar_check` and `three_ways` — run *one* rendered prompt twice
        // on purpose, so those two arms share a marker; without this suffix the
        // free arm overwrote the constrained arm's `command`, and what survived
        // was a command line with no `--grammar-file` in it. The saved evidence
        // for the arm that mattered would have described the other one.
        //
        // The prose arm renders its own prompt and so carries its own marker,
        // but it is named too: `-free` on two different questions would leave a
        // reader to tell them apart by opening the prompts.
        let named: String = prompt
            .marker()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .chain(arm.suffix().chars())
            .collect();
        let scratch = Scratch::open(
            self.invocation.keep_prompt.as_deref(),
            self.engine.scratch_root().as_deref(),
            &named,
        )?;
        let prompt_file = scratch.path().join("prompt.txt");
        let grammar_file = scratch.path().join("proposal.gbnf");
        std::fs::write(&prompt_file, prompt.text())?;
        std::fs::write(&grammar_file, crate::grammar::gbnf())?;

        // Same reason as the directory above, one level down. An engine that
        // runs as its own user has to be able to open both of these.
        if self.engine.scratch_root().is_some() {
            make_readable(scratch.path(), 0o755)?;
            make_readable(&prompt_file, 0o644)?;
            make_readable(&grammar_file, 0o644)?;
        }

        // Written beside the files it names, rather than printed: this is a
        // library, and a command line naming two paths is worth nothing away
        // from them. Until this call `command_line` had no caller outside its
        // own test, while the module docs said it was how a run got reproduced.
        if let Scratch::Kept(dir) = &scratch {
            let line = self.invocation.command_line(
                &prompt_file,
                match constrained {
                    Constrained::Yes => Some(grammar_file.as_path()),
                    Constrained::No => None,
                },
            );
            std::fs::write(dir.join("command"), line + "\n")?;
        }

        let started = Instant::now();
        let run = self.engine.complete(EngineCall {
            weights: &self.invocation.weights,
            prompt_file: &prompt_file,
            grammar_file: match constrained {
                Constrained::Yes => Some(grammar_file.as_path()),
                Constrained::No => None,
            },
            predict: self.invocation.predict,
            seed: self.invocation.seed,
            extra_args: &self.invocation.extra_args,
            timeout: self.invocation.timeout,
        })?;

        if run.stdout.len() > MAX_OUTPUT_BYTES {
            return Err(LlamaError::Runaway);
        }
        if let Some(status) = run.failed {
            return Err(LlamaError::Exited {
                status,
                stderr: String::from_utf8_lossy(&run.stderr).trim().to_string(),
            });
        }

        let peak_rss = run.peak_rss;
        let text = String::from_utf8_lossy(&run.stdout).into_owned();

        // The marker is gone, so the prompt was never completed. That is a tool
        // which does not do this job, not an answer that came out bad — and it
        // used to arrive at `Proposal::parse` as "the model said something
        // unparseable", naming the wrong culprit for the one failure a real
        // llama.cpp actually produced.
        //
        // Checked here rather than in `run` because it is true of any
        // completion, grammar or no grammar: a tool that never read the prompt
        // has not answered the probe either.
        let Some(answer) = prompt.answer_in(&text) else {
            return Err(LlamaError::NotOneShot {
                binary: self.engine.describe(),
                sample: sample_of(&text),
            });
        };
        let answer = answer.trim().to_string();

        Ok(Run {
            answer,
            latency: started.elapsed(),
            peak_rss,
        })
    }
}

impl Model for LlamaModel {
    fn propose(&self, transcript: &Transcript) -> Result<String, ModelError> {
        Ok(self.run(transcript)?.answer)
    }
}

/// Read a pipe to the end, on its own thread.
///
/// On a thread because a pipe that nobody is draining fills up, and a child
/// blocked writing into a full pipe is a child that never exits — which the
/// deadline would then report as a timeout, sending whoever reads it looking at
/// the model instead of at this file.
fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = pipe {
            // Bounded, so a runaway cannot be answered by filling memory.
            let _ = pipe
                .by_ref()
                .take(MAX_OUTPUT_BYTES as u64 + 1)
                .read_to_end(&mut buffer);
            // Keep draining and discarding so the child can still exit.
            let _ = std::io::copy(&mut pipe, &mut std::io::sink());
        }
        buffer
    })
}

/// A thread that watches `/proc/<pid>/status` and keeps the largest `VmHWM`.
///
/// `VmHWM` is itself a high-water mark, so any single successful read after the
/// peak is the true peak; sampling is only how one of those reads is caught
/// before the process goes away.
struct Sampler {
    stop: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Sampler {
    fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn sample_peak_rss(pid: u32, peak: Arc<AtomicU64>) -> Sampler {
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let handle = std::thread::spawn(move || {
        let status = PathBuf::from(format!("/proc/{pid}/status"));
        while !flag.load(Ordering::Relaxed) {
            if let Some(bytes) = read_vm_hwm(&status) {
                peak.fetch_max(bytes, Ordering::Relaxed);
            }
            std::thread::sleep(RSS_SAMPLE);
        }
    });
    Sampler {
        stop,
        handle: Some(handle),
    }
}

fn read_vm_hwm(status: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(status).ok()?;
    let line = text.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// Enough of an output to recognise it, without pasting a whole session.
///
/// Bounded because the failure it appears in is one where the tool printed a
/// banner, and a banner in an error message that scrolls the diagnosis off the
/// screen is a diagnosis nobody reads.
fn sample_of(text: &str) -> String {
    const AT: usize = 400;
    let trimmed = text.trim();
    match trimmed.char_indices().nth(AT) {
        Some((at, _)) => format!("{}…", &trimmed[..at]),
        None => trimmed.to_string(),
    }
}

/// Whether a binary name or path names something that can be executed.
///
/// A bare name is searched for on `PATH` the same way the shell would, because
/// "llama-cli is not installed" and "llama-cli is not where you said" are
/// different sentences and the person reading one of them is doing different
/// things next.
fn resolves_to_a_program(binary: &Path) -> bool {
    if binary.components().count() > 1 {
        return binary.is_file();
    }
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(binary).is_file())
}

/// Write the grammar somewhere a person can point llama.cpp at it.
pub fn write_grammar(to: &Path) -> std::io::Result<()> {
    let mut file = std::fs::File::create(to)?;
    file.write_all(crate::grammar::gbnf().as_bytes())
}

#[cfg(test)]
mod engine_seam {
    use super::*;
    use crate::transcript::{Segment, Transcript};
    use std::sync::Mutex;

    /// An engine that records where it was asked to read from, and answers.
    #[derive(Debug)]
    struct Recording {
        under: Option<PathBuf>,
        seen: Mutex<Vec<PathBuf>>,
    }

    impl Engine for Recording {
        fn describe(&self) -> PathBuf {
            PathBuf::from("module dev.thalyx.engine")
        }
        fn preflight(&self) -> Result<(), LlamaError> {
            Ok(())
        }
        fn scratch_root(&self) -> Option<PathBuf> {
            self.under.clone()
        }
        fn complete(&self, call: EngineCall<'_>) -> Result<EngineRun, LlamaError> {
            self.seen
                .lock()
                .unwrap()
                .push(call.prompt_file.to_path_buf());
            // What llama.cpp does: echo the prompt, then answer past the marker.
            let mut stdout = std::fs::read(call.prompt_file).unwrap_or_default();
            stdout
                .extend_from_slice(br#"{ "operation": "make_directory", "targets": ["pruebas"] }"#);
            Ok(EngineRun {
                stdout,
                ..EngineRun::default()
            })
        }
    }

    /// The one invariant the confined engine cannot survive without.
    ///
    /// A module sees only the directories its manifest was granted. The prompt
    /// therefore has to be written *inside* one of them, and a scratch
    /// directory in `/tmp` — which is what every run used before 2026-08-28 —
    /// is a file the engine is told to read and cannot see. What comes back is
    /// llama.cpp failing to complete a prompt, which reads as the model's
    /// fault and is this line's.
    #[test]
    fn the_prompt_is_written_where_the_engine_can_read_it() {
        let scratch = tempfile::tempdir().unwrap();
        let granted = scratch.path().join("granted");
        let weights = scratch.path().join("w.gguf");
        std::fs::write(&weights, b"gguf").unwrap();

        let engine = Arc::new(Recording {
            under: Some(granted.clone()),
            seen: Mutex::new(Vec::new()),
        });
        let model = LlamaModel::new(Invocation::new("unused", &weights))
            .through(Arc::clone(&engine) as Arc<dyn Engine>);

        let run = model
            .run(&Transcript::new().with(Segment::typed("crea una carpeta llamada pruebas")))
            .expect("the stand-in engine answers");
        assert!(run.answer.contains("make_directory"), "{}", run.answer);

        let seen = engine.seen.lock().unwrap();
        let prompt = seen.first().expect("the engine was asked something");
        assert!(
            prompt.starts_with(&granted),
            "the prompt was written to {}, which is outside the only directory the \
             engine was granted ({})",
            prompt.display(),
            granted.display()
        );
    }

    /// The control: with no granted directory, nothing is forced anywhere.
    ///
    /// Without it, an engine seam that always wrote under the first path it was
    /// handed would pass the test above and quietly break `thalyx agent bench`
    /// on a development machine.
    #[test]
    fn an_engine_that_can_read_anywhere_is_not_confined_to_a_directory() {
        let scratch = tempfile::tempdir().unwrap();
        let weights = scratch.path().join("w.gguf");
        std::fs::write(&weights, b"gguf").unwrap();

        let engine = Arc::new(Recording {
            under: None,
            seen: Mutex::new(Vec::new()),
        });
        let model = LlamaModel::new(Invocation::new("unused", &weights))
            .through(Arc::clone(&engine) as Arc<dyn Engine>);
        model
            .run(&Transcript::new().with(Segment::typed("crea una carpeta llamada pruebas")))
            .expect("the stand-in engine answers");

        let seen = engine.seen.lock().unwrap();
        assert!(
            !seen.first().unwrap().starts_with(scratch.path()),
            "a scratch root nobody asked for"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Segment;

    fn transcript() -> Transcript {
        Transcript::new().with(Segment::typed("instala dev.thalyx.demo"))
    }

    /// A stand-in for llama.cpp: a script that ignores every flag and does what
    /// the test needs.
    ///
    /// A script rather than `sh -c` because the flags this file passes come
    /// *before* `extra_args`, so `sh` would read `-m` as its own and the
    /// weights path as the program to run. That is not a detail of the fake —
    /// it is the real argument order, and a fake that could not survive it
    /// would be a fake of a different invocation.
    ///
    /// Each stand-in gets its **own** file name, and that was not enough.
    ///
    /// ## The diagnosis, which took two runs a year apart to get
    ///
    /// The first time, this failed once in twenty-five runs, the failure was not
    /// captured, and the fix written here — one file name per stand-in — was a
    /// guess that said so. On 2026-08-10 Cesar's twelve-core machine failed it
    /// **twice in one run** and the errors were captured:
    ///
    /// ```text
    /// could not start /tmp/.tmpf0elxc/llama-cli-7: Text file busy (os error 26)
    /// ```
    ///
    /// `ETXTBSY` is the kernel refusing to `execve` a file **any** process holds
    /// open for writing. The count it checks lives on the inode, not in a file
    /// table, so `O_CLOEXEC` does not help and unique names do not either. The
    /// mechanism is the fork window: `Command::spawn` forks, and between that
    /// fork and the child's `execve` the child holds a copy of every descriptor
    /// its parent had — including a write descriptor another test thread was
    /// using at that instant to create *this* file. Twelve test threads make
    /// that window ordinary; two make it once in twenty-five runs.
    ///
    /// So the helper does not return a path, it returns a path **that has been
    /// run**. Waiting for the window to close is the only thing that closes it,
    /// and doing it here means no test has to know the window exists.
    ///
    /// Rule 5, ninth time: the instrument includes the harness. Nothing about
    /// Thalyx was wrong in that run.
    fn stand_in(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);

        let path = dir.join(format!(
            "llama-cli-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// Run a stand-in, waiting out a fork window that is not what any test here
    /// is about.
    ///
    /// Every test in this file calls this instead of [`LlamaModel::run`]. What
    /// it adds is one thing: `ETXTBSY` is retried instead of failing the test.
    ///
    /// It is **not** in the production path, deliberately. If the real
    /// `llama-completion` on somebody's machine is busy, that is a fact about
    /// their machine that Thalyx should report rather than paper over. What is
    /// being papered over here is this process racing with itself, which is a
    /// property of running twelve test threads and of nothing else.
    fn run_past_the_fork_window(
        model: &LlamaModel,
        transcript: &Transcript,
    ) -> std::result::Result<Run, LlamaError> {
        past_the_fork_window(|| model.run(transcript))
    }

    /// The same, for the check that runs the stand-in twice inside one call.
    ///
    /// Added on 2026-08-10, after the run above went green and this one did
    /// not. The comment on `run_past_the_fork_window` claimed *every test in
    /// this file calls this instead of `LlamaModel::run`*, and it was not true:
    /// three tests reach `run` through [`LlamaModel::grammar_check`], which
    /// spawns the binary twice and was never wrapped. A comment claiming a
    /// property is not the property — the same lesson `list_disks` learned, in
    /// the same repository, for the same reason.
    fn grammar_check_past_the_fork_window(
        model: &LlamaModel,
    ) -> std::result::Result<GrammarCheck, LlamaError> {
        past_the_fork_window(|| model.grammar_check())
    }

    fn past_the_fork_window<T>(
        mut attempt: impl FnMut() -> std::result::Result<T, LlamaError>,
    ) -> std::result::Result<T, LlamaError> {
        for round in 0..100 {
            match attempt() {
                Err(LlamaError::Spawn { source, .. })
                    if source.kind() == std::io::ErrorKind::ExecutableFileBusy =>
                {
                    // Another thread's child, between its fork and its exec,
                    // holding a write descriptor to this file. Short, and not
                    // ours to shorten.
                    std::thread::sleep(std::time::Duration::from_millis(1 + round / 10));
                }
                other => return other,
            }
        }
        panic!("the stand-in stayed busy for a hundred attempts")
    }

    #[test]
    fn a_binary_something_else_is_still_writing_is_waited_for_and_not_reported_as_broken() {
        // The defect that failed Cesar's run of 2026-08-10, reproduced on
        // purpose instead of waited for. Holding the file open for writing is
        // exactly the state `execve` refuses — the kernel checks a count on the
        // inode, so it does not matter which process holds it or whether the
        // descriptor is close-on-exec.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            r#"cat "$4"; printf '%s' '{"operation": "install_module", "targets": ["dev.thalyx.demo"]}'"#,
        );
        let model = LlamaModel::new(Invocation::new(&binary, &weights));

        let held = std::fs::OpenOptions::new()
            .write(true)
            .open(&binary)
            .unwrap();

        // The baseline. Without it this test could pass on a kernel that never
        // returns ETXTBSY at all, and would be proving nothing about the retry.
        let refused = model.run(&transcript());
        assert!(
            matches!(
                &refused,
                Err(LlamaError::Spawn { source, .. })
                    if source.kind() == std::io::ErrorKind::ExecutableFileBusy
            ),
            "the kernel did not refuse a binary held open for writing: {refused:?}"
        );

        // And the claim: the harness waits it out rather than failing whatever
        // test happened to be running when another thread forked.
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            drop(held);
        });
        let run = run_past_the_fork_window(&model, &transcript())
            .expect("the stand-in should have been waited for");
        assert!(run.answer.contains("install_module"), "{}", run.answer);
    }

    fn weights(dir: &Path) -> PathBuf {
        let path = dir.join("weights.gguf");
        std::fs::write(&path, b"not really a model").unwrap();
        path
    }

    #[test]
    fn a_missing_binary_and_missing_weights_are_told_apart() {
        // Rule 10, and the reason it is a rule: "not installed" sends somebody
        // to a package manager and "not at that path" sends them to a file
        // listing, and being sent to the wrong one costs an afternoon.
        let scratch = tempfile::tempdir().unwrap();
        let weights = scratch.path().join("weights.gguf");

        let no_weights = LlamaModel::new(Invocation::new("/bin/sh", &weights));
        assert!(matches!(
            no_weights.preflight(),
            Err(LlamaError::WeightsMissing(_))
        ));

        std::fs::write(&weights, b"not really a model").unwrap();
        let no_binary = LlamaModel::new(Invocation::new(
            scratch.path().join("llama-cli-that-is-not-there"),
            &weights,
        ));
        assert!(matches!(
            no_binary.preflight(),
            Err(LlamaError::NotInstalled(_))
        ));
    }

    #[test]
    fn the_answer_is_taken_from_after_the_marker_the_prompt_ended_with() {
        // The closest this container gets to an end-to-end run: a process that
        // behaves the way llama-cli does when it echoes the prompt back before
        // the completion. What is being checked is that the answer survives the
        // round trip through a real process, real pipes and real files.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        // `$4` is the prompt file, since the flags go -m W -f P ...
        let binary = stand_in(
            scratch.path(),
            r#"cat "$4"; printf '%s' '{"operation": "install_module", "targets": ["dev.thalyx.demo"]}'"#,
        );

        let run = run_past_the_fork_window(
            &LlamaModel::new(Invocation::new(&binary, &weights)),
            &transcript(),
        )
        .expect("the stand-in exits cleanly");

        let proposal = crate::proposal::Proposal::parse(&run.answer)
            .expect("the echoed prompt was not stripped back off");
        assert_eq!(proposal.targets, ["dev.thalyx.demo"]);
    }

    #[test]
    fn the_bytes_a_real_llama_cpp_printed_are_read_as_the_proposal_they_are() {
        // The second defect Cesar hit on iron, and the one that cost the model
        // its reputation for a day: Qwen answered exactly what the grammar
        // describes, llama.cpp appended its own ` [end of text]`, and Thalyx
        // accused the tool of ignoring a grammar it had obeyed.
        //
        // The stand-in prints the captured bytes verbatim, which makes this the
        // first test in this file built on a sample rather than on a guess about
        // the format. Every earlier stand-in stopped where the parser expected
        // an answer to stop, because the same person wrote both.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            r#"cat "$4"
cat <<'CAPTURED'
{
  "operation": "install_module",
  "targets": [
    "dev.thalyx.demo"
  ]
} [end of text]
CAPTURED"#,
        );

        let run = run_past_the_fork_window(
            &LlamaModel::new(Invocation::new(&binary, &weights)),
            &transcript(),
        )
        .expect("a correct proposal, followed by llama.cpp's own suffix");

        let proposal = crate::proposal::Proposal::parse(&run.answer)
            .expect("the tool's suffix was left on the model's answer");
        assert_eq!(proposal.targets, ["dev.thalyx.demo"]);
        assert!(
            !run.answer.contains("end of text"),
            "the answer carries text the model did not write: {:?}",
            run.answer
        );
    }

    /// A stand-in for the *interactive* llama.cpp frontend.
    ///
    /// This is the regression test for what a real llama.cpp did on 2026-08-08.
    /// Handed `-f`, the new `llama-cli` opens a session on the file rather than
    /// completing it: it loads, prints a banner and its slash commands, reads
    /// end-of-input from the closed stdin and exits **zero**. Nothing errors.
    ///
    /// It is deliberately not a copy of llama.cpp's banner — rule 6, and this is
    /// not a claim about that text. The only property it models is the one that
    /// matters: **cleanly exiting without ever completing the prompt.**
    fn interactive_stand_in(dir: &Path) -> PathBuf {
        stand_in(
            dir,
            "echo 'main: interactive mode; type /help for commands'\n\
             echo 'Available commands: /exit /regen /clear'\n\
             echo '> '\n\
             exit 0",
        )
    }

    #[test]
    fn a_tool_that_opens_a_session_instead_of_completing_is_named_as_the_problem() {
        // The defect Cesar hit on iron. It used to arrive as "the model said
        // something that does not parse", which sends whoever reads it to look
        // at Qwen — and the model had never been asked anything.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = interactive_stand_in(scratch.path());

        let error = run_past_the_fork_window(
            &LlamaModel::new(Invocation::new(&binary, &weights)),
            &transcript(),
        )
        .expect_err("a chat session is not a completion");

        let LlamaError::NotOneShot { sample, .. } = &error else {
            panic!("a tool that never completed the prompt was reported as {error}");
        };
        assert!(
            sample.contains("/regen"),
            "the diagnosis does not show what it got instead: {sample:?}"
        );
        assert!(
            error.to_string().contains(COMPLETION_BINARY),
            "the error does not name the tool that would work: {error}"
        );
    }

    #[test]
    fn a_program_that_prints_nothing_at_all_never_read_the_prompt() {
        // Distinct from the test below, and the distinction is the point: a
        // tool that printed nothing cannot have echoed the prompt, so it did
        // not complete it. That is a broken contract, not a quiet model.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(scratch.path(), "exit 0");

        let error = run_past_the_fork_window(
            &LlamaModel::new(Invocation::new(&binary, &weights)),
            &transcript(),
        )
        .expect_err("no output means no completion");
        assert!(
            matches!(error, LlamaError::NotOneShot { .. }),
            "got {error}"
        );
    }

    #[test]
    fn a_model_that_completed_to_nothing_is_an_empty_answer_and_not_a_broken_tool() {
        // The control for the two above. Without it, a contract check that
        // rejected everything would pass them both and Thalyx would have
        // stopped being able to run any model at all.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(scratch.path(), r#"cat "$4""#);

        let run = run_past_the_fork_window(
            &LlamaModel::new(Invocation::new(&binary, &weights)),
            &transcript(),
        )
        .expect("the prompt was read; the completion was empty");

        assert_eq!(run.answer, "");
        assert_eq!(
            crate::proposal::Proposal::parse(&run.answer),
            Err(crate::proposal::ProposalError::Empty)
        );
    }

    #[test]
    fn an_answer_that_ran_out_of_tokens_is_told_apart_from_one_the_grammar_never_shaped() {
        // Rule 10, in the production path rather than in the probe. Both of
        // these fail to parse, and they want opposite things done about them:
        // one says raise the budget or ask a narrower question, the other says
        // the grammar is not reaching llama.cpp at all. They used to be the
        // same message, and it was the wrong one for the case that actually
        // happens.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());

        let truncated = stand_in(
            scratch.path(),
            r#"cat "$4"; printf '%s' '{"operation": "install_module", "targets": ["dev.thalyx.aaaa'"#,
        );
        let error = run_past_the_fork_window(
            &LlamaModel::new(Invocation::new(&truncated, &weights)),
            &transcript(),
        )
        .expect_err("an unclosed object is not an answer");
        assert!(
            matches!(error, LlamaError::Truncated { .. }),
            "an obedient answer that ran out of budget was reported as {error}"
        );
        assert!(
            error.to_string().contains("the grammar working"),
            "the message does not say which of the two this is: {error}"
        );

        // The control. Without it, a check that called everything a truncation
        // would pass the test above while losing the failure that matters.
        let unconstrained = stand_in(
            scratch.path(),
            r#"cat "$4"; printf '%s' 'Sure, I can install that for you'"#,
        );
        let error = run_past_the_fork_window(
            &LlamaModel::new(Invocation::new(&unconstrained, &weights)),
            &transcript(),
        )
        .expect_err("prose is not an answer");
        assert!(
            matches!(error, LlamaError::GrammarNotInForce { .. }),
            "an answer the grammar never shaped was reported as {error}"
        );
    }

    #[test]
    fn a_completion_that_is_prose_means_the_grammar_was_not_applied() {
        // A grammar-constrained completion cannot produce prose. So prose after
        // the marker is the tool ignoring --grammar-file, and saying "the model
        // answered badly" would again blame the wrong side.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            r#"cat "$4"; printf '%s' 'Sure! I can help you install that module.'"#,
        );

        let error = run_past_the_fork_window(
            &LlamaModel::new(Invocation::new(&binary, &weights)),
            &transcript(),
        )
        .expect_err("prose is not a proposal and not the model's fault");
        assert!(
            matches!(error, LlamaError::GrammarNotInForce { .. }),
            "got {error}"
        );
    }

    /// A stand-in that actually is constrained by the flag.
    ///
    /// Rule 8, and the reason this one is written the long way: a fake that
    /// printed a proposal whatever it was given would pass the check that says
    /// the grammar works, while modelling a tool that ignores the grammar
    /// completely. What has to be modelled is the *dependence* — say the word
    /// when free to, and be unable to when not.
    fn obeys_the_grammar(dir: &Path) -> PathBuf {
        stand_in(
            dir,
            r#"cat "$4"
case "$*" in
  *--grammar-file*) printf '%s' '{"operation": "install_module", "targets": []}' ;;
  *)                printf '%s' 'BANANA' ;;
esac"#,
        )
    }

    #[test]
    fn a_tool_whose_answer_changes_with_the_grammar_flag_proves_the_grammar_is_applied() {
        // The claim `run` cannot make. llama.cpp exits non-zero on a flag it
        // does not know, so a clean run proves --grammar-file was *accepted*;
        // it says nothing about whether it constrained anything, because the
        // real prompt asks for an object and a model that gives one was only
        // doing as it was told.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = obeys_the_grammar(scratch.path());

        let check = grammar_check_past_the_fork_window(&LlamaModel::new(Invocation::new(
            &binary, &weights,
        )))
        .expect("both arms ran");

        let GrammarCheck::InForce { unconstrained, .. } = &check else {
            panic!("a tool that plainly obeys the grammar was reported as {check:?}");
        };
        assert!(
            unconstrained.contains(crate::prompt::PROBE_WORD),
            "the control arm did not show the model doing what it was asked: {unconstrained:?}"
        );
    }

    #[test]
    fn an_obedient_answer_cut_off_by_the_token_cap_is_not_a_broken_grammar() {
        // What Cesar's machine actually did, and what the first version of this
        // check called a failure. Told to say a word the grammar forbids, the
        // model could not refuse and could not comply, so it went hunting for a
        // legal way to say it — a module id reading `banana_module_1234…` — and
        // ran into the token cap mid-string.
        //
        // The answer therefore did not parse. It was also the most obedient
        // output the grammar could possibly have produced: it opens exactly
        // where `root` opens. Judging by "did it parse" read maximal obedience
        // as no grammar at all. Rule 10, in a new place: a failure to *finish*
        // is not a failure to comply.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            r#"cat "$4"
case "$*" in
  *--grammar-file*) printf '%s' '{
  "operation": "install_module",
  "targets": ["banana_module_1234567890123456789012345678901234567890' ;;
  *)                printf '%s' 'BANANA' ;;
esac"#,
        );

        let check = grammar_check_past_the_fork_window(&LlamaModel::new(Invocation::new(
            &binary, &weights,
        )))
        .expect("both arms ran");

        assert!(
            matches!(check, GrammarCheck::InForce { .. }),
            "a truncated but perfectly obedient answer was reported as {check:?}"
        );
    }

    #[test]
    fn a_tool_that_says_the_forbidden_word_while_constrained_is_not_applying_the_grammar() {
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(scratch.path(), r#"cat "$4"; printf '%s' 'BANANA'"#);

        let check = grammar_check_past_the_fork_window(&LlamaModel::new(Invocation::new(
            &binary, &weights,
        )))
        .expect("both arms ran");
        assert!(
            matches!(check, GrammarCheck::NotInForce { .. }),
            "a tool ignoring --grammar-file was reported as {check:?}"
        );
    }

    #[test]
    fn a_tool_that_answers_the_same_either_way_is_inconclusive_and_not_a_pass() {
        // Rule 3 and rule 4 together. This is the shape a real 3B might take —
        // a model that gives JSON whether or not anything made it — and it must
        // not be counted as evidence the grammar works. Without this arm, a
        // grammar that is silently doing nothing looks exactly like one that is.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            r#"cat "$4"; printf '%s' '{"operation": "install_module", "targets": []}'"#,
        );

        let check = grammar_check_past_the_fork_window(&LlamaModel::new(Invocation::new(
            &binary, &weights,
        )))
        .expect("both arms ran");
        assert!(
            matches!(check, GrammarCheck::Inconclusive { .. }),
            "a probe that measured nothing was reported as {check:?}"
        );
    }

    #[test]
    fn a_control_arm_that_said_nothing_proves_nothing_however_obedient_the_other_was() {
        // The light tier, 2026-08-08, on Cesar's Fedora. Qwen2.5-1.5B answered
        // the free probe by ending generation immediately, so the control arm
        // came back as the trailer llama.cpp prints after an empty completion:
        //
        //     with the grammar     { "operation": "install_module", "targets": ["python3.…
        //     without it           [end of text]
        //
        // and the probe said PROVEN, over a verdict that reads "left alone it
        // did [say the word]". Nothing had checked that. `InForce` was the
        // `else`, so it was reached by the free arm merely failing to open an
        // object — and an arm that says nothing fails that too.
        //
        // The trailer is verbatim from that build rather than invented: rule 6,
        // and the whole point is what a real tool prints when a model is quiet.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            r#"cat "$4"
case "$*" in
  *--grammar-file*) printf '%s' '{"operation": "install_module", "targets": ["python3.abc_1.abc"' ;;
  *)                printf '%s' ' [end of text]' ;;
esac"#,
        );

        let check = grammar_check_past_the_fork_window(&LlamaModel::new(Invocation::new(
            &binary, &weights,
        )))
        .expect("both arms ran");

        assert!(
            matches!(check, GrammarCheck::Inconclusive { .. }),
            "a probe whose control arm never said the word was reported as {check:?}"
        );
    }

    #[test]
    fn a_control_arm_that_said_something_else_entirely_proves_nothing_either() {
        // The general shape of the case above, and the reason the check is
        // "did it say the word" rather than "was it silent": a free arm that
        // answers with prose has also not shown the model would say the word,
        // so the grammar is not what the difference is attributable to.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            r#"cat "$4"
case "$*" in
  *--grammar-file*) printf '%s' '{"operation": "install_module", "targets": []}' ;;
  *)                printf '%s' 'Sure! I can help you with that.' ;;
esac"#,
        );

        let check = grammar_check_past_the_fork_window(&LlamaModel::new(Invocation::new(
            &binary, &weights,
        )))
        .expect("both arms ran");

        assert!(
            matches!(check, GrammarCheck::Inconclusive { .. }),
            "a control arm that answered something else was reported as {check:?}"
        );
    }

    #[test]
    fn the_word_the_probe_asks_for_is_one_the_grammar_cannot_produce() {
        // The instrument includes the harness. If the probe word happened to be
        // readable as a proposal, every arm of the check above would invert and
        // the whole thing would report the opposite of what it saw.
        assert!(crate::proposal::Proposal::completion_in(crate::prompt::PROBE_WORD).is_none());

        // `root` is three alternatives now rather than one object, so it is no
        // longer enough to read the first production and see a brace. Every
        // one of them has to open on `ROOT_FIRST_CHAR`, because a constrained
        // decode can take any of them and the probe rests on all of them
        // starting the same way.
        let grammar = crate::grammar::gbnf();
        let root = grammar
            .lines()
            .find(|line| line.starts_with("root "))
            .expect("the grammar has a root");
        let alternatives: Vec<&str> = root
            .split("::=")
            .nth(1)
            .expect("root has a body")
            .split('|')
            .map(str::trim)
            .collect();
        assert!(alternatives.len() > 1, "read {root:?} as one alternative");

        for alternative in alternatives {
            let production = grammar
                .lines()
                .find(|line| line.starts_with(&format!("{alternative} ")))
                .unwrap_or_else(|| panic!("{alternative} is named by root and never defined"));
            assert!(
                production.contains(&format!(r#"::= "{}""#, crate::grammar::ROOT_FIRST_CHAR)),
                "{alternative} does not open on {:?}, so a constrained decode \
                 can put something else first and the probe proves nothing",
                crate::grammar::ROOT_FIRST_CHAR
            );
        }
    }

    #[test]
    fn the_stored_settings_have_no_field_that_could_turn_the_grammar_off() {
        // `Gamas-de-Modelo.md` decrees that every inference runs grammar
        // constrained, so a setting for skipping it would be a decree with an
        // opt-out. `Constrained::No` is a private argument only the probe
        // passes, and this reads the *serialised shape* rather than the source
        // text — a doc comment mentioning the grammar is not a knob, and the
        // first version of this test could not tell the two apart.
        //
        // Not sealed, and saying so is the point: `extra_args` takes arbitrary
        // flags by design, so somebody can hand their own llama.cpp a second
        // --grammar-file. That is a person switching off a quality guarantee on
        // their own machine, not a hole — the defence against a model that
        // misbehaves is attribution, and it holds whatever the grammar does.
        let settings = crate::config::Settings {
            tier: "media".to_string(),
            weights: PathBuf::from("/models/qwen.gguf"),
            binary: PathBuf::from(COMPLETION_BINARY),
            extra_args: vec![],
            predict: 256,
            seed: 1,
            timeout_seconds: 180,
            weights_bytes: 1,
            weights_digest: "sha256:00".to_string(),
            engine_module: None,
        };

        for line in toml::to_string(&settings)
            .expect("the settings serialise")
            .lines()
        {
            let key = line.split('=').next().unwrap_or_default().trim();
            assert!(
                !key.contains("grammar") && !key.contains("constrain"),
                "the config file grew {key:?}, which is how the grammar becomes optional"
            );
        }
    }

    #[test]
    fn the_flag_that_would_hide_the_evidence_is_not_passed() {
        // `--no-display-prompt` suppresses the echo, and the echo is the only
        // proof that the prompt was read. With it, the tool that opens a
        // session and the tool that completes silently are the same bytes.
        let invocation = Invocation::new(COMPLETION_BINARY, "/models/qwen.gguf");
        assert!(
            !invocation
                .extra_args
                .iter()
                .any(|a| a == "--no-display-prompt"),
            "the contract check has been disarmed by a default flag"
        );
    }

    #[test]
    fn a_program_that_fails_carries_out_what_it_said_on_stderr() {
        // The one thing this container cannot check about llama.cpp is whether
        // his build accepts these flags. When it does not, the words it used to
        // say so are the entire diagnosis, so they must not be swallowed.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            "echo 'error: unknown argument: --no-display-prompt' >&2; exit 1",
        );

        let error = run_past_the_fork_window(
            &LlamaModel::new(Invocation::new(&binary, &weights)),
            &transcript(),
        )
        .expect_err("a non-zero exit is not an answer");
        let LlamaError::Exited { stderr, .. } = &error else {
            panic!("expected a non-zero exit, got {error}");
        };
        assert!(
            stderr.contains("unknown argument"),
            "the diagnosis was dropped: {stderr:?}"
        );
    }

    #[test]
    fn a_process_that_never_finishes_is_killed_rather_than_waited_for() {
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(scratch.path(), "sleep 60");

        let mut invocation = Invocation::new(&binary, &weights);
        invocation.timeout = Duration::from_millis(300);

        let started = Instant::now();
        let error = run_past_the_fork_window(&LlamaModel::new(invocation), &transcript())
            .expect_err("a process that never answers is not an answer");

        assert!(matches!(error, LlamaError::TimedOut(_)), "got {error}");
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the deadline did not fire; the process was waited for instead"
        );
    }

    #[test]
    fn a_program_that_will_not_stop_writing_is_stopped() {
        // The token cap belongs to llama.cpp and is the first line of defence.
        // This is the case where the cap is not doing its job, which is exactly
        // when nobody is watching.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            "yes 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' | head -c 200000",
        );

        let mut invocation = Invocation::new(&binary, &weights);
        invocation.timeout = Duration::from_secs(20);

        let error = run_past_the_fork_window(&LlamaModel::new(invocation), &transcript())
            .expect_err("200 kB of output is a runaway, not an answer");
        assert!(matches!(error, LlamaError::Runaway), "got {error}");
    }

    #[test]
    fn the_command_line_printed_is_the_one_that_would_run() {
        // It exists so a strange answer can be reproduced in a terminal. A
        // printed command missing the grammar would reproduce a different
        // inference and prove the wrong thing innocent.
        let invocation = Invocation::new(COMPLETION_BINARY, "/models/qwen.gguf");
        let line = invocation.command_line(Path::new("/tmp/p.txt"), Some(Path::new("/tmp/g.gbnf")));

        for expected in [
            COMPLETION_BINARY,
            "-m /models/qwen.gguf",
            "-f /tmp/p.txt",
            "--grammar-file /tmp/g.gbnf",
            "--temp 0",
            "--seed 1",
        ] {
            assert!(
                line.contains(expected),
                "{expected:?} missing from {line:?}"
            );
        }
        for extra in &invocation.extra_args {
            assert!(line.contains(extra), "{extra:?} missing from {line:?}");
        }
    }

    #[test]
    fn the_free_arm_of_the_probe_is_not_written_down_as_a_constrained_one() {
        // The probe's two arms differ in exactly one flag, so a command line
        // that names a grammar for the arm that ran without one describes the
        // other arm. Somebody pasting it would watch the grammar hold and
        // conclude the probe had lied to them.
        let invocation = Invocation::new(COMPLETION_BINARY, "/models/qwen.gguf");
        let line = invocation.command_line(Path::new("/tmp/p.txt"), None);

        assert!(!line.contains("--grammar-file"), "got {line:?}");
        assert!(line.contains("-f /tmp/p.txt"), "got {line:?}");
    }

    #[test]
    fn a_kept_prompt_outlives_the_run_and_carries_the_marker_that_ran() {
        // The reason this exists: `Prompt::render` mints a new marker every
        // invocation, so re-rendering the same transcript afterwards does not
        // rebuild the prompt that produced a strange answer. The bytes have to
        // survive the run or they are unrecoverable.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(
            scratch.path(),
            r#"cat "$4"; printf '%s' '{"operation": "install_module", "targets": ["dev.thalyx.demo"]}'"#,
        );
        let kept = scratch.path().join("kept");

        let mut invocation = Invocation::new(&binary, &weights);
        invocation.keep_prompt = Some(kept.clone());
        run_past_the_fork_window(&LlamaModel::new(invocation), &transcript())
            .expect("the stand-in answers");

        let dirs: Vec<_> = std::fs::read_dir(&kept)
            .expect("the kept directory exists")
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(dirs.len(), 1, "one run, one directory: {dirs:?}");

        let prompt = std::fs::read_to_string(dirs[0].join("prompt.txt")).unwrap();
        let command = std::fs::read_to_string(dirs[0].join("command")).unwrap();
        assert!(prompt.contains("<<<THALYX-"), "no marker in {prompt:?}");
        assert!(
            dirs[0].to_string_lossy().ends_with("-with-grammar"),
            "the arm is not in the name: {dirs:?}"
        );
        assert!(dirs[0].join("proposal.gbnf").exists());
        assert!(
            command.contains("--grammar-file") && command.contains("-f "),
            "the command does not name what it ran: {command:?}"
        );
    }

    #[test]
    fn the_two_arms_of_a_probe_do_not_overwrite_each_other() {
        // Both probes run one rendered prompt twice, so the two arms share a
        // marker. Named by the marker alone they shared a directory, and the
        // free arm's `command` — the one with no --grammar-file — was what
        // survived. The evidence for the constrained arm would have been a
        // command line describing the other arm.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(scratch.path(), r#"cat "$4"; printf '%s' 'BANANA'"#);
        let kept = scratch.path().join("kept");

        let mut invocation = Invocation::new(&binary, &weights);
        invocation.keep_prompt = Some(kept.clone());
        let _ = grammar_check_past_the_fork_window(&LlamaModel::new(invocation));

        let mut names: Vec<String> = std::fs::read_dir(&kept)
            .expect("the kept directory exists")
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names.len(), 2, "the two arms shared a directory: {names:?}");

        let commands: Vec<String> = names
            .iter()
            .map(|name| std::fs::read_to_string(kept.join(name).join("command")).unwrap())
            .collect();
        assert!(
            commands.iter().any(|c| c.contains("--grammar-file")),
            "no arm kept the constrained command: {commands:?}"
        );
        assert!(
            commands.iter().any(|c| !c.contains("--grammar-file")),
            "no arm kept the free command: {commands:?}"
        );
    }

    #[test]
    fn the_three_arms_leave_three_directories_that_say_which_arm_they_are() {
        // Two of the three run unconstrained, and they are different questions.
        // Named by the flag alone the evidence would hold two directories called
        // `-free` per case, and telling them apart would mean opening the
        // prompts — which is exactly the reading the kept evidence exists to
        // save somebody from.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(scratch.path(), r#"cat "$4"; printf '%s' 'dev.thalyx.demo'"#);
        let kept = scratch.path().join("kept");

        let mut invocation = Invocation::new(&binary, &weights);
        invocation.keep_prompt = Some(kept.clone());
        let (constrained, free, prose) = LlamaModel::new(invocation).three_ways(&transcript());
        assert!(constrained.is_ok() && free.is_ok() && prose.is_ok());

        let mut names: Vec<String> = std::fs::read_dir(&kept)
            .expect("the kept directory exists")
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names.len(), 3, "two arms shared a directory: {names:?}");

        for suffix in ["-with-grammar", "-free", "-prose"] {
            assert!(
                names.iter().any(|name| name.ends_with(suffix)),
                "no directory is the {suffix} arm: {names:?}"
            );
        }

        // And the prose arm's kept prompt has to be the prose one. A name that
        // says `-prose` over the object prompt would be worse than no name.
        let prose_dir = names.iter().find(|n| n.ends_with("-prose")).unwrap();
        let asked = std::fs::read_to_string(kept.join(prose_dir).join("prompt.txt")).unwrap();
        assert!(!asked.contains('{'), "the prose arm kept an object prompt");
        assert!(asked.contains(crate::prompt::ABSTENTION_WORD));
    }

    #[test]
    fn asking_for_no_path_leaves_nothing_behind_and_asking_for_one_leaves_everything() {
        // The default has to stay the disposable one: a bench of twenty cases
        // that quietly filled a directory would be a disk leak nobody asked
        // for, and the tier run most often would leak most. Both halves are
        // here because a `Scratch` that always kept and a `Scratch` that always
        // discarded each pass one of them.
        let root = tempfile::tempdir().unwrap();

        let discarded = {
            let scratch = Scratch::open(None, None, "ignored").unwrap();
            std::fs::write(scratch.path().join("prompt.txt"), "asked").unwrap();
            scratch.path().to_path_buf()
        };
        assert!(!discarded.exists(), "{discarded:?} survived the run");

        let kept = {
            let scratch = Scratch::open(Some(root.path()), None, "THALYXdeadbeef").unwrap();
            std::fs::write(scratch.path().join("prompt.txt"), "asked").unwrap();
            scratch.path().to_path_buf()
        };
        assert_eq!(
            std::fs::read_to_string(kept.join("prompt.txt")).unwrap(),
            "asked"
        );
        assert!(
            kept.ends_with("THALYXdeadbeef"),
            "{kept:?} is not named after its marker"
        );
    }

    #[test]
    fn peak_memory_that_was_never_sampled_is_reported_as_unknown_not_as_zero() {
        // Rule 10 again. A tier reported as using 0 bytes of RAM would be read
        // as a measurement, and the number a bench prints is the whole reason
        // the bench exists.
        let run = Run {
            answer: String::new(),
            latency: Duration::from_millis(1),
            peak_rss: None,
        };
        assert!(run.peak_rss.is_none());
    }

    #[test]
    fn a_bare_name_is_looked_for_on_path_and_a_path_is_not() {
        assert!(resolves_to_a_program(Path::new("sh")) || std::env::var("PATH").is_err());
        assert!(!resolves_to_a_program(Path::new(
            "a-program-with-this-name-does-not-exist"
        )));
        assert!(!resolves_to_a_program(Path::new(
            "/does/not/exist/llama-cli"
        )));
    }
}
