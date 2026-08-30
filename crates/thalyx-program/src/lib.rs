//! The programmable transaction: a short program from a model, run here.
//!
//! `vault/03-Primitivas/Transaccion-Programable.md`, and the hypothesis it
//! serves is `vault/09-Notas-Tecnicas/Trabajo-Entre-Inferencias.md`.
//!
//! ## What was missing
//!
//! `hacer` already took several requests and ran them inside one reversible
//! boundary. But the requests were a `Vec<Step>`: **the model had to know every
//! operation and every argument before anything ran.** That is batching, and
//! batching cannot express the thing an agent actually spends its turns on —
//! *ask, look at the answer, decide what to do next*. A rename across the
//! references a query just returned, an edit applied only to the ones whose
//! surrounding lines say something, a validation whose result decides whether
//! the next thing happens at all: none of that is writable in advance, so every
//! one of those decisions was a round trip to a frontier model, dragging the
//! whole conversation with it.
//!
//! So the unit is now a **program**. One inference produces a short piece of
//! code; the machine runs it locally, with variables, loops, conditions and
//! assertions; the model is asked again when the program finishes, when it
//! explicitly asks for judgment, or when it genuinely cannot continue.
//!
//! ## Why JavaScript, and why QuickJS
//!
//! The language is not sacred and the properties are. What decided it:
//!
//! - **A frontier model already writes it.** The one thing a bespoke Thalyx
//!   scripting language guarantees is that every model using it is writing a
//!   language it has never seen, from a description in a tool schema. That is
//!   the most expensive possible way to spend the very attention this whole
//!   mechanism exists to save. JavaScript costs zero prompt.
//! - **QuickJS has no ambient authority to take away.** Its core is a language
//!   and nothing else: no `fs`, no `net`, no `process`, no `require`, no
//!   `fetch`, no clock that reaches the outside. A program starts able to do
//!   arithmetic, and the only things it can reach are the functions bound here.
//!   That is the opposite of embedding a shell, where the work would be
//!   subtracting authority from something that starts with all of it — and
//!   subtracting is the direction that fails quietly.
//! - **It is C99 with no dependencies**, so it compiles into a static musl
//!   binary. Rule 12: the binary that gets verified has to be the binary that
//!   ships, and a runtime that needed a shared library could not be in the
//!   image at all. This is the same argument the workspace already makes for
//!   compiling SQLite in.
//! - **It can be stopped.** An interrupt handler, a memory ceiling and a stack
//!   ceiling are part of the engine, so `while (true) {}` terminates for a
//!   reason rather than by luck.
//!
//! ## The program is not the authority
//!
//! It is untrusted code written by a language model. Everything it can do it
//! does by calling [`Machine`], which is implemented **above** this crate by
//! whoever owns the transaction — and every one of those calls goes through the
//! same door a single request goes through, against the same workspace
//! boundary, inside the same snapshot. This crate:
//!
//! - opens nothing, reads nothing, writes nothing, and starts no process;
//! - has no filesystem or network API to bind, because QuickJS ships none;
//! - never decides what an answer *means* — a refusal comes back as a value
//!   with `ok: false` in it, and the program branches on it like any other.
//!
//! What a program can reach is exactly the union of what its calls could have
//! reached one at a time. If that were not true this crate would be the
//! parallel API `Agentes-Externos.md` forbids.
//!
//! ## Stopping is enforced twice
//!
//! An assertion that only threw a JavaScript exception could be caught by the
//! program that failed it — `try { thalyx.assert(false) } catch {}` — and the
//! run would carry on past the thing that was supposed to end it. So a failed
//! assertion **latches**: it is recorded on the Rust side, it throws, and from
//! that moment the interrupt handler stops the engine and every host call
//! refuses. A program cannot catch its way past a stop, because the thing that
//! stops it is not in the language.

use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

mod bind;

pub use bind::PRELUDE;

/// What a program is allowed to ask the machine for.
///
/// Four questions and no more, and every one of them is answered by code that
/// already existed for the single-request path. A fifth would have to be
/// justified as a *capability*, not as a convenience: the way to give programs
/// more reach is to expose more verbs to [`Machine::request`], which is one
/// list in one place that a person can read.
pub trait Machine {
    /// One Thalyx request — a verb and its arguments — and whatever it
    /// answered.
    ///
    /// Returns the verb's own object, verbatim, refusals included. A refusal is
    /// `{"ok": false, …}` and is a value the program branches on; turning it
    /// into an error here would make every mistake a program makes end the
    /// program, which is the opposite of being able to write `if`.
    fn request(&mut self, verb: &str, arguments: &[String]) -> Value;

    /// One validation, through the same checker the whole verb uses.
    fn validate(&mut self, check: &Value) -> Value;

    /// What the tree really shows changed since the boundary opened.
    ///
    /// Observed and not remembered: what a call *said* it changed is a claim by
    /// the call, and this is the filesystem.
    fn changed(&mut self) -> Value;

