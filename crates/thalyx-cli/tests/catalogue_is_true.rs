//! The catalogue describes the machine that exists, not one that was intended.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **A1**: an agent
//! that arrives asks the system what it can do instead of being told. That is
//! only worth anything if the answer is true, and a table of verbs is exactly
//! the kind of thing that goes quietly out of date — the list used to live in
//! three places and one of them had already fallen behind.
//!
//! So none of these tests compares the catalogue to another list. Every one of
//! them **runs the session** and asks it.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

fn piped(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session");

    let mut typed = String::new();
    for line in lines {
        typed.push_str(line);
        typed.push('\n');
    }
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(typed.as_bytes())
        .expect("feeding the session");

    child.wait_with_output().expect("waiting for the session")
}

/// How long the session may take to answer every verb before it counts as stuck.
///
/// Generous, because this is not a measurement of speed. It exists because on
/// 2026-08-10 this test stopped answering on Cesar's machine and the only thing
/// anybody could say afterwards was *«has been running for over 60 seconds»* —
/// he killed the run, and the whole verification had to be thrown away without
/// learning which verb it was on. A hang that names nothing is the most
/// expensive kind of failure there is: it costs a run and teaches nothing.
const PATIENCE: Duration = Duration::from_secs(180);

/// Everything the session said, or — if it stopped answering — everything it had
/// said by then.
///
/// The output is drained on its own thread rather than after the wait. A session
/// that filled its stdout pipe while this side slept in the poll loop would
/// block in `write`, and the hang under investigation would be one this harness
/// had caused. Rule 5: the instrument includes the harness.
fn piped_within(root: &Path, lines: &[&str], patience: Duration) -> Result<String, String> {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session");

    let collected = Arc::new(Mutex::new(Vec::new()));
    let mut draining = Vec::new();
    for stream in [
        Box::new(child.stdout.take().expect("stdout")) as Box<dyn std::io::Read + Send>,
        Box::new(child.stderr.take().expect("stderr")),
    ] {
        let into = Arc::clone(&collected);
        draining.push(std::thread::spawn(move || {
            let mut stream = stream;
            let mut chunk = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut stream, &mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => into
                        .lock()
                        .expect("the collected output")
                        .extend_from_slice(&chunk[..read]),
                }
            }
        }));
    }

    let mut typed = String::new();
    for line in lines {
        typed.push_str(line);
        typed.push('\n');
    }
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(typed.as_bytes())
        .expect("feeding the session");

    let deadline = Instant::now() + patience;
    let finished = loop {
        match child.try_wait().expect("waiting for the session") {
            Some(_) => break true,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    for thread in draining {
        let _ = thread.join();
    }

    let said =
        String::from_utf8_lossy(&collected.lock().expect("the collected output")).replace('\r', "");
    if finished { Ok(said) } else { Err(said) }
}

/// Which of the typed lines the session never came back from.
///
/// The session prints its prompt before reading each line, so the number of
/// prompts in what it managed to say is the number of lines it got as far as —
/// and the last of those is the one it is still inside. Off by one here would
/// name the neighbour of the guilty verb and send somebody to read the wrong
/// code, which is why it is a function with a test rather than an expression
/// inside a `panic!` that only ever runs on a machine nobody can attach to.
fn stuck_on(partial: &str, typed: &[String]) -> String {
    let reached = partial.matches(" > ").count();
    reached
        .checked_sub(1)
        .and_then(|index| typed.get(index))
        .cloned()
        .unwrap_or_else(|| "the banner, before any verb was read".to_string())
}

fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

fn said(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace('\r', "")
}

fn objects(output: &Output) -> Vec<serde_json::Value> {
    said(output)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(serde_json::Value::is_object)
        .collect()
}

/// The catalogue as the running binary reports it — never the table linked into
/// the test, which would let both drift together.
fn catalogue(root: &Path) -> Vec<serde_json::Value> {
    let output = piped(root, &["structured on", "describe", "salir"]);
    let answer = objects(&output)
        .into_iter()
        .find(|value| value["op"] == serde_json::json!("describe"))
        .expect("the machine did not describe itself");
    answer["verbs"].as_array().expect("verbs").clone()
}

fn a_machine() -> tempfile::TempDir {
    tempfile::tempdir().expect("a store")
}

#[test]
fn the_machine_describes_itself_to_something_that_asks() {
    let root = a_machine();
    let verbs = catalogue(root.path());

    assert!(verbs.len() > 20, "only {} verbs", verbs.len());
    for verb in &verbs {
        // Everything a caller needs to use a verb without having seen one used.
        for field in [
            "id", "names", "takes", "flags", "changes", "errors", "summary",
        ] {
            assert!(
                verb.get(field).is_some(),
                "a verb is missing `{field}`: {verb}"
            );
        }
        // `answers` is allowed to be null and is not allowed to be absent: "this
        // one only speaks prose" is a fact a caller needs before it parses.
        assert!(verb.get("answers").is_some(), "no `answers` on {verb}");
    }
}

#[test]
fn every_verb_the_catalogue_advertises_is_understood_at_the_prompt() {
    let root = a_machine();
    let verbs = catalogue(root.path());

    // Typed bare, one per line, in one session. Anything the session does not
    // recognise falls through to the "I have no model loaded" paragraph, which
    // is how five verbs were found to not exist on 2026-08-09 — they had been
    // built, and every arm required a trailing space.
    let mut typed: Vec<String> = Vec::new();
    let mut expected: Vec<String> = Vec::new();
    for verb in &verbs {
        for name in verb["names"].as_array().expect("names") {
            let name = name.as_str().expect("a name").to_string();
            // Not these two: one ends the session and the other turns the
            // machine off, and both would stop the run before it finished.
            if ["salir", "exit", "quit", "apagar", "poweroff"].contains(&name.as_str()) {
                continue;
            }
            expected.push(name.clone());
            typed.push(name);
        }
    }
    typed.push("salir".to_string());

    let lines: Vec<&str> = typed.iter().map(String::as_str).collect();
    let text = match piped_within(root.path(), &lines, PATIENCE) {
        Ok(text) => text,
        Err(partial) => panic!(
            "the session stopped answering for {}s. It never came back from \
             `{}`, of {} verbs.\nWhat it had said by then:\n{}",
            PATIENCE.as_secs(),
            stuck_on(&partial, &expected),
            expected.len(),
            last_lines(&partial, 15),
        ),
    };

    let fell_through = text.matches("I have no model loaded").count();
    assert_eq!(
        fell_through,
        0,
        "{fell_through} of {} advertised verbs are not verbs:\n{text}",
        expected.len()
    );
}

#[test]
fn a_session_that_stops_answering_is_killed_rather_than_waited_for_forever() {
    // The watchdog above only ever runs on a machine this container is not, so
    // the one thing that can be checked here is that its impatient path works
    // at all: that it kills the session, comes back, and comes back with
    // whatever had been said rather than with nothing.
    //
    // A patience of zero makes the first poll the deadline, so this is
    // deterministic and does not sleep.
    let root = a_machine();
    let outcome = piped_within(root.path(), &["salir"], Duration::from_secs(0));

    assert!(
        outcome.is_err(),
        "a patience of zero let the session finish: {outcome:?}"
    );
}

#[test]
fn the_verb_named_in_a_hang_is_the_one_the_session_was_inside() {
    // The arithmetic, on its own, because the failure it belongs to cannot be
    // produced on demand. One prompt means the session had read the first line
    // and was inside it — not that it had finished it, and not that it was
    // waiting for the second.
    let typed: Vec<String> = ["ls", "cat", "pwd", "cambios"]
        .iter()
        .map(|word| word.to_string())
        .collect();

    assert_eq!(stuck_on("  /home > ", &typed), "ls");
    assert_eq!(stuck_on("  /home > out\n  /home > ", &typed), "cat");
    assert_eq!(
        stuck_on("  a > \n  a > \n  a > \n  a > ", &typed),
        "cambios"
    );
    // Nothing said yet is a different fact from a verb, and it is said as one:
    // a session that hangs before its first prompt is not hanging on a verb.
    assert_eq!(stuck_on("", &typed), "the banner, before any verb was read");
}

#[test]
fn every_verb_the_catalogue_advertises_is_in_the_banner() {
    let root = a_machine();
    let verbs = catalogue(root.path());

    // The banner is the only place a person at the machine can learn a verb —
    // there is no shell behind it and no `man`. A verb that exists and is not
    // named there does not exist for whoever is sitting in front of it, and
    // that has happened before.
    let banner = said(&piped(root.path(), &["salir"]));

    let mut missing = Vec::new();
    for verb in &verbs {
        let first = verb["names"][0].as_str().expect("a name");
        if !banner.contains(first) {
            missing.push(first.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "these exist and the banner never mentions them: {missing:?}"
    );
}

#[test]
fn a_verb_can_be_asked_about_by_name() {
    let root = a_machine();
    let output = piped(root.path(), &["structured on", "describe rm", "salir"]);
    let answer = objects(&output)
        .into_iter()
        .find(|value| value["op"] == serde_json::json!("describe"))
        .expect("nothing answered");

    assert_eq!(answer["count"], serde_json::json!(1));
    assert_eq!(answer["verbs"][0]["id"], serde_json::json!("remove"));
    assert_eq!(answer["verbs"][0]["changes"], serde_json::json!(true));
}

#[test]
fn asking_about_something_that_is_not_a_verb_says_so_rather_than_listing_everything() {
    let root = a_machine();
    let output = piped(root.path(), &["structured on", "describe volar", "salir"]);
    let answer = objects(&output)
        .into_iter()
        .find(|value| value["op"] == serde_json::json!("describe"))
        .expect("nothing answered");

    // Falling back to the whole catalogue would look like success and would send
    // a caller off believing `volar` is in the list somewhere.
    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("unknown_verb"));
}

#[test]
fn the_op_a_verb_says_it_answers_with_is_the_op_it_answers_with() {
    let root = a_machine();
    let verbs = catalogue(root.path());

    // Four with no arguments, so each can be typed bare and still answer.
    for (name, expected) in [("pwd", "where"), ("ls", "list"), ("describe", "describe")] {
        let output = piped(root.path(), &["structured on", name, "salir"]);
        let ops: Vec<String> = objects(&output)
            .iter()
            .filter_map(|value| value["op"].as_str().map(str::to_string))
            .collect();
        assert!(
            ops.contains(&expected.to_string()),
            "`{name}` was supposed to answer `{expected}` and answered {ops:?}"
        );
    }

    // And the claim the loop above is checking really is in the catalogue,
    // rather than being a constant this test invented.
    let listed = verbs
        .iter()
        .find(|verb| verb["id"] == serde_json::json!("list"))
        .expect("list");
    assert_eq!(listed["answers"], serde_json::json!("list"));
}
