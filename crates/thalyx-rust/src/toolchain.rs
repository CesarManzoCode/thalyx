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
//! 2. **Thalyx's own runtime**, staged on the store — see [`crate::runtime`].
//!    Added 2026-08-31, after a paid benchmark run inside the machine came
//!    back with `there is no cargo on this machine` for a workspace Thalyx had
//!    promised it could rename symbols in. It is second and not fifth on
//!    purpose: `Filosofia-Fundacional.md` says Thalyx is the whole system, so
//!    when Thalyx carries a compiler that is *the* compiler, and a host's
//!    installed one is the fallback rather than the other way round. Only a
//!    person naming a file outright outranks it.
//! 3. **What rustup itself was told.** `CARGO_HOME` and `RUSTUP_HOME` are
//!    rustup's own variables, they survive `sudo -E`, and `verify.sh` sets
//!    them from `$SUDO_USER`'s home precisely so that root can use the
//!    person's toolchain on purpose. Reading them is not a workaround: it is
//!    reading the configuration.
//! 4. **The invoking user's home, when `sudo` says who that was.** `SUDO_USER`
//!    plus the passwd entry, so a root shell finds the toolchain of the person
//!    who asked for it.
//! 5. **`HOME`.** The ordinary case, where nobody is pretending to be anybody.
//! 6. **Named system locations.** `/usr/local/bin`, `/usr/bin`. Two paths, in
//!    a fixed order.
//!
//! Steps 3 to 6 are how `dev/verify.sh` and every developer machine still
//! work: they have no store and therefore no step 2, and nothing about them
//! changed.
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

/// Whose toolchain answered.
///
/// Reported rather than inferred from the path, because the whole point of
/// `vault/09-Notas-Tecnicas/Runtime-Rust-Agente.md` is a machine that can say
/// **it is using its own** — and "the path starts with /opt/thalyx" is a guess
/// that is right until somebody mounts a store somewhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A variable named this file outright.
    Named,
    /// Thalyx's own runtime artifact, staged on the store.
    Managed,
    /// A toolchain somebody installed on the machine this is running on.
    Installed,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Named => "named",
            Kind::Managed => "thalyx",
            Kind::Installed => "host",
        }
    }
}

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
    /// Whose toolchain it turned out to be. `None` when nothing was found.
    pub kind: Option<Kind>,
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

/// The `bin` of Thalyx's own runtime artifact, when the store carries one.
///
/// **Ahead of every installed toolchain**, and that ordering is the decree of
/// 2026-08-31 rather than a preference: a Thalyx that resolved names with the
/// host's compiler would be a Thalyx whose programming face belongs to the
/// host. Only a variable that names a file outright comes before it, because a
/// person saying which compiler to use is the one thing more explicit than
/// Thalyx's own.
///
/// On a machine with no store — this container, a laptop running the tests —
/// there is nothing here and the search continues exactly as it did.
fn managed_places_under(store_root: &Path) -> Vec<PathBuf> {
    crate::runtime::staged(store_root)
        .into_iter()
        .map(|runtime| runtime.root.join("bin"))
        .collect()
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
///
/// Each place carries whose it is, so the answer can say which toolchain
/// produced it rather than leaving a caller to guess from the path.
fn look_for(binary: &str, named_by: &'static str, places: Vec<(PathBuf, Kind)>) -> Found {
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
                    kind: Some(Kind::Named),
                };
            }
            // Named and wrong is worth saying. A variable pointing at a file
            // that will not run is a mistake somebody made on purpose, and
            // falling through silently to a different binary would answer a
            // question nobody asked.
            named = Some(named_by);
        }
    }

    for (place, kind) in places {
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
                kind: Some(kind),
            };
        }
    }

    Found {
        path: None,
        looked_at,
        named_by: named,
        kind: None,
    }
}

/// Every place a tool could be, in the order authority says to look.
///
/// One list, built once, so `cargo` and `rust-analyzer` cannot disagree about
/// which toolchain this machine is using — a machine whose cargo is Thalyx's
/// and whose rust-analyzer is the host's is two machines wearing one name.
fn places() -> Vec<(PathBuf, Kind)> {
    places_under(&crate::runtime::store_root())
}

/// The same, for a named store — which is what makes the *order* testable
/// without a test having to change the machine it is measuring. Rule 11.
fn places_under(store_root: &Path) -> Vec<(PathBuf, Kind)> {
    let mut places: Vec<(PathBuf, Kind)> = managed_places_under(store_root)
        .into_iter()
        .map(|path| (path, Kind::Managed))
        .collect();
    places.extend(
        toolchain_places()
            .into_iter()
            .chain(shim_places())
            .map(|path| (path, Kind::Installed)),
    );
    places
}

