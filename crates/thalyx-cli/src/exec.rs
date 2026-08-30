//! `hacer` — one composed intention, executed as one transaction.
//!
//! `vault/03-Primitivas/Ejecucion-Transaccional.md`. Every other verb on this
//! machine answers one question and hands the answer back to whoever asked. For
//! a person that is right: they are standing there, and the next thing they type
//! depends on what they just read. For a frontier model it is the dominant cost
//! of the whole arrangement — **every** answer is another inference pass, and an
//! inference pass drags the entire conversation with it.
//!
//! The traces in `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md` say it
//! plainly: of the calls an agent spent on a reversible task, the ones that
//! carried a *decision* were a minority. Opening a boundary, checking what
//! changed, deciding whether a rename left anything behind, putting it back — a
//! deterministic machine can do all of that without asking anybody, and until
//! now this machine made the model watch.
//!
//! So this verb takes a **program**: several requests, what has to be true when
//! they are done, and what to do if it is not. It runs the lot inside a
//! reversible boundary, decides commit or rollback from the result, and answers
//! once with a summary small enough that reading it is not itself a cost.
//!
//! ## What it is not, and the four ways this could have been cheating
//!
//! 1. **It is not a shell, and it does not run one.** The image carries the
//!    Linux kernel and one program; there is nothing to shell out to. What
//!    composes here is Thalyx's own requests, which is the substrate that
//!    actually exists.
//! 2. **It is not a second authority.** Every step goes through
//!    [`crate::external::one`] — the same function, the same argument check
//!    against the same workspace boundary, that a single request goes through.
//!    A program is not a way to reach a verb that is not exposed, is not a way
//!    to reach a path outside the workspace, and is not a way to reach one
//!    argument slot with another's contents. If that were not true this file
//!    would be the parallel API `Agentes-Externos.md` forbids, and the boundary
//!    would hold for one request and not for thirty.
//! 3. **The transaction is a real one.** The boundary is `intento`, which is a
//!    snapshot; the rollback is `intento abandonar`, which is a restore; and it
//!    is authorised by the exact state witness of the tree, checked under the
//!    lock at the instant of the destruction. A label around some writes would
//!    be worse than nothing, because it would be believed.
//! 4. **Validation is a real decision.** A check that always passed would make
//!    every commit a lie about having been checked. Each one here either
//!    genuinely runs — a search of the tree, a parse of what changed, a
//!    confined process whose exit status is read — or reports that it could not
//!    run, and a check that could not run is never a check that passed.
//!
//! ## Why the answer is small and the evidence is not
//!
//! The point of doing thirty things locally is lost if the answer is thirty
//! answers. So what comes back is the shape of what happened — how much
//! changed, what was checked, whether it committed — and the raw material goes
//! into the store under a handle. `evidencia <id>` fetches it, and nothing
//! fetches it by default. Progressive disclosure: the model pays for the detail
//! only in the cases where the detail is what it needs.
//!
//! It is never silently cut. Every bound this file applies says it applied.

use crate::files::{Face, Where};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use thalyx_core::Store;
use thalyx_core::attempt::{self, Authorised};
use thalyx_snapshot::{Snapshots, Volumes};

type Fallible = Result<(), Box<dyn std::error::Error>>;

pub const OP: &str = "exec";

/// The most requests one program may hold.
///
/// A ceiling and not a preference. A program is checked, snapshotted and run
/// inside one request, which means the caller on the other end is waiting with
/// no way to see progress; a thousand-step program is one that cannot be
/// interrupted, cannot be reported on, and would hold the session for as long
/// as it took. Sixty is far past what the traces show an agent composing.
pub const MOST_STEPS: usize = 64;

/// The most checks one program may ask for. Each can launch a process.
pub const MOST_CHECKS: usize = 16;

/// How many changed paths the answer names before the count takes over.
const NAMED: usize = 32;

/// How much of a program's own output the answer carries, per stream.
///
/// Small on purpose: this is the number that decides how much machine noise
/// reaches a context window. The whole of it is in the evidence.
const SUMMARY: usize = 480;

// ── the program ──────────────────────────────────────────────────────────────

/// One request of a program: exactly the shape a single request has.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub verb: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}

/// What a check demands, and how it finds out.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "check", rename_all = "snake_case")]
pub enum Check {
    /// The text must be gone from the workspace, or must still be there.
    ///
    /// The rename's post-condition, and the one an agent otherwise spends a
    /// whole round trip on: search, read the answer, decide it is empty.
    Text {
        text: String,
        /// `"none"` or `"some"`.
        #[serde(default = "none")]
        expect: String,
        /// Where to look. The workspace when absent.
        #[serde(default)]
        r#in: Option<String>,
    },
    /// Every changed source file still parses.
    ///
    /// Thalyx's own parser, the one the index is built with — so what this
    /// calls a file it can read is the same thing the rest of the machine calls
    /// one. It is a syntax check and says so: it is not a type check and must
    /// never be reported as one.
    Parses,
    /// Run a program, confined, and require it to exit 0.
    Program {
        program: String,
        #[serde(default)]
        arguments: Vec<String>,
    },
    /// `cargo check` over the crates the change really reaches.
    ///
    /// **Not the crates the changed files are in.** Change a type in
    /// `thalyx-core` and compiling `thalyx-core` proves nothing about the
    /// twelve crates that use it, so the selection is the reverse dependency
    /// closure — derived from Cargo's own graph, so the model never spends a
    /// turn deciding it. `thalyx_rust::affected` is where the rule lives.
    ///
    /// The answer is reused when the exact bytes it would read have already
    /// been checked by this machine under this toolchain, and it says which
    /// happened.
    Rust {
        /// `check` (the default) or `test`.
        #[serde(default = "check_word")]
        mode: String,
        /// Check these crates instead of the derived ones.
        ///
        /// The escape hatch, and it is deliberately a *replacement* rather than
        /// an addition: a caller that has decided what to compile has made a
        /// decision, and a machine that widened it would make the hatch a
        /// suggestion.
        #[serde(default)]
        packages: Vec<String>,
    },
}

fn none() -> String {
    "none".to_string()
}
fn check_word() -> String {
    "check".to_string()
}

/// What to do when the steps or the checks say no.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFailure {
    /// Put the tree back. The default, and the reason this verb exists.
    #[default]
    Rollback,
    /// Leave the workspace as the failure left it, with the attempt still open.
    ///
    /// For the case where the failure *is* the information — a caller that
    /// wants to look at what the compiler complained about, in the tree that
    /// produced it. It is never the default: a caller that did not say leaves
    /// nothing behind.
    Keep,
}

/// The most bytes of program source one request may carry.
///
/// Not a guess about complexity — a bound on what arrives from outside. Sixty
/// four kilobytes is far more than any program a model writes in one inference
/// and small enough that a caller sending a file by mistake is refused rather
/// than parsed.
pub const MOST_PROGRAM_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct Program {
    #[serde(default = "agent")]
    pub label: String,
    /// A program: JavaScript, run locally, with the machine's capabilities and
    /// none of its authority.
    ///
    /// **This is the form that made the verb worth having.** `steps` below
    /// requires every operation and every argument to be known before anything
    /// runs, which cannot express the thing an agent actually spends its turns
    /// on — ask, look at the answer, decide. A program can: the references a
    /// query returned are a variable, the ones worth changing are an `if`, and
    /// the validation that decides whether any of it is kept is a call whose
    /// result the next line reads.
    #[serde(default)]
    pub run: Option<String>,
    /// The older form: a list of requests, in order.
    ///
    /// Kept, and not deprecated. It is the right shape when the work really is
    /// known in advance — and it is the control column for every measurement
    /// of what the programmable form buys, which is a use that does not expire.
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub validate: Vec<Check>,
    #[serde(default)]
    pub on_failure: OnFailure,
}

fn agent() -> String {
    "agent".to_string()
}

impl Program {
    /// Read a program, or say what is wrong with it in one sentence.
    ///
    /// Everything refusable is refused **here**, before a snapshot is taken: a
    /// program with a bad step at position nine would otherwise open a
    /// boundary, do eight things and roll them back, which costs a caller a
    /// snapshot and a restore to learn about a typo.
    pub fn read(text: &str) -> Result<Self, String> {
        let program: Program = serde_json::from_str(text).map_err(|error| {
            format!(
                "the program is not the object this verb takes: {error}. It is \
                 {{\"steps\":[{{\"verb\":…,\"arguments\":[…]}}],\"validate\":[…]}}"
            )
        })?;

        let has_source = program
            .run
            .as_ref()
            .is_some_and(|source| !source.trim().is_empty());
        // Exactly one, and refused rather than resolved by precedence. A
        // caller that sent both has two ideas about what it wants done, and a
        // rule that silently ran one of them would be Thalyx picking which —
        // inside a transaction, over somebody's files.
        if has_source && !program.steps.is_empty() {
            return Err(
                "a program is either `run` — code — or `steps` — a list of requests — \
                 and this has both. Which one was meant is not something this machine \
                 will decide inside a transaction"
                    .to_string(),
            );
        }
        if !has_source && program.steps.is_empty() {
            return Err(
                "there is nothing to run: give `run`, a short JavaScript program, or \
                 `steps`, a list of requests. A program with neither changes nothing \
                 and there is nothing to be transactional about"
                    .to_string(),
            );
        }
        if let Some(source) = &program.run
            && source.len() > MOST_PROGRAM_BYTES
        {
            return Err(format!(
                "the program is {} bytes and this verb takes at most {MOST_PROGRAM_BYTES}",
                source.len()
            ));
        }
        if program.steps.len() > MOST_STEPS {
            return Err(format!(
                "a program takes at most {MOST_STEPS} steps and this one has {}",
                program.steps.len()
            ));
        }
        if program.validate.len() > MOST_CHECKS {
            return Err(format!(
                "a program takes at most {MOST_CHECKS} checks and this one has {}",
                program.validate.len()
            ));
        }
        for (index, step) in program.steps.iter().enumerate() {
            // Rejected by name rather than by recursion depth. A program that
            // may contain a program is a program whose cost cannot be read off
            // it, and the one nesting anybody would write — open a boundary
            // inside a boundary — is the one `intento` refuses anyway.
            if step.verb == OP {
                return Err(format!(
                    "step {}: `{OP}` cannot be a step of a program",
                    index + 1
                ));
            }
            // The runtime owns the boundary. A step that settled the attempt
            // would leave this verb committing or rolling back something that
            // is no longer there — and, worse, would let a program abandon a
            // tree without ever naming a state.
            if step.verb == "attempt" {
                return Err(format!(
                    "step {}: `attempt` is what this verb *is*. The boundary is opened \
                     around the whole program and settled by what the checks say; \
                     `on_failure` chooses which way",
                    index + 1
                ));
            }
        }
        Ok(program)
    }
}

// ── what one run produced ────────────────────────────────────────────────────

/// One step, as the evidence records it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub verb: String,
    pub arguments: Vec<String>,
    pub ok: bool,
    /// The verb's own answer, whole. This is the part that does not go back to
    /// the model.
    pub answer: Value,
}

/// One check, as the evidence records it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRecord {
    /// What was asked for, exactly, so two runs of the same check can be
    /// recognised as the same question.
    ///
    /// It exists because a **program** can validate more than once — the
    /// ordinary shape being *check, see it fail, fix it, check again* — and a
    /// transaction that rolled back because an earlier attempt had failed
    /// would make that shape impossible to write. What gates the commit is the
    /// last verdict per key; every attempt is still in the evidence.
    #[serde(default)]
    pub key: String,
    pub check: String,
    /// Three outcomes and not two. A check that could not run is not a check
    /// that failed and is not a check that passed — rule 10, and here it
    /// decides whether a commit was checked at all.
    pub verdict: Verdict,
    /// One line, for the answer.
    pub summary: String,
    /// Everything it produced. For a confined program, both streams whole.
    #[serde(default)]
    pub output: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Passed,
    Failed,
    /// It could not be run here. Treated as a failure by the transaction —
    /// fail closed — and reported as what it is, so nobody reads a rollback
    /// caused by a missing toolchain as a rollback caused by broken code.
    NotProven,
}

impl Verdict {
    fn word(self) -> &'static str {
        match self {
            Verdict::Passed => "passed",
            Verdict::Failed => "failed",
            Verdict::NotProven => "not_proven",
        }
    }
}

/// What a whole run did, kept in the store and fetched by handle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub transaction: String,
    pub label: String,
    pub at: String,
    pub workspace: String,
    pub snapshot: Option<String>,
    pub start_state: Option<String>,
    pub end_state: Option<String>,
    pub status: String,
    pub rolled_back: bool,
    pub reason: String,
    pub steps: Vec<StepRecord>,
    pub checks: Vec<CheckRecord>,
    pub changed: Vec<String>,
    pub change_count: usize,
    pub metrics: Metrics,

    /// The source of the program, when there was one.
    ///
    /// Kept because a run is not auditable without it: the steps say what
    /// happened and only this says what was *asked for*, and the two differ
    /// exactly where the interesting behaviour is.
    #[serde(default)]
    pub program: Option<String>,
    /// How the program ended: `returned`, `needs_model`, `assertion`,
    /// `threw`, `exhausted` or `refused`.
    #[serde(default)]
    pub finish: Option<String>,
    /// One sentence saying why it ended that way.
    #[serde(default)]
    pub finish_why: Option<String>,
    /// What it handed back, or what it asked the model about.
    #[serde(default)]
    pub returned: Value,
    /// What the program said with `thalyx.log`, bounded.
    #[serde(default)]
    pub printed: Vec<String>,
}

/// What the machine did, as numbers, so the hypothesis this verb exists to test
/// can be read off a run instead of argued about.
///
/// The one that matters is the ratio between [`Metrics::external_requests`] —
/// which is one, always, by construction — and everything else. See
/// `vault/09-Notas-Tecnicas/Trabajo-Entre-Inferencias.md`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    /// Round trips to whatever asked. One. It is written down rather than
    /// assumed because it is the numerator of the whole measurement.
    pub external_requests: usize,
    /// Requests dispatched inside the machine, boundary and rollback included.
    pub machine_operations: usize,
    /// Processes started under confinement.
    pub process_launches: usize,
    /// Files the workspace gained, lost or changed.
    pub filesystem_mutations: usize,
    pub validations: usize,
    /// Times a state witness was computed and compared against a claim.
    pub state_witness_checks: usize,
    pub machine_time_ms: u128,
    /// Bytes of answer and program output produced inside the machine.
    pub internal_bytes: usize,
    /// Bytes of the answer that leaves for the model.
    pub returned_bytes: usize,
    /// Questions put to the Rust semantic provider inside this request.
    pub semantic_queries: usize,
    /// Of those, answered from what the machine already knew about an
    /// unchanged tree.
    pub semantic_cache_hits: usize,
    /// Times a rust-analyzer had to be started. The expensive one — about 25
    /// seconds on this workspace — and the number the persistent store exists
    /// to keep at zero.
    pub analyzer_starts: usize,
    /// Validations answered from a previous run over the same bytes.
    pub validation_cache_hits: usize,
    /// Validations that had to be run.
    pub validation_cache_misses: usize,
    /// How many crates the change was found to reach.
    pub affected_packages: usize,
    /// Whether the semantic provider that answered was under Thalyx's
    /// confinement.
    ///
    /// `None` when nothing semantic was asked. Never assumed: rust-analyzer
    /// runs Cargo, which compiles and runs build scripts, and a run that
    /// reported nothing about where that happened would let a reader believe a
    /// compiler tree had been confined when it had not.
    #[serde(default)]
    pub analyzer_confined: Option<bool>,
    /// One phrase saying what started it.
    #[serde(default)]
    pub analyzer_how: Option<String>,

    // ── what the programmable form produced ────────────────────────────────
    //
    // Zeroes on a run that sent `steps`, and said anyway: a field that only
    // appears on the interesting day is a field nobody handles on the
    // interesting day.
    /// Things the program asked the machine for: requests, validations and
    /// observations of the tree.
    ///
    /// **The numerator of the whole claim**, against `external_requests`, which
    /// is one. A static list of steps could produce a big number here too; what
    /// it could not produce is a big number where the *later* operations were
    /// chosen from the answers to the earlier ones.
    pub program_operations: usize,
    /// Premises the program checked, held or not.
    pub program_assertions: usize,
    /// Times the engine's interrupt handler fired. A rough measure of how much
    /// the program actually did, and the units the `ticks` ceiling is in.
    pub program_ticks: u64,
    /// Whether the program was the code form.
    pub programmable: bool,
}

