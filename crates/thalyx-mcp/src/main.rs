//! `thalyx-mcp` — the adapter that puts a Thalyx machine inside a programming
//! agent that runs on the host.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md`. During the adoption phase
//! Claude Code, Codex and the editors stay on Fedora and Thalyx is the machine
//! they work *on*. This process is the whole of the joint: MCP on one side,
//! Thalyx's own protocol on the other, and **no third thing** in between.
//!
//! ## What it must never grow
//!
//! A filesystem. A graph. A rollback. A copy of `catalogue.rs`. The decree is
//! that MCP is an adapter and Thalyx's surface is the authority, and the way
//! that decree dies is one convenience at a time — a cache of file contents
//! "for speed", a path normaliser "so the model does not have to", a retry that
//! turns a refusal into a success. Every one of those makes this process a
//! second, worse Thalyx that disagrees with the first.
//!
//! So the rule is mechanical: **nothing in this crate may open a file of the
//! workspace.** It opens a socket and it writes a metrics summary. That is all.
//!
//! ## And what it checks before it advertises anything
//!
//! The machine's hello names the verbs it will accept. A tool whose verbs are
//! not in that list is **not offered**, and is not offered silently to the
//! model but loudly on stderr, where the person running the experiment can see
//! it. A version skew that quietly dropped a tool would look like a model that
//! chose not to use one — which is exactly the measurement this is here to make.

mod machine;
mod metrics;
mod tools;

use clap::Parser;
use machine::{Machine, Trouble};
use metrics::Metrics;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::PathBuf;

/// The MCP revision this speaks. Echoed back to a client that asked for one it
/// also supports, which is what the specification asks a server to do.
const MCP_PROTOCOL: &str = "2025-06-18";

#[derive(Parser)]
#[command(
    name = "thalyx-mcp",
    version,
    about = "Give a programming agent a Thalyx machine to work in"
)]
struct Cli {
    /// The QEMU chardev socket the machine's agent port is on
    #[arg(long, default_value = "/tmp/thalyx-agent.sock")]
    connect: PathBuf,

    /// Where to keep a running summary of what this session cost
    #[arg(long)]
    metrics: Option<PathBuf>,

    /// How long to wait for the machine to come up before giving up
    ///
    /// A VM that is still booting is the ordinary case, not an error.
    #[arg(long, default_value_t = 30)]
    wait: u64,

    /// Ask the machine whether it is alive and what workspace it is holding,
    /// print that as one JSON object, and exit without serving anything
    ///
    /// The benchmark's arm B is the expensive half and it is the half that can
    /// be dead in a way a socket cannot show. On 2026-08-29 a paid run spent
    /// arm A in full and then arm B produced `0s wall, 0 stream events`: the
    /// socket was there, the machine behind it was not. `[ -S "$SOCKET" ]` is a
    /// question about a file. This is a question about the machine, it costs
    /// nothing, and it is the same code path the run itself uses — a probe
    /// written against a mock would prove the mock.
    ///
    /// Read-only, on purpose and by construction: `where` and `list` are the
    /// only verbs it asks, and neither can change the workspace it is checking.
    #[arg(long)]
    preflight: bool,
}

