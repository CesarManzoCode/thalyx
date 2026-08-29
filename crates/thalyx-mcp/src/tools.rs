//! The tools a programming agent is offered, and the Thalyx verbs behind them.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md`: **MCP is an adapter, not an
//! interface.** Nothing here reads a file, walks a tree, resolves a symbol or
//! undoes anything. Every entry below turns a call into a verb and its
//! arguments, and the machine does the rest — which is why this file is a table
//! and not a program.
//!
//! ## Why there are eleven of these and not forty
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

/// The files a substitution names, from `paths` or from the single `path`.
///
/// Both spellings are read because both are natural to write and refusing one
/// of them would cost a whole turn to learn. What is not accepted is neither:
/// an empty list is a call that names no file, and answering it with a
/// substitution across nothing would be this adapter inventing a meaning the
/// machine does not have.
fn paths(arguments: &Value) -> Result<Vec<String>, String> {
    if let Some(Value::Array(listed)) = arguments.get("paths") {
        let named: Vec<String> = listed
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        if named.len() != listed.len() {
            return Err("every entry of `paths` must be a string".to_string());
        }
        if !named.is_empty() {
            return Ok(named);
        }
        return Err("`paths` is empty; name at least one file to change".to_string());
    }
    Ok(vec![text(arguments, "path")?])
}

/// One substitution of a batch, as the arguments spelled it.
pub struct Operation {
    pub old: String,
    pub new: String,
    pub paths: Vec<String>,
}

/// The operations of a `substitute_batch`, or which one of them is wrong.
///
/// Every refusal here happens before anything is sent to the machine, for the
/// reason [`paths`] gives for the same choice: a shape this adapter can see is
/// wrong costs the caller one corrected call, and a shape it passes on costs a
/// round trip into the machine first. What it does **not** do is decide what a
/// batch means — that is the verb's, and every rule about composition, order and
/// ceilings lives there.
fn batch(arguments: &Value) -> Result<Vec<Operation>, String> {
    let Some(Value::Array(listed)) = arguments.get("operations") else {
        return Err("`substitute_batch` needs `operations`, a list of \
                    {old, new, paths}"
            .to_string());
    };
    if listed.is_empty() {
        return Err("`operations` is empty; name at least one substitution".to_string());
    }
    let mut operations = Vec::with_capacity(listed.len());
    for (index, one) in listed.iter().enumerate() {
        let at = index + 1;
        let old = text(one, "old").map_err(|why| format!("operation {at}: {why}"))?;
        let new = text(one, "new").map_err(|why| format!("operation {at}: {why}"))?;
        let named = paths(one).map_err(|why| format!("operation {at}: {why}"))?;
        operations.push(Operation {
            old,
            new,
            paths: named,
        });
    }
    Ok(operations)
}

/// A mutation, with the attempt the caller asked to be opened around it.
///
/// **Two requests and one round trip**, which is the composition this crate
/// exists to do — `thalyx_state` is three of them. It comes from the traces in
/// `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md`: in all three real runs
/// `attempt begin` is followed *immediately* by the mutation, with no call in
/// between, so the two were one intent that the surface charged twice for.
///
/// The order is the whole of it. The snapshot is taken before anything is
/// written, and a refused request stops the rest — so a caller that asked for
/// both and got `already_open` back has changed nothing at all. Nothing here
/// reads an answer or decides what one means; the machine still owns every rule
/// about whether an attempt may be opened.
fn inside_a_new_attempt(arguments: &Value, mutation: Request) -> Result<Vec<Request>, String> {
    let Some(asked) = optional(arguments, "attempt") else {
        return Ok(vec![mutation]);
    };
    if asked != "begin" {
        return Err(format!(
            "`attempt` here is `begin`, which opens one around this change; it was \
             `{asked}`. thalyx_attempt is what settles an attempt"
        ));
    }
    Ok(vec![
        (
            "attempt",
            vec![
                "empezar".to_string(),
                optional(arguments, "label").unwrap_or_else(|| "agent".into()),
            ],
        ),
        mutation,
    ])
}

