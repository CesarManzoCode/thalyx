//! The inference engine as an installed module that stays alive.
//!
//! Cesar's decree of 2026-08-28: **the engine is the first real module**, and
//! it is not part of Thalyx. `vault/02-Arquitectura/Gamas-de-Modelo.md` already
//! put llama.cpp outside the process and `vault/11-Seguridad/Modelo-de-Amenaza.md`
//! puts the model outside the TCB; this is what makes both of those something
//! the operating system enforces rather than something the design asserts.
//!
//! ## Why it is resident, since the same day
//!
//! The first shape of this ran `llama-completion` once per sentence, and
//! `llama-completion` is one-shot by construction: it loads a GGUF, answers,
//! and dies. So the second sentence read two gigabytes off disk again, rebuilt
//! a context again, and warmed nothing — which is most of what a local model
//! costs, spent on work the first sentence had already done, every time.
//!
//! What replaces it is not a second launcher and not a daemon. It is the same
//! module, started by `thalyx_core::run::start` with the same cgroup, the same
//! kernel policy, the same seccomp filter, the same pivoted root, the same uid
//! and the same channel — and *kept*. The handle that owns all of that lives in
//! [`RESIDENT`] until Thalyx exits or the engine dies.
//!
//! ## Why this file is in the CLI and not in `thalyx-agent`
//!
//! `thalyx_agent::llama::Engine` is a seam with two sides. The agent crate is
//! where the model's output is parsed, and it is deliberately not a crate that
//! can start a confined process — everything it knows arrived from an
//! untrusted model. Running one is the store's business and the sandbox's, so
//! the implementation lives where those are, and the agent only ever sees a
//! call going out and bytes coming back.
//!
//! ## The two paths it needs, and why they are absolute constants
//!
//! A confined module sees only what its manifest was granted. Two directories
//! matter:
//!
//! - [`MODELS_DIR`], where the GGUF lives. Read.
//! - [`RUN_DIR`], where Thalyx writes the prompt and the grammar for one
//!   inference. Read.
//!
//! They are spelled here and in the manifest that `image/Makefile` writes, and
//! they are absolute because a grant is a path inside the machine: the module's
//! root filesystem binds granted paths at the names they already have. Writing
//! a prompt anywhere else produces a run where the engine is handed a path to a
//! file that does not exist inside its root — which comes back as "the tool
//! never completed the prompt" and sends whoever reads it to audit llama.cpp.
//!
//! Keeping the two files on disk is also what makes residency inspectable. The
//! engine is sent **paths**, not text, so `--keep-prompt` still leaves the
//! exact bytes of an inference where a person can read them, and the request on
//! the wire stays small enough to print.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thalyx_agent::llama::{Engine, EngineCall, EngineRun, LlamaError};
use thalyx_core::Store;
use thalyx_journal::Origin;
use thalyx_permd::KernelStore;

/// The id the image's engine module is packed under.
pub const ENGINE_MODULE_ID: &str = "dev.thalyx.engine";

/// Where the engine's data lives inside the machine — the `modules` subvolume.
pub const ENGINE_DATA: &str = "/opt/thalyx/data/engine";

/// Moves [`ENGINE_DATA`] somewhere else, for a machine that is not Thalyx.
///
/// `dev/verify.sh` is the caller. It runs as root on Cesar's Fedora, where
/// `/opt/thalyx` is a real store belonging to a real installation, and a stage
/// that made directories under it would be rule 11 — a test that writes
/// something machine-global has changed the machine it was measuring. Inside
/// Thalyx nothing sets it and the constant stands.
pub const DATA_ENV: &str = "THALYX_ENGINE_DATA";

/// How much context the resident engine is given, in tokens.
///
/// Fixed rather than derived, and modest on purpose: the KV cache is charged to
/// the module's cgroup along with the mmapped weights, and the smallest tier is
/// granted 4 GiB for both. A prompt longer than this is refused by the engine
/// with the number in it rather than silently cut — a prompt quietly truncated
/// loses the marker it ends with, and that arrives as "the tool never read the
/// prompt", which is a diagnosis of the wrong component.
pub const CONTEXT_ENV: &str = "THALYX_ENGINE_CTX";
const DEFAULT_CONTEXT: u32 = 4096;

