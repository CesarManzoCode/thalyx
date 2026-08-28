//! The tools a programming agent is offered, and the Thalyx verbs behind them.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md`: **MCP is an adapter, not an
//! interface.** Nothing here reads a file, walks a tree, resolves a symbol or
//! undoes anything. Every entry below turns a call into a verb and its
//! arguments, and the machine does the rest — which is why this file is a table
//! and not a program.
//!
//! ## Why there are ten of these and not forty
//!
//! Because the list is a prompt. Every tool an agent is shown is a branch it has
//! to consider on every turn, and a surface that offers one tool per verb spends
//! the model's attention on choosing rather than on the work. So the question
//! asked of each entry was not *does this exist* but **can this make an agent
//! program better here than it would on Linux**, and a verb that failed it is
//! reachable and not advertised.
//!
//! ## The descriptions are the product
//!
//! An agent that has never seen Thalyx decides what to call from these
//! sentences and nothing else. So each says *when* to reach for the primitive
//! rather than what it returns — "prefer this over reading files" is the whole
//! value of the tool, and a description that only named its output would leave
//! the model doing what it already knows how to do.

use serde_json::{Value, json};

/// One tool, and the verb it becomes.
pub struct Tool {
    pub name: &'static str,
    /// The verb ids this tool needs the machine to have. Checked against what
    /// the machine said in its hello, so a tool whose verb is gone is never
    /// advertised — see `main::usable`.
    pub verbs: &'static [&'static str],
    pub description: &'static str,
    /// The JSON schema the agent's arguments are validated against by the
    /// client. Written out rather than derived, for the reason
    /// `thalyx_files::machine` gives for the same choice: a shape a caller
    /// parses is a decision, and a derived one is renamed by a refactor.
    pub schema: fn() -> Value,
    /// Turn the call's arguments into requests for the machine, in order.
    ///
    /// A `Vec` because two of these ask more than one question — `thalyx_state`
    /// is three — and that composition is exactly the adapting this crate is
    /// for. It is never *logic*: no branch here decides what an answer means.
    pub calls: fn(&Value) -> Result<Vec<Request>, String>,
}

/// One thing to ask the machine: a verb id and its arguments, in order.
///
/// A named alias rather than the tuple written out, so that the signature above
/// reads as what it is. It is deliberately the same shape as
/// `thalyx_bridge::ToThalyx::Request` minus the id, which this crate has no
/// opinion about — the id belongs to the channel, not to the tool.
pub type Request = (&'static str, Vec<String>);

/// Read one string out of the arguments, or say which one was missing.
fn text(arguments: &Value, name: &str) -> Result<String, String> {
    match arguments.get(name) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(other) => Err(format!("`{name}` must be a string, and is {other}")),
        None => Err(format!("`{name}` is required")),
    }
}

fn optional(arguments: &Value, name: &str) -> Option<String> {
    match arguments.get(name) {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    }
}

/// A window flag, when the caller asked for one.
fn limit(arguments: &Value) -> Option<String> {
    arguments
        .get("limit")
        .and_then(Value::as_u64)
        .map(|limit| format!("limite={limit}"))
}