    /// How many processes have been started under confinement so far.
    ///
    /// Read by the runtime rather than counted here, because the launches
    /// happen inside `request` and `validate` and only the machine knows about
    /// them. It is what the process ceiling is checked against.
    fn process_launches(&self) -> usize;

    /// The verb ids this session may reach, for the program to look at.
    fn verbs(&self) -> Vec<String>;
}

/// What one program may spend.
///
/// A static list of steps had one bound — how many steps — and that was enough
/// for something that could not loop. A program can, so every resource it can
/// consume needs a ceiling, and each is separate because a machine that has one
/// of them to spare and not another must be able to say so.
///
/// Every default here is deliberately generous enough that no honest program
/// meets it and small enough that a runaway one dies in seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// How long the program may run in total.
    ///
    /// **The one that catches `while (true) {}`.** Checked by the engine's
    /// interrupt handler, which QuickJS calls during bytecode execution, so a
    /// loop that never allocates and never calls anything still stops.
    pub wall: Duration,
    /// How much the engine may allocate. Reaching it is an engine error, not a
    /// kill: the program is unwound and the run is reported.
    pub memory_bytes: usize,
    /// How deep the JavaScript stack may go, so unbounded recursion is a
    /// refusal rather than this process's own stack overflowing.
    pub stack_bytes: usize,
    /// How many times the program may ask the machine for anything.
    ///
    /// The successor to `MOST_STEPS`, and it counts *calls* rather than lines:
    /// a loop over two hundred references is two hundred requests however
    /// short the source is.
    pub calls: usize,
    /// How many processes the whole run may start under confinement.
    ///
    /// A fork bomb inside a program is not possible — there is nothing in the
    /// language to fork with — but a loop calling a validation that compiles is
    /// a process explosion by a slower route, and it is the same ceiling.
    pub process_launches: usize,
    /// How many bytes of answers the program may take in.
    ///
    /// Bounded because a program is welcome to read a hundred files and this
    /// keeps a loop over a huge tree from becoming this process's memory. It is
    /// *not* the model's budget: none of these bytes leave the machine.
    pub answer_bytes: usize,
    /// How much the program may write with `thalyx.log`.
    pub log_bytes: usize,
    /// How large the value the program returns may be.
    ///
    /// The one ceiling that is about the model's context rather than about this
    /// machine, and the reason it is a refusal rather than a truncation: an
    /// answer cut in half is an answer a model will act on believing it is
    /// whole. The whole of it is in the evidence either way.
    pub returned_bytes: usize,
    /// How many times the interrupt handler may fire before the run is
    /// stopped.
    ///
    /// A second ceiling beside the clock, and a different kind: the clock
    /// measures how busy the machine is, and this measures how much the program
    /// did. A run that fails this one fails it identically on a fast machine
    /// and a slow one, which is what makes a limit reportable.
    pub ticks: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            wall: Duration::from_secs(120),
            memory_bytes: 128 * 1024 * 1024,
            stack_bytes: 1024 * 1024,
            calls: 512,
            process_launches: 32,
            answer_bytes: 64 * 1024 * 1024,
            log_bytes: 64 * 1024,
            returned_bytes: 32 * 1024,
            ticks: 200_000_000,
        }
    }
}

/// How a run ended.
///
/// Five arms and not two, because "it worked" and "it did not" is exactly the
/// distinction that loses the two interesting cases: a program that asked for
/// judgment did not fail, and a program that hit a ceiling did not produce a
/// wrong answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Finish {
    /// It ran to the end and returned this.
    Returned { value: Value },
    /// It decided the next decision is not one a machine should make.
    ///
    /// The shape the ambiguity contract is answered in: three candidates come
    /// back, the program sees `resolution === "ambiguous"`, and it stops
    /// **without having mutated anything** rather than guessing. Not a failure
    /// — the transaction commits or rolls back on the usual rules — and never
    /// silently a success either.
    NeedsModel { value: Value },
    /// An assertion said no. Everything after it did not run.
    Assertion { message: String, detail: Value },
    /// The program threw something it did not catch.
    Threw { message: String },
    /// A ceiling was reached. Names which one, in the units it is counted in.
    Exhausted {
        limit: &'static str,
        message: String,
    },
    /// The program could not be read or the engine could not be built.
    Refused { message: String },
}

impl Finish {
    /// Whether the transaction should treat this as the program having
    /// succeeded.
    ///
    /// `needs_model` is **not** a success: the program stopped short of what it
    /// was asked to do, and a commit here would keep half a change and report
    /// it as finished work. It is not a failure either, and the difference is
    /// visible in the word rather than in the outcome.
    pub fn went_through(&self) -> bool {
        matches!(self, Finish::Returned { .. })
    }