/// The most bytes one answer may be. `thalyx-agent` refuses past 64 KiB
/// anyway; this is the frame-level ceiling, so a length that arrived corrupt
/// cannot make Thalyx allocate before the agent gets to refuse it.
const MAX_BODY: u32 = 8 * 1024 * 1024;

fn data_root() -> PathBuf {
    match std::env::var(DATA_ENV) {
        Ok(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(ENGINE_DATA),
    }
}

/// The GGUF the agent runs. A human puts it there; Thalyx never fetches it.
pub fn models_dir() -> PathBuf {
    data_root().join("models")
}

/// Where one inference's prompt and grammar are written.
pub fn run_dir() -> PathBuf {
    data_root().join("run")
}

fn context_size() -> u32 {
    std::env::var(CONTEXT_ENV)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|size| *size > 0)
        .unwrap_or(DEFAULT_CONTEXT)
}

// ───────────────────────────────────────────────────────────────── the wire
//
// Little-endian, length-prefixed, and no text framing anywhere. A completion is
// arbitrary bytes chosen by an untrusted model, so any delimiter it could type
// is a delimiter it could forge. `engine/thalyx-engine.cpp` is the other side
// of every one of these; the two are one protocol and the frame names are the
// only thing tying them together, so they are spelled the same in both files.

const READY: &[u8; 4] = b"THR1";
const REQUEST: &[u8; 4] = b"THQ1";
const RESPONSE: &[u8; 4] = b"THA1";

/// One request, as bytes on the pipe.
///
/// Paths rather than the prompt itself, which is the cheap half of the trade
/// this whole file is: the files are already written where the module was
/// granted to read them, so the request is under a kilobyte and the inference
/// stays inspectable on disk.
fn request_frame(call: &EngineCall<'_>) -> Vec<u8> {
    let mut frame = Vec::with_capacity(64);
    frame.extend_from_slice(REQUEST);
    frame.extend_from_slice(&call.predict.to_le_bytes());
    frame.extend_from_slice(&call.seed.to_le_bytes());
    let mut field = |bytes: &[u8]| {
        frame.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        frame.extend_from_slice(bytes);
    };
    field(call.prompt_file.as_os_str().as_encoded_bytes());
    field(
        call.grammar_file
            .map(|path| path.as_os_str().as_encoded_bytes())
            .unwrap_or_default(),
    );
    frame
}

fn read_exactly<R: Read>(from: &mut R, into: &mut [u8]) -> std::io::Result<()> {
    from.read_exact(into)
}