fn main() {
    let cli = Cli::parse();
    let mut metrics = Metrics::new(cli.metrics.clone());

    // Connected before the first message, so that a machine that is not there is
    // an error the person starting this sees rather than one the model meets
    // halfway through a task and reasons about.
    let mut machine = match Machine::connect(&cli.connect, std::time::Duration::from_secs(cli.wait))
    {
        Ok(machine) => machine,
        Err(trouble) => {
            eprintln!("thalyx-mcp: {trouble}");
            std::process::exit(1);
        }
    };
    let greeting = machine.greeting().clone();
    eprintln!(
        "thalyx-mcp: Thalyx {} on {}, workspace {}",
        greeting.thalyx,
        cli.connect.display(),
        greeting.workspace
    );

    let offered = usable(&greeting.verbs);
    eprintln!(
        "thalyx-mcp: {} of {} tools offered",
        offered.len(),
        tools::TOOLS.len()
    );

    if cli.preflight {
        let (report, ready) = preflight(&mut machine, &greeting, offered.len());
        println!("{report}");
        std::process::exit(i32::from(!ready));
    }

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message): Result<Value, _> = serde_json::from_str(&line) else {
            // A client that sent something unparseable has no id to answer, so
            // there is nothing to reply to. Said on stderr rather than
            // swallowed, which is the difference between a bug and a mystery.
            eprintln!("thalyx-mcp: a message that is not JSON was ignored");
            continue;
        };

        let Some(reply) = handle(&message, &mut machine, &offered, &mut metrics) else {
            // A notification. The specification says a notification is never
            // answered, and answering one is how a client ends up waiting for a
            // response to a response.
            continue;
        };
        if writeln!(stdout, "{reply}").is_err() || stdout.flush().is_err() {
            break;
        }
    }

    eprintln!("thalyx-mcp: {}", metrics.object());
}

/// The tools this machine can actually serve.
///
/// The anti-drift check, and it is a real one rather than a copy: the verbs come
/// from the machine's own hello, so a tool built against a verb this Thalyx does
/// not have is dropped here instead of failing on the model's first use of it.
fn usable(verbs: &[String]) -> Vec<&'static tools::Tool> {
    tools::TOOLS
        .iter()
        .filter(|tool| {
            let missing: Vec<&str> = tool
                .verbs
                .iter()
                .copied()
                .filter(|verb| !verbs.iter().any(|had| had == verb))
                .collect();
            if !missing.is_empty() {
                eprintln!(
                    "thalyx-mcp: {} is not offered — this machine has no {}",
                    tool.name,
                    missing.join(", ")
                );
            }
            missing.is_empty()
        })
        .collect()
}

/// Is the machine behind the socket alive, and is it holding the project the
/// experiment is about?
///
/// `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md`. The 2026-08-29 run paid
/// for arm A in full and then arm B came back `0s wall, 0 stream events`, and
/// the only check standing between the money and that outcome was
/// `[ -S "$SOCKET" ]` — which asks whether a *file* exists. QEMU creates that
/// file the instant it starts and holds it open whether or not anything inside
/// the guest ever answers. So the socket is necessary and it is nowhere near
/// sufficient, and the difference cost a whole arm.
///
/// What this asks instead, over the real channel, with the real adapter, in
/// under a second and for nothing:
///
///   1. **the hello**, which `Machine::connect` has already waited for and
///      checked the protocol of — a machine that never says hello never gets
///      here;
///   2. **`where`**, the smallest round trip there is: a request goes down the
///      wire and an answer with a matching id comes back up. A bridge that
///      accepts and then says nothing fails here and not in the middle of a
///      paid run;
///   3. **`list .`**, whose entry names are the only free evidence this host
///      can get that the workspace inside the machine is the project it was
///      told to import. A stale store from last week answers every question
///      correctly and answers them *about the wrong tree*.
///
/// Both verbs are reads. That is a requirement and not a coincidence: a probe
/// that opened an attempt, or touched a file to see whether writing worked,
/// would have changed the starting state of the very run it was clearing —
/// and the `reversible` task's whole verdict is a comparison against that
/// starting state.
fn preflight(machine: &mut Machine, greeting: &machine::Greeting, offered: usize) -> (Value, bool) {
    let mut report = json!({
        "protocol": thalyx_bridge::PROTOCOL,
        "thalyx": greeting.thalyx,
        "workspace": greeting.workspace,
        "verbs": greeting.verbs,
        "tools_offered": offered,
        "tools_total": tools::TOOLS.len(),
    });

    let mut trouble: Vec<String> = Vec::new();

    // The hello named a workspace or it did not. A machine that booted without
    // one answers `where` perfectly well and is not a machine this benchmark
    // can compare anything against.
    if greeting.workspace.trim().is_empty() {
        trouble.push("the machine came up without a workspace".into());
    }
    if offered == 0 {
        trouble.push("the machine offers no verb this adapter has a tool for".into());
    }

    match machine.ask("where", vec![]) {
        Ok(answer) => report["where"] = answer,
        Err(why) => trouble.push(format!("`where` did not answer: {why}")),
    }

    match machine.ask("list", vec![".".into()]) {
        Ok(answer) => {
            let names: Vec<Value> = answer
                .get("entries")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| entry.get("name").cloned())
                        .collect()
                })
                .unwrap_or_default();
            if names.is_empty() {
                // An empty root is not a workspace with the project in it, and
                // it is exactly what a store that was never staged looks like.
                trouble.push("the workspace root is empty".into());
            }
            report["top_level"] = Value::Array(names);
            report["list"] = answer;
        }
        Err(why) => trouble.push(format!("`list .` did not answer: {why}")),
    }

    let ready = trouble.is_empty();
    report["ready"] = json!(ready);
    // Always present, never only on failure: a caller that reads `because` to
    // find out what went wrong should not have to tell an absent key from an
    // empty one. Rule 10.
    report["because"] = json!(trouble);
    (report, ready)
}

