//! The Rust runtime Thalyx carries, as opposed to the one a host happens to
//! have.
//!
//! ## The failure this file exists to stop
//!
//! On 2026-08-30 a paid benchmark watched Claude, inside the machine, choose
//! exactly the right primitive — `thalyx.context`, then `thalyx.rename` — and
//! receive `source: index`, `analyzer_starts: 0`, and
//!
//! ```text
//! rename: { ok: false, error: unresolved,
//!           message: "there is no `cargo` on this machine" }
//! ```
//!
//! Everything the agent did afterwards was a consequence of a promise the
//! machine could not keep. [`crate::toolchain`] knows how to find a toolchain
//! that somebody installed; inside Thalyx nobody installed one, and nobody is
//! going to — `Filosofia-Fundacional.md` says Thalyx is the whole system, so a
//! programming face that only works because the host has rustup is a
//! programming face that belongs to the host.
//!
//! So the runtime is **an artifact of Thalyx's own**, built by
//! `dev/build-rust-runtime.sh` from digest-checked upstream tarballs, staged
//! onto the store, and found here. Move the disk to another x86_64 machine and
//! the semantic provider moves with it.
//!
//! ## Where it lives, and why the store rather than the image
//!
//! `<store>/toolchains/rust/<identity>/`, which is
//! `/opt/thalyx/toolchains/rust/rust-<version>-<target>/` on a running machine.
//!
//! Never the initramfs. `make -C image count` says the image is the Linux
//! kernel and one program, and that is the decree it exists to keep countable
//! — six hundred megabytes of compiler is *software installed on Thalyx*,
//! which is the whole distinction between what Thalyx is and what has been put
//! on it. The engine and its weights already live on the store for the same
//! reason.
//!
//! ## What the artifact is made of
//!
//! The upstream `x86_64-unknown-linux-musl` host tools, which need exactly two
//! files Rust does not publish: musl's loader — compiled from musl's own
//! release tarball by the build script — and `libgcc_s.so.1`, linked out of
//! the `libunwind.a` that ships inside `rust-std` itself. Neither is copied
//! from the machine that built it. The reasoning, and the measurement behind
//! choosing musl over the ordinary GNU toolchain, is in
//! `dev/build-rust-runtime.sh` and in
//! `vault/09-Notas-Tecnicas/Runtime-Rust-Agente.md`.

use std::path::{Path, PathBuf};

/// Where runtimes live under a store root.
///
/// One string, used by the build script's `INSIDE`, by PID 1, and by
/// discovery. Two places spelling this is two answers to where the toolchain
/// is, and the second one is always the empty directory somebody is confused
/// by.
pub const UNDER: &str = "toolchains/rust";

/// The store root a machine uses when nothing says otherwise.
///
/// The same default `thalyx-cli` has. Repeated rather than depended on because
/// this crate must not depend on the CLI, and a *wrong* default here would be
/// a silent failure to find a toolchain that is right there.
pub const STORE_ROOT: &str = "/opt/thalyx";

/// The file the loader is asked for by every program in the artifact.
///
/// It is in the ELF header of the binaries themselves — `PT_INTERP` — so it is
/// not a choice anybody here gets to make. PID 1 makes the name resolve.
pub const LOADER: &str = "/lib/ld-musl-x86_64.so.1";

/// Everything the artifact must contain to be one, as paths relative to its
/// root.
///
/// A list of what is there rather than a list of what is not: an exclusion
/// list is a claim about everything nobody thought of.
///
/// `lib/rustlib/src` is on it because it was **measured** to be required, not
/// because it seemed thorough: without it rust-analyzer says `can't load
/// standard library, try installing rust-src` and then dies partway through
/// the first analysis.
pub const NEEDED: &[&str] = &[
    "bin/cargo",
    "bin/rustc",
    "bin/rust-analyzer",
    "libexec/rust-analyzer-proc-macro-srv",
    "lib/libc.so",
    "lib/ld-musl-x86_64.so.1",
    "lib/libgcc_s.so.1",
    "lib/rustlib/src",
    "runtime.json",
];