fn read_u32<R: Read>(from: &mut R) -> std::io::Result<u32> {
    let mut bytes = [0u8; 4];
    read_exactly(from, &mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64<R: Read>(from: &mut R) -> std::io::Result<u64> {
    let mut bytes = [0u8; 8];
    read_exactly(from, &mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

/// What the engine says once, when the weights are in.
#[derive(Debug, Clone, Copy)]
struct Ready {
    load: Duration,
    /// The engine's own pid, as it sees itself. Inside a pid namespace that is
    /// not the number Thalyx holds — both are reported, and neither is guessed
    /// from the other.
    engine_pid: u32,
    threads: u32,
    context: u32,
}

fn read_ready<R: Read>(from: &mut R) -> std::io::Result<Ready> {
    let mut magic = [0u8; 4];
    read_exactly(from, &mut magic)?;
    if &magic != READY {
        return Err(std::io::Error::other(
            "the engine's first frame was not the one that says the weights are in",
        ));
    }
    Ok(Ready {
        load: Duration::from_millis(read_u64(from)?),
        engine_pid: read_u32(from)?,
        threads: read_u32(from)?,
        context: read_u32(from)?,
    })
}

/// One answer, or one refusal with the reason in it.
#[derive(Debug)]
struct Answered {
    ok: bool,
    elapsed: Duration,
    body: Vec<u8>,
}

fn read_answer<R: Read>(from: &mut R) -> std::io::Result<Answered> {
    let mut magic = [0u8; 4];
    read_exactly(from, &mut magic)?;
    if &magic != RESPONSE {
        return Err(std::io::Error::other(
            "the engine sent something that is not an answer frame",
        ));
    }
    let mut status = [0u8; 1];
    read_exactly(from, &mut status)?;
    let elapsed = Duration::from_millis(read_u64(from)?);
    let length = read_u32(from)?;
    if length > MAX_BODY {
        return Err(std::io::Error::other(format!(
            "the engine said its answer is {length} bytes, which is past what Thalyx will read"
        )));
    }
    let mut body = vec![0u8; length as usize];
    read_exactly(from, &mut body)?;
    Ok(Answered {
        ok: status[0] == 0,
        elapsed,
        body,
    })
}

// ────────────────────────────────────────────────── the engine, while it lives

/// The engine, started, with its weights already loaded.
///
/// Everything holding it up is inside `module`: the process, the cgroup, the
/// policy in the kernel, the channel. Dropping this without
/// [`Resident::retire`] would leave all of them, so nothing drops it —
/// [`RESIDENT`] is the only owner and it always retires what it replaces.
struct Resident {
    module: thalyx_core::run::RunningModule,
    to: std::process::ChildStdin,
    /// Taken out for the length of one request so the read can be given a
    /// deadline on another thread, and put back after. See [`Resident::ask`].
    from: Option<std::process::ChildStdout>,
    /// What it was started for. A different module or different weights is a
    /// different engine, and reusing this one for them would answer with the
    /// wrong model and never say so.
    module_id: String,
    weights: PathBuf,
    ready: Ready,
    /// Thalyx's own pid for it, which is the one a person can see in `procesos`.
    pid: u32,
    answered: u64,
}

impl Resident {
    /// Send one request and wait for its answer, with a deadline.
    ///
    /// The read happens on a thread so the deadline is real: a pipe read has
    /// none, and an engine that stopped answering would otherwise hold this
    /// lock for as long as the machine is on. On a timeout the reader is left
    /// blocked and the caller kills the engine — which closes the pipe, which
    /// is what wakes the thread up to exit.
    fn ask(&mut self, call: &EngineCall<'_>) -> Result<Answered, std::io::Error> {
        self.to.write_all(&request_frame(call))?;
        self.to.flush()?;

        let mut pipe = self
            .from
            .take()
            .ok_or_else(|| std::io::Error::other("the engine's answer pipe was already taken"))?;
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let answered = read_answer(&mut pipe);
            let _ = sender.send((pipe, answered));
        });

        match receiver.recv_timeout(call.timeout) {
            Ok((pipe, answered)) => {
                self.from = Some(pipe);
                self.answered += 1;
                answered
            }
            Err(_) => Err(std::io::Error::other(format!(
                "the engine did not answer within {:?}",
                call.timeout
            ))),
        }
    }

    /// Kill it and take the cgroup and the policy out of the kernel.
    fn retire(self) {
        let policies = KernelStore::default_map();
        drop(self.to);
        self.module.shutdown(&policies);
    }
}

/// The one engine this process has, or none yet.
///
/// Process-wide rather than a field of [`ModuleEngine`] because the callers are
/// not one object: the graphical session prewarms it on a thread, a worker
/// thread asks it a question, and `thalyx agent plan` in the same process would
/// be a third. Two of those starting two engines would load the weights twice
/// and charge both to the same cgroup, which is the one failure this whole
/// change exists to remove.
static RESIDENT: Mutex<Option<Resident>> = Mutex::new(None);

/// Where the model is, for anything that draws or prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    /// Nothing has been asked of it yet.
    Cold,
    /// Somebody is loading the weights right now. The next request waits for
    /// *this* load rather than starting a second one — the lock sees to that.
    Loading,
    Ready {
        /// Thalyx's pid for the engine. The evidence that two sentences were
        /// answered by one process is this number twice.
        pid: u32,
        engine_pid: u32,
        load_ms: u64,
        threads: u32,
        context: u32,
        answered: u64,
    },
    /// It could not be started, in the words that say why.
    Failed(String),
}

static STATE: Mutex<ModelState> = Mutex::new(ModelState::Cold);

fn set_state(state: ModelState) {
    if let Ok(mut held) = STATE.lock() {
        *held = state;
    }
}

/// What the model is doing, for a panel or a status line.
pub fn model_state() -> ModelState {
    STATE
        .lock()
        .map(|held| held.clone())
        .unwrap_or(ModelState::Cold)
}

/// What the last inference cost, and whether it paid for the weights.
#[derive(Debug, Clone, Copy)]
pub struct LastInference {
    pub pid: u32,
    pub elapsed: Duration,
    /// Whether this request is the one that loaded the weights.
    pub cold: bool,
}

static LAST: Mutex<Option<LastInference>> = Mutex::new(None);

/// The cost of the most recent inference, for the line the screen prints.
///
/// The whole of the metrics Cesar asked for, and deliberately no more: what a
/// person needs is to be able to tell a cold answer from a warm one, and a
/// second number beside the same pid is enough to see it.
pub fn last_inference() -> Option<LastInference> {
    LAST.lock().ok().and_then(|held| *held)
}

/// What the engine cost, in one line, or nothing when there was no engine.
///
/// The evidence Cesar asked for and no more: **the same pid twice is two
/// sentences answered by one process**, and `frío`/`tibio` beside it is a load
/// of the weights next to no load at all. Printed by both surfaces from here,
/// rather than written twice, so the screen and the terminal cannot come to
/// disagree about what a warm answer looks like.
///
/// `None` when nothing has gone through a resident engine in this process —
/// the rules answered, or the machine has no model, or the engine is a program
/// on `PATH`. A line that appeared anyway would be a measurement of nothing.
pub fn cost_line() -> Option<String> {
    let last = last_inference()?;
    Some(format!(
        "motor {} ▪ {} ▪ {:.1} s",
        last.pid,
        if last.cold { "frío" } else { "tibio" },
        last.elapsed.as_secs_f32()
    ))
}

/// The engine, run the way every other module is run, and kept.
#[derive(Debug)]
pub struct ModuleEngine {
    /// The store root, kept rather than the `Store`: a `Store` is not `Sync`
    /// and this is handed to the agent behind an `Arc`. Opening it per run
    /// costs a stat and buys a much simpler ownership story.
    root: PathBuf,
    module_id: String,
    /// This binary. It re-executes itself into the cgroup and only then becomes
    /// the module — see `thalyx_sandbox::launch`.
    helper: PathBuf,
    /// Set by `THALYX_ENGINE_UNCONFINED=1`, for a machine with no BPF LSM.
    ///
    /// It exists so the engine can be exercised on a development machine at
    /// all, and it is deliberately loud rather than silent: `thalyx_core::run`
    /// records such a run as degraded in the journal, exactly as `correr
    /// --unconfined` is.
    unconfined: bool,
}

impl ModuleEngine {
    pub fn new(root: &Path, module_id: &str) -> Result<ModuleEngine, std::io::Error> {
        Ok(ModuleEngine {
            root: root.to_path_buf(),
            module_id: module_id.to_string(),
            helper: std::env::current_exe()?,
            unconfined: std::env::var("THALYX_ENGINE_UNCONFINED").as_deref() == Ok("1"),
        })
    }

    /// The engine for the settings, when the settings say there is one.
    pub fn for_settings(
        store: &Store,
        settings: &thalyx_agent::Settings,
    ) -> Result<Option<Arc<dyn Engine>>, std::io::Error> {
        match &settings.engine_module {
            None => Ok(None),
            Some(id) => Ok(Some(Arc::new(ModuleEngine::new(store.root(), id)?))),
        }
    }

    /// Start the engine and wait for it to say the weights are in.
    fn start(&self, weights: &Path) -> Result<Resident, LlamaError> {
        let store = Store::open(&self.root)
            .map_err(|error| not_installed(&self.module_id, &error.to_string()))?;
        let policies = KernelStore::default_map();

        // As many as this machine has. Asked of the machine rather than
        // written down, because a number that is right on the developer's
        // laptop is contention on a four-core mini PC and idle cores on
        // anything larger.
        let threads = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(4);

        let args: Vec<std::ffi::OsString> = vec![
            "-m".into(),
            weights.into(),
            "--ctx".into(),
            context_size().to_string().into(),
            "--threads".into(),
            threads.to_string().into(),
        ];

        let mut module = thalyx_core::run::start(
            &store,
            &policies,
            &thalyx_core::RunRequest {
                module_id: &self.module_id,
                entrypoint: thalyx_core::run::DEFAULT_ENTRYPOINT,
                args,
                helper: self.helper.clone(),
                request_id: format!("engine-{}", thalyx_journal::now()),
                // The human said the sentence that started this. The model is
                // downstream of that and cannot start one on its own.
                origin: Origin::UserUtterance,
                profile: thalyx_sandbox::profile::MODULE_STANDARD,
                unconfined: self.unconfined,
                // The one caller of this in the whole system. See `Wiring`.
                wiring: thalyx_core::run::Wiring::Talks,
            },
        )
        .map_err(|error| LlamaError::Spawn {
            binary: self.describe(),
            source: std::io::Error::other(error.to_string()),
        })?;

        let pid = module.pid();
        let taken = module.take_stdin().zip(module.take_stdout());
        let Some((to, mut from)) = taken else {
            let policies = KernelStore::default_map();
            module.shutdown(&policies);
            return Err(LlamaError::Spawn {
                binary: self.describe(),
                source: std::io::Error::other(
                    "the engine started without the pipes it is spoken to over",
                ),
            });
        };

        // Blocking, and that is what prewarming is for: this is the wait the
        // graphical session moves off the keystroke, not one it removes.
        let ready = match read_ready(&mut from) {
            Ok(ready) => ready,
            Err(error) => {
                let policies = KernelStore::default_map();
                let outcome = module.shutdown(&policies);
                // What the module wrote at its own descriptors, which for a
                // model that could not load its weights is the only place the
                // reason exists. Rule 10: a failure to read is not a failure to
                // exist, and an engine that died saying why must not be
                // reported as one that died silently.
                return Err(LlamaError::Spawn {
                    binary: self.describe(),
                    source: std::io::Error::other(format!(
                        "{error}\n{}",
                        last_words(&outcome.wrote.stderr)
                    )),
                });
            }
        };

        Ok(Resident {
            module,
            to,
            from: Some(from),
            module_id: self.module_id.clone(),
            weights: weights.to_path_buf(),
            ready,
            pid,
            answered: 0,
        })
    }

    /// The engine, started if it is not there and restarted if it died.
    ///
    /// The one place a `Resident` is ever created, so "two engines at once" is
    /// not a state the rest of the file has to consider.
    fn ensure<'a>(
        &self,
        held: &'a mut Option<Resident>,
        weights: &Path,
    ) -> Result<&'a mut Resident, LlamaError> {
        let usable = match held.as_mut() {
            Some(resident) => {
                resident.module_id == self.module_id
                    && resident.weights == weights
                    && resident.module.still_running()
            }
            None => false,
        };
        // Deliberately not `if let (false, Some(stale)) = (usable, held.take())`,
        // which is what this was and which is wrong in a way that compiles and
        // reads correctly: both halves of a tuple are evaluated before the
        // pattern is matched, so `take` ran on every call and the live engine
        // was dropped whether or not the arm fired. The machine then loaded the
        // weights again for every sentence — the exact failure this whole file
        // exists to remove — and there was nothing to see, because the resident
        // it had just thrown away was a process nobody was waiting for.
        if !usable && let Some(stale) = held.take() {
            stale.retire();
        }
        if held.is_none() {
            set_state(ModelState::Loading);
            match self.start(weights) {
                Ok(resident) => {
                    set_state(ready_state(&resident));
                    *held = Some(resident);
                }
                Err(error) => {
                    set_state(ModelState::Failed(error.to_string()));
                    return Err(error);
                }
            }
        }
        Ok(held.as_mut().expect("just started"))
    }

    /// Load the weights now, on this thread, so a later sentence does not.
    ///
    /// Returns once the engine has said the weights are in, or with the reason
    /// it could not. Nothing else in the system has to call this: an inference
    /// that arrives first simply does the load itself, behind the same lock.
    pub fn warm(&self, weights: &Path) -> Result<(), LlamaError> {
        let mut held = RESIDENT
            .lock()
            .map_err(|_| LlamaError::Io(std::io::Error::other("the engine lock was poisoned")))?;
        self.ensure(&mut held, weights).map(|_| ())
    }
}

