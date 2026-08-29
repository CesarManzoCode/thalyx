//! rust-analyzer, spoken to over LSP, as a read-only provider.
//!
//! ## Why this exists rather than more scanner
//!
//! `crates/thalyx-graph/corpus/05-alias` has carried this sentence for months:
//! *«`Keys` at src/boot.rs:3 is a use of `Keystore` and the index does not say
//! so. Following an alias means tracking a binding, which is a compiler and not
//! a scan.»* It is right, and it does not get less right by trying harder. Name
//! resolution in Rust is `use` renames, glob imports, prelude shadowing,
//! `macro_rules` hygiene, trait method selection and `impl` blocks spread over
//! files — a scanner that answered all of it would **be** rust-analyzer, badly.
//!
//! So Thalyx asks the thing that already answers it. What Thalyx keeps is the
//! part nobody else does: the answer's identity, its freshness, its budget, the
//! transaction it happens inside, and the authority to change anything.
//!
//! ## The boundary, said out loud
//!
//! **This process is a reader.** It is started with the workspace as its root,
//! it is asked questions, and it is killed. It never writes: a rename comes back
//! as a *description* of edits, and applying them is Thalyx's, through the same
//! path every other mutation goes through, inside the same transaction, subject
//! to the same workspace boundary. A URI that points outside the workspace is
//! refused here rather than trusted — see [`Analyzer::rename`].
//!
//! It runs outside the confinement `ejecutar` puts a foreign program in, and
//! that is a real gap rather than a decision that is finished: today it is a
//! host process Thalyx launches, and the guest architecture will have to give
//! it the same treatment `cargo` gets. `vault/06-Pendientes/Tareas-Pendientes.md`
//! carries it.
//!
//! ## Rule 5, before anything is believed
//!
//! The instrument includes the harness. A query that comes back empty is
//! reported as empty **and as having been asked**, and a server that never got
//! ready is [`Ready::TimedOut`], never an answer of "no references". A missing
//! rust-analyzer is [`RustError::NoAnalyzer`] and not a symbol that does not
//! exist: a failure to read is not a failure to exist.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use crate::{Result, RustError};

/// How long to wait for the server to finish its first indexing pass.
///
/// Measured rather than guessed: this workspace's twenty-eight crates take
/// about 25 seconds on the container, of which 15 are building proc-macro and
/// build-script dependencies. Five times that is the ceiling, and reaching it
/// is reported as a timeout rather than as an empty answer.
pub const READY_CEILING: Duration = Duration::from_secs(150);

/// How long any single question may take once the server is ready.
pub const ANSWER_CEILING: Duration = Duration::from_secs(30);

/// LSP's `ContentModified`: the document changed while the answer was being
/// computed, and the specified remedy is to ask again.
const CONTENT_MODIFIED: i64 = -32801;

/// How many times to ask again before calling it a refusal. Three, because the
/// thing that provokes it is the client's own `didOpen` settling, and a fourth
/// would be waiting on something else.
const RETRIES: u32 = 3;

/// How long to let the server settle before asking again.
const SETTLE: Duration = Duration::from_millis(250);

/// Where a name is, in the file the server named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spot {
    pub path: PathBuf,
    /// Zero-based, as LSP counts. Turned into the one-based line every other
    /// Thalyx answer uses at the surface and nowhere else, so there is exactly
    /// one place that can get it wrong.
    pub line: u32,
    pub character: u32,
    pub end_line: u32,
    pub end_character: u32,
}

/// A symbol as the server describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// `struct`, `function`, `method`, … — the LSP kind, spelled.
    pub kind: &'static str,
    pub at: Spot,
    /// The enclosing name, when the server gave one.
    pub container: Option<String>,
}

/// One file's worth of a rename, as ranges to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEdit {
    pub path: PathBuf,
    /// Ranges and their replacements, in the order the server gave them.
    pub edits: Vec<(Spot, String)>,
}

/// Whether the server ever became able to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ready {
    /// It finished its first pass, in this long.
    Indexed(Duration),
    /// It did not, within [`READY_CEILING`]. Every answer after this is
    /// suspect and the caller is told so rather than shown an empty list.
    TimedOut,
}

