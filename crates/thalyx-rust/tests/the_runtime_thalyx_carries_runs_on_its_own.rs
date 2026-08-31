//! The artifact is asked to be a machine's whole Rust toolchain, on a host that
//! is not the one that built it.
//!
//! `vault/09-Notas-Tecnicas/Runtime-Rust-Agente.md`. The failure these are
//! written against is not a crash: it is an artifact that works perfectly on
//! the machine that assembled it, because every library it forgot to carry was
//! sitting in that machine's `/usr/lib`. On the machine it is *for* — a Thalyx,
//! whose `/lib` is empty — the same artifact is a directory of ELF files that
//! cannot start, and what the human reads is `there is no cargo on this
//! machine`.
//!
//! ## Why these can run anywhere, including a host with no musl
//!
//! musl's `libc.so` **is** the dynamic loader, and a loader can be invoked
//! directly with the program as its argument. So
//! `<artifact>/lib/libc.so <artifact>/bin/cargo` runs the staged cargo using
//! the staged loader, on any x86_64 Linux, without the host having musl and
//! without anybody writing to `/lib`. Rule 11: a test that wrote a machine-wide
//! symlink would have changed the machine it was measuring.
//!
//! ## And why they skip loudly
//!
//! Building the artifact downloads about 170 MB and compiles a C library, so
//! it is not a thing every `cargo test` should do. Rule 3: a test that skips
//! says `NOT PROVEN`, and `THALYX_REQUIRE_RUST_RUNTIME=1` turns the skip into
//! a failure, so a machine that has one can demand that it was used.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the artifact is, if this machine has one.
///
/// `THALYX_RUST_RUNTIME` names it outright; otherwise the place
/// `make -C image rust-runtime` leaves it, which is where a developer who ran
/// the documented command will have it.
fn artifact() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("THALYX_RUST_RUNTIME") {
        let path = PathBuf::from(named);
        return path.is_dir().then_some(path);
    }
    let built = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../image/build/rust-runtime")
        .canonicalize()
        .ok()?;
    std::fs::read_dir(built)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| thalyx_rust::runtime::read(path).is_some())
}

/// The artifact, or a skip that says so in the words rule 3 requires.
fn artifact_or_skip(claim: &str) -> Option<PathBuf> {
    match artifact() {
        Some(path) => Some(path),
        None => {
            let demanded = std::env::var("THALYX_REQUIRE_RUST_RUNTIME").as_deref() == Ok("1");
            assert!(
                !demanded,
                "THALYX_REQUIRE_RUST_RUNTIME=1 and there is no artifact to test: {claim}. \
                 Build one with `make -C image rust-runtime`, or name one with \
                 THALYX_RUST_RUNTIME."
            );
            println!(
                "NOT PROVEN: {claim} — no Rust runtime artifact on this machine. \
                 `make -C image rust-runtime` builds one; THALYX_REQUIRE_RUST_RUNTIME=1 \
                 makes this a failure instead of a skip."
            );
            None
        }
    }
}

/// Run a program of the artifact through the artifact's own loader.
///
/// The environment is emptied first, and that is the whole test rather than
/// tidiness: an inherited `LD_LIBRARY_PATH`, `RUSTUP_HOME` or `PATH` is a way
/// for the host to help, and the claim is that it cannot. `HOME` is named at a
/// directory the caller controls because a Cargo with no home writes into the
/// one it finds.
fn through_its_own_loader(artifact: &Path, program: &str, arguments: &[&str]) -> Command {
    let mut command = Command::new(artifact.join("lib/libc.so"));
    command.arg(artifact.join(program));
    command.args(arguments);
    command.env_clear();
    command
}

#[test]
fn an_artifact_carries_the_loader_and_every_library_its_own_programs_name() {
    let Some(artifact) = artifact_or_skip("the artifact is closed") else {
        return;
    };
    let report = thalyx_rust::runtime::inspect(&artifact);
    assert!(
        report.missing.is_empty(),
        "the artifact is missing {:?}",
        report.missing
    );
    assert!(
        report.forbidden.is_empty(),
        "a whole toolchain was copied rather than a runtime assembled: {:?}",
        report.forbidden
    );
    assert!(
        report.other_targets.is_empty(),
        "it carries a standard library for a machine this is not: {:?}",
        report.other_targets
    );

    let closure = thalyx_rust::runtime::closure(&artifact);
    assert!(
        closure.unresolved.is_empty(),
        "these would resolve against whatever the host happens to have: {:?}",
        closure.unresolved
    );
    assert!(
        closure.interpreter_inside,
        "the artifact does not carry the loader its programs ask the kernel for: {:?}",
        closure.interpreters
    );
    assert!(
        closure.programs.len() >= 3,
        "an artifact with fewer than three programs is not a toolchain: {:?}",
        closure.programs
    );
}

