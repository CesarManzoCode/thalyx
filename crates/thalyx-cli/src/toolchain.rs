//! Can this machine actually resolve a Rust name, asked before anybody pays to
//! find out.
//!
//! ## The failure this file exists to stop
//!
//! On 2026-08-30 the benchmark's preflight said `READY` and the run was paid
//! for. Inside the machine, the first thing the agent did was the right thing
//! — ask what a symbol is, then rename it — and the machine answered
//! `source: index`, `analyzer_starts: 0`, and
//!
//! ```text
//! rename: { ok: false, error: unresolved,
//!           message: "there is no `cargo` on this machine" }
//! ```
//!
//! The preflight had checked that the machine was **alive** and holding the
//! **right tree**. Both were true. Neither is the capability the task needed,
//! and there was nothing between the money and finding that out.
//!
//! So this verb answers the question the preflight could not ask: *is there a
//! compiler here, and did it run*. It is free, it is read-only, and it is the
//! same code path `context` and `rename` use to find their tools — a probe
//! written against a copy of that search would prove the copy.
//!
//! ## What it does not do
//!
//! It does not rename anything, write anything, or start rust-analyzer. The
//!2026-08-29 lesson was a probe that changed the starting state of the run it
//! was clearing, and `cargo metadata --no-deps --offline` — the heaviest thing
//! here — reads manifests and resolves nothing, so it writes no `Cargo.lock`.
//!
//! ## Why running `--version` is the whole point
//!
//! `~/.cargo/bin/rust-analyzer` exists on every rustup install and answers
//! `error: Unknown binary`. A staged runtime whose loader is missing is a
//! directory full of perfectly good ELF files that cannot start. In both cases
//! the file is there and the program is not, and only one of those two facts is
//! the one a benchmark needs. [`thalyx_rust::toolchain`] has had that rule
//! since it was written: a candidate becomes the answer after it answers
//! `--version`, so a path coming back from it *is* the evidence that something
//! ran.

use crate::files::{Face, Where};
use serde_json::{Value, json};
use std::path::Path;
use thalyx_rust::toolchain::{Found, Kind};

type Fallible = Result<(), Box<dyn std::error::Error>>;

pub const OP: &str = "toolchain";

/// What one tool answered when it was asked what it is.
///
/// Run a second time rather than remembered from the search, because the
/// search only kept *whether* the exit status was zero. The string is what
/// makes an answer worth reading: "cargo 1.90.0" and "cargo is somewhere" are
/// different amounts of knowing.
fn version_of(path: &Path) -> Option<String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let said = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!said.is_empty()).then_some(said)
}

/// One tool, as the answer carries it.
fn tool(found: &Found) -> Value {
    match &found.path {
        Some(path) => json!({
            "path": path.display().to_string(),
            // `thalyx` when it is the runtime on the store, `host` when it is
            // one somebody installed, `named` when a variable said so. The
            // distinction is the decree of 2026-08-31 and it is the field
            // worth reading first.
            "from": found.kind.map(Kind::as_str).unwrap_or("unknown"),
            "version": version_of(path),
        }),
        None => json!({
            "path": Value::Null,
            "from": Value::Null,
            // Where it looked, so a refusal is something a person can act on
            // rather than a sentence they have to believe.
            "looked_at": found
                .looked_at
                .iter()
                .map(|path| json!(path.display().to_string()))
                .collect::<Vec<Value>>(),
        }),
    }
}