/// A running rust-analyzer, and the conversation with it.
pub struct Analyzer {
    child: Child,
    stdin: ChildStdin,
    incoming: Receiver<Value>,
    next_id: i64,
    root: PathBuf,
    opened: std::collections::HashSet<PathBuf>,
    /// What the server said it counts columns in. LSP's default is UTF-16 code
    /// units, which is not what a byte offset is; a client that assumed bytes
    /// would be right on ASCII and wrong on the first accented identifier, and
    /// this codebase is written by somebody who types Spanish.
    utf8_columns: bool,
    pub ready: Ready,
}

impl Analyzer {
    /// Start one, rooted at the workspace, and wait until it has indexed.
    pub fn start(root: &Path, binary: &Path) -> Result<Self> {
        let mut child = Command::new(binary)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Its log is noise on the way to an answer, and a pipe nobody
            // drains is a server that blocks on a full buffer halfway through
            // indexing — which would look exactly like a server that hung.
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| RustError::NoAnalyzer(format!("{}: {error}", binary.display())))?;

        let stdin = child.stdin.take().ok_or_else(|| {
            RustError::NoAnalyzer("rust-analyzer was started without a stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            RustError::NoAnalyzer("rust-analyzer was started without a stdout".to_string())
        })?;

        // A reader thread and a channel rather than reading in line: every read
        // here needs a deadline, and a blocking read on a pipe has none. A
        // server that wedges must become a refusal, not a machine that stops.
        let (sender, incoming) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Some(message) = read_message(&mut reader) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });

        let mut analyzer = Self {
            child,
            stdin,
            incoming,
            next_id: 1,
            root: root.to_path_buf(),
            opened: std::collections::HashSet::new(),
            utf8_columns: false,
            ready: Ready::TimedOut,
        };

        let initialized = analyzer.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": uri_of(root),
                // Asked for explicitly. The server answers with what it will
                // actually use, and that answer is read below rather than
                // assumed.
                "capabilities": {
                    "general": {"positionEncodings": ["utf-8", "utf-16"]},
                    // Without this the server sends no progress at all, and a
                    // client waiting for indexing to finish waits forever. It
                    // is the whole reason the first attempt at this hung.
                    "window": {"workDoneProgress": true},
                    "workspace": {"workspaceEdit": {"documentChanges": true}},
                    "textDocument": {
                        "documentSymbol": {"hierarchicalDocumentSymbolSupport": false},
                        "rename": {"prepareSupport": true}
                    }
                }
            }),
            READY_CEILING,
        )?;
        analyzer.utf8_columns = initialized
            .pointer("/capabilities/positionEncoding")
            .and_then(Value::as_str)
            == Some("utf-8");
        analyzer.notify("initialized", json!({}))?;
        analyzer.ready = analyzer.wait_until_indexed();
        Ok(analyzer)
    }

    /// Wait for the server's first indexing pass to end.
    ///
    /// The end of `rustAnalyzer/cachePriming` and not a sleep: a sleep long
    /// enough for a big workspace is wasted on a small one, and a sleep short
    /// enough for a small one answers a big one's questions with silence — the
    /// exact shape of a test that measures the harness instead of the system.
    fn wait_until_indexed(&mut self) -> Ready {
        let started = Instant::now();
        while started.elapsed() < READY_CEILING {
            let Ok(message) = self
                .incoming
                .recv_timeout(READY_CEILING.saturating_sub(started.elapsed()))
            else {
                return Ready::TimedOut;
            };
            if message.get("method").and_then(Value::as_str) == Some("$/progress")
                && message
                    .pointer("/params/value/kind")
                    .and_then(Value::as_str)
                    == Some("end")
                && message.pointer("/params/token").and_then(Value::as_str)
                    == Some("rustAnalyzer/cachePriming")
            {
                return Ready::Indexed(started.elapsed());
            }
        }
        Ready::TimedOut
    }

    /// Tell the server about a file before asking about it.
    ///
    /// Sent from the bytes on disk rather than as a bare "this file exists":
    /// the server answers about the document it holds, and a client that let
    /// the two drift would get positions for a file nobody has.
    pub fn open(&mut self, file: &Path) -> Result<()> {
        if self.opened.contains(file) {
            return Ok(());
        }
        let text = std::fs::read_to_string(file)
            .map_err(|error| RustError::Unreadable(format!("{}: {error}", file.display())))?;
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": uri_of(file), "languageId": "rust", "version": 1, "text": text
            }}),
        )?;
        self.opened.insert(file.to_path_buf());
        Ok(())
    }

    /// Where the name at this position is defined.
    ///
    /// **This is the answer the scanner cannot give.** Asked at `Keys` in
    /// `fn boot() -> Keys`, it says `keystore.rs`, because `Keys` is a binding
    /// introduced by `use … as` and following a binding is compiler work.
    pub fn definition(&mut self, file: &Path, line: u32, character: u32) -> Result<Vec<Spot>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/definition",
            json!({"textDocument": {"uri": uri_of(file)},
                   "position": {"line": line, "character": character}}),
            ANSWER_CEILING,
        )?;
        Ok(spots(&answer))
    }

    /// Every place the name at this position is used, the declaration included.
    pub fn references(&mut self, file: &Path, line: u32, character: u32) -> Result<Vec<Spot>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/references",
            json!({"textDocument": {"uri": uri_of(file)},
                   "position": {"line": line, "character": character},
                   "context": {"includeDeclaration": true}}),
            ANSWER_CEILING,
        )?;
        Ok(spots(&answer))
    }

    /// The one-line signature the server shows on hover, without the prose.
    pub fn signature(&mut self, file: &Path, line: u32, character: u32) -> Result<Option<String>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/hover",
            json!({"textDocument": {"uri": uri_of(file)},
                   "position": {"line": line, "character": character}}),
            ANSWER_CEILING,
        )?;
        let text = answer
            .pointer("/contents/value")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // The hover is a document: a path, a signature, then documentation.
        // What a repo map wants is the declaration, so the first line that
        // looks like one is taken and the rest is left in the machine.
        let signature = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && *line != "```rust" && *line != "```")
            .find(|line| {
                [
                    "pub ", "fn ", "struct ", "enum ", "trait ", "type ", "const ", "static ",
                    "impl ",
                ]
                .iter()
                .any(|start| line.starts_with(start))
            })
            .map(str::to_string);
        Ok(signature)
    }

    /// Everything declared in one file.
    pub fn symbols_in(&mut self, file: &Path) -> Result<Vec<Symbol>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": uri_of(file)}}),
            ANSWER_CEILING,
        )?;
        Ok(symbols(&answer, Some(file)))
    }

    /// Everything in the workspace whose name matches, as the server matches.
    pub fn symbols_named(&mut self, query: &str) -> Result<Vec<Symbol>> {
        let answer = self.request("workspace/symbol", json!({"query": query}), ANSWER_CEILING)?;
        Ok(symbols(&answer, None))
    }

    /// What a rename would change, described rather than done.
    ///
    /// Every path is checked against the workspace root **here**, and a URI
    /// pointing anywhere else makes the whole answer a refusal rather than a
    /// filtered list. A partial rename is worse than none: it compiles nowhere
    /// and looks like it was applied.
    pub fn rename(
        &mut self,
        file: &Path,
        line: u32,
        character: u32,
        to: &str,
    ) -> Result<Vec<FileEdit>> {
        self.open(file)?;
        let answer = self.request(
            "textDocument/rename",
            json!({"textDocument": {"uri": uri_of(file)},
                   "position": {"line": line, "character": character},
                   "newName": to}),
            ANSWER_CEILING,
        )?;

        let mut changes: Vec<FileEdit> = Vec::new();
        if let Some(documents) = answer.get("documentChanges").and_then(Value::as_array) {
            for document in documents {
                let Some(uri) = document
                    .pointer("/textDocument/uri")
                    .and_then(Value::as_str)
                else {
                    continue;
                };
                changes.push(self.file_edit(uri, document.get("edits"))?);
            }
        } else if let Some(map) = answer.get("changes").and_then(Value::as_object) {
            for (uri, edits) in map {
                changes.push(self.file_edit(uri, Some(edits))?);
            }
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }

    fn file_edit(&self, uri: &str, edits: Option<&Value>) -> Result<FileEdit> {
        let path = path_of(uri).ok_or_else(|| RustError::Outside(uri.to_string()))?;
        if !path.starts_with(&self.root) {
            return Err(RustError::Outside(path.display().to_string()));
        }
        let mut ranges = Vec::new();
        for edit in edits.and_then(Value::as_array).into_iter().flatten() {
            let Some(spot) = spot_of(&path, edit.get("range")) else {
                continue;
            };
            ranges.push((
                spot,
                edit.get("newText")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ));
        }
        Ok(FileEdit {
            path,
            edits: ranges,
        })
    }

    /// Whether the server counts columns in bytes. When it does not, a caller
    /// turning a position into a byte offset has to convert — see
    /// `crate::edits`.
    pub fn utf8_columns(&self) -> bool {
        self.utf8_columns
    }

    /// Ask, and ask again when the server says the file moved under it.
    ///
    /// LSP's `ContentModified` (-32801) is defined as *retry*: the server was
    /// answering about a document that changed while it was answering, which
    /// happens on the very next request after a `didOpen` because the open is
    /// what changed it. Treating it as a refusal made two of these tests fail
    /// with "content modified" and would have made a frontier model believe a
    /// symbol has no references — rule 5, where the instrument is the protocol.
    fn request(&mut self, method: &str, params: Value, ceiling: Duration) -> Result<Value> {
        let mut left = RETRIES;
        loop {
            match self.request_once(method, params.clone(), ceiling) {
                Err(RustError::Moved(_)) if left > 0 => {
                    left -= 1;
                    std::thread::sleep(SETTLE);
                }
                other => return other,
            }
        }
    }

    fn request_once(&mut self, method: &str, params: Value, ceiling: Duration) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + ceiling;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let message = match self.incoming.recv_timeout(left) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(RustError::Silent(format!("`{method}` after {ceiling:?}")));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(RustError::Silent(format!("`{method}`: the server stopped")));
                }
            };
            if message.get("id").and_then(Value::as_i64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                let why = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("no reason given");
                if error.get("code").and_then(Value::as_i64) == Some(CONTENT_MODIFIED) {
                    return Err(RustError::Moved(format!("`{method}`: {why}")));
                }
                return Err(RustError::Refused(format!("`{method}`: {why}")));
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn send(&mut self, message: Value) -> Result<()> {
        let body = serde_json::to_vec(&message).map_err(|error| {
            RustError::Silent(format!("a request could not be written: {error}"))
        })?;
        self.stdin
            .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
            .and_then(|()| self.stdin.write_all(&body))
            .and_then(|()| self.stdin.flush())
            .map_err(|error| RustError::Silent(format!("the server stopped listening: {error}")))
    }
}