/// Start loading the weights now, on a thread, and come straight back.
///
/// Cesar's requirement of 2026-08-28: the machine must draw its screen first
/// and warm the model behind it, so that the first sentence somebody types
/// probably finds a model that is already in memory. Loading a 3B on the boot
/// path would instead have the machine spend several seconds before showing
/// anything, for a model nobody may ask anything of.
///
/// Nothing waits for the handle and nothing has to: a request that arrives
/// mid-load takes the same lock and waits for the same load. There is exactly
/// one engine, and this is why that is a property of the type rather than a
/// thing the callers coordinate.
///
/// Silent when there is no model configured, no engine module, or no store —
/// all three are supported machines by `Principio-Doble-Ruta.md`, and a screen
/// that complained about them at boot would be complaining about a choice.
pub fn prewarm(root: &Path) {
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let Ok(store) = Store::open(&root) else {
            return;
        };
        let Ok(Some(settings)) = crate::agent_model::configured(&store) else {
            return;
        };
        let Some(module_id) = settings.engine_module.clone() else {
            return;
        };
        let Ok(engine) = ModuleEngine::new(&root, &module_id) else {
            return;
        };
        // The error is kept in `STATE` by `warm` itself, which is where the
        // panel reads it. Nothing is printed: this thread has no console —
        // the screen has the display, and a `println!` from here would land on
        // a framebuffer console in graphics mode.
        let _ = engine.warm(&settings.weights);
    });
}