/// The environment variable that names a cargo outright.
pub const CARGO_VARIABLE: &str = "THALYX_CARGO";

/// The environment variable that names a rust-analyzer outright.
pub const ANALYZER_VARIABLE: &str = "THALYX_RUST_ANALYZER";

/// Where this machine's `cargo` is, and where it was looked for.
pub fn cargo() -> &'static Found {
    static ASKED: OnceLock<Found> = OnceLock::new();
    ASKED.get_or_init(|| look_for("cargo", CARGO_VARIABLE, places()))
}

/// Where this machine's `rust-analyzer` is, and where it was looked for.
pub fn rust_analyzer() -> &'static Found {
    static ASKED: OnceLock<Found> = OnceLock::new();
    ASKED.get_or_init(|| look_for("rust-analyzer", ANALYZER_VARIABLE, places()))
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
///
/// ## Two machines, two answers, and the difference is the whole decree
///
/// When the toolchain is **Thalyx's own**, this must not name the host's
/// `RUSTUP_HOME` or `CARGO_HOME`. Handing a managed cargo the registry of
/// whoever built the disk is exactly the borrowing that
/// `vault/09-Notas-Tecnicas/Runtime-Rust-Agente.md` exists to end — and it
/// would be invisible, because on the machine that built the store it works.
/// So a managed toolchain gets a `CARGO_HOME` **on the store**, beside the
/// rest of Thalyx's state, and nothing pointing outward.
///
/// `CARGO_NET_OFFLINE` because the semantic provider has no network by
/// construction, and a Cargo that does not know that spends its timeout
/// finding out. Failing closed is the fast answer here as well as the correct
/// one.
///
/// `LD_LIBRARY_PATH` because the toolchain's binaries carry
/// `RPATH: [$ORIGIN/../lib]`, and **musl resolves `$ORIGIN` for the main
/// program by reading `/proc/self/exe`** — measured on 2026-08-31, where the
/// same binary ran with `/proc` mounted and failed without it with
/// `Error loading shared library librustc_driver-<hash>.so`. Naming the
/// directory outright makes the toolchain independent of whether whoever
/// starts it remembered `/proc`. It reaches nothing new: the directory is
/// inside the artifact [`readable`] already grants.
pub fn environment() -> Vec<(&'static str, String)> {
    let mut environment: Vec<(&'static str, String)> = Vec::new();

    if let Some(runtime) = managed_runtime() {
        environment.push((LOADER_PATH_VARIABLE, runtime.lib().display().to_string()));
        // Under the store, so it survives a reboot and belongs to Thalyx. Made
        // here rather than left to Cargo: a directory a grant names has to
        // exist before the grant can be given.
        let home = crate::runtime::store_root().join("state").join("cargo");
        let _ = std::fs::create_dir_all(&home);
        environment.push(("CARGO_HOME", home.display().to_string()));
        environment.push(("CARGO_NET_OFFLINE", "true".to_string()));
        return environment;
    }

    let rustup = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            homes()
                .into_iter()
                .map(|home| home.join(".rustup"))
                .find(|path| path.is_dir())
        });
    if let Some(rustup) = rustup {
        environment.push(("RUSTUP_HOME", rustup.display().to_string()));
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
        environment.push(("CARGO_HOME", cargo_home.display().to_string()));
    }
    environment
}