// ── the evidence store ───────────────────────────────────────────────────────

/// Where a run's evidence is kept.
///
/// In the store and **never in the workspace**, which is not tidiness: the
/// workspace is what a rollback replaces, so evidence written there would be
/// destroyed by the very rollback it is the explanation for. A caller would be
/// told its program failed and handed a dangling handle.
fn evidence_directory(store: &Store) -> PathBuf {
    store.state_root().join("evidence")
}

pub fn evidence_path(store: &Store, id: &str) -> PathBuf {
    evidence_directory(store).join(format!("{id}.json"))
}

/// Whether a handle is one this machine could have issued.
///
/// Checked before it is ever joined onto a path. A handle arrives from outside
/// and `../../etc/passwd` is a string like any other; the ids this verb makes
/// are hex and hyphens, so anything else is refused rather than resolved.
pub fn is_a_handle(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn keep(store: &Store, evidence: &Evidence) -> std::io::Result<()> {
    let directory = evidence_directory(store);
    std::fs::create_dir_all(&directory)?;
    let text = serde_json::to_string_pretty(evidence)?;
    let path = evidence_path(store, &evidence.transaction);
    // Written whole and renamed over, the way every other state file in this
    // system is published. A half-written evidence file is the one artefact
    // nobody can reconstruct: the tree it describes has already been rolled
    // back by the time this is written.
    let temporary = directory.join(format!(".{}.writing", evidence.transaction));
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, &path)
}

// ── the runtime ──────────────────────────────────────────────────────────────

/// Everything one run needs that is not the program.
pub struct Asked<'a> {
    pub store: &'a Store,
    /// What a program of this request may spend.
    ///
    /// Carried rather than read from the environment where it is used, and
    /// that is rule 11: a test that wanted a one-second ceiling would
    /// otherwise have to set a variable of *this whole process*, which is a
    /// global switch with no owner whose value is some other test's
    /// precondition. The verb reads the environment once, here; everything
    /// below takes what it is given.
    pub limits: thalyx_program::Limits,
    /// The tree the boundary is about. Where the session stands, exactly, and
    /// never an ancestor — `crate::attempt::subvolume_to_attempt` is why.
    pub subvolume: PathBuf,
    pub request_id: String,
}

/// Run a program inside a boundary, and answer once.
///
/// Generic over [`Volumes`] for the reason the rest of this system is: the
/// policy — what happens when a step is refused, what a failing check does,
/// what authorises the rollback — is not Btrfs, and policy that can only be
/// exercised on Btrfs is policy that is never exercised. The verb passes
/// `Native`; the tests pass the directory-backed fake and run everywhere.
pub fn carry_out<V: Volumes>(
    asked: &Asked<'_>,
    volumes: V,
    here: &mut Where,
    program: &Program,
) -> Evidence {
    let started = std::time::Instant::now();
    // Read before anything runs and subtracted after, because the provider
    // outlives the request on purpose: its totals are the process's and only
    // the difference belongs to this call.
    let semantics_before = crate::semantic::tally(&asked.subvolume);
    let mut metrics = Metrics {
        external_requests: 1,
        ..Metrics::default()
    };

    let mut evidence = Evidence {
        transaction: asked.request_id.clone(),
        label: program.label.clone(),
        at: thalyx_journal::now(),
        workspace: asked.subvolume.display().to_string(),
        snapshot: None,
        start_state: None,
        end_state: None,
        status: "refused".to_string(),
        rolled_back: false,
        reason: String::new(),
        steps: Vec::new(),
        checks: Vec::new(),
        changed: Vec::new(),
        change_count: 0,
        metrics: metrics.clone(),
        program: program.run.clone(),
        finish: None,
        finish_why: None,
        returned: Value::Null,
        printed: Vec::new(),
    };

    // ── the boundary, before anything is written ────────────────────────────
    //
    // The snapshot is taken first and always. A program that mutated and then
    // discovered it had no way back would be exactly the irreversible machine
    // this whole project exists to replace.
    let snapshots = Snapshots::of(volumes, &asked.subvolume);
    let opened = match attempt::begin(asked.store, &snapshots, &program.label, &asked.request_id) {
        Ok(opened) => opened,
        Err(error) => {
            evidence.reason = format!("no boundary could be opened, so nothing ran: {error}");
            metrics.machine_time_ms = started.elapsed().as_millis();
            evidence.metrics = metrics;
            return evidence;
        }
    };
    metrics.machine_operations += 1;
    evidence.snapshot = Some(opened.snapshot.clone());

    let start = thalyx_snapshot::witness(&asked.subvolume);
    evidence.start_state = start.is_complete().then(|| start.id.clone());

    // ── the work ────────────────────────────────────────────────────────────
    let boundary = here.confined_to().map(Path::to_path_buf);
    let mut refused = None;

    if let Some(source) = &program.run {
        let ran = drive(
            asked,
            &snapshots,
            &opened.snapshot,
            here,
            boundary.as_deref(),
            source,
            &mut metrics,
            &mut evidence,
        );
        // A program that did not run to the end is a program whose work is
        // half done, whatever it managed before it stopped — and that includes
        // `needs_model`, which is not a failure and is not a success either.
        // The transaction settles it the same way it settles a failed check:
        // put it back, unless the caller asked to keep it.
        if !ran {
            refused = Some(0);
        }
    } else {
        for (index, step) in program.steps.iter().enumerate() {
            metrics.machine_operations += 1;
            let answered = crate::external::one(
                asked.store,
                here,
                boundary.as_deref(),
                &step.verb,
                &step.arguments,
            );
            let (ok, answer) = match answered {
                Ok(answer) => {
                    // A verb that answered is not a verb that succeeded: every
                    // refusal on this surface is a well-formed object with
                    // `ok: false` in it, and reading "it answered" as "it worked"
                    // is how a program carries on past the edit that did not
                    // happen and validates a tree nobody changed.
                    let ok = answer.get("ok").and_then(Value::as_bool).unwrap_or(false);
                    (ok, answer)
                }
                Err(refusal) => (
                    false,
                    json!({
                        "ok": false,
                        "word": refusal.word,
                        "remedy": refusal.remedy,
                        "message": refusal.message,
                    }),
                ),
            };

            metrics.internal_bytes += answer.to_string().len();
            evidence.steps.push(StepRecord {
                verb: step.verb.clone(),
                arguments: step.arguments.clone(),
                ok,
                answer,
            });

            if !ok {
                refused = Some(index);
                break;
            }
        }
    }

    // ── what changed, before anything is decided about it ───────────────────
    //
    // Observed rather than believed. What a step *said* it changed is a claim
    // by the step; this is the tree.
    let difference = match snapshots.find(&opened.snapshot) {
        Ok(found) => thalyx_snapshot::difference(&asked.subvolume, &found.path),
        // The snapshot the boundary named is gone. Reported by the settling
        // below, which is the only place that can do anything about it; what
        // matters here is that nothing is invented for it — an empty difference
        // is "nothing changed", and this is "nobody could tell".
        Err(_) => Default::default(),
    };
    evidence.change_count =
        difference.added_total + difference.modified_total + difference.removed_total;
    evidence.changed = difference
        .added
        .iter()
        .chain(difference.modified.iter())
        .chain(difference.removed.iter())
        .take(NAMED)
        .cloned()
        .collect();
    metrics.filesystem_mutations = evidence.change_count;

    // ── validation ──────────────────────────────────────────────────────────
    //
    // Skipped entirely when a step was refused, and that is deliberate: the
    // tree is halfway through something nobody asked for, and a check run
    // against it would answer a question about a state the program never meant
    // to produce. The failure is already known.
    if refused.is_none() {
        for (index, check) in program.validate.iter().enumerate() {
            metrics.validations += 1;
            let mut record = run_check(
                asked,
                here,
                boundary.as_deref(),
                check,
                &difference,
                &mut metrics,
            );
            // Position and not content: two identical checks in a declarative
            // list are two things the caller asked for, and collapsing them
            // under one key would let the second silently answer for the
            // first. A program's repeated check is the opposite case and is
            // keyed by what it asked — see `CheckRecord::key`.
            record.key = format!("declared:{index}");
            metrics.internal_bytes += record.output.to_string().len();
            evidence.checks.push(record);
        }
    }

    // The last verdict of each distinct check, and every one of them must have
    // passed. `Passed` is never assumed for a check that did not run: a
    // `not_proven` is a failure here, which is rule 9 — a commit that believed
    // it had been checked would be a commit lying about itself.
    let mut last: std::collections::BTreeMap<&str, Verdict> = std::collections::BTreeMap::new();
    for record in &evidence.checks {
        last.insert(record.key.as_str(), record.verdict);
    }
    let everything_held =
        refused.is_none() && last.values().all(|verdict| *verdict == Verdict::Passed);

    evidence.reason = if program.run.is_some() && refused.is_some() {
        // The program's own word for how it stopped, which is the only thing
        // that distinguishes "it asked for the model" from "it threw" from "it
        // ran out of time" — three outcomes with the same effect on the tree
        // and three different next moves for whoever reads this.
        evidence
            .finish_why
            .clone()
            .unwrap_or_else(|| "the program did not run to the end".to_string())
    } else if let Some(index) = refused {
        let step = &evidence.steps[index];
        format!(
            "step {} (`{}`) was refused, so the rest of the program did not run",
            index + 1,
            step.verb
        )
    } else if everything_held {
        match evidence.checks.len() {
            0 => "every step went through; nothing was asked to be checked".to_string(),
            n => format!("every step went through and all {n} check(s) held"),
        }
    } else {
        let failed: Vec<&str> = last
            .iter()
            .filter(|(_, verdict)| **verdict != Verdict::Passed)
            .map(|(key, _)| *key)
            .collect();
        format!(
            "every step went through and {} did not hold",
            failed.join(", ")
        )
    };

    // ── commit, or put it back ──────────────────────────────────────────────
    if everything_held || program.on_failure == OnFailure::Keep {
        metrics.machine_operations += 1;
        match attempt::keep(asked.store, &snapshots, &asked.request_id) {
            Ok(_) => {
                evidence.status = if everything_held {
                    "committed".to_string()
                } else {
                    // Kept on purpose, and named differently from a commit. A
                    // caller that reads `committed` on a run whose checks
                    // failed would believe the machine agreed with it.
                    "kept_after_failure".to_string()
                };
            }
            Err(error) => {
                evidence.status = "open".to_string();
                evidence.reason = format!(
                    "{}; and the boundary could not be closed: {error}",
                    evidence.reason
                );
            }
        }
    } else {
        metrics.machine_operations += 1;
        metrics.state_witness_checks += 1;
        let plan = match attempt::what_abandoning_costs(asked.store, &snapshots) {
            Ok((_, plan)) => Some(plan),
            Err(error) => {
                evidence.status = "open".to_string();
                evidence.reason = format!(
                    "{}; and it could not be put back: {error}. The attempt is still open",
                    evidence.reason
                );
                None
            }
        };
        if let Some(plan) = plan {
            // Authorised by the state this run itself observed, not by a bare
            // yes. So a person who wrote in the shared tree between the last
            // check and this instant stops the rollback and keeps their work —
            // the same rule a caller doing this by hand is held to, applied to
            // the runtime that does it automatically.
            let authorised = Authorised::ByState(&plan.state.id);
            match attempt::abandon(
                asked.store,
                &snapshots,
                &opened,
                &plan,
                authorised,
                &asked.request_id,
            ) {
                Ok(_) => {
                    evidence.status = "rolled_back".to_string();
                    evidence.rolled_back = true;
                }
                Err(error) => {
                    evidence.status = "open".to_string();
                    evidence.reason = format!(
                        "{}; and it was NOT put back: {error}. The attempt is still open, \
                         and `intento` says what abandoning it would cost now",
                        evidence.reason
                    );
                }
            }
        }
    }

    let end = thalyx_snapshot::witness(&asked.subvolume);
    evidence.end_state = end.is_complete().then(|| end.id.clone());

    let semantics = crate::semantic::tally(&asked.subvolume);
    metrics.semantic_queries = semantics.queries.saturating_sub(semantics_before.queries);
    metrics.semantic_cache_hits = semantics.hits.saturating_sub(semantics_before.hits);
    metrics.analyzer_starts = semantics
        .analyzer_starts
        .saturating_sub(semantics_before.analyzer_starts);
    metrics.analyzer_confined = semantics.analyzer_confined;
    metrics.analyzer_how = semantics.analyzer_how.clone();
    metrics.machine_time_ms = started.elapsed().as_millis();
    evidence.metrics = metrics;
    evidence
}

// ── the programmable form ────────────────────────────────────────────────────

/// The machine, as a program is allowed to see it.
///
/// **Every field here is a borrow of something that already existed.** There is
/// no second store, no second session, no second boundary and no second
/// checker: a program's request goes through [`crate::external::one`], its
/// validation through [`run_check`], and its view of what changed through
/// `thalyx_snapshot::difference` — the same three things the static form uses,
/// called from a different place. If that were not true this would be the
/// parallel API `Agentes-Externos.md` forbids, and the workspace boundary would
/// hold for a list of steps and not for a loop.
struct Runner<'a, V: Volumes> {
    asked: &'a Asked<'a>,
    snapshots: &'a Snapshots<V>,
    snapshot: &'a str,
    here: &'a mut Where,
    boundary: Option<&'a Path>,
    metrics: &'a mut Metrics,
    /// Every validation the program asked for, in order, with what it found.
    checks: Vec<CheckRecord>,
}