/// The last few lines of what a module wrote before it stopped.
///
/// llama.cpp says a great deal on its way up, and the sentence that matters is
/// always the last one. Printing all of it would bury the reason inside two
/// hundred lines of tensor shapes.
fn last_words(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().filter(|line| !line.is_empty()).collect();
    let from = lines.len().saturating_sub(6);
    lines[from..].join("\n")
}

fn ready_state(resident: &Resident) -> ModelState {
    ModelState::Ready {
        pid: resident.pid,
        engine_pid: resident.ready.engine_pid,
        load_ms: resident.ready.load.as_millis() as u64,
        threads: resident.ready.threads,
        context: resident.ready.context,
        answered: resident.answered,
    }
}

/// The engine module is not installed, said in the words that fix it.
fn not_installed(module_id: &str, why: &str) -> LlamaError {
    LlamaError::Spawn {
        binary: PathBuf::from(format!("module {module_id}")),
        source: std::io::Error::other(format!(
            "{why}\n\
             The agent runs the engine as an installed module. Install it from \
             the repository on the store:\n    \
             instalar {module_id}\n\
             and point the agent at it with `thalyx agent model use <gama> \
             --weights <gguf> --module {module_id}`."
        )),
    }
}

impl Engine for ModuleEngine {
    fn describe(&self) -> PathBuf {
        PathBuf::from(format!("module {}", self.module_id))
    }

