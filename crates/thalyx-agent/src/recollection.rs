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
//!
//! ## Why undoing writes only the first kind
//!
//! A rollback records that it was asked for and records nothing about the
//! world, which looks like an omission and is the opposite. The install's own
//! record witnesses the module's `current` link; taking the module away is
//! precisely what makes that link stop resolving, so the removal already shows
//! up — as the install becoming unconfirmable, said in those words.
//!
//! Writing "took back X" as well would mean asserting an outcome on the
//! strength of having performed it, next to a record of the same event that is
//! checked against the disk every time it is read. When the two disagreed, the
//! unchecked one would be the one still claiming to be true.

use crate::transcript::Segment;
use std::path::{Path, PathBuf};
use thalyx_memory::{LexicalEmbedder, Memory, Standing, Witness};

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

/// What the agent still knows about a task, ready to be reasoned over.
///
/// The split is the whole design. See [`context`].
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Everything usable as trusted context, ready for a transcript.
    ///
    /// This is [`Context::said`] and [`Context::holds`] together — kept as a
    /// separate field rather than recomputed, so that what the agent reasons
    /// over and what it shows can never drift apart.
    pub segments: Vec<Segment>,
    /// Records of things that were said. Nothing on disk can confirm or refute
    /// them, and nothing should try.
    pub said: Vec<String>,
    /// Claims about the world that still check out.
    pub holds: Vec<String>,
    /// Claims that no longer check out. Shown, never used.
    pub unconfirmable: Vec<String>,
}

impl Context {
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty() && self.unconfirmable.is_empty()
    }
}

/// Read back what the agent knows about `task`.
///
/// ## Why memory is Thalyx's channel and not a fourth one
///
/// A remembered fact is not text somebody sent us. It is Thalyx's own record of
/// something Thalyx did, checked against the disk every time it is read. That
/// is [`Channel::Thalyx`] by definition, and it means a value the agent can only
/// have got from its own memory is still allowed to have effect — which is what
/// makes "install that again" possible at all.
///
/// ## Three standings, not two, and the middle one is the useful one
///
/// The memory grades every fact as verified, unverified, or **unwitnessed** —
/// recorded with nothing on disk to check it against. The first version of this
/// function handled two of them and silently dropped the third, which threw
/// away exactly the record worth keeping: *what the human asked for*. Found by
/// running `thalyx agent recall` and noticing the task's own subject was
/// missing from its own summary.
///
/// The three are treated differently because they are different in kind:
///
/// - **Unwitnessed** — "you asked me to X". A record of speech, not a claim
///   about the world. There is nothing for it to go stale against, so it is
///   context, and it is said as something you told me rather than as something
///   that holds.
/// - **Verified** — "X is installed", and the disk still agrees. Context.
/// - **Unverified** — it did agree once and no longer does. **Not context.**
///
/// ## Why an unconfirmable fact is not context
///
/// One the memory cannot check is **not wrong** — the file it described may
/// simply have changed outside Thalyx — but it is no longer a statement about
/// the present, and authorising an action from it means acting on a belief the
/// system itself has just said it cannot confirm. Rule 9: the cautious answer,
/// never the fast one.
///
/// The human is never stuck by this: they can always name the thing themselves
/// and take the rules path, which is the double route being the reason a
/// defence like this is affordable at all.
///
/// [`Channel::Thalyx`]: crate::transcript::Channel::Thalyx
pub fn context(memory_path: &Path, task: &str) -> Result<Context, RecollectionError> {
    if !memory_path.exists() {
        // Nothing remembered yet is not an error. A first conversation and a
        // lost database would be indistinguishable if it were, and only one of
        // those is worth alarming anyone about.
        return Ok(Context::default());
    }

    let embedder = LexicalEmbedder;
    let memory = Memory::open(memory_path, &embedder)?;
    let recalled = memory.recall(task)?;

    let mut context = Context::default();
    for fact in &recalled.facts {
        let text = fact.record.text.clone();
        match fact.standing {
            Standing::Unwitnessed => {
                context.segments.push(Segment::thalyx(text.clone()));
                context.said.push(text);
            }
            Standing::Verified => {
                context.segments.push(Segment::thalyx(text.clone()));
                context.holds.push(text);
            }
            // Deliberately not pushed into `segments`. This is the whole
            // fail-closed half; a test disables it and watches an action get
            // authorised from a belief the memory had disowned.
            Standing::Unverified { .. } => context.unconfirmable.push(text),
        }
    }

    Ok(context)
}

/// Record that the human asked for something, under `task`.
///
/// Witnesses nothing, and that is not a shortcoming. No file changing on disk
/// can make "the human said this" stop being true, so a witness could only
/// manufacture a way for a true statement to start reading as doubtful.
///
/// Called *after* the operation succeeds, never before — same reason as
/// [`record_install`]. An utterance recorded ahead of the act would survive a
/// refusal at the trusted path and read afterwards as though the person had got
/// what they asked for.
pub fn record_utterance(
    memory_path: &Path,
    task: &str,
    utterance: &str,
) -> Result<(), RecollectionError> {
    let embedder = LexicalEmbedder;
    let memory = open(memory_path)?;
    memory.remember_fact(
        task,
        &format!("the human asked: {utterance}"),
        &Witness::nothing(),
        &embedder,
    )?;
    Ok(())
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
    record_utterance(memory_path, task, utterance)?;

    let embedder = LexicalEmbedder;
    let memory = open(memory_path)?;
    memory.remember_fact(
        task,
        &format!("installed {module_id} {version}"),
        &Witness::over([installed_at]),
        &embedder,
    )?;

    Ok(())
}

fn open(memory_path: &Path) -> Result<Memory, RecollectionError> {
    if let Some(parent) = memory_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| RecollectionError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(Memory::open(memory_path, &LexicalEmbedder)?)
}
