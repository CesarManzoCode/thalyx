//! Where the engine meets the machine: five host functions and a prelude.
//!
//! Everything a program can do that is not arithmetic goes through one of the
//! functions bound here, and every one of them is a call into [`Machine`],
//! which is implemented by whoever owns the transaction. Nothing in this file
//! opens a file, resolves a path, starts a process or decides what an answer
//! means.
//!
//! ## Why the ergonomics are JavaScript and not Rust
//!
//! `thalyx.read(path)` is one line of [`PRELUDE`] over `thalyx.call`. Writing
//! it in Rust instead would mean a second host binding per convenience — a
//! second thing to keep in step with the verb table, a second place a boundary
//! check could be forgotten, and a second surface to describe. The prelude is
//! *inside* the sandbox, has no more authority than the program that follows
//! it, and is read by the same engine.
//!
//! Which is the same argument the MCP surface makes one level up: an operation
//! does not need its own schema to be reachable, and a capability that can be
//! composed cheaply should be composed rather than enumerated.

use crate::{Ask, Finish, Halt, Limits, Served};
use rquickjs::{Context, Ctx, Function, Object, Runtime, Value as JsValue};
use serde_json::{Value, json};
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Instant;

pub const PRELUDE: &str = r#"
(function () {
  "use strict";

  // Arguments to the machine are strings — a request is a verb and a list of
  // them. Numbers and booleans are coerced because writing `line + 1` is
  // natural; everything else is refused **by name**, because a silent
  // `String(undefined)` sends the machine the word "undefined" as a path.
  function word(value, where) {
    const kind = typeof value;
    if (kind === "string") { return value; }
    if (kind === "number" || kind === "boolean") { return String(value); }
    throw new TypeError(
      "thalyx." + where + ": every argument must be a string, and one is " +
      (value === null ? "null" : kind)
    );
  }

  // Nothing is dropped here, and that is the fix rather than the tidy version.
  // Skipping an `undefined` turned `thalyx.read(missing)` into `read` with no
  // arguments, which the machine answered about the current directory — a
  // well-formed request, a plausible answer, and an empty variable nobody was
  // told about.
  function words(given, where) {
    const out = [];
    for (const value of given) {
      if (Array.isArray(value)) {
        for (const inner of value) { out.push(word(inner, where)); }
      } else {
        out.push(word(value, where));
      }
    }
    return out;
  }

  thalyx.call = function (verb, args) {
    return thalyx.__call(word(verb, "call"), words(args === undefined ? [] : args, "call"));
  };

  // ── looking ──────────────────────────────────────────────────────────────
  thalyx.state = () => thalyx.call("state", []);
  thalyx.where = () => thalyx.call("where", []);
  thalyx.list = (path, ...options) => thalyx.call("list", [path === undefined ? "." : path, ...options]);
  thalyx.read = (path) => thalyx.call("read", [path]);
  thalyx.grep = (text, ...options) => thalyx.call("grep", [...options, text]);
  thalyx.find = (pattern, ...options) => thalyx.call("find", [pattern, ...options]);
  thalyx.symbol = (name, ...options) => thalyx.call("symbol", [name, ...options]);
  thalyx.dependsOn = (path, ...options) => thalyx.call("depends_on", [path, ...options]);
  thalyx.dependedOnBy = (path, ...options) => thalyx.call("depended_on_by", [path, ...options]);
  thalyx.describe = (verb) => thalyx.call("describe", [verb]);

  // ── what a name is ───────────────────────────────────────────────────────
  thalyx.context = (query, ...options) => thalyx.call("context", [query, ...options]);
  thalyx.rename = (from, to) => thalyx.call("rename", [from, to]);

  // ── changing the workspace ───────────────────────────────────────────────
  thalyx.edit = (path, action, ...rest) => thalyx.call("edit", [path, action, ...rest]);
  thalyx.substitute = (path, before, after, ...more) =>
    thalyx.call("edit", [path, "sustituir", before, after, ...more]);
  thalyx.write = (path, line, text) => thalyx.call("edit", [path, "cambiar", line, text]);
  thalyx.append = (path, text) => thalyx.call("edit", [path, "poner", text]);
  thalyx.makeFile = (...paths) => thalyx.call("make_file", paths);
  thalyx.makeDirectory = (...paths) => thalyx.call("make_directory", paths);
  thalyx.copy = (from, to) => thalyx.call("copy", [from, to]);
  thalyx.move = (from, to) => thalyx.call("move", [from, to]);
  thalyx.remove = (...paths) => thalyx.call("remove", paths);

  // ── reading a window rather than a file ──────────────────────────────────
  //
  // Inside the program a whole file is cheap, so this is not about cost — it
  // is about writing the loop that looks at what surrounds a reference without
  // spelling the slicing out every time.
  thalyx.window = function (path, line, around) {
    const answer = thalyx.read(path);
    if (!answer.ok || typeof answer.text !== "string") { return answer; }
    const lines = answer.text.split("\n");
    const reach = around === undefined ? 4 : around;
    const from = Math.max(1, line - reach);
    const through = Math.min(lines.length, line + reach);
    return {
      ok: true,
      path: path,
      from: from,
      through: through,
      text: lines.slice(from - 1, through).join("\n"),
    };
  };

  // ── premises ─────────────────────────────────────────────────────────────
  //
  // `mustWork` is the one an agent needs on every mutating call and forgets on
  // every one: a verb that answered is not a verb that succeeded.
  thalyx.mustWork = function (answer, what) {
    thalyx.assert(answer && answer.ok === true, what, answer);
    return answer;
  };

  // The same for a validation, whose "did it work" is a verdict and not `ok`.
  // Three outcomes and not two: `not_proven` is neither, and a program that
  // treated it as a pass would commit over a check that never ran.
  thalyx.mustPass = function (record, what) {
    thalyx.assert(
      record && record.verdict === "passed",
      what === undefined
        ? ("a check did not hold: " + (record && record.summary))
        : what,
      record
    );
    return record;
  };

  Object.freeze(thalyx);
})();
"#;

