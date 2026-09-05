//! rust-analyzer is not a program Thalyx runs. It is a program that runs
//! programs, and until 2026-08-31 nobody had told it where they are.
//!
//! ## What the machine actually did
//!
//! A booted Thalyx, asked over its agent channel about `dev/rust-corpus`:
//!
//! ```text
//! context('lantern/src/lib.rs')  → { name: "LanternRegistry", kind: "struct",
//!                                    crate: "lantern", source: "rust-analyzer" }
//! context('LanternRegistry')     → { source: "rust-analyzer",
//!                                    resolution: "nothing", entries: [] }
//! rename at lantern/src/lib.rs:8:12
//!                                → "rust-analyzer refused: No references
//!                                   found at position"
//! ```
//!
//! rust-analyzer was alive, had opened the file and had parsed it — and could
//! not resolve a name declared in the file it had just outlined. The reason is
//! in its own log, reproduced under the environment Thalyx hands over:
//!
//! ```text
//! ERROR FetchWorkspaceError: rust-analyzer failed to load workspace:
//!   Failed to run `cargo metadata …`: No such file or directory (os error 2)
//! WARN  failed to get rustc cfgs e=unable to fetch cfgs via `… "rustc" …`
//!   Caused by: No such file or directory (os error 2)
//! ```
//!
//! It spells its subprocesses as bare program names, the kernel resolves those
//! through `PATH`, and Thalyx — which finds every tool by absolute path and is
//! right to — had never given its children one. Syntax survives that because
//! syntax needs no subprocess. Everything else is the crate graph, and there
//! was none.
//!
//! ## Why every test here empties the environment first
//!
//! `cargo test` runs with a `PATH` that has a Rust toolchain on it, so a check
//! that inherited the caller's environment would pass on this machine for a
//! reason that does not exist inside Thalyx. Rule 8: a fake must model the
//! property under test, and the property is *what a process is given*.
//! [`WithNothingButWhatThalyxHandsOver`] clears everything and applies exactly
//! the pairs [`thalyx_rust::toolchain::carried_by`] builds — which is the
//! guest's own environment, assembled by the guest's own function, on any
//! machine.
//!
//! ## And why there is a control
//!
//! Rule 4. Without the same fixture answered by an environment with the `PATH`
//! taken back out, "rust-analyzer resolves the symbol" is an assertion about a
//! machine rather than a comparison, and it would go on passing on the day
//! something else started supplying the toolchain.

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use thalyx_know::Knowledge;
use thalyx_rust::analyzer::{Launching, Spawn, Started};
use thalyx_rust::{Provider, Resolution};

/// A spawner that gives the server the guest's environment and nothing else.
///
/// `env_clear` is the whole test. Inside Thalyx there is no login shell, no
/// profile and no distribution `PATH`; a provider gets what Thalyx hands it,
/// and this is the only way to ask what that is worth without a booted
/// machine.
struct WithNothingButWhatThalyxHandsOver;

impl Spawn for WithNothingButWhatThalyxHandsOver {
    fn start(&self, asked: Launching<'_>) -> thalyx_rust::Result<Started> {
        let mut command = Command::new(asked.program);
        command.env_clear();
        if let Some(target) = asked.build_into {
            command.env("CARGO_TARGET_DIR", target);
        }
        for (name, value) in asked.environment {
            command.env(name, value);
        }
        let child = command
            .current_dir(asked.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                thalyx_rust::RustError::NoAnalyzer(format!("{}: {error}", asked.program.display()))
            })?;
        Ok(Started {
            child,
            release: None,
            how: "an empty environment".to_string(),
            confined: false,
        })
    }
}

