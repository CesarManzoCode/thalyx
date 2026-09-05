//! rust-analyzer, spoken to over LSP, as a read-only provider.
//!
//! ## Why this exists rather than more scanner
//!
//! `crates/thalyx-graph/corpus/05-alias` has carried this sentence for months:
//! *«`Keys` at src/boot.rs:3 is a use of `Keystore` and the index does not say
//! so. Following an alias means tracking a binding, which is a compiler and not
//! a scan.»* It is right, and it does not get less right by trying harder. Name
//! resolution in Rust is `use` renames, glob imports, prelude shadowing,
//! `macro_rules` hygiene, trait method selection and `impl` blocks spread over
//! files — a scanner that answered all of it would **be** rust-analyzer, badly.
//!
//! So Thalyx asks the thing that already answers it. What Thalyx keeps is the
//! part nobody else does: the answer's identity, its freshness, its budget, the
//! transaction it happens inside, and the authority to change anything.
//!
//! ## The boundary, said out loud
//!
//! **This process is a reader.** It is started with the workspace as its root,
//! it is asked questions, and it is killed. It never writes: a rename comes back
//! as a *description* of edits, and applying them is Thalyx's, through the same
//! path every other mutation goes through, inside the same transaction, subject
//! to the same workspace boundary. A URI that points outside the workspace is
//! refused here rather than trusted — see [`Analyzer::rename`].
//!
//! It runs outside the confinement `ejecutar` puts a foreign program in, and
//! that is a real gap rather than a decision that is finished: today it is a
//! host process Thalyx launches, and the guest architecture will have to give
//! it the same treatment `cargo` gets. `vault/06-Pendientes/Tareas-Pendientes.md`
//! carries it.
//!
//! ## Rule 5, before anything is believed
//!
//! The instrument includes the harness. A query that comes back empty is
//! reported as empty **and as having been asked**, and a server that never got
//! ready is [`Ready::TimedOut`], never an answer of "no references". A missing
//! rust-analyzer is [`RustError::NoAnalyzer`] and not a symbol that does not
//! exist: a failure to read is not a failure to exist.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{Result, RustError};

// ── whose process it is ──────────────────────────────────────────────────────

/// What starting a semantic provider needs.
#[derive(Debug, Clone, Copy)]
pub struct Launching<'a> {
    /// The server binary.
    pub program: &'a Path,
    /// The workspace it is rooted at, and its working directory.
    pub root: &'a Path,
    /// Where its Cargo is told to build. Outside the workspace, always: see
    /// [`Analyzer::start`].
    pub build_into: Option<&'a Path>,
    /// Everything it must be able to read that is not the workspace — the
    /// toolchain and the registry, read-only.
    pub readable: &'a [PathBuf],
    /// Where the toolchain is, for a process that is not the user who
    /// installed it.
    pub environment: &'a [(String, String)],
}

/// A started server, and how to end it.
pub struct Started {
    /// Its stdin and stdout must be pipes: LSP is a conversation.
    pub child: Child,
    /// Called when the analyzer is let go: kill the process tree and take the
    /// confinement down.
    ///
    /// A boxed closure rather than a type, because what has to be torn down is
    /// a cgroup and a kernel policy and **this crate must not know that**. It
    /// resolves names and describes edits; the authority that confines things
    /// is two layers up, and a `thalyx-rust` that depended on the sandbox
    /// would be the semantic provider deciding what a process may reach.
    pub release: Option<Box<dyn FnOnce() + Send>>,
    /// One phrase for the answer: `confined: <profile>`, or `host`.
    pub how: String,
    /// **Whether Thalyx's confinement is what stands behind this process.**
    ///
    /// Reported on every answer that came from it, and never assumed. See
    /// [`Spawn`].
    pub confined: bool,
}