/// The answer, and whether the machine can do semantics at all.
pub fn report(here: &Where) -> (Value, bool, Vec<String>) {
    let cargo = thalyx_rust::toolchain::cargo();
    let analyzer = thalyx_rust::toolchain::rust_analyzer();
    let tree = crate::semantic::tree_of(here);

    let mut because: Vec<String> = Vec::new();
    if cargo.path.is_none() {
        because.push(cargo.why_not(
            "cargo that runs",
            "A machine meant to program Rust is prepared with \
             `make -C image agent PROJECT=… RUST=1`.",
        ));
    }
    if analyzer.path.is_none() {
        because.push(analyzer.why_not(
            "rust-analyzer that runs",
            "Without it every answer comes from the scan, which matches names \
             rather than resolving them.",
        ));
    }

    let runtime = thalyx_rust::toolchain::managed_runtime();
    let is_rust = tree.join("Cargo.toml").is_file();

    // Read-only and cheap: `--no-deps` resolves nothing, so no lockfile is
    // written and the starting state of whatever runs next is untouched. It is
    // the difference between "a cargo exists" and "a cargo can read *this*
    // workspace", which is the question a run about this workspace has.
    let workspace = if is_rust && cargo.path.is_some() {
        match thalyx_rust::Workspace::read(&tree) {
            Ok(workspace) => json!({
                "rust": true,
                "root": workspace.root.display().to_string(),
                "packages": workspace.packages.len(),
            }),
            Err(why) => {
                because.push(format!("cargo could not read this workspace: {why}"));
                json!({"rust": true, "root": tree.display().to_string(), "read": false})
            }
        }
    } else {
        json!({"rust": is_rust, "root": tree.display().to_string()})
    };

    let ready = because.is_empty() && is_rust;
    if !is_rust {
        // Not a fault. A machine holding a tree that is not a Cargo workspace
        // has nothing to be ready *for*, and saying so is different from
        // saying the toolchain is missing. Rule 10.
        because.push(format!(
            "{} is not a Cargo workspace, so there is nothing here for a Rust \
             semantic provider to answer about",
            tree.display()
        ));
    }

    let answer = json!({
        "cargo": tool(cargo),
        "rust_analyzer": tool(analyzer),
        "runtime": match &runtime {
            Some(runtime) => json!({
                "identity": runtime.identity,
                "rust": runtime.rust,
                "musl": runtime.musl,
                "root": runtime.root.display().to_string(),
            }),
            None => Value::Null,
        },
        "workspace": workspace,
        "semantic_ready": ready,
        "because": because.clone(),
    });
    (answer, ready, because)
}

/// The verb.
pub fn act(here: &Where, face: Face) -> Fallible {
    let (answer, ready, because) = report(here);

    if face.is_machine() {
        let carried: Vec<(&'static str, Value)> = answer
            .as_object()
            .map(|fields| {
                fields
                    .iter()
                    .map(|(name, value)| (leak(name), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        face.say(thalyx_files::machine::answer(OP, carried));
        return Ok(());
    }

    println!();
    for (name, value) in [
        ("cargo", &answer["cargo"]),
        ("rust-analyzer", &answer["rust_analyzer"]),
    ] {
        match value["path"].as_str() {
            Some(path) => println!(
                "  {name:<15}{path}\n  {:<15}{} ({})",
                "",
                value["version"].as_str().unwrap_or("no version"),
                value["from"].as_str().unwrap_or("unknown"),
            ),
            None => println!("  {name:<15}none of the places this machine looks holds one"),
        }
    }
    if let Some(identity) = answer["runtime"]["identity"].as_str() {
        println!("  {:<15}{identity}", "runtime");
    }
    println!(
        "  {:<15}{}",
        "semantic",
        if ready {
            "ready — names can be resolved here"
        } else {
            "NOT ready"
        }
    );
    for line in &because {
        println!("      {line}");
    }
    println!();
    Ok(())
}

/// The field names of a fixed object, as the `'static` strings the machine
/// face takes.
///
/// The set is closed and written three screens above; leaking it is bounded by
/// the number of keys in that literal, not by anything a caller controls.
fn leak(name: &str) -> &'static str {
    match name {
        "cargo" => "cargo",
        "rust_analyzer" => "rust_analyzer",
        "runtime" => "runtime",
        "workspace" => "workspace",
        "semantic_ready" => "semantic_ready",
        "because" => "because",
        _ => "extra",
    }
}