/// The pairs a machine whose Rust is Thalyx's own hands its children,
/// assembled by the function that assembles them.
///
/// Built with [`thalyx_rust::toolchain::carried_by`] over whatever toolchain
/// this machine has, laid out the way the artifact is: `bin` beside `lib`,
/// which is rustup's layout as well as the runtime's. So these run on a
/// developer's laptop and inside Thalyx and produce the same shape in both.
///
/// The `CARGO_HOME` is an empty directory the caller owns, and that is not
/// tidiness. rust-analyzer looks for `cargo` in three places — `$CARGO`,
/// `PATH`, and `$CARGO_HOME/bin` — and on a rustup machine the third one is
/// full. A control that only took `PATH` away therefore *resolved the symbol
/// anyway*, and the first version of this file asserted a defect that its own
/// environment had repaired. Rule 5 again, and rule 8: on the store,
/// `CARGO_HOME` is `<store>/state/cargo` and has no `bin` in it, so a fake
/// that models the guest has none either.
fn handed_over(cargo_home: &Path) -> Vec<(String, String)> {
    let cargo = thalyx_rust::toolchain::cargo()
        .path
        .clone()
        .expect("a cargo, which `cargo_or_skip` has already established");
    let root = cargo
        .parent()
        .and_then(Path::parent)
        .expect("a toolchain laid out as bin beside lib");
    let runtime = thalyx_rust::runtime::Runtime {
        root: root.to_path_buf(),
        identity: "the toolchain this machine has".to_string(),
        rust: None,
        musl: None,
    };
    std::fs::create_dir_all(cargo_home).expect("a cargo home with nothing in it");
    thalyx_rust::toolchain::carried_by(&runtime, cargo_home)
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

/// A provider over the corpus, started with exactly these pairs and no others.
fn provider(root: &Path, target: &Path, environment: Vec<(String, String)>) -> Provider {
    Provider::open(root, Knowledge::in_memory().expect("a knowledge store"))
        .spawning(std::sync::Arc::new(WithNothingButWhatThalyxHandsOver))
        .building_into(target)
        .reaching(thalyx_rust::toolchain::readable(), environment)
}

fn lib_of(root: &Path) -> PathBuf {
    root.join("lantern").join("src").join("lib.rs")
}

#[test]
fn a_provider_with_no_path_parses_the_file_and_cannot_resolve_a_name_in_it() {
    // The control, and the reproduction. It asserts the *defect*: with the one
    // variable taken back out, the machine does exactly what the booted one
    // did — the outline is right and the resolution is empty.
    if !support::analyzer_or_skip("that a workspace does not load without a PATH") {
        return;
    }
    if !support::unconfined_or_skip("that a workspace does not load without a PATH") {
        return;
    }
    let (_held, root) = support::corpus();
    let target = tempfile::tempdir().expect("somewhere to build");

    let blinded: Vec<(String, String)> = handed_over(&target.path().join("cargo-home"))
        .into_iter()
        .filter(|(name, _)| name != thalyx_rust::toolchain::SEARCH_PATH_VARIABLE)
        .collect();
    let mut provider = provider(&root, target.path(), blinded);

    // Syntax. It works, and that it works is the thing that made the defect so
    // hard to see: the machine answered a question about the file correctly.
    let outline = provider.outline(&lib_of(&root)).expect("an outline");
    assert!(
        outline.iter().any(|entry| entry.name == "LanternRegistry"),
        "rust-analyzer could not even parse the file, so this run is not \
         measuring what it thinks it is: {outline:?}"
    );

    // Semantics. There is no crate graph, so there is nothing to resolve
    // against.
    let (resolution, _, source) = provider.known("LanternRegistry").expect("an answer");
    assert_eq!(source, "rust-analyzer");
    assert_eq!(
        resolution,
        Resolution::Nothing,
        "a server with no toolchain on its PATH resolved a name. If that is \
         real, this control is measuring an environment that is no longer \
         empty — check what `handed_over` returned before believing it"
    );

    // And the rename, at the identifier's own physical position — the exact
    // question the booted machine was asked when `documentSymbol` was ruled
    // out as the cause.
    let refused = provider.rename_plan(&lib_of(&root), 8, 12, "BeaconRegistry");
    assert!(
        refused.is_err(),
        "a rename resolved with no crate graph: {refused:?}"
    );
}

#[test]
fn the_path_thalyx_builds_is_what_makes_a_name_resolve() {
    if !support::analyzer_or_skip("that the toolchain's own PATH resolves a name") {
        return;
    }
    if !support::unconfined_or_skip("that the toolchain's own PATH resolves a name") {
        return;
    }
    let (_held, root) = support::corpus();
    let target = tempfile::tempdir().expect("somewhere to build");
    let handed = handed_over(&target.path().join("cargo-home"));
    assert!(
        handed
            .iter()
            .any(|(name, _)| name == thalyx_rust::toolchain::SEARCH_PATH_VARIABLE),
        "nothing tells the toolchain's children where the toolchain is: {handed:?}"
    );
    // And the machine's own answer, whichever branch it takes. The pairs above
    // are the guest's shape; this is the wiring that has to carry it, and a
    // `carried_by` that grew a `PATH` while `environment` did not would leave
    // the booted machine exactly as broken as it was.
    assert!(
        thalyx_rust::toolchain::environment()
            .iter()
            .any(|(name, _)| *name == thalyx_rust::toolchain::SEARCH_PATH_VARIABLE),
        "this machine's own toolchain environment names no PATH, so nothing it \
         starts can run `cargo metadata`"
    );
    let mut provider = provider(&root, target.path(), handed);

    let (resolution, _, source) = provider.known("LanternRegistry").expect("an answer");
    assert_eq!(source, "rust-analyzer");
    let Resolution::One { known } = resolution else {
        panic!(
            "the corpus declares `LanternRegistry` exactly once and the machine \
             answered {resolution:?}"
        );
    };
    assert_eq!(known.kind, "struct");
    assert_eq!(
        known.package.as_deref(),
        Some("lantern"),
        "the package came from `cargo metadata`, so an answer without one is a \
         workspace that did not load: {known:?}"
    );
    assert_eq!(
        known.defined.len(),
        1,
        "one declaration was expected: {known:?}"
    );
    // 8:12, one-based — the identifier, not the doc comment three lines above
    // it that the flat outline used to report.
    assert_eq!(
        (
            known.defined[0].path.as_str(),
            known.defined[0].line,
            known.defined[0].column
        ),
        ("lantern/src/lib.rs", 8, 12),
        "the declaration is not where the file puts it: {:?}",
        known.defined[0]
    );
    // Uses in the other crate. A resolution that found the declaration and no
    // references would be a file that parsed rather than a workspace that
    // loaded.
    assert!(
        known
            .used
            .iter()
            .any(|used| used.path.starts_with("harbour/")),
        "nothing in the second crate refers to it, so the crate graph has one \
         crate in it: {:?}",
        known.used
    );
}

#[test]
fn a_rename_crosses_the_crate_that_only_the_compiler_knows_about() {
    // The claim the machine makes and the index cannot: `harbour` mentions the
    // name in a `use`, in a return type, in a call, in a reference and behind
    // a type alias, and every one of them moves. `documentSymbol found the
    // struct` is not this, which is the whole reason the verifier had to stop
    // accepting it.
    if !support::analyzer_or_skip("that a rename really crosses files") {
        return;
    }
    if !support::unconfined_or_skip("that a rename really crosses files") {
        return;
    }
    let (_held, root) = support::corpus();
    let target = tempfile::tempdir().expect("somewhere to build");
    let mut provider = provider(
        &root,
        target.path(),
        handed_over(&target.path().join("cargo-home")),
    );

    let plan = provider
        .rename_plan(&lib_of(&root), 8, 12, "BeaconRegistry")
        .expect("a rename rust-analyzer resolved");
    let mut touched: Vec<String> = plan
        .iter()
        .map(|change| {
            change
                .path
                .strip_prefix(&root)
                .unwrap_or(&change.path)
                .display()
                .to_string()
        })
        .collect();
    touched.sort();
    assert_eq!(
        touched,
        vec![
            "harbour/src/lib.rs".to_string(),
            "lantern/src/lib.rs".to_string()
        ],
        "a rename that stayed in one file is a rename that resolved nothing"
    );
    let crossing = plan
        .iter()
        .find(|change| change.path.ends_with("harbour/src/lib.rs"))
        .expect("the other crate");
    assert!(
        crossing.edits.len() >= 4,
        "the second crate mentions the name five times and {} moved: {:?}",
        crossing.edits.len(),
        crossing.edits
    );

    // What the files would say. Nothing is written — this crate describes and
    // the authority above it decides — but the text is the thing a caller
    // applies, so a plan that produced text nobody checked is a plan.
    let texts = provider
        .rename_texts(&lib_of(&root), 8, 12, "BeaconRegistry")
        .expect("the text after");
    for written in &texts {
        assert!(
            written.text.contains("BeaconRegistry"),
            "{} came back without the new name",
            written.path.display()
        );
        assert!(
            !written.text.contains("LanternRegistry"),
            "{} still carries the old name, so the rename is partial and \
             compiles nowhere",
            written.path.display()
        );
    }
}

#[test]
fn the_toolchains_own_cargo_finds_the_toolchains_own_rustc() {
    // The chain the whole change is about, one link further down than any
    // other test reaches: `cargo --version` proves nothing, because that is
    // the binary Thalyx already found by absolute path. This runs `cargo` as a
    // **bare program name** with an empty environment, so it can only be
    // resolved through the `PATH` Thalyx built — and then makes it compile,
    // which it can only do by finding `rustc` the same way.
    //
    // A library workspace on purpose: an rlib needs no linker, so this asks
    // about the toolchain rather than about whether the machine has a `cc`.
    if !support::cargo_or_skip("that the toolchain's cargo finds its rustc") {
        return;
    }
    let (_held, root) = support::corpus();
    let target = tempfile::tempdir().expect("somewhere to build");

    let mut command = Command::new("cargo");
    command.env_clear();
    for (name, value) in handed_over(&target.path().join("cargo-home")) {
        command.env(name, value);
    }
    command.env("CARGO_TARGET_DIR", target.path());
    let built = command
        .arg("build")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("cargo could not be started by name at all — the PATH names no cargo");
    assert!(
        built.status.success(),
        "the staged cargo could not build the corpus with only what Thalyx \
         hands over:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(
        target.path().join("debug").exists(),
        "cargo reported success and built nothing, so it was not the compiler \
         that answered"
    );
}