/// Answer one JSON-RPC message, or `None` when it was a notification.
fn handle(
    message: &Value,
    machine: &mut Machine,
    offered: &[&'static tools::Tool],
    metrics: &mut Metrics,
) -> Option<String> {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").cloned();

    // No id is a notification. `notifications/initialized` is the one that
    // always arrives, and it wants silence.
    id.as_ref()?;
    let id = id.expect("just checked");

    let outcome: Result<Value, (i64, String)> = match method {
        "initialize" => {
            let asked = message
                .get("params")
                .and_then(|params| params.get("protocolVersion"))
                .and_then(Value::as_str)
                .unwrap_or(MCP_PROTOCOL);
            Ok(json!({
                "protocolVersion": asked,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {
                    "name": "thalyx",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "instructions": instructions(machine, offered),
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": offered
                .iter()
                .map(|tool| json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": (tool.schema)(),
                }))
                .collect::<Vec<_>>()
        })),
        "tools/call" => call(message, machine, offered, metrics),
        // Answered rather than ignored: a client waiting on an id it will never
        // see is a client that hangs, and a hung experiment looks like a slow
        // one.
        other => Err((-32601, format!("`{other}` is not a method this serves"))),
    };

    Some(match outcome {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string(),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": code, "message": message}
        })
        .to_string(),
    })
}

