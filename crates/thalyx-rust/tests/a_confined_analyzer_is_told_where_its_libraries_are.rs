//! The semantic provider is started knowing where its own libraries are.
//!
//! Written on 2026-08-30 from physical evidence on Fedora. Under confinement
//! rust-analyzer died like this, and it was read as the seccomp filter for a
//! day:
//!
//! ```text
//! the process exited with status 127
//! /module/rust-analyzer: error while loading shared libraries:
//! librustc_driver-<hash>.so: cannot open shared object file
//! ```
//!
//! No `SIGSYS`, nothing in `ausearch`, and cargo running fine beside it. The
//! cause is `RUNPATH: [$ORIGIN/../lib]`, which every binary rustup installs
//! carries: `$ORIGIN` is the directory the **loader** finds the binary in, and
//! `foreign::establish` mounts the program's own directory at `/module`, so a
//! server installed in `<toolchain>/bin` looks for its libraries in `/lib`.
//!
//! What is asserted here is the property the fix controls: the environment
//! handed to whoever starts the process names the directory that `RUNPATH`
//! meant, derived from where the binary really is. The confinement itself
//! cannot be built in this container, and a test that claimed otherwise would
//! be claiming to have proven the thing that was broken.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use thalyx_rust::analyzer::{Analyzer, Launching, Spawn, Started};

/// A spawner that starts nothing and remembers what it was asked with.
///
/// It refuses rather than standing a process up: the question is what reaches
/// a spawner, and a stand-in that spoke LSP would put a conversation between
/// the assertion and the thing being asserted.
#[derive(Default)]
struct Remembers(Arc<Mutex<Vec<(String, String)>>>);

impl Spawn for Remembers {
    fn start(&self, asked: Launching<'_>) -> thalyx_rust::Result<Started> {
        *self.0.lock().expect("the record") = asked.environment.to_vec();
        Err(thalyx_rust::RustError::NoAnalyzer(
            "this spawner exists to be asked, not to start anything".to_string(),
        ))
    }
}

/// A toolchain's shape on disk — `bin` beside `lib` — without a toolchain.
///
/// The layout is the whole of what the answer is derived from, so a fake that
/// has it models the property under test. Nothing here is executed.
fn toolchain_shaped(with_lib: bool) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("a temp dir");
    let bin = directory.path().join("bin");
    std::fs::create_dir_all(&bin).expect("a bin directory");
    if with_lib {
        std::fs::create_dir_all(directory.path().join("lib")).expect("a lib directory");
    }
    let binary = bin.join("rust-analyzer");
    (directory, binary)
}

/// What the spawner was handed, for a server started from `binary`.
fn environment_for(binary: &Path, named: &[(String, String)]) -> Vec<(String, String)> {
    let remembers = Remembers::default();
    let seen = Arc::clone(&remembers.0);
    let _ = Analyzer::start(Path::new("."), binary, None, &[], named, &remembers);
    seen.lock().expect("the record").clone()
}

#[test]
fn the_directory_the_runpath_meant_is_named_outright() {
    let (toolchain, binary) = toolchain_shaped(true);
    let handed = environment_for(&binary, &[]);
    let lib = toolchain.path().join("lib").display().to_string();
    assert!(
        handed.contains(&("LD_LIBRARY_PATH".to_string(), lib.clone())),
        "a server whose `RUNPATH` is `$ORIGIN/../lib` must be told that \
         directory by name, because inside the confinement `$ORIGIN` is \
         `/module`. Expected {lib}, and the spawner was handed: {handed:?}"
    );
}

#[test]
fn a_binary_with_no_such_directory_is_left_to_its_own_runpath() {
    // A guess would be worse than nothing here: naming a directory that is not
    // there puts a path in front of the loader that no grant covers, and turns
    // "this layout is unusual" into a second failure to read.
    let (_toolchain, binary) = toolchain_shaped(false);
    let handed = environment_for(&binary, &[]);
    assert!(
        !handed.iter().any(|(name, _)| name == "LD_LIBRARY_PATH"),
        "nothing should be named when there is no such directory, and the \
         spawner was handed: {handed:?}"
    );
}

#[test]
fn a_caller_that_named_it_is_not_overruled() {
    let (_toolchain, binary) = toolchain_shaped(true);
    let named = vec![("LD_LIBRARY_PATH".to_string(), "/somewhere/said".to_string())];
    let handed = environment_for(&binary, &named);
    assert_eq!(
        handed
            .iter()
            .filter(|(name, _)| name == "LD_LIBRARY_PATH")
            .collect::<Vec<_>>(),
        vec![&named[0]],
        "an explicit value is somebody's decision and this is a default; \
         the spawner was handed: {handed:?}"
    );
}
