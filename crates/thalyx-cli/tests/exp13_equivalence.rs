//! EXP-13's equivalence corpus, run against the binary that ships.
//!
//! `vault/09-Notas-Tecnicas/Frontera-de-Plataforma.md`. Cutting a platform
//! boundary through `hacer` is exactly the kind of change that can quietly make
//! Thalyx a different system while every unit test goes on passing, so this asks
//! the machine from outside, the way an agent does: a `thalyx bridge` process
//! per case, a confined session over its socket, and a scripted agent that asks
//! what a name is, reads the answer, and sends the program the answer decides.
//!
//! ## What is compared, and against what
//!
//! Every case under `dev/exp13/corpus/` is **inputs only**: a tree, what the
//! agent asks, and — for the managed backend — the differences the design says
//! it has, each with its reason. Nothing in a case says what the answer is. The
//! answers come from running things:
//!
//! 1. **`linux-current` against the unmodified revision** (`0492f72`), exactly:
//!    every answer, every evidence record, every metric that is not a clock, the
//!    journal, and the bytes of the tree afterwards. The unmodified binary is run
//!    beside it when `THALYX_EXP13_BASELINE` names one; otherwise its recorded
//!    answers in `dev/exp13/baseline/` are used — and only on a machine that
//!    answers `toolchain` the way the recording machine did, because an answer
//!    about a rust-analyzer that is not installed is an answer about a machine.
//! 2. **`linux-managed` against `linux-current`**, on what a transaction
//!    *means*: outcomes, verdicts, authority decisions, what the program
//!    returned, what changed, and the bytes of the tree afterwards. Identities
//!    and byte counts are left out because they are different by design, and
//!    every other difference must be one the case declares — and every
//!    declaration must happen, so a declaration cannot outlive the difference it
//!    excused.
//!
//! The tree digest is taken by this file, walking the directory itself. Asking
//! Thalyx what it published proves nothing (rule 2).
//!
//! ## What needs what
//!
//! `linux-current` needs a real Btrfs subvolume: `THALYX_BTRFS_SCRATCH` names a
//! directory on one, as for every other Btrfs test, and without it that half is
//! `NOT PROVEN` — `THALYX_REQUIRE_BTRFS_TESTS=1` makes it a failure. The managed
//! half runs anywhere. A case that launches a process depends on the kernel's
//! policy map being loaded, and says so in `depends_on`; on a machine where it is
//! not, both backends answer `not_proven` and are compared on that.
//!
//! `THALYX_EXP13_REPORT=<dir>` writes every observation with the platform trace
//! of each run beside it, which is what EXP-13 compares across backends for time.
//!
//! ## Recording the baseline
//!
//! ```text
//! git archive 0492f72e | tar -x -C /tmp/thalyx-0492f72
//! cargo build -p thalyx-cli --manifest-path /tmp/thalyx-0492f72/Cargo.toml
//! THALYX_EXP13_BASELINE=/tmp/thalyx-0492f72/target/debug/thalyx \
//! THALYX_BTRFS_SCRATCH=<dir on btrfs> \
//!   cargo test -p thalyx-cli --test exp13_equivalence -- --ignored record_the_baseline
//! ```
//!
//! It runs the corpus twice and refuses to write anything if the two runs of the
//! same binary differ: a recording whose noise floor is unknown is not a
//! reference.

use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use thalyx_bridge::{FromThalyx, ToThalyx, read_frame, write_frame};

const BASELINE_REVISION: &str = "0492f72e487e2463b0d7b938365a8b3383364cb9";
const CURRENT: &str = "linux-current";
const MANAGED: &str = "linux-managed";
const POLICY_MAP: &str = "/sys/fs/bpf/thalyx/maps/thalyx_policy";
const NO_BTRFS: &str = "no btrfs arena:";

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn thalyx() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_thalyx"))
}

// ── the corpus ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Case {
    #[serde(skip)]
    name: String,
    about: String,
    tree: BTreeMap<String, String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    depends_on: Vec<String>,
    agent: Vec<Ask>,
    /// Per backend, the differences from `linux-current` the design has.
    #[serde(default)]
    differences: BTreeMap<String, Vec<Declared>>,
}

#[derive(Debug, Deserialize)]
struct Ask {
    verb: String,
    #[serde(default)]
    arguments: Vec<String>,
    /// For `exec`: the program, sent as its one argument.
    #[serde(default)]
    program: Option<Value>,
    /// What the agent reads out of this answer, by JSON pointer, to decide what
    /// it sends next. `${name}` in a later request is replaced by it.
    #[serde(default)]
    bind: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Declared {
    at: String,
    why: String,
}

fn corpus() -> Vec<Case> {
    let directory = repository().join("dev/exp13/corpus");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("the corpus directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "an empty corpus proves nothing");
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("a case");
            let mut case: Case = serde_json::from_str(&text)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            case.name = path
                .file_stem()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            case
        })
        .collect()
}

// ── the arena ────────────────────────────────────────────────────────────────

enum Arena {
    Temporary(tempfile::TempDir),
    Btrfs(PathBuf),
}