pub const TOOLS: &[Tool] = &[
    Tool {
        name: "thalyx_state",
        verbs: &["where", "state", "attempt"],
        description: "\
What this Thalyx machine is right now: where the workspace is, what the machine \
can and cannot enforce, and whether a reversible attempt is currently open. \
Call this once at the start of a task — it answers in one call what would \
otherwise take several commands and several guesses.",
        schema: || json!({"type": "object", "properties": {}}),
        calls: |_| {
            Ok(vec![
                ("where", vec![]),
                ("state", vec![]),
                ("attempt", vec![]),
            ])
        },
    },
    Tool {
        name: "thalyx_list",
        verbs: &["list"],
        description: "\
What is in a directory of the workspace. Sizes are exact and nothing is hidden. \
Use it to orient; use thalyx_symbol or thalyx_dependencies to find code.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative to the workspace root. Defaults to the root."
                    },
                    "limit": {"type": "integer", "description": "Most entries to return."}
                }
            })
        },
        calls: |arguments| {
            let mut given = vec![optional(arguments, "path").unwrap_or_else(|| ".".into())];
            given.extend(limit(arguments));
            Ok(vec![("list", given)])
        },
    },
    Tool {
        name: "thalyx_read",
        verbs: &["read"],
        description: "\
The text of one file, with its exact size and its sha256. A file that is not \
text is refused rather than printed. Before reading many files to find out what \
a change would touch, ask thalyx_dependencies instead — it answers that without \
opening any of them.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Relative to the workspace root."}
                },
                "required": ["path"]
            })
        },
        calls: |arguments| Ok(vec![("read", vec![text(arguments, "path")?])]),
    },
    Tool {
        name: "thalyx_symbol",
        verbs: &["symbol", "index_build"],
        description: "\
Where a name is defined and every place it is used, from Thalyx's parsed \
semantic index — exact, and never a match inside a comment or a string. \
Prefer this over text search whenever the question is about a code symbol. \
The answer says whether the index is current; if it is stale, call \
thalyx_index first.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "The exact symbol name."},
                    "limit": {"type": "integer"}
                },
                "required": ["name"]
            })
        },
        calls: |arguments| {
            let mut given = vec![text(arguments, "name")?];
            given.extend(limit(arguments));
            Ok(vec![("symbol", given)])
        },
    },
    Tool {
        name: "thalyx_dependencies",
        verbs: &["depends_on", "depended_on_by"],
        description: "\
Structural dependencies of one file, from the index and without reading \
anything: what it refers to, or what refers to it. Use it before reading files \
to discover the impact of a change — `dependents` is the direction no directory \
walk and no grep can answer.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Relative to the workspace root."},
                    "direction": {
                        "type": "string",
                        "enum": ["depends_on", "dependents"],
                        "description": "What this file refers to, or what refers to it."
                    },
                    "limit": {"type": "integer"}
                },
                "required": ["path", "direction"]
            })
        },
        calls: |arguments| {
            let path = text(arguments, "path")?;
            let verb = match text(arguments, "direction")?.as_str() {
                "depends_on" => "depends_on",
                "dependents" => "depended_on_by",
                other => {
                    return Err(format!(
                        "`direction` is `depends_on` or `dependents`, and was `{other}`"
                    ));
                }
            };
            let mut given = vec![path];
            given.extend(limit(arguments));
            Ok(vec![(verb, given)])
        },
    },
    Tool {
        name: "thalyx_index",
        verbs: &["index_build"],
        description: "\
Read the workspace and record what refers to what. Needed once before \
thalyx_symbol and thalyx_dependencies can answer, and again after a change \
those answers should reflect. Hidden directories and build outputs are skipped.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Defaults to the workspace root."}
                }
            })
        },
        calls: |arguments| {
            Ok(vec![(
                "index_build",
                vec![optional(arguments, "path").unwrap_or_else(|| ".".into())],
            )])
        },
    },
    Tool {
        name: "thalyx_find",
        verbs: &["find", "grep"],
        description: "\
Search the workspace by file name pattern, or for a literal string inside \
files. This is the fallback: for anything that is a code symbol, thalyx_symbol \
is exact where this is not.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["name", "text"],
                        "description": "Match file names (`*.rs`), or lines inside files."
                    },
                    "query": {"type": "string"},
                    "in": {
                        "type": "string",
                        "description": "A folder to search under. Defaults to the workspace root."
                    },
                    "limit": {"type": "integer"}
                },
                "required": ["mode", "query"]
            })
        },
        calls: |arguments| {
            let query = text(arguments, "query")?;
            let verb = match text(arguments, "mode")?.as_str() {
                "name" => "find",
                "text" => "grep",
                other => return Err(format!("`mode` is `name` or `text`, and was `{other}`")),
            };
            // The options come first because `grep` takes the rest of the line
            // as its text — `search::parse` reads flags until it meets one that
            // is not a flag, and everything from there is the subject.
            let mut given: Vec<String> = Vec::new();
            given.extend(limit(arguments));
            if let Some(folder) = optional(arguments, "in") {
                given.push(format!("en={folder}"));
            }
            if verb == "find" {
                given.insert(0, query);
            } else {
                given.push(query);
            }
            Ok(vec![(verb, given)])
        },
    },
    Tool {
        name: "thalyx_attempt",
        verbs: &["attempt"],
        description: "\
A reversible boundary around a task. `begin` takes a snapshot of the whole \
workspace; `abandon` puts every file back exactly as it was, in one step and \
whatever was changed in between; `commit` keeps the work and closes it. \
Begin one before any multi-file or risky change, so that being wrong costs a \
call instead of a reconstruction. `abandon` answers with what it would destroy \
and does nothing the first time — repeat it with confirm true to go ahead.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["begin", "commit", "abandon", "status"]
                    },
                    "label": {
                        "type": "string",
                        "description": "What this attempt is about. Used with `begin`."
                    },
                    "confirm": {
                        "type": "boolean",
                        "description": "Required to actually abandon, having seen the cost."
                    }
                },
                "required": ["action"]
            })
        },
        calls: |arguments| {
            let given = match text(arguments, "action")?.as_str() {
                "status" => vec![],
                "begin" => vec![
                    "empezar".to_string(),
                    optional(arguments, "label").unwrap_or_else(|| "agent".into()),
                ],
                "commit" => vec!["confirmar".to_string()],
                "abandon" => {
                    let mut given = vec!["abandonar".to_string()];
                    if arguments.get("confirm").and_then(Value::as_bool) == Some(true) {
                        given.push("si".to_string());
                    }
                    given
                }
                other => {
                    return Err(format!(
                        "`action` is one of begin, commit, abandon, status, and was `{other}`"
                    ));
                }
            };
            Ok(vec![("attempt", given)])
        },
    },
    Tool {
        name: "thalyx_changed",
        verbs: &["attempt"],
        description: "\
What has changed in the workspace since the open attempt began — files made, \
files edited, files deleted — without rescanning anything. Ask this instead of \
re-reading files to find out what you have done. It needs an open attempt: \
with none, it answers that none is open.",
        schema: || json!({"type": "object", "properties": {}}),
        calls: |_| Ok(vec![("attempt", vec![])]),
    },
    Tool {
        name: "thalyx_edit",
        verbs: &["edit"],
        description: "\
Change a file by line. `insert` puts text before a line, `replace` swaps a line \
or a range of lines, `delete` removes them, `show` returns numbered lines. \
Line numbers are 1-based and a range is `3-7`. Use \\n in the text for more \
than one line. Every answer carries the exact call that undoes it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Relative to the workspace root."},
                    "action": {
                        "type": "string",
                        "enum": ["show", "insert", "replace", "delete"]
                    },
                    "at": {
                        "type": "string",
                        "description": "A line, `12`, or a range, `12-18`. 1-based."
                    },
                    "text": {
                        "type": "string",
                        "description": "What goes in. \\n for a line break, \\t for a tab."
                    }
                },
                "required": ["path", "action"]
            })
        },
        calls: |arguments| {
            let mut given = vec![text(arguments, "path")?, text(arguments, "action")?];
            if let Some(at) = optional(arguments, "at") {
                given.push(at);
            }
            if let Some(body) = arguments.get("text").and_then(Value::as_str) {
                // Newlines and tabs go on the wire as the escapes `editar`
                // reads, because a session line is a line: there is no newline
                // left by the time the verb sees its argument. `edit::unescape`
                // is the other half.
                given.push(
                    body.replace('\\', r"\\")
                        .replace('\n', r"\n")
                        .replace('\t', r"\t"),
                );
            }
            Ok(vec![("edit", given)])
        },
    },
    Tool {
        name: "thalyx_file",
        verbs: &["make_file", "make_directory", "remove", "move", "copy"],
        description: "\
Create, delete, move or copy a file or directory in the workspace. Every answer \
says exactly what happened to each path and, where there is one, the call that \
undoes it. For changing what is *inside* a file, use thalyx_edit.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["create", "create_directory", "delete", "move", "copy"]
                    },
                    "path": {"type": "string", "description": "Relative to the workspace root."},
                    "to": {"type": "string", "description": "The destination, for move and copy."}
                },
                "required": ["action", "path"]
            })
        },
        calls: |arguments| {
            let path = text(arguments, "path")?;
            Ok(vec![match text(arguments, "action")?.as_str() {
                "create" => ("make_file", vec![path]),
                "create_directory" => ("make_directory", vec![path]),
                "delete" => ("remove", vec![path]),
                "move" => ("move", vec![path, text(arguments, "to")?]),
                "copy" => ("copy", vec![path, text(arguments, "to")?]),
                other => {
                    return Err(format!(
                        "`action` is one of create, create_directory, delete, move, copy, \
                         and was `{other}`"
                    ));
                }
            }])
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_two_tools_share_a_name() {
        let mut seen = std::collections::BTreeSet::new();
        for tool in TOOLS {
            assert!(seen.insert(tool.name), "`{}` is defined twice", tool.name);
        }
    }

    #[test]
    fn every_tool_says_when_to_use_it_and_not_only_what_it_returns() {
        // The descriptions are the product: an agent that has never seen Thalyx
        // chooses from these sentences alone. A one-line label would leave the
        // model doing what it already knows how to do on Linux, which is the one
        // outcome this whole delivery exists to measure against.
        for tool in TOOLS {
            assert!(
                tool.description.len() > 80,
                "`{}` has a label rather than a description",
                tool.name
            );
            assert!(
                (tool.schema)().get("type") == Some(&json!("object")),
                "`{}` does not take an object",
                tool.name
            );
        }
    }

    #[test]
    fn the_tool_surface_is_small_enough_to_choose_from() {
        // Pinned as a claim, not a guess. Every tool added is a branch the model
        // takes on every turn, and the decree is that a verb which cannot make
        // an agent program better is reachable and not advertised.
        assert!(
            TOOLS.len() <= 12,
            "{} tools is a menu rather than a surface",
            TOOLS.len()
        );
    }

    #[test]
    fn a_symbol_call_becomes_the_symbol_verb_with_its_name() {
        let calls = (TOOLS
            .iter()
            .find(|t| t.name == "thalyx_symbol")
            .unwrap()
            .calls)(&json!({"name": "Store", "limit": 5}))
        .expect("a name is enough");
        assert_eq!(
            calls,
            vec![("symbol", vec!["Store".to_string(), "limite=5".to_string()])]
        );
    }

    #[test]
    fn a_text_search_puts_its_options_before_the_text() {
        // `contenido` takes the rest of the line as the thing to look for, so an
        // option after it is not an option, it is part of the search. Found by
        // reading `search::parse`, and pinned here because the ordering is
        // invisible in the answer: a search for `hello limite=5` returns nothing
        // and looks exactly like a search that found nothing.
        let calls = (TOOLS
            .iter()
            .find(|t| t.name == "thalyx_find")
            .unwrap()
            .calls)(
            &json!({"mode": "text", "query": "fn main", "in": "src", "limit": 3})
        )
        .expect("a query is enough");
        assert_eq!(
            calls,
            vec![(
                "grep",
                vec![
                    "limite=3".to_string(),
                    "en=src".to_string(),
                    "fn main".to_string()
                ]
            )]
        );
    }

    #[test]
    fn an_edit_sends_line_breaks_as_the_escapes_the_verb_reads() {
        let calls = (TOOLS
            .iter()
            .find(|t| t.name == "thalyx_edit")
            .unwrap()
            .calls)(&json!({
            "path": "src/a.rs", "action": "replace", "at": "3", "text": "one\ntwo"
        }))
        .expect("an edit");
        assert_eq!(calls[0].1[3], r"one\ntwo");
    }

    #[test]
    fn abandoning_without_confirming_does_not_send_the_confirmation() {
        // The confirmation is the trusted path's own, and forging it here would
        // be this adapter deciding something the machine reserved for whoever
        // saw the cost.
        let attempt = TOOLS.iter().find(|t| t.name == "thalyx_attempt").unwrap();
        assert_eq!(
            (attempt.calls)(&json!({"action": "abandon"})).unwrap(),
            vec![("attempt", vec!["abandonar".to_string()])]
        );
        assert_eq!(
            (attempt.calls)(&json!({"action": "abandon", "confirm": true})).unwrap(),
            vec![("attempt", vec!["abandonar".to_string(), "si".to_string()])]
        );
    }

    #[test]
    fn an_argument_of_the_wrong_shape_is_refused_here_and_never_sent() {
        let read = TOOLS.iter().find(|t| t.name == "thalyx_read").unwrap();
        assert!((read.calls)(&json!({})).is_err());
        assert!((read.calls)(&json!({"path": 3})).is_err());
    }
}