/// The program, wrapped so that every way it can end is a value.
///
/// The wrapper is JavaScript rather than Rust error handling because the two
/// ordinary endings — returning and throwing — both happen *inside* the engine,
/// and a `catch` there can tell them apart while an `Err` outside cannot. What
/// the Rust side still handles is the two endings the language cannot see: a
/// ceiling, and a latch.
fn wrapped(program: &str) -> String {
    format!(
        r#"(function () {{
  "use strict";
  try {{
    const value = (function () {{
{program}
    }})();
    if (value && typeof value.then === "function") {{
      return {{ kind: "threw", message:
        "this runtime is synchronous: every thalyx call returns its answer "
        + "directly and there is nothing here that awaits. Return the value "
        + "itself rather than a promise." }};
    }}
    return {{ kind: "returned", value: value === undefined ? null : value }};
  }} catch (error) {{
    // The message **and** the stack, in that order.
    //
    // QuickJS's `error.stack` is only the frames — unlike V8's, it does not
    // begin with the message — so a wrapper that preferred `.stack` reported
    // every failure as a list of line numbers with the reason missing. Found
    // by a test that asserted the message said which argument was empty and
    // got seven frames instead. Rule 5: the instrument includes the harness,
    // and here the harness is somebody's memory of another engine.
    const said = (error instanceof Error)
      ? (String(error.name || "Error") + ": " + String(error.message) +
         (error.stack ? "\n" + String(error.stack) : ""))
      : (error && typeof error === "object") ? JSON.stringify(error)
      : String(error);
    return {{ kind: "threw", message: said }};
  }}
}})()"#
    )
}

/// Turn a JavaScript value into JSON, or say it could not be.
///
/// `JSON.stringify` and not a hand-written walk: a value with a cycle in it, a
/// `BigInt`, a function or a `Symbol` are all things a program can produce, and
/// the engine's own serialiser already has a settled answer for every one of
/// them. Writing a second one would be rule 6 in the small — do not
/// re-implement a format the tool will tell you about.
fn to_json(value: &JsValue<'_>) -> Value {
    if value.is_undefined() || value.is_null() {
        return Value::Null;
    }
    let ctx = value.ctx();
    match ctx.json_stringify(value.clone()) {
        Ok(Some(text)) => text
            .to_string()
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null),
        // A value JSON cannot carry — a function, a `Symbol`. Named rather than
        // dropped: a program that returned one should be told that is what
        // happened.
        Ok(None) | Err(_) => json!({"unrepresentable": true}),
    }
}