/// How the semantic provider's process gets started.
///
/// ## Why this is a trait and not a `Command`
///
/// Until 2026-08-30 this file started rust-analyzer with `Command::new`, as an
/// ordinary host process, with everything Thalyx itself can reach — the whole
/// filesystem and the network. The justification written in this very module
/// was that it is *a reader*: it never applies an edit, a rename comes back as
/// a description, and Thalyx does the writing.
///
/// **That reasoning is about the LSP protocol and not about the process tree.**
/// rust-analyzer runs `cargo metadata`, and to answer anything about a
/// workspace with a proc-macro or a build script in it, it *compiles and runs
/// them* — which is arbitrary code from a registry, executing at analysis time,
/// with Thalyx's own reach. "It does not apply edits, therefore it is
/// read-only" was the wrong conclusion from a true premise.
///
/// So the process belongs under the same authority every other program nobody
/// signed runs under. The trait is what lets that authority live where it
/// already is: `thalyx-core` confines things, this crate asks names of a
/// compiler, and neither has to know how the other works.
pub trait Spawn: Send + Sync {
    fn start(&self, asked: Launching<'_>) -> Result<Started>;
}

/// The provider as a plain host process.
///
/// **Not the default anywhere Thalyx can enforce**, and it says `confined:
/// false` on every answer that comes through it. It exists for exactly one
/// case, which is real and is this container: a machine with no BPF LSM cannot
/// confine anything, `start_foreign` refuses rather than degrading — that is
/// `Programas-Ajenos.md`'s decree, and it is right — and a Thalyx that
/// therefore could not resolve a symbol at all would be a machine where the
/// programming face does not exist.
///
/// `THALYX_REQUIRE_CONFINED_ANALYZER=1` turns this into a refusal, which is
/// rule 3's shape: one variable per requirement, so a machine that can enforce
/// can demand that it did.
pub struct OnTheHost;

impl Spawn for OnTheHost {
    fn start(&self, asked: Launching<'_>) -> Result<Started> {
        if std::env::var("THALYX_REQUIRE_CONFINED_ANALYZER").as_deref() == Ok("1") {
            return Err(RustError::NoAnalyzer(
                "THALYX_REQUIRE_CONFINED_ANALYZER=1 and this provider would have run as \
                 an ordinary host process. Nothing started."
                    .to_string(),
            ));
        }
        let mut command = Command::new(asked.program);
        if let Some(target) = asked.build_into {
            command.env("CARGO_TARGET_DIR", target);
        }
        for (name, value) in asked.environment {
            command.env(name, value);
        }
        let child = command
            .current_dir(asked.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // It was the null device, for the reason that a pipe nobody
            // drains is a server that blocks on a full buffer halfway through
            // indexing — which looks exactly like a server that hung. Somebody
            // drains it now: `Analyzer::start` reads it continuously and keeps
            // only the last of it, so the log costs nothing while the server
            // lives and is there the moment it dies. The confined path has
            // always had this pipe; having it here too means a death is
            // diagnosed the same way whichever spawner started the process.
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                RustError::NoAnalyzer(format!("{}: {error}", asked.program.display()))
            })?;
        Ok(Started {
            child,
            release: None,
            how: "host".to_string(),
            confined: false,
        })
    }
}

// ── what the server said on its way out ──────────────────────────────────────

/// How much of the server's `stderr` is kept for the moment it dies.
///
/// A ceiling and not a buffer that grows: rust-analyzer logs while it indexes,
/// and a server held for the length of a session can write more than anybody
/// will ever read. Four kilobytes is a panic with its message, a linker error
/// or a runtime's complaint — which is what is wanted at the one moment this
/// gets read.
const STDERR_KEPT: usize = 4096;

/// How long to wait for the process's status once its channel has closed.
///
/// The pipe closing and the process being reaped are two events, in that order,
/// and asking for the status at the instant of the first would answer "still
/// running" for a process that has already died. Bounded, because a diagnosis
/// that blocks is worse than one that is vague.
const EPITAPH_GRACE: Duration = Duration::from_millis(500);

/// The tail of what the server wrote to `stderr`, and how much of it there was.
///
/// The **last** bytes and not the first. A process that dies says why on the
/// way out, and one that logged its way through an indexing pass first would
/// otherwise fill any buffer with progress before reaching the sentence that
/// matters.
#[derive(Default)]
struct LastWords {
    tail: Vec<u8>,
    total: usize,
    /// Whether the pipe reached its end. Read before the tail is quoted, so a
    /// diagnosis is not written while the dying process's last line is still in
    /// flight — rule 5, where the instrument is this reader.
    closed: bool,
}

impl LastWords {
    fn push(&mut self, bytes: &[u8]) {
        self.total += bytes.len();
        self.tail.extend_from_slice(bytes);
        if self.tail.len() > STDERR_KEPT {
            self.tail.drain(..self.tail.len() - STDERR_KEPT);
        }
    }

    /// Rendered into a sentence, saying plainly when something was dropped.
    ///
    /// Control characters go and newlines and tabs stay: rust-analyzer colours
    /// its log, and an escape sequence pasted into a diagnosis is a diagnosis
    /// that repaints the terminal somebody reads it on.
    fn spoken(&self) -> String {
        if self.total == 0 {
            return "it wrote nothing to stderr".to_string();
        }
        let text: String = String::from_utf8_lossy(&self.tail)
            .chars()
            .map(|c| {
                if c == '\n' || c == '\t' || !c.is_control() {
                    c
                } else {
                    ' '
                }
            })
            .collect();
        let text = text.trim();
        if self.total > self.tail.len() {
            format!(
                "its stderr, the last {} of {} bytes: {text}",
                self.tail.len(),
                self.total
            )
        } else {
            format!("its stderr, {} bytes: {text}", self.total)
        }
    }
}

/// Drain a dying server's `stderr` into a bounded tail, forever.
///
/// **Drained and not merely piped.** `launch::spawn` has always given a
/// confined program a `stderr` pipe, and nothing on this path ever read it: a
/// pipe whose reader never empties it blocks the writer on a full buffer, which
/// is a server that stops mid-indexing and looks exactly like one that hung.
/// Reading it continuously and keeping only the end is what makes the pipe safe
/// to have at all.
fn keep_last_words(stderr: Option<std::process::ChildStderr>) -> Arc<Mutex<LastWords>> {
    let kept = Arc::new(Mutex::new(LastWords::default()));
    let Some(mut stderr) = stderr else {
        // No pipe is not "the pipe has not closed yet". Saying so here is what
        // stops `epitaph` from spending its grace waiting for an end that
        // cannot come.
        held(&kept).closed = true;
        return kept;
    };
    let into = Arc::clone(&kept);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 1024];
        loop {
            match stderr.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => held(&into).push(&chunk[..read]),
            }
        }
        held(&into).closed = true;
    });
    kept
}