impl<V: Volumes> Runner<'_, V> {
    /// What the tree really shows changed since the boundary opened.
    ///
    /// Recomputed on every call and never remembered, which is the whole point
    /// of it being available *inside* a program: after the third edit the
    /// answer is different from what it was after the second, and a program
    /// that could only see the difference at the end could not decide anything
    /// from it.
    fn difference(&self) -> thalyx_snapshot::Difference {
        match self.snapshots.find(self.snapshot) {
            Ok(found) => thalyx_snapshot::difference(&self.asked.subvolume, &found.path),
            // Rule 10, and it matters more here than in the static form: a
            // program that got an empty difference would read it as "nothing
            // changed" and commit. So this is empty *and* the settling below
            // reports the snapshot as gone, which is what stops the commit.
            Err(_) => Default::default(),
        }
    }
}

/// The two verbs a program may not reach, and why they are checked here.
///
/// `Program::read` refuses a *step* named `exec` or `attempt` before a snapshot
/// is taken, which is the right place for a list: the list is a value something
/// can look at. **A program is not.** It reaches verbs by name at runtime, so
/// there is nothing to inspect in advance and the check has to be at the
/// moment of the call.
///
/// Found by a test on 2026-08-30 that asked for `attempt abandonar` from inside
/// a program and got `ok: true` with a `confirm_with` line in it — carrying the
/// snapshot name and the exact state witness the machine had just computed. The
/// next two lines of that program would have been to send it back, and the
/// transaction would have been abandoned from inside itself, mid-run, with
/// `carry_out` still holding a boundary that no longer existed. Nothing was
/// destroyed in the test because a one-call abandon needs a state claim; what
/// the test found is that the machine hands the claim over on request.
///
/// So it is the same rule as the static form's, applied where it can be
/// applied. It is not an argument check and does not belong in `EXPOSED`: those
/// verbs are legitimately reachable by a session, and what is illegitimate is
/// reaching them *from inside the transaction they would settle*.
const NOT_FROM_INSIDE: &[&str] = &[OP, "attempt"];

impl<V: Volumes> thalyx_program::Machine for Runner<'_, V> {
    fn request(&mut self, verb: &str, arguments: &[String]) -> Value {
        self.metrics.machine_operations += 1;

        if NOT_FROM_INSIDE.contains(&verb) {
            return json!({
                "ok": false,
                "word": "not_from_inside",
                "error": "not_from_inside",
                "remedy": "let_the_program_end",
                "message": format!(
                    "`{verb}` is what this program is running *inside*. The boundary is                      opened around the whole program and settled by what the checks say;                      `on_failure` chooses which way. Return a value, or call                      `thalyx.needModel(…)`"
                ),
            });
        }
        let answer =
            crate::external::one(self.asked.store, self.here, self.boundary, verb, arguments);
        match answer {
            Ok(answer) => answer,
            // A refusal is a value and not an end. The program branches on
            // `ok`, exactly as the static form's runtime does, and a mistake it
            // can recover from does not cost a round trip to whoever wrote it.
            Err(refusal) => json!({
                "ok": false,
                "word": refusal.word,
                "error": refusal.word,
                "remedy": refusal.remedy,
                "message": refusal.message,
            }),
        }
    }

    fn validate(&mut self, asked: &Value) -> Value {
        self.metrics.validations += 1;
        self.metrics.machine_operations += 1;

        // Read as the same object the declarative `validate` list takes, so
        // there is one shape of check on this machine and not two. A program
        // that sends something else is told what a check looks like rather
        // than having one guessed for it.
        let check: Check = match serde_json::from_value(asked.clone()) {
            Ok(check) => check,
            Err(error) => {
                return json!({
                    "verdict": "not_proven",
                    "summary": format!(
                        "that is not a check this machine knows: {error}. A check is \
                         {{\"check\":\"text\"|\"parses\"|\"rust\"|\"program\", …}}"
                    ),
                });
            }
        };

        let difference = self.difference();
        let mut record = run_check(
            self.asked,
            self.here,
            self.boundary,
            &check,
            &difference,
            self.metrics,
        );
        record.key = asked.to_string();
        self.metrics.internal_bytes += record.output.to_string().len();
        let answer = json!({
            "check": record.check,
            "verdict": record.verdict.word(),
            "passed": record.verdict == Verdict::Passed,
            "summary": record.summary,
            // The whole of it, because this side of the boundary is the cheap
            // side. What crosses back to a model is whatever the program
            // decides to return, and that is a separate decision made later.
            "output": record.output.clone(),
        });
        self.checks.push(record);
        answer
    }

    fn changed(&mut self) -> Value {
        self.metrics.machine_operations += 1;
        let difference = self.difference();
        json!({
            "count": difference.added_total + difference.modified_total + difference.removed_total,
            "added": difference.added,
            "modified": difference.modified,
            "removed": difference.removed,
        })
    }

    fn process_launches(&self) -> usize {
        self.metrics.process_launches
    }

    fn verbs(&self) -> Vec<String> {
        crate::external::ExternalAgentSession::verbs()
    }
}

/// What a program may spend, on this machine.
///
/// Read from the environment so a person can loosen or tighten it without a
/// rebuild, and defaulted to [`thalyx_program::Limits`]'s own numbers. Every
/// variable is separate, which is rule 3's shape applied to resources: a
/// machine that has time to spare and not memory must be able to say which.
fn limits() -> thalyx_program::Limits {
    let mut limits = thalyx_program::Limits::default();
    let number = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
    };
    if let Some(seconds) = number("THALYX_PROGRAM_SECONDS") {
        limits.wall = std::time::Duration::from_secs(seconds.max(1));
    }
    if let Some(megabytes) = number("THALYX_PROGRAM_MEGABYTES") {
        limits.memory_bytes = (megabytes.max(1) as usize) * 1024 * 1024;
    }
    if let Some(calls) = number("THALYX_PROGRAM_CALLS") {
        limits.calls = calls.max(1) as usize;
    }
    if let Some(launches) = number("THALYX_PROGRAM_LAUNCHES") {
        limits.process_launches = launches as usize;
    }
    limits
}

/// Run one program inside the boundary, and say whether it ran to the end.
///
/// The transaction settles on the answer: `true` lets the checks decide, and
/// `false` puts the tree back unless the caller asked to keep it. Everything
/// the program did is in `evidence` either way — a run that stopped halfway is
/// the run whose record is most worth having.
#[allow(clippy::too_many_arguments)]
fn drive<V: Volumes>(
    asked: &Asked<'_>,
    snapshots: &Snapshots<V>,
    snapshot: &str,
    here: &mut Where,
    boundary: Option<&Path>,
    source: &str,
    metrics: &mut Metrics,
    evidence: &mut Evidence,
) -> bool {
    let mut runner = Runner {
        asked,
        snapshots,
        snapshot,
        here,
        boundary,
        metrics,
        checks: Vec::new(),
    };
    let outcome = thalyx_program::run(source, &mut runner, &asked.limits);
    let checks = std::mem::take(&mut runner.checks);

    // Every call the program made becomes a step, in order, with its answer
    // whole. Deliberately the same shape the static form produces, so
    // `evidencia <id> paso=N` fetches a program's ninth operation exactly as it
    // fetches a list's ninth step — one shape for "what happened", and not a
    // second one that only programs have.
    for call in &outcome.calls {
        if call.kind == "validate" {
            // Already a check. Keeping it here too would put the compiler's
            // whole output in the evidence twice.
            continue;
        }
        evidence.steps.push(StepRecord {
            verb: call.verb.clone(),
            arguments: call.arguments.clone(),
            ok: call.ok,
            answer: call.answer.clone(),
        });
    }
    evidence.checks.extend(checks);
    evidence.printed = outcome.printed.clone();
    evidence.finish = Some(outcome.finish.word().to_string());
    evidence.finish_why = Some(outcome.finish.why());
    evidence.returned = outcome.value();

    metrics.programmable = true;
    metrics.program_operations =
        outcome.metrics.requests + outcome.metrics.validations + outcome.metrics.observations;
    metrics.program_assertions = outcome.metrics.assertions;
    metrics.program_ticks = outcome.metrics.ticks;
    metrics.internal_bytes += outcome.metrics.answer_bytes;

    outcome.finish.went_through()
}

// ── validation ───────────────────────────────────────────────────────────────

/// Run one check and say what it found.
///
/// Every arm here either does the work or answers [`Verdict::NotProven`].
/// Nothing returns `Passed` without having established something, which is the
/// difference between a validating runtime and a runtime that commits.
fn run_check(
    asked: &Asked<'_>,
    here: &mut Where,
    boundary: Option<&Path>,
    check: &Check,
    difference: &thalyx_snapshot::Difference,
    metrics: &mut Metrics,
) -> CheckRecord {
    match check {
        Check::Text { text, expect, r#in } => {
            let wants_none = expect != "some";
            let mut arguments = Vec::new();
            if let Some(folder) = r#in {
                arguments.push(format!("en={folder}"));
            }
            arguments.push(text.clone());

            // Through the same door as any other request, so a check cannot
            // read a tree a step could not have read.
            metrics.machine_operations += 1;
            let answer = match crate::external::one(asked.store, here, boundary, "grep", &arguments)
            {
                Ok(answer) => answer,
                Err(refusal) => {
                    return CheckRecord {
                        key: String::new(),
                        check: format!("text `{text}` {expect}"),
                        verdict: Verdict::NotProven,
                        summary: format!("the search could not be made: {}", refusal.message),
                        output: json!({"word": refusal.word, "message": refusal.message}),
                    };
                }
            };

            let total = answer.get("total").and_then(Value::as_u64);
            let unreadable = answer
                .get("unreadable")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let verdict = match total {
                // Rule 9 and rule 10. A tree part of which could not be read
                // cannot answer "this text is nowhere": what was not read is
                // exactly where it might be. The presence question is
                // unaffected — a hit is a hit — so only the absence side is
                // withheld.
                Some(_) if unreadable > 0 && wants_none => Verdict::NotProven,
                Some(0) if wants_none => Verdict::Passed,
                Some(0) => Verdict::Failed,
                Some(_) if wants_none => Verdict::Failed,
                Some(_) => Verdict::Passed,
                None => Verdict::NotProven,
            };
            let hits = total.unwrap_or(0);
            CheckRecord {
                key: String::new(),
                check: format!("text `{text}` {expect}"),
                verdict,
                summary: match verdict {
                    Verdict::NotProven if unreadable > 0 => format!(
                        "{unreadable} path(s) could not be read, so `{text}` cannot be \
                         said to be absent"
                    ),
                    Verdict::NotProven => "the search answered nothing countable".to_string(),
                    _ => format!("{hits} occurrence(s) of `{text}`"),
                },
                output: answer,
            }
        }

        Check::Parses => {
            let mut looked_at = 0usize;
            let mut broken = Vec::new();
            for name in difference.added.iter().chain(difference.modified.iter()) {
                let path = asked.subvolume.join(name);
                let Some(language) = thalyx_parser::Language::from_path(&path) else {
                    continue;
                };
                let Ok(source) = std::fs::read_to_string(&path) else {
                    // Rule 10, and it is a hole in the check rather than a
                    // failure of the file: say so where the caller can see it.
                    broken.push(json!({"path": name, "why": "could not be read"}));
                    continue;
                };
                looked_at += 1;
                if let Some(why) = thalyx_parser::unbalanced(language, &source) {
                    broken.push(json!({"path": name, "why": why}));
                }
            }
            let verdict = if !broken.is_empty() {
                Verdict::Failed
            } else if looked_at == 0 {
                // Nothing this parser understands changed. Not a pass: a check
                // that examined nothing has established nothing, and a commit
                // that believed it had been checked would be a commit lying
                // about itself.
                Verdict::NotProven
            } else {
                Verdict::Passed
            };
            CheckRecord {
                key: String::new(),
                check: "parses".to_string(),
                verdict,
                summary: match verdict {
                    Verdict::Passed => format!("{looked_at} changed file(s) still parse"),
                    Verdict::Failed => format!("{} changed file(s) do not parse", broken.len()),
                    Verdict::NotProven => {
                        "no changed file is in a language this machine parses".to_string()
                    }
                },
                output: json!({"looked_at": looked_at, "broken": broken}),
            }
        }

        Check::Program { program, arguments } => {
            // Nothing added to its environment. A check that names a program
            // has named a program, and telling it where a Rust toolchain is
            // would be this verb deciding what somebody else's binary is for.
            let outcome = run_confined(asked, program, arguments, &[], &[], &[], metrics);
            CheckRecord {
                key: String::new(),
                check: format!("program `{program}`"),
                verdict: outcome.verdict,
                summary: outcome.summary,
                output: outcome.output,
            }
        }

        Check::Rust { mode, packages } => rust_check(asked, difference, mode, packages, metrics),
    }
}

/// What a confined run came back with, in the two shapes a check needs.
struct Ran {
    verdict: Verdict,
    summary: String,
    output: Value,
}

/// Start a program under the confinement a program nobody signed gets, and read
/// its exit status.
///
/// **This is `ejecutar`'s path and not a new one.** `thalyx_core::foreign`
/// resolves the binary, refuses when nothing can enforce, gives it its own user,
/// its own cgroup, its own root filesystem, the seccomp filter, and the grants
/// named here and nothing else. A validation that shelled out would be this
/// crate becoming a host shell, which is the one thing `Agentes-Externos.md`
/// says the adapter side must never become — and it would be worse here, on the
/// authority side, where it would be Thalyx handing out its own reach.
///
/// It refuses on a machine whose kernel is not denying, and that refusal
/// arrives as [`Verdict::NotProven`]: **a check that could not run is not a
/// check that passed.** This container is such a machine, which is why the
/// tests that exercise this arm say so out loud rather than pretending.
fn run_confined(
    asked: &Asked<'_>,
    program: &str,
    arguments: &[String],
    also_readable: &[PathBuf],
    also_writable: &[PathBuf],
    environment: &[(String, String)],
    metrics: &mut Metrics,
) -> Ran {
    use thalyx_manifest::{Permission, PermissionKind};

    let path = PathBuf::from(program);
    if !path.is_absolute() {
        return Ran {
            verdict: Verdict::NotProven,
            summary: format!(
                "`{program}` is not an absolute path, and this machine has no search \
                 path to look one up on"
            ),
            output: json!({"word": "not_absolute"}),
        };
    }

    let mut grants = vec![
        // The workspace, both ways: a build writes into the tree it builds.
        Permission {
            resource: asked.subvolume.display().to_string(),
            action: "read".to_string(),
            kind: PermissionKind::Session,
        },
        Permission {
            resource: asked.subvolume.display().to_string(),
            action: "write".to_string(),
            kind: PermissionKind::Session,
        },
    ];
    for extra in also_readable {
        grants.push(Permission {
            resource: extra.display().to_string(),
            action: "read".to_string(),
            kind: PermissionKind::Session,
        });
    }
    // A place to build that is not the workspace. Both ways, because a build
    // directory is written and then read back.
    for extra in also_writable {
        for action in ["read", "write"] {
            grants.push(Permission {
                resource: extra.display().to_string(),
                action: action.to_string(),
                kind: PermissionKind::Session,
            });
        }
    }

    metrics.process_launches += 1;
    metrics.machine_operations += 1;
    let outcome = thalyx_core::foreign::run_foreign(
        asked.store,
        &thalyx_permd::KernelStore::default_map(),
        thalyx_core::foreign::ForeignRequest {
            program: &path,
            args: arguments.iter().map(std::ffi::OsString::from).collect(),
            grants,
            helper: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("thalyx")),
            request_id: asked.request_id.clone(),
            profile: thalyx_sandbox::profile::MODULE_STANDARD,
            environment: environment.to_vec(),
        },
    );

    match outcome {
        Ok(outcome) => {
            metrics.internal_bytes += outcome.wrote.stdout.len() + outcome.wrote.stderr.len();
            let verdict = match outcome.exit_code {
                Some(0) => Verdict::Passed,
                Some(_) => Verdict::Failed,
                // A signal, and under this profile the likeliest one is
                // `SIGSYS`: the program tried something the filter denies. That
                // is a fact about the confinement, not about the code, so it is
                // never reported as the code failing.
                None => Verdict::NotProven,
            };
            Ran {
                verdict,
                summary: match outcome.exit_code {
                    Some(0) => format!("`{program}` exited 0"),
                    Some(code) => format!("`{program}` exited {code}"),
                    None => format!(
                        "`{program}` was killed by a signal rather than exiting; under \
                         this profile that is most often a denied syscall"
                    ),
                },
                output: json!({
                    "exit_code": outcome.exit_code,
                    "stdout": outcome.wrote.stdout,
                    "stderr": outcome.wrote.stderr,
                    "truncated": outcome.wrote.truncated,
                    "cgroup": outcome.cgroup_id,
                    "isolated": outcome.isolated,
                }),
            }
        }
        Err(error) => Ran {
            verdict: Verdict::NotProven,
            summary: format!("`{program}` could not be run under confinement: {error}"),
            output: json!({"word": "could_not_run", "message": error.to_string()}),
        },
    }
}