impl Arena {
    fn path(&self) -> &Path {
        match self {
            Arena::Temporary(dir) => dir.path(),
            Arena::Btrfs(path) => path,
        }
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        if let Arena::Btrfs(path) = self {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// Where a backend's cases run.
///
/// `linux-current`'s boundary is a subvolume, so its arena is on Btrfs; the
/// name has a fixed width so that two runs from the same scratch directory have
/// paths of the same length, which is what lets byte counts that include a path
/// be compared at all.
fn arena(backend: &str, label: &str) -> Result<Arena, String> {
    if backend != CURRENT {
        return tempfile::tempdir()
            .map(Arena::Temporary)
            .map_err(|error| error.to_string());
    }
    let base = std::env::var("THALYX_BTRFS_SCRATCH").map_err(|_| {
        format!(
            "{NO_BTRFS} THALYX_BTRFS_SCRATCH names no directory on Btrfs, and linux-current's \
             boundary is a real subvolume"
        )
    })?;
    let root = Path::new(&base).join(format!("thalyx-exp13-{label}-{:010}", std::process::id()));
    // Never removed first: a path this did not make is not a path it may remove.
    std::fs::create_dir(&root)
        .map_err(|error| format!("{NO_BTRFS} {} could not be made: {error}", root.display()))?;
    Ok(Arena::Btrfs(root))
}

fn make_tree(backend: &str, tree: &Path) -> Result<(), String> {
    if backend == CURRENT {
        let made = Command::new("btrfs")
            .args(["subvolume", "create"])
            .arg(tree)
            .output()
            .map_err(|error| format!("{NO_BTRFS} btrfs could not be run: {error}"))?;
        if !made.status.success() {
            return Err(format!(
                "{NO_BTRFS} no subvolume could be made at {}: {}",
                tree.display(),
                String::from_utf8_lossy(&made.stderr).trim()
            ));
        }
        return Ok(());
    }
    std::fs::create_dir_all(tree).map_err(|error| error.to_string())
}

/// Take a case's root away, snapshots included.
///
/// A read-only snapshot cannot be emptied, and an unprivileged delete of a
/// subvolume is refused on a filesystem mounted without
/// `user_subvol_rm_allowed` — but a snapshot made writable by its owner can be
/// emptied and removed like a directory.
fn discard(root: &Path) {
    if let Ok(entries) = std::fs::read_dir(root.join(".thalyx-snapshots")) {
        for entry in entries.flatten() {
            let _ = Command::new("btrfs")
                .args(["property", "set", "-ts"])
                .arg(entry.path())
                .args(["ro", "false"])
                .output();
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    let _ = std::fs::remove_dir_all(root);
}

// ── talking to the machine ───────────────────────────────────────────────────

struct Server {
    child: Child,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(
    binary: &Path,
    backend: &str,
    root: &Path,
    env: &BTreeMap<String, String>,
    extra_env: &[(&str, String)],
) -> Result<(Server, UnixStream, tempfile::TempDir), String> {
    // Not in the arena. A socket's path has to fit in `sun_path`, 108 bytes, and
    // a Btrfs scratch under a home directory does not leave room for one — which
    // on 2026-09-12 made every linux-current case fail with "nothing listened",
    // a harness error that read like a machine that would not start. Where the
    // socket is changes nothing Thalyx answers: the bridge prints it only to the
    // stdout this discards.
    let sockets = tempfile::Builder::new()
        .prefix("x13")
        .tempdir_in("/tmp")
        .map_err(|error| format!("no directory for a socket: {error}"))?;
    let socket = sockets.path().join("s");
    let mut command = Command::new(binary);
    command
        .arg("bridge")
        .arg("--workspace")
        .arg(root.join("tree"))
        .arg("--listen")
        .arg(&socket)
        .env("THALYX_ROOT", root.join("store"))
        .env("THALYX_PLATFORM", backend)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Rule 5's seventeenth: whatever was typed on the command line that ran these
    // tests is in every child's environment. The limits a case runs under are the
    // case's and nobody else's.
    for inherited in [
        "THALYX_PROGRAM_SECONDS",
        "THALYX_PROGRAM_MEGABYTES",
        "THALYX_PROGRAM_CALLS",
        "THALYX_PROGRAM_LAUNCHES",
    ] {
        command.env_remove(inherited);
    }
    command.envs(env);
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("{} could not be started: {error}", binary.display()))?;
    let server = Server { child };

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match UnixStream::connect(&socket) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(600)));
                return Ok((server, stream, sockets));
            }
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                return Err(format!(
                    "nothing listened on {} within 30 s: {error}",
                    socket.display()
                ));
            }
        }
    }
}

// ── the kernel guest ──────────────────────────────────────────────────────────
//
// EXP-13's third arm boots the real Thalyx-Kernel and drives it as the machine
// side of the managed boundary. This is the harness half: it boots one guest
// per case with a fresh medium, waits until the link is serving, hands the case
// the console port the session speaks on, and shuts the guest down after.
//
// The guest is booted only when the environment names the image and the kernel
// repository's launcher; otherwise the arm is `NOT PROVEN`, the same way the
// Btrfs arm is when no scratch subvolume is named.

const KERNEL: &str = "thalyx-kernel-managed";
const NO_KERNEL: &str = "no kernel guest:";
/// The worker port a session speaks on, and the control port the harness
/// drives, as `tools/run_k1_link.py` and `thalyx_user_k5pkg::link` fix them.
const KERNEL_LINE_PORT: u32 = 1;
const KERNEL_CONTROL_PORT: u32 = 5;