    fn preflight(&self) -> Result<(), LlamaError> {
        let store = Store::open(&self.root)
            .map_err(|error| not_installed(&self.module_id, &error.to_string()))?;
        // The manifest and not the directory: a module is installed when the
        // manifest verifies against the pinned key, and a version directory
        // left behind by a removal is not that.
        thalyx_core::installed_manifest(&store, &self.module_id)
            .map_err(|error| not_installed(&self.module_id, &error.to_string()))?;
        Ok(())
    }

    fn scratch_root(&self) -> Option<PathBuf> {
        Some(run_dir())
    }

    fn complete(&self, call: EngineCall<'_>) -> Result<EngineRun, LlamaError> {
        let mut held = RESIDENT
            .lock()
            .map_err(|_| LlamaError::Io(std::io::Error::other("the engine lock was poisoned")))?;

        // Two attempts, and exactly two. A resident process can die — the
        // kernel's memory ceiling, a bad frame, somebody's `kill` — and the
        // cheap right answer is to start it again once and let the second
        // failure be the real one. A restart loop would turn a model that
        // cannot load into a machine that reloads it forever.
        let mut last: Option<LlamaError> = None;
        for attempt in 0..2 {
            let cold = held.is_none();
            let resident = self.ensure(&mut held, call.weights)?;
            let pid = resident.pid;
            match resident.ask(&call) {
                Ok(answered) => {
                    let elapsed = answered.elapsed;
                    let state = ready_state(resident);
                    set_state(state);
                    if let Ok(mut recorded) = LAST.lock() {
                        *recorded = Some(LastInference { pid, elapsed, cold });
                    }
                    // A refusal travels on `stderr` and an answer on
                    // `stdout`, which is where each would have been had this
                    // been a process that ran once and exited. Everything above
                    // the seam reads them from there and does not have to know
                    // the engine is still alive.
                    let (stdout, stderr) = if answered.ok {
                        (answered.body, Vec::new())
                    } else {
                        (Vec::new(), answered.body)
                    };
                    let refused = !stderr.is_empty() || !answered.ok;
                    return Ok(EngineRun {
                        stdout,
                        stderr,
                        failed: refused.then(|| "the engine refused the request".to_string()),
                        // Not sampled. The peak is read from `/proc/<pid>/status`
                        // and this process did not fork the engine; reporting a
                        // zero would be rule 10 broken — a failure to read
                        // printed as a measurement of a small thing.
                        peak_rss: None,
                    });
                }
                Err(error) => {
                    // The engine is not usable any more, whatever the reason:
                    // a broken pipe, a frame that did not parse, a deadline it
                    // ran past. All of them leave Thalyx unable to say where in
                    // the protocol the engine is, and going on would be
                    // guessing. Rule 9 — the cautious answer.
                    if let Some(dead) = held.take() {
                        dead.retire();
                    }
                    set_state(ModelState::Cold);
                    last = Some(LlamaError::Io(std::io::Error::other(format!(
                        "the engine stopped answering: {error}{}",
                        if attempt == 0 {
                            " — starting it again"
                        } else {
                            ""
                        }
                    ))));
                }
            }
        }
        Err(last.expect("the loop only ends with an answer or an error"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame Thalyx writes is the frame `engine/thalyx-engine.cpp` reads.
    ///
    /// Read back by this file rather than by the engine, which is what makes
    /// this a *roundtrip* test and not a proof: the C++ side is the other half
    /// and it is exercised by running it. What this catches is the half that
    /// can be got wrong silently — a field written in the wrong order, or a
    /// length that counts characters instead of bytes.
    #[test]
    fn a_request_frame_says_the_paths_the_engine_will_open() {
        let call = EngineCall {
            weights: Path::new("/models/model.gguf"),
            prompt_file: Path::new("/run/ábc/prompt.txt"),
            grammar_file: Some(Path::new("/run/ábc/grammar.gbnf")),
            predict: 256,
            seed: 7,
            extra_args: &[],
            timeout: Duration::from_secs(1),
        };
        let frame = request_frame(&call);
        let mut at = &frame[..];

        let mut magic = [0u8; 4];
        read_exactly(&mut at, &mut magic).unwrap();
        assert_eq!(&magic, REQUEST);
        assert_eq!(read_u32(&mut at).unwrap(), 256);
        assert_eq!(read_u64(&mut at).unwrap(), 7);

        let field = |at: &mut &[u8]| {
            let length = read_u32(at).unwrap() as usize;
            let mut bytes = vec![0u8; length];
            read_exactly(at, &mut bytes).unwrap();
            String::from_utf8(bytes).unwrap()
        };
        assert_eq!(field(&mut at), "/run/ábc/prompt.txt");
        assert_eq!(field(&mut at), "/run/ábc/grammar.gbnf");
        assert!(at.is_empty(), "the frame had {} bytes left over", at.len());
    }

    /// The free arm of the grammar probe sends no grammar, and says so with a
    /// zero length rather than by leaving the field out.
    ///
    /// A frame whose shape depends on its contents is a frame the other side
    /// has to guess at, and the guess is where a protocol goes wrong.
    #[test]
    fn a_call_with_no_grammar_still_carries_the_field() {
        let call = EngineCall {
            weights: Path::new("/models/model.gguf"),
            prompt_file: Path::new("/p"),
            grammar_file: None,
            predict: 1,
            seed: 1,
            extra_args: &[],
            timeout: Duration::from_secs(1),
        };
        let frame = request_frame(&call);
        // magic, predict, seed, the prompt path's length and its two bytes,
        // and the grammar field's length with nothing after it.
        assert_eq!(frame.len(), 4 + 4 + 8 + 4 + 2 + 4);
        assert_eq!(&frame[frame.len() - 4..], &0u32.to_le_bytes());
    }

    /// A ready frame is read back field for field, in the order the engine
    /// writes it.
    #[test]
    fn the_ready_frame_says_what_the_load_cost_and_who_paid_it() {
        let mut frame = Vec::new();
        frame.extend_from_slice(READY);
        frame.extend_from_slice(&4200u64.to_le_bytes());
        frame.extend_from_slice(&11u32.to_le_bytes());
        frame.extend_from_slice(&4u32.to_le_bytes());
        frame.extend_from_slice(&4096u32.to_le_bytes());

        let ready = read_ready(&mut &frame[..]).unwrap();
        assert_eq!(ready.load, Duration::from_millis(4200));
        assert_eq!(ready.engine_pid, 11);
        assert_eq!(ready.threads, 4);
        assert_eq!(ready.context, 4096);
    }

    /// A first frame that is not the ready frame is a failure, not a wait.
    ///
    /// The case this is about: a module that is not the engine at all. Waiting
    /// for a frame it will never send would hang the boot, and the machine
    /// would come up with a screen that never says anything about the model.
    #[test]
    fn something_that_is_not_the_engine_is_refused_rather_than_waited_for() {
        let error = read_ready(&mut &b"main: build = 1234\n"[..]).unwrap_err();
        assert!(
            error.to_string().contains("weights are in"),
            "unhelpful: {error}"
        );
    }

    /// A length past the ceiling is refused before anything is allocated for it.
    #[test]
    fn an_answer_longer_than_the_ceiling_is_refused_before_it_is_read() {
        let mut frame = Vec::new();
        frame.extend_from_slice(RESPONSE);
        frame.push(0);
        frame.extend_from_slice(&1u64.to_le_bytes());
        frame.extend_from_slice(&(MAX_BODY + 1).to_le_bytes());

        let error = read_answer(&mut &frame[..]).unwrap_err();
        assert!(error.to_string().contains("past what Thalyx will read"));
    }

    /// A refusal comes back as a refusal, with the engine's own words in it.
    #[test]
    fn a_status_of_one_is_the_engine_saying_why_it_could_not() {
        let reason = b"could not open /run/x/prompt.txt";
        let mut frame = Vec::new();
        frame.extend_from_slice(RESPONSE);
        frame.push(1);
        frame.extend_from_slice(&12u64.to_le_bytes());
        frame.extend_from_slice(&(reason.len() as u32).to_le_bytes());
        frame.extend_from_slice(reason);

        let answered = read_answer(&mut &frame[..]).unwrap();
        assert!(!answered.ok);
        assert_eq!(answered.elapsed, Duration::from_millis(12));
        assert_eq!(answered.body, reason);
    }
}