// ── the Rust vertical ────────────────────────────────────────────────────────

/// The packages the changed files belong to, and `cargo` over exactly those.
///
/// **Package scope, not test selection.** Working out which *tests* a change
/// could affect needs a call graph that survives macros, generics and trait
/// objects, and getting it wrong in the optimistic direction means a green run
/// that proved nothing. The scope that is both cheap and sound is the package:
/// a change inside a crate can only change that crate's behaviour and the
/// behaviour of what depends on it, and `cargo` already knows the second half.
///
/// The scope is read from the manifests on disk rather than from `cargo
/// metadata`, because reading a file is not running a program: a scope that
/// needed a process to compute could not be computed on a machine where the
/// process cannot run, and this then could not say *which* package it failed
/// to check.
fn rust_check(
    asked: &Asked<'_>,
    difference: &thalyx_snapshot::Difference,
    mode: &str,
    named: &[String],
    metrics: &mut Metrics,
) -> CheckRecord {
    let subcommand = if mode == "test" { "test" } else { "check" };
    let changed: Vec<String> = difference
        .added
        .iter()
        .chain(difference.modified.iter())
        .chain(difference.removed.iter())
        .cloned()
        .collect();

    let Some(selection) =
        crate::semantic::selection(asked.store.root(), &asked.subvolume, &changed, named)
    else {
        return CheckRecord {
            key: String::new(),
            check: format!("cargo {subcommand}"),
            verdict: Verdict::NotProven,
            summary: "this workspace is not one Cargo can describe, so there is nothing \
                      this could have compiled"
                .to_string(),
            output: json!({"word": "not_a_cargo_workspace", "cached": false}),
        };
    };
    let packages = selection.packages;
    metrics.affected_packages = packages.len();

    if packages.is_empty() {
        return CheckRecord {
            key: String::new(),
            check: format!("cargo {subcommand}"),
            verdict: Verdict::NotProven,
            summary: format!("nothing was compiled: {}", selection.why),
            output: json!({
                "packages": packages,
                "why": selection.why,
                "cached": false,
                // Named rather than dropped. A changed file nobody could place
                // is a reason this check might not cover the change, and a
                // caller that only saw "no packages" would read it as a clean
                // check of nothing.
                "unattributed": selection.unattributed,
            }),
        };
    }

    // ── has this exact state already been checked? ──────────────────────────
    let key = format!("cargo {subcommand}|{}", packages.join(","));
    if let Some(identity) = &selection.identity
        && let Some(remembered) =
            crate::semantic::recall_validation(asked.store.root(), &asked.subvolume, &key, identity)
        && let Ok(record) = serde_json::from_str::<Remembered>(&remembered)
    {
        metrics.validation_cache_hits += 1;
        return CheckRecord {
            key: String::new(),
            check: format!("cargo {subcommand} over {}", packages.join(", ")),
            verdict: record.verdict,
            // Said out loud. A caller told a check passed, when what happened
            // is that the same bytes passed an hour ago, is entitled to know
            // which of the two it is being told.
            summary: format!(
                "{} (reused: these exact bytes were already checked)",
                record.summary
            ),
            output: json!({
                "packages": packages,
                "why": selection.why,
                "cached": true,
                "state": identity.id,
            }),
        };
    }
    metrics.validation_cache_misses += 1;

    let found = thalyx_rust::toolchain::cargo();
    let Some(cargo) = &found.path else {
        // Rule 10, in the place it matters most: a toolchain that is not here
        // is not a change that does not compile.
        //
        // It says *where it looked*, and that is the whole of the 2026-08-29
        // failure: `sudo` had made `$HOME` be `/root`, the search found
        // nothing, and the sentence gave nobody a way to notice that the
        // toolchain was one directory away under `$SUDO_USER`'s home.
        return CheckRecord {
            key: String::new(),
            check: format!("cargo {subcommand}"),
            verdict: Verdict::NotProven,
            summary: found.why_not(
                "`cargo`",
                "So the change was not compiled. Name one with THALYX_CARGO, or run \
                 with RUSTUP_HOME and CARGO_HOME set",
            ),
            output: json!({
                "packages": packages,
                "word": "no_cargo",
                // Present on every answer this check gives, whichever way it
                // went. A field that disappears when a tool is missing is a
                // field every caller has to write two readings of — and on
                // 2026-08-29 it is what turned "there is no cargo here" into
                // two assertions failing about a cache.
                "cached": false,
                "looked_at": found.looked_at.iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<String>>(),
            }),
        };
    };

    // Built outside the workspace, and it is not tidiness. A `target/` inside
    // the tree is inside the snapshot: the boundary would copy a build tree,
    // the difference would report thousands of changed files, and a rollback
    // would throw away the build cache that makes the *next* check cheap. It is
    // the same reason the provider tells rust-analyzer where to build.
    let build_into = crate::semantic::build_directory(asked.store.root(), &asked.subvolume);
    let mut arguments = vec![
        subcommand.to_string(),
        // No network from inside the confinement, so a build that wanted to
        // fetch would fail as a network error and read as broken code.
        "--offline".to_string(),
        "--target-dir".to_string(),
        build_into.display().to_string(),
        "--manifest-path".to_string(),
        asked.subvolume.join("Cargo.toml").display().to_string(),
    ];
    for package in &packages {
        arguments.push("-p".to_string());
        arguments.push(package.clone());
    }

    // The toolchain, read-only. Without these `cargo` cannot find `rustc` or
    // the registry it already downloaded, and the run would fail for a reason
    // that has nothing to do with the change.
    //
    // Asked of `toolchain::readable` rather than assembled here: a grant list
    // written twice is two grant lists, and the second one is always missing
    // the entry that only matters on somebody else's machine — which under
    // `sudo` is every entry, because they are all under a home this process
    // does not have.
    let readable = thalyx_rust::toolchain::readable();

    // Made here rather than left to Cargo: the grant names a path, and a
    // grant on a directory that does not exist yet is a grant on nothing.
    let _ = std::fs::create_dir_all(&build_into);
    // Where the toolchain is, said rather than assumed. Under `sudo` the
    // process running this has a `HOME` with no `.cargo` in it, and a Cargo
    // that cannot find its registry fails `--offline` — which arrives as the
    // change not compiling.
    let environment: Vec<(String, String)> = thalyx_rust::toolchain::environment()
        .into_iter()
        .map(|(name, path)| (name.to_string(), path.display().to_string()))
        .collect();
    let outcome = run_confined(
        asked,
        &cargo.display().to_string(),
        &arguments,
        &readable,
        std::slice::from_ref(&build_into),
        &environment,
        metrics,
    );

    // A verdict about the tree is remembered; `not_proven` never is. A machine
    // that once had no cargo would otherwise go on reporting `not_proven`
    // about bytes it never compiled, for as long as nobody touched them.
    if let Some(identity) = &selection.identity
        && outcome.verdict != Verdict::NotProven
        && let Ok(text) = serde_json::to_string(&Remembered {
            verdict: outcome.verdict,
            summary: outcome.summary.clone(),
        })
    {
        crate::semantic::remember_validation(
            asked.store.root(),
            &asked.subvolume,
            &key,
            identity,
            &text,
        );
    }

    CheckRecord {
        key: String::new(),
        check: format!("cargo {subcommand} over {}", packages.join(", ")),
        verdict: outcome.verdict,
        summary: outcome.summary,
        output: {
            let mut output = outcome.output;
            if let Some(object) = output.as_object_mut() {
                object.insert("packages".to_string(), json!(packages));
                object.insert("why".to_string(), json!(selection.why));
                object.insert("unattributed".to_string(), json!(selection.unattributed));
                object.insert("cached".to_string(), json!(false));
            }
            output
        },
    }
}

/// What is kept about a check that has already been run.
///
/// The verdict and one line, and never the compiler's output: the output is
/// megabytes, it is already in the evidence of the run that produced it, and a
/// cache that carried it would be a second copy of the thing this whole system
/// keeps out of the model's context.
#[derive(Serialize, Deserialize)]
struct Remembered {
    verdict: Verdict,
    summary: String,
}

// ── the two faces ────────────────────────────────────────────────────────────

/// Cut a stream to something an answer can carry, and say that it was cut.
fn shortened(text: &str) -> Value {
    let trimmed = text.trim();
    if trimmed.len() <= SUMMARY {
        return json!({"text": trimmed, "cut": false, "bytes": text.len()});
    }
    // The tail rather than the head. A compiler prints its errors and then a
    // summary line, and the head of a long build log is the part that went
    // right — which is the half a caller never needs.
    let tail: String = trimmed
        .char_indices()
        .rev()
        .take_while(|(index, _)| trimmed.len() - index <= SUMMARY)
        .map(|(_, c)| c)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    json!({"text": tail, "cut": true, "bytes": text.len()})
}

/// The answer that goes back to whoever asked, and nothing more than that.
///
/// The whole argument of this verb is in what this function leaves out. Every
/// step's answer, every line a compiler printed, every hit of every search —
/// all of it is in the evidence, none of it is here, and the handle is how to
/// ask for the part that turns out to matter.
pub fn answer_object(evidence: &Evidence) -> Vec<(&'static str, Value)> {
    let checks: Vec<Value> = evidence
        .checks
        .iter()
        .map(|record| {
            let mut carried = json!({
                "check": record.check,
                "verdict": record.verdict.word(),
                "summary": record.summary,
            });
            // The one place a check's own words come back, and only for the
            // ones that did not hold: a caller that has to fetch the evidence
            // to learn *why* a rollback happened has paid a round trip for the
            // one fact it always needs.
            if record.verdict != Verdict::Passed
                && let Some(object) = carried.as_object_mut()
            {
                for stream in ["stdout", "stderr"] {
                    if let Some(text) = record.output.get(stream).and_then(Value::as_str)
                        && !text.trim().is_empty()
                    {
                        object.insert(stream.to_string(), shortened(text));
                    }
                }
            }
            carried
        })
        .collect();

    let failed_step = evidence
        .steps
        .iter()
        .enumerate()
        .find(|(_, record)| !record.ok)
        .map(|(index, record)| {
            json!({
                "at": index + 1,
                "verb": record.verb,
                "word": record.answer.get("word").cloned().unwrap_or(Value::Null),
                "message": record.answer.get("message").cloned().unwrap_or(Value::Null),
            })
        });

    let mut carried = vec![
        ("status", json!(evidence.status)),
        ("transaction", json!(evidence.transaction)),
        ("attempt", json!(evidence.label)),
        ("snapshot", json!(evidence.snapshot)),
        ("start_state", json!(evidence.start_state)),
        ("end_state", json!(evidence.end_state)),
        ("steps_run", json!(evidence.steps.len())),
        ("failed_step", json!(failed_step)),
        ("change_count", json!(evidence.change_count)),
        ("changed_files", json!(evidence.changed)),
        (
            "changed_files_cut",
            json!(evidence.change_count > evidence.changed.len()),
        ),
        ("validations", json!(checks)),
        ("rolled_back", json!(evidence.rolled_back)),
        ("reason", json!(evidence.reason)),
        // The handle, in every answer including the ones that went well. A
        // caller that only gets it on failure cannot audit a success.
        ("evidence", json!(evidence.transaction)),
        (
            "evidence_with",
            json!(format!("evidencia {}", evidence.transaction)),
        ),
        (
            "machine_operations",
            json!(evidence.metrics.machine_operations),
        ),
        (
            "external_requests",
            json!(evidence.metrics.external_requests),
        ),
        ("process_launches", json!(evidence.metrics.process_launches)),
        (
            "filesystem_mutations",
            json!(evidence.metrics.filesystem_mutations),
        ),
        (
            "state_witness_checks",
            json!(evidence.metrics.state_witness_checks),
        ),
        ("machine_time_ms", json!(evidence.metrics.machine_time_ms)),
        ("internal_bytes", json!(evidence.metrics.internal_bytes)),
        // The programming face's own numbers. Zeroes on a run that asked
        // nothing semantic, and said anyway: a field that only appears on the
        // interesting day is a field nobody handles on the interesting day.
        ("semantic_queries", json!(evidence.metrics.semantic_queries)),
        (
            "semantic_cache_hits",
            json!(evidence.metrics.semantic_cache_hits),
        ),
        ("analyzer_starts", json!(evidence.metrics.analyzer_starts)),
        (
            "analyzer_confined",
            json!(evidence.metrics.analyzer_confined),
        ),
        ("analyzer_how", json!(evidence.metrics.analyzer_how)),
        (
            "validation_cache_hits",
            json!(evidence.metrics.validation_cache_hits),
        ),
        (
            "validation_cache_misses",
            json!(evidence.metrics.validation_cache_misses),
        ),
        (
            "affected_packages",
            json!(evidence.metrics.affected_packages),
        ),
    ];

    // The programmable form's own fields, and only on a run that was one. This
    // is the one place in this file where a field is conditional, and the
    // reason is that `steps_run` already means something for a list of steps:
    // `returned` on a run that had no program would be a null every caller has
    // to special-case, and `finish` would be a word about something that did
    // not happen.
    if evidence.metrics.programmable {
        carried.extend([
            ("finish", json!(evidence.finish)),
            ("finish_why", json!(evidence.finish_why)),
            // **What the program handed back.** The whole of the compression is
            // that this is small and everything it was computed from is not:
            // the program read whole files, walked every reference and ran the
            // compiler, and what crosses back is whatever it decided mattered.
            ("returned", evidence.returned.clone()),
            (
                "program_operations",
                json!(evidence.metrics.program_operations),
            ),
            (
                "program_assertions",
                json!(evidence.metrics.program_assertions),
            ),
            ("program_ticks", json!(evidence.metrics.program_ticks)),
            ("printed", json!(evidence.printed)),
        ]);
    }

    carried
}

