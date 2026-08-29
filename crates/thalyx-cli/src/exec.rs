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
    /// `cargo check` over the packages the changed files belong to.
    Rust {
        /// `check` (the default) or `test`.
        #[serde(default = "check_word")]
        mode: String,
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

#[derive(Debug, Clone, Deserialize)]
pub struct Program {
    #[serde(default = "agent")]
    pub label: String,
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

        if program.steps.is_empty() {
            return Err(
                "`steps` is empty; a program with no requests in it changes \
                        nothing and there is nothing to be transactional about"
                    .to_string(),
            );
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

    // ── the steps ───────────────────────────────────────────────────────────
    let boundary = here.confined_to().map(Path::to_path_buf);
    let mut refused = None;
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
        for check in &program.validate {
            metrics.validations += 1;
            let record = run_check(
                asked,
                here,
                boundary.as_deref(),
                check,
                &difference,
                &mut metrics,
            );
            metrics.internal_bytes += record.output.to_string().len();
            evidence.checks.push(record);
        }
    }

    let everything_held = refused.is_none()
        && evidence
            .checks
            .iter()
            .all(|record| record.verdict == Verdict::Passed);

    evidence.reason = if let Some(index) = refused {
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
        let failed: Vec<&str> = evidence
            .checks
            .iter()
            .filter(|record| record.verdict != Verdict::Passed)
            .map(|record| record.check.as_str())
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

    metrics.machine_time_ms = started.elapsed().as_millis();
    evidence.metrics = metrics;
    evidence
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
            let outcome = run_confined(asked, program, arguments, &[], metrics);
            CheckRecord {
                check: format!("program `{program}`"),
                verdict: outcome.verdict,
                summary: outcome.summary,
                output: outcome.output,
            }
        }

        Check::Rust { mode } => rust_check(asked, difference, mode, metrics),
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
    metrics: &mut Metrics,
) -> CheckRecord {
    let packages = affected_packages(&asked.subvolume, difference);
    let subcommand = if mode == "test" { "test" } else { "check" };

    if packages.is_empty() {
        return CheckRecord {
            check: format!("cargo {subcommand}"),
            verdict: Verdict::NotProven,
            summary: "no changed file belongs to a Cargo package, so there is nothing \
                      this could have checked"
                .to_string(),
            output: json!({"packages": []}),
        };
    }

    let Some(cargo) = find_cargo() else {
        // Rule 10, in the place it matters most: a toolchain that is not here
        // is not a change that does not compile.
        return CheckRecord {
            check: format!("cargo {subcommand}"),
            verdict: Verdict::NotProven,
            summary: "there is no `cargo` on this machine, so the change was not compiled"
                .to_string(),
            output: json!({"packages": packages, "word": "no_cargo"}),
        };
    };

    let mut arguments = vec![
        subcommand.to_string(),
        // No network from inside the confinement, so a build that wanted to
        // fetch would fail as a network error and read as broken code.
        "--offline".to_string(),
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
    let mut readable = vec![cargo.parent().unwrap_or(Path::new("/")).to_path_buf()];
    if let Some(home) = std::env::var_os("HOME") {
        readable.push(PathBuf::from(&home).join(".cargo"));
        readable.push(PathBuf::from(&home).join(".rustup"));
    }

    let outcome = run_confined(
        asked,
        &cargo.display().to_string(),
        &arguments,
        &readable,
        metrics,
    );
    CheckRecord {
        check: format!("cargo {subcommand} over {}", packages.join(", ")),
        verdict: outcome.verdict,
        summary: outcome.summary,
        output: {
            let mut output = outcome.output;
            if let Some(object) = output.as_object_mut() {
                object.insert("packages".to_string(), json!(packages));
            }
            output
        },
    }
}

/// Every Cargo package a changed file falls inside.
///
/// Walking up from each changed file to the nearest manifest that declares a
/// package, which is what Cargo itself means by "which package is this file
/// in". A file under no manifest belongs to no package and is skipped rather
/// than attributed to the workspace root, because a workspace root often has no
/// package of its own and `-p` on it would fail.
fn affected_packages(root: &Path, difference: &thalyx_snapshot::Difference) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for name in difference.added.iter().chain(difference.modified.iter()) {
        let mut directory = root.join(name);
        while let Some(parent) = directory.parent() {
            if !parent.starts_with(root) {
                break;
            }
            if let Some(package) = package_named_in(&parent.join("Cargo.toml"))
                && !found.contains(&package)
            {
                found.push(package);
                break;
            }
            if parent == root {
                break;
            }
            directory = parent.to_path_buf();
        }
    }
    found
}

/// The `name` of the `[package]` a manifest declares, if it declares one.
///
/// Read by hand rather than with a TOML parser, and the reason is what it must
/// *not* do: a workspace manifest with no `[package]` must come back `None`, and
/// a `name` under `[dependencies]` must never be mistaken for the package's own.
/// So the section is tracked, and only the first `name` inside `[package]`
/// counts.
fn package_named_in(manifest: &Path) -> Option<String> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[package]";
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(value) = line.strip_prefix("name") {
            let value = value.trim_start().strip_prefix('=')?.trim();
            let name = value.trim_matches(|c| c == '"' || c == '\'');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Where `cargo` is, if it is anywhere this machine can name.
///
/// The places are listed rather than searched for on `PATH`, because inside
/// Thalyx there is no `PATH` and no shell to expand one — and because a
/// validation that ran whichever `cargo` came first on a caller's environment
/// would be a validation whose meaning depends on who started the session.
fn find_cargo() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".cargo/bin/cargo"));
    }
    candidates.push(PathBuf::from("/usr/local/bin/cargo"));
    candidates.push(PathBuf::from("/usr/bin/cargo"));
    candidates.into_iter().find(|path| path.is_file())
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

    vec![
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
    ]
}