/// Things whose presence means somebody copied a whole toolchain in.
///
/// Checked rather than trusted, because the way this goes wrong is not a crash
/// — it is a store that quietly grew by a gigabyte of manual pages and
/// documentation nothing inside the machine can read, and nobody noticing
/// until the disk is full.
pub const FORBIDDEN: &[&str] = &[
    "share",
    "bin/rustdoc",
    "bin/rustfmt",
    "bin/cargo-clippy",
    "bin/rustup",
    "lib/rustlib/etc",
];

/// A staged runtime, and what it says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    /// Where the artifact is, on the machine asking.
    pub root: PathBuf,
    /// `rust-<version>-<target>`, which is the directory's own name.
    pub identity: String,
    /// The Rust release, from `runtime.json`.
    pub rust: Option<String>,
    /// The musl release the loader was built from, from `runtime.json`.
    pub musl: Option<String>,
}

impl Runtime {
    pub fn cargo(&self) -> PathBuf {
        self.root.join("bin/cargo")
    }
    pub fn rustc(&self) -> PathBuf {
        self.root.join("bin/rustc")
    }
    pub fn rust_analyzer(&self) -> PathBuf {
        self.root.join("bin/rust-analyzer")
    }
    pub fn lib(&self) -> PathBuf {
        self.root.join("lib")
    }
    /// The loader, at the path inside the artifact — which is the file PID 1
    /// makes [`LOADER`] point at.
    pub fn loader(&self) -> PathBuf {
        self.root.join("lib/libc.so")
    }
    /// One line naming what this is, for an answer that has to say which
    /// toolchain produced it.
    pub fn describe(&self) -> String {
        match (&self.rust, &self.musl) {
            (Some(rust), Some(musl)) => {
                format!(
                    "Thalyx runtime {} (Rust {rust}, musl {musl})",
                    self.identity
                )
            }
            _ => format!("Thalyx runtime {}", self.identity),
        }
    }
}

/// The directory runtimes are staged into, under a store root.
pub fn directory(store_root: &Path) -> PathBuf {
    store_root.join(UNDER)
}

/// The store root this process should look under.
///
/// `THALYX_ROOT` first, because that is how every test and every stage of
/// `verify.sh` moves the store somewhere it can be written without becoming
/// the machine's real one.
pub fn store_root() -> PathBuf {
    std::env::var_os("THALYX_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(STORE_ROOT))
}

/// Every runtime staged under a store root, in a fixed order.
///
/// Sorted by directory name, so a store that somehow holds two of them picks
/// the same one on every boot. A verdict whose meaning depends on `read_dir`
/// order is a verdict nobody can reproduce — and `read_dir` is not sorted on
/// any filesystem this runs on.
pub fn staged(store_root: &Path) -> Vec<Runtime> {
    let Ok(entries) = std::fs::read_dir(directory(store_root)) else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    roots.sort();
    roots.iter().filter_map(|root| read(root)).collect()
}

/// Read one artifact directory, if it looks like an artifact at all.
///
/// `None` when the required files are not all there. That is deliberate and it
/// is rule 9: a half-staged toolchain — an interrupted copy, a disk that
/// filled — must not be discovered, because what it produces is not a refusal
/// but a rust-analyzer that starts and dies, which reads as the semantic
/// provider being broken rather than as a store that was never finished.
pub fn read(root: &Path) -> Option<Runtime> {
    if !NEEDED.iter().all(|needed| root.join(needed).exists()) {
        return None;
    }
    let identity = root.file_name()?.to_string_lossy().into_owned();
    let (rust, musl) = match std::fs::read_to_string(root.join("runtime.json")) {
        Ok(text) => (field(&text, "rust"), field(&text, "musl")),
        // A runtime.json that cannot be read is not a runtime that is not
        // there: the binaries are all present and they will run. Rule 10 — say
        // which one happened — so the version comes back unknown and the
        // toolchain is still found.
        Err(_) => (None, None),
    };
    Some(Runtime {
        root: root.to_path_buf(),
        identity,
        rust,
        musl,
    })
}

/// One string field out of `runtime.json`, without a JSON dependency.
///
/// This crate is read by PID 1's own binary and the file is written by a shell
/// script three lines above where it is read; a parser here would be a
/// dependency taken on to read two version numbers that are only ever used in
/// a sentence a human reads.
fn field(text: &str, name: &str) -> Option<String> {
    let needle = format!("\"{name}\"");
    let after = text.split_once(&needle)?.1;
    let after = after.split_once(':')?.1;
    let after = after.trim_start();
    let rest = after.strip_prefix('"')?;
    let (value, _) = rest.split_once('"')?;
    Some(value.to_string())
}

/// The runtime this machine has, if it has one.
pub fn managed() -> Option<Runtime> {
    staged(&store_root()).into_iter().next()
}

// ── is a staged tree the thing it claims to be ───────────────────────────────

/// What a directory has and has not, said in the terms that decide what to do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// Required paths that are not there.
    pub missing: Vec<String>,
    /// Paths that are there and should not be — the sign that a whole
    /// toolchain was copied rather than a runtime assembled.
    pub forbidden: Vec<String>,
    /// Target directories under `lib/rustlib` other than this artifact's own.
    pub other_targets: Vec<String>,
}

