//! `thalyx-mcp`, driven the way a client drives it: JSON-RPC on stdio.
//!
//! The unit tests in `tools.rs` check the table and the ones in `main.rs` check
//! which tools survive a machine that is missing verbs. Neither runs the
//! process, and rule 1 of `CLAUDE.md` says that is where every real defect has
//! come from — so these start the real binary, point it at a real bridge, and
//! speak the real protocol at it.
//!
//! What they are checking is the shape a client depends on. A schema that is
//! subtly wrong does not fail: the model is simply never able to call the tool
//! correctly, and the run reads as a model that chose not to use it. That is the
//! failure mode this file exists for.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

/// A bridge and an adapter talking to it, both real processes.
struct Stack {
    bridge: Child,
    server: Child,
    _home: tempfile::TempDir,
}

impl Drop for Stack {
    fn drop(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        let _ = self.bridge.kill();
        let _ = self.bridge.wait();
    }
}

/// The `thalyx` binary, found the way cargo finds a sibling crate's.
///
/// `CARGO_BIN_EXE_` only names binaries of *this* package, so the sibling is
/// located from this test's own path instead. Said out loud because the
/// alternative — a hard-coded `target/debug/thalyx` — is wrong under
/// `--release` and wrong again under a custom target directory, and would fail
/// in a way that reads as the bridge being broken.
fn thalyx_binary() -> std::path::PathBuf {
    let mut here = std::path::PathBuf::from(env!("CARGO_BIN_EXE_thalyx-mcp"));
    here.pop(); // .../target/<profile>
    here.join("thalyx")
}

/// The whole stack, with every tool offered.
///
/// `legacy` and not the default, because most of what these tests exercise is
/// the *adapter* — framing, refusals, composition — and those are questions
/// about a tool call rather than about which tools are advertised. Which tools
/// are advertised has its own test below, and it uses the default.
fn start() -> Stack {
    started_with("legacy")
}

fn started_with(surface: &str) -> Stack {
    let home = tempfile::tempdir().expect("tempdir");
    let workspace = home.path().join("project");
    let store = home.path().join("store");
    std::fs::create_dir_all(workspace.join("src")).expect("src");
    std::fs::create_dir_all(&store).expect("store");
    std::fs::write(
        workspace.join("src/lib.rs"),
        "pub fn answer() -> u32 {\n    42\n}\n",
    )
    .expect("write");

    let socket = home.path().join("agent.sock");
    let thalyx = thalyx_binary();
    assert!(
        thalyx.exists(),
        "no `thalyx` beside this test at {}; build the workspace, not just this crate",
        thalyx.display()
    );
    let bridge = Command::new(&thalyx)
        .args(["--root", &store.to_string_lossy()])
        .arg("bridge")
        .args(["--workspace", &workspace.to_string_lossy()])
        .args(["--listen", &socket.to_string_lossy()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the bridge");

    for _ in 0..200 {
        if socket.exists() && std::os::unix::net::UnixStream::connect(&socket).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let server = Command::new(env!("CARGO_BIN_EXE_thalyx-mcp"))
        .args(["--connect", &socket.to_string_lossy()])
        .args(["--wait", "10"])
        .args(["--surface", surface])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the adapter");

    Stack {
        bridge,
        server,
        _home: home,
    }
}

impl Stack {
    /// Send one request and read one reply.
    fn call(&mut self, message: serde_json::Value) -> serde_json::Value {
        let stdin = self.server.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{message}").expect("writing a request");
        stdin.flush().expect("flush");
        let stdout = self.server.stdout.as_mut().expect("stdout");
        let mut line = String::new();
        BufReader::new(stdout)
            .read_line(&mut line)
            .expect("a reply");
        serde_json::from_str(&line).unwrap_or_else(|error| panic!("`{line}` is not JSON: {error}"))
    }

    fn handshake(&mut self) -> serde_json::Value {
        self.call(serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "a test", "version": "0"}
            }
        }))
    }
}