/// The tail, whoever poisoned the lock.
///
/// A panicking drain thread must not turn a diagnosis into a second panic: the
/// whole reason this exists is to be readable at the worst moment.
fn held(kept: &Arc<Mutex<LastWords>>) -> std::sync::MutexGuard<'_, LastWords> {
    kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The name of a signal, for a number nobody should have to look up.
///
/// Spelled here rather than taken from a crate: this crate has no `libc`
/// dependency, and taking one on to name thirty constants would be a build-time
/// dependency for a string. The numbers are Linux's on x86-64 and aarch64,
/// which is what Thalyx runs on. `SIGSYS` is the one this was written for — a
/// process killed by the seccomp filter dies of it, and 31 on its own tells the
/// person reading nothing.
fn signal_name(number: i32) -> &'static str {
    match number {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        4 => "SIGILL",
        5 => "SIGTRAP",
        6 => "SIGABRT",
        7 => "SIGBUS",
        8 => "SIGFPE",
        9 => "SIGKILL",
        10 => "SIGUSR1",
        11 => "SIGSEGV",
        12 => "SIGUSR2",
        13 => "SIGPIPE",
        14 => "SIGALRM",
        15 => "SIGTERM",
        24 => "SIGXCPU",
        25 => "SIGXFSZ",
        31 => "SIGSYS",
        _ => "unnamed here",
    }
}

/// How long to wait for the server to finish its first indexing pass.
///
/// Measured rather than guessed: this workspace's twenty-eight crates take
/// about 25 seconds on the container, of which 15 are building proc-macro and
/// build-script dependencies. Five times that is the ceiling, and reaching it
/// is reported as a timeout rather than as an empty answer.
pub const READY_CEILING: Duration = Duration::from_secs(150);

/// How long any single question may take once the server is ready.
pub const ANSWER_CEILING: Duration = Duration::from_secs(30);

/// LSP's `ContentModified`: the document changed while the answer was being
/// computed, and the specified remedy is to ask again.
const CONTENT_MODIFIED: i64 = -32801;

/// How many times to ask again before calling it a refusal. Three, because the
/// thing that provokes it is the client's own `didOpen` settling, and a fourth
/// would be waiting on something else.
const RETRIES: u32 = 3;

/// How long to let the server settle before asking again.
const SETTLE: Duration = Duration::from_millis(250);

/// Where a name is, in the file the server named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spot {
    pub path: PathBuf,
    /// Zero-based, as LSP counts. Turned into the one-based line every other
    /// Thalyx answer uses at the surface and nowhere else, so there is exactly
    /// one place that can get it wrong.
    pub line: u32,
    pub character: u32,
    pub end_line: u32,
    pub end_character: u32,
}

/// A symbol as the server describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// `struct`, `function`, `method`, … — the LSP kind, spelled.
    pub kind: &'static str,
    pub at: Spot,
    /// The enclosing name, when the server gave one.
    pub container: Option<String>,
}

/// One file's worth of a rename, as ranges to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEdit {
    pub path: PathBuf,
    /// Ranges and their replacements, in the order the server gave them.
    pub edits: Vec<(Spot, String)>,
}

/// Whether the server ever became able to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ready {
    /// It finished its first pass, in this long.
    Indexed(Duration),
    /// It did not, within [`READY_CEILING`]. Every answer after this is
    /// suspect and the caller is told so rather than shown an empty list.
    TimedOut,
}

/// A running rust-analyzer, and the conversation with it.
pub struct Analyzer {
    child: Child,
    /// How the confinement around it is taken down. See [`Started::release`].
    release: Option<Box<dyn FnOnce() + Send>>,
    /// One phrase saying what started it.
    how: String,
    /// Whether Thalyx's confinement stands behind it.
    confined: bool,
    stdin: ChildStdin,
    /// The tail of what it wrote to `stderr`, drained continuously and read
    /// only when it dies. See [`Analyzer::epitaph`].
    noise: Arc<Mutex<LastWords>>,
    incoming: Receiver<Value>,
    next_id: i64,
    root: PathBuf,
    opened: std::collections::HashSet<PathBuf>,
    /// What the server said it counts columns in. LSP's default is UTF-16 code
    /// units, which is not what a byte offset is; a client that assumed bytes
    /// would be right on ASCII and wrong on the first accented identifier, and
    /// this codebase is written by somebody who types Spanish.
    utf8_columns: bool,
    pub ready: Ready,
}