fn from_json<'js>(ctx: &Ctx<'js>, value: &Value) -> rquickjs::Result<JsValue<'js>> {
    ctx.json_parse(value.to_string())
}

/// The engine's half of the conversation with the machine.
///
/// Cloned into every bound function. Holding the channel ends rather than the
/// machine itself is what makes these closures `'static`, which is what
/// `rquickjs` requires and what lets a real transaction — which borrows a
/// store, a session and a workspace — be the thing on the other end.
struct Line {
    asking: SyncSender<Ask>,
    answered: Receiver<Served>,
    halt: Halt,
}

impl Line {
    /// Ask the machine one thing and wait for the answer.
    ///
    /// A [`Served::Stop`] halts the engine **and** throws. Both, because they
    /// do different jobs: the throw ends the statement the program is in, and
    /// the halt ends the run whatever the program does with the throw —
    /// including catching it, which is the habit this exists to be safe
    /// against.
    fn ask<'js>(&self, ctx: &Ctx<'js>, ask: Ask) -> rquickjs::Result<JsValue<'js>> {
        if self.halt.is_set() {
            return Err(ctx.throw(from_json(
                ctx,
                &json!({"stopped": "the run has already been stopped"}),
            )?));
        }
        if self.asking.send(ask).is_err() {
            self.halt.set();
            return Err(ctx.throw(from_json(
                ctx,
                &json!({"stopped": "the machine stopped answering"}),
            )?));
        }
        match self.answered.recv() {
            Ok(Served::Answer(answer)) => from_json(ctx, &answer),
            Ok(Served::Nothing) => Ok(JsValue::new_undefined(ctx.clone())),
            Ok(Served::Stop(why)) => {
                self.halt.set();
                Err(ctx.throw(from_json(ctx, &json!({"stopped": why}))?))
            }
            Err(_) => {
                self.halt.set();
                Err(ctx.throw(from_json(
                    ctx,
                    &json!({"stopped": "the machine stopped answering"}),
                )?))
            }
        }
    }
}