/// What the agent is told about this machine before it does anything.
///
/// Short on purpose. The tool descriptions carry the "when to use this", and a
/// wall of prose here would be paid for on every single turn.
///
/// ## Why it names every tool
///
/// Because two of the three real runs in
/// `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md` spent **two** calls
/// finding this surface, and the first of the two was a failed selection: the
/// agent asked for tools by name, named them wrong, and had to search again. A
/// client that defers tool schemas makes one lookup unavoidable; what is not
/// unavoidable is guessing at the names, and this is the only text the model has
/// read by then.
///
/// So the list is generated from what this machine actually offers, never
/// written out — a hand-kept copy would go stale exactly when a verb is missing
/// from the hello, which is the one moment a wrong name costs the most.
fn instructions(machine: &Machine, offered: &[&'static tools::Tool]) -> String {
    let greeting = machine.greeting();
    let names: Vec<&str> = offered.iter().map(|tool| tool.name).collect();
    format!(
        "You are working inside a Thalyx machine, not on this host. The workspace is {} \
         and nothing outside it can be reached. This machine's tools are exactly these, \
         and there are no others: {}. Load them in one lookup — every one begins with \
         `thalyx_`. Thalyx answers with structured objects that carry an exact remedy \
         when they refuse — read the `remedy` field rather than guessing. Prefer \
         thalyx_symbol and thalyx_dependencies over reading or searching files, and open \
         the reversible boundary in the same call as your first change by passing \
         `attempt: begin` to thalyx_edit or thalyx_file.",
        greeting.workspace,
        names.join(", ")
    )
}

fn call(
    message: &Value,
    machine: &mut Machine,
    offered: &[&'static tools::Tool],
    metrics: &mut Metrics,
) -> Result<Value, (i64, String)> {
    let params = message.get("params").cloned().unwrap_or(json!({}));
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "a call needs a tool name".to_string()))?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let Some(tool) = offered.iter().find(|tool| tool.name == name) else {
        return Err((-32602, format!("`{name}` is not a tool of this machine")));
    };

    let requests = match (tool.calls)(&arguments) {
        Ok(requests) => requests,
        // A tool error and not a protocol error: the specification says a tool
        // that could not do its job reports that *inside* the result, so the
        // model sees it and can correct itself instead of the client raising it
        // as a transport fault.
        Err(why) => return Ok(tool_error(tool.name, &arguments, &why, metrics)),
    };

    let mut answers = Vec::new();
    for (verb, given) in requests {
        // Timed around the whole question, which is the only place on this side
        // of the wire where the bridge's cost is separable from the model's
        // thinking. `metrics::Metrics::machine_seconds` says what it can and
        // cannot see.
        let began = std::time::Instant::now();
        let answered = machine.ask(verb, given);
        metrics.asked(began.elapsed());
        match answered {
            Ok(answer) => answers.push(answer),
            Err(Trouble::Refused {
                word,
                remedy,
                message,
            }) => {
                // The machine's own words, passed through. An adapter that
                // reworded a refusal would be throwing away punto A2 — the
                // remedy is the part that lets a model fix itself in one turn
                // instead of three.
                let refusal =
                    json!({"ok": false, "error": word, "remedy": remedy, "message": message});
                let text = refusal.to_string();
                metrics.call(tool.name, &arguments, text.len(), true, true);
                return Ok(json!({
                    "content": [{"type": "text", "text": text}],
                    "isError": true
                }));
            }
            Err(trouble) => {
                let why = trouble.to_string();
                return Ok(tool_error(tool.name, &arguments, &why, metrics));
            }
        }
    }

    // One answer stays one object; several stay several. Merging them would be
    // this adapter composing a view, which is the thing it is not for.
    let answer = if answers.len() == 1 {
        answers.remove(0)
    } else {
        Value::Array(answers)
    };
    let text = answer.to_string();
    metrics.call(tool.name, &arguments, text.len(), false, false);
    Ok(json!({"content": [{"type": "text", "text": text}]}))
}

fn tool_error(name: &str, arguments: &Value, why: &str, metrics: &mut Metrics) -> Value {
    metrics.call(name, arguments, why.len(), true, false);
    json!({
        "content": [{"type": "text", "text": why}],
        "isError": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_whose_verb_the_machine_does_not_have_is_not_offered() {
        // The version skew this exists to make visible. A machine that lost
        // `symbol` must not be handed a model that has been told to prefer it —
        // the model would call it, get a refusal, and the run would read as an
        // agent that cannot use the index.
        let without: Vec<String> = ["where", "state", "attempt", "read", "list"]
            .iter()
            .map(|verb| verb.to_string())
            .collect();
        let offered = usable(&without);
        assert!(offered.iter().any(|tool| tool.name == "thalyx_read"));
        assert!(!offered.iter().any(|tool| tool.name == "thalyx_symbol"));
    }

    #[test]
    fn a_machine_with_every_verb_offers_every_tool() {
        // The control. Without it a filter that dropped everything would look
        // exactly like one that works.
        let every: Vec<String> = tools::TOOLS
            .iter()
            .flat_map(|tool| tool.verbs.iter().map(|verb| verb.to_string()))
            .collect();
        assert_eq!(usable(&every).len(), tools::TOOLS.len());
    }
}
