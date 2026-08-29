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
                "instructions": instructions(machine),
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
fn instructions(machine: &Machine) -> String {
    let greeting = machine.greeting();
    format!(
        "You are working inside a Thalyx machine, not on this host. The workspace is {} \
         and nothing outside it can be reached. Thalyx answers with structured objects \
         that carry an exact remedy when they refuse — read the `remedy` field rather \
         than guessing. Prefer thalyx_symbol and thalyx_dependencies over reading or \
         searching files, and open a thalyx_attempt before any multi-file change so it \
         can be undone in one call.",
        greeting.workspace
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
        match machine.ask(verb, given) {
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