/// Writes one length-prefixed frame — the grammar `thalyx_bridge` fixes.
fn write_console_frame(stream: &mut UnixStream, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    stream.write_all(&(body.len() as u32).to_le_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Reads one length-prefixed frame.
fn read_console_frame(stream: &mut UnixStream) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut header = [0u8; 4];
    stream.read_exact(&mut header)?;
    let length = u32::from_le_bytes(header) as usize;
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    Ok(body)
}

/// A booted kernel, and the console sockets it exposes.
struct Guest {
    child: Child,
    sockets: PathBuf,
    _medium: Option<tempfile::TempDir>,
}

impl Guest {
    fn line_socket(&self) -> PathBuf {
        self.sockets.join(format!("port{KERNEL_LINE_PORT}"))
    }

    fn control_socket(&self) -> PathBuf {
        self.sockets.join(format!("port{KERNEL_CONTROL_PORT}"))
    }

    /// Boots a guest with a fresh medium and waits until the link answers the
    /// control port, which is proof the link is up and moving bytes on a port.
    fn boot(label: &str) -> Result<Guest, String> {
        let medium = tempfile::Builder::new()
            .prefix("k1med")
            .tempdir_in("/tmp")
            .map_err(|error| format!("{NO_KERNEL} no directory for the medium: {error}"))?;
        let medium_file = medium.path().join("medium.img");
        Guest::boot_on(label, &medium_file, true, Some(medium))
    }

    /// Boots a guest on a named medium, fresh or reused. A reused medium is how
    /// durable recovery is shown: publish, power the machine off, boot again on
    /// the same bytes, and read the version back.
    fn boot_on(
        label: &str,
        medium_file: &Path,
        fresh: bool,
        owner: Option<tempfile::TempDir>,
    ) -> Result<Guest, String> {
        let launcher = std::env::var("THALYX_K1_LAUNCHER").map_err(|_| {
            format!(
                "{NO_KERNEL} THALYX_K1_LAUNCHER names no `tools/run_k1_link.py` of the kernel \
                 repository"
            )
        })?;
        let image = std::env::var("THALYX_K1_IMAGE").map_err(|_| {
            format!("{NO_KERNEL} THALYX_K1_IMAGE names no built `thalyx-k5.img` (stage thalyx)")
        })?;
        if !Path::new(&image).exists() {
            return Err(format!("{NO_KERNEL} {image} is not there"));
        }
        let sockets = tempfile::Builder::new()
            .prefix("k1sock")
            .tempdir_in("/tmp")
            .map_err(|error| format!("{NO_KERNEL} no directory for the sockets: {error}"))?;
        let sockets_path = sockets.path().to_path_buf();
        let out = medium_file
            .parent()
            .unwrap_or(Path::new("/tmp"))
            .join(format!("run-{label}"));
        let mut command = Command::new("python3");
        command
            .arg(&launcher)
            .arg("--image")
            .arg(&image)
            .arg("--medium")
            .arg(medium_file)
            .arg("--sockets")
            .arg(&sockets_path)
            .arg("--out")
            .arg(out)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if fresh {
            command.arg("--fresh-medium");
        }
        let child = command
            .spawn()
            .map_err(|error| format!("{NO_KERNEL} the launcher could not be started: {error}"))?;
        // Keep the sockets directory alive for as long as the guest: leaking it
        // into the guest is what lets the control port be reached at drop.
        let sockets_dir = sockets.keep();
        let guest = Guest {
            child,
            sockets: sockets_dir,
            _medium: owner,
        };

        // Ready when the control port answers `stats`. A guest under TCG takes
        // a few seconds to boot and stand its services up; the deadline is
        // generous because a slow host, not a broken guest, is what makes it
        // long.
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if Instant::now() >= deadline {
                return Err(format!(
                    "{NO_KERNEL} the guest did not answer its control port within 180 s"
                ));
            }
            if let Ok(mut control) = UnixStream::connect(guest.control_socket()) {
                let _ = control.set_read_timeout(Some(Duration::from_secs(5)));
                if write_console_frame(&mut control, br#"{"op":"stats"}"#).is_ok()
                    && read_console_frame(&mut control).is_ok()
                {
                    return Ok(guest);
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Sends a control-port request and reads its reply.
    fn control(&self, request: &[u8]) -> Result<Value, String> {
        let mut stream = UnixStream::connect(self.control_socket())
            .map_err(|error| format!("the control port could not be reached: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| error.to_string())?;
        write_console_frame(&mut stream, request).map_err(|error| error.to_string())?;
        let reply = read_console_frame(&mut stream).map_err(|error| error.to_string())?;
        serde_json::from_slice(&reply).map_err(|error| error.to_string())
    }
}

impl Drop for Guest {
    fn drop(&mut self) {
        // Ask the guest to end, which powers the machine off, then wait for the
        // launcher to exit; kill it if it will not.
        if let Ok(mut control) = UnixStream::connect(self.control_socket()) {
            let _ = write_console_frame(&mut control, br#"{"op":"shutdown"}"#);
            let _ = read_console_frame(&mut control);
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.sockets);
    }
}

fn ask(
    stream: &mut UnixStream,
    id: usize,
    verb: &str,
    arguments: &[String],
) -> Result<Value, String> {
    let request = ToThalyx::Request {
        id: id.to_string(),
        verb: verb.to_string(),
        arguments: arguments.to_vec(),
    };
    write_frame(stream, &request.encode()).map_err(|error| error.to_string())?;
    let body = read_frame(stream).map_err(|error| format!("`{verb}` got no answer: {error}"))?;
    match FromThalyx::decode(&body).map_err(|error| error.to_string())? {
        FromThalyx::Response { answer, .. } => Ok(answer),
        // A refusal on the wire is an answer, and the corpus compares it.
        FromThalyx::Error {
            word,
            remedy,
            message,
            ..
        } => Ok(json!({"refused": word, "remedy": remedy, "message": message})),
        FromThalyx::Hello { .. } => Err(format!("`{verb}` was answered with a second hello")),
    }
}

// ── observing ────────────────────────────────────────────────────────────────

/// What varies between two honest runs and says nothing about Thalyx: where the
/// arena is, the clock, and the request ids the machine mints.
struct Scrub {
    paths: Vec<(String, &'static str)>,
}

impl Scrub {
    fn new(root: &Path) -> Self {
        let mut paths = Vec::new();
        for (path, word) in [
            (root.join("tree"), "<tree>"),
            (root.join("store"), "<store>"),
            (root.to_path_buf(), "<root>"),
        ] {
            if let Ok(real) = std::fs::canonicalize(&path) {
                paths.push((real.display().to_string(), word));
            }
            paths.push((path.display().to_string(), word));
        }
        if let Ok(home) = std::env::var("HOME")
            && home.len() > 1
        {
            paths.push((home, "<home>"));
        }
        paths.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
        Self { paths }
    }

    fn text(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (path, word) in &self.paths {
            out = out.replace(path.as_str(), word);
        }
        tokens(&managed_workspace(&out))
    }

    fn value(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(text)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| self.value(item)).collect())
            }
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(key, field)| (self.text(key), self.value(field)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

/// The managed backend's private workspace is where its work's requests act, so
/// the paths it answers with are the same tree the other backend names directly.
fn managed_workspace(text: &str) -> String {
    const PREFIX: &str = "<store>/state/managed/";
    const SUFFIX: &str = "/workspaces/session";
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(PREFIX) {
        let after = &rest[at + PREFIX.len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if !name.is_empty() && after[name.len()..].starts_with(SUFFIX) {
            out.push_str(&rest[..at]);
            out.push_str("<tree>");
            rest = &after[name.len() + SUFFIX.len()..];
        } else {
            out.push_str(&rest[..at + PREFIX.len()]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if let Some((length, word)) = token_at(&text[index..]) {
            out.push_str(word);
            index += length;
            continue;
        }
        let character = text[index..].chars().next().expect("a character");
        out.push(character);
        index += character.len_utf8();
    }
    out
}

fn token_at(rest: &str) -> Option<(usize, &'static str)> {
    let hex = |text: &str, n: usize| {
        text.len() >= n
            && text.as_bytes()[..n]
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    };
    if let Some(after) = rest.strip_prefix("req-")
        && after.len() >= 36
        && after.as_bytes()[..36]
            .iter()
            .all(|b| b.is_ascii_hexdigit() || *b == b'-')
    {
        return Some((40, "req-<request>"));
    }
    if let Some(after) = rest.strip_prefix("w2-")
        && hex(after, 64)
    {
        return Some((67, "w2-<witness>"));
    }
    // The validation cache's identity for a tree. Measured before it was scrubbed,
    // not assumed: on 2026-09-12 two runs of the *same* binary over the same
    // corpus, from paths of the same length, answered `a-rust-check` with two
    // different `k1-` identities — it varies from run to run the way a `w2-`
    // witness does, so it says nothing about which binary answered.
    if let Some(after) = rest.strip_prefix("k1-")
        && hex(after, 64)
    {
        return Some((67, "k1-<identity>"));
    }
    if let Some(after) = rest.strip_prefix("ctx-")
        && hex(after, 12)
    {
        return Some((16, "ctx-<handle>"));
    }
    // `2026-09-12T21:52:32.746024954Z`, and the snapshot-name spelling of the
    // same instant, `2026-09-12T21-52-32-746109554Z`.
    let b = rest.as_bytes();
    if b.len() >= 20
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && b[10] == b'T'
    {
        let mut end = 11;
        while end < b.len() && (b[end].is_ascii_digit() || b"-:.".contains(&b[end])) {
            end += 1;
        }
        if end < b.len() && b[end] == b'Z' {
            return Some((end + 1, "<time>"));
        }
    }
    None
}

/// What the tree holds, walked here and hashed here.
fn tree_digest(tree: &Path) -> Value {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;

    fn walk(root: &Path, directory: &Path, lines: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            lines.push(format!("? {}", directory.display()));
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                lines.push(format!("? {name}"));
                continue;
            };
            if meta.is_dir() {
                lines.push(format!("d {name}"));
                walk(root, &path, lines);
            } else if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&path)
                    .map(|target| target.display().to_string())
                    .unwrap_or_default();
                lines.push(format!("l {name} {target}"));
            } else if meta.is_file() {
                let bytes = std::fs::read(&path).unwrap_or_default();
                let executable = if meta.permissions().mode() & 0o111 != 0 {
                    "x"
                } else {
                    "-"
                };
                lines.push(format!(
                    "f {name} {} {executable}",
                    hex::encode(Sha256::digest(&bytes))
                ));
            } else {
                lines.push(format!("o {name}"));
            }
        }
    }

    let mut lines = Vec::new();
    walk(tree, tree, &mut lines);
    lines.sort();
    let mut hasher = Sha256::new();
    for line in &lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    json!({"digest": hex::encode(hasher.finalize()), "entries": lines})
}

fn journal(store: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(store.join("journal.jsonl")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|entry| {
            let outcome = match &entry["outcome"] {
                Value::String(word) => word.clone(),
                Value::Object(fields) => fields.keys().next().cloned().unwrap_or_default(),
                other => other.to_string(),
            };
            format!(
                "{}:{outcome}",
                entry["operation"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

/// The platform trace a run left in its evidence. For the report only: nothing
/// is judged from it, because a timing is not an outcome.
fn trace(backend: &str, store: &Path, handle: &str) -> Value {
    let body = if backend == CURRENT {
        std::fs::read(store.join("state/evidence").join(format!("{handle}.json"))).ok()
    } else {
        std::fs::read_dir(store.join("state/managed"))
            .ok()
            .and_then(|entries| {
                entries.flatten().find_map(|entry| {
                    match thalyx_managed::read_evidence(&entry.path().join("store"), handle) {
                        thalyx_platform::evidence::Fetched::Found(bytes) => Some(bytes),
                        _ => None,
                    }
                })
            })
    };
    body.and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|evidence| evidence.pointer("/platform/trace").cloned())
        .unwrap_or(Value::Null)
}

struct Ran {
    observation: Value,
    traces: Vec<Value>,
}

fn bound(text: &str, bindings: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = text.to_string();
    for (name, value) in bindings {
        // Escaped the way JSON escapes a string, because the text it lands in may
        // be a program inside a JSON object.
        let escaped = serde_json::to_string(value).expect("a string");
        out = out.replace(&format!("${{{name}}}"), &escaped[1..escaped.len() - 1]);
    }
    if let Some(at) = out.find("${") {
        return Err(format!(
            "the agent was to fill in `{}` and nothing it read named it",
            &out[at..(at + 32).min(out.len())]
        ));
    }
    Ok(out)
}

fn run_case(binary: &Path, backend: &str, root: &Path, case: &Case) -> Result<Ran, String> {
    run_case_with(binary, backend, root, case, None)
}

fn run_case_with(
    binary: &Path,
    backend: &str,
    root: &Path,
    case: &Case,
    kernel: Option<&Guest>,
) -> Result<Ran, String> {
    let tree = root.join("tree");
    make_tree(backend, &tree)?;
    for (path, text) in &case.tree {
        let full = tree.join(path);
        std::fs::create_dir_all(full.parent().expect("a parent")).map_err(|e| e.to_string())?;
        std::fs::write(&full, text).map_err(|e| e.to_string())?;
    }

    // The kernel-managed backend speaks its managed protocol on one console
    // port of the running guest; the host side connects to that socket.
    let extra_env: Vec<(&str, String)> = match kernel {
        Some(guest) => vec![(
            "THALYX_KERNEL_SOCKET",
            guest.line_socket().display().to_string(),
        )],
        None => Vec::new(),
    };

    // The socket's directory is held for as long as the server is, and dropped
    // after it.
    let (server, mut stream, _sockets) = serve(binary, backend, root, &case.env, &extra_env)?;
    let hello = read_frame(&mut stream).map_err(|error| format!("no hello: {error}"))?;
    if !matches!(FromThalyx::decode(&hello), Ok(FromThalyx::Hello { .. })) {
        return Err(format!(
            "the bridge did not say hello: {}",
            String::from_utf8_lossy(&hello)
        ));
    }

    // What this machine can resolve and compile with, asked the way a harness
    // asks before it spends anything: recorded answers are only comparable with a
    // machine that answers this the same way.
    let toolchain = ask(&mut stream, 0, "toolchain", &[])?;

    let mut bindings = BTreeMap::new();
    let mut requests = Vec::new();
    let mut traces = Vec::new();
    for (index, step) in case.agent.iter().enumerate() {
        let arguments = match &step.program {
            Some(program) => vec![bound(&program.to_string(), &bindings)?],
            None => step
                .arguments
                .iter()
                .map(|argument| bound(argument, &bindings))
                .collect::<Result<_, _>>()?,
        };
        let answer = ask(&mut stream, index + 1, &step.verb, &arguments)?;
        for (name, pointer) in &step.bind {
            let value = answer
                .pointer(pointer)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "request {index} (`{}`): the agent reads `{pointer}` and the answer has no \
                     such thing: {answer}",
                        step.verb
                    )
                })?;
            bindings.insert(name.clone(), value.to_string());
        }
        let mut record = json!({"verb": step.verb, "arguments": arguments, "answer": answer});
        if step.verb == "exec"
            && let Some(handle) = answer.get("evidence").and_then(Value::as_str)
        {
            record["evidence"] = ask(&mut stream, index + 1, "evidence", &[handle.to_string()])?;
            traces.push(json!({
                "request": index,
                "trace": trace(backend, &root.join("store"), handle),
            }));
        }
        requests.push(record);
    }
    drop(stream);
    drop(server);

    let scrub = Scrub::new(root);
    Ok(Ran {
        observation: json!({
            "case": case.name,
            "backend": backend,
            "root_length": root.display().to_string().len(),
            "machine": {
                "toolchain": scrub.value(&toolchain),
                "policy_map_loaded": Path::new(POLICY_MAP).exists(),
            },
            "requests": scrub.value(&Value::Array(requests)),
            "tree": tree_digest(&tree),
            "journal": journal(&root.join("store")),
        }),
        traces,
    })
}

/// Run the whole corpus on one binary and one backend.
fn observe(binary: &Path, backend: &str, label: &str) -> Result<BTreeMap<String, Ran>, String> {
    let arena = arena(backend, label)?;
    let mut observed = BTreeMap::new();
    for case in corpus() {
        let root = arena.path().join(&case.name);
        std::fs::create_dir(&root).map_err(|error| format!("{}: {error}", root.display()))?;
        let ran = run_case(binary, backend, &root, &case);
        if backend == CURRENT {
            discard(&root);
        }
        let ran = ran.map_err(|why| format!("{} on {backend}: {why}", case.name))?;
        observed.insert(case.name.clone(), ran);
    }

    if let Ok(directory) = std::env::var("THALYX_EXP13_REPORT") {
        let directory = Path::new(&directory).join(format!("{backend}-{label}"));
        let _ = std::fs::create_dir_all(&directory);
        for (name, ran) in &observed {
            let _ = std::fs::write(
                directory.join(format!("{name}.json")),
                serde_json::to_string_pretty(&json!({
                    "binary": binary.display().to_string(),
                    "observation": ran.observation,
                    "traces": ran.traces,
                }))
                .expect("JSON"),
            );
        }
    }
    Ok(observed)
}

/// Run the whole corpus on the kernel-managed backend, a fresh guest per case.
///
/// A guest per case is what keeps a case's line empty, exactly as a fresh
/// tempdir store does for `linux-managed`: the medium starts zeroed and the
/// session speaks on the one worker port, so what the case sees is a store with
/// nothing but what the case published.
fn observe_kernel(binary: &Path, label: &str) -> Result<BTreeMap<String, Ran>, String> {
    let arena = arena(KERNEL, label)?;
    let mut observed = BTreeMap::new();
    for case in corpus() {
        let root = arena.path().join(&case.name);
        std::fs::create_dir(&root).map_err(|error| format!("{}: {error}", root.display()))?;
        let guest = Guest::boot(&case.name)?;
        let ran = run_case_with(binary, KERNEL, &root, &case, Some(&guest));
        // What the kernel accounted for the run, for the report: the guest's
        // own numbers, read over the control port before it is shut down.
        let stats = guest.control(br#"{"op":"stats"}"#).ok();
        drop(guest);
        let mut ran = ran.map_err(|why| format!("{} on {KERNEL}: {why}", case.name))?;
        if let (Some(object), Some(stats)) = (ran.observation.as_object_mut(), stats) {
            object.insert("kernel".to_string(), stats);
        }
        observed.insert(case.name.clone(), ran);
    }

    if let Ok(directory) = std::env::var("THALYX_EXP13_REPORT") {
        let directory = Path::new(&directory).join(format!("{KERNEL}-{label}"));
        let _ = std::fs::create_dir_all(&directory);
        for (name, ran) in &observed {
            let _ = std::fs::write(
                directory.join(format!("{name}.json")),
                serde_json::to_string_pretty(&json!({
                    "binary": binary.display().to_string(),
                    "observation": ran.observation,
                    "traces": ran.traces,
                }))
                .expect("JSON"),
            );
        }
    }
    Ok(observed)
}

// ── comparing ────────────────────────────────────────────────────────────────

/// Every path at which two answers differ. A field present on one side and
/// absent on the other is a difference: Thalyx says `null` and says nothing on
/// purpose, and those are two answers.
fn differences(left: &Value, right: &Value, at: &str, into: &mut Vec<String>) {
    let join = |key: &str| {
        if at.is_empty() {
            key.to_string()
        } else {
            format!("{at}.{key}")
        }
    };
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                match (a.get(key), b.get(key)) {
                    (Some(x), Some(y)) => differences(x, y, &join(key), into),
                    _ => into.push(join(key)),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for index in 0..a.len().max(b.len()) {
                match (a.get(index), b.get(index)) {
                    (Some(x), Some(y)) => differences(x, y, &join(&index.to_string()), into),
                    _ => into.push(join(&index.to_string())),
                }
            }
        }
        _ if left == right => {}
        _ => into.push(at.to_string()),
    }
}

fn remove(value: &mut Value, pointer: &str, field: &str) {
    if let Some(Value::Object(fields)) = value.pointer_mut(pointer) {
        fields.remove(field);
    }
}

/// Everything that is not a clock, and byte counts only when the arena paths
/// have the same length.
fn exact(observation: &Value, with_bytes: bool) -> Value {
    let mut value = observation.clone();
    for field in ["backend", "root_length"] {
        remove(&mut value, "", field);
    }
    let count = value["requests"].as_array().map(Vec::len).unwrap_or(0);
    for index in 0..count {
        remove(
            &mut value,
            &format!("/requests/{index}/evidence/metrics"),
            "machine_time_ms",
        );
        if !with_bytes {
            for place in ["answer", "evidence"] {
                for field in ["internal_bytes", "returned_bytes"] {
                    remove(
                        &mut value,
                        &format!("/requests/{index}/{place}/metrics"),
                        field,
                    );
                }
            }
            if let Some(Value::Array(steps)) =
                value.pointer_mut(&format!("/requests/{index}/evidence/steps"))
            {
                for step in steps {
                    remove(step, "", "answer_bytes");
                }
            }
        }
    }
    value
}

/// What a transaction means, without what a backend is.
///
/// Out: byte counts (the private workspace is a longer path), the two state
/// identities (a witness on one backend and a content identity on the other —
/// whether each is present is kept), the journal (each backend writes its own
/// operations) and the machine fingerprint (both ran on the same machine).
fn semantic(observation: &Value) -> Value {
    let mut value = exact(observation, false);
    for field in ["machine", "journal", "kernel"] {
        remove(&mut value, "", field);
    }
    let count = value["requests"].as_array().map(Vec::len).unwrap_or(0);
    for index in 0..count {
        if let Some(Value::Object(evidence)) =
            value.pointer_mut(&format!("/requests/{index}/evidence"))
        {
            for field in ["start_state", "end_state"] {
                if let Some(state) = evidence.get_mut(field) {
                    *state = json!(state.is_string());
                }
            }
        }
    }
    value
}

fn goldens() -> BTreeMap<String, Value> {
    let directory = repository().join("dev/exp13/baseline");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return BTreeMap::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .map(|entry| {
            let text = std::fs::read_to_string(entry.path()).expect("a recording");
            let value: Value = serde_json::from_str(&text).expect("a recording is JSON");
            (
                entry
                    .path()
                    .file_stem()
                    .expect("a name")
                    .to_string_lossy()
                    .into_owned(),
                value["observation"].clone(),
            )
        })
        .collect()
}

/// Why recorded answers cannot stand in for this machine, if they cannot.
fn not_this_machine(
    recorded: &BTreeMap<String, Value>,
    live: &BTreeMap<String, Ran>,
) -> Option<String> {
    let recorded_machine = recorded
        .values()
        .next()
        .map(|observation| &observation["machine"]);
    let live_machine = live.values().next().map(|ran| &ran.observation["machine"]);
    match (recorded_machine, live_machine) {
        (None, _) => Some("there are no recorded answers in dev/exp13/baseline".to_string()),
        (Some(recorded), Some(live)) if recorded != live => {
            let mut paths = Vec::new();
            differences(recorded, live, "machine", &mut paths);
            Some(format!(
                "the answers were recorded on a machine that differs from this one at {}",
                paths.join(", ")
            ))
        }
        _ => None,
    }
}

fn not_proven(what: &str, why: &str, variable: &str) {
    let message = format!("NOT PROVEN: {what} — {why}. Set {variable}=1 to make this a failure.");
    assert!(std::env::var(variable).as_deref() != Ok("1"), "{message}");
    eprintln!("{message}");
}

fn observations(runs: BTreeMap<String, Ran>) -> BTreeMap<String, Value> {
    runs.into_iter()
        .map(|(name, ran)| (name, ran.observation))
        .collect()
}

// ── the claims ───────────────────────────────────────────────────────────────

#[test]
fn linux_current_answers_what_the_unmodified_revision_answered() {
    let current = match observe(&thalyx(), CURRENT, "current") {
        Ok(current) => current,
        Err(why) if why.contains(NO_BTRFS) => {
            return not_proven(
                "the refactored linux-current was not compared with 0492f72",
                &why,
                "THALYX_REQUIRE_BTRFS_TESTS",
            );
        }
        Err(why) => panic!("{why}"),
    };

    let (reference, against) = match std::env::var("THALYX_EXP13_BASELINE") {
        Ok(binary) => (
            observations(
                observe(Path::new(&binary), CURRENT, "baselin")
                    .unwrap_or_else(|why| panic!("the baseline binary: {why}")),
            ),
            format!("{binary}, run beside it"),
        ),
        Err(_) => {
            let recorded = goldens();
            if let Some(why) = not_this_machine(&recorded, &current) {
                return not_proven(
                    "the refactored linux-current was not compared with 0492f72",
                    &format!(
                        "{why}, and THALYX_EXP13_BASELINE names no baseline binary to run beside it"
                    ),
                    "THALYX_REQUIRE_EXP13_BASELINE",
                );
            }
            (recorded, "the answers recorded from 0492f72".to_string())
        }
    };

    let mut failures = Vec::new();
    for case in corpus() {
        let ours = &current[&case.name].observation;
        let Some(theirs) = reference.get(&case.name) else {
            failures.push(format!("{}: nothing to compare it with", case.name));
            continue;
        };
        let with_bytes = ours["root_length"] == theirs["root_length"];
        let mut paths = Vec::new();
        differences(
            &exact(theirs, with_bytes),
            &exact(ours, with_bytes),
            "",
            &mut paths,
        );
        if paths.is_empty() {
            eprintln!(
                "PROVEN {}: linux-current answered what {BASELINE_REVISION:.7} answered{}",
                case.name,
                if with_bytes {
                    ""
                } else {
                    " (byte counts not compared: the arena paths differ in length)"
                }
            );
        } else {
            failures.push(format!(
                "{} — {}: differs at {}",
                case.name,
                case.about,
                paths.join(", ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "linux-current is not what {against} was:\n{}",
        failures.join("\n")
    );
}

#[test]
fn linux_managed_carries_out_the_corpus_and_differs_only_where_the_design_says() {
    let managed = observe(&thalyx(), MANAGED, "managed").unwrap_or_else(|why| panic!("{why}"));

    let (reference, against) = match observe(&thalyx(), CURRENT, "referen") {
        Ok(current) => (observations(current), "linux-current, run beside it"),
        Err(why) if why.contains(NO_BTRFS) => {
            let recorded = goldens();
            if let Some(mismatch) = not_this_machine(&recorded, &managed) {
                return not_proven(
                    "linux-managed ran the corpus and was compared with nothing",
                    &format!("{why}; and {mismatch}"),
                    "THALYX_REQUIRE_BTRFS_TESTS",
                );
            }
            (
                recorded,
                "the answers recorded from 0492f72's linux-current",
            )
        }
        Err(why) => panic!("{why}"),
    };

    let mut failures = Vec::new();
    for case in corpus() {
        let ours = semantic(&managed[&case.name].observation);
        let Some(theirs) = reference.get(&case.name) else {
            failures.push(format!("{}: nothing to compare it with", case.name));
            continue;
        };
        let mut paths = Vec::new();
        differences(&semantic(theirs), &ours, "", &mut paths);

        let declared = case
            .differences
            .get(MANAGED)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let covered = |path: &str, declaration: &Declared| {
            path == declaration.at || path.starts_with(&format!("{}.", declaration.at))
        };
        let undeclared: Vec<&String> = paths
            .iter()
            .filter(|path| {
                !declared
                    .iter()
                    .any(|declaration| covered(path, declaration))
            })
            .collect();
        let stale: Vec<&Declared> = declared
            .iter()
            .filter(|declaration| !paths.iter().any(|path| covered(path, declaration)))
            .collect();

        if undeclared.is_empty() && stale.is_empty() {
            let excused: Vec<String> = declared
                .iter()
                .map(|declaration| format!("{} ({})", declaration.at, declaration.why))
                .collect();
            eprintln!(
                "PROVEN {}: linux-managed meant what linux-current meant{}{}",
                case.name,
                if excused.is_empty() {
                    String::new()
                } else {
                    format!(", except as declared: {}", excused.join("; "))
                },
                if case.depends_on.is_empty() {
                    String::new()
                } else {
                    format!(" [depends on: {}]", case.depends_on.join(", "))
                }
            );
        } else {
            if !undeclared.is_empty() {
                failures.push(format!(
                    "{}: differs where nothing says it may, at {}",
                    case.name,
                    undeclared
                        .iter()
                        .map(|path| path.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            for declaration in stale {
                failures.push(format!(
                    "{}: declares a difference at `{}` that did not happen ({})",
                    case.name, declaration.at, declaration.why
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "linux-managed against {against}:\n{}",
        failures.join("\n")
    );
}

#[test]
fn thalyx_kernel_managed_means_what_linux_managed_means() {
    // The third arm: the same managed transaction, over a console port of the
    // running Thalyx-Kernel, against its K4 state service on a real medium. It
    // is held to `linux-managed` rather than to `linux-current`, because K1 is
    // the managed model and the question is whether putting it on the kernel
    // changed what a transaction means. Anything that differs must be declared
    // under `thalyx-kernel-managed` in the case, and a declaration that did not
    // happen is as much a failure as an undeclared difference.
    let kernel = match observe_kernel(&thalyx(), "kernel") {
        Ok(kernel) => kernel,
        Err(why) if why.contains(NO_KERNEL) => {
            return not_proven(
                "the kernel-managed backend ran the corpus and was compared with nothing",
                &why,
                "THALYX_REQUIRE_K1",
            );
        }
        Err(why) => panic!("{why}"),
    };
    let managed = observe(&thalyx(), MANAGED, "referen").unwrap_or_else(|why| panic!("{why}"));

    let mut failures = Vec::new();
    for case in corpus() {
        let ours = semantic(&kernel[&case.name].observation);
        let theirs = semantic(&managed[&case.name].observation);
        let mut paths = Vec::new();
        differences(&theirs, &ours, "", &mut paths);

        let declared = case
            .differences
            .get(KERNEL)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let covered = |path: &str, declaration: &Declared| {
            path == declaration.at || path.starts_with(&format!("{}.", declaration.at))
        };
        let undeclared: Vec<&String> = paths
            .iter()
            .filter(|path| {
                !declared
                    .iter()
                    .any(|declaration| covered(path, declaration))
            })
            .collect();
        let stale: Vec<&Declared> = declared
            .iter()
            .filter(|declaration| !paths.iter().any(|path| covered(path, declaration)))
            .collect();

        if undeclared.is_empty() && stale.is_empty() {
            eprintln!(
                "PROVEN {}: thalyx-kernel-managed meant what linux-managed meant{}",
                case.name,
                if declared.is_empty() {
                    String::new()
                } else {
                    let excused: Vec<String> = declared
                        .iter()
                        .map(|d| format!("{} ({})", d.at, d.why))
                        .collect();
                    format!(", except as declared: {}", excused.join("; "))
                }
            );
        } else {
            if !undeclared.is_empty() {
                failures.push(format!(
                    "{}: differs where nothing says it may, at {}",
                    case.name,
                    undeclared
                        .iter()
                        .map(|p| p.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            for declaration in stale {
                failures.push(format!(
                    "{}: declares a difference at `{}` that did not happen ({})",
                    case.name, declaration.at, declaration.why
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "thalyx-kernel-managed against linux-managed, run beside it:\n{}",
        failures.join("\n")
    );
}

#[test]
#[ignore = "records dev/exp13/baseline from the unmodified binary; run by hand as the module documentation says"]
fn record_the_baseline() {
    use sha2::{Digest, Sha256};

    let binary = std::env::var("THALYX_EXP13_BASELINE")
        .expect("THALYX_EXP13_BASELINE names the binary built from 0492f72");
    let binary = Path::new(&binary);
    let first = observe(binary, CURRENT, "current").unwrap_or_else(|why| panic!("{why}"));
    let second = observe(binary, CURRENT, "current").unwrap_or_else(|why| panic!("{why}"));

    // The noise floor, before anything is written.
    let mut noisy = Vec::new();
    for (name, ran) in &first {
        let mut paths = Vec::new();
        differences(
            &exact(&ran.observation, true),
            &exact(&second[name].observation, true),
            "",
            &mut paths,
        );
        if !paths.is_empty() {
            noisy.push(format!("{name}: {}", paths.join(", ")));
        }
    }
    assert!(
        noisy.is_empty(),
        "two runs of the same binary answered differently, so neither is a reference:\n{}",
        noisy.join("\n")
    );

    let digest = hex::encode(Sha256::digest(std::fs::read(binary).expect("the binary")));
    let directory = repository().join("dev/exp13/baseline");
    std::fs::create_dir_all(&directory).expect("the recording directory");
    for (name, ran) in first {
        let recording = json!({
            "recorded": {
                "revision": BASELINE_REVISION,
                "binary_sha256": digest,
                "backend": CURRENT,
                "runs_compared": 2,
            },
            "observation": ran.observation,
        });
        std::fs::write(
            directory.join(format!("{name}.json")),
            serde_json::to_string_pretty(&recording).expect("JSON") + "\n",
        )
        .expect("a recording");
    }
}

#[test]
fn what_varies_between_honest_runs_is_scrubbed_and_nothing_else_is() {
    // A real answer, captured from 0492f72 over the bridge on 2026-09-12 (rule 6):
    // a request id, a timestamp and a witness, next to text that must survive.
    let captured = r#"{"at": "2026-09-12T21:52:32.746024954Z", "transaction": "req-65a65ad8-2678-48cf-b8e2-6d2543da7787", "end_state": "w2-7683e52773a1b50fbab4cb25f61f548294f7421253eb45114ebf7e38a0a9a619", "snapshot": "2026-09-12T21-52-32-746109554Z-probe", "handle": "ctx-8ca10265b285", "reason": "the program threw: TypeError: not a function"}"#;
    let root = Path::new("/nonexistent/exp13");
    let scrubbed = Scrub::new(root).value(&serde_json::from_str(captured).expect("JSON"));
    assert_eq!(scrubbed["at"], json!("<time>"));
    assert_eq!(scrubbed["transaction"], json!("req-<request>"));
    assert_eq!(scrubbed["end_state"], json!("w2-<witness>"));
    assert_eq!(scrubbed["snapshot"], json!("<time>-probe"));
    assert_eq!(scrubbed["handle"], json!("ctx-<handle>"));
    let identity = Scrub::new(root)
        .text("state k1-4429b88b78908fadb39504982a99efd80f626665d4cecedb26ead18187d80b38 held");
    assert_eq!(identity, "state k1-<identity> held");
    assert_eq!(
        scrubbed["reason"],
        json!("the program threw: TypeError: not a function")
    );
    assert_eq!(
        managed_workspace(
            "at <store>/state/managed/0123456789abcdef01234567/workspaces/session/src"
        ),
        "at <tree>/src"
    );
}

// ── the kernel-specific adversarial gates ─────────────────────────────────────
//
// The equivalence corpus already exercises publication success, a validation
// failure that puts the tree back, and a failure kept on request — on K1 as on
// every backend, because it runs the same corpus. Three properties are the
// kernel's own and are checked here, over the same console transport the real
// Thalyx uses, by driving the shared managed client directly: a stale
// publication conflict, a fenced work that cannot publish, and a version that
// survives the machine being powered off and booted again.
//
// These are `NOT PROVEN` unless a kernel image and launcher are named, the same
// way the Btrfs arm is when no scratch subvolume is named.

use thalyx_platform::managed::client::Managed as KernelClient;
use thalyx_platform::state::{Decision, Receipt, VersionedState};
use thalyx_platform::transport::{Carried, Transport, TransportError};

/// The managed protocol over one console port, for driving the client from a
/// test. The same length-prefixed frames the backend uses.
struct TestConsole {
    stream: UnixStream,
    carried: Carried,
}

impl TestConsole {
    fn connect(socket: &Path) -> Result<Self, String> {
        let stream = UnixStream::connect(socket)
            .map_err(|error| format!("the console port could not be reached: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            stream,
            carried: Carried::default(),
        })
    }
}

impl Transport for TestConsole {
    fn call(&mut self, request: &[u8]) -> Result<Vec<u8>, TransportError> {
        self.carried.calls += 1;
        write_console_frame(&mut self.stream, request)
            .map_err(|error| TransportError::Gone(error.to_string()))?;
        read_console_frame(&mut self.stream)
            .map_err(|error| TransportError::Gone(error.to_string()))
    }

    fn carried(&self) -> Carried {
        self.carried
    }
}

/// A view and a private workspace on the host, seeded with one file. The view
/// is what a publication lands in; the workspace is where a transaction acts.
struct Session {
    _dir: tempfile::TempDir,
    client: KernelClient<TestConsole>,
    workspace: PathBuf,
}

fn session(socket: &Path, principal: &str, seed_file: &str, seed: &str) -> Result<Session, String> {
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let view = dir.path().join("view");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&view).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&workspace).map_err(|error| error.to_string())?;
    std::fs::write(view.join(seed_file), seed).map_err(|error| error.to_string())?;
    let transport = TestConsole::connect(socket)?;
    let client = KernelClient::new(transport, view.clone(), workspace.clone(), principal);
    Ok(Session {
        _dir: dir,
        client,
        workspace,
    })
}

fn commit_receipt(request_id: &str) -> Receipt {
    Receipt {
        transaction: request_id.to_string(),
        label: "adversarial".to_string(),
        decision: Decision::Commit,
        succeeded: true,
        checks: Vec::new(),
        program: None,
    }
}

#[test]
fn thalyx_kernel_refuses_a_stale_publication() {
    let guest = match Guest::boot("stale") {
        Ok(guest) => guest,
        Err(why) if why.contains(NO_KERNEL) => {
            return not_proven(
                "a stale publication was not refused",
                &why,
                "THALYX_REQUIRE_K1",
            );
        }
        Err(why) => panic!("{why}"),
    };
    // Two ports on one line: two principals contend on one generation.
    let port1 = guest.line_socket();
    let port2 = guest.sockets.join("port2");
    guest
        .control(br#"{"op":"bind","port":2,"line":1}"#)
        .expect("bind port 2 to line 1");

    let mut a = session(&port1, "a", "src/lib.rs", "pub fn one() {}\n").expect("session a");
    let mut b = session(&port2, "b", "src/lib.rs", "pub fn one() {}\n").expect("session b");

    // A seeds generation 1 and forks it; B forks the same generation 1 before A
    // publishes, so both hold work over the same version.
    a.client.open("a", "req-a").expect("a opens");
    b.client.open("b", "req-b").expect("b opens");

    std::fs::write(
        a.workspace.join("src/lib.rs"),
        "pub fn one() {}\npub fn a() {}\n",
    )
    .expect("a edits");
    std::fs::write(
        b.workspace.join("src/lib.rs"),
        "pub fn one() {}\npub fn b() {}\n",
    )
    .expect("b edits");
    a.client
        .candidate()
        .expect("a freezes")
        .expect("a candidate");
    b.client
        .candidate()
        .expect("b freezes")
        .expect("b candidate");

    let published = a
        .client
        .keep(&commit_receipt("req-a"))
        .expect("a publishes");
    assert_eq!(
        published.generation,
        Some(2),
        "a moved the line to generation 2"
    );

    let stale = b
        .client
        .keep(&commit_receipt("req-b"))
        .expect_err("b's publication is stale and must be refused");
    assert!(
        stale.contains("no longer the published one") || stale.to_lowercase().contains("stale"),
        "the refusal should say the generation moved: {stale}"
    );
    eprintln!("PROVEN stale: the kernel refused a publication against a generation that moved");
}

#[test]
fn thalyx_kernel_fences_a_work_so_it_cannot_publish() {
    let guest = match Guest::boot("fence") {
        Ok(guest) => guest,
        Err(why) if why.contains(NO_KERNEL) => {
            return not_proven("a fenced work still published", &why, "THALYX_REQUIRE_K1");
        }
        Err(why) => panic!("{why}"),
    };
    let mut a =
        session(&guest.line_socket(), "a", "src/lib.rs", "pub fn one() {}\n").expect("session");
    a.client.open("a", "req-a").expect("a opens");
    std::fs::write(
        a.workspace.join("src/lib.rs"),
        "pub fn one() {}\npub fn a() {}\n",
    )
    .expect("a edits");
    a.client
        .candidate()
        .expect("a freezes")
        .expect("a candidate");

    // The work's scope is fenced from outside, over the control port. Every
    // grant derived under it stops working, checked by the kernel.
    let fenced = guest.control(br#"{"op":"fence","line":1}"#).expect("fence");
    assert_eq!(fenced["ok"], json!(true), "the fence took: {fenced}");

    let refused = a
        .client
        .keep(&commit_receipt("req-a"))
        .expect_err("a fenced work must not publish");
    assert!(
        refused.to_lowercase().contains("closed") || refused.contains("work"),
        "the refusal should name the closed work: {refused}"
    );
    // And the kernel is the one that refused it: the control port's own view of
    // the work's grant says the lineage is fenced.
    let admit = guest
        .control(br#"{"op":"admit","line":1,"principal":1,"transaction":"a"}"#)
        .unwrap_or(json!({"admitted": true}));
    assert_eq!(
        admit["admitted"],
        json!(false),
        "the kernel should refuse the work's grant: {admit}"
    );
    eprintln!("PROVEN fence: a fenced work's publication was refused by the kernel");
}

#[test]
fn thalyx_kernel_recovers_a_published_version_after_a_reboot() {
    // The medium outlives the guest, so the second boot is the same bytes the
    // first left. That is what durability is here: not a power cut, but the
    // writer disappearing and the store coming back on what reached the medium.
    let medium = match tempfile::tempdir() {
        Ok(medium) => medium,
        Err(error) => panic!("{error}"),
    };
    let medium_file = medium.path().join("medium.img");

    let root = {
        let guest = match Guest::boot_on("recover-1", &medium_file, true, None) {
            Ok(guest) => guest,
            Err(why) if why.contains(NO_KERNEL) => {
                return not_proven(
                    "a published version did not survive a reboot",
                    &why,
                    "THALYX_REQUIRE_K1",
                );
            }
            Err(why) => panic!("{why}"),
        };
        let mut a =
            session(&guest.line_socket(), "a", "src/lib.rs", "pub fn one() {}\n").expect("session");
        a.client.open("a", "req-a").expect("a opens");
        std::fs::write(
            a.workspace.join("src/lib.rs"),
            "pub fn one() {}\npub fn a() {}\n",
        )
        .expect("a edits");
        a.client
            .candidate()
            .expect("a freezes")
            .expect("a candidate");
        let kept = a
            .client
            .keep(&commit_receipt("req-a"))
            .expect("a publishes");
        assert_eq!(kept.generation, Some(2));
        kept.root.expect("a root")
        // The guest is dropped here: shutdown, and the machine powers off.
    };

    // Boot again on the same medium and read the published version back.
    let guest = Guest::boot_on("recover-2", &medium_file, false, Some(medium))
        .expect("the guest boots again on the same medium");
    let transport = TestConsole::connect(&guest.line_socket()).expect("connect");
    let mut client = KernelClient::new(
        transport,
        Path::new("/nonexistent/view"),
        Path::new("/nonexistent/workspace"),
        "a",
    );
    let (generation, recovered) = client
        .published()
        .expect("the store answers after a reboot");
    assert_eq!(generation, 2, "the generation survived the reboot");
    assert_eq!(
        recovered.as_deref(),
        Some(root.as_str()),
        "the published root is the one written before the reboot"
    );
    eprintln!("PROVEN recovery: a version published before a reboot came back after it");
}

// ── the EXP-13 campaign ───────────────────────────────────────────────────────
//
// The final comparison. One frozen configuration, the same corpus, and as many
// paired rounds as asked for; every arm that this machine can run is run each
// round, in a randomised order, and every observation is written with its
// platform trace so the analysis can take paired differences across rounds.
//
// It is `#[ignore]` because it is the campaign, not a unit test: it boots a
// guest per K1 case per round, and it wants a Btrfs scratch for L0. It writes
// nothing but observations; `dev/exp13/analyze.py` turns them into the causal
// verdict. Run it with:
//
//   THALYX_EXP13_REPORT=<dir> THALYX_EXP13_ROUNDS=<n> \
//   THALYX_BTRFS_SCRATCH=<btrfs dir> \
//   THALYX_K1_IMAGE=<thalyx-k5.img> THALYX_K1_LAUNCHER=<run_k1_link.py> \
//     cargo test -p thalyx-cli --test exp13_equivalence -- --ignored --nocapture exp13_campaign
#[test]
#[ignore = "the EXP-13 campaign: boots a guest per K1 case per round; run by hand"]
fn exp13_campaign() {
    let report = std::env::var("THALYX_EXP13_REPORT")
        .expect("THALYX_EXP13_REPORT names where the campaign writes its observations");
    let rounds: usize = std::env::var("THALYX_EXP13_ROUNDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5);
    let base = Path::new(&report);
    std::fs::create_dir_all(base).expect("the report directory");

    // Which arms this machine can run, decided once so every round runs the
    // same set. L0 needs a Btrfs scratch; K1 needs a kernel image and launcher.
    let mut arms: Vec<&str> = vec![MANAGED];
    if std::env::var("THALYX_BTRFS_SCRATCH").is_ok() {
        arms.insert(0, CURRENT);
    } else {
        eprintln!("campaign: L0 (linux-current) skipped — no THALYX_BTRFS_SCRATCH");
    }
    let has_kernel =
        std::env::var("THALYX_K1_IMAGE").is_ok() && std::env::var("THALYX_K1_LAUNCHER").is_ok();
    if has_kernel {
        arms.push(KERNEL);
    } else {
        eprintln!("campaign: K1 (thalyx-kernel-managed) skipped — no THALYX_K1_IMAGE/LAUNCHER");
    }

    for round in 0..rounds {
        // A randomised order per round, so a drift in the machine over a round
        // does not land on one arm. A cheap deterministic shuffle seeded by the
        // round is enough: the order is recorded in the paths.
        let mut order = arms.clone();
        let seed = (round as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        for index in (1..order.len()).rev() {
            let pick = ((seed >> index) as usize + index) % (index + 1);
            order.swap(index, pick);
        }
        for arm in &order {
            let label = format!("r{round}");
            // observe/observe_kernel write to `$THALYX_EXP13_REPORT/{backend}-{label}`,
            // and the label carries the round, so the whole campaign lands under
            // the one report directory without the environment changing.
            let outcome = match *arm {
                KERNEL => observe_kernel(&thalyx(), &label).map(|_| ()),
                other => observe(&thalyx(), other, &label).map(|_| ()),
            };
            match outcome {
                Ok(()) => eprintln!("campaign: round {round} arm {arm} done"),
                Err(why) => panic!("campaign: round {round} arm {arm}: {why}"),
            }
        }
    }
    eprintln!(
        "campaign: {rounds} round(s) of {:?} written under {}; analyze with \
         dev/exp13/analyze.py",
        arms,
        base.display()
    );
}