#[test]
fn nothing_in_the_artifact_points_at_the_machine_that_built_it() {
    // The shape of the failure: a staging step that "worked" because it left
    // symlinks into `~/.rustup`. Every check passes on the machine that ran
    // it, and the store is useless the moment it is carried anywhere else —
    // which is the entire property this artifact exists to have.
    let Some(artifact) = artifact_or_skip("the artifact is self-contained") else {
        return;
    };

    fn walk(path: &Path, artifact: &Path, escaping: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let here = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_symlink() => {
                    let Ok(target) = std::fs::read_link(&here) else {
                        continue;
                    };
                    let resolved = if target.is_absolute() {
                        target.clone()
                    } else {
                        here.parent().unwrap_or(artifact).join(&target)
                    };
                    // Canonicalised, because `../..` inside the artifact is
                    // still inside the artifact and a string comparison would
                    // call it an escape.
                    let inside = resolved
                        .canonicalize()
                        .map(|path| path.starts_with(artifact))
                        .unwrap_or(false);
                    if !inside {
                        escaping.push(format!("{} → {}", here.display(), target.display()));
                    }
                }
                Ok(kind) if kind.is_dir() => walk(&here, artifact, escaping),
                _ => {}
            }
        }
    }

    let canonical = artifact.canonicalize().expect("the artifact's real path");
    let mut escaping = Vec::new();
    walk(&canonical, &canonical, &mut escaping);
    assert!(
        escaping.is_empty(),
        "the artifact links out of itself, so it is not a thing that can be carried: {escaping:?}"
    );

    // And what it says about itself names no home either. A `runtime.json`
    // carrying `/home/<somebody>` would mean the description was written from
    // the builder's paths rather than from the pins.
    let described = std::fs::read_to_string(canonical.join("runtime.json")).expect("runtime.json");
    for shape in ["/home/", "/root/", ".rustup", ".cargo"] {
        assert!(
            !described.contains(shape),
            "runtime.json mentions {shape}, so it was written from the builder's machine:\n{described}"
        );
    }
}

#[test]
fn the_staged_cargo_and_rust_analyzer_run_with_nothing_from_the_host() {
    let Some(artifact) = artifact_or_skip("the staged programs run") else {
        return;
    };
    for (program, expected) in [
        ("bin/cargo", "cargo "),
        ("bin/rustc", "rustc "),
        ("bin/rust-analyzer", "rust-analyzer "),
    ] {
        let output = through_its_own_loader(&artifact, program, &["--version"])
            .output()
            .unwrap_or_else(|error| panic!("{program} could not be started at all: {error}"));
        let said = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{program} did not answer --version with an empty environment: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            said.starts_with(expected),
            "{program} answered {said:?}, which is not a version"
        );
    }
}

#[test]
fn the_staged_cargo_can_read_a_workspace_it_has_never_seen() {
    // A version string proves a program starts. This proves it *works*: the
    // question a run about a Rust tree has is not whether cargo exists but
    // whether cargo can describe this tree, and `--no-deps` resolves nothing,
    // so it writes no lockfile and needs no registry and no network.
    let Some(artifact) = artifact_or_skip("the staged cargo reads a workspace") else {
        return;
    };
    let tree = tempfile::tempdir().expect("a temp workspace");
    std::fs::create_dir_all(tree.path().join("src")).expect("src");
    std::fs::write(
        tree.path().join("Cargo.toml"),
        b"[package]\nname = \"beacon\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .expect("the manifest");
    std::fs::write(
        tree.path().join("src/lib.rs"),
        b"pub struct LanternRegistry { pub lit: u32 }\n",
    )
    .expect("the source");

    let manifest = tree.path().join("Cargo.toml");
    let output = through_its_own_loader(
        &artifact,
        "bin/cargo",
        &[
            "metadata",
            "--no-deps",
            "--offline",
            "--format-version",
            "1",
            "--manifest-path",
            &manifest.display().to_string(),
        ],
    )
    .env("HOME", tree.path())
    .env("CARGO_HOME", tree.path().join("cargo-home"))
    .env("CARGO_NET_OFFLINE", "true")
    .output()
    .expect("starting the staged cargo");

    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the staged cargo could not read a workspace with nothing from the host:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        said.contains("\"name\":\"beacon\""),
        "cargo answered something that is not this workspace: {}",
        &said[..said.len().min(400)]
    );
}
