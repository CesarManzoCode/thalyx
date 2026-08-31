//! Before anybody pays for a run, the machine is asked whether it has a
//! compiler — and the answer has to be a thing a program can read.
//!
//! ## The failure this file exists to stop
//!
//! `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md`, 2026-08-30. The
//! benchmark's preflight said `READY`, the run was paid for, and the first
//! thing the agent did inside the machine came back
//!
//! ```text
//! rename: { ok: false, error: unresolved,
//!           message: "there is no `cargo` on this machine" }
//! ```
//!
//! Everything the preflight had checked was true: the machine answered, and it
//! was holding the right tree. Neither is the capability a Rust task needs, and
//! there was nothing between the money and finding that out.
//!
//! `thalyx-mcp --preflight --needs-rust` now asks this verb and refuses to be
//! READY when the answer is no. That refusal is only worth anything if the
//! fields it reads exist, in the machine face, with those names — which is what
//! this test is. The decision itself is tested separately and for free in
//! `dev/bench-summary.py --self-test`, against a machine with a compiler, one
//! without, and one too old to be asked.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// Type at the prompt down a plain pipe, which is how a program drives Thalyx.
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

fn answer(output: &Output, op: &str) -> serde_json::Value {
    String::from_utf8_lossy(&output.stdout)
        .replace('\r', "")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .find(|value| value["op"] == serde_json::json!(op))
        .unwrap_or_else(|| {
            panic!(
                "nothing answered `{op}`:\n{}",
                String::from_utf8_lossy(&output.stdout)
            )
        })
}

#[test]
fn the_machine_answers_whether_it_can_resolve_a_rust_name() {
    let root = tempfile::tempdir().expect("a store");
    let said = piped(root.path(), &["structured on", "toolchain", "salir"]);
    let answer = answer(&said, "toolchain");

    // Every field the preflight reads, present whatever the machine turned out
    // to have. An absent key and a false one are the same shape and different
    // facts, and a harness that had to tell them apart by `in` would be reading
    // the wrong thing on the day it mattered. Rule 10.
    for field in [
        "cargo",
        "rust_analyzer",
        "runtime",
        "workspace",
        "semantic_ready",
        "because",
    ] {
        assert!(
            answer.get(field).is_some(),
            "the answer has no `{field}`, which the preflight reads:\n{answer:#}"
        );
    }
    assert!(
        answer["semantic_ready"].is_boolean(),
        "`semantic_ready` has to be a yes or a no, not {:?}",
        answer["semantic_ready"]
    );
    assert!(
        answer["because"].is_array(),
        "`because` has to be a list, so a refusal can be printed line by line"
    );

    // Whichever way this machine went, the answer says *why* and *whose*. A
    // machine that said `false` with an empty `because` would be the 2026-08-30
    // failure again with a different word on it.
    if answer["semantic_ready"] == serde_json::json!(false) {
        assert!(
            !answer["because"].as_array().expect("a list").is_empty(),
            "not ready, and it did not say why:\n{answer:#}"
        );
    } else {
        for tool in ["cargo", "rust_analyzer"] {
            assert!(
                answer[tool]["path"].is_string(),
                "ready, and {tool} has no path:\n{answer:#}"
            );
            assert!(
                answer[tool]["from"].is_string(),
                "ready, and nothing says whose {tool} it is — `thalyx`, `host` or \
                 `named` is the field that decides whether this machine was autonomous"
            );
        }
    }
}

#[test]
fn a_tool_that_is_not_there_is_reported_with_the_places_it_was_looked_for() {
    // "There is no rust-analyzer" is a sentence nobody can act on. The whole
    // reason `Found` carries `looked_at` is that "there is none at these four
    // paths" tells a person which home the search was in — which, under `sudo`,
    // is the entire problem.
    let root = tempfile::tempdir().expect("a store");
    let said = piped(root.path(), &["structured on", "toolchain", "salir"]);
    let answer = answer(&said, "toolchain");

    for tool in ["cargo", "rust_analyzer"] {
        if answer[tool]["path"].is_null() {
            let looked = answer[tool]["looked_at"]
                .as_array()
                .unwrap_or_else(|| panic!("{tool} is absent and did not say where it looked"));
            assert!(
                !looked.is_empty(),
                "{tool} is absent and the list of places is empty"
            );
            let why = answer["because"]
                .as_array()
                .expect("a list")
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<&str>>()
                .join(" ");
            assert!(
                why.contains("place(s) this machine looks"),
                "the reason does not name where it looked: {why}"
            );
            return;
        }
    }
    println!(
        "NOT PROVEN: this machine has both tools, so the shape of the refusal was not \
         exercised here. `crates/thalyx-rust/src/toolchain.rs` tests it directly."
    );
}