impl Analyzer {
    /// Start one, rooted at the workspace, and wait until it has indexed.
    ///
    /// `build_into` is where its Cargo is told to put build output. Not a
    /// tidiness preference: rust-analyzer runs `cargo metadata` and builds
    /// build scripts, and with no `CARGO_TARGET_DIR` that lands **inside the
    /// workspace** — which means a snapshot taken around a transaction
    /// contains a build tree, a rollback destroys the build cache, and a run
    /// that changed two files reports twenty-nine. It was found by a test
    /// asserting the count.
    pub fn start(
        root: &Path,
        binary: &Path,
        build_into: Option<&Path>,
        readable: &[PathBuf],
        environment: &[(String, String)],
        spawner: &dyn Spawn,
    ) -> Result<Self> {
        // The loader path the binary's own `RUNPATH` cannot reach from where it
        // is executed. See [`crate::toolchain::loader_path`]: confined, this
        // server runs as `/module/rust-analyzer`, so its `$ORIGIN/../lib` is
        // `/lib`, `librustc_driver-<hash>.so` is not there, and the process
        // exits 127 before its first byte of LSP — with no `SIGSYS` and nothing
        // in `ausearch`, which reads exactly like the filter killing it.
        //
        // Added here, where the binary's real directory is still known, rather
        // than by each spawner: two spawners assembling it is two answers to
        // where a toolchain keeps its libraries. A caller that named the
        // variable itself is left alone — an explicit value is somebody's
        // decision and this is a default.
        let mut environment = environment.to_vec();
        if !environment
            .iter()
            .any(|(name, _)| name == crate::toolchain::LOADER_PATH_VARIABLE)
            && let Some(lib) = crate::toolchain::loader_path(binary)
        {
            environment.push((
                crate::toolchain::LOADER_PATH_VARIABLE.to_string(),
                lib.display().to_string(),
            ));
        }

        let Started {
            mut child,
            release,
            how,
            confined,
        } = spawner.start(Launching {
            program: binary,
            root,
            build_into,
            readable,
            environment: &environment,
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            RustError::NoAnalyzer("rust-analyzer was started without a stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            RustError::NoAnalyzer("rust-analyzer was started without a stdout".to_string())
        })?;
        // Started before the first byte is sent, because the death this is for
        // happens during `initialize` and what it said is on its way out then.
        let noise = keep_last_words(child.stderr.take());

        // A reader thread and a channel rather than reading in line: every read
        // here needs a deadline, and a blocking read on a pipe has none. A
        // server that wedges must become a refusal, not a machine that stops.
        let (sender, incoming) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Some(message) = read_message(&mut reader) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });

        let mut analyzer = Self {
            child,
            release,
            how,
            confined,
            stdin,
            noise,
            incoming,
            next_id: 1,
            root: root.to_path_buf(),
            opened: std::collections::HashSet::new(),
            utf8_columns: false,
            ready: Ready::TimedOut,
        };

