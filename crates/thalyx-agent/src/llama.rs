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
//! ## What has never run
//!
//! All of it. The development container has no llama.cpp and no route to the
//! weights, so nothing in this file has been executed against the real tool —
//! only against the harness in `dev/verify.sh`, on his machine. That is stated
//! here rather than in a commit message because the next person to read this
//! deserves to know which claims are load-bearing.
//!
//! ## The version-dependent part, kept where it can be edited
//!
//! Flags come and go between llama.cpp releases. The ones this file passes
//! itself — `-m`, `-f`, `-n`, `--seed`, `--temp`, `--grammar-file` — have been
//! stable for a long time. The ones that have not are in
//! [`Invocation::extra_args`], which lives in the config file rather than in
//! this source, so a build that rejects one is fixed by editing a line instead
//! of by rebuilding Thalyx. If llama.cpp refuses a flag it says so on stderr,
//! and [`LlamaError::Exited`] carries that text out verbatim.
//!
//! Stdin is closed rather than being given a flag. Recent llama-cli versions
//! drop into an interactive chat when they think they are talking to a person,
//! and a closed stdin ends that at once — a hang is the one failure that looks
//! like nothing at all.

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

impl Invocation {
    /// The defaults, which are the flags that have been stable the longest.
    pub fn new(binary: impl Into<PathBuf>, weights: impl Into<PathBuf>) -> Invocation {
        Invocation {
            binary: binary.into(),
            weights: weights.into(),
            extra_args: vec!["-no-cnv".to_string(), "--no-display-prompt".to_string()],
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
        Ok(Run {
            answer: prompt.answer_in(&text).trim().to_string(),
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

    #[test]
    fn a_program_that_prints_nothing_is_a_failure_and_not_an_empty_answer() {
        // llama.cpp ran, exited cleanly, and said nothing. That has to reach
        // `Proposal::parse` as emptiness rather than as a proposal with fields
        // missing — silence and a bad answer are different events.
        let scratch = tempfile::tempdir().unwrap();
        let weights = weights(scratch.path());
        let binary = stand_in(scratch.path(), "exit 0");

        let run = LlamaModel::new(Invocation::new(&binary, &weights))
            .run(&transcript())
            .expect("the stand-in exits cleanly");

        assert_eq!(run.answer, "");
        assert_eq!(
            crate::proposal::Proposal::parse(&run.answer),
            Err(crate::proposal::ProposalError::Empty)
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
        let invocation = Invocation::new("llama-cli", "/models/qwen.gguf");
        let line = invocation.command_line(Path::new("/tmp/p.txt"), Path::new("/tmp/g.gbnf"));

        for expected in [
            "llama-cli",
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
