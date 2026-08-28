//! The inference engine as an installed module, rather than a program on `PATH`.
//!
//! Cesar's decree of 2026-08-28: **the engine is the first real module**, and
//! it is not part of Thalyx. `vault/02-Arquitectura/Gamas-de-Modelo.md` already
//! put llama.cpp outside the process and `vault/11-Seguridad/Modelo-de-Amenaza.md`
//! puts the model outside the TCB; this is what makes both of those something
//! the operating system enforces rather than something the design asserts.
//!
//! ## Why this file is in the CLI and not in `thalyx-agent`
//!
//! `thalyx_agent::llama::Engine` is a seam with two sides. The agent crate is
//! where the model's output is parsed, and it is deliberately not a crate that
//! can start a confined process — everything it knows arrived from an
//! untrusted model. Running one is the store's business and the sandbox's, so
//! the implementation lives where those are, and the agent only ever sees an
//! argument vector going out and bytes coming back.
//!
//! It also means there is exactly one launcher: `thalyx_core::run`, the same
//! one `correr` uses. The engine gets the cgroup, the seccomp filter, the
//! pivoted root, the uid of its own and the journal entry that every other
//! module gets, because it *is* every other module. Nothing here re-implements
//! any of that, and the one thing this file must not do is grow a second way
//! to start a program.
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
//! a prompt anywhere else produces a run where llama.cpp is handed `-f` to a
//! file that does not exist inside its root — which comes back as "the tool
//! never completed the prompt" and sends whoever reads it to audit llama.cpp.

use std::path::{Path, PathBuf};
use std::sync::Arc;
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

/// The engine, run the way every other module is run.
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
        let store = Store::open(&self.root)
            .map_err(|error| not_installed(&self.module_id, &error.to_string()))?;
        let policies = KernelStore::default_map();

        let outcome = thalyx_core::run(
            &store,
            &policies,
            thalyx_core::RunRequest {
                module_id: &self.module_id,
                entrypoint: thalyx_core::run::DEFAULT_ENTRYPOINT,
                args: call.args(),
                helper: self.helper.clone(),
                request_id: format!("inference-{}", thalyx_journal::now()),
                // The human said the sentence that started this. The model is
                // downstream of that and cannot start one on its own.
                origin: Origin::UserUtterance,
                profile: thalyx_sandbox::profile::MODULE_STANDARD,
                unconfined: self.unconfined,
            },
        )
        .map_err(|error| LlamaError::Spawn {
            binary: self.describe(),
            source: std::io::Error::other(error.to_string()),
        })?;

        // What the module wrote at its own descriptors, which is where
        // llama.cpp puts a completion. Not the channel: the channel is the
        // surface Thalyx mediates and llama.cpp knows nothing about it.
        Ok(EngineRun {
            stdout: outcome.wrote.stdout.into_bytes(),
            stderr: outcome.wrote.stderr.into_bytes(),
            failed: match outcome.exit_code {
                Some(0) => None,
                Some(code) => Some(format!("exit status: {code}")),
                None => Some("terminated by a signal".to_string()),
            },
            // Not sampled. The peak is read from `/proc/<pid>/status` and the
            // pid here belongs to a process this one did not fork; reporting a
            // zero would be rule 10 broken — a failure to read printed as a
            // measurement of a small thing.
            peak_rss: None,
        })
    }
}