impl Drop for Analyzer {
    /// Killed rather than asked to shut down.
    ///
    /// A polite `shutdown`/`exit` would need a reply, and the one case where
    /// this matters most is the case where the server is not replying. It holds
    /// no state anybody needs: everything it learned is either in the answer
    /// already given or being recomputed next time.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Read one LSP message, or `None` when the pipe closed.
fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            length = value.trim().parse().ok();
        }
    }
    let length = length?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn spots(answer: &Value) -> Vec<Spot> {
    let mut found = Vec::new();
    let listed = match answer {
        Value::Array(listed) => listed.clone(),
        Value::Object(_) => vec![answer.clone()],
        _ => return found,
    };
    for entry in listed {
        // `Location`, `LocationLink` and `SymbolInformation` all appear here
        // depending on the request; each names its range differently and all
        // three are read rather than one being assumed.
        let uri = entry
            .get("uri")
            .or_else(|| entry.get("targetUri"))
            .or_else(|| entry.pointer("/location/uri"))
            .and_then(Value::as_str);
        let range = entry
            .get("range")
            .or_else(|| entry.get("targetSelectionRange"))
            .or_else(|| entry.get("targetRange"))
            .or_else(|| entry.pointer("/location/range"));
        let (Some(uri), Some(range)) = (uri, range) else {
            continue;
        };
        let Some(path) = path_of(uri) else { continue };
        if let Some(spot) = spot_of(&path, Some(range)) {
            found.push(spot);
        }
    }
    found
}

