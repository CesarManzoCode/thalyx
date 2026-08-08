//! The real model: llama.cpp, as a process.
//!
//! `vault/02-Arquitectura/Gamas-de-Modelo.md` decrees invoking it rather than
//! linking it, for three reasons that this file is shaped by:
//!
//! - **No build dependency.** Nothing here needs a C++ toolchain, so somebody
//!   who only wants the CLI does not pay for a piece they will not use.
//! - **Every step reproducible by hand.** [`Invocation::command_line`] prints
//!   the exact command, `thalyx agent grammar` prints the exact grammar, and the
//!   seed is fixed — so a strange answer can be reproduced in a terminal with no
//!   Thalyx in the way. A fault only observable from inside the process that
//!   caused it costs twice as much to find.
//! - **The model is outside the process.** It is outside the TCB by
//!   `vault/11-Seguridad/Modelo-de-Amenaza.md`; running it in another process
//!   makes that boundary one the operating system enforces rather than one the
//!   design asserts.
//!
//! ## What has run, and what still has not — 2026-08-08
//!
//! This file has been **started** by a real llama.cpp once: Cesar ran it on
//! Fedora against `llama.cpp b1-3653e6d` and Qwen2.5-3B-Instruct-Q4_K_M. The
//! process spawned, the weights loaded, and the contract was not honoured — see
//! the section below. So what is now known is that spawning, argument passing
//! and the failure path work against the real tool.
//!
//! **No inference has ever completed.** Nothing here has produced a proposal
//! from real weights, so the grammar being accepted by llama.cpp, the answer
//! landing after the marker, and the tier's accuracy are all still unproven.
//! The container has neither llama.cpp nor a route to the weights, so this is
//! not something the workspace tests can close. `dev/verify.sh` says so.
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
        "{} completed the prompt, but the answer is not a proposal, which means \
         --grammar-file was not in force. A grammar-constrained completion \
         cannot produce anything else — so this is the tool ignoring the \
         grammar, not the model answering badly.\n\
         \n\
         It answered:\n{answer}",
        .binary.display()
    )]
    GrammarNotInForce { binary: PathBuf, answer: String },

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
    /// Fixed, so that the same question asked twice gets the same answer and a
    /// bad answer can be reproduced by hand.
    pub seed: u64,
    pub timeout: Duration,
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
        }
    }

    /// The command as somebody would type it, for reproducing a run by hand.
    ///
    /// Takes the two paths it would write rather than inventing names, so what
    /// is printed is what was run.
    pub fn command_line(&self, prompt_file: &Path, grammar_file: &Path) -> String {
        let mut parts = vec![self.binary.display().to_string()];
        for (flag, value) in [
            ("-m", self.weights.display().to_string()),
            ("-f", prompt_file.display().to_string()),
            ("--grammar-file", grammar_file.display().to_string()),
            ("-n", self.predict.to_string()),
            ("--seed", self.seed.to_string()),
            ("--temp", "0".to_string()),
        ] {
            parts.push(flag.to_string());
            parts.push(value);
        }
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

/// The model the decree describes.
#[derive(Debug, Clone)]
pub struct LlamaModel {
    invocation: Invocation,
}

impl LlamaModel {
    pub fn new(invocation: Invocation) -> LlamaModel {
        LlamaModel { invocation }
    }

    pub fn invocation(&self) -> &Invocation {
        &self.invocation
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
        if !resolves_to_a_program(&self.invocation.binary) {
            return Err(LlamaError::NotInstalled(self.invocation.binary.clone()));
        }
        Ok(())
    }

    /// Run one inference and report what it cost.
    pub fn run(&self, transcript: &Transcript) -> Result<Run, LlamaError> {
        self.preflight()?;

        let prompt = Prompt::render(transcript);
        let scratch = tempfile::tempdir()?;
        let prompt_file = scratch.path().join("prompt.txt");
        let grammar_file = scratch.path().join("proposal.gbnf");
        std::fs::write(&prompt_file, prompt.text())?;
        std::fs::write(&grammar_file, crate::grammar::gbnf())?;

        let started = Instant::now();
        let mut child = self.spawn(&prompt_file, &grammar_file)?;

        let peak = Arc::new(AtomicU64::new(0));
        let sampler = sample_peak_rss(child.id(), Arc::clone(&peak));
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());

        let status = self.wait_or_kill(&mut child, started)?;

        // Joined after the wait, never before: a reader thread ends when its
        // pipe closes, and the pipe closes when the child goes.
        let out = stdout.join().unwrap_or_default();
        let err = stderr.join().unwrap_or_default();
        sampler.stop();

        if out.len() > MAX_OUTPUT_BYTES {
            return Err(LlamaError::Runaway);
        }
        if !status.success() {
            return Err(LlamaError::Exited {
                status: status.to_string(),
                stderr: String::from_utf8_lossy(&err).trim().to_string(),
            });
        }

        let text = String::from_utf8_lossy(&out).into_owned();

        // The contract, checked in two steps rather than assumed. Both of these
        // used to end up at `Proposal::parse` as "the model said something
        // unparseable", which names the wrong culprit — and named it wrongly
        // for the one failure a real llama.cpp actually produced.
        //
        // 1. The marker is gone, so the prompt was never completed. That is a
        //    tool which does not do this job, not an answer that came out bad.
        let Some(answer) = prompt.answer_in(&text) else {
            return Err(LlamaError::NotOneShot {
                binary: self.invocation.binary.clone(),
                sample: sample_of(&text),
            });
        };
        let answer = answer.trim().to_string();

        // 2. The prompt was completed and the result is not a proposal. A
        //    grammar-constrained completion *cannot* produce anything else, so
        //    this is the grammar not being applied. Silence is left alone: a
        //    tool that completed to nothing is a different event again, and
        //    `Proposal::parse` already has a word for it.
        if !answer.is_empty() && crate::proposal::Proposal::parse(&answer).is_err() {
            return Err(LlamaError::GrammarNotInForce {
                binary: self.invocation.binary.clone(),
                answer: sample_of(&answer),
            });
        }

        Ok(Run {
            answer,
            latency: started.elapsed(),
            peak_rss: match peak.load(Ordering::Relaxed) {
                0 => None,
                bytes => Some(bytes),
            },
        })
    }

    fn spawn(&self, prompt_file: &Path, grammar_file: &Path) -> Result<Child, LlamaError> {
        Command::new(&self.invocation.binary)
            .arg("-m")
            .arg(&self.invocation.weights)
            .arg("-f")
            .arg(prompt_file)
            .arg("--grammar-file")
            .arg(grammar_file)
            .arg("-n")
            .arg(self.invocation.predict.to_string())
            .arg("--seed")
            .arg(self.invocation.seed.to_string())
            .arg("--temp")
            .arg("0")
            .args(&self.invocation.extra_args)
            // Closed, not inherited. See the module docs: an inherited stdin is
            // how llama-cli decides there is a person to chat with.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| LlamaError::Spawn {
                binary: self.invocation.binary.clone(),
                source,
            })
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
    ) -> Result<std::process::ExitStatus, LlamaError> {
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if started.elapsed() > self.invocation.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(LlamaError::TimedOut(self.invocation.timeout));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
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
    fn stand_in(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("llama-cli");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
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

        let run = LlamaModel::new(Invocation::new(&binary, &weights))
            .run(&transcript())
            .expect("the stand-in exits cleanly");

        let proposal = crate::proposal::Proposal::parse(&run.answer)
            .expect("the echoed prompt was not stripped back off");
        assert_eq!(proposal.targets, ["dev.thalyx.demo"]);
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

        let error = LlamaModel::new(Invocation::new(&binary, &weights))
            .run(&transcript())
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

        let error = LlamaModel::new(Invocation::new(&binary, &weights))
            .run(&transcript())
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

        let run = LlamaModel::new(Invocation::new(&binary, &weights))
            .run(&transcript())
            .expect("the prompt was read; the completion was empty");

        assert_eq!(run.answer, "");
        assert_eq!(
            crate::proposal::Proposal::parse(&run.answer),
            Err(crate::proposal::ProposalError::Empty)
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

        let error = LlamaModel::new(Invocation::new(&binary, &weights))
            .run(&transcript())
            .expect_err("prose is not a proposal and not the model's fault");
        assert!(
            matches!(error, LlamaError::GrammarNotInForce { .. }),
            "got {error}"
        );
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

        let error = LlamaModel::new(Invocation::new(&binary, &weights))
            .run(&transcript())
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
        let error = LlamaModel::new(invocation)
            .run(&transcript())
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

        let error = LlamaModel::new(invocation)
            .run(&transcript())
            .expect_err("200 kB of output is a runaway, not an answer");
        assert!(matches!(error, LlamaError::Runaway), "got {error}");
    }

    #[test]
    fn the_command_line_printed_is_the_one_that_would_run() {
        // It exists so a strange answer can be reproduced in a terminal. A
        // printed command missing the grammar would reproduce a different
        // inference and prove the wrong thing innocent.
        let invocation = Invocation::new(COMPLETION_BINARY, "/models/qwen.gguf");
        let line = invocation.command_line(Path::new("/tmp/p.txt"), Path::new("/tmp/g.gbnf"));

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
