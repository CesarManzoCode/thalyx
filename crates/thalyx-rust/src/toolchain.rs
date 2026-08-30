//! Where this machine's Rust toolchain is — asked once, in one place.
//!
//! ## The failure this file exists to stop
//!
//! `dev/verify.sh` needs root: cgroups, namespaces, mounts, and an LSM that
//! actually denies. `rustup` installs into the *invoking* user's home. `sudo`
//! sets `HOME=/root`. So on the overwhelmingly common Fedora setup the whole
//! toolchain is right there and every `$HOME`-shaped search finds nothing —
//! and what Thalyx then reports is not "I am running as the wrong user", it is
//! `no_cargo`, `NoAnalyzer`, and two stages saying `NOT PROVEN` about a
//! machine that has everything they asked for.
//!
//! It happened on 2026-08-29 with `rustup component add rust-analyzer` typed
//! into the shell *immediately before* the run that said there was none. Rule
//! 5: the instrument includes the harness, and here the harness is the
//! environment `sudo` hands over.
//!
//! There were three separate searches before this file — one in
//! `metadata::cargo`, one in `analyzer::find`, one in `thalyx-cli`'s `hacer` —
//! and they disagreed about every question: which variables to read, whether
//! to trust `PATH`, whether to run a candidate before believing it. Three
//! answers to "where is cargo" is three machines.
//!
//! ## The order, and why it is this order
//!
//! 1. **What somebody said explicitly.** `THALYX_CARGO` and
//!    `THALYX_RUST_ANALYZER` name a file. A run whose meaning has to be
//!    reproducible is a run whose tools were named, not found.
//! 2. **What rustup itself was told.** `CARGO_HOME` and `RUSTUP_HOME` are
//!    rustup's own variables, they survive `sudo -E`, and `verify.sh` sets
//!    them from `$SUDO_USER`'s home precisely so that root can use the
//!    person's toolchain on purpose. Reading them is not a workaround: it is
//!    reading the configuration.
//! 3. **The invoking user's home, when `sudo` says who that was.** `SUDO_USER`
//!    plus the passwd entry, so a root shell finds the toolchain of the person
//!    who asked for it.
//! 4. **`HOME`.** The ordinary case, where nobody is pretending to be anybody.
//! 5. **Named system locations.** `/usr/local/bin`, `/usr/bin`. Two paths, in
//!    a fixed order.
//!
//! ## And never a walk of `PATH`
//!
//! A validation that ran whichever `cargo` came first on a caller's `PATH` is
//! a validation whose meaning depends on who started the session — and inside
//! Thalyx there is no `PATH` and no shell to expand one. The list above is
//! *named places*, every one of which a person can print. The one thing
//! `PATH` was buying was "it works on my machine", and the price was that
//! nobody could say which compiler had produced a verdict.
//!
//! ## Every candidate is run, not stat'ed
//!
//! `~/.cargo/bin/rust-analyzer` exists on every rustup install and is a shim
//! that answers `error: Unknown binary`. `~/.cargo/bin/cargo` is a shim that
//! re-executes `rustup`, which inside a confinement is a second program the
//! grants say nothing about. A search that stopped at the first *file* would
//! pick both of them, every time. So a candidate becomes the answer only after
//! it has answered `--version`.
//!
//! The result is remembered for the life of the process. It cannot change
//! under a running one, and paying a subprocess per lookup would put a
//! `--version` in front of every question a program asks.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// A tool that was looked for, and what was found.
///
/// Two fields and not an `Option`, because "there is no cargo here" and "here
/// is the cargo" have different remedies and a caller that only got a path or
/// nothing cannot tell a missing toolchain from a missing *component*. Rule
/// 10, at the smallest scale there is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The executable, when one of the named places held one that ran.
    pub path: Option<PathBuf>,
    /// Every place that was looked at, in the order they were looked at.
    ///
    /// Carried so a refusal can name them. "There is no rust-analyzer" is a
    /// sentence a person cannot act on; "there is none at these five paths"
    /// is one they can.
    pub looked_at: Vec<PathBuf>,
    /// The variable that named it, when one did.
    pub named_by: Option<&'static str>,
}

impl Found {
    /// One sentence saying what was not found and where it was looked for.
    pub fn why_not(&self, what: &str, remedy: &str) -> String {
        let places: Vec<String> = self
            .looked_at
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        format!(
            "there is no {what} at any of the {} place(s) this machine looks: {}. {remedy}",
            places.len(),
            places.join(", ")
        )
    }
}