/// `hacer <programa>` — the verb, in whichever face is asking.
pub fn run(store: &Store, here: &mut Where, rest: &str, face: Face, request_id: &str) -> Fallible {
    // A program is JSON, and JSON is made of double quotes — which is exactly
    // what `words.rs` takes off a word. So a person typing `hacer {"steps":…}`
    // at a prompt would have every quote in it eaten and be told the object is
    // not an object, which is a true sentence about something Thalyx did to it.
    //
    // JSON delimits itself, so when the line already begins with `{` there is
    // nothing to split and it is taken byte for byte. Anything else goes
    // through the words, which is how the bridge sends one — `compose` puts
    // every argument in single quotes, and inside those the double quotes are
    // literal and come back whole.
    let trimmed = rest.trim();
    let read_as_words;
    let text = if trimmed.starts_with('{') {
        trimmed
    } else {
        let Some(given) = crate::words::asked(face, OP, rest) else {
            return Ok(());
        };
        read_as_words = given;
        read_as_words
            .first()
            .map(|word| word.as_str())
            .unwrap_or("")
            .trim()
    };
    if text.is_empty() {
        declined(
            face,
            "nothing_asked",
            "which program — `hacer '{\"steps\":[{\"verb\":\"edit\",\"arguments\":[…]}]}'`. \
             `describe hacer` says the whole shape",
        );
        return Ok(());
    }

    let program = match Program::read(text) {
        Ok(program) => program,
        Err(why) => {
            declined(face, "unintelligible", &why);
            return Ok(());
        }
    };

    // The same rule `intento` is held to, and reached through the same
    // function: where the session stands, exactly, or nothing. A verb that
    // could replace a whole subvolume must never choose which one by searching
    // — 2026-08-10, and the read-only snapshot of somebody's entire root
    // filesystem that came of it.
    let subvolume = match crate::attempt::subvolume_for(here.at()) {
        Ok(subvolume) => subvolume,
        Err(why) => {
            declined(face, why.word(), &why.message(here.at()));
            return Ok(());
        }
    };

    let asked = Asked {
        store,
        limits: limits(),
        subvolume,
        request_id: request_id.to_string(),
    };
    let evidence = carry_out(&asked, thalyx_snapshot::Native, here, &program);

    // Kept before it is answered. A caller handed a handle that names nothing
    // has been handed a lie, and the failure it would meet is a second call.
    let kept = keep(store, &evidence);

    if face == Face::Machine {
        let mut carried = answer_object(&evidence);
        // Measured on the object that actually leaves, not estimated. The
        // whole claim of this verb is a ratio between this and
        // `internal_bytes`, and a numerator nobody weighed is a ratio nobody
        // can check.
        let leaving: usize = carried
            .iter()
            .map(|(name, value)| name.len() + value.to_string().len() + 4)
            .sum();
        carried.push(("returned_bytes", json!(leaving)));
        if let Err(error) = &kept {
            carried.push(("evidence_kept", json!(false)));
            carried.push(("evidence_why_not", json!(error.to_string())));
        } else {
            carried.push(("evidence_kept", json!(true)));
        }
        face.say(thalyx_files::machine::answer(OP, carried));
        return Ok(());
    }

    println!();
    println!("  `{}` — {}", evidence.label, evidence.status);
    println!("  {}", evidence.reason);
    println!();
    println!(
        "  {} step(s), {} check(s), {} file(s) changed, {} operation(s) inside the machine",
        evidence.steps.len(),
        evidence.checks.len(),
        evidence.change_count,
        evidence.metrics.machine_operations
    );
    for record in &evidence.checks {
        println!("    {} — {}", record.verdict.word(), record.summary);
    }
    if evidence.rolled_back {
        println!();
        println!("  The workspace is back as it was.");
    }
    println!();
    println!("  `evidencia {}` has all of it.", evidence.transaction);
    println!();
    Ok(())
}

/// `evidencia <id> [paso=N]` — the raw material one run produced.
///
/// The other half of the compression. What the answer left out is here, and it
/// is fetched by a caller that decided it needs it rather than pushed at one
/// that did not.
pub fn evidence(store: &Store, rest: &str, face: Face) -> Fallible {
    const OP: &str = "evidence";

    let Some(given) = crate::words::asked(face, OP, rest) else {
        return Ok(());
    };
    let mut id = None;
    let mut step = None;
    for word in given.iter().map(crate::words::Word::as_str) {
        match word.split_once('=') {
            Some(("paso" | "step", value)) => match value.parse::<usize>() {
                Ok(number) => step = Some(number),
                Err(_) => {
                    say(
                        face,
                        OP,
                        "incomplete",
                        &format!("`paso=` takes a number, and was given `{value}`"),
                    );
                    return Ok(());
                }
            },
            // Refused rather than ignored. A caller that misspelled `paso=`
            // and got the whole run back would read that as "this run has one
            // step", which is a wrong belief about the evidence rather than a
            // wrong call — and the same argument the abandon's parser makes for
            // refusing a malformed count.
            Some((other, _)) => {
                say(
                    face,
                    OP,
                    "unknown_argument",
                    &format!("`{other}=` is not something this verb takes; it takes `paso=N`"),
                );
                return Ok(());
            }
            None if id.is_none() => id = Some(word.to_string()),
            None => {
                say(
                    face,
                    OP,
                    "unknown_argument",
                    &format!(
                        "`{word}` is a second run to fetch, and this verb fetches one: \
                         `evidencia <id> [paso=N]`"
                    ),
                );
                return Ok(());
            }
        }
    }

    let Some(id) = id else {
        say(
            face,
            OP,
            "nothing_asked",
            "which run — `evidencia <id>`, the id every `hacer` answers with",
        );
        return Ok(());
    };
    if !is_a_handle(&id) {
        // Refused on its shape, before it is joined onto anything. A handle
        // arrives from outside and `../../etc/passwd` is a string like any
        // other.
        say(
            face,
            OP,
            "not_a_handle",
            &format!("`{id}` is not the shape of a handle this machine issues"),
        );
        return Ok(());
    }

    let path = evidence_path(store, &id);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            say(
                face,
                OP,
                "absent",
                &format!("no run called `{id}` was kept here"),
            );
            return Ok(());
        }
        // Rule 10: a failure to read is not a failure to exist, and the two
        // have different remedies.
        Err(error) => {
            say(
                face,
                OP,
                "unreadable",
                &format!("`{id}` is there and could not be read: {error}"),
            );
            return Ok(());
        }
    };
    let record: Evidence = match serde_json::from_str(&raw) {
        Ok(record) => record,
        Err(error) => {
            say(
                face,
                OP,
                "unreadable",
                &format!("`{id}` could not be understood: {error}"),
            );
            return Ok(());
        }
    };

    if let Some(number) = step {
        let Some(one) = number
            .checked_sub(1)
            .and_then(|index| record.steps.get(index))
        else {
            say(
                face,
                OP,
                "absent",
                &format!(
                    "this run has {} step(s), and `paso={number}` names none of them",
                    record.steps.len()
                ),
            );
            return Ok(());
        };
        if face == Face::Machine {
            face.say(thalyx_files::machine::answer(
                OP,
                vec![
                    ("transaction", json!(record.transaction)),
                    ("step", json!(number)),
                    ("verb", json!(one.verb)),
                    ("arguments", json!(one.arguments)),
                    ("ok", json!(one.ok)),
                    ("answer", one.answer.clone()),
                ],
            ));
        } else {
            println!();
            println!(
                "  step {number}: {} — {}",
                one.verb,
                if one.ok { "ok" } else { "refused" }
            );
            println!("  {}", one.answer);
            println!();
        }
        return Ok(());
    }

    if face == Face::Machine {
        face.say(thalyx_files::machine::answer(
            OP,
            vec![
                ("transaction", json!(record.transaction)),
                ("attempt", json!(record.label)),
                ("at", json!(record.at)),
                ("workspace", json!(record.workspace)),
                ("status", json!(record.status)),
                ("reason", json!(record.reason)),
                ("rolled_back", json!(record.rolled_back)),
                ("start_state", json!(record.start_state)),
                ("end_state", json!(record.end_state)),
                ("change_count", json!(record.change_count)),
                ("changed_files", json!(record.changed)),
                (
                    "steps",
                    json!(
                        record
                            .steps
                            .iter()
                            .enumerate()
                            .map(|(index, one)| json!({
                                "step": index + 1,
                                "verb": one.verb,
                                "arguments": one.arguments,
                                "ok": one.ok,
                                // The answers themselves are behind `paso=`.
                                // A caller asking "what happened" wants the
                                // shape of it; one asking "what did step 4
                                // say" asks for step 4.
                                "answer_bytes": one.answer.to_string().len(),
                            }))
                            .collect::<Vec<_>>()
                    ),
                ),
                (
                    "checks",
                    json!(
                        record
                            .checks
                            .iter()
                            .map(|one| json!({
                                "check": one.check,
                                "verdict": one.verdict.word(),
                                "summary": one.summary,
                                "output": one.output,
                            }))
                            .collect::<Vec<_>>()
                    ),
                ),
                (
                    "metrics",
                    serde_json::to_value(&record.metrics).unwrap_or(Value::Null),
                ),
            ],
        ));
        return Ok(());
    }

    println!();
    println!(
        "  {} — {} ({})",
        record.transaction, record.status, record.at
    );
    println!("  {}", record.reason);
    println!();
    for (index, one) in record.steps.iter().enumerate() {
        println!(
            "    {}. {} — {}",
            index + 1,
            one.verb,
            if one.ok { "ok" } else { "refused" }
        );
    }
    for one in &record.checks {
        println!("    {} — {}", one.verdict.word(), one.summary);
    }
    println!();
    Ok(())
}

fn declined(face: Face, word: &str, why: &str) {
    say(face, OP, word, why);
}