/// `hacer <programa>` — the verb, in whichever face is asking.
pub fn run(store: &Store, here: &mut Where, rest: &str, face: Face, request_id: &str) -> Fallible {
    let Some(given) = crate::words::asked(face, OP, rest) else {
        return Ok(());
    };
    let text = given.first().map(|word| word.as_str()).unwrap_or("").trim();
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
        subvolume,
        request_id: request_id.to_string(),
    };
    let evidence = carry_out(&asked, thalyx_snapshot::Native, here, &program);

    // Kept before it is answered. A caller handed a handle that names nothing
    // has been handed a lie, and the failure it would meet is a second call.
    let kept = keep(store, &evidence);

    if face == Face::Machine {
        let mut carried = answer_object(&evidence);
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
            Some(_) => {}
            None if id.is_none() => id = Some(word.to_string()),
            None => {}
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
        carry_out(
            &Asked {
                store,
                subvolume: tree.to_path_buf(),
                request_id: format!("t-{}", std::process::id()),
            },
            Directories,
            here,
            program,
        )
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
        // Package scope, read off the manifests on disk. A file under no
        // manifest belongs to no package and is skipped rather than attributed
        // to the workspace root, which often has no package of its own.
        let base = tempfile::tempdir().expect("a temp dir");
        let root = base.path();
        for (path, text) in [
            ("Cargo.toml", "[workspace]\nmembers = [\"crates/one\"]\n"),
            (
                "crates/one/Cargo.toml",
                "[package]\nname = \"one\"\n\n[dependencies]\nname = \"not-this\"\n",
            ),
            ("crates/two/Cargo.toml", "[package]\nname = \"two\"\n"),
        ] {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }

        let difference = thalyx_snapshot::Difference {
            modified: vec![
                "crates/one/src/lib.rs".into(),
                "crates/one/src/deep/other.rs".into(),
                "crates/two/src/lib.rs".into(),
                "README.md".into(),
            ],
            modified_total: 4,
            ..Default::default()
        };
        assert_eq!(affected_packages(root, &difference), vec!["one", "two"]);

        // And a `name` under `[dependencies]` is never mistaken for the
        // package's own — which is the whole reason this is not a `split` on
        // the word `name`.
        assert_eq!(
            package_named_in(&root.join("crates/one/Cargo.toml")),
            Some("one".to_string())
        );
        assert_eq!(package_named_in(&root.join("Cargo.toml")), None);
    }
}