/// The homes a toolchain could be under, most explicit first.
///
/// Returned rather than searched in place so that both tools ask the same
/// question of the same list, and so a test can assert the *order* — which is
/// the part that decides whether root borrows the person's toolchain or looks
/// at its own empty one.
fn homes() -> Vec<PathBuf> {
    let mut homes = Vec::new();
    let mut push = |path: PathBuf| {
        if !homes.contains(&path) {
            homes.push(path);
        }
    };

    // `sudo` hands root a `HOME` of `/root` and leaves `SUDO_USER` saying who
    // actually typed the command. Their home is where rustup put everything.
    if let Some(user) = std::env::var_os("SUDO_USER")
        && let Some(home) = home_of(&user.to_string_lossy())
    {
        push(home);
    }
    if let Some(home) = std::env::var_os("HOME") {
        push(PathBuf::from(home));
    }
    homes
}

/// A user's home directory, read from the passwd database.
///
/// `getent` rather than parsing `/etc/passwd`: a machine using LDAP, SSSD or
/// systemd-homed has users that are not in that file, and a lookup that only
/// worked for local accounts would be a lookup that works on a laptop and not
/// on a workstation joined to a domain. If `getent` is not there, this
/// answers nothing rather than guessing `/home/<name>` — a guessed home is a
/// path that exists on most machines and belongs to the wrong person on some.
fn home_of(user: &str) -> Option<PathBuf> {
    let output = Command::new("getent")
        .arg("passwd")
        .arg(user)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let home = line.trim_end().split(':').nth(5)?;
    (!home.is_empty()).then(|| PathBuf::from(home))
}

/// Every `bin` directory of every installed toolchain under a rustup home.
///
/// Sorted, so a machine with several toolchains picks the same one every time.
/// A verdict whose meaning depends on directory order is a verdict nobody can
/// reproduce.
fn toolchain_bins(rustup_home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(rustup_home.join("toolchains")) else {
        return Vec::new();
    };
    let mut bins: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path().join("bin"))
        .collect();
    bins.sort();
    bins
}

/// Where an installed toolchain's binaries could be, most explicit first.
fn toolchain_places() -> Vec<PathBuf> {
    let mut places = Vec::new();
    if let Some(named) = std::env::var_os("RUSTUP_HOME") {
        places.extend(toolchain_bins(Path::new(&named)));
    }
    for home in homes() {
        places.extend(toolchain_bins(&home.join(".rustup")));
    }
    places
}

/// The `bin` directories of a cargo installation — rustup's shims and the
/// distribution's.
///
/// Last resort for both tools, and for cargo it is behind the real toolchains
/// on purpose: `~/.cargo/bin/cargo` is a shim that re-executes `rustup`, and
/// inside a confinement that is a second program the grants say nothing about.
/// The run is then refused for naming `rustup`, which reads like a broken
/// change and is a broken installation layout.
fn shim_places() -> Vec<PathBuf> {
    let mut places = Vec::new();
    if let Some(named) = std::env::var_os("CARGO_HOME") {
        places.push(PathBuf::from(named).join("bin"));
    }
    for home in homes() {
        places.push(home.join(".cargo").join("bin"));
    }
    places.push(PathBuf::from("/usr/local/bin"));
    places.push(PathBuf::from("/usr/bin"));
    places
}

/// Whether a candidate is the tool, established by asking it.
fn answers(candidate: &Path) -> bool {
    candidate.is_file()
        && Command::new(candidate)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
}

/// Look for one binary in the named places, running each candidate.
fn look_for(binary: &str, named_by: &'static str, places: Vec<PathBuf>) -> Found {
    let mut looked_at = Vec::new();
    let mut named = None;

    if let Some(explicit) = std::env::var_os(named_by) {
        let path = PathBuf::from(explicit);
        if !path.as_os_str().is_empty() {
            looked_at.push(path.clone());
            if answers(&path) {
                return Found {
                    path: Some(path),
                    looked_at,
                    named_by: Some(named_by),
                };
            }
            // Named and wrong is worth saying. A variable pointing at a file
            // that will not run is a mistake somebody made on purpose, and
            // falling through silently to a different binary would answer a
            // question nobody asked.
            named = Some(named_by);
        }
    }

    for place in places {
        let candidate = place.join(binary);
        if looked_at.contains(&candidate) {
            continue;
        }
        looked_at.push(candidate.clone());
        if answers(&candidate) {
            return Found {
                path: Some(candidate),
                looked_at,
                named_by: None,
            };
        }
    }

    Found {
        path: None,
        looked_at,
        named_by: named,
    }
}