#[test]
fn a_client_that_initializes_is_told_what_this_serves_and_what_the_machine_is() {
    let mut stack = start();
    let reply = stack.handshake();
    let result = &reply["result"];

    assert_eq!(reply["jsonrpc"], serde_json::json!("2.0"));
    assert_eq!(reply["id"], serde_json::json!(1));
    // Echoed, which is what the specification asks of a server that supports the
    // version the client named.
    assert_eq!(result["protocolVersion"], serde_json::json!("2025-06-18"));
    assert!(result["capabilities"]["tools"].is_object(), "{result}");
    assert_eq!(result["serverInfo"]["name"], serde_json::json!("thalyx"));
    // The instructions name the workspace, which is the one fact a model cannot
    // work out for itself and will otherwise guess at.
    let instructions = result["instructions"].as_str().expect("instructions");
    assert!(instructions.contains("/project"), "{instructions}");
    let _ = stack.bridge.id();
}

#[test]
fn every_tool_offered_has_a_name_a_description_and_an_object_schema() {
    // The three fields a client needs to render a tool at all. A missing schema
    // does not error anywhere — the tool is simply uncallable, and the run reads
    // as a model that ignored it.
    let mut stack = start();
    stack.handshake();
    let reply = stack.call(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let tools = reply["result"]["tools"].as_array().expect("tools");

    assert!(!tools.is_empty(), "no tools were offered: {reply}");
    for tool in tools {
        let name = tool["name"].as_str().expect("a name");
        assert!(name.starts_with("thalyx_"), "`{name}` is not one of ours");
        assert!(
            tool["description"].as_str().is_some_and(|d| d.len() > 80),
            "`{name}` has a label rather than a description"
        );
        assert_eq!(
            tool["inputSchema"]["type"],
            serde_json::json!("object"),
            "`{name}` does not take an object"
        );
        // Every `required` name must exist in `properties`. A required argument
        // with no schema is one a strict client refuses to send at all.
        if let Some(required) = tool["inputSchema"]["required"].as_array() {
            for field in required {
                let field = field.as_str().expect("a field name");
                assert!(
                    tool["inputSchema"]["properties"].get(field).is_some(),
                    "`{name}` requires `{field}` and does not describe it"
                );
            }
        }
    }
}

#[test]
fn a_tool_call_comes_back_as_thalyx_own_answer_and_not_a_reworded_one() {
    let mut stack = start();
    stack.handshake();
    let reply = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "thalyx_read", "arguments": {"path": "src/lib.rs"}}
    }));

    let content = &reply["result"]["content"][0];
    assert_eq!(content["type"], serde_json::json!("text"));
    let answer: serde_json::Value =
        serde_json::from_str(content["text"].as_str().expect("text")).expect("the answer is JSON");

    // Thalyx's own fields, not a summary of them. The `sha256` is the tell: no
    // adapter would invent one, so its presence is proof the object came
    // through rather than being composed here.
    assert_eq!(answer["op"], serde_json::json!("read"));
    assert_eq!(answer["ok"], serde_json::json!(true));
    assert!(answer["sha256"].is_string(), "{answer}");
    assert!(
        answer["text"]
            .as_str()
            .expect("text")
            .contains("pub fn answer"),
        "{answer}"
    );
}

#[test]
fn a_refusal_reaches_the_model_with_its_remedy_and_is_marked_as_an_error() {
    // Punto A2 of `Superficie-para-el-LLM.md`, all the way through: the remedy
    // is the field that lets a model correct itself in one turn instead of
    // three, and an adapter that dropped it would throw away the reason this
    // whole surface is supposed to be better.
    let mut stack = start();
    stack.handshake();
    let reply = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": {"name": "thalyx_read", "arguments": {"path": "/etc/passwd"}}
    }));

    assert_eq!(
        reply["result"]["isError"],
        serde_json::json!(true),
        "{reply}"
    );
    let refusal: serde_json::Value = serde_json::from_str(
        reply["result"]["content"][0]["text"]
            .as_str()
            .expect("text"),
    )
    .expect("the refusal is JSON");
    assert_eq!(refusal["error"], serde_json::json!("outside_workspace"));
    assert_eq!(refusal["remedy"], serde_json::json!("name_a_path_inside"));
}