    pub fn word(&self) -> &'static str {
        match self {
            Finish::Returned { .. } => "returned",
            Finish::NeedsModel { .. } => "needs_model",
            Finish::Assertion { .. } => "assertion",
            Finish::Threw { .. } => "threw",
            Finish::Exhausted { .. } => "exhausted",
            Finish::Refused { .. } => "refused",
        }
    }

    /// One sentence, for the answer that goes back.
    pub fn why(&self) -> String {
        match self {
            Finish::Returned { .. } => "the program ran to the end".to_string(),
            Finish::NeedsModel { .. } => {
                "the program stopped and asked for a decision it would not make".to_string()
            }
            Finish::Assertion { message, .. } => format!("an assertion did not hold: {message}"),
            Finish::Threw { message } => format!("the program threw: {message}"),
            Finish::Exhausted { message, .. } => message.clone(),
            Finish::Refused { message } => message.clone(),
        }
    }
}

use serde::Serialize;

/// One thing the program asked the machine for, as the evidence records it.
///
/// The answer is kept whole. **This is the half that does not go back to the
/// model** — it is the compression's other side, and an evidence record that
/// only kept summaries would leave a caller unable to audit the run it is being
/// asked to trust.
#[derive(Debug, Clone, Serialize)]
pub struct CallRecord {
    /// `request`, `validate` or `changed`.
    pub kind: &'static str,
    pub verb: String,
    pub arguments: Vec<String>,
    pub ok: bool,
    pub answer: Value,
}

/// What one run cost, as numbers.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProgramMetrics {
    /// Requests the program made through [`Machine::request`].
    pub requests: usize,
    /// Validations it asked for.
    pub validations: usize,
    /// Times it observed the tree.
    pub observations: usize,
    /// Assertions it made, held or not. The count of *checked* premises, which
    /// is the number the early-failure claim is about.
    pub assertions: usize,
    /// Bytes of answers the machine handed the program.
    ///
    /// The numerator of the compression: this against `returned_bytes` is how
    /// much of what the program looked at never crossed back.
    pub answer_bytes: usize,
    /// Times the engine's interrupt handler fired.
    pub ticks: u64,
    pub wall_ms: u128,
}

/// Everything one run produced.
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub finish: Finish,
    pub calls: Vec<CallRecord>,
    pub printed: Vec<String>,
    pub metrics: ProgramMetrics,
}

impl Outcome {
    /// The value the program meant to hand back, whatever shape it ended in.
    pub fn value(&self) -> Value {
        match &self.finish {
            Finish::Returned { value } | Finish::NeedsModel { value } => value.clone(),
            _ => Value::Null,
        }
    }
}

/// Why a run stopped, latched on the serving side where the program cannot
/// reach it.
///
/// The point of latching: a `try`/`catch` around an assertion would otherwise
/// let a program carry on past the thing that was meant to end it, and a
/// program written by a language model is exactly the kind of program that
/// wraps everything in `try`/`catch`. Once this is set the interrupt handler
/// stops the engine and every host call refuses, neither of which is in the
/// language.
#[derive(Debug, Clone)]
enum Stopped {
    NeedsModel(Value),
    Assertion {
        message: String,
        detail: Value,
    },
    Exhausted {
        limit: &'static str,
        message: String,
    },
}

/// What the engine asks the machine for.
///
/// A message and not a function call, because the engine runs on **its own
/// thread**. Two reasons, and the second is the one that decided it:
///
/// 1. `rquickjs` binds `'static` closures, and the machine a real transaction
///    hands over borrows a store, a session and a workspace. A channel is the
///    ordinary way to lend a borrow to a thread; the alternative was `unsafe`,
///    which in this codebase lives in `thalyx-syscall` and nowhere else.
/// 2. Untrusted code gets its own stack. A program that recurses without end
///    unwinds a stack that is not the one Thalyx is standing on.
#[derive(Debug)]
enum Ask {
    Request {
        verb: String,
        arguments: Vec<String>,
    },
    Validate(Value),
    Changed,
    Assert {
        holds: bool,
        message: String,
        detail: Value,
    },
    NeedModel(Value),
    Log(String),
}

/// What the machine answers.
#[derive(Debug)]
enum Served {
    /// The answer, whole. Refusals included: `{"ok": false, …}` is a value.
    Answer(Value),
    /// The run is over. The host function throws this and halts the engine, so
    /// a program cannot catch its way past it.
    Stop(String),
    /// Nothing to hand back — a log line, or an assertion that held.
    Nothing,
}

/// Everything the serving side keeps about a run.
struct Held<'a> {
    machine: &'a mut dyn Machine,
    limits: Limits,
    calls: Vec<CallRecord>,
    printed: Vec<String>,
    printed_bytes: usize,
    metrics: ProgramMetrics,
    stopped: Option<Stopped>,
}