        let initialized = analyzer.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": uri_of(root),
                // Asked for explicitly. The server answers with what it will
                // actually use, and that answer is read below rather than
                // assumed.
                "capabilities": {
                    "general": {"positionEncodings": ["utf-8", "utf-16"]},
                    // Without this the server sends no progress at all, and a
                    // client waiting for indexing to finish waits forever. It
                    // is the whole reason the first attempt at this hung.
                    "window": {"workDoneProgress": true},
                    "workspace": {"workspaceEdit": {"documentChanges": true}},
                    "textDocument": {
                        // `true`, and the difference is a position rather
                        // than a shape. Flat `SymbolInformation` carries one
                        // range per entry and rust-analyzer fills it with the
                        // **whole item**, doc comment included — so the
                        // outline of `dev/rust-corpus` placed
                        // `LanternRegistry` at 3:1, where the `///` starts,
                        // and a caller that renamed at the place the outline
                        // gave it was pointing at a comment. The hierarchical
                        // answer carries `selectionRange`, which is the
                        // identifier: 8:12, the place it really is. Captured
                        // in `tests/samples/document-symbol-hierarchical.json`
                        // — rule 6, because the first version of this was
                        // written against a fixture somebody invented.
                        "documentSymbol": {"hierarchicalDocumentSymbolSupport": true},
                        "rename": {"prepareSupport": true}
                    }
                }
            }),
            READY_CEILING,
        )?;
        analyzer.utf8_columns = initialized
            .pointer("/capabilities/positionEncoding")
            .and_then(Value::as_str)
            == Some("utf-8");
        analyzer.notify("initialized", json!({}))?;
        analyzer.ready = analyzer.wait_until_indexed();
        Ok(analyzer)
    }

    /// Wait for the server's first indexing pass to end.
    ///
    /// The end of `rustAnalyzer/cachePriming` and not a sleep: a sleep long
    /// enough for a big workspace is wasted on a small one, and a sleep short
    /// enough for a small one answers a big one's questions with silence — the
    /// exact shape of a test that measures the harness instead of the system.
    fn wait_until_indexed(&mut self) -> Ready {
        let started = Instant::now();
        while started.elapsed() < READY_CEILING {
            let Ok(message) = self
                .incoming
                .recv_timeout(READY_CEILING.saturating_sub(started.elapsed()))
            else {
                return Ready::TimedOut;
            };
            if message.get("method").and_then(Value::as_str) == Some("$/progress")
                && message
                    .pointer("/params/value/kind")
                    .and_then(Value::as_str)
                    == Some("end")
                && message.pointer("/params/token").and_then(Value::as_str)
                    == Some("rustAnalyzer/cachePriming")
            {
                return Ready::Indexed(started.elapsed());
            }
        }
        Ready::TimedOut
    }

    /// Tell the server about a file before asking about it.
    ///
    /// Sent from the bytes on disk rather than as a bare "this file exists":
    /// the server answers about the document it holds, and a client that let
    /// the two drift would get positions for a file nobody has.
    pub fn open(&mut self, file: &Path) -> Result<()> {
        if self.opened.contains(file) {
            return Ok(());
        }
        let text = std::fs::read_to_string(file)
            .map_err(|error| RustError::Unreadable(format!("{}: {error}", file.display())))?;
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": uri_of(file), "languageId": "rust", "version": 1, "text": text
            }}),
        )?;
        self.opened.insert(file.to_path_buf());
        Ok(())
    }

    /// Where the name at this position is defined.
    ///
    /// **This is the answer the scanner cannot give.** Asked at `Keys` in
    /// `fn boot() -> Keys`, it says `keystore.rs`, because `Keys` is a binding
    /// introduced by `use … as` and following a binding is compiler work.
    pub fn definition(&mut self, file: &Path, line: u32, character: u32) -> Result<Vec<Spot>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/definition",
            json!({"textDocument": {"uri": uri_of(file)},
                   "position": {"line": line, "character": character}}),
            ANSWER_CEILING,
        )?;
        Ok(spots(&answer))
    }

    /// Every place the name at this position is used, the declaration included.
    pub fn references(&mut self, file: &Path, line: u32, character: u32) -> Result<Vec<Spot>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/references",
            json!({"textDocument": {"uri": uri_of(file)},
                   "position": {"line": line, "character": character},
                   "context": {"includeDeclaration": true}}),
            ANSWER_CEILING,
        )?;
        Ok(spots(&answer))
    }

    /// The one-line signature the server shows on hover, without the prose.
    pub fn signature(&mut self, file: &Path, line: u32, character: u32) -> Result<Option<String>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/hover",
            json!({"textDocument": {"uri": uri_of(file)},
                   "position": {"line": line, "character": character}}),
            ANSWER_CEILING,
        )?;
        let text = answer
            .pointer("/contents/value")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // The hover is a document: a path, a signature, then documentation.
        // What a repo map wants is the declaration, so the first line that
        // looks like one is taken and the rest is left in the machine.
        let signature = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && *line != "```rust" && *line != "```")
            .find(|line| {
                [
                    "pub ", "fn ", "struct ", "enum ", "trait ", "type ", "const ", "static ",
                    "impl ",
                ]
                .iter()
                .any(|start| line.starts_with(start))
            })
            .map(str::to_string);
        Ok(signature)
    }

    /// Everything declared in one file.
    pub fn symbols_in(&mut self, file: &Path) -> Result<Vec<Symbol>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": uri_of(file)}}),
            ANSWER_CEILING,
        )?;
        Ok(symbols(&answer, Some(file)))
    }

    /// Everything in the workspace whose name matches, as the server matches.
    pub fn symbols_named(&mut self, query: &str) -> Result<Vec<Symbol>> {
        let answer = self.request("workspace/symbol", json!({"query": query}), ANSWER_CEILING)?;
        Ok(symbols(&answer, None))
    }

    /// What a rename would change, described rather than done.
    ///
    /// Every path is checked against the workspace root **here**, and a URI
    /// pointing anywhere else makes the whole answer a refusal rather than a
    /// filtered list. A partial rename is worse than none: it compiles nowhere
    /// and looks like it was applied.
    pub fn rename(
        &mut self,
        file: &Path,
        line: u32,
        character: u32,
        to: &str,
    ) -> Result<Vec<FileEdit>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/rename",
            json!({"textDocument": {"uri": uri_of(file)},
                   "position": {"line": line, "character": character},
                   "newName": to}),
            ANSWER_CEILING,
        )?;

        let mut changes: Vec<FileEdit> = Vec::new();
        if let Some(documents) = answer.get("documentChanges").and_then(Value::as_array) {
            for document in documents {
                let Some(uri) = document
                    .pointer("/textDocument/uri")
                    .and_then(Value::as_str)
                else {
                    continue;
                };
                changes.push(self.file_edit(uri, document.get("edits"))?);
            }
        } else if let Some(map) = answer.get("changes").and_then(Value::as_object) {
            for (uri, edits) in map {
                changes.push(self.file_edit(uri, Some(edits))?);
            }
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }

    fn file_edit(&self, uri: &str, edits: Option<&Value>) -> Result<FileEdit> {
        let path = path_of(uri).ok_or_else(|| RustError::Outside(uri.to_string()))?;
        if !path.starts_with(&self.root) {
            return Err(RustError::Outside(path.display().to_string()));
        }
        let mut ranges = Vec::new();
        for edit in edits.and_then(Value::as_array).into_iter().flatten() {
            let Some(spot) = spot_of(&path, edit.get("range")) else {
                continue;
            };
            ranges.push((
                spot,
                edit.get("newText")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ));
        }
        Ok(FileEdit {
            path,
            edits: ranges,
        })
    }

    /// Whether the server counts columns in bytes. When it does not, a caller
    /// turning a position into a byte offset has to convert — see
    /// `crate::edits`.
    pub fn utf8_columns(&self) -> bool {
        self.utf8_columns
    }

    /// Ask, and ask again when the server says the file moved under it.
    ///
    /// LSP's `ContentModified` (-32801) is defined as *retry*: the server was
    /// answering about a document that changed while it was answering, which
    /// happens on the very next request after a `didOpen` because the open is
    /// what changed it. Treating it as a refusal made two of these tests fail
    /// with "content modified" and would have made a frontier model believe a
    /// symbol has no references — rule 5, where the instrument is the protocol.
    fn request(&mut self, method: &str, params: Value, ceiling: Duration) -> Result<Value> {
        let mut left = RETRIES;
        loop {
            match self.request_once(method, params.clone(), ceiling) {
                Err(RustError::Moved(_)) if left > 0 => {
                    left -= 1;
                    std::thread::sleep(SETTLE);
                }
                other => return other,
            }
        }
    }

    /// What became of the server, said at the moment its channel closes.
    ///
    /// Until 2026-08-30 a server that died during `initialize` produced exactly
    /// one sentence — *«the server stopped»* — which is the shape rule 10 is
    /// about: it reports that the reading failed and nothing about what
    /// happened. A process killed by the seccomp filter, a process that could
    /// not find its toolchain and a process that panicked all closed a pipe,
    /// and on Fedora on 2026-08-30 they were indistinguishable events. Stage 58
    /// could say `analyzer_starts=1` and then that the server stopped, with
    /// `ausearch -m SECCOMP` showing nothing, and no way to tell which of the
    /// three it had been.
    ///
    /// So the two things the kernel already knows get read: how the process
    /// ended — a status, or the signal that killed it — and the last of what it
    /// wrote on the way out.
    ///
    /// Bounded on both sides. The wait for the status is [`EPITAPH_GRACE`] and
    /// then it says the process was still running, rather than becoming a
    /// diagnosis that hangs; the stderr quoted is the last [`STDERR_KEPT`]
    /// bytes, and it says so when there was more.
    fn epitaph(&mut self) -> String {
        let deadline = Instant::now() + EPITAPH_GRACE;
        let mut ended: Option<std::result::Result<std::process::ExitStatus, String>> = None;
        loop {
            if ended.is_none() {
                match self.child.try_wait() {
                    Ok(Some(status)) => ended = Some(Ok(status)),
                    Ok(None) => {}
                    // Rule 10: a failure to read is not a failure to exist, and
                    // "could not be asked" is not "was still running".
                    Err(error) => ended = Some(Err(error.to_string())),
                }
            }
            if (ended.is_some() && held(&self.noise).closed) || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let how = match ended {
            Some(Ok(status)) => {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = status.signal() {
                    format!(
                        "the process was killed by signal {signal} ({})",
                        signal_name(signal)
                    )
                } else if let Some(code) = status.code() {
                    format!("the process exited with status {code}")
                } else {
                    format!("the process ended as {status}")
                }
            }
            Some(Err(why)) => format!("how the process ended could not be read: {why}"),
            None => format!("the process was still running {EPITAPH_GRACE:?} later"),
        };
        format!("{how}; {}", held(&self.noise).spoken())
    }

    fn request_once(&mut self, method: &str, params: Value, ceiling: Duration) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + ceiling;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let message = match self.incoming.recv_timeout(left) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(RustError::Silent(format!("`{method}` after {ceiling:?}")));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    // The one place the death is visible, and until this said
                    // more than "it stopped" there was nothing to diagnose it
                    // with. See [`Analyzer::epitaph`].
                    let epitaph = self.epitaph();
                    return Err(RustError::Silent(format!(
                        "`{method}`: the server stopped — {epitaph}"
                    )));
                }
            };
            if message.get("id").and_then(Value::as_i64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                let why = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("no reason given");
                if error.get("code").and_then(Value::as_i64) == Some(CONTENT_MODIFIED) {
                    return Err(RustError::Moved(format!("`{method}`: {why}")));
                }
                return Err(RustError::Refused(format!("`{method}`: {why}")));
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn send(&mut self, message: Value) -> Result<()> {
        let body = serde_json::to_vec(&message).map_err(|error| {
            RustError::Silent(format!("a request could not be written: {error}"))
        })?;
        let written = self
            .stdin
            .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
            .and_then(|()| self.stdin.write_all(&body))
            .and_then(|()| self.stdin.flush());
        // The same death seen from the writing side, and it is a real race
        // rather than a second case: a server that dies before the request is
        // written fails here with `EPIPE`, and one that dies just after fails
        // as a disconnected channel above. Which of the two happens is timing,
        // so both carry the epitaph or the diagnosis is a coin toss.
        match written {
            Ok(()) => Ok(()),
            Err(error) => {
                let epitaph = self.epitaph();
                Err(RustError::Silent(format!(
                    "the server stopped listening: {error} — {epitaph}"
                )))
            }
        }
    }
}