/// The environment variable that names a cargo outright.
pub const CARGO_VARIABLE: &str = "THALYX_CARGO";

/// The environment variable that names a rust-analyzer outright.
pub const ANALYZER_VARIABLE: &str = "THALYX_RUST_ANALYZER";

/// Where this machine's `cargo` is, and where it was looked for.
pub fn cargo() -> &'static Found {
    static ASKED: OnceLock<Found> = OnceLock::new();
    ASKED.get_or_init(|| {
        let mut places = toolchain_places();
        places.extend(shim_places());
        look_for("cargo", CARGO_VARIABLE, places)
    })
}

/// Where this machine's `rust-analyzer` is, and where it was looked for.
pub fn rust_analyzer() -> &'static Found {
    static ASKED: OnceLock<Found> = OnceLock::new();
    ASKED.get_or_init(|| {
        let mut places = toolchain_places();
        places.extend(shim_places());
        look_for("rust-analyzer", ANALYZER_VARIABLE, places)
    })
}

/// The `cargo` to run, falling back to the bare name.
///
/// The bare name is a `PATH` lookup, which everything above exists to avoid —
/// so it is reached only when the named places held nothing that ran, and it
/// exists so that a machine laying its toolchain out in some way nobody
/// anticipated still gets an attempt rather than a refusal. Every caller that
/// needs to *say* whether a real cargo was found asks [`cargo`] instead.
pub fn cargo_command() -> PathBuf {
    cargo()
        .path
        .clone()
        .unwrap_or_else(|| PathBuf::from("cargo"))
}

/// The environment a Cargo — or anything that runs one — needs to find its
/// toolchain when the process asking is not the user who installed it.
///
/// Returned as pairs rather than set on this process: setting `HOME` here
/// would change what every other part of Thalyx thinks the machine is, which
/// is rule 11 — a global switch with no owner, whose value is some other
/// check's precondition.
pub fn environment() -> Vec<(&'static str, PathBuf)> {
    let mut environment = Vec::new();
    let rustup = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            homes()
                .into_iter()
                .map(|home| home.join(".rustup"))
                .find(|path| path.is_dir())
        });
    if let Some(rustup) = rustup {
        environment.push(("RUSTUP_HOME", rustup));
    }
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            homes()
                .into_iter()
                .map(|home| home.join(".cargo"))
                .find(|path| path.is_dir())
        });
    if let Some(cargo_home) = cargo_home {
        environment.push(("CARGO_HOME", cargo_home));
    }
    environment
}

/// The environment variable that names where the loader looks first.
pub const LOADER_PATH_VARIABLE: &str = "LD_LIBRARY_PATH";

/// The directory a toolchain binary's own `RUNPATH` means, resolved from where
/// the binary really is rather than from where it is executed.
///
/// ## The failure this exists to stop
///
/// Every binary rustup installs carries `RUNPATH: [$ORIGIN/../lib]`, and
/// `librustc_driver-<hash>.so` — which `rust-analyzer` cannot start without —
/// is what it is there to find. **`$ORIGIN` is the directory the loader finds
/// the binary in**, and inside a confinement that is not where it was
/// installed: `foreign::establish` mounts the program's own directory at
/// `/module`, so a `rust-analyzer` living in `<toolchain>/bin` is executed as
/// `/module/rust-analyzer`, `$ORIGIN/../lib` becomes `/lib`, and the process
/// dies before its first byte of LSP saying
///
/// ```text
/// error while loading shared libraries: librustc_driver-<hash>.so:
/// cannot open shared object file: No such file or directory
/// ```
///
/// That is status 127 with no `SIGSYS` and nothing in `ausearch` — a death
/// that looks exactly like the seccomp filter killing the process and is not
/// the filter at all. Cargo does not meet it because `cargo` needs no
/// `librustc_driver`, and the `rustc` it starts is started at its own absolute
/// path, where `$ORIGIN` still means what it was linked to mean.
///
/// ## And why naming it is not a widening
///
/// The directory is inside the toolchain [`readable`] already grants
/// read-only, a grant keeps its absolute path inside the root filesystem, and
/// a `LD_LIBRARY_PATH` entry naming something nobody granted names something
/// that is not there. Nothing new is reachable; the loader is told the one
/// place its own `RUNPATH` meant.
///
/// Derived from the binary and never spelled: no hash, no version, no
/// toolchain name. `None` when there is no such directory, so a binary laid
/// out some other way is left to its own `RUNPATH`.
pub fn loader_path(binary: &Path) -> Option<PathBuf> {
    let lib = binary.parent()?.parent()?.join("lib");
    lib.is_dir().then_some(lib)
}