fn say(face: Face, op: &str, word: &str, why: &str) {
    if face == Face::Machine {
        face.say(thalyx_files::machine::declined(op, word, why));
    } else {
        println!("\n  {why}\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_snapshot::directories::Directories;

    /// A workspace, a store, and a session standing in it under the boundary an
    /// external agent gets.
    ///
    /// The boundary is real: `Confinement::of` opens the workspace and every
    /// step below is checked against it by the same function a single request
    /// is checked by. A fixture that left it off would be testing a program
    /// running with the person's authority, which is not the case this verb
    /// exists for.
    ///
    /// The snapshots are directory-backed, which is this project's standing
    /// split: naming, ordering, what a rollback aims at and what authorises it
    /// are not Btrfs questions, and policy that can only be exercised on Btrfs
    /// is policy that is never exercised. What is **not** proven here is that
    /// the snapshot is atomic and free — that is Cesar's machine, and
    /// `dev/verify.sh` says so where it runs.
    fn a_workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Store, PathBuf, Where) {
        let base = tempfile::tempdir().expect("a temp dir");
        let store = Store::open(base.path().join("store")).expect("a store");
        let tree = base.path().join("work");
        Directories::make_subvolume(&tree).expect("a subvolume");
        for (path, text) in files {
            let full = tree.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("the parent");
            }
            std::fs::write(&full, text).expect("the file");
        }
        let real = std::fs::canonicalize(&tree).expect("a real path");

        let mut here = Where::start();
        here.confine(crate::confine::Confinement::of(&real).expect("a boundary"));
        here.go(&real.to_string_lossy()).expect("standing in it");
        (base, store, real, here)
    }

    fn program(json: serde_json::Value) -> Program {
        Program::read(&json.to_string()).expect("a program this verb can read")
    }

    fn run(store: &Store, tree: &Path, here: &mut Where, program: &Program) -> Evidence {
        run_within(
            store,
            tree,
            here,
            program,
            thalyx_program::Limits::default(),
        )
    }

    /// The same, with a ceiling of this test's own.
    fn run_within(
        store: &Store,
        tree: &Path,
        here: &mut Where,
        program: &Program,
        limits: thalyx_program::Limits,
    ) -> Evidence {
        carry_out(
            &Asked {
                store,
                limits,
                subvolume: tree.to_path_buf(),
                request_id: format!("t-{}", std::process::id()),
            },
            Directories,
            here,
            program,
        )
    }

    /// A workspace that is a real Cargo package, for the checks that ask Cargo
    /// and the compiler frontend about it.
    ///
    /// A real one — manifest, `src/`, a name used across two files — because
    /// rule 8 applies to a workspace as much as to a fake: a directory with
    /// `.rs` files in it that Cargo cannot describe is not a small Cargo
    /// project, it is a different system.
    fn a_crate() -> (tempfile::TempDir, Store, PathBuf, Where) {
        a_workspace(&[
            (
                "Cargo.toml",
                "[workspace]\n\n[package]\nname = \"vertical\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("src/lib.rs", "pub mod boot;\npub mod keystore;\n"),
            (
                "src/keystore.rs",
                "pub struct Keystore;\n\npub fn unlock() -> Keystore {\n    Keystore\n}\n",
            ),
            (
                "src/boot.rs",
                "use crate::keystore::Keystore as Keys;\n\npub fn boot() -> Keys {\n    crate::keystore::unlock()\n}\n",
            ),
        ])
    }

    /// Rule 3, inside the crate: a machine with no rust-analyzer says so, and
    /// `THALYX_REQUIRE_RUST_ANALYZER=1` turns the skip into a failure.
    fn analyzer_or_skip(what: &str) -> bool {
        if thalyx_rust::analyzer::find().is_some() {
            return true;
        }
        let message = format!(
            "NOT PROVEN: {what} — there is no rust-analyzer on this machine. \
             Set THALYX_REQUIRE_RUST_ANALYZER=1 to make this a failure."
        );
        assert!(
            std::env::var("THALYX_REQUIRE_RUST_ANALYZER").as_deref() != Ok("1"),
            "{message}"
        );
        eprintln!("{message}");
        false
    }

    #[test]
    fn one_request_resolves_a_symbol_edits_every_use_and_commits() {
        // **The vertical.** One call in. Inside it: a boundary, a compiler
        // frontend asked what `Keystore` is, every use of it found — including
        // the one three files away that is spelled `Keys` — two files
        // rewritten, the tree observed, two checks run, a commit. Out comes one
        // small answer.
        //
        // What makes it worth having is not that it is possible. It is that
        // *no* frontier-model inference happens between any two of those.
        if !analyzer_or_skip("that one request resolves, edits and commits") {
            return;
        }
        let (_base, store, tree, mut here) = a_crate();
        crate::semantic::release(&tree);

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "rename Keystore",
                "steps": [
                    {"verb": "rename", "arguments": ["Keystore", "KeyVault"]},
                    // A read whose answer the model never sees, in the same
                    // call, from the tree the rename has just produced.
                    {"verb": "read", "arguments": ["src/boot.rs"]}
                ],
                "validate": [
                    {"check": "text", "text": "Keystore", "expect": "none"},
                    {"check": "text", "text": "KeyVault", "expect": "some"},
                    {"check": "parses"}
                ]
            })),
        );

        assert_eq!(evidence.status, "committed", "{}", evidence.reason);
        assert!(!evidence.rolled_back);

        let keystore = std::fs::read_to_string(tree.join("src/keystore.rs")).expect("keystore");
        assert!(keystore.contains("pub struct KeyVault;"), "{keystore}");
        let boot = std::fs::read_to_string(tree.join("src/boot.rs")).expect("boot");
        assert!(
            boot.contains("use crate::keystore::KeyVault as Keys;"),
            "the import three files away is the whole claim, and it says:\n{boot}"
        );
        assert!(
            boot.contains("-> Keys"),
            "`Keys` is a different name and must not have been touched:\n{boot}"
        );

        assert_eq!(evidence.metrics.external_requests, 1);
        assert!(
            evidence.metrics.machine_operations >= 4,
            "the boundary, two steps and the settling, at least: {:?}",
            evidence.metrics
        );
        assert!(
            evidence.metrics.semantic_queries >= 1,
            "the rename was supposed to resolve a name, and asked nothing: {:?}",
            evidence.metrics
        );
        assert_eq!(
            evidence.metrics.analyzer_starts, 1,
            "one start for the whole request. More than one means the provider \
             is being rebuilt per question, which is 25 seconds each: {:?}",
            evidence.metrics
        );
        // Named rather than counted: `cargo metadata`, run inside the
        // boundary by the provider, writes a `Cargo.lock` into a workspace that
        // has not got one — a real change, made inside the transaction, and one
        // a count would have silently absorbed.
        for name in ["src/keystore.rs", "src/boot.rs"] {
            assert!(
                evidence.changed.iter().any(|changed| changed == name),
                "{name} is not among what the tree says changed: {:?}",
                evidence.changed
            );
        }
        assert!(
            evidence.metrics.filesystem_mutations <= 3,
            "two files and at most a lockfile: {:?}",
            evidence.changed
        );
    }

    #[test]
    fn the_rust_check_selects_the_crates_the_change_reaches() {
        // The container cannot run the compiler under confinement, so the
        // verdict here is `not_proven` and says so. What *is* proven is the
        // half this change is about: which crates the machine decided to
        // compile, derived from Cargo's graph rather than from the model.
        let (_base, store, tree, mut here) = a_crate();
        crate::semantic::release(&tree);

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "affected",
                "steps": [{"verb": "edit", "arguments": [
                    "src/keystore.rs", "sustituir", "Keystore", "KeyVault"
                ]}],
                "validate": [{"check": "rust"}],
                "on_failure": "keep"
            })),
        );

        let check = evidence.checks.first().expect("the rust check ran");
        assert_eq!(
            check.output["packages"],
            serde_json::json!(["vertical"]),
            "the crate the changed file is in was not selected: {check:?}"
        );
        assert_eq!(evidence.metrics.affected_packages, 1);
        assert_eq!(check.output["cached"], serde_json::json!(false));
        assert!(
            check.output["why"]
                .as_str()
                .is_some_and(|why| why.contains("vertical")),
            "the answer should say why it chose what it chose: {check:?}"
        );
    }

    #[test]
    fn a_check_of_bytes_this_machine_has_already_checked_is_not_run_again() {
        // Rule 13, as a design: the claim is that an expensive thing stops
        // happening, so the test counts the times it happens. `process_launches`
        // is that count, and a cache that quietly re-ran the compiler would pass
        // every assertion about the verdict.
        //
        // The program below changes a file and changes it back. That is not a
        // contrived shape: it is the reversible task the whole benchmark is
        // made of, and it is exactly where an identity made of timestamps says
        // "a different tree" about the same bytes.
        let (_base, store, tree, mut here) = a_crate();
        crate::semantic::release(&tree);

        let changed = vec!["src/keystore.rs".to_string()];
        let selection = crate::semantic::selection(store.root(), &tree, &changed, &[])
            .expect("a Cargo workspace");
        assert_eq!(selection.packages, vec!["vertical".to_string()]);
        let identity = selection.identity.expect("an identity for the check");

        let remembered = serde_json::to_string(&Remembered {
            verdict: Verdict::Passed,
            summary: "compiled clean".to_string(),
        })
        .expect("a record");
        crate::semantic::remember_validation(
            store.root(),
            &tree,
            "cargo check|vertical",
            &identity,
            &remembered,
        );

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "there and back",
                "steps": [
                    {"verb": "edit", "arguments": [
                        "src/keystore.rs", "sustituir", "Keystore", "Interim"
                    ]},
                    {"verb": "edit", "arguments": [
                        "src/keystore.rs", "sustituir", "Interim", "Keystore"
                    ]}
                ],
                "validate": [{"check": "rust"}]
            })),
        );

        let check = evidence.checks.first().expect("the rust check ran");
        assert_eq!(
            check.verdict,
            Verdict::Passed,
            "the same bytes were checked before and the answer was not reused: {check:?}"
        );
        assert_eq!(check.output["cached"], serde_json::json!(true));
        assert_eq!(evidence.metrics.validation_cache_hits, 1);
        assert_eq!(evidence.metrics.validation_cache_misses, 0);
        assert_eq!(
            evidence.metrics.process_launches, 0,
            "a compiler was started for bytes this machine had already compiled"
        );
        assert_eq!(evidence.status, "committed", "{}", evidence.reason);
    }

    #[test]
    fn a_check_of_bytes_nobody_has_seen_is_run() {
        // The control for the test above. Without it, a cache that answered
        // `passed` to everything would pass that one — rule 4, where the
        // baseline is the miss.
        let (_base, store, tree, mut here) = a_crate();
        crate::semantic::release(&tree);

        let changed = vec!["src/keystore.rs".to_string()];
        let selection = crate::semantic::selection(store.root(), &tree, &changed, &[])
            .expect("a Cargo workspace");
        let remembered = serde_json::to_string(&Remembered {
            verdict: Verdict::Passed,
            summary: "compiled clean".to_string(),
        })
        .expect("a record");
        crate::semantic::remember_validation(
            store.root(),
            &tree,
            "cargo check|vertical",
            &selection.identity.expect("an identity"),
            &remembered,
        );

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "a real change",
                "steps": [{"verb": "edit", "arguments": [
                    "src/keystore.rs", "sustituir", "Keystore", "KeyVault"
                ]}],
                "validate": [{"check": "rust"}],
                "on_failure": "keep"
            })),
        );

        let check = evidence.checks.first().expect("the rust check ran");
        assert_ne!(
            check.verdict,
            Verdict::Passed,
            "the bytes changed and the old answer was handed over anyway: {check:?}"
        );
        assert_eq!(check.output["cached"], serde_json::json!(false));
        assert_eq!(evidence.metrics.validation_cache_hits, 0);
        assert_eq!(evidence.metrics.validation_cache_misses, 1);
    }

    #[test]
    fn every_answer_the_rust_check_gives_says_whether_it_was_reused() {
        // **The 2026-08-29 Fedora failure, as an assertion.**
        //
        // Two tests asserted `check.output["cached"] == false` and got `Null`,
        // because the arm that answers "there is no cargo on this machine"
        // never wrote the field. So the failure a person read was two
        // assertions about a *cache*, and what had actually happened is that
        // `sudo` put `HOME=/root` in front of a toolchain installed under
        // `$SUDO_USER`'s home.
        //
        // A field that only appears on the days it is interesting is a field
        // nobody handles on the interesting day. This walks every arm the
        // check has and demands the shape be the same in all of them —
        // including the two that cannot be reached on a machine that has
        // everything, which is why it is a unit test over the arms and not a
        // run that happens to take one of them.
        let (_base, store, tree, mut here) = a_crate();
        crate::semantic::release(&tree);

        // Arm one: a tree Cargo cannot describe at all.
        let (_plain_base, plain_store, plain_tree, mut plain_here) =
            a_workspace(&[("notes.txt", "not a crate\n")]);
        let plain = run(
            &plain_store,
            &plain_tree,
            &mut plain_here,
            &program(serde_json::json!({
                "steps": [{"verb": "edit", "arguments": ["notes.txt", "sustituir", "not", "still not"]}],
                "validate": [{"check": "rust"}],
                "on_failure": "keep"
            })),
        );
        let check = plain.checks.first().expect("the rust check ran");
        assert_eq!(
            check.output["cached"],
            serde_json::json!(false),
            "a tree Cargo cannot describe answered without saying whether it \
             reused anything: {check:?}"
        );

        // Arm two: a real crate, whichever way the toolchain search goes on
        // this machine. Both outcomes carry the field; only one of them can
        // happen here, and the test is about the shape rather than the
        // verdict.
        let real = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "steps": [{"verb": "edit", "arguments": [
                    "src/keystore.rs", "sustituir", "Keystore", "KeyVault"
                ]}],
                "validate": [{"check": "rust"}],
                "on_failure": "keep"
            })),
        );
        let check = real.checks.first().expect("the rust check ran");
        assert!(
            check.output["cached"].is_boolean(),
            "the rust check answered `{}` for `cached`: {check:?}",
            check.output["cached"]
        );
    }

    #[test]
    fn a_missing_toolchain_says_where_it_looked_and_never_says_the_change_failed() {
        // Rule 10 with an address on it. "There is no cargo" is a sentence a
        // person cannot act on when the cargo is right there under another
        // home; the list of paths is what makes the `sudo` boundary visible at
        // the moment somebody is looking at it.
        let found = thalyx_rust::toolchain::cargo();
        let why = found.why_not("`cargo`", "So nothing was compiled");
        assert!(why.contains("place(s) this machine looks"), "{why}");
        assert!(
            found.looked_at.iter().any(|path| path.ends_with("cargo")),
            "the search names no candidate at all: {found:?}"
        );
    }

    // ── the programmable form ────────────────────────────────────────────

    /// A tree whose files differ in a way nothing outside it can know in
    /// advance.
    ///
    /// Five modules; three of them use `old_api` and two do not, and **which
    /// three is not visible from the file names**. That is the property the
    /// fixture exists for: a caller composing a static list of steps would
    /// have to read all five first — five answers, five round trips — or edit
    /// all five and be wrong about two.
    fn a_tree_that_has_to_be_looked_at() -> (tempfile::TempDir, Store, PathBuf, Where) {
        a_workspace(&[
            (
                "Cargo.toml",
                "[workspace]\n\n[package]\nname = \"vertical\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            (
                "src/lib.rs",
                "pub mod one;\npub mod two;\npub mod three;\npub mod four;\npub mod five;\n",
            ),
            ("src/one.rs", "pub fn one() -> u32 {\n    old_api(1)\n}\n"),
            ("src/two.rs", "pub fn two() -> u32 {\n    2\n}\n"),
            (
                "src/three.rs",
                "pub fn three() -> u32 {\n    old_api(3)\n}\n",
            ),
            ("src/four.rs", "pub fn four() -> u32 {\n    4\n}\n"),
            ("src/five.rs", "pub fn five() -> u32 {\n    old_api(5)\n}\n"),
        ])
    }

    /// The program the fixtures below run, in one place so the three columns
    /// differ only in what they are run against.
    ///
    /// Read it as the claim: **nothing in it names a file.** The list comes
    /// from the machine, the choice comes from what each file says, and the
    /// last decision comes from what a check answered.
    const LOOK_THEN_CHANGE: &str = r#"
        const listing = thalyx.list("src");
        thalyx.assert(listing.ok, "src could not be listed", listing);

        const sources = (listing.entries || [])
            .map((entry) => entry.name)
            .filter((name) => name.endsWith(".rs") && name !== "lib.rs")
            .sort();
        thalyx.assert(sources.length >= 4, "this tree should have several modules", sources);

        // The loop that could not have been written in advance: what is in each
        // file decides whether it is touched.
        const touched = [];
        const skipped = [];
        for (const name of sources) {
            const path = "src/" + name;
            const source = thalyx.read(path);
            if (!source.ok) { skipped.push(name); continue; }
            if (source.text.includes("old_api")) {
                thalyx.mustWork(
                    thalyx.substitute(path, "old_api", "new_api"),
                    "the substitution in " + path + " did not happen"
                );
                touched.push(name);
            } else {
                skipped.push(name);
            }
        }

        // What the tree says, not what the edits claimed.
        const seen = thalyx.changed();
        thalyx.assert(
            seen.count === touched.length,
            "the tree shows " + seen.count + " change(s) and the program made " + touched.length,
            seen
        );

        // And the branch on a validation, which is the last decision and the
        // one a static list has to hand back to a model to make.
        const parses = thalyx.validate({ check: "parses" });
        if (parses.verdict !== "passed") {
            return { gave_up: true, why: parses.summary };
        }

        const left = thalyx.grep("old_api");
        thalyx.assert(left.total === 0, "old_api is still somewhere", left);

        return {
            changed: touched,
            left_alone: skipped.length,
            still_there: left.total,
        };
    "#;

    #[test]
    fn one_request_looks_at_a_tree_and_changes_only_what_the_looking_says_to() {
        // **The defining fixture of the programmable form.**
        //
        // One external request. Inside it: a listing whose contents were not
        // known, a loop over that listing, a read per entry, a decision per
        // read, a mutation for three of them and none for two, an observation
        // of what really changed, a validation, a branch on the validation, and
        // a compact answer. Not one of those decisions went back to a model.
        //
        // A `Vec<Step>` cannot express this. To produce the same result it
        // would have to already contain the three file names — which is the
        // answer, so producing it *is* the work this is doing.
        let (_base, store, tree, mut here) = a_tree_that_has_to_be_looked_at();

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "only what needs it",
                "run": LOOK_THEN_CHANGE,
            })),
        );

        assert_eq!(evidence.status, "committed", "{}", evidence.reason);
        assert_eq!(evidence.finish.as_deref(), Some("returned"));
        assert_eq!(
            evidence.returned["changed"],
            serde_json::json!(["five.rs", "one.rs", "three.rs"]),
            "the program changed the wrong set: {}",
            evidence.returned
        );
        assert_eq!(evidence.returned["left_alone"], serde_json::json!(2));
        assert_eq!(evidence.returned["still_there"], serde_json::json!(0));

        // The bytes, which is the claim rather than the count.
        for name in ["one", "three", "five"] {
            let text = std::fs::read_to_string(tree.join(format!("src/{name}.rs"))).expect(name);
            assert!(text.contains("new_api"), "src/{name}.rs: {text}");
        }
        for name in ["two", "four"] {
            let text = std::fs::read_to_string(tree.join(format!("src/{name}.rs"))).expect(name);
            assert!(
                !text.contains("new_api") && !text.contains("old_api"),
                "src/{name}.rs was touched and had no reason to be: {text}"
            );
        }

        // And the numbers the hypothesis is stated in.
        assert_eq!(evidence.metrics.external_requests, 1);
        assert!(
            evidence.metrics.program_operations >= 10,
            "one request did {} things inside the machine: {:?}",
            evidence.metrics.program_operations,
            evidence.metrics
        );
        assert!(
            evidence.metrics.program_assertions >= 3,
            "{:?}",
            evidence.metrics
        );
        assert_eq!(evidence.change_count, 3);
    }

    #[test]
    fn what_the_program_read_is_far_more_than_what_came_back() {
        // The compression, as two numbers from the same run. Without it "the
        // answer is small" is a sentence about a JSON blob rather than a
        // measurement against what producing it cost.
        let (_base, store, tree, mut here) = a_tree_that_has_to_be_looked_at();
        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({"label": "measured", "run": LOOK_THEN_CHANGE})),
        );

        let returned = serde_json::to_string(&evidence.returned)
            .expect("the answer")
            .len();
        assert!(
            evidence.metrics.internal_bytes > returned * 8,
            "the machine handled {} bytes and handed back {returned}; the whole point \
             of running the program here is that those two numbers are far apart",
            evidence.metrics.internal_bytes
        );
    }

    #[test]
    fn a_program_whose_validation_fails_puts_every_one_of_its_changes_back() {
        // **The control**, and the reason the fixture above is safe to use. The
        // same program, over a tree with a file that will not parse after the
        // substitution: the program mutates three files, the check says no, and
        // a real boundary puts all three back byte for byte.
        let (_base, store, tree, mut here) = a_workspace(&[
            ("src/one.rs", "pub fn one() -> u32 {\n    old_api(1)\n}\n"),
            // The trap: substituting here leaves an unbalanced brace, which is
            // exactly what `parses` is for.
            ("src/two.rs", "pub fn two() -> u32 {\n    old_api(2) }}\n"),
            ("src/three.rs", "pub fn three() -> u32 {\n    3\n}\n"),
        ]);
        let before: Vec<String> = ["src/one.rs", "src/two.rs", "src/three.rs"]
            .iter()
            .map(|name| std::fs::read_to_string(tree.join(name)).expect(name))
            .collect();

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "it does not hold",
                "run": r#"
                    const listing = thalyx.list("src");
                    for (const entry of listing.entries || []) {
                        const path = "src/" + entry.name;
                        const source = thalyx.read(path);
                        if (source.ok && source.text.includes("old_api")) {
                            thalyx.substitute(path, "old_api", "new_api(");
                        }
                    }
                    const parses = thalyx.validate({ check: "parses" });
                    thalyx.mustPass(parses, "a changed file no longer parses");
                    return "should not get here";
                "#,
            })),
        );

        assert_eq!(evidence.status, "rolled_back", "{}", evidence.reason);
        assert!(evidence.rolled_back);
        assert_eq!(evidence.finish.as_deref(), Some("assertion"));
        for (name, was) in ["src/one.rs", "src/two.rs", "src/three.rs"]
            .iter()
            .zip(before.iter())
        {
            assert_eq!(
                &std::fs::read_to_string(tree.join(name)).expect(name),
                was,
                "{name} did not come back"
            );
        }
        // And the diagnosis survived the rollback, because it was never in the
        // tree the rollback replaced.
        assert!(
            evidence
                .checks
                .iter()
                .any(|record| record.verdict == Verdict::Failed),
            "{:?}",
            evidence.checks
        );
    }

    #[test]
    fn a_program_that_asks_for_the_model_has_changed_nothing() {
        // The third column, and the one the ambiguity contract needs: a program
        // that meets a decision it will not make stops **before** mutating and
        // says what it needs decided. Not a failure, not a success, and — the
        // part that matters — not a guess.
        let (_base, store, tree, mut here) = a_tree_that_has_to_be_looked_at();
        let before = std::fs::read_to_string(tree.join("src/one.rs")).expect("one.rs");

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "not my decision",
                "run": r#"
                    const listing = thalyx.list("src");
                    const many = (listing.entries || []).filter(
                        (entry) => entry.name.endsWith(".rs")
                    );
                    if (many.length > 2) {
                        return thalyx.needModel({
                            question: "which of these should be migrated",
                            candidates: many.map((entry) => entry.name).sort(),
                        });
                    }
                    thalyx.substitute("src/one.rs", "old_api", "new_api");
                    return "changed it";
                "#,
            })),
        );

        assert_eq!(evidence.finish.as_deref(), Some("needs_model"));
        assert_eq!(evidence.status, "rolled_back", "{}", evidence.reason);
        assert_eq!(evidence.change_count, 0, "{:?}", evidence.changed);
        assert_eq!(
            std::fs::read_to_string(tree.join("src/one.rs")).expect("one.rs"),
            before
        );
        assert!(
            evidence.returned["candidates"].is_array(),
            "the question came back without what it is about: {}",
            evidence.returned
        );
    }

    #[test]
    fn the_answer_of_one_operation_decides_whether_a_third_runs() {
        // **The composition test.** Written so that a hardcoded list of steps
        // cannot satisfy it: operation A is a search whose answer is a number
        // nothing outside the tree knows, B is an arithmetic decision on that
        // number, and C happens only on one side of it. The assertion is on
        // which requests reached the machine.
        let (_base, store, tree, mut here) =
            a_workspace(&[("a.txt", "needle\nneedle\n"), ("b.txt", "nothing here\n")]);

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "a decides c",
                "run": r#"
                    const found = thalyx.grep("needle");           // A
                    const many = found.total >= 2;                  // B
                    if (many) {                                     // C
                        thalyx.substitute("a.txt", "needle", "pin");
                    } else {
                        thalyx.substitute("b.txt", "nothing", "something");
                    }
                    return { total: found.total, took: many ? "a" : "b" };
                "#,
            })),
        );

        assert_eq!(evidence.status, "committed", "{}", evidence.reason);
        assert_eq!(evidence.returned["took"], serde_json::json!("a"));
        assert_eq!(evidence.returned["total"], serde_json::json!(2));
        assert_eq!(
            std::fs::read_to_string(tree.join("a.txt")).expect("a.txt"),
            "pin\npin\n"
        );
        assert_eq!(
            std::fs::read_to_string(tree.join("b.txt")).expect("b.txt"),
            "nothing here\n",
            "the branch that should not have run, ran"
        );

        // And the same program over a tree where A answers differently takes
        // the other branch, with nothing in the program changed. Without this
        // column, a program that always edited `a.txt` would pass everything
        // above.
        let (_base, store, tree, mut here) =
            a_workspace(&[("a.txt", "needle\n"), ("b.txt", "nothing here\n")]);
        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "a decides c, the other way",
                "run": r#"
                    const found = thalyx.grep("needle");
                    const many = found.total >= 2;
                    if (many) {
                        thalyx.substitute("a.txt", "needle", "pin");
                    } else {
                        thalyx.substitute("b.txt", "nothing", "something");
                    }
                    return { total: found.total, took: many ? "a" : "b" };
                "#,
            })),
        );
        assert_eq!(evidence.returned["took"], serde_json::json!("b"));
        assert_eq!(
            std::fs::read_to_string(tree.join("a.txt")).expect("a.txt"),
            "needle\n",
            "the branch that should not have run, ran"
        );
    }

    #[test]
    fn a_program_reaches_no_verb_and_no_path_a_single_request_could_not() {
        // The boundary, put to the form that could most easily have escaped it.
        // A loop can try a hundred paths where a list of steps tries one, so
        // the check has to be in the door rather than in the composing — and it
        // is: `Runner::request` is `external::one`.
        let (_base, store, tree, mut here) = a_workspace(&[("inside.txt", "here\n")]);
        std::fs::write(tree.parent().expect("a parent").join("outside.txt"), "no\n")
            .expect("a file outside");

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "trying every door",
                "run": r#"
                    const tried = [];
                    for (const path of ["../outside.txt", "/etc/passwd",
                                        "inside/../../outside.txt"]) {
                        tried.push(thalyx.read(path).ok === true);
                    }
                    // And a verb that is not exposed at all.
                    tried.push(thalyx.call("install_at", ["/dev/sda"]).ok === true);
                    return { any: tried.some((got) => got), tried: tried.length };
                "#,
                "on_failure": "keep",
            })),
        );

        assert_eq!(
            evidence.returned["any"],
            serde_json::json!(false),
            "{evidence:?}"
        );
        assert_eq!(evidence.returned["tried"], serde_json::json!(4));
    }

    #[test]
    fn a_program_may_not_open_a_boundary_or_settle_one() {
        // The same rule the static form has, and it has to be checked here too
        // because a program reaches verbs by name at runtime rather than in a
        // list something looked at first.
        let (_base, store, tree, mut here) = a_workspace(&[("a.txt", "x\n")]);
        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "settling its own boundary",
                "run": r#"
                    const opened = thalyx.call("attempt", ["abandonar", "agent"]);
                    return { ok: opened.ok === true, word: opened.word || opened.error };
                "#,
                "on_failure": "keep",
            })),
        );
        assert_eq!(
            evidence.returned["ok"],
            serde_json::json!(false),
            "{evidence:?}"
        );
        assert_eq!(
            evidence.returned["word"],
            serde_json::json!("not_from_inside"),
            "{evidence:?}"
        );
        assert_eq!(evidence.change_count, 0);

        // And the fact this test was written for. Before 2026-08-30 the call
        // above answered `ok: true` with a `confirm_with` line carrying the
        // snapshot name and the exact state witness — everything a second call
        // needs to abandon the transaction from inside itself. So the assertion
        // is not only that it was refused: it is that the answer hands over
        // nothing to try again with.
        assert!(
            evidence.returned.get("confirm_with").is_none(),
            "the refusal handed the program what it needs to retry: {}",
            evidence.returned
        );

        // The other half of the pair, and the one a list of steps is refused
        // for by name.
        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "a program inside a program",
                "run": r#"
                    const inner = thalyx.call("exec", ['{"steps":[{"verb":"state"}]}']);
                    return { ok: inner.ok === true, word: inner.word || inner.error };
                "#,
                "on_failure": "keep",
            })),
        );
        assert_eq!(
            evidence.returned["word"],
            serde_json::json!("not_from_inside"),
            "{evidence:?}"
        );
    }

    #[test]
    fn a_program_and_a_list_of_steps_are_not_both_accepted() {
        let error = Program::read(
            &serde_json::json!({"run": "return 1;", "steps": [{"verb": "state"}]}).to_string(),
        )
        .expect_err("two ideas about what to do");
        assert!(error.contains("both"), "{error}");
    }

    #[test]
    fn a_request_with_neither_says_what_it_is_missing() {
        let error = Program::read(&serde_json::json!({"label": "empty"}).to_string())
            .expect_err("nothing to run");
        assert!(error.contains("run"), "{error}");
        assert!(error.contains("steps"), "{error}");
    }

    #[test]
    fn a_program_that_never_stops_stops_and_the_tree_is_untouched() {
        // Rule: a limit failure produces evidence and rolls back. Nothing here
        // is about the program being malicious — a loop with a bad condition is
        // the commonest bug there is, and the machine's answer to it must not
        // be to hold the session open forever.
        let (_base, store, tree, mut here) = a_workspace(&[("a.txt", "x\n")]);
        let started = std::time::Instant::now();
        let evidence = run_within(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "forever",
                "run": "thalyx.substitute('a.txt', 'x', 'y'); while (true) {}",
            })),
            // Its own ceiling, handed to this run and to nothing else. A
            // variable would be a global switch with no owner — rule 11, and
            // the thing it would break is every other test's patience.
            thalyx_program::Limits {
                wall: std::time::Duration::from_secs(1),
                ..thalyx_program::Limits::default()
            },
        );

        assert!(started.elapsed() < std::time::Duration::from_secs(60));
        assert_eq!(
            evidence.finish.as_deref(),
            Some("exhausted"),
            "{}",
            evidence.reason
        );
        assert_eq!(evidence.status, "rolled_back", "{}", evidence.reason);
        assert_eq!(
            std::fs::read_to_string(tree.join("a.txt")).expect("a.txt"),
            "x\n",
            "the edit it made before it hung was kept"
        );
        assert!(
            evidence.reason.contains("stopped") || evidence.reason.contains("all"),
            "{}",
            evidence.reason
        );
    }

    #[test]
    fn a_failing_check_puts_the_rename_back_and_keeps_the_evidence() {
        // The other half of the vertical, and the half that makes the first
        // half safe to use: a rename that touched two files and a check that
        // says no leaves a tree byte for byte what it was — with the diagnosis
        // still in the store.
        if !analyzer_or_skip("that a failed check undoes a semantic rename") {
            return;
        }
        let (_base, store, tree, mut here) = a_crate();
        crate::semantic::release(&tree);
        let before: Vec<String> = ["src/keystore.rs", "src/boot.rs"]
            .iter()
            .map(|name| std::fs::read_to_string(tree.join(name)).expect(name))
            .collect();

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "a rename nobody wanted",
                "steps": [{"verb": "rename", "arguments": ["Keystore", "KeyVault"]}],
                // A check that cannot hold: the old name is gone by
                // construction, and this demands it be there.
                "validate": [{"check": "text", "text": "Keystore", "expect": "some"}]
            })),
        );

        assert_eq!(evidence.status, "rolled_back", "{}", evidence.reason);
        assert!(evidence.rolled_back);
        for (name, was) in ["src/keystore.rs", "src/boot.rs"].iter().zip(before.iter()) {
            assert_eq!(
                &std::fs::read_to_string(tree.join(name)).expect(name),
                was,
                "{name} did not come back"
            );
        }
        // The evidence outlives the tree it describes, which is why it is in
        // the store and not in the workspace.
        assert!(
            evidence_path(&store, &evidence.transaction).is_file() || !evidence.checks.is_empty(),
            "the diagnosis was rolled back along with the change"
        );
        assert_eq!(evidence.checks.len(), 1);
        assert_eq!(evidence.checks[0].verdict, Verdict::Failed);
    }

    #[test]
    fn one_request_performs_many_operations_and_commits() {
        // **The whole hypothesis, as an assertion.** One call in; a boundary, a
        // directory, two files, two edits, a search and two checks done inside
        // the machine; one answer out. Every one of those was a round trip to
        // the model before this verb existed.
        let (_base, store, tree, mut here) = a_workspace(&[
            (
                "src/lib.rs",
                "pub struct UidRegistry;\npub fn load() -> UidRegistry { UidRegistry }\n",
            ),
            (
                "src/main.rs",
                "use crate::UidRegistry;\nfn main() { let _ = UidRegistry; }\n",
            ),
            ("Cargo.toml", "[package]\nname = \"demo\"\n"),
        ]);

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "rename UidRegistry",
                "steps": [
                    {"verb": "edit", "arguments": [
                        "src/lib.rs", "sustituir-lote", "2",
                        "UidRegistry", "UserRegistry", "src/main.rs"
                    ]},
                    {"verb": "make_directory", "arguments": ["notes"]},
                    {"verb": "make_file", "arguments": ["notes/why.md"]},
                    // Three reads whose answers the model never sees. Before
                    // this verb each of them was a round trip, and each round
                    // trip carried the whole conversation with it.
                    {"verb": "list", "arguments": ["src"]},
                    {"verb": "read", "arguments": ["src/lib.rs"]},
                    {"verb": "grep", "arguments": ["UserRegistry"]}
                ],
                "validate": [
                    {"check": "text", "text": "UidRegistry", "expect": "none"},
                    {"check": "text", "text": "UserRegistry", "expect": "some"},
                    {"check": "parses"}
                ]
            })),
        );

        assert_eq!(evidence.status, "committed", "{}", evidence.reason);
        assert!(!evidence.rolled_back);

        // The old name is gone from both files and the new one is in both.
        for name in ["src/lib.rs", "src/main.rs"] {
            let after = std::fs::read_to_string(tree.join(name)).expect(name);
            assert!(!after.contains("UidRegistry"), "{name}: {after}");
            assert!(after.contains("UserRegistry"), "{name}: {after}");
        }
        assert!(tree.join("notes/why.md").is_file());

        // Every check ran and every one of them held.
        assert_eq!(evidence.checks.len(), 3);
        assert!(
            evidence.checks.iter().all(|c| c.verdict == Verdict::Passed),
            "{:?}",
            evidence
                .checks
                .iter()
                .map(|c| (&c.check, c.verdict))
                .collect::<Vec<_>>()
        );

        // The measurement the whole session is about: one request from outside,
        // and a great deal more than one thing done inside.
        assert_eq!(evidence.metrics.external_requests, 1);
        // Written as the exact arithmetic rather than a floor, so that a
        // change to what this verb does shows up here as a number to re-read
        // rather than as a test that keeps passing: one boundary opened, six
        // requests dispatched, two searches run by the checks — `parses` reads
        // files directly and dispatches nothing — and one boundary closed.
        assert_eq!(
            (
                evidence.metrics.external_requests,
                evidence.metrics.machine_operations
            ),
            (1, 10),
            "the ratio this verb exists for has moved: {:?}",
            evidence.metrics
        );
        assert!(evidence.metrics.filesystem_mutations >= 3);

        // And the boundary was closed rather than left open. An attempt still
        // open after a committed program is one nobody will ever settle.
        assert!(
            thalyx_core::attempt::open(&store).unwrap().is_none(),
            "the boundary is still open after a commit"
        );
    }

    #[test]
    fn a_program_whose_check_fails_puts_the_workspace_back_by_itself() {
        // The other half, and the one that makes the first half safe to use. A
        // rename that missed a file is a broken tree, and the model does not
        // have to be told about it, decide about it, or ask for it to be
        // undone: it is undone before the answer is written.
        let before_lib = "pub struct UidRegistry;\n";
        let before_main = "use crate::UidRegistry;\n";
        let (_base, store, tree, mut here) =
            a_workspace(&[("src/lib.rs", before_lib), ("src/main.rs", before_main)]);

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "label": "an incomplete rename",
                "steps": [
                    // One file of the two. The check below is what notices.
                    {"verb": "edit", "arguments": [
                        "src/lib.rs", "sustituir", "UidRegistry", "UserRegistry"
                    ]}
                ],
                "validate": [
                    {"check": "text", "text": "UidRegistry", "expect": "none"}
                ]
            })),
        );

        assert_eq!(evidence.status, "rolled_back", "{}", evidence.reason);
        assert!(evidence.rolled_back);

        // Byte for byte, both of them.
        assert_eq!(
            std::fs::read_to_string(tree.join("src/lib.rs")).unwrap(),
            before_lib
        );
        assert_eq!(
            std::fs::read_to_string(tree.join("src/main.rs")).unwrap(),
            before_main
        );
        assert!(thalyx_core::attempt::open(&store).unwrap().is_none());

        // The check really ran and really failed — not a program that never
        // mutated and was rolled back for nothing.
        assert_eq!(evidence.change_count, 1, "the mutation did not happen");
        assert_eq!(evidence.checks[0].verdict, Verdict::Failed);

        // The rollback was authorised by the state, not by a bare yes.
        assert_eq!(evidence.metrics.state_witness_checks, 1);

        // **And the diagnosis survived the rollback.** It is in the store, not
        // in the tree that was just replaced — which is why it is still here.
        keep(&store, &evidence).expect("evidence is kept");
        let kept: Evidence = serde_json::from_str(
            &std::fs::read_to_string(evidence_path(&store, &evidence.transaction)).unwrap(),
        )
        .expect("the evidence reads back");
        assert_eq!(kept.checks[0].verdict, Verdict::Failed);
        assert!(
            kept.checks[0].output.get("hits").is_some(),
            "the search's own rows are gone"
        );
    }

    #[test]
    fn a_refused_step_stops_the_program_where_it_stands() {
        // A program is not a batch of independent wishes. A step that did not
        // happen makes every step after it a step against a tree nobody
        // intended, so the rest does not run — and the tree goes back.
        let (_base, store, tree, mut here) = a_workspace(&[("a.rs", "fn a() {}\n")]);

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "steps": [
                    {"verb": "make_file", "arguments": ["b.rs"]},
                    {"verb": "read", "arguments": ["nowhere.rs"]},
                    {"verb": "make_file", "arguments": ["c.rs"]}
                ]
            })),
        );

        assert_eq!(evidence.status, "rolled_back", "{}", evidence.reason);
        assert_eq!(evidence.steps.len(), 2, "the third step ran anyway");
        assert!(!evidence.steps[1].ok);
        assert!(!tree.join("b.rs").exists(), "the first step was not undone");
        assert!(!tree.join("c.rs").exists());
        // No check was run against a tree the program never meant to produce.
        assert!(evidence.checks.is_empty());
    }

    #[test]
    fn a_step_that_reaches_outside_the_workspace_is_refused_exactly_as_a_lone_request_is() {
        // The claim that makes composition safe: a program is not an authority.
        // If this ever passes, `hacer` has become a way around the boundary
        // and every argument in this file's header is void.
        let (_base, store, tree, mut here) = a_workspace(&[("a.rs", "fn a() {}\n")]);
        let outside = tree.parent().expect("a parent").join("secret.txt");
        std::fs::write(&outside, "not the agent's").expect("the file");

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "steps": [{"verb": "read", "arguments": [outside.display().to_string()]}]
            })),
        );

        assert!(!evidence.steps[0].ok);
        assert_eq!(evidence.status, "rolled_back");
        assert_eq!(
            std::fs::read_to_string(&outside).unwrap(),
            "not the agent's"
        );
    }

    #[test]
    fn a_verb_that_is_not_exposed_is_not_reachable_by_putting_it_in_a_program() {
        let (_base, store, tree, mut here) = a_workspace(&[("a.rs", "fn a() {}\n")]);
        for verb in ["execute", "power_off", "deny", "install_onto"] {
            let evidence = run(
                &store,
                &tree,
                &mut here,
                &program(serde_json::json!({
                    "steps": [{"verb": verb, "arguments": []}]
                })),
            );
            assert!(
                !evidence.steps[0].ok,
                "`{verb}` ran inside a program and is not exposed outside one"
            );
        }
    }

    #[test]
    fn a_program_may_not_contain_a_program_or_settle_its_own_boundary() {
        // Refused when the program is read, which is before a snapshot is
        // taken: a caller learns about it without paying for a snapshot and a
        // restore.
        for verb in ["exec", "attempt"] {
            let refusal = Program::read(
                &serde_json::json!({"steps": [{"verb": verb, "arguments": []}]}).to_string(),
            )
            .expect_err("this must not be readable as a program");
            assert!(refusal.contains(verb), "{refusal}");
        }
    }

    #[test]
    fn a_check_that_could_not_run_is_never_a_check_that_passed() {
        // Rule 9 as the difference between a runtime that validates and one
        // that commits. `cargo` is asked for over a tree with no manifest in
        // it, so there is nothing it could have checked — and `not_proven` is
        // not `passed`, so the program is rolled back.
        let (_base, store, tree, mut here) = a_workspace(&[("notes.md", "# hello\n")]);

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "steps": [{"verb": "make_file", "arguments": ["more.md"]}],
                "validate": [{"check": "rust"}]
            })),
        );

        assert_eq!(evidence.checks[0].verdict, Verdict::NotProven);
        assert_eq!(evidence.status, "rolled_back", "{}", evidence.reason);
        assert!(!tree.join("more.md").exists());
    }

    #[test]
    fn a_parse_check_over_nothing_it_understands_is_not_a_pass_either() {
        let (_base, store, tree, mut here) = a_workspace(&[("notes.md", "# hello\n")]);
        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "steps": [{"verb": "make_file", "arguments": ["more.md"]}],
                "validate": [{"check": "parses"}]
            })),
        );
        assert_eq!(evidence.checks[0].verdict, Verdict::NotProven);
        assert!(evidence.rolled_back);
    }

    #[test]
    fn a_mechanical_edit_that_ate_a_brace_is_caught_before_it_is_committed() {
        // What `parses` is actually for. The substitution is exactly the kind a
        // pattern-based agent writes, it succeeds, and it leaves a file no
        // compiler will accept.
        let (_base, store, tree, mut here) =
            a_workspace(&[("src/lib.rs", "pub fn go() {\n    work();\n}\n")]);

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                // Single-line, because that is what a mechanical rename is:
                // the closing brace two lines below is now orphaned and no
                // step said anything about it.
                "steps": [{"verb": "edit", "arguments": [
                    "src/lib.rs", "sustituir", "pub fn go() {", "pub fn go()"
                ]}],
                "validate": [{"check": "parses"}]
            })),
        );

        assert_eq!(
            evidence.checks.first().map(|c| c.verdict),
            Some(Verdict::Failed),
            "{}",
            evidence.reason
        );
        assert_eq!(evidence.status, "rolled_back");
        assert_eq!(
            std::fs::read_to_string(tree.join("src/lib.rs")).unwrap(),
            "pub fn go() {\n    work();\n}\n"
        );
    }

    #[test]
    fn asking_to_keep_a_failure_keeps_it_and_says_it_is_not_a_commit() {
        // For the caller that wants to look at the tree a failure produced. It
        // is a different word from `committed`, deliberately: a caller reading
        // `committed` on a run whose checks failed would believe the machine
        // agreed with it.
        let (_base, store, tree, mut here) = a_workspace(&[("a.rs", "fn a() {}\n")]);
        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "steps": [{"verb": "make_file", "arguments": ["b.rs"]}],
                "validate": [{"check": "text", "text": "nowhere", "expect": "some"}],
                "on_failure": "keep"
            })),
        );

        assert_eq!(evidence.status, "kept_after_failure");
        assert!(!evidence.rolled_back);
        assert!(tree.join("b.rs").exists());
        assert!(thalyx_core::attempt::open(&store).unwrap().is_none());
    }

    #[test]
    fn the_answer_is_small_and_the_evidence_is_not() {
        // The compression, measured rather than asserted about. Everything the
        // steps and the checks produced is kept; a fraction of it is what goes
        // back to the model.
        let (_base, store, tree, mut here) = a_workspace(&[
            ("src/lib.rs", &"pub fn a() {}\n".repeat(200)),
            ("src/other.rs", &"pub fn b() {}\n".repeat(200)),
        ]);

        let evidence = run(
            &store,
            &tree,
            &mut here,
            &program(serde_json::json!({
                "steps": [
                    {"verb": "grep", "arguments": ["pub fn"]},
                    {"verb": "edit", "arguments": ["src/lib.rs", "sustituir", "pub fn a", "pub fn c"]}
                ],
                "validate": [{"check": "text", "text": "pub fn", "expect": "some"}]
            })),
        );

        assert_eq!(evidence.status, "committed", "{}", evidence.reason);
        let returned = serde_json::to_string(&serde_json::Value::Object(
            answer_object(&evidence)
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        ))
        .expect("an answer");

        assert!(
            returned.len() * 4 < evidence.metrics.internal_bytes,
            "the answer is {} bytes against {} produced inside; this is not compression",
            returned.len(),
            evidence.metrics.internal_bytes
        );
        // And the raw material is reachable, which is what makes the smallness
        // honest rather than a loss.
        keep(&store, &evidence).expect("kept");
        let kept: Evidence = serde_json::from_str(
            &std::fs::read_to_string(evidence_path(&store, &evidence.transaction)).unwrap(),
        )
        .unwrap();
        assert_eq!(kept.steps.len(), 2);
        assert!(kept.steps[0].answer.get("hits").is_some());
    }

    #[test]
    fn a_handle_that_is_not_the_shape_of_one_is_refused_before_it_is_a_path() {
        for wrong in ["../../etc/passwd", "a/b", "", "a b", "..", "x/../y"] {
            assert!(!is_a_handle(wrong), "`{wrong}` was accepted as a handle");
        }
        assert!(is_a_handle("t-1234"));
    }

    #[test]
    fn a_program_bigger_than_the_ceiling_is_refused_before_anything_is_snapshotted() {
        let steps: Vec<serde_json::Value> = (0..MOST_STEPS + 1)
            .map(|n| serde_json::json!({"verb": "make_file", "arguments": [format!("f{n}")]}))
            .collect();
        assert!(Program::read(&serde_json::json!({"steps": steps}).to_string()).is_err());
        assert!(Program::read(&serde_json::json!({"steps": []}).to_string()).is_err());
    }

    #[test]
    fn the_packages_a_change_touches_are_the_packages_that_get_checked() {
        // Now asked of Cargo rather than read off the manifests by hand. The
        // hand-written version got the attribution right and could not answer
        // the question that actually decides a check: `two` uses `one`, so a
        // change to `one` has to compile `two` as well, and no amount of
        // walking up to the nearest manifest says so.
        let (_base, store, tree, _here) = a_workspace(&[
            (
                "Cargo.toml",
                "[workspace]\nresolver = \"2\"\nmembers = [\"crates/one\", \"crates/two\"]\n",
            ),
            (
                "crates/one/Cargo.toml",
                "[package]\nname = \"one\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("crates/one/src/lib.rs", "pub fn ground() -> u32 { 1 }\n"),
            (
                "crates/two/Cargo.toml",
                "[package]\nname = \"two\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n                 [dependencies]\none = { path = \"../one\" }\n",
            ),
            (
                "crates/two/src/lib.rs",
                "pub fn stacked() -> u32 { one::ground() + 1 }\n",
            ),
        ]);
        crate::semantic::release(&tree);

        let selection = crate::semantic::selection(
            store.root(),
            &tree,
            &["crates/one/src/lib.rs".to_string(), "README.md".to_string()],
            &[],
        )
        .expect("a Cargo workspace");

        assert_eq!(
            selection.packages,
            vec!["one".to_string(), "two".to_string()],
            "compiling only `one` would prove nothing about the crate that uses it"
        );
        assert_eq!(
            selection.unattributed,
            vec!["README.md".to_string()],
            "a file nobody can place is named rather than dropped"
        );

        // The escape hatch replaces the derivation rather than adding to it.
        let asked = crate::semantic::selection(
            store.root(),
            &tree,
            &["crates/one/src/lib.rs".to_string()],
            &["two".to_string()],
        )
        .expect("a Cargo workspace");
        assert_eq!(asked.packages, vec!["two".to_string()]);
    }
}