impl Drop for Analyzer {
    /// Killed rather than asked to shut down.
    ///
    /// A polite `shutdown`/`exit` would need a reply, and the one case where
    /// this matters most is the case where the server is not replying. It holds
    /// no state anybody needs: everything it learned is either in the answer
    /// already given or being recomputed next time.
    /// Kill the server, then take down whatever was confining it.
    ///
    /// In that order and both, and the second half is the one that was not
    /// here before there was anything to take down. A `release` skipped leaves
    /// a cgroup and a kernel policy behind — and an entry left in the map after
    /// its directory is gone becomes the policy of whatever cgroup the kernel
    /// gives that inode to next.
    ///
    /// Killing the one process Thalyx holds is enough for the *tree* under a
    /// profile with a pid namespace: the kernel reaps a namespace when its init
    /// dies, and every `cargo`, `rustc` and build script the server started is
    /// inside it. `release` kills the cgroup as well, which covers the window
    /// before the re-exec that becomes that init.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

impl Analyzer {
    /// One phrase saying what started this server.
    pub fn how(&self) -> &str {
        &self.how
    }

    /// Whether Thalyx's confinement stands behind it.
    pub fn confined(&self) -> bool {
        self.confined
    }
}

/// Read one LSP message, or `None` when the pipe closed.
fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            length = value.trim().parse().ok();
        }
    }
    let length = length?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn spots(answer: &Value) -> Vec<Spot> {
    let mut found = Vec::new();
    let listed = match answer {
        Value::Array(listed) => listed.clone(),
        Value::Object(_) => vec![answer.clone()],
        _ => return found,
    };
    for entry in listed {
        // `Location`, `LocationLink` and `SymbolInformation` all appear here
        // depending on the request; each names its range differently and all
        // three are read rather than one being assumed.
        let uri = entry
            .get("uri")
            .or_else(|| entry.get("targetUri"))
            .or_else(|| entry.pointer("/location/uri"))
            .and_then(Value::as_str);
        let range = entry
            .get("range")
            .or_else(|| entry.get("targetSelectionRange"))
            .or_else(|| entry.get("targetRange"))
            .or_else(|| entry.pointer("/location/range"));
        let (Some(uri), Some(range)) = (uri, range) else {
            continue;
        };
        let Some(path) = path_of(uri) else { continue };
        if let Some(spot) = spot_of(&path, Some(range)) {
            found.push(spot);
        }
    }
    found
}