impl Report {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.forbidden.is_empty() && self.other_targets.is_empty()
    }
}

/// Look at a staged tree and say what is wrong with it.
///
/// Separate from [`read`] because they answer different questions: `read` asks
/// "can this be used", which a machine needs at every boot, and this asks "was
/// this assembled correctly", which the build and its tests need once.
pub fn inspect(root: &Path) -> Report {
    let mut report = Report::default();
    for needed in NEEDED {
        if !root.join(needed).exists() {
            report.missing.push((*needed).to_string());
        }
    }
    for forbidden in FORBIDDEN {
        if root.join(forbidden).exists() {
            report.forbidden.push((*forbidden).to_string());
        }
    }
    // The target the artifact says it is for is the only one that may be under
    // `rustlib`. `lib/rustlib/x86_64-unknown-linux-gnu` in a musl artifact is
    // 220 MB of a standard library for a machine this is not.
    //
    // Read from `runtime.json` and only then from the directory's name: a
    // staging step copies the artifact under whatever name it likes, and an
    // artifact that reported a problem because somebody renamed its directory
    // would be reporting on the copy rather than on the toolchain. Found by
    // staging one as `broken/` on purpose and watching it accuse its own
    // standard library.
    let ours = std::fs::read_to_string(root.join("runtime.json"))
        .ok()
        .and_then(|text| field(&text, "target"))
        .or_else(|| {
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    if let Ok(entries) = std::fs::read_dir(root.join("lib/rustlib")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "src" || !entry.path().is_dir() {
                continue;
            }
            if !ours.ends_with(&name) {
                report.other_targets.push(name);
            }
        }
    }
    report
}

// ── does the artifact carry what its own programs ask for ────────────────────

/// One program of the artifact and what it will ask the loader for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub program: PathBuf,
    pub interpreter: Option<String>,
    /// Every `DT_NEEDED`, paired with whether this artifact has it.
    pub libraries: Vec<(String, bool)>,
}

/// Whether the artifact is closed: everything its programs name is inside it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Closure {
    pub programs: Vec<Asked>,
    /// `(program, library)` for every name nothing in the artifact provides.
    pub unresolved: Vec<(String, String)>,
    /// The interpreters the programs ask the kernel for, deduplicated.
    ///
    /// Carried rather than checked against a constant, because the constant is
    /// derived *from* this: [`LOADER`] is what the binaries say, not what
    /// anybody chose.
    pub interpreters: Vec<String>,
    /// Whether the artifact carries a file with the interpreter's own name.
    pub interpreter_inside: bool,
}