/// Build the engine, bind the capabilities, run the program.
///
/// Runs on the program's own thread; everything it needs from the machine
/// travels over `asking`/`answered`.
pub(crate) fn execute(
    source: &str,
    asking: SyncSender<Ask>,
    answered: Receiver<Served>,
    verbs: Vec<String>,
    limits: &Limits,
    started: Instant,
) -> Finish {
    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            return Finish::Refused {
                message: format!("the runtime could not be built: {error}"),
            };
        }
    };
    runtime.set_memory_limit(limits.memory_bytes);
    runtime.set_max_stack_size(limits.stack_bytes);

    // The interrupt, which is the only thing that can end a loop with nothing
    // in it. It reads an atomic rather than any of the run's state, because it
    // fires while a host function is blocked waiting for the machine — and a
    // handler that waited for a lock would deadlock the first time a program
    // called anything.
    let halt = Halt::default();
    let deadline = started + limits.wall;
    let ticks = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    // Whether the engine was ever told to stop.
    //
    // Kept separately from `halt` because of what QuickJS does with an
    // interrupt: it raises an ordinary JavaScript exception, and an ordinary
    // JavaScript exception is **catchable**. So a program inside a `try` — or
    // for that matter the wrapper's own `catch` — turns "this run was
    // terminated" into "the program threw", which is a sentence about the
    // program rather than about the ceiling it hit.
    //
    // Found by the test for `while (true) {}`, which reported `Threw:
    // interrupted` and would have let every ceiling in this file be reported
    // as the program's own fault.
    let interrupted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let halt = halt.clone();
        let ticks = ticks.clone();
        let interrupted = interrupted.clone();
        let budget = limits.ticks;
        runtime.set_interrupt_handler(Some(Box::new(move || {
            let seen = ticks.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let stop = halt.is_set() || Instant::now() >= deadline || seen > budget;
            if stop {
                interrupted.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            stop
        })));
    }

    // The language, and **no clock**.
    //
    // `Date` and `performance` are left out on purpose, and the reason is not
    // security — a timer reaches nothing. It is that a program which can read
    // the clock can behave differently depending on how busy this machine is,
    // and a transaction whose outcome depends on that is one nobody can
    // reproduce from its evidence. The runtime owns the wall time; the program
    // is not asked to have an opinion about it.
    //
    // Everything else the language has stays: a model writing JavaScript
    // expects `Map`, `RegExp`, `Proxy` and typed arrays to be there, and none
    // of them can reach anything.
    let context = match Context::builder()
        .with::<(
            rquickjs::context::intrinsic::Eval,
            rquickjs::context::intrinsic::RegExpCompiler,
            rquickjs::context::intrinsic::RegExp,
            rquickjs::context::intrinsic::Json,
            rquickjs::context::intrinsic::Proxy,
            rquickjs::context::intrinsic::MapSet,
            rquickjs::context::intrinsic::TypedArrays,
            rquickjs::context::intrinsic::Promise,
            rquickjs::context::intrinsic::WeakRef,
        )>()
        .build(&runtime)
    {
        Ok(context) => context,
        Err(error) => {
            return Finish::Refused {
                message: format!("the context could not be built: {error}"),
            };
        }
    };

    let line = std::rc::Rc::new(Line {
        asking,
        answered,
        halt: halt.clone(),
    });

    context.with(|ctx| {
        if let Err(error) = install(&ctx, &line, verbs) {
            return Finish::Refused {
                message: format!("the capabilities could not be bound: {error}"),
            };
        }
        if let Err(error) = ctx.eval::<(), _>(PRELUDE) {
            return Finish::Refused {
                message: format!(
                    "the prelude did not load, which is a defect in Thalyx and not in the \
                     program: {}",
                    said(&ctx, &error)
                ),
            };
        }

        match ctx.eval::<JsValue, _>(wrapped(source)) {
            // An interrupt beats whatever the wrapper managed to say. It is an
            // ordinary catchable exception inside the engine, so a run that was
            // terminated can arrive here looking like a program that threw —
            // and reporting a ceiling as the program's mistake is the kind of
            // wrong diagnosis rule 5 is about.
            Ok(_) if interrupted.load(std::sync::atomic::Ordering::Relaxed) => Finish::Exhausted {
                limit: limit_reached(started, limits, &ticks),
                message: "the program was stopped before it finished".to_string(),
            },
            Ok(value) => {
                let value = to_json(&value);
                match value.get("kind").and_then(Value::as_str) {
                    Some("returned") => Finish::Returned {
                        value: value.get("value").cloned().unwrap_or(Value::Null),
                    },
                    Some("threw") => Finish::Threw {
                        message: value
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("something with no message")
                            .to_string(),
                    },
                    // Cannot happen through `wrapped`, which returns one of two
                    // shapes. Written so a future change to the wrapper is a
                    // refusal rather than a run reported as having returned
                    // nothing.
                    _ => Finish::Refused {
                        message: format!("the runtime produced a shape it does not know: {value}"),
                    },
                }
            }
            // Interrupted, out of memory, a stack overflow, or a program that
            // is not JavaScript at all. Which one is decided here only when the
            // serving side has nothing better to say: a latch — an assertion, a
            // ceiling met inside a host call — also arrives as an interrupt,
            // and `run` prefers the latch's reason because it is the useful one.
            Err(error) => {
                let stopped =
                    halt.is_set() || interrupted.load(std::sync::atomic::Ordering::Relaxed);
                if matches!(error, rquickjs::Error::Exception) && !stopped {
                    // A syntax error in the program itself never reaches the
                    // `try` inside `wrapped`, because the whole wrapper failed
                    // to compile. It is the program's fault and is reported as
                    // such rather than as a ceiling nobody reached.
                    return Finish::Threw {
                        message: said(&ctx, &error),
                    };
                }
                Finish::Exhausted {
                    limit: limit_reached(started, limits, &ticks),
                    message: format!("the program was stopped: {}", said(&ctx, &error)),
                }
            }
        }
    })
}

