//! What the agent writes down, so that a reboot does not lose the task.
//!
//! Step 6 of the Phase 1 exit criterion is restarting the machine and finding
//! that the agent still knows what was being done. This is the half that makes
//! that possible; `thalyx memory recall` is the half that reads it back, and it
//! is an ordinary command rather than an agent-only one, because by
//! `vault/01-Filosofia/Principio-Doble-Ruta.md` anything the agent can see, a
//! human must be able to see without it.
//!
//! ## Two records, and the difference between them is the point
//!
//! **What was asked** is a fact about the conversation and it witnesses
//! nothing. No file changing on disk can make "the human said this" stop being
//! true, so attaching a witness to it would only manufacture a way for a true
//! statement to start reading as doubtful.
//!
//! **What was installed** is a claim about the filesystem, and it witnesses the
//! module's `current` link — the single point that decides whether a module is
//! installed at all. Remove the module, or upgrade past that version, and the
//! memory stops being assertable and says which path changed, instead of going
//! on reporting an installation that is no longer there.
//!
//! That second one is `vault/03-Primitivas/Memoria-Persistente.md` applied to
//! the agent's own output: the agent does not get to be believed about the
//! world just because it was the one who acted on it.

use std::path::{Path, PathBuf};
use thalyx_memory::{LexicalEmbedder, Memory, Witness};

#[derive(Debug, thiserror::Error)]
pub enum RecollectionError {
    #[error("could not prepare {path} for the agent's memory: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Memory(#[from] thalyx_memory::MemoryError),
}

/// Record an install under `task`.
///
/// Called *after* the install succeeds, never before. A memory written first
/// would describe an installation that then failed, and a memory of something
/// that did not happen is worse than no memory at all.
pub fn record_install(
    memory_path: &Path,
    task: &str,
    utterance: &str,
    module_id: &str,
    version: &str,
    installed_at: &Path,
) -> Result<(), RecollectionError> {
    if let Some(parent) = memory_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| RecollectionError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let embedder = LexicalEmbedder;
    let memory = Memory::open(memory_path, &embedder)?;

    memory.remember_fact(
        task,
        &format!("the human asked: {utterance}"),
        &Witness::nothing(),
        &embedder,
    )?;
    memory.remember_fact(
        task,
        &format!("installed {module_id} {version}"),
        &Witness::over([installed_at]),
        &embedder,
    )?;

    Ok(())
}