impl Held<'_> {
    /// Note a stop, unless one is already noted.
    ///
    /// First one wins: a program interrupted by a ceiling should be reported as
    /// having hit the ceiling, and whatever the unwinding does after that is
    /// not the reason.
    fn stop(&mut self, why: Stopped) {
        if self.stopped.is_none() {
            self.stopped = Some(why);
        }
    }

    fn spend(&mut self, answer: &Value) {
        self.metrics.answer_bytes += answer.to_string().len();
        if self.metrics.answer_bytes > self.limits.answer_bytes {
            self.stop(Stopped::Exhausted {
                limit: "answer_bytes",
                message: format!(
                    "the program has taken in {} bytes of answers, past the {} it may; \
                     nothing was cut, the run was stopped",
                    self.metrics.answer_bytes, self.limits.answer_bytes
                ),
            });
        }
    }

    /// Whether the run may still do anything, and why not if it may not.
    fn may_continue(&mut self) -> Option<String> {
        if let Some(stopped) = &self.stopped {
            return Some(match stopped {
                Stopped::NeedsModel(_) => "the program has asked for the model".to_string(),
                Stopped::Assertion { message, .. } => {
                    format!("an assertion did not hold: {message}")
                }
                Stopped::Exhausted { message, .. } => message.clone(),
            });
        }
        let spent = self.metrics.requests + self.metrics.validations + self.metrics.observations;
        if spent >= self.limits.calls {
            let message = format!(
                "the program has asked the machine {spent} things, which is all {} it may",
                self.limits.calls
            );
            self.stop(Stopped::Exhausted {
                limit: "calls",
                message: message.clone(),
            });
            return Some(message);
        }
        let launched = self.machine.process_launches();
        if launched >= self.limits.process_launches {
            let message = format!(
                "the program has started {launched} process(es), which is all {} it may",
                self.limits.process_launches
            );
            self.stop(Stopped::Exhausted {
                limit: "process_launches",
                message: message.clone(),
            });
            return Some(message);
        }
        None
    }

    /// Serve one question from the engine.
    ///
    /// **The only place a program's request becomes a machine operation.** Every
    /// ceiling is checked before the work rather than after: a call that has
    /// already run cannot be un-run, and a limit that only notices afterwards
    /// is a limit that is always exceeded by one.
    fn serve(&mut self, ask: Ask) -> Served {
        match ask {
            Ask::Log(text) => {
                let room = self.limits.log_bytes.saturating_sub(self.printed_bytes);
                if room > 0 {
                    // Cut, and *said* to be cut. Nothing is lost in silence,
                    // and a log line is the one thing here whose whole purpose
                    // is to be read by a person.
                    let line = if text.len() > room {
                        let mut end = room;
                        while end > 0 && !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!(
                            "{}… (cut: the program has logged all {} bytes it may)",
                            &text[..end],
                            self.limits.log_bytes
                        )
                    } else {
                        text
                    };
                    self.printed_bytes += line.len();
                    self.printed.push(line);
                }
                Served::Nothing
            }
            Ask::Assert {
                holds,
                message,
                detail,
            } => {
                self.metrics.assertions += 1;
                if holds {
                    return Served::Nothing;
                }
                self.stop(Stopped::Assertion {
                    message: message.clone(),
                    detail,
                });
                Served::Stop(message)
            }
            Ask::NeedModel(value) => {
                self.stop(Stopped::NeedsModel(value));
                Served::Stop("the program asked for the model".to_string())
            }
            Ask::Request { verb, arguments } => {
                if let Some(why) = self.may_continue() {
                    return Served::Stop(why);
                }
                self.metrics.requests += 1;
                // Through `Machine::request`, which above this crate is
                // `external::one` — the same function, the same argument check,
                // the same workspace boundary a single request goes through. A
                // program is not a way to reach a verb that is not exposed.
                let answer = self.machine.request(&verb, &arguments);
                self.spend(&answer);
                self.calls.push(CallRecord {
                    kind: "request",
                    verb,
                    arguments,
                    ok: went_well(&answer),
                    answer: answer.clone(),
                });
                Served::Answer(answer)
            }
            Ask::Validate(check) => {
                if let Some(why) = self.may_continue() {
                    return Served::Stop(why);
                }
                self.metrics.validations += 1;
                let answer = self.machine.validate(&check);
                self.spend(&answer);
                self.calls.push(CallRecord {
                    kind: "validate",
                    verb: check
                        .get("check")
                        .and_then(Value::as_str)
                        .unwrap_or("check")
                        .to_string(),
                    arguments: Vec::new(),
                    ok: answer.get("verdict") == Some(&json!("passed")),
                    answer: answer.clone(),
                });
                Served::Answer(answer)
            }
            Ask::Changed => {
                if let Some(why) = self.may_continue() {
                    return Served::Stop(why);
                }
                self.metrics.observations += 1;
                let answer = self.machine.changed();
                self.spend(&answer);
                self.calls.push(CallRecord {
                    kind: "changed",
                    verb: "changed".to_string(),
                    arguments: Vec::new(),
                    ok: true,
                    answer: answer.clone(),
                });
                Served::Answer(answer)
            }
        }
    }
}