/// The runtime this machine's tools actually came out of, or `None`.
///
/// Asked of [`cargo`] rather than of the store, and the difference matters: a
/// store can hold an artifact that does not run — built for another
/// architecture, half copied, staged on a host with no musl loader — and in
/// that case [`cargo`] has already fallen through to an installed toolchain.
/// Reading the store would then describe a toolchain nothing is using.
pub fn managed_runtime() -> Option<crate::runtime::Runtime> {
    if cargo().kind != Some(Kind::Managed) {
        return None;
    }
    let bin = cargo().path.as_ref()?.parent()?;
    crate::runtime::read(bin.parent()?)
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
    // Thalyx's own runtime first, and it is the only entry that matters
    // inside the machine: the artifact holds the compiler, the standard
    // library, the standard library's sources and the loader, and a confined
    // provider that cannot read it cannot start.
    if let Some(runtime) = managed_runtime() {
        push(runtime.root.clone());
        push(crate::runtime::store_root().join("state").join("cargo"));
    }
    if let Some(named) = std::env::var_os("RUSTUP_HOME") {
        push(PathBuf::from(named));
    }
    if let Some(named) = std::env::var_os("CARGO_HOME") {
        push(PathBuf::from(named));
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
            vec![(PathBuf::from("/nonexistent/place"), Kind::Installed)],
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
            vec![(directory.path().to_path_buf(), Kind::Installed)],
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
                (PathBuf::from("/nonexistent/one"), Kind::Installed),
                (PathBuf::from("/none/two"), Kind::Installed),
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

    /// A staged runtime whose `bin/<name>` is a script that answers.
    ///
    /// A fake, and it models the property under test rather than standing in
    /// for the whole thing: the question here is *which place is looked at
    /// first and does a candidate that answers win*, and for that a script
    /// that prints a version is exactly as good as six hundred megabytes of
    /// compiler. Rule 8 — a fake must model the property, and this one does.
    fn a_runtime_that_answers(store_root: &Path, identity: &str) -> PathBuf {
        let root = crate::runtime::directory(store_root).join(identity);
        for needed in crate::runtime::NEEDED {
            let path = root.join(needed);
            if *needed == "lib/rustlib/src" {
                std::fs::create_dir_all(&path).expect("the sources");
                continue;
            }
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            std::fs::write(&path, b"{}").expect("a file");
        }
        for name in ["cargo", "rust-analyzer"] {
            let path = root.join("bin").join(name);
            std::fs::write(&path, format!("#!/bin/sh\necho '{name} 0.0.0 (thalyx)'\n"))
                .expect("the program");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                    .expect("executable");
            }
        }
        root
    }

    #[test]
    fn the_machines_own_runtime_is_looked_at_before_anything_installed() {
        // The decree of 2026-08-31: when Thalyx carries a compiler, that is
        // *the* compiler. An installed one is the fallback and not the other
        // way round — otherwise the machine's programming face belongs to
        // whatever host it happens to be booted on.
        let store = tempfile::tempdir().expect("a temp store");
        a_runtime_that_answers(store.path(), "rust-1.90.0-x86_64-unknown-linux-musl");
        let places = places_under(store.path());
        assert_eq!(places.first().map(|(_, kind)| *kind), Some(Kind::Managed));
        assert!(
            places[0]
                .0
                .ends_with("toolchains/rust/rust-1.90.0-x86_64-unknown-linux-musl/bin"),
            "{places:?}"
        );
        assert!(
            places[1..].iter().all(|(_, kind)| *kind == Kind::Installed),
            "{places:?}"
        );
    }

    #[test]
    fn a_machine_with_no_store_looks_exactly_where_it_always_did() {
        // The other half of the same claim, and the one that keeps every
        // developer machine and `dev/verify.sh` working: with nothing staged
        // there is no managed place at all, and the list is what it was.
        let empty = tempfile::tempdir().expect("a temp store");
        let places = places_under(empty.path());
        assert!(
            places.iter().all(|(_, kind)| *kind == Kind::Installed),
            "{places:?}"
        );
    }

    #[test]
    fn the_toolchain_thalyx_carries_is_the_one_that_answers() {
        // End of the ordering claim: not merely that the managed place is
        // first in a list, but that the search returns it and says whose it
        // is. `from: "thalyx"` is the field the preflight prints, and a run
        // that cannot tell whose compiler answered cannot tell whether the
        // machine was autonomous.
        let store = tempfile::tempdir().expect("a temp store");
        let root = a_runtime_that_answers(store.path(), "rust-1.90.0-x86_64-unknown-linux-musl");
        let found = look_for(
            "cargo",
            "THALYX_TEST_NOTHING_NAMES_THIS",
            places_under(store.path()),
        );
        assert_eq!(
            found.path.as_deref(),
            Some(root.join("bin/cargo").as_path())
        );
        assert_eq!(found.kind, Some(Kind::Managed));
    }

    #[test]
    fn a_half_staged_runtime_is_not_offered_as_a_toolchain() {
        // Rule 9. An interrupted copy leaves `bin/cargo` sitting there looking
        // finished; what it produces is a rust-analyzer that starts and dies,
        // which reads as a broken provider rather than an unfinished store.
        let store = tempfile::tempdir().expect("a temp store");
        let root = a_runtime_that_answers(store.path(), "rust-1.90.0-x86_64-unknown-linux-musl");
        std::fs::remove_file(root.join("lib/libc.so")).expect("removing the loader");
        assert!(
            managed_places_under(store.path()).is_empty(),
            "a runtime with no loader was offered as one"
        );
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