impl Closure {
    /// Nothing is missing and the loader travels with the artifact.
    pub fn is_closed(&self) -> bool {
        self.unresolved.is_empty() && self.interpreter_inside && !self.programs.is_empty()
    }
}

/// Read every program in the artifact and say whether the artifact is enough.
///
/// This is the check that distinguishes "the files were copied" from "the
/// machine can run them", and it is the whole point of the exercise: a runtime
/// that resolves against the *building* host looks identical, on the building
/// host, to one that resolves against itself. `ldd` cannot tell them apart
/// because `ldd` asks the host. This asks the artifact.
///
/// Symbols are not its business — a name that resolves to a library which is
/// missing a function is a different failure, and the build script catches
/// that one by running the programs.
pub fn closure(root: &Path) -> Closure {
    let mut closure = Closure::default();
    let lib = root.join("lib");
    for relative in NEEDED {
        if !relative.starts_with("bin/") && !relative.starts_with("libexec/") {
            continue;
        }
        let program = root.join(relative);
        let Some(needs) = crate::elf::needs_of(&program) else {
            continue;
        };
        if let Some(interpreter) = &needs.interpreter
            && !closure.interpreters.contains(interpreter)
        {
            closure.interpreters.push(interpreter.clone());
        }
        let mut libraries = Vec::new();
        for name in &needs.libraries {
            let here = lib.join(name).exists();
            if !here {
                closure
                    .unresolved
                    .push(((*relative).to_string(), name.clone()));
            }
            libraries.push((name.clone(), here));
        }
        closure.programs.push(Asked {
            program,
            interpreter: needs.interpreter,
            libraries,
        });
    }
    // The kernel reads `PT_INTERP` as an absolute path, so what matters is
    // that the artifact carries a file of that name for PID 1 to point the
    // path at. Derived from what the binaries said rather than from `LOADER`,
    // so an artifact built for some other interpreter is reported honestly
    // instead of silently checked against the wrong name.
    closure.interpreter_inside = !closure.interpreters.is_empty()
        && closure.interpreters.iter().all(|interpreter| {
            Path::new(interpreter)
                .file_name()
                .is_some_and(|name| lib.join(name).exists())
        });
    closure
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree shaped like a finished artifact, with empty files.
    fn staged_tree(identity: &str) -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("a temp dir");
        let root = directory.path().join(identity);
        for needed in NEEDED {
            let path = root.join(needed);
            if *needed == "lib/rustlib/src" {
                std::fs::create_dir_all(&path).expect("the source directory");
                continue;
            }
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
            std::fs::write(&path, b"").expect("the file");
        }
        std::fs::create_dir_all(root.join("lib/rustlib/x86_64-unknown-linux-musl/lib"))
            .expect("the sysroot");
        std::fs::write(
            root.join("runtime.json"),
            br#"{"identity": "x", "rust": "1.90.0", "musl": "1.2.4", "target": "x86_64-unknown-linux-musl"}"#,
        )
        .expect("the description");
        directory
    }

    #[test]
    fn an_artifact_with_everything_it_needs_is_complete() {
        let held = staged_tree("rust-1.90.0-x86_64-unknown-linux-musl");
        let root = held.path().join("rust-1.90.0-x86_64-unknown-linux-musl");
        let report = inspect(&root);
        assert!(report.is_complete(), "{report:?}");
    }

    #[test]
    fn a_copy_of_a_whole_toolchain_is_not_an_artifact() {
        // The mistake this catches is not a crash: it is a store that grew by
        // most of a gigabyte of manual pages, and nobody noticing until the
        // disk filled.
        let held = staged_tree("rust-1.90.0-x86_64-unknown-linux-musl");
        let root = held.path().join("rust-1.90.0-x86_64-unknown-linux-musl");
        std::fs::create_dir_all(root.join("share/doc")).expect("the documentation");
        std::fs::write(root.join("bin/rustdoc"), b"").expect("rustdoc");
        let report = inspect(&root);
        assert!(!report.is_complete());
        assert!(
            report.forbidden.contains(&"share".to_string()),
            "{report:?}"
        );
        assert!(
            report.forbidden.contains(&"bin/rustdoc".to_string()),
            "{report:?}"
        );
    }

    #[test]
    fn a_standard_library_for_another_machine_is_not_wanted() {
        let held = staged_tree("rust-1.90.0-x86_64-unknown-linux-musl");
        let root = held.path().join("rust-1.90.0-x86_64-unknown-linux-musl");
        std::fs::create_dir_all(root.join("lib/rustlib/x86_64-unknown-linux-gnu/lib"))
            .expect("the other target");
        let report = inspect(&root);
        assert_eq!(
            report.other_targets,
            vec!["x86_64-unknown-linux-gnu".to_string()]
        );
        assert!(!report.is_complete());
    }

    #[test]
    fn the_standard_library_sources_are_required_because_the_analyzer_dies_without_them() {
        // Measured, not guessed: with `lib/rustlib/src` absent, rust-analyzer
        // logs `can't load standard library, try installing rust-src` and
        // aborts partway through its first analysis.
        assert!(NEEDED.contains(&"lib/rustlib/src"));
        let held = staged_tree("rust-1.90.0-x86_64-unknown-linux-musl");
        let root = held.path().join("rust-1.90.0-x86_64-unknown-linux-musl");
        std::fs::remove_dir_all(root.join("lib/rustlib/src")).expect("removing the sources");
        assert!(
            inspect(&root)
                .missing
                .contains(&"lib/rustlib/src".to_string())
        );
    }

    #[test]
    fn renaming_the_directory_does_not_make_the_artifact_wrong() {
        // Found by staging one under the name `broken/` on purpose: the target
        // used to be read from the directory's name, so an artifact copied
        // under any other name accused its own standard library of being for a
        // machine it is not. The check was reporting on the copy instead of on
        // the toolchain.
        let held = staged_tree("something-somebody-renamed");
        let root = held.path().join("something-somebody-renamed");
        assert!(inspect(&root).other_targets.is_empty(), "{root:?}");
    }

    #[test]
    fn a_half_staged_runtime_is_not_discovered() {
        // An interrupted copy leaves a directory that looks finished. What it
        // produces is not a refusal but a rust-analyzer that starts and dies,
        // which reads as a broken provider rather than an unfinished store.
        let held = staged_tree("rust-1.90.0-x86_64-unknown-linux-musl");
        let root = held.path().join("rust-1.90.0-x86_64-unknown-linux-musl");
        std::fs::remove_file(root.join("lib/libgcc_s.so.1")).expect("removing the unwinder");
        assert_eq!(read(&root), None);
        assert!(staged(held.path()).is_empty());
    }

    #[test]
    fn a_staged_runtime_says_which_rust_and_which_musl_it_is() {
        let held = staged_tree("rust-1.90.0-x86_64-unknown-linux-musl");
        let root = held.path().join("rust-1.90.0-x86_64-unknown-linux-musl");
        let runtime = read(&root).expect("a runtime");
        assert_eq!(runtime.rust.as_deref(), Some("1.90.0"));
        assert_eq!(runtime.musl.as_deref(), Some("1.2.4"));
        assert!(runtime.describe().contains("musl 1.2.4"), "{runtime:?}");
    }

    #[test]
    fn two_staged_runtimes_are_chosen_between_the_same_way_every_time() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let under = directory.path().join(UNDER);
        for identity in ["rust-1.91.0-x86_64-unknown-linux-musl", "rust-1.90.0-x"] {
            for needed in NEEDED {
                let path = under.join(identity).join(needed);
                if *needed == "lib/rustlib/src" {
                    std::fs::create_dir_all(&path).expect("sources");
                    continue;
                }
                std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
                std::fs::write(&path, b"{}").expect("a file");
            }
        }
        let once = staged(directory.path());
        let twice = staged(directory.path());
        assert_eq!(once, twice);
        assert_eq!(once.len(), 2);
        assert_eq!(once[0].identity, "rust-1.90.0-x");
    }
}