/// Whether the answer counts as the thing having worked.
///
/// A verb that answered is not a verb that succeeded: every refusal on this
/// surface is a well-formed object with `ok: false` in it, and reading "it
/// answered" as "it worked" is how a program carries on past the edit that did
/// not happen.
fn went_well(answer: &Value) -> bool {
    answer.get("ok").and_then(Value::as_bool).unwrap_or(false)
}

/// The flag the interrupt handler reads.
///
/// An atomic rather than the run's state, because the handler fires *while* a
/// host function is waiting for an answer — which is most of the time — and a
/// handler that took a lock would deadlock the first time a program called
/// anything.
#[derive(Clone, Default)]
struct Halt(Arc<AtomicBool>);

impl Halt {
    fn set(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// How much stack the engine's own thread gets.
///
/// Comfortably more than [`Limits::stack_bytes`]'s default, so that the
/// ceiling a runaway recursion meets is QuickJS's — which is a refusal — and
/// never this process's, which is a crash with no report in it.
const ENGINE_STACK: usize = 16 * 1024 * 1024;

/// Run a program and say what it did.
///
/// Never panics and never returns an error: every way this can go wrong is one
/// of [`Finish`]'s arms, because the caller is a transaction that has already
/// taken a snapshot and has to decide commit or rollback about *something*.
pub fn run(source: &str, machine: &mut dyn Machine, limits: &Limits) -> Outcome {
    let started = Instant::now();
    let verbs = machine.verbs();
    let mut held = Held {
        machine,
        limits: limits.clone(),
        calls: Vec::new(),
        printed: Vec::new(),
        printed_bytes: 0,
        metrics: ProgramMetrics::default(),
        stopped: None,
    };

    let (asking, asked) = std::sync::mpsc::sync_channel::<Ask>(0);
    let (answering, answered) = std::sync::mpsc::sync_channel::<Served>(0);

    let outcome = std::thread::scope(|scope| {
        let engine = std::thread::Builder::new()
            .name("thalyx-program".to_string())
            .stack_size(ENGINE_STACK)
            .spawn_scoped(scope, {
                let limits = limits.clone();
                let source = source.to_string();
                move || bind::execute(&source, asking, answered, verbs, &limits, started)
            });
        let engine = match engine {
            Ok(engine) => engine,
            Err(error) => {
                return Finish::Refused {
                    message: format!("the program's thread could not be started: {error}"),
                };
            }
        };

        // The serving loop, and it ends by itself: the engine holds the only
        // sender, so the iterator finishes exactly when the engine thread is
        // gone. A loop with its own idea of when to stop would be a second
        // opinion about whether the program had finished.
        for ask in asked {
            let served = held.serve(ask);
            if answering.send(served).is_err() {
                break;
            }
        }

        match engine.join() {
            Ok(finish) => finish,
            // A panic in the engine thread is a defect in Thalyx and not in the
            // program, and it is reported as one rather than as the program
            // having failed.
            Err(_) => Finish::Refused {
                message: "the program's runtime stopped unexpectedly; this is a defect in \
                          Thalyx and not in the program"
                    .to_string(),
            },
        }
    });

    held.metrics.wall_ms = started.elapsed().as_millis();

    // The latch wins over whatever the engine said. A run stopped by an
    // assertion comes back from QuickJS as "interrupted", which is true and is
    // not the reason anybody needs.
    let finish = match held.stopped.take() {
        Some(Stopped::NeedsModel(value)) => Finish::NeedsModel { value },
        Some(Stopped::Assertion { message, detail }) => Finish::Assertion { message, detail },
        Some(Stopped::Exhausted { limit, message }) => Finish::Exhausted { limit, message },
        None => outcome,
    };

    // Checked last, because a value is only too big once it exists — and
    // refused rather than cut, so a model is never handed half an answer to act
    // on. All of it is in the evidence.
    let finish = match &finish {
        Finish::Returned { value } | Finish::NeedsModel { value } => {
            let size = value.to_string().len();
            if size > limits.returned_bytes {
                Finish::Exhausted {
                    limit: "returned_bytes",
                    message: format!(
                        "the program returned {size} bytes and may return {}. It is not cut \
                         short here — a halved answer is one a model acts on believing it is \
                         whole — and the whole of it is in the evidence",
                        limits.returned_bytes
                    ),
                }
            } else {
                finish
            }
        }
        _ => finish,
    };

    Outcome {
        finish,
        calls: held.calls,
        printed: held.printed,
        metrics: held.metrics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine that answers from a script, so the runtime can be tested
    /// without a filesystem, a workspace or a transaction.
    ///
    /// Rule 8: a fake must model the property under test. The property here is
    /// **that an answer from one call reaches the next one**, so this fake
    /// answers differently depending on what it is asked — a fake that answered
    /// a constant would make every control-flow test pass by accident.
    #[derive(Default)]
    pub(crate) struct Recorder {
        pub asked: Vec<(String, Vec<String>)>,
        pub launches: usize,
    }

    impl Machine for Recorder {
        fn request(&mut self, verb: &str, arguments: &[String]) -> Value {
            self.asked.push((verb.to_string(), arguments.to_vec()));
            match verb {
                "read" => json!({
                    "ok": true, "op": "read",
                    "path": arguments.first().cloned().unwrap_or_default(),
                    "text": format!("line one\\nold_api in {}\\nline three\\n",
                                    arguments.first().cloned().unwrap_or_default()),
                }),
                "grep" => json!({"ok": true, "op": "grep", "total": 2}),
                "nope" => json!({"ok": false, "op": "nope", "error": "not_exposed"}),
                other => json!({"ok": true, "op": other, "arguments": arguments}),
            }
        }
        fn validate(&mut self, check: &Value) -> Value {
            self.launches += 1;
            json!({"check": check, "verdict": "passed", "summary": "the fake checked"})
        }
        fn changed(&mut self) -> Value {
            json!({"count": 2, "paths": ["a.rs", "b.rs"]})
        }
        fn process_launches(&self) -> usize {
            self.launches
        }
        fn verbs(&self) -> Vec<String> {
            vec!["read".into(), "edit".into(), "grep".into()]
        }
    }

    fn ran(source: &str) -> Outcome {
        let mut machine = Recorder::default();
        run(source, &mut machine, &Limits::default())
    }

    #[test]
    fn a_value_returned_by_the_program_comes_back() {
        let outcome = ran("return { hello: 1 + 1 };");
        assert_eq!(
            outcome.finish,
            Finish::Returned {
                value: json!({"hello": 2})
            }
        );
    }

    #[test]
    fn a_program_that_returns_nothing_returns_null_rather_than_failing() {
        // `undefined` is not JSON, and a runtime that turned "the program ended
        // without a return" into an error would make the commonest shape of a
        // program written for effect a failure.
        let outcome = ran("thalyx.log('done');");
        assert_eq!(outcome.finish, Finish::Returned { value: Value::Null });
        assert_eq!(outcome.printed, vec!["done".to_string()]);
    }

    #[test]
    fn an_endless_loop_is_stopped_by_the_clock() {
        // The claim a static list of steps never had to make. Nothing in this
        // program allocates or calls anything, so only the engine's own
        // interrupt can end it.
        let mut machine = Recorder::default();
        let limits = Limits {
            wall: Duration::from_millis(200),
            ..Limits::default()
        };
        let started = Instant::now();
        let outcome = run("while (true) {}", &mut machine, &limits);
        assert!(
            matches!(&outcome.finish, Finish::Exhausted { limit, .. } if *limit == "wall"),
            "{:?}",
            outcome.finish
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_loop_that_catches_its_own_interruption_is_still_stopped() {
        // QuickJS raises an interrupt as an ordinary JavaScript exception,
        // which means a program can catch it. So the adversarial shape is not
        // `while (true) {}` — it is a program that swallows being terminated
        // and keeps going, which is what a model that wraps everything in
        // `try`/`catch` writes without meaning to.
        let mut machine = Recorder::default();
        let limits = Limits {
            wall: Duration::from_millis(200),
            ..Limits::default()
        };
        let started = Instant::now();
        let outcome = run(
            "while (true) { try { for (let i = 0; i < 1e6; i++) {} } catch (e) { } }",
            &mut machine,
            &limits,
        );
        assert!(
            matches!(outcome.finish, Finish::Exhausted { .. }),
            "{:?}",
            outcome.finish
        );
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "it took {:?} to stop a loop that catches its own interruption",
            started.elapsed()
        );
    }

    #[test]
    fn allocating_without_end_is_a_refusal_rather_than_this_machine_running_out() {
        let mut machine = Recorder::default();
        let limits = Limits {
            memory_bytes: 4 * 1024 * 1024,
            wall: Duration::from_secs(20),
            ..Limits::default()
        };
        let outcome = run(
            "const held = []; for (;;) { held.push(new Array(50000).fill('x')); } ",
            &mut machine,
            &limits,
        );
        assert!(
            matches!(
                outcome.finish,
                Finish::Exhausted { .. } | Finish::Threw { .. }
            ),
            "{:?}",
            outcome.finish
        );
    }

    #[test]
    fn a_program_cannot_reach_the_machine_by_any_route_but_the_bound_one() {
        // Not a proof of the sandbox — the proof is that QuickJS has no I/O to
        // bind. What this catches is a future change: a convenience added to
        // the prelude, or a global left behind by a binding, that hands a
        // program something the verb table never approved.
        let outcome = ran(
            "const names = Object.getOwnPropertyNames(globalThis).sort();
             return names.filter((n) => !['thalyx'].includes(n) &&
                                        typeof globalThis[n] === 'object' &&
                                        globalThis[n] !== null);",
        );
        let value = outcome.value();
        let reachable = value.as_array().cloned().unwrap_or_default();
        // Only the language's own namespace objects, and the ones QuickJS
        // defines are `Math`, `JSON`, `Reflect` and `globalThis` itself. A new
        // name here is a new thing a program can touch and wants reading.
        let names: Vec<&str> = reachable.iter().filter_map(Value::as_str).collect();
        for name in &names {
            assert!(
                ["Math", "JSON", "Reflect", "Atomics", "globalThis"].contains(name),
                "`{name}` is reachable from a program and is not one of the language's own \
                 namespaces: {names:?}"
            );
        }
        // And the two that were reachable until 2026-08-30 and are not now.
        // Neither is authority; both are a clock, and a program that can read
        // one can behave differently depending on how busy this machine is —
        // which makes a transaction nobody can reproduce from its evidence.
        for clock in ["Date", "performance"] {
            let outcome = ran(&format!("return typeof {clock};"));
            assert_eq!(
                outcome.value(),
                json!("undefined"),
                "a program can read the clock through `{clock}`"
            );
        }
    }

    #[test]
    fn arguments_that_are_not_words_are_refused_by_name() {
        // A silent `String(undefined)` would send the machine the *word*
        // "undefined" as a path — a request that is well formed, is refused for
        // the wrong reason, and costs a caller a round trip to work out that
        // its variable was empty.
        let mut machine = Recorder::default();
        let outcome = run(
            "let missing; return thalyx.read(missing);",
            &mut machine,
            &Limits::default(),
        );
        assert!(
            matches!(&outcome.finish, Finish::Threw { message } if message.contains("undefined")),
            "{:?}",
            outcome.finish
        );
        assert!(machine.asked.is_empty(), "{:?}", machine.asked);
    }

    #[test]
    fn a_loop_that_calls_forever_runs_out_of_calls() {
        // The other runaway, and the one that costs the *machine* rather than
        // the clock: a program looping over requests. `MOST_STEPS` used to
        // bound this by construction; a language cannot be bounded by
        // construction, so it is bounded by counting.
        let mut machine = Recorder::default();
        let limits = Limits {
            calls: 12,
            ..Limits::default()
        };
        let outcome = run(
            "for (let i = 0; i < 1000; i++) { thalyx.read('f' + i); } return 'never';",
            &mut machine,
            &limits,
        );
        assert!(
            matches!(&outcome.finish, Finish::Exhausted { limit, .. } if *limit == "calls"),
            "{:?}",
            outcome.finish
        );
        assert_eq!(
            machine.asked.len(),
            12,
            "the ceiling let extra calls through"
        );
    }

    #[test]
    fn an_assertion_that_fails_cannot_be_caught_by_the_program() {
        // **The latch.** A program written by a language model wraps things in
        // `try`/`catch`, and an assertion that were only an exception would be
        // swallowed by exactly that habit — leaving a run that carried on past
        // the premise it had just disproved, and committed.
        let mut machine = Recorder::default();
        let outcome = run(
            "try { thalyx.assert(1 === 2, 'one is not two'); } catch (e) { }
             thalyx.read('after.rs');
             return 'the program continued';",
            &mut machine,
            &Limits::default(),
        );
        assert!(
            matches!(&outcome.finish, Finish::Assertion { message, .. } if message.contains("one is not two")),
            "{:?}",
            outcome.finish
        );
        assert!(
            machine.asked.is_empty(),
            "the machine was asked for something after the assertion failed: {:?}",
            machine.asked
        );
    }

    #[test]
    fn an_assertion_that_holds_costs_nothing_and_is_counted() {
        let outcome =
            ran("thalyx.assert(true, 'fine'); thalyx.assert(1 < 2, 'also fine'); return 'ok';");
        assert_eq!(outcome.finish, Finish::Returned { value: json!("ok") });
        assert_eq!(outcome.metrics.assertions, 2);
    }

    #[test]
    fn a_program_can_ask_for_the_model_without_having_failed() {
        let outcome = ran("return thalyx.needModel({ candidates: [1, 2, 3] });");
        assert_eq!(
            outcome.finish,
            Finish::NeedsModel {
                value: json!({"candidates": [1, 2, 3]})
            }
        );
        assert!(!outcome.finish.went_through());
        assert_eq!(outcome.finish.word(), "needs_model");
    }

    #[test]
    fn nothing_of_the_host_is_reachable_from_a_program() {
        // The claim that makes "untrusted code" true rather than hopeful. None
        // of these exist in QuickJS's core; the test is here so that a future
        // change which adds a convenience module is caught by something rather
        // than by nobody.
        for name in [
            "require",
            "process",
            "fetch",
            "XMLHttpRequest",
            "WebAssembly",
            "std",
            "os",
            "Deno",
            "globalThis.process",
            "import",
        ] {
            let outcome = ran(&format!("return typeof {name};"));
            let value = outcome.value();
            assert!(
                value == json!("undefined") || matches!(outcome.finish, Finish::Threw { .. }),
                "`{name}` is reachable from a program: {outcome:?}"
            );
        }
    }

    #[test]
    fn a_refusal_is_a_value_the_program_can_branch_on() {
        // Not an error. A verb that refuses is information, and a program that
        // could not read it would have to end at the first mistake it made —
        // which is the thing that costs a round trip.
        let outcome = ran("const answer = thalyx.call('nope', []);
             if (answer.ok) { return 'wrong'; }
             return { caught: answer.error };");
        assert_eq!(
            outcome.finish,
            Finish::Returned {
                value: json!({"caught": "not_exposed"})
            }
        );
    }

    #[test]
    fn an_answer_from_one_call_decides_whether_a_later_one_happens() {
        // **The defining property of the whole sprint**, at the smallest scale
        // it can be stated: the machine answered something, the program read
        // it, and what it read is what determined the next request. A
        // hardcoded list of steps cannot express this, which is why the fake
        // above answers differently per verb rather than answering a constant.
        let mut machine = Recorder::default();
        let outcome = run(
            "const window = thalyx.read('one.rs');
             if (window.text.includes('old_api')) {
                 thalyx.edit('one.rs', 'sustituir', 'old_api', 'new_api');
             }
             const other = thalyx.read('two.rs');
             if (other.text.includes('nothing like this')) {
                 thalyx.edit('two.rs', 'sustituir', 'a', 'b');
             }
             return thalyx.call('grep', ['new_api']).total;",
            &mut machine,
            &Limits::default(),
        );
        assert_eq!(outcome.finish, Finish::Returned { value: json!(2) });
        let verbs: Vec<&str> = machine
            .asked
            .iter()
            .map(|(verb, _)| verb.as_str())
            .collect();
        assert_eq!(
            verbs,
            ["read", "edit", "read", "grep"],
            "the edit that should have been skipped ran, or the one that should \
             have happened did not: {:?}",
            machine.asked
        );
    }

    #[test]
    fn what_a_program_reads_is_far_more_than_what_it_returns() {
        // The compression, as a number rather than as a claim. The program
        // reads three whole files and hands back one integer.
        let outcome = ran("let hits = 0;
             for (const name of ['a.rs', 'b.rs', 'c.rs']) {
                 if (thalyx.read(name).text.includes('old_api')) { hits++; }
             }
             return hits;");
        assert_eq!(outcome.finish, Finish::Returned { value: json!(3) });
        assert!(outcome.metrics.answer_bytes > 100, "{:?}", outcome.metrics);
        assert!(outcome.metrics.requests == 3);
    }

    #[test]
    fn an_answer_too_big_to_hand_back_is_refused_and_not_halved() {
        let mut machine = Recorder::default();
        let limits = Limits {
            returned_bytes: 64,
            ..Limits::default()
        };
        let outcome = run("return 'x'.repeat(500);", &mut machine, &limits);
        assert!(
            matches!(&outcome.finish, Finish::Exhausted { limit, .. } if *limit == "returned_bytes"),
            "{:?}",
            outcome.finish
        );
        assert!(
            outcome.finish.why().contains("evidence"),
            "{}",
            outcome.finish.why()
        );
    }

    #[test]
    fn a_program_that_is_not_javascript_is_refused_rather_than_run() {
        let outcome = ran("this is not a program {{{");
        assert!(
            matches!(
                outcome.finish,
                Finish::Threw { .. } | Finish::Refused { .. }
            ),
            "{:?}",
            outcome.finish
        );
    }

    #[test]
    fn every_call_is_recorded_whole_for_the_evidence() {
        let outcome = ran(
            "thalyx.read('a.rs'); thalyx.validate({check: 'parses'}); thalyx.changed(); return 1;",
        );
        let kinds: Vec<&str> = outcome.calls.iter().map(|call| call.kind).collect();
        assert_eq!(kinds, ["request", "validate", "changed"]);
        assert!(
            outcome.calls[0].answer["text"].is_string(),
            "the answer was summarised instead of kept: {:?}",
            outcome.calls[0]
        );
    }

    #[test]
    fn recursion_without_end_is_a_refusal_and_not_this_process_falling_over() {
        let mut machine = Recorder::default();
        let limits = Limits {
            stack_bytes: 64 * 1024,
            ..Limits::default()
        };
        let outcome = run(
            "function down(n) { return down(n + 1); } return down(0);",
            &mut machine,
            &limits,
        );
        assert!(
            matches!(
                outcome.finish,
                Finish::Threw { .. } | Finish::Exhausted { .. }
            ),
            "{:?}",
            outcome.finish
        );
    }
}