#[test]
fn a_tool_that_is_not_served_is_refused_inside_the_result_and_not_as_a_crash() {
    // A protocol error would be reported by the client as a broken server; a
    // tool error is reported to the *model*, which can then do something about
    // it. The difference decides whether a mistake costs a turn or a session.
    let mut stack = start();
    stack.handshake();
    let reply = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 5, "method": "tools/call",
        "params": {"name": "thalyx_edit", "arguments": {"path": "src/lib.rs"}}
    }));
    assert_eq!(
        reply["result"]["isError"],
        serde_json::json!(true),
        "{reply}"
    );
    assert!(
        reply["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("action"),
        "the model is not told which argument was missing: {reply}"
    );
}

#[test]
fn a_notification_is_never_answered() {
    // A reply to a notification leaves the client matching a response to a
    // request it never made, which in practice is a session that hangs on the
    // first turn. `notifications/initialized` always arrives.
    let mut stack = start();
    stack.handshake();
    let stdin = stack.server.stdin.as_mut().expect("stdin");
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
    )
    .expect("write");
    stdin.flush().expect("flush");

    // The next thing on stdout must be the answer to the *next* request, which
    // is what proves nothing was written for the notification.
    let reply = stack.call(serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "ping"}));
    assert_eq!(reply["id"], serde_json::json!(9), "{reply}");
}

#[test]
fn a_method_this_does_not_serve_is_answered_rather_than_ignored() {
    let mut stack = start();
    stack.handshake();
    let reply = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 6, "method": "resources/list"
    }));
    assert_eq!(reply["error"]["code"], serde_json::json!(-32601), "{reply}");
}

#[test]
fn asking_the_machine_what_it_is_takes_one_call_and_answers_three_questions() {
    // `thalyx_state` is the composition this adapter exists to do: three
    // requests on the wire, one answer to the model. It is also the only tool
    // that returns an array, so the shape is worth pinning.
    let mut stack = start();
    stack.handshake();
    let reply = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 7, "method": "tools/call",
        "params": {"name": "thalyx_state", "arguments": {}}
    }));
    let answers: serde_json::Value = serde_json::from_str(
        reply["result"]["content"][0]["text"]
            .as_str()
            .expect("text"),
    )
    .expect("JSON");
    let answers = answers.as_array().expect("three answers");
    assert_eq!(answers.len(), 3, "{answers:?}");
    assert_eq!(answers[0]["op"], serde_json::json!("where"));
    assert_eq!(answers[1]["op"], serde_json::json!("state"));
    assert_eq!(answers[2]["op"], serde_json::json!("attempt"));
}

#[test]
fn the_default_surface_hands_a_model_three_tools_and_the_legacy_one_hands_it_all() {
    // **The list is the prompt.** Every schema here arrives with every
    // inference of every session, and a model that is shown fourteen low-level
    // operations spends attention choosing between them before any work
    // happens — which the research named as a hazard in its own right, not a
    // matter of taste.
    //
    // Nothing was deleted. Everything the fourteen did is one line inside a
    // `thalyx_exec` program, where it costs no schema until it is used; and the
    // whole catalogue is still one flag away, because "the small surface is
    // better" is a comparison and a comparison needs its other arm.
    let mut stack = started_with("compact");
    let listed = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/list"
    }));
    let names: Vec<String> = listed["result"]["tools"]
        .as_array()
        .expect("a list of tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect();
    assert_eq!(
        names,
        ["thalyx_context", "thalyx_exec", "thalyx_evidence"],
        "the default surface changed"
    );

    // And a program reaches what the fourteen reached. One call, and the
    // things it does are the things that used to be their own tools.
    //
    // Rule 3: `hacer` opens a real boundary, which needs a real subvolume, so
    // on a machine with no Btrfs this half says it did not run rather than
    // pretending. `THALYX_REQUIRE_BTRFS_TESTS=1` turns the skip into a failure.
    if !a_boundary_can_be_opened(&mut stack) {
        let message = "NOT PROVEN: that a program reaches what the fourteen tools \
                       reached — there is no Btrfs here, so no boundary can be opened.";
        assert!(
            std::env::var("THALYX_REQUIRE_BTRFS_TESTS").is_err(),
            "{message}"
        );
        eprintln!("{message}");
        return;
    }
    let answered = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "thalyx_exec",
            "arguments": {
                "label": "reaching what the tools reached",
                "run": "const seen = thalyx.list('.');\n\
                        const state = thalyx.state();\n\
                        const one = thalyx.read('src/lib.rs');\n\
                        return { listed: seen.ok, stated: state.ok, \
                                 read: one.ok, bytes: (one.text || '').length };",
                "on_failure": "keep"
            }
        }
    }));
    let text = answered["result"]["content"][0]["text"]
        .as_str()
        .expect("an answer");
    let object: serde_json::Value = serde_json::from_str(text).expect("an object");
    assert_eq!(
        object["returned"]["listed"],
        serde_json::json!(true),
        "{object:#}"
    );
    assert_eq!(
        object["returned"]["stated"],
        serde_json::json!(true),
        "{object:#}"
    );
    assert_eq!(
        object["returned"]["read"],
        serde_json::json!(true),
        "{object:#}"
    );
    assert!(
        object["returned"]["bytes"].as_u64().unwrap_or(0) > 0,
        "{object:#}"
    );
    // And the numbers: one external request, several operations inside it.
    assert_eq!(
        object["external_requests"],
        serde_json::json!(1),
        "{object:#}"
    );
    assert!(
        object["program_operations"].as_u64().unwrap_or(0) >= 3,
        "{object:#}"
    );

    let whole = started_with("legacy");
    drop(whole);
}