/// Which ceiling a stopped run reached, named in the units it is counted in.
///
/// Asked of the clock and the counter rather than of the engine, because
/// QuickJS reports "interrupted" for every one of them and a report that said
/// only that would leave a person unable to tell a slow machine from a runaway
/// loop.
fn limit_reached(
    started: Instant,
    limits: &Limits,
    ticks: &std::sync::atomic::AtomicU64,
) -> &'static str {
    if started.elapsed() >= limits.wall {
        return "wall";
    }
    if ticks.load(std::sync::atomic::Ordering::Relaxed) > limits.ticks {
        return "ticks";
    }
    "memory"
}

/// What the engine actually said, which for an exception is not the error type.
fn said(ctx: &Ctx<'_>, error: &rquickjs::Error) -> String {
    if matches!(error, rquickjs::Error::Exception) {
        let caught = ctx.catch();
        if let Some(exception) = caught.as_exception() {
            return format!("{exception}");
        }
        return to_json(&caught).to_string();
    }
    error.to_string()
}

/// Bind everything a program can do that is not arithmetic.
fn install<'js>(
    ctx: &Ctx<'js>,
    line: &std::rc::Rc<Line>,
    verbs: Vec<String>,
) -> rquickjs::Result<()> {
    let thalyx = Object::new(ctx.clone())?;

    // ── the one door ────────────────────────────────────────────────────────
    {
        let line = line.clone();
        let call = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>,
                  verb: String,
                  arguments: Vec<String>|
                  -> rquickjs::Result<JsValue<'js>> {
                line.ask(&ctx, Ask::Request { verb, arguments })
            },
        )?;
        thalyx.set("__call", call)?;
    }

    // ── validation, which is the same checker the whole verb uses ───────────
    {
        let line = line.clone();
        let validate = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, check: JsValue<'js>| -> rquickjs::Result<JsValue<'js>> {
                let asked = to_json(&check);
                line.ask(&ctx, Ask::Validate(asked))
            },
        )?;
        thalyx.set("validate", validate)?;
    }

    // ── what the tree really shows ──────────────────────────────────────────
    {
        let line = line.clone();
        let changed = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>| -> rquickjs::Result<JsValue<'js>> { line.ask(&ctx, Ask::Changed) },
        )?;
        thalyx.set("changed", changed)?;
    }

    // ── premises, which stop the run rather than raising an exception ───────
    {
        let line = line.clone();
        let assert = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>,
                  holds: bool,
                  message: rquickjs::function::Opt<String>,
                  detail: rquickjs::function::Opt<JsValue<'js>>|
                  -> rquickjs::Result<JsValue<'js>> {
                let detail = detail.0.as_ref().map(to_json).unwrap_or(Value::Null);
                let message = message
                    .0
                    .unwrap_or_else(|| "an assertion did not hold".to_string());
                line.ask(
                    &ctx,
                    Ask::Assert {
                        holds,
                        message,
                        detail,
                    },
                )
            },
        )?;
        thalyx.set("assert", assert)?;
    }

    // ── asking for the model ────────────────────────────────────────────────
    {
        let line = line.clone();
        let need = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>,
                  value: rquickjs::function::Opt<JsValue<'js>>|
                  -> rquickjs::Result<JsValue<'js>> {
                let value = value.0.as_ref().map(to_json).unwrap_or(Value::Null);
                line.ask(&ctx, Ask::NeedModel(value))
            },
        )?;
        thalyx.set("needModel", need)?;
    }

    // ── saying something, bounded ───────────────────────────────────────────
    {
        let line = line.clone();
        let log = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, text: String| -> rquickjs::Result<JsValue<'js>> {
                line.ask(&ctx, Ask::Log(text))
            },
        )?;
        thalyx.set("log", log)?;
    }

    // ── what this session can reach, so a program can look before it asks ──
    thalyx.set("verbs", verbs)?;

    ctx.globals().set("thalyx", thalyx)?;
    Ok(())
}