/// Everything a confined toolchain run must be able to read.
///
/// The registry and the toolchain, and nothing else. Named here rather than at
/// each call site, because a grant list assembled twice is two grant lists and
/// the second one is always missing the thing that only breaks on somebody
/// else's machine.
pub fn readable() -> Vec<PathBuf> {
    let mut readable = Vec::new();
    let mut push = |path: PathBuf| {
        if path.is_dir() && !readable.contains(&path) {
            readable.push(path);
        }
    };
    for (_, path) in environment() {
        push(path);
    }
    for home in homes() {
        push(home.join(".cargo"));
        push(home.join(".rustup"));
    }
    // The directory the binary itself is in, whichever of the above it turned
    // out to be under. A grant on `~/.cargo` does not cover `/usr/bin/cargo`.
    for found in [cargo(), rust_analyzer()] {
        if let Some(path) = &found.path
            && let Some(parent) = path.parent()
        {
            push(parent.to_path_buf());
        }
    }
    readable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_variable_that_names_nothing_does_not_become_the_answer() {
        // The shape of the mistake: a caller sets `THALYX_CARGO` to a typo and
        // the machine quietly runs some other cargo, so the run is reproducible
        // for everyone except the person who configured it.
        let found = look_for(
            "cargo",
            "THALYX_TEST_NOTHING_NAMES_THIS",
            vec![PathBuf::from("/nonexistent/place")],
        );
        assert_eq!(found.path, None);
        assert!(
            found
                .looked_at
                .contains(&PathBuf::from("/nonexistent/place/cargo")),
            "{found:?}"
        );
    }

    #[test]
    fn a_file_that_is_not_the_tool_is_not_the_tool() {
        // `~/.cargo/bin/rust-analyzer` is exactly this: present, executable,
        // and answers `error: Unknown binary`. A search that stopped at the
        // first file would pick it on every rustup install there is.
        let directory = tempfile::tempdir().expect("a temp dir");
        let impostor = directory.path().join("rust-analyzer");
        std::fs::write(&impostor, "#!/bin/sh\nexit 1\n").expect("the file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&impostor, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }

        let found = look_for(
            "rust-analyzer",
            "THALYX_TEST_NOTHING_NAMES_THIS",
            vec![directory.path().to_path_buf()],
        );
        assert_eq!(found.path, None, "a shim that fails was taken for the tool");
        assert_eq!(found.looked_at, vec![impostor]);
    }

    #[test]
    fn what_was_not_found_says_where_it_looked() {
        // A refusal a person can act on. "There is no rust-analyzer" is not
        // one; "there is none at these paths" tells them which home the search
        // was in, which is the whole of the sudo problem.
        let found = look_for(
            "rust-analyzer",
            "THALYX_TEST_NOTHING_NAMES_THIS",
            vec![
                PathBuf::from("/nonexistent/one"),
                PathBuf::from("/none/two"),
            ],
        );
        let why = found.why_not(
            "rust-analyzer",
            "Add it with: rustup component add rust-analyzer",
        );
        assert!(why.contains("/nonexistent/one/rust-analyzer"), "{why}");
        assert!(why.contains("/none/two/rust-analyzer"), "{why}");
        assert!(why.contains("rustup component add"), "{why}");
    }

    #[test]
    fn a_home_is_never_guessed_from_a_user_name() {
        // `/home/<name>` exists on most machines and belongs to the wrong
        // person on some. Rule 9: the cautious answer, never the fast one.
        assert_eq!(home_of("a-user-that-does-not-exist-here"), None);
    }

    #[test]
    fn several_toolchains_are_looked_at_in_the_same_order_every_time() {
        // A verdict whose meaning depends on `read_dir` order is a verdict
        // nobody can reproduce — and `read_dir` order is not sorted on any
        // filesystem this runs on.
        let directory = tempfile::tempdir().expect("a temp dir");
        for name in ["stable-x86_64", "beta-x86_64", "nightly-x86_64"] {
            std::fs::create_dir_all(directory.path().join("toolchains").join(name).join("bin"))
                .expect("a toolchain");
        }
        let once = toolchain_bins(directory.path());
        let twice = toolchain_bins(directory.path());
        assert_eq!(once, twice);
        assert_eq!(once.len(), 3);
        assert!(once[0].ends_with("beta-x86_64/bin"), "{once:?}");
    }
}