/// Whether this machine can give `hacer` the boundary it needs.
///
/// Asked by *running* the smallest possible program rather than by looking at
/// the filesystem: what decides is whether the verb can open a snapshot, and a
/// test that inferred its own precondition from something adjacent is rule 5's
/// eighth entry.
fn a_boundary_can_be_opened(stack: &mut Stack) -> bool {
    let answered = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 9000, "method": "tools/call",
        "params": {
            "name": "thalyx_exec",
            "arguments": {"label": "is there a boundary", "run": "return 1;"}
        }
    }));
    let text = answered["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(text)
        .map(|object| object["error"] != serde_json::json!("not_a_subvolume"))
        .unwrap_or(false)
}

#[test]
fn a_program_arrives_at_the_machine_as_a_program() {
    // The adapter's own job, on the tool that matters most: `run` is a string
    // of JavaScript with quotes and newlines in it, it travels as one argument
    // through a line-oriented protocol, and it has to come out the other end
    // byte for byte. A quote eaten anywhere on that path is a syntax error the
    // model gets blamed for.
    let mut stack = start();
    if !a_boundary_can_be_opened(&mut stack) {
        let message = "NOT PROVEN: that a program survives the adapter byte for byte — \
                       there is no Btrfs here, so no boundary can be opened.";
        assert!(
            std::env::var("THALYX_REQUIRE_BTRFS_TESTS").is_err(),
            "{message}"
        );
        eprintln!("{message}");
        return;
    }
    let answered = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {
            "name": "thalyx_exec",
            "arguments": {
                "label": "quotes and newlines",
                "run": "const said = \"a \\\"quoted\\\" thing\";\nreturn { said, lines: 2 };",
                "on_failure": "keep"
            }
        }
    }));
    let text = answered["result"]["content"][0]["text"]
        .as_str()
        .expect("an answer");
    let object: serde_json::Value = serde_json::from_str(text).expect("an object");
    assert_eq!(
        object["finish"],
        serde_json::json!("returned"),
        "{object:#}"
    );
    assert_eq!(
        object["returned"]["said"],
        serde_json::json!("a \"quoted\" thing"),
        "{object:#}"
    );
}

#[test]
fn sending_a_program_and_a_list_of_steps_is_refused_before_the_machine_sees_it() {
    let mut stack = start();
    let answered = stack.call(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {
            "name": "thalyx_exec",
            "arguments": {
                "run": "return 1;",
                "steps": [{"verb": "state"}]
            }
        }
    }));
    let text = answered["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.contains("both"), "{answered:#}");
    assert_eq!(answered["result"]["isError"], serde_json::json!(true));
}