fn spot_of(path: &Path, range: Option<&Value>) -> Option<Spot> {
    let range = range?;
    Some(Spot {
        path: path.to_path_buf(),
        line: range.pointer("/start/line")?.as_u64()? as u32,
        character: range.pointer("/start/character")?.as_u64()? as u32,
        end_line: range.pointer("/end/line")?.as_u64()? as u32,
        end_character: range.pointer("/end/character")?.as_u64()? as u32,
    })
}

fn symbols(answer: &Value, file: Option<&Path>) -> Vec<Symbol> {
    let Value::Array(listed) = answer else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in listed {
        let Some(name) = entry.get("name").and_then(Value::as_str) else {
            continue;
        };
        let path = entry
            .pointer("/location/uri")
            .and_then(Value::as_str)
            .and_then(path_of)
            .or_else(|| file.map(Path::to_path_buf));
        let Some(path) = path else { continue };
        let range = entry
            .pointer("/location/range")
            .or_else(|| entry.get("selectionRange"))
            .or_else(|| entry.get("range"));
        let Some(at) = spot_of(&path, range) else {
            continue;
        };
        found.push(Symbol {
            name: name.to_string(),
            kind: kind_of(entry.get("kind").and_then(Value::as_u64).unwrap_or(0)),
            at,
            container: entry
                .get("containerName")
                .and_then(Value::as_str)
                .filter(|container| !container.is_empty())
                .map(str::to_string),
        });
    }
    found
}