fn spot_of(path: &Path, range: Option<&Value>) -> Option<Spot> {
    let range = range?;
    Some(Spot {
        path: path.to_path_buf(),
        line: range.pointer("/start/line")?.as_u64()? as u32,
        character: range.pointer("/start/character")?.as_u64()? as u32,
        end_line: range.pointer("/end/line")?.as_u64()? as u32,
        end_character: range.pointer("/end/character")?.as_u64()? as u32,
    })
}

/// Both shapes the protocol allows for a list of symbols, flattened.
///
/// `workspace/symbol` answers with flat `SymbolInformation`, which carries a
/// `location` and a `containerName`. `textDocument/documentSymbol` answers with
/// a **tree** of `DocumentSymbol`, which carries neither and instead nests its
/// members under `children`. One reader for both, because a caller asking what
/// is declared in a file and a caller asking who is called `Config` are asking
/// the same question of the same server and must not get two different notions
/// of where a symbol is.
///
/// ## Which range, and the defect that decided it
///
/// `selectionRange` before `range`, and that order is the whole fix. A
/// `DocumentSymbol`'s `range` is the item — for a documented struct it starts
/// at the first `///`, which is how a booted Thalyx reported `LanternRegistry`
/// at 3:1 for a struct whose name is at 8:12. `selectionRange` is the
/// identifier. Flat entries have no `selectionRange` at all and their
/// `location.range` already is the identifier, so they are read first and are
/// unaffected.
fn symbols(answer: &Value, file: Option<&Path>) -> Vec<Symbol> {
    let Value::Array(listed) = answer else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in listed {
        gather(entry, file, None, &mut found);
    }
    found
}

/// One entry and everything nested under it.
///
/// `inside` is the name of the item this was found in, used only for the
/// hierarchical shape: a flat entry brings its own `containerName` and a tree
/// entry has none, so without this a method would come back with no idea which
/// `impl` it belongs to and an outline of a file would list six `new`s.
fn gather(entry: &Value, file: Option<&Path>, inside: Option<&str>, found: &mut Vec<Symbol>) {
    let Some(name) = entry.get("name").and_then(Value::as_str) else {
        return;
    };
    let path = entry
        .pointer("/location/uri")
        .and_then(Value::as_str)
        .and_then(path_of)
        .or_else(|| file.map(Path::to_path_buf));
    let Some(path) = path else { return };
    let range = entry
        .pointer("/location/range")
        .or_else(|| entry.get("selectionRange"))
        .or_else(|| entry.get("range"));
    if let Some(at) = spot_of(&path, range) {
        found.push(Symbol {
            name: name.to_string(),
            kind: kind_of(entry.get("kind").and_then(Value::as_u64).unwrap_or(0)),
            at,
            container: entry
                .get("containerName")
                .and_then(Value::as_str)
                .filter(|container| !container.is_empty())
                .map(str::to_string)
                .or_else(|| inside.map(str::to_string)),
        });
    }
    // Descended even when this entry had no usable range: a member whose
    // parent the reader could not place is still a member somebody asked for.
    for child in entry
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        gather(child, file, Some(name), found);
    }
}

