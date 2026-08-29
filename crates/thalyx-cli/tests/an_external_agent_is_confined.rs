//! The bridge, driven the way a host drives it: a socket, frames, and answers.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md`. `external.rs` has unit tests
//! for the containment arithmetic and `thalyx-bridge` has them for the framing.
//! What neither can check is the thing rule 1 of `CLAUDE.md` says is the only
//! thing that has ever found a real defect: **the whole of it, running.**
//!
//! So these start a real `thalyx bridge` process, connect a real socket to it,
//! and speak the real protocol. Two of the defects already found here could not
//! have been found any other way — `leer` was taking the rest of the line raw, so
//! every quoted path came back `'src/main.rs' is not there`, and `ensayo` was
//! being handed a quoted verb name that no machine has. Both passed every unit
//! test in the crate.
//!
//! ## What this file is *not* proving
//!
//! virtio-serial, and Btrfs. The transport here is a UNIX socket, which is the
//! same `serve` over a different pair of descriptors; and `intento` needs a
//! subvolume, which this container has no filesystem capable of. Both are named
//! where they are named rather than skipped quietly — see
//! `an_attempt_on_a_plain_directory_is_refused_rather_than_faked`, which is the
//! fail-closed half and *is* checkable here.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// A running bridge and the workspace it is confined to.
struct Bridge {
    child: Child,
    socket: PathBuf,
    workspace: PathBuf,
    _home: tempfile::TempDir,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Bridge {
    /// A workspace with a small project in it, and a bridge serving it.
    fn open() -> Self {
        let home = tempfile::tempdir().expect("tempdir");
        let workspace = home.path().join("project");
        let store = home.path().join("store");
        std::fs::create_dir_all(workspace.join("src")).expect("src");
        std::fs::create_dir_all(&store).expect("store");
        std::fs::write(
            workspace.join("src/main.rs"),
            "mod greeting;\nfn main() {\n    greeting::greet(\"world\");\n}\n",
        )
        .expect("write");
        std::fs::write(
            workspace.join("src/greeting.rs"),
            "pub fn greet(who: &str) {\n    println!(\"hello {who}\");\n}\n",
        )
        .expect("write");
        // Something outside the workspace and inside the same parent, so that a
        // guard which only compared prefixes of strings would let it through.
        std::fs::write(home.path().join("secret.txt"), "not the agent's\n").expect("write");

        // A socket path under the same temporary directory, so a test that is
        // killed leaves nothing behind in /tmp.
        let socket = home.path().join("agent.sock");
        let child = Command::new(env!("CARGO_BIN_EXE_thalyx"))
            .args(["--root", &store.to_string_lossy()])
            .arg("bridge")
            .args(["--workspace", &workspace.to_string_lossy()])
            .args(["--listen", &socket.to_string_lossy()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the bridge");

        // Waited for rather than slept on: a fixed sleep is either slower than
        // it needs to be or shorter than a loaded machine needs, and this suite
        // has already paid once for a test that raced with a file it had just
        // written.
        for _ in 0..200 {
            if socket.exists() && UnixStream::connect(&socket).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        Self {
            child,
            socket,
            workspace,
            _home: home,
        }
    }

    /// One connection, with its hello already read and kept.
    ///
    /// The hello is kept rather than discarded because of what happened when it
    /// was not: a test opened a *second* connection to read it and hung. The
    /// bridge serves one agent at a time — which is right, a machine has one
    /// agent channel and two sessions sharing one workspace is not a feature —
    /// so a second connection opened while the first is live waits forever.
    fn connect(&self) -> Wire {
        let stream = UnixStream::connect(&self.socket).expect("connecting to the bridge");
        let mut wire = Wire {
            stream,
            hello: serde_json::Value::Null,
        };
        let hello = wire.read();
        assert_eq!(hello["type"], serde_json::json!("hello"), "{hello}");
        wire.hello = hello;
        wire
    }
}

struct Wire {
    stream: UnixStream,
    hello: serde_json::Value,
}

impl Wire {
    /// The verb ids this session said it would take.
    fn verbs(&self) -> Vec<String> {
        self.hello["verbs"]
            .as_array()
            .expect("a hello names its verbs")
            .iter()
            .map(|verb| verb.as_str().expect("a name").to_string())
            .collect()
    }
}

impl Wire {
    fn read(&mut self) -> serde_json::Value {
        let mut length = [0u8; 4];
        self.stream.read_exact(&mut length).expect("a frame length");
        let mut body = vec![0u8; u32::from_le_bytes(length) as usize];
        self.stream.read_exact(&mut body).expect("a frame body");
        serde_json::from_slice(&body).expect("a frame is JSON")
    }

    fn write(&mut self, body: &[u8]) {
        self.stream
            .write_all(&(body.len() as u32).to_le_bytes())
            .expect("a length");
        self.stream.write_all(body).expect("a body");
    }

    fn ask(&mut self, verb: &str, arguments: &[&str]) -> serde_json::Value {
        let request = serde_json::json!({
            "type": "request",
            "id": format!("t-{verb}"),
            "verb": verb,
            "arguments": arguments,
        });
        self.write(request.to_string().as_bytes());
        let answer = self.read();
        assert_eq!(answer["id"], serde_json::json!(format!("t-{verb}")));
        answer
    }
}

/// The `answer` of a response, or a panic naming what came back instead.
fn answered(value: &serde_json::Value) -> &serde_json::Value {
    assert_eq!(
        value["type"],
        serde_json::json!("response"),
        "expected an answer and got {value}"
    );
    &value["answer"]
}

/// The `word` of an error, or a panic naming what came back instead.
fn refused(value: &serde_json::Value) -> &str {
    assert_eq!(
        value["type"],
        serde_json::json!("error"),
        "expected a refusal and got {value}"
    );
    value["word"].as_str().expect("a refusal has a word")
}

/// The same line, typed at a real session in the machine face.
///
/// The other half of the equivalence claim. Not a re-implementation: it is the
/// text session, running as a program, answering the same verb — which is the
/// thing the bridge must not have grown a second version of.
fn typed_at_a_session(root: &Path, workspace: &Path, line: &str) -> serde_json::Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .arg("session")
        .env("THALYX_ROOT", root)
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the session");
    let typed = format!(
        "estructurado on\nir '{}'\n{line}\nsalir\n",
        workspace.display()
    );
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(typed.as_bytes())
        .expect("typing");
    let out = child.wait_with_output().expect("the session finishing");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(|value| value.is_object())
        .find(|value| value["op"] == serde_json::json!("read"))
        .unwrap_or_else(|| {
            panic!(
                "the session never answered `read`:\n{}",
                String::from_utf8_lossy(&out.stdout)
            )
        })
}

#[test]
fn a_hello_names_the_workspace_and_every_verb_the_session_will_take() {
    let bridge = Bridge::open();
    let wire = bridge.connect();

    assert_eq!(wire.hello["protocol"], serde_json::json!(1));
    assert_eq!(
        wire.hello["workspace"],
        serde_json::json!(bridge.workspace.display().to_string())
    );
    let verbs = wire.verbs();
    let verbs: Vec<&str> = verbs.iter().map(String::as_str).collect();
    // The ones every tool on the host is built against. Advertising a set the
    // machine will not honour is the version skew `thalyx-mcp` drops tools over.
    for needed in ["read", "list", "symbol", "depends_on", "attempt", "edit"] {
        assert!(
            verbs.contains(&needed),
            "the hello does not offer `{needed}`"
        );
    }
    // And the ones it must never offer. This is the boundary said on the wire.
    for forbidden in ["power_off", "install_onto", "run", "execute", "deny"] {
        assert!(
            !verbs.contains(&forbidden),
            "the hello offers `{forbidden}` to something outside the machine"
        );
    }
}

#[test]
fn a_file_inside_the_workspace_is_read_and_one_outside_it_is_not() {
    // The baseline and the denial together, rule 4 of `CLAUDE.md`: without the
    // first, a refusal and an operation that never worked look identical.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();

    let inside = wire.ask("read", &["src/main.rs"]);
    assert_eq!(answered(&inside)["ok"], serde_json::json!(true));
    assert!(
        answered(&inside)["text"]
            .as_str()
            .expect("text")
            .contains("greeting::greet"),
        "{inside}"
    );

    assert_eq!(
        refused(&wire.ask("read", &["/etc/passwd"])),
        "outside_workspace"
    );
    assert_eq!(
        refused(&wire.ask("read", &["../secret.txt"])),
        "outside_workspace"
    );
}

#[test]
fn a_symlink_out_of_the_workspace_is_refused_and_one_that_stays_in_is_not() {
    // The check a lexical guard cannot make. Nothing in either argument says
    // `..` or names anything outside, and by every string comparison there is
    // both are inside the workspace.
    let bridge = Bridge::open();
    std::os::unix::fs::symlink("/etc", bridge.workspace.join("out")).expect("symlink");
    std::os::unix::fs::symlink("src", bridge.workspace.join("code")).expect("symlink");
    // Absolute, and pointing at a directory inside this very workspace. Since
    // 2026-08-28 that is refused too — see the narrowing below.
    std::os::unix::fs::symlink(
        bridge.workspace.join("src"),
        bridge.workspace.join("spelled_out"),
    )
    .expect("symlink");
    let mut wire = bridge.connect();

    assert_eq!(
        refused(&wire.ask("read", &["out/passwd"])),
        "outside_workspace"
    );
    // The control beside it, without which a guard that refused every link
    // would look exactly like one that works. It is a **relative** link now:
    // the boundary resolves with `RESOLVE_BENEATH`, and the kernel contains a
    // relative link by construction.
    let inside = wire.ask("read", &["code/greeting.rs"]);
    assert_eq!(answered(&inside)["ok"], serde_json::json!(true), "{inside}");

    // The narrowing, asserted rather than left as a surprise. An **absolute**
    // symlink is resolved against the host's root, and deciding whether it
    // lands inside the workspace means resolving it in userspace first — which
    // is the two-step check the anchor exists to get rid of. So the kernel
    // refuses all of them, including ones that would have landed inside.
    //
    // The direction of the loss is the one to accept, and it is the same
    // trade `crates/thalyx-core/src/api.rs` made for modules: an agent is
    // refused something it should have been allowed, which somebody notices
    // and reports, rather than allowed something it should have been refused,
    // which nobody notices at all.
    assert_eq!(
        refused(&wire.ask("read", &["spelled_out/greeting.rs"])),
        "outside_workspace"
    );
}

#[test]
fn a_verb_that_could_change_the_machine_is_not_reachable_from_outside_it() {
    let bridge = Bridge::open();
    let mut wire = bridge.connect();
    for forbidden in [
        "power_off",
        "install_onto",
        "run",
        "execute",
        "deny",
        "stop",
    ] {
        assert_eq!(
            refused(&wire.ask(forbidden, &[])),
            "not_exposed",
            "`{forbidden}` was not refused"
        );
    }
}

#[test]
fn the_bridge_and_the_session_answer_the_same_question_the_same_way() {
    // The claim that this is an adapter and not a second implementation, made
    // as a measurement rather than an assertion in a comment. The two routes are
    // a socket and a keyboard; what comes back has to mean the same thing.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();

    let over_the_wire = wire.ask("read", &["src/greeting.rs"]).clone();
    let over_the_wire = answered(&over_the_wire);

    let store = bridge._home.path().join("store");
    let typed = typed_at_a_session(&store, &bridge.workspace, "leer 'src/greeting.rs'");

    // Field by field rather than whole-object, because the two answers are
    // *allowed* to differ in nothing at all — and saying which fields is what
    // makes the claim readable.
    for field in ["op", "ok", "path", "bytes", "sha256", "text", "truncated"] {
        assert_eq!(
            over_the_wire[field], typed[field],
            "`{field}` differs between the bridge and the session:\n  bridge {over_the_wire}\n  session {typed}"
        );
    }
}

#[test]
fn every_verb_the_session_offers_answers_with_exactly_one_object() {
    // The framing contract of `thalyx_files::machine`, checked by running every
    // verb rather than by reading them. A verb that answered twice, or not at
    // all, is a verb that would leave a host either parsing half an answer or
    // waiting forever — and `answer` turns both into a named refusal, which is
    // what this is really asserting cannot fire.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();
    let hello_verbs = wire.verbs();

    // Arguments that are legal for each, so that what is being measured is the
    // shape of the answer and not a refusal. `remove` and `move` are given a
    // scratch file made for them.
    std::fs::write(bridge.workspace.join("scratch.txt"), "x\n").expect("write");
    std::fs::write(bridge.workspace.join("doomed.txt"), "x\n").expect("write");
    for verb in &hello_verbs {
        let arguments: Vec<&str> = match verb.as_str() {
            "state" | "where" | "attempt" => vec![],
            "describe" => vec!["read"],
            "list" | "index_build" => vec!["."],
            "read" | "depends_on" | "depended_on_by" => vec!["src/main.rs"],
            "symbol" => vec!["greet"],
            "find" => vec!["*.rs"],
            "grep" => vec!["greet"],
            "edit" => vec!["src/main.rs", "ver", "1"],
            "make_file" => vec!["made.txt"],
            "make_directory" => vec!["made"],
            "copy" => vec!["scratch.txt", "copy.txt"],
            "move" => vec!["scratch.txt", "moved.txt"],
            "remove" => vec!["doomed.txt"],
            "rehearse" => vec!["rm", "moved.txt"],
            // A whole program as one argument. Here it can only reach
            // `not_a_subvolume` — this container has no Btrfs — and that is
            // still the thing this test is about: one line in, exactly one
            // object out, through the real bridge.
            "exec" => vec![r#"{"steps":[{"verb":"list","arguments":["."]}]}"#],
            "evidence" => vec!["t-0"],
            other => panic!("`{other}` is offered and this test does not know how to call it"),
        };
        let answer = wire.ask(verb, &arguments);
        assert_eq!(
            answer["type"],
            serde_json::json!("response"),
            "`{verb}` did not answer with one object: {answer}"
        );
        assert!(
            answer["answer"].is_object(),
            "`{verb}` answered with something that is not an object: {answer}"
        );
    }
}

#[test]
fn an_attempt_on_a_plain_directory_is_refused_rather_than_faked() {
    // Rule 9, at the place it matters most. A workspace that is not a Btrfs
    // subvolume cannot be snapshotted, and an agent told "started" would make
    // thirty changes believing it could take them back. This container has no
    // Btrfs at all, so what runs here is the refusal — and that the refusal is a
    // refusal, and not a copy pretending to be a snapshot, is the thing worth
    // checking. The other half needs Cesar's machine; `dev/verify.sh` has it.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();

    let nothing_open = wire.ask("attempt", &[]);
    assert_eq!(answered(&nothing_open)["open"], serde_json::json!(false));

    let began = wire.ask("attempt", &["empezar", "a change"]);
    let began = answered(&began);
    if began["ok"] == serde_json::json!(true) {
        // A machine that *does* have Btrfs got here. Then the claim is the
        // other one: it opened, and it says what abandoning would cost.
        let open = wire.ask("attempt", &[]);
        assert_eq!(answered(&open)["open"], serde_json::json!(true));
        return;
    }
    assert_eq!(
        began["error"],
        serde_json::json!("not_a_subvolume"),
        "an attempt that cannot be taken back must say so by name: {began}"
    );
    // And nothing is open afterwards. An attempt that failed and left a record
    // saying it was open would block every later one.
    let after = wire.ask("attempt", &[]);
    assert_eq!(answered(&after)["open"], serde_json::json!(false));
}

#[test]
fn a_malformed_request_is_answered_and_never_guessed_at() {
    // Fail closed, and answer anyway. A frame with no reply leaves a host
    // waiting on an id it will never see, which is a hang — and a hang looks
    // exactly like a machine that is thinking.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();

    for malformed in [
        &b"not json at all"[..],
        br#"{"type":"execute","program":"/bin/sh"}"#,
        br#"{"type":"request"}"#,
        br#"{}"#,
    ] {
        wire.write(malformed);
        let answer = wire.read();
        assert_eq!(
            answer["type"],
            serde_json::json!("error"),
            "a malformed frame was not refused: {answer}"
        );
        assert_eq!(answer["word"], serde_json::json!("unintelligible"));
    }

    // And the channel still works afterwards. A protocol that closed on the
    // first bad frame would make one typo the end of an agent's session.
    assert_eq!(
        answered(&wire.ask("where", &[]))["path"],
        serde_json::json!(bridge.workspace.display().to_string())
    );
}

#[test]
fn an_agent_hanging_up_leaves_the_machine_running() {
    // The property that makes this safe to have on a machine at all. The bridge
    // is a thread of the session inside the image; a disconnect that took the
    // session with it would mean anybody who could reach the port could turn the
    // machine off by closing a socket.
    let bridge = Bridge::open();
    {
        let mut first = bridge.connect();
        assert_eq!(
            answered(&first.ask("where", &[]))["ok"],
            serde_json::json!(true)
        );
        // Dropped here: the socket closes with a request half-answered as far
        // as the far end knows.
    }
    // Killed mid-frame as well, which is the uglier way a client goes away: a
    // length with no body behind it.
    {
        let mut wire = bridge.connect();
        wire.stream
            .write_all(&1024u32.to_le_bytes())
            .expect("a length with nothing behind it");
    }

    // And a third agent connects and is served, which is the claim.
    let mut third = bridge.connect();
    let answer = third.ask("read", &["src/main.rs"]);
    assert_eq!(answered(&answer)["ok"], serde_json::json!(true), "{answer}");
}

#[test]
fn a_change_made_through_the_bridge_is_really_on_the_disk() {
    // Rule 1: a test that something was produced correctly is not a test that it
    // works. The answer saying `inserted` is Thalyx's word for it; this reads
    // the file with something that is not Thalyx.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();

    let edited = wire.ask(
        "edit",
        &["src/greeting.rs", "poner", "1", "/// Says hello."],
    );
    assert_eq!(answered(&edited)["ok"], serde_json::json!(true), "{edited}");

    let on_disk = std::fs::read_to_string(bridge.workspace.join("src/greeting.rs")).expect("read");
    assert!(
        on_disk.starts_with("/// Says hello.\n"),
        "the file on disk does not carry the edit:\n{on_disk}"
    );

    let made = wire.ask("make_file", &["src/new.rs"]);
    assert_eq!(answered(&made)["ok"], serde_json::json!(true), "{made}");
    assert!(bridge.workspace.join("src/new.rs").exists());

    let removed = wire.ask("remove", &["src/new.rs"]);
    assert_eq!(
        answered(&removed)["ok"],
        serde_json::json!(true),
        "{removed}"
    );
    assert!(!bridge.workspace.join("src/new.rs").exists());
}

#[test]
fn what_the_external_agent_changed_is_in_the_journal_and_says_where_it_came_from() {
    // `Marcado-de-Origen.md`: what the machine did on its own account and what it
    // did on somebody else's must not be the same colour in the record. Without
    // this, a person looking at a workspace after a session has no way to tell
    // which changes were the agent's.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();
    let _ = wire.ask("make_file", &["from-the-agent.txt"]);
    let _ = wire.ask("read", &["/etc/passwd"]);

    let journal = bridge._home.path().join("store/journal.jsonl");
    let text = std::fs::read_to_string(&journal)
        .unwrap_or_else(|error| panic!("no journal at {}: {error}", journal.display()));
    let entries: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|entry: &serde_json::Value| {
            entry["operation"] == serde_json::json!("external_agent")
        })
        .collect();

    assert_eq!(
        entries.len(),
        2,
        "the change and the refused escape should both be recorded:\n{text}"
    );
    for entry in &entries {
        assert_eq!(entry["origin"], serde_json::json!("untrusted_content"));
    }
    // The refused escape is there as a rejection, which is the entry whose
    // absence would matter most.
    assert!(
        entries
            .iter()
            .any(|entry| entry["outcome"].get("rejected").is_some()),
        "a refused escape left no trace:\n{text}"
    );
    // And a read that *succeeded* is not there. A journal holding every question
    // a reading agent asks is a journal nobody reads — which is the same as no
    // journal, at the moment somebody is looking for what an agent did.
    let mut wire = wire;
    let _ = wire.ask("read", &["src/main.rs"]);
    let after = std::fs::read_to_string(&journal).expect("the journal");
    assert_eq!(
        after.lines().count(),
        text.lines().count(),
        "a read that changed nothing was journalled:\n{after}"
    );
}

#[test]
fn one_substitution_through_the_bridge_changes_every_file_it_named() {
    // What the whole delivery is for, driven the way the host drives it. The
    // fixture's two files are in two directories and the name is in both, which
    // is the shape `dev/bench-external-agent.sh --task reversible` measures.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();

    let answer = wire.ask(
        "edit",
        &[
            "src/main.rs",
            "substitute",
            "greet",
            "salute",
            "src/greeting.rs",
        ],
    );
    let said = answered(&answer);
    assert_eq!(said["ok"], serde_json::json!(true), "{said}");
    assert_eq!(said["did"], serde_json::json!("substituted"));
    assert_eq!(said["files"], serde_json::json!(2));
    assert_eq!(said["replacements"], serde_json::json!(4));

    // Read with something that is not Thalyx — and read as the demonstration
    // that this matches text and not symbols. `greet` is inside `greeting`, so
    // the module name moved too. That is what was asked for, and it is why the
    // tool description sends a caller to `thalyx_symbol` first.
    let main = std::fs::read_to_string(bridge.workspace.join("src/main.rs")).unwrap();
    assert!(main.contains("mod saluteing;"), "{main}");
    assert!(main.contains("saluteing::salute("), "{main}");
    assert!(
        std::fs::read_to_string(bridge.workspace.join("src/greeting.rs"))
            .unwrap()
            .contains("pub fn salute(")
    );
}

#[test]
fn a_second_file_outside_the_workspace_is_refused_the_way_the_first_one_is() {
    // The repeating slot is guarded exactly as the leading one. Without this
    // the operation would have added a way to name a file outside the workspace
    // that the boundary had never been asked about — every path after the first.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();
    let outside = bridge
        .workspace
        .parent()
        .unwrap()
        .join("secret.txt")
        .display()
        .to_string();

    let answer = wire.ask(
        "edit",
        &["src/main.rs", "substitute", "greeting", "x", &outside],
    );
    assert_eq!(refused(&answer), "outside_workspace");
    // And the file it *was* allowed to touch is untouched, because the refusal
    // happened before the verb ran at all.
    let main = std::fs::read_to_string(bridge.workspace.join("src/main.rs")).unwrap();
    assert!(main.contains("mod greeting;"), "{main}");
    assert_eq!(
        std::fs::read_to_string(bridge.workspace.parent().unwrap().join("secret.txt")).unwrap(),
        "not the agent's\n"
    );
}

#[test]
fn a_relative_path_that_climbs_out_is_refused_wherever_it_sits_in_the_call() {
    let bridge = Bridge::open();
    let mut wire = bridge.connect();
    let answer = wire.ask(
        "edit",
        &[
            "src/main.rs",
            "substitute",
            "greeting",
            "x",
            "../secret.txt",
        ],
    );
    assert_eq!(refused(&answer), "outside_workspace");
}

#[test]
fn texts_with_spaces_in_them_arrive_as_two_arguments_and_not_as_four() {
    // The composition, checked where it is actually composed. `editar` puts the
    // rest of its line on unquoted for the line subverbs — which is what a body
    // with leading spaces needs — and this subverb is the exception. If it were
    // not, `fn greet` would arrive as `fn` and `greet` and the machine would
    // substitute the wrong thing without anybody being told.
    let bridge = Bridge::open();
    let mut wire = bridge.connect();

    let answer = wire.ask(
        "edit",
        &[
            "src/greeting.rs",
            "substitute",
            "pub fn greet",
            "pub fn say_hello",
        ],
    );
    let said = answered(&answer);
    assert_eq!(said["ok"], serde_json::json!(true), "{said}");
    assert_eq!(said["replacements"], serde_json::json!(1));
    assert!(
        std::fs::read_to_string(bridge.workspace.join("src/greeting.rs"))
            .unwrap()
            .contains("pub fn say_hello(who: &str)")
    );
}

#[test]
fn a_substitution_the_bridge_refuses_leaves_every_file_it_named_alone() {
    let bridge = Bridge::open();
    let mut wire = bridge.connect();
    let before = std::fs::read_to_string(bridge.workspace.join("src/main.rs")).unwrap();

    // `src/greeting.rs` has no `mod ` in it, so the whole call has to refuse.
    let answer = wire.ask(
        "edit",
        &[
            "src/main.rs",
            "substitute",
            "mod ",
            "module ",
            "src/greeting.rs",
        ],
    );
    let said = answered(&answer);
    assert_eq!(said["ok"], serde_json::json!(false), "{said}");
    assert_eq!(said["error"], serde_json::json!("no_occurrences"));
    assert_eq!(said["wrote"], serde_json::json!(false));
    assert_eq!(
        std::fs::read_to_string(bridge.workspace.join("src/main.rs")).unwrap(),
        before
    );
}