/// The LSP `SymbolKind` numbers, spelled.
///
/// Written out rather than derived: these are wire constants, and a table that
/// guessed would rename a struct into a class on the day the enum grew.
fn kind_of(kind: u64) -> &'static str {
    match kind {
        2 => "module",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "trait",
        12 => "function",
        13 => "variable",
        14 => "constant",
        22 => "variant",
        23 => "struct",
        26 => "type parameter",
        _ => "symbol",
    }
}

/// `file://` for a path. Percent-encoded, because a workspace under a folder
/// with a space in its name is a workspace, and a URI that did not say so would
/// send the server looking for a file that does not exist.
pub fn uri_of(path: &Path) -> String {
    let mut encoded = String::from("file://");
    for byte in path.to_string_lossy().as_bytes() {
        match byte {
            b'/' | b'-' | b'_' | b'.' | b'~' => encoded.push(*byte as char),
            b if b.is_ascii_alphanumeric() => encoded.push(*b as char),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

/// The path a `file://` URI names, or `None` for a URI that names no file.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                decoded.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    Some(PathBuf::from(String::from_utf8(decoded).ok()?))
}

/// The rust-analyzer this machine has, or nothing.
///
/// Every candidate is **run** rather than tested for existence, and the reason
/// is this container: `~/.cargo/bin/rust-analyzer` is there, is executable, and
/// is a rustup shim that answers `error: Unknown binary`. A search that stopped
/// at the first file it found would pick it every time — rule 5, where the
/// harness is the `PATH`.
pub fn find() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(named) = std::env::var_os("THALYX_RUST_ANALYZER") {
        candidates.push(PathBuf::from(named));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let toolchains = PathBuf::from(&home).join(".rustup").join("toolchains");
        if let Ok(entries) = std::fs::read_dir(&toolchains) {
            for entry in entries.flatten() {
                candidates.push(entry.path().join("bin").join("rust-analyzer"));
            }
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            candidates.push(directory.join("rust-analyzer"));
        }
    }
    candidates.into_iter().find(|candidate| {
        candidate.is_file()
            && Command::new(candidate)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
    })
}