/// The LSP `SymbolKind` numbers, spelled.
///
/// Written out rather than derived: these are wire constants, and a table that
/// guessed would rename a struct into a class on the day the enum grew.
fn kind_of(kind: u64) -> &'static str {
    match kind {
        2 => "module",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "trait",
        12 => "function",
        13 => "variable",
        14 => "constant",
        22 => "variant",
        23 => "struct",
        26 => "type parameter",
        _ => "symbol",
    }
}

/// `file://` for a path. Percent-encoded, because a workspace under a folder
/// with a space in its name is a workspace, and a URI that did not say so would
/// send the server looking for a file that does not exist.
pub fn uri_of(path: &Path) -> String {
    let mut encoded = String::from("file://");
    for byte in path.to_string_lossy().as_bytes() {
        match byte {
            b'/' | b'-' | b'_' | b'.' | b'~' => encoded.push(*byte as char),
            b if b.is_ascii_alphanumeric() => encoded.push(*b as char),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

/// The path a `file://` URI names, or `None` for a URI that names no file.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                decoded.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    Some(PathBuf::from(String::from_utf8(decoded).ok()?))
}

/// The rust-analyzer this machine has, or nothing.
///
/// One line, because the search itself belongs to [`crate::toolchain`] and
/// there used to be three of them that disagreed. Kept as a function here
/// because this is where a reader of the LSP client looks for it.
pub fn find() -> Option<PathBuf> {
    crate::toolchain::rust_analyzer().path.clone()
}

/// Why there is no rust-analyzer, naming every place that was looked at.
pub fn why_no_analyzer() -> String {
    crate::toolchain::rust_analyzer().why_not(
        "rust-analyzer",
        "Add it with: rustup component add rust-analyzer, or name one with \
         THALYX_RUST_ANALYZER",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// rust-analyzer's own answer for `dev/rust-corpus/lantern/src/lib.rs`,
    /// captured verbatim on 2026-08-31.
    ///
    /// Rule 6, and it is the rule this defect was found under: the reader was
    /// written against nothing, the outline reported `LanternRegistry` at 3:1,
    /// and the sentence «the outline is probably taking the wrong range» could
    /// not be settled by anything in the repository. A file the server wrote
    /// settles it.
    const DOCUMENT_SYMBOL: &str =
        include_str!("../tests/samples/document-symbol-hierarchical.json");

    #[test]
    fn an_outline_places_a_documented_struct_at_its_name_and_not_at_its_comment() {
        let answer: Value = serde_json::from_str(DOCUMENT_SYMBOL).expect("the captured answer");
        let file = Path::new("/w/lantern/src/lib.rs");
        let found = symbols(&answer, Some(file));

        let registry = found
            .iter()
            .find(|symbol| symbol.name == "LanternRegistry")
            .expect("the struct the corpus is about");
        // Zero-based, as the wire is. The item's own range starts at line 2,
        // which is the first `///` — that is the position a booted Thalyx
        // reported as 3:1 and then could not rename at.
        assert_eq!(
            (registry.at.line, registry.at.character),
            (7, 11),
            "the outline is pointing at the doc comment again: {registry:?}"
        );
        assert_eq!(registry.kind, "struct");
        assert_eq!(registry.at.path, file);
    }

    #[test]
    fn the_members_nested_under_an_item_are_in_the_outline_too() {
        // The hierarchical answer is a tree. A reader that took only its top
        // level would have traded a wrong position for a missing half of the
        // file — every method, every field, gone, and nothing would have said
        // so because an outline with fewer entries still looks like an
        // outline.
        let answer: Value = serde_json::from_str(DOCUMENT_SYMBOL).expect("the captured answer");
        let found = symbols(&answer, Some(Path::new("/w/lantern/src/lib.rs")));
        let named: Vec<&str> = found.iter().map(|symbol| symbol.name.as_str()).collect();
        for member in ["lit", "new", "light", "default"] {
            assert!(named.contains(&member), "{member} is not in {named:?}");
        }
        let light = found
            .iter()
            .find(|symbol| symbol.name == "light")
            .expect("a method");
        assert_eq!(
            light.container.as_deref(),
            Some("impl LanternRegistry"),
            "a method came back without the item it belongs to: {light:?}"
        );
    }

    #[test]
    fn a_flat_answer_still_reads_out_of_its_location() {
        // `workspace/symbol` answers in the other shape, and it is the shape
        // every resolution goes through. A reader that started preferring
        // `selectionRange` and stopped reading `location` would have moved the
        // defect rather than fixed it.
        let answer = json!([{
            "name": "LanternRegistry",
            "kind": 23,
            "containerName": "lantern",
            "location": {
                "uri": "file:///w/lantern/src/lib.rs",
                "range": {"start": {"line": 7, "character": 11},
                          "end": {"line": 7, "character": 26}}
            }
        }]);
        let found = symbols(&answer, None);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].at.line, found[0].at.character), (7, 11));
        assert_eq!(found[0].container.as_deref(), Some("lantern"));
        assert_eq!(found[0].at.path, Path::new("/w/lantern/src/lib.rs"));
    }
}