/// The same schema, with the two properties that open a boundary around it.
///
/// Folded in here rather than written into each schema, because a surface where
/// one mutation can do this and another cannot is a surface an agent has to
/// remember exceptions about — and because two copies of a description are two
/// things to keep in step.
fn with_attempt(mut schema: Value) -> Value {
    let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
        // Never reached: both callers pass an object schema, and the test
        // `every_tool_says_when_to_use_it` asserts that of every tool. Handed
        // back unchanged rather than panicking, because a tool list that cannot
        // be built is a machine with no tools at all.
        return schema;
    };
    properties.insert(
        "attempt".to_string(),
        json!({
            "type": "string",
            "enum": ["begin"],
            "description": "Open a reversible attempt around this change, in this same \
                            call. Use it on the first change of a task; thalyx_attempt \
                            is what settles it afterwards."
        }),
    );
    properties.insert(
        "label".to_string(),
        json!({
            "type": "string",
            "description": "What that attempt is about. Used with `attempt`."
        }),
    );
    schema
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
        name: "thalyx_context",
        verbs: &["context"],
        description: "\
What a name IS, resolved by a compiler frontend rather than matched as text: \
its kind, its crate, its signature, where it is declared, how many places use \
it, and a handle. Ask this INSTEAD of reading files. One answer is a few \
hundred bytes where the file it describes is tens of thousands, and it is \
exact — an alias, a re-export or a trait method resolves to the thing it \
really names, which no text search can do. \
Give it a symbol (`Store::lock`, `Keystore`), or a path ending in `.rs` for a \
map of everything one file declares. \
`budget` bounds the answer in bytes and the answer says how many entries did \
not fit; nothing is lost, it is held. \
When you actually need the source, call this again with `expand` set to an \
entry's handle and you get exactly the lines that declaration occupies — not \
the file. \
Every answer says `source` and `fresh`: `rust-analyzer` means the name was \
resolved, `index` means it was matched, and `stale` means the tree moved since \
the machine last looked. Believe them.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "A symbol name, `Type::method`, or a path to a \
                                        `.rs` file for a map of it."
                    },
                    "budget": {
                        "type": "integer",
                        "description": "Most bytes of entries to return. Defaults to 2000."
                    },
                    "expand": {
                        "type": "string",
                        "description": "A handle from a previous answer. Returns the exact \
                                        source lines of that declaration."
                    }
                }
            })
        },
        calls: |arguments| {
            let mut given: Vec<String> = Vec::new();
            if let Some(handle) = optional(arguments, "expand") {
                given.push(format!("expandir={handle}"));
            } else {
                given.push(text(arguments, "query")?);
            }
            if let Some(budget) = arguments.get("budget").and_then(Value::as_u64) {
                given.push(format!("presupuesto={budget}"));
            }
            Ok(vec![("context", given)])
        },
    },
    Tool {
        name: "thalyx_symbol",
        verbs: &["symbol", "index_build"],
        description: "\
Where a name is defined and every place it is used, from Thalyx's parsed \
semantic index — exact, and never a match inside a comment or a string. \
For Rust, prefer thalyx_context: it resolves names with a compiler frontend \
where this one matches them, so it follows aliases and re-exports and this \
does not. Use this for every other language, and when you want the whole list \
of uses rather than a count. \
Prefer either over text search whenever the question is about a code symbol. \
If the workspace changed since the index was built, this rebuilds it and then \
answers about the tree as it is now; the answer says which happened. Matching \
is exact and case-sensitive, so `login` does not find `login_user`.",
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
Which files depend on one file, or which it depends on, from the index and \
without reading anything. Use it before reading files to work out the impact \
of a change: `dependents` is the direction no directory walk and no grep can \
answer. Every row says how the dependency is known — `via: import` when the \
file declares it (use, mod, import, #include), `via: symbol` when the file \
uses a name that exactly one file in the workspace declares, is visible outside \
that file, and is not something this file binds or declares itself. That is how \
a field access, a method call, a trait bound or a re-export gets caught. It is \
built to be precise rather than complete: a name declared in two files is never \
turned into a dependency, because which one it meant is a guess, and an alias \
(`use X as Y`) is followed to the file and not to the name. A method called on \
a type from outside the workspace can still slip through. So treat the list as \
solid but not exhaustive — for a name you suspect is reached some other way, \
thalyx_symbol is the finer question.",
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
Read the workspace and record what refers to what. thalyx_symbol and \
thalyx_dependencies keep the index current by themselves, so you rarely need \
this: call it for the first index of a workspace, or when one of them answered \
`refreshed: declined_too_large` because the tree is big enough that rebuilding \
inside a question would keep you waiting. Hidden directories and build outputs \
are skipped.",
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
call instead of a reconstruction — thalyx_edit and thalyx_file can open one \
around their first change with `attempt: begin`, which saves a call. \
`abandon` in one call: pass back the `snapshot` that `begin` answered with, \
together with `state` — the workspace state id from the most recent answer that \
carried one. It goes ahead only if the workspace is still exactly that state, so \
work somebody else did while you were busy cannot be thrown away by a claim that \
is out of date. Get it wrong, or leave it out, and it destroys nothing and \
answers with the true cost and the exact line that would do it. \
For a whole task at once — change, check, and keep-or-undo without coming back \
here between the steps — use thalyx_exec instead.",
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
                    },
                    "snapshot": {
                        "type": "string",
                        "description": "For abandon in one call: the `snapshot` this \
                                        tool answered with when it began."
                    },
                    "state": {
                        "type": "string",
                        "description": "For abandon in one call: the `state` id of the \
                                        workspace you are authorising the undo of, copied \
                                        from the most recent answer that carried one. Any \
                                        write by anybody since then makes it stale and the \
                                        undo is refused rather than done."
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
                    match (
                        optional(arguments, "snapshot"),
                        optional(arguments, "state"),
                    ) {
                        // Both or neither. A half-stated claim is a call the
                        // machine answers with the cost object, which is what a
                        // caller that stated nothing gets anyway — so there is
                        // nothing for this to decide.
                        (Some(named), Some(state)) => {
                            given.push(format!("snapshot={named}"));
                            given.push(format!("state={state}"));
                        }
                        // And `si` is **not** added beside them. A caller that
                        // said both would otherwise have its stale claim waved
                        // through by the blind word, which is the one thing the
                        // claim exists to stop.
                        _ => {
                            if arguments.get("confirm").and_then(Value::as_bool) == Some(true) {
                                given.push("si".to_string());
                            }
                        }
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
        name: "thalyx_exec",
        verbs: &["exec"],
        description: "\
Do a whole deterministic stretch of work in ONE call. Give Thalyx the list of \
operations you already know you want, plus what must be true afterwards, and it \
opens a reversible boundary, runs every operation in order, observes what really \
changed, runs the checks, and then keeps the work or puts the workspace back \
exactly as it was — without coming back to you in between. \
Reach for this whenever the next several steps do not need you to think between \
them: a rename across files then a search proving the old name is gone; an edit \
then a compile; make files, change them, verify, and undo it all if the \
verification fails. That is the normal case, not the exotic one. \
`steps` are ordinary Thalyx requests — the same verbs and arguments the other \
tools send, so anything you could do in five calls you can do here in one. Two \
are worth knowing by name: `rename` takes a Rust symbol and a new name and \
rewrites every place that really refers to it, resolved by a compiler frontend, \
including the aliased imports a search would miss; and the `rust` check \
compiles exactly the crates your change reaches — the ones it is in and the \
ones that depend on them, worked out from Cargo's graph — and reuses the answer \
when this machine has already compiled these exact bytes. They \
run in order and stop at the first refusal. `validate` decides the outcome: if \
every check passes the work is committed, and otherwise the whole thing is \
rolled back and the workspace is byte-for-byte what it was. A check that could \
not be run counts as failure, never as success. \
The answer is deliberately small — status, what changed, how each check went, \
and an `evidence` id. Every answer, every search hit and every line of compiler \
output stays inside the machine; thalyx_evidence fetches any of it if you \
actually need it. \
Do not use it for exploring: if you need to read an answer before choosing the \
next step, that is a real decision and belongs in its own call.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "label": {
                        "type": "string",
                        "description": "What this piece of work is about, for the journal."
                    },
                    "steps": {
                        "type": "array",
                        "description": "The operations, in order. Each is a Thalyx verb and \
                                        its arguments — exactly what the single-purpose \
                                        tools send. They stop at the first refusal.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "verb": {
                                    "type": "string",
                                    "description": "One of: edit, make_file, make_directory, \
                                                    copy, move, remove, read, list, grep, \
                                                    find, symbol, depends_on, \
                                                    depended_on_by, index_build, where, \
                                                    state, describe, rehearse."
                                },
                                "arguments": {
                                    "type": "array",
                                    "items": {"type": "string"},
                                    "description": "The verb's arguments, in order. For \
                                                    `edit`: [path, action, …] where action \
                                                    is sustituir | sustituir-lote | poner | \
                                                    cambiar | borrar."
                                }
                            },
                            "required": ["verb"]
                        }
                    },
                    "validate": {
                        "type": "array",
                        "description": "What must be true for the work to be kept. An empty \
                                        list commits whatever the steps did.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "check": {
                                    "type": "string",
                                    "enum": ["text", "parses", "rust", "program"],
                                    "description": "`text`: a string must be absent from (or \
                                                    present in) the workspace — the way to \
                                                    prove a rename left nothing behind. \
                                                    `parses`: every changed source file \
                                                    still has balanced brackets, strings and \
                                                    comments, which is what a mechanical \
                                                    edit breaks. `rust`: cargo over the \
                                                    packages the changed files belong to. \
                                                    `program`: run an absolute path, \
                                                    confined, and require exit 0."
                                },
                                "text": {"type": "string", "description": "For `text`."},
                                "expect": {
                                    "type": "string",
                                    "enum": ["none", "some"],
                                    "description": "For `text`: whether it must be gone \
                                                    (default) or must still be there."
                                },
                                "in": {
                                    "type": "string",
                                    "description": "For `text`: a folder to look in. \
                                                    Defaults to the whole workspace."
                                },
                                "mode": {
                                    "type": "string",
                                    "enum": ["check", "test"],
                                    "description": "For `rust`: `cargo check` (default) or \
                                                    `cargo test`."
                                },
                                "program": {
                                    "type": "string",
                                    "description": "For `program`: an absolute path."
                                },
                                "arguments": {
                                    "type": "array",
                                    "items": {"type": "string"},
                                    "description": "For `program`: its arguments."
                                }
                            },
                            "required": ["check"]
                        }
                    },
                    "on_failure": {
                        "type": "string",
                        "enum": ["rollback", "keep"],
                        "description": "What to do when a step is refused or a check does \
                                        not hold. `rollback` is the default and is almost \
                                        always what you want; `keep` leaves the failed tree \
                                        in place for you to look at."
                    }
                },
                "required": ["steps"]
            })
        },
        // The whole program travels as **one** argument, and that is the shape
        // rather than an encoding trick: a request is a verb and a list of
        // strings, and a program is structured. Serialised here, read by the
        // verb, and every step inside it then checked by the machine against
        // the same table a single request is checked against.
        calls: |arguments| {
            let mut program = serde_json::Map::new();
            let Some(steps) = arguments.get("steps") else {
                return Err(
                    "`thalyx_exec` needs `steps`, the operations to run in order".to_string(),
                );
            };
            if steps.as_array().is_none_or(Vec::is_empty) {
                return Err(
                    "`steps` is empty; there is nothing to do and nothing to be \
                            transactional about"
                        .to_string(),
                );
            }
            program.insert("steps".to_string(), steps.clone());
            for name in ["label", "validate", "on_failure"] {
                if let Some(value) = arguments.get(name) {
                    program.insert(name.to_string(), value.clone());
                }
            }
            Ok(vec![("exec", vec![Value::Object(program).to_string()])])
        },
    },
    Tool {
        name: "thalyx_evidence",
        verbs: &["evidence"],
        description: "\
Everything a thalyx_exec run did and did not send back: each step's full answer, \
each check's full output, every line a program printed. Ask for it only when the \
summary was not enough — that is the point of the summary. Pass the `evidence` \
id from the run; add `step` to get one step's answer whole.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "evidence": {
                        "type": "string",
                        "description": "The `evidence` id thalyx_exec answered with."
                    },
                    "step": {
                        "type": "integer",
                        "description": "1-based. Without it, the shape of the whole run."
                    }
                },
                "required": ["evidence"]
            })
        },
        calls: |arguments| {
            let mut given = vec![text(arguments, "evidence")?];
            if let Some(step) = arguments.get("step").and_then(Value::as_u64) {
                given.push(format!("paso={step}"));
            }
            Ok(vec![("evidence", given)])
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
Change a file. For anything repeated or mechanical — a rename, a changed \
constant, a moved import — `substitute` replaces an exact string everywhere it \
occurs, in one call, across every file in `paths` — the file list thalyx_symbol \
just gave you — and answers with how many places in each, so you do not read \
them back. \
`substitute_batch` does the same with several different old/new pairs at once, \
which is what a rename usually needs: the qualified name, the bare name, the \
definition. Put them all in `operations`; they apply in the order you give \
them. Both match text and not symbols, so get the name from thalyx_symbol \
first. The line actions are for surgical changes: `insert` puts text before a \
line, `replace` swaps a line or a range (`3-7`), `delete` removes them, `show` \
returns numbered lines; use \\n in the text for more than one line. Nothing is \
written unless every part of the call passes — a file the text is not in \
refuses the whole thing and changes nothing. Every answer carries its undo. \
On the first change of a task pass `attempt: begin` here as well: it opens the \
reversible boundary around this very call, so the change and the way back cost \
one call and not two.",
        schema: || {
            let schema = json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative to the workspace root. For the line actions."
                    },
                    "action": {
                        "type": "string",
                        "enum": ["substitute", "substitute_batch", "show", "insert",
                                 "replace", "delete"]
                    },
                    "operations": {
                        "type": "array",
                        "description": "For substitute_batch: several substitutions in one \
                                        call, applied in this order. Each must be found in \
                                        every file it names, or nothing is written.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old": {"type": "string",
                                        "description": "The exact text to find. One line."},
                                "new": {"type": "string",
                                        "description": "What replaces it. One line."},
                                "paths": {
                                    "type": "array",
                                    "items": {"type": "string"},
                                    "description": "Every file this one substitution changes."
                                }
                            },
                            "required": ["old", "new", "paths"]
                        }
                    },
                    "paths": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "For substitute: every file to change. Each must \
                                        contain the text, or nothing is written."
                    },
                    "old": {
                        "type": "string",
                        "description": "For substitute: the exact text to find. One line."
                    },
                    "new": {
                        "type": "string",
                        "description": "For substitute: what replaces it. One line."
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
                "required": ["action"]
            });
            with_attempt(schema)
        },
        calls: |arguments| {
            let action = text(arguments, "action")?;
            // The one action whose arguments are not a line and a body. Composed
            // here and nowhere else: this is still only a translation — the
            // machine decides what a substitution is, refuses it, and counts it.
            // Several exact substitutions in one call, composed onto the one
            // line `editar` reads. The counts are the line's own grammar — see
            // `edit::substitute_batch` for why it is counts and not a separator
            // — and composing them is exactly the adapting this crate is for:
            // no agent writes this, and no logic here decides what any of it
            // means.
            if action == "substitute_batch" {
                let operations = batch(arguments)?;
                let (first, rest) = operations
                    .split_first()
                    .expect("`batch` never returns an empty list");
                let mut given = vec![
                    first.paths[0].clone(),
                    // The CLI's own spelling of the subverb, which is this
                    // string unchanged: `edit::SUBSTITUTE_BATCH` carries it, so
                    // there is no mapping between two names to keep in step.
                    action,
                    first.paths.len().to_string(),
                    first.old.clone(),
                    first.new.clone(),
                ];
                given.extend(first.paths[1..].iter().cloned());
                for operation in rest {
                    given.push(operation.paths.len().to_string());
                    given.push(operation.old.clone());
                    given.push(operation.new.clone());
                    given.extend(operation.paths.iter().cloned());
                }
                return inside_a_new_attempt(arguments, ("edit", given));
            }
            if action == "substitute" {
                let named = paths(arguments)?;
                let (first, rest) = named.split_first().expect("`paths` is never empty here");
                let mut given = vec![
                    first.clone(),
                    action,
                    text(arguments, "old")?,
                    text(arguments, "new")?,
                ];
                given.extend(rest.iter().cloned());
                return inside_a_new_attempt(arguments, ("edit", given));
            }

            let mut given = vec![text(arguments, "path")?, action];
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
            inside_a_new_attempt(arguments, ("edit", given))
        },
    },
    Tool {
        name: "thalyx_file",
        verbs: &["make_file", "make_directory", "remove", "move", "copy"],
        description: "\
Create, delete, move or copy a file or directory in the workspace. Every answer \
says exactly what happened to each path and, where there is one, the call that \
undoes it. For changing what is *inside* a file, use thalyx_edit. On the first \
change of a task pass `attempt: begin` here as well, which opens the reversible \
boundary around this call instead of costing a call of its own.",
        schema: || {
            with_attempt(json!({
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
            }))
        },
        calls: |arguments| {
            let path = text(arguments, "path")?;
            let mutation: Request = match text(arguments, "action")?.as_str() {
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
            };
            inside_a_new_attempt(arguments, mutation)
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
        // Raised from twelve to thirteen on 2026-08-29, deliberately and once:
        // `thalyx_exec` is the tool the other twelve are steps of, and
        // `thalyx_evidence` is the half of it that keeps its answers out of the
        // context window. The reason this is not simply "one more branch to
        // consider" is that a turn spent choosing `thalyx_exec` replaces
        // several turns spent choosing the others — which is the only argument
        // that has ever justified adding one here, and the next tool needs its
        // own.
        assert!(
            TOOLS.len() <= 14,
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
    fn a_substitution_becomes_one_edit_call_naming_every_file() {
        // The adapter's whole job, and the shape that matters: **one** request,
        // whatever the file count. If this ever became one request per file the
        // measurement that motivated the operation would be back where it was,
        // with the loop moved from the model into this process.
        let calls = (TOOLS
            .iter()
            .find(|t| t.name == "thalyx_edit")
            .unwrap()
            .calls)(&json!({
            "action": "substitute",
            "old": "UidRegistry",
            "new": "UidRegistryRenamed",
            "paths": ["core/src/uids.rs", "core/src/run.rs", "cli/src/render.rs"],
        }))
        .expect("a substitution");

        assert_eq!(
            calls,
            vec![(
                "edit",
                vec![
                    "core/src/uids.rs".to_string(),
                    "substitute".to_string(),
                    "UidRegistry".to_string(),
                    "UidRegistryRenamed".to_string(),
                    "core/src/run.rs".to_string(),
                    "cli/src/render.rs".to_string(),
                ]
            )]
        );
    }

    #[test]
    fn a_substitution_that_names_no_file_is_refused_here_rather_than_invented() {
        let edit = TOOLS.iter().find(|t| t.name == "thalyx_edit").unwrap();
        // An empty list is a call that means nothing, and an adapter that
        // quietly turned it into "every file" or into "no files, ok" would be
        // deciding something the machine never said.
        assert!(
            (edit.calls)(&json!({
                "action": "substitute", "old": "a", "new": "b", "paths": []
            }))
            .is_err()
        );
        // And the single-file spelling still works, because both are natural to
        // write and refusing one costs a whole turn to find out which.
        let calls = (edit.calls)(&json!({
            "action": "substitute", "old": "a", "new": "b", "path": "src/x.rs"
        }))
        .expect("one file is enough");
        assert_eq!(calls[0].1[0], "src/x.rs");
        assert_eq!(calls[0].1[1], "substitute");
    }

    #[test]
    fn the_edit_tool_tells_the_model_which_of_its_two_shapes_to_reach_for() {
        // Point 8 of the delivery this came from: a better operation the model
        // never picks buys nothing. The description is the only thing that
        // decides, so it has to name the case — repeated, mechanical, many
        // files — and it has to warn that this matches text and not symbols.
        let edit = TOOLS.iter().find(|t| t.name == "thalyx_edit").unwrap();
        for taught in [
            "substitute",
            // The second shape, which is worth nothing if the model never
            // learns that the first one cannot carry two patterns.
            "substitute_batch",
            "operations",
            "mechanical",
            "one call",
            "thalyx_symbol",
            "paths",
        ] {
            assert!(
                edit.description.contains(taught),
                "the edit description never mentions `{taught}`"
            );
        }
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
    fn a_batch_of_substitutions_becomes_one_line_with_a_count_in_front_of_each() {
        // The composition, spelled out. Nothing here is clever and that is the
        // point: `edit::read_operations` reads exactly this back, and the two
        // are a grammar that only works if both halves agree about where the
        // counts go.
        let edit = TOOLS.iter().find(|t| t.name == "thalyx_edit").unwrap();
        let calls = (edit.calls)(&json!({
            "action": "substitute_batch",
            "operations": [
                {"old": "a::B::c", "new": "a::D::c", "paths": ["one.rs", "two.rs"]},
                {"old": "B::c", "new": "D::c", "paths": ["three.rs"]},
            ]
        }))
        .unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "edit");
        assert_eq!(
            calls[0].1,
            vec![
                // The first operation's first file, where `editar` puts a name.
                "one.rs",
                "substitute_batch",
                // Two files in the first operation, one of which is above.
                "2",
                "a::B::c",
                "a::D::c",
                "two.rs",
                "1",
                "B::c",
                "D::c",
                "three.rs",
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_batch_that_is_not_a_batch_is_refused_here_rather_than_by_the_machine() {
        // Every one of these costs a round trip into the machine if it goes out,
        // and none of them needs one to be seen as wrong.
        let edit = TOOLS.iter().find(|t| t.name == "thalyx_edit").unwrap();
        for given in [
            json!({"action": "substitute_batch"}),
            json!({"action": "substitute_batch", "operations": []}),
            json!({"action": "substitute_batch", "operations": [{"new": "b", "paths": ["x"]}]}),
            json!({"action": "substitute_batch", "operations": [{"old": "a", "paths": ["x"]}]}),
            json!({"action": "substitute_batch", "operations": [{"old": "a", "new": "b"}]}),
            json!({"action": "substitute_batch",
                   "operations": [{"old": "a", "new": "b", "paths": []}]}),
        ] {
            assert!(
                (edit.calls)(&given).is_err(),
                "`{given}` was composed into a line instead of refused"
            );
        }
    }

    #[test]
    fn a_batch_asks_for_fewer_bytes_than_the_calls_it_replaces() {
        // The request half of the table in
        // `tests/several_substitutions_are_one_call.rs`, which measures the
        // answers. Both directions are asserted rather than reported, because a
        // change that made the surface *bigger* to send would be a change that
        // moved the cost rather than removing it.
        let five: Vec<Value> = [
            (
                "store::Ledger::open",
                "store::LedgerRenamed::open",
                vec!["api/src/serve.rs"],
            ),
            (
                "pub struct Ledger;",
                "pub struct LedgerRenamed;",
                vec!["store/src/ledger.rs"],
            ),
            (
                "impl Ledger {",
                "impl LedgerRenamed {",
                vec!["store/src/ledger.rs"],
            ),
            (
                "(Ledger, usize)",
                "(LedgerRenamed, usize)",
                vec!["store/src/ledger.rs", "api/src/report.rs"],
            ),
            (
                "Ledger::open",
                "LedgerRenamed::open",
                vec!["api/src/report.rs"],
            ),
        ]
        .into_iter()
        .map(|(old, new, paths)| {
            json!({"action": "substitute", "old": old, "new": new,
                                        "paths": paths})
        })
        .collect();
        let one = json!({
            "action": "substitute_batch",
            "operations": five.iter().map(|call| json!({
                "old": call["old"], "new": call["new"], "paths": call["paths"]
            })).collect::<Vec<_>>()
        });

        let apart: usize = five.iter().map(|call| call.to_string().len()).sum();
        let together = one.to_string().len();
        assert!(
            together < apart,
            "one batch is {together} bytes to send and five calls are {apart}"
        );
        // And every one of them still composes, so the comparison is between two
        // things the machine would actually accept.
        let edit = TOOLS.iter().find(|t| t.name == "thalyx_edit").unwrap();
        assert!((edit.calls)(&one).is_ok());
        for call in &five {
            assert!((edit.calls)(call).is_ok());
        }
    }

    /// How many times a caller has to speak to the machine for one reversible
    /// change, counted off the tool surface itself.
    ///
    /// `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md`: runs 5 and 6 of the
    /// reversible benchmark are the same six calls, and four of them are this
    /// sequence. What is asserted below is the sequence, not a saving in
    /// somebody's bill — that number cannot be known without running the
    /// benchmark again, and a fixture that claimed one would be inventing it.
    fn round_trips(calls: &[(&str, Value)]) -> usize {
        for (name, arguments) in calls {
            let tool = TOOLS
                .iter()
                .find(|tool| &tool.name == name)
                .unwrap_or_else(|| panic!("`{name}` is not a tool"));
            (tool.calls)(arguments)
                .unwrap_or_else(|why| panic!("`{name}` refused its own fixture: {why}"));
        }
        calls.len()
    }

    #[test]
    fn opening_an_attempt_and_changing_something_is_one_round_trip_and_two_requests() {
        // The traces, as arithmetic. In all three runs `attempt begin` was
        // followed immediately by the edit, with nothing in between: one intent
        // the surface charged twice for.
        let edit = TOOLS.iter().find(|t| t.name == "thalyx_edit").unwrap();
        let together = json!({
            "attempt": "begin",
            "label": "rename",
            "action": "substitute",
            "old": "SlotTable",
            "new": "SlotTableRenamed",
            "paths": ["core/src/slots.rs", "cli/src/render.rs"],
        });
        let requests = (edit.calls)(&together).expect("one call");

        // Two questions for the machine, in the order that makes the change
        // reversible: the snapshot is taken before a byte is written.
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].0, "attempt");
        assert_eq!(requests[0].1, vec!["empezar", "rename"]);
        assert_eq!(requests[1].0, "edit");

        // And the old shape still means what it meant, which is the control: a
        // change that quietly opened an attempt around every edit would be a
        // change nobody asked for.
        let alone = json!({
            "action": "substitute", "old": "a", "new": "b", "paths": ["x.rs"],
        });
        assert_eq!((edit.calls)(&alone).unwrap().len(), 1);
    }

    #[test]
    fn a_reversible_change_is_three_round_trips_where_it_was_five() {
        // The whole B sequence of runs 5 and 6, minus the two spent finding the
        // tools, before and after. Counted as calls made to this surface, which
        // is the only unit this file can honestly measure.
        let before = round_trips(&[
            (
                "thalyx_attempt",
                json!({"action": "begin", "label": "rename"}),
            ),
            (
                "thalyx_edit",
                json!({
                    "action": "substitute", "old": "SlotTable", "new": "SlotTableRenamed",
                    "paths": ["core/src/slots.rs"],
                }),
            ),
            ("thalyx_attempt", json!({"action": "abandon"})),
            (
                "thalyx_attempt",
                json!({"action": "abandon", "confirm": true}),
            ),
        ]);
        let after = round_trips(&[
            (
                "thalyx_edit",
                json!({
                    "attempt": "begin", "label": "rename",
                    "action": "substitute", "old": "SlotTable", "new": "SlotTableRenamed",
                    "paths": ["core/src/slots.rs"],
                }),
            ),
            (
                "thalyx_attempt",
                json!({
                    "action": "abandon",
                    "snapshot": "2026-08-29T11-04-02Z-rename",
                    "state": "w2-0f3c",
                }),
            ),
        ]);
        assert_eq!((before, after), (4, 2), "{before} calls became {after}");
    }

    #[test]
    fn the_same_reversible_change_is_one_round_trip_through_the_program() {
        // The next step of the same measurement, and the one this session is
        // about. Two calls become one — and the one is doing *more*: the
        // rename, the search that proves it landed, and the decision to keep it
        // or undo it, none of which comes back here.
        let through_the_program = round_trips(&[(
            "thalyx_exec",
            json!({
                "label": "rename",
                "steps": [{
                    "verb": "edit",
                    "arguments": [
                        "core/src/slots.rs", "sustituir", "SlotTable", "SlotTableRenamed"
                    ]
                }],
                "validate": [{"check": "text", "text": "SlotTable", "expect": "none"}],
            }),
        )]);
        assert_eq!(through_the_program, 1);
    }

    #[test]
    fn a_program_travels_as_one_request_however_many_steps_it_holds() {
        // The property the whole hypothesis rests on: what the machine is asked
        // does not grow with what the machine does. If this ever fails, the
        // adapter has started making a round trip per step and the tool is a
        // loop wearing a transaction's name.
        let exec = TOOLS.iter().find(|t| t.name == "thalyx_exec").unwrap();
        for how_many in [1usize, 8, 40] {
            let steps: Vec<Value> = (0..how_many)
                .map(|n| json!({"verb": "make_file", "arguments": [format!("f{n}.rs")]}))
                .collect();
            let sent = (exec.calls)(&json!({"steps": steps})).expect("composed");
            assert_eq!(
                sent.len(),
                1,
                "{how_many} steps became {} requests",
                sent.len()
            );
            assert_eq!(sent[0].0, "exec");
            assert_eq!(sent[0].1.len(), 1, "a program is one argument");
        }
    }

    #[test]
    fn a_program_with_no_steps_is_refused_here_rather_than_in_the_machine() {
        let exec = TOOLS.iter().find(|t| t.name == "thalyx_exec").unwrap();
        for wrong in [json!({}), json!({"steps": []})] {
            assert!((exec.calls)(&wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn the_program_reaches_the_machine_with_everything_the_caller_said_in_it() {
        // The adapter composes and never decides. A field dropped here would be
        // a validation the caller asked for and never got, and the run would
        // commit having checked nothing — silently, and looking like a success.
        let exec = TOOLS.iter().find(|t| t.name == "thalyx_exec").unwrap();
        let sent = (exec.calls)(&json!({
            "label": "rename",
            "steps": [{"verb": "edit", "arguments": ["a.rs", "sustituir", "A", "B"]}],
            "validate": [{"check": "text", "text": "A", "expect": "none"}],
            "on_failure": "keep",
        }))
        .expect("composed");
        let program: Value = serde_json::from_str(&sent[0].1[0]).expect("a JSON program");
        assert_eq!(program["label"], json!("rename"));
        assert_eq!(program["on_failure"], json!("keep"));
        assert_eq!(program["validate"][0]["check"], json!("text"));
        assert_eq!(program["steps"][0]["arguments"][3], json!("B"));
    }

    #[test]
    fn abandoning_in_one_call_states_which_attempt_and_what_it_costs() {
        let attempt = TOOLS.iter().find(|t| t.name == "thalyx_attempt").unwrap();
        assert_eq!(
            (attempt.calls)(&json!({
                "action": "abandon",
                "snapshot": "2026-08-29T11-04-02Z-rename",
                "state": "w2-0f3cbe11",
            }))
            .unwrap(),
            vec![(
                "attempt",
                vec![
                    "abandonar".to_string(),
                    "snapshot=2026-08-29T11-04-02Z-rename".to_string(),
                    "state=w2-0f3cbe11".to_string(),
                ]
            )]
        );
    }

    #[test]
    fn a_claim_and_a_blind_yes_together_send_the_claim_and_not_the_yes() {
        // `si` beside a stale claim would have the machine wave the claim
        // through on the blind word, which is the one thing the claim exists to
        // stop. So the claim goes alone, and the machine checks it.
        let attempt = TOOLS.iter().find(|t| t.name == "thalyx_attempt").unwrap();
        let sent = (attempt.calls)(&json!({
            "action": "abandon", "confirm": true,
            "snapshot": "2026-08-29T11-04-02Z-rename", "state": "w2-0f3cbe11",
        }))
        .unwrap();
        assert!(!sent[0].1.iter().any(|word| word == "si"), "{sent:?}");
    }

    #[test]
    fn a_half_stated_claim_falls_back_to_the_protocol_it_replaces() {
        // Each of these is a caller that has not said what it accepts. None of
        // them may become a one-call abandon here, and none of them is refused
        // here either: the machine answers with the cost, which is what a
        // caller that said nothing gets.
        let attempt = TOOLS.iter().find(|t| t.name == "thalyx_attempt").unwrap();
        for half in [
            json!({"action": "abandon", "snapshot": "x"}),
            json!({"action": "abandon", "state": "w2-0f3cbe11"}),
        ] {
            let sent = (attempt.calls)(&half).expect("composed");
            assert_eq!(sent[0].1, vec!["abandonar".to_string()], "{half}");
        }
    }

    #[test]
    fn a_mutation_may_open_a_boundary_and_may_not_settle_one() {
        // The line this composition does not cross. Opening an attempt costs
        // nothing and can be taken back; keeping or abandoning one is a decision
        // about somebody's work, and it belongs to the verb that shows the cost.
        for name in ["thalyx_edit", "thalyx_file"] {
            let tool = TOOLS.iter().find(|t| t.name == name).unwrap();
            let arguments = json!({
                "attempt": "commit",
                "action": if name == "thalyx_edit" { "substitute" } else { "create" },
                "old": "a", "new": "b", "path": "x.rs",
            });
            assert!(
                (tool.calls)(&arguments).is_err(),
                "`{name}` composed an attempt it must not settle"
            );
        }
    }

    #[test]
    fn every_mutating_tool_can_open_its_own_boundary_and_no_other_tool_pretends_to() {
        // A surface where one mutation can do this and another cannot is a
        // surface an agent has to remember exceptions about — and a reading tool
        // that advertised it would be advertising something that does nothing.
        for tool in TOOLS {
            let offers = (tool.schema)()
                .get("properties")
                .and_then(Value::as_object)
                .is_some_and(|properties| properties.contains_key("attempt"));
            let mutates = matches!(tool.name, "thalyx_edit" | "thalyx_file");
            assert_eq!(
                offers, mutates,
                "`{}` offers `attempt`: {offers}",
                tool.name
            );
        }
    }

    #[test]
    fn an_argument_of_the_wrong_shape_is_refused_here_and_never_sent() {
        let read = TOOLS.iter().find(|t| t.name == "thalyx_read").unwrap();
        assert!((read.calls)(&json!({})).is_err());
        assert!((read.calls)(&json!({"path": 3})).is_err());
    }
}
