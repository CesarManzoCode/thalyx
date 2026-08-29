//! Changing the text in a file, from inside Thalyx.
//!
//! ## Why this exists
//!
//! `vault/06-Pendientes/Tareas-Pendientes.md` lists it as point 5 of the usable
//! terminal, and the sentence next to it is the whole argument: *without one, a
//! configuration file cannot be corrected from the machine*. Until this crate
//! Thalyx could make a file, copy it, move it, delete it and print it, and could
//! not change one byte inside it. A machine that installs and administers itself
//! and in which a typo is unfixable.
//!
//! ## The problem this crate has that no earlier verb had
//!
//! `vault/01-Filosofia/Principio-Doble-Ruta.md` is non-negotiable: everything one
//! route can do, the other can do, without loss of capability. Every verb before
//! this one satisfied it for free, because *list a directory* and *copy a file*
//! are the same act however you ask for them.
//!
//! **An editor is the first place the two faces genuinely differ in shape.** What
//! a person wants is a screen they type on. A screen is exactly what a program
//! cannot drive: it redraws, it has no framing, and asking a model to count
//! keystrokes to reach line 40 is asking it to pay all five costs of
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md` at once.
//!
//! Cesar settled it on 2026-08-22 by asking for both in one delivery. So:
//!
//! - **one engine** — this file — which owns what a change *is*;
//! - **the machine face** ([`machine`]), where a change is addressed by line and
//!   each one is a whole transaction: open, change, save, answer;
//! - **the human face** (`thalyx-cli/src/edit.rs`), a screen drawn over
//!   [`screen`], which is the same operations driven by keystrokes.
//!
//! Neither face implements an edit. Both call the functions below, which is the
//! only arrangement in which the screen and the structured answer cannot come to
//! disagree about what the file now says.
//!
//! ## Why the machine face is transactional and holds no buffer
//!
//! `thalyx-files/src/machine.rs` decrees the framing contract: **one typed line,
//! exactly one object**, and a boundary defined on one side only is not a
//! boundary. An open buffer surviving across several typed lines is hidden state
//! that a caller has to track and that nothing in the answer describes — and a
//! caller that lost track of it writes line 12 of a file it thinks is another
//! one.
//!
//! So every structured edit reads the file, changes it, and saves it. Taking
//! back more than one edit is what `intento` is for
//! (`vault/03-Primitivas/Journal-y-Snapshots.md`), and that is what the `undo`
//! field of every answer says, rather than this crate growing a second, weaker
//! version of a primitive that already exists and is proven on hardware.
//!
//! ## What governs the code below
//!
//! **Rule 9 — fail closed.** An editor is the one verb here whose ordinary use
//! destroys the previous contents of a file. Every doubt therefore refuses:
//! bytes that are not UTF-8 are refused rather than replaced with `U+FFFD` and
//! written back, a file over the ceiling is refused rather than half-loaded, and
//! a line number outside the file is refused rather than clamped to the end.
//! Every one of those, done the accommodating way, silently changes a file
//! somebody asked to edit.

pub mod machine;
pub mod screen;

use std::path::{Path, PathBuf};

/// The largest file this will open.
///
/// A ceiling and not a guess at what is reasonable. The whole file is held in
/// memory and written back in one act, so without a ceiling `editar` on a
/// multi-gigabyte log is the machine running out of memory — and on the image
/// that is PID 1 running out of memory.
///
/// It refuses **before** reading rather than while reading, which is the lesson
/// `indexar` cost on 2026-08-10: a verb that starts and then dies has already
/// spent the machine, and the caller cannot tell that from a hang.
pub const CEILING: u64 = 4 * 1024 * 1024;

/// How many bytes decide whether a file is text.
///
/// The same prefix-sized question `thalyx-files` asks before printing, asked
/// again here for a different reason: there, printing a binary wrecks a
/// terminal; here, *saving* one destroys a file that was never text.
const SNIFF: usize = 8192;

/// How many changes back the screen can go.
///
/// Bounded, and the bound is the point. Each step keeps a copy of the lines, so
/// an unbounded stack turns a long editing session into the memory problem
/// [`CEILING`] exists to prevent. Deeper undo is `intento`, which costs nothing
/// per step because Btrfs does the work.
pub const UNDO_DEPTH: usize = 100;

// ─────────────────────────────────────────────────────────── how lines are named

/// A run of lines, 1-based and inclusive at both ends.
///
/// 1-based because every error message, every editor and every person counts
/// that way, and a crate that stores 0-based indices while its answers are
/// 1-based is a crate with an off-by-one waiting in whichever of the two faces
/// is tested less.
///
/// Inclusive at both ends for the same reason: `3-5` means three lines to
/// everyone who is not a programmer, and this is typed by people.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: usize,
    pub to: usize,
}

impl Span {
    pub fn one(line: usize) -> Self {
        Self {
            from: line,
            to: line,
        }
    }

    pub fn count(self) -> usize {
        self.to.saturating_sub(self.from) + 1
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.from == self.to {
            write!(f, "{}", self.from)
        } else {
            write!(f, "{}-{}", self.from, self.to)
        }
    }
}

/// Read `12` or `12-20` as a run of lines.
///
/// Deliberately only these two forms. `$`, `.`, `+3` and the rest of the `ed`
/// address language would each be a thing a caller has to learn and a thing this
/// has to get right; the total number of lines is in every answer this crate
/// gives, so a caller that wants the last line has been told which one it is.
pub fn span(text: &str) -> Result<Span, EditError> {
    let text = text.trim();
    let malformed = || EditError::Malformed {
        asked: text.to_string(),
    };

    let (from, to) = match text.split_once('-') {
        Some((from, to)) => (from.trim(), to.trim()),
        None => (text, text),
    };

    let from: usize = from.parse().map_err(|_| malformed())?;
    let to: usize = to.parse().map_err(|_| malformed())?;

    // Zero is not a line. Accepting it and treating it as 1 would put text
    // somewhere other than where it was asked for, which is the whole failure
    // this crate is careful about.
    if from == 0 || to == 0 {
        return Err(malformed());
    }
    if to < from {
        return Err(EditError::Backwards { from, to });
    }
    Ok(Span { from, to })
}

// ────────────────────────────────────────────────────────────── what came back

/// What an edit actually did, in the terms it did it in.
///
/// Returned rather than printed, exactly as `thalyx-files::Done` is, and for the
/// same reason: the human face formats this and the machine face serialises it,
/// so there is one account of what happened rather than two that can drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub what: Change,
    pub path: PathBuf,
    /// The lines the change left behind, numbered as they are *now*.
    ///
    /// After a delete there are none, and `None` says so rather than an empty
    /// span, which would read as "line 0 to line 0".
    pub span: Option<Span>,
    pub lines_before: usize,
    pub lines_after: usize,
    /// Bytes on disk after the change, exact. Never the rounded form: two
    /// programs comparing two rounded numbers compare two lies.
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Inserted,
    Replaced,
    Deleted,
    /// The screen was left without changing anything on disk.
    Unchanged,
    /// The screen's changes were written.
    Saved,
}

impl Change {
    /// The word a program matches on. Stable, lowercase, never translated.
    pub fn word(self) -> &'static str {
        match self {
            Change::Inserted => "inserted",
            Change::Replaced => "replaced",
            Change::Deleted => "deleted",
            Change::Unchanged => "unchanged",
            Change::Saved => "saved",
        }
    }
}

// ─────────────────────────────────────────────────────────────── what can fail

/// Everything editing a file can fail with.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("{0} is not there")]
    Absent(PathBuf),

    #[error("{0} is a directory, and there is no text in one to change")]
    IsDirectory(PathBuf),

    /// Deliberately not "could not read". The bytes arrived and they are not
    /// text, so there is nothing here that could write them back unharmed.
    #[error("{path} is not text ({why}), and saving it as text would destroy it")]
    NotText { path: PathBuf, why: &'static str },

    #[error("{path} is {bytes} bytes and the ceiling is {ceiling}; it will not be opened")]
    TooLarge {
        path: PathBuf,
        bytes: u64,
        ceiling: u64,
    },

    /// Asked for a line the file does not have.
    ///
    /// Refused rather than clamped to the end. Clamping is how a caller that
    /// miscounted writes at the bottom of a file believing it wrote in the
    /// middle, and nothing in the answer would have told it otherwise.
    #[error("line {asked} was asked for and {path} has {has}")]
    NoSuchLine {
        path: PathBuf,
        asked: usize,
        has: usize,
    },

    #[error("{from}-{to} runs backwards")]
    Backwards { from: usize, to: usize },

    #[error("`{asked}` is not a line number or a range like 12-20")]
    Malformed { asked: String },

    #[error("{path} could not be read: {detail}")]
    Unreadable { path: PathBuf, detail: String },

    #[error("{path} could not be written: {detail}")]
    Unwritable { path: PathBuf, detail: String },

    /// The screen was asked for where there is no terminal to draw it on.
    ///
    /// Its own variant, and this is the one that earns it: a program down a pipe
    /// typing `editar notas.txt` must be told to address lines instead of
    /// waiting forever for a screen that will never arrive. Rule 10 — this is a
    /// failure to *have* a terminal, and it says so rather than looking like a
    /// failure to open the file.
    #[error("there is no terminal here to draw an editor on; address lines instead")]
    NoScreen,
}

impl EditError {
    /// The word a program matches on, kept apart from the English sentence in
    /// `Display`, which is prose for a person and will be reworded.
    pub fn word(&self) -> &'static str {
        match self {
            EditError::Absent(_) => "absent",
            EditError::IsDirectory(_) => "is_directory",
            EditError::NotText { .. } => "not_text",
            EditError::TooLarge { .. } => "too_large",
            EditError::NoSuchLine { .. } => "no_such_line",
            EditError::Backwards { .. } => "backwards",
            EditError::Malformed { .. } => "malformed_address",
            EditError::Unreadable { .. } => "unreadable",
            EditError::Unwritable { .. } => "unwritable",
            EditError::NoScreen => "no_screen",
        }
    }

    /// What would get past this, as a word.
    ///
    /// `Superficie-para-el-LLM.md`, punto **A2**: an error that names the way
    /// out is documentation delivered at the moment it is useful, and it costs
    /// one field. `cannot` is an answer — a binary file and an unwritable path
    /// have no remedy inside Thalyx, and inventing an encouraging one sends a
    /// caller into a loop retrying what will never work.
    pub fn remedy(&self) -> &'static str {
        match self {
            EditError::Absent(_) => "make_it_first",
            EditError::IsDirectory(_) => "use_list",
            EditError::NotText { .. } | EditError::TooLarge { .. } => "cannot",
            // Both are the same fix and it is the cheap one: the file says how
            // many lines it has, so ask.
            EditError::NoSuchLine { .. } | EditError::Backwards { .. } => "read_the_count",
            EditError::Malformed { .. } => "address_lines",
            EditError::Unreadable { .. } | EditError::Unwritable { .. } => "cannot",
            EditError::NoScreen => "address_lines",
        }
    }
}

fn classify(path: &Path, error: std::io::Error) -> EditError {
    match error.kind() {
        std::io::ErrorKind::NotFound => EditError::Absent(path.to_path_buf()),
        std::io::ErrorKind::IsADirectory => EditError::IsDirectory(path.to_path_buf()),
        _ => EditError::Unreadable {
            path: path.to_path_buf(),
            detail: error.to_string(),
        },
    }
}

// ──────────────────────────────────────────────────────────────── the text itself

/// How the lines of the file were separated, so that saving puts them back.
///
/// A file written on Windows and edited here must not come back with every line
/// ending changed. That is a whole-file diff nobody asked for, and on a shared
/// repository it is the kind of change that hides the one real edit inside three
/// hundred false ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Lf,
    CrLf,
}

impl Ending {
    fn as_str(self) -> &'static str {
        match self {
            Ending::Lf => "\n",
            Ending::CrLf => "\r\n",
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Ending::Lf => "lf",
            Ending::CrLf => "crlf",
        }
    }
}

/// A file's text, in memory, with everything needed to write it back as it was
/// found except for the part that was changed.
#[derive(Debug, Clone)]
pub struct Text {
    path: PathBuf,
    /// The file the save actually writes, which is not `path` when `path` is a
    /// symlink.
    ///
    /// Two fields rather than one because both are true and they answer
    /// different questions. Renaming over the link path — the obvious
    /// implementation — replaces the link with a regular file: the link is gone,
    /// the file it pointed at still says the old thing, and on a machine where
    /// `/etc` is full of links that is a configuration change nobody made.
    target: PathBuf,
    /// The name the file was actually opened by, when that is not the name it
    /// is reported under. `None` for every ordinary open. See
    /// [`Text::open_anchored`].
    opened_as: Option<PathBuf>,
    lines: Vec<String>,
    ending: Ending,
    /// Whether the file on disk ended with a line ending.
    ///
    /// Kept because it is a real difference and an invisible one: a POSIX text
    /// file ends with a newline, plenty of real files do not, and a save that
    /// adds or drops one changes a file in a way the person will only find out
    /// about from a diff.
    final_newline: bool,
    /// The mode the file had, to put back on the replacement.
    ///
    /// The save writes a *new* file and renames it over this one, so without
    /// this an executable script silently stops being executable — the file is
    /// correct and the machine no longer runs it.
    mode: Option<u32>,
    modified: bool,
    undo: Vec<Vec<String>>,
}

impl Text {
    /// Read a file, or say precisely why it will not be edited.
    pub fn open(path: &Path) -> Result<Self, EditError> {
        Self::open_named(path, None)
    }

    /// The same, opened through one name and reported under another.
    ///
    /// The caller with two names is `crate::confine`'s anchor: a confined
    /// session opens `/proc/self/fd/9/main.rs`, which is a descriptor the
    /// kernel resolved inside the workspace and therefore a name nothing can
    /// redirect, and the agent asked about `src/main.rs`. Every path this type
    /// hands back has to be the second one — a refusal naming a file descriptor
    /// describes a filesystem the caller may not see.
    ///
    /// What is *written* stays the first: [`Self::save`] stages beside the file
    /// and renames, and the staging has to happen in the pinned directory or it
    /// is a different directory.
    pub fn open_anchored(open_as: &Path, shown: &Path) -> Result<Self, EditError> {
        Self::open_named(open_as, Some(shown))
    }

    fn open_named(path: &Path, shown: Option<&Path>) -> Result<Self, EditError> {
        let named = shown.unwrap_or(path);
        let meta = path.symlink_metadata().map_err(|e| classify(named, e))?;

        // Followed deliberately: a person editing a symlink means the file it
        // points at, which is what every editor does and what makes `editar`
        // work on a config file that is a link. `metadata` rather than
        // `symlink_metadata` for the size and mode below, for the same reason.
        let (meta, target) = if meta.file_type().is_symlink() {
            (
                path.metadata().map_err(|e| classify(named, e))?,
                path.canonicalize().map_err(|e| classify(named, e))?,
            )
        } else {
            (meta, path.to_path_buf())
        };

        if meta.is_dir() {
            return Err(EditError::IsDirectory(named.to_path_buf()));
        }
        // Asked before reading. A ceiling checked after the read has already
        // spent the memory it exists to protect.
        if meta.len() > CEILING {
            return Err(EditError::TooLarge {
                path: named.to_path_buf(),
                bytes: meta.len(),
                ceiling: CEILING,
            });
        }

        let raw = std::fs::read(path).map_err(|e| classify(named, e))?;
        if let Some(why) = not_text(&raw) {
            return Err(EditError::NotText {
                path: named.to_path_buf(),
                why,
            });
        }
        // Not `from_utf8_lossy`. Lossy is how an editor turns bytes it did not
        // understand into `U+FFFD` and then writes them back over the original,
        // which destroys the file it was asked to fix.
        let body = String::from_utf8(raw).map_err(|_| EditError::NotText {
            path: named.to_path_buf(),
            why: "not valid UTF-8",
        })?;

        let mut text = Self::from_str(named, &body, mode_of(&meta));
        text.target = target;
        // Only when the two differ. `through_link` compares the reported path
        // with the written one, and for an anchored open those differ for a
        // reason that is not a symlink — so the comparison is made against what
        // was opened rather than against what is said.
        text.opened_as = shown.map(|_| path.to_path_buf());
        Ok(text)
    }

    /// The same thing from text already in hand, which is how the tests reach it
    /// without a filesystem.
    pub fn from_str(path: &Path, body: &str, mode: Option<u32>) -> Self {
        // The first ending in the file decides, rather than a vote. A file with
        // mixed endings has something wrong with it already, and picking the
        // majority would rewrite the minority without saying so.
        let ending = match body.find('\n') {
            Some(at) if at > 0 && body.as_bytes()[at - 1] == b'\r' => Ending::CrLf,
            _ => Ending::Lf,
        };
        let final_newline = body.ends_with('\n');

        let mut lines: Vec<String> = body
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
            .collect();
        // `split` on a body ending in a newline leaves an empty last piece that
        // is not a line. Dropping it here is what makes "how many lines" agree
        // with what every other tool on earth says about the same file.
        if final_newline {
            lines.pop();
        }
        // An empty file is one empty line and not zero lines, because a person
        // who opens it has a cursor sitting somewhere, and zero lines is nowhere.
        if lines.is_empty() {
            lines.push(String::new());
        }

        Self {
            path: path.to_path_buf(),
            target: path.to_path_buf(),
            opened_as: None,
            lines,
            ending,
            final_newline,
            mode,
            modified: false,
            undo: Vec::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file the save writes to. Equal to [`Self::path`] unless that is a
    /// symlink, and said rather than hidden: a caller that edited `/etc/thing`
    /// and got back a different path has learnt something true about its machine.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// Whether what was named and what gets written are two different paths.
    ///
    /// Compared against the name that was *opened*, which for an anchored open
    /// is a descriptor path and not what the caller said. Comparing against the
    /// reported name would make every confined edit look like an edit through a
    /// symlink, which is a true-sounding sentence about something that did not
    /// happen.
    pub fn through_link(&self) -> bool {
        self.opened_as.as_deref().unwrap_or(&self.path) != self.target
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn count(&self) -> usize {
        self.lines.len()
    }

    pub fn ending(&self) -> Ending {
        self.ending
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// The bytes this would occupy on disk, without writing it.
    ///
    /// Used by every answer, so that "how big is it now" never costs a caller a
    /// second command and never disagrees with what the save produced.
    pub fn weight(&self) -> u64 {
        self.render().len() as u64
    }

    fn render(&self) -> String {
        let mut out = self.lines.join(self.ending.as_str());
        if self.final_newline {
            out.push_str(self.ending.as_str());
        }
        out
    }

    /// Refuse a span this file cannot answer for.
    ///
    /// One place, called by every operation, because a check copied per
    /// operation is a check that will be right in three of them.
    fn hold(&self, span: Span, may_append: bool) -> Result<(), EditError> {
        let ceiling = if may_append {
            self.lines.len() + 1
        } else {
            self.lines.len()
        };
        if span.to > ceiling || span.from > ceiling {
            return Err(EditError::NoSuchLine {
                path: self.path.clone(),
                asked: span.to.max(span.from),
                has: self.lines.len(),
            });
        }
        Ok(())
    }

    fn remember(&mut self) {
        if self.undo.len() == UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.undo.push(self.lines.clone());
    }

    /// Step back one change, or say there was nothing to step back to.
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(lines) => {
                self.lines = lines;
                self.modified = true;
                true
            }
            None => false,
        }
    }

    /// Put `body` in before line `at`, so that it becomes line `at`.
    ///
    /// `at` may be one past the end, which is the only way to append — and it is
    /// deliberately not a separate verb, because two ways to add a line is two
    /// places for the line count to be got wrong.
    pub fn insert(&mut self, at: usize, body: &str) -> Result<Edited, EditError> {
        self.hold(Span::one(at), true)?;
        let before = self.lines.len();
        let incoming = split_incoming(body);
        let added = incoming.len();

        self.remember();
        let index = at - 1;
        self.lines.splice(index..index, incoming);
        self.modified = true;

        Ok(Edited {
            what: Change::Inserted,
            path: self.path.clone(),
            span: Some(Span {
                from: at,
                to: at + added - 1,
            }),
            lines_before: before,
            lines_after: self.lines.len(),
            bytes: self.weight(),
        })
    }

    /// Replace a run of lines with `body`.
    pub fn replace(&mut self, span: Span, body: &str) -> Result<Edited, EditError> {
        self.hold(span, false)?;
        let before = self.lines.len();
        let incoming = split_incoming(body);
        let added = incoming.len();

        self.remember();
        self.lines.splice(span.from - 1..span.to, incoming);
        self.modified = true;

        Ok(Edited {
            what: Change::Replaced,
            path: self.path.clone(),
            span: Some(Span {
                from: span.from,
                to: span.from + added - 1,
            }),
            lines_before: before,
            lines_after: self.lines.len(),
            bytes: self.weight(),
        })
    }

    /// Take a run of lines out.
    pub fn delete(&mut self, span: Span) -> Result<Edited, EditError> {
        self.hold(span, false)?;
        let before = self.lines.len();

        self.remember();
        self.lines.drain(span.from - 1..span.to);
        // A file with no lines cannot be edited afterwards — there is nowhere to
        // put the cursor and nowhere to insert relative to. One empty line is
        // what every editor leaves behind, and it is what keeps the next
        // operation from having to special-case emptiness.
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.modified = true;

        Ok(Edited {
            what: Change::Deleted,
            path: self.path.clone(),
            span: None,
            lines_before: before,
            lines_after: self.lines.len(),
            bytes: self.weight(),
        })
    }

    /// Write the text back, atomically.
    ///
    /// A new file, then `rename` over the old one, which is the same shape as
    /// `thalyx-core/src/commit.rs` and for the same reason: writing in place
    /// means a machine that loses power mid-write leaves a file that is half the
    /// old text and half the new, and a configuration file in that state is a
    /// machine that does not boot.
    ///
    /// Both `fsync` calls are load-bearing. Without the first the rename can
    /// reach the disk before the bytes do; without the second the rename itself
    /// can be lost, which puts back a file that was already replaced.
    pub fn save(&mut self) -> Result<Edited, EditError> {
        use std::io::Write;

        let body = self.render();
        let folder = self.target.parent().unwrap_or(Path::new("."));
        let name = self
            .target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());
        // Beside the file rather than in a temporary directory, because `rename`
        // does not cross filesystems: a staging file in `/tmp` would fail with
        // `EXDEV` on precisely the machine layout this project is built on.
        let staging = folder.join(format!(".{name}.thalyx-edit.{}", std::process::id()));

        let unwritable = |path: &Path, error: std::io::Error| EditError::Unwritable {
            path: path.to_path_buf(),
            detail: error.to_string(),
        };

        let write = || -> std::io::Result<()> {
            let mut file = std::fs::File::create(&staging)?;
            file.write_all(body.as_bytes())?;
            file.sync_all()
        };
        if let Err(error) = write() {
            // The staging file is this crate's mess and it is cleaned up before
            // the error is reported, so a failed save does not leave a dotfile
            // beside the real one for somebody to find later and wonder about.
            let _ = std::fs::remove_file(&staging);
            return Err(unwritable(&staging, error));
        }

        if let Some(mode) = self.mode {
            use std::os::unix::fs::PermissionsExt;
            if let Err(error) =
                std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(mode))
            {
                let _ = std::fs::remove_file(&staging);
                return Err(unwritable(&staging, error));
            }
        }

        if let Err(error) = std::fs::rename(&staging, &self.target) {
            let _ = std::fs::remove_file(&staging);
            return Err(unwritable(&self.target, error));
        }
        if let Ok(dir) = std::fs::File::open(folder) {
            let _ = dir.sync_all();
        }

        self.modified = false;
        Ok(Edited {
            what: Change::Saved,
            path: self.path.clone(),
            span: None,
            lines_before: self.lines.len(),
            lines_after: self.lines.len(),
            bytes: body.len() as u64,
        })
    }
}

/// Split what a caller handed us into lines to put in.
///
/// `\n` inside the text is a real line break, so one structured call can put in
/// a whole block. Text with no break is one line — and never zero, because
/// inserting nothing at line 12 would answer with a span that describes no lines
/// and a count that did not move.
fn split_incoming(body: &str) -> Vec<String> {
    body.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect()
}

fn mode_of(meta: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(meta.permissions().mode())
}

/// Whether these bytes are something that could be written back as text.
///
/// The same question `thalyx-files` asks before printing, and the answer is a
/// reason rather than a bool: "it will not open this" with no reason is what
/// sends a person looking for a broken editor instead of at the file.
fn not_text(raw: &[u8]) -> Option<&'static str> {
    let prefix = &raw[..raw.len().min(SNIFF)];
    if prefix.contains(&0) {
        return Some("it has zero bytes in it");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(body: &str) -> Text {
        Text::from_str(Path::new("/tmp/notes.txt"), body, None)
    }

    #[test]
    fn a_file_that_ends_without_a_newline_still_ends_without_one_after_a_save() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("no-newline.txt");
        std::fs::write(&path, "uno\ndos").unwrap();

        let mut file = Text::open(&path).unwrap();
        file.replace(Span::one(1), "UNO").unwrap();
        file.save().unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "UNO\ndos");
    }

    #[test]
    fn a_file_written_on_windows_comes_back_with_its_own_line_endings() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("windows.txt");
        std::fs::write(&path, "uno\r\ndos\r\n").unwrap();

        let mut file = Text::open(&path).unwrap();
        assert_eq!(file.ending(), Ending::CrLf);
        file.replace(Span::one(2), "DOS").unwrap();
        file.save().unwrap();

        // The whole point: one line changed, and the other line's ending did not.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "uno\r\nDOS\r\n");
    }

    #[test]
    fn bytes_that_are_not_utf8_are_refused_rather_than_replaced_and_written_back() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("latin1.txt");
        // `ñ` in Latin-1, which is not valid UTF-8. Lossy decoding would turn it
        // into U+FFFD and a save would then destroy the only copy.
        std::fs::write(&path, b"contrase\xf1a\n").unwrap();

        let error = Text::open(&path).unwrap_err();
        assert_eq!(error.word(), "not_text");
        assert_eq!(std::fs::read(&path).unwrap(), b"contrase\xf1a\n");
    }

    #[test]
    fn a_file_over_the_ceiling_is_refused_before_it_is_read() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("huge.txt");
        std::fs::write(&path, vec![b'x'; (CEILING + 1) as usize]).unwrap();

        let error = Text::open(&path).unwrap_err();
        assert_eq!(error.word(), "too_large");
        assert_eq!(error.remedy(), "cannot");
    }

    #[test]
    fn a_line_the_file_does_not_have_is_refused_rather_than_clamped_to_the_end() {
        let mut file = text("uno\ndos\n");
        let error = file.replace(Span::one(9), "nueve").unwrap_err();
        assert_eq!(error.word(), "no_such_line");
        // And nothing moved, which is the half that matters: a refusal that
        // already changed the file is not a refusal.
        assert_eq!(file.lines(), &["uno", "dos"]);
    }

    #[test]
    fn inserting_one_past_the_end_is_how_a_line_is_appended() {
        let mut file = text("uno\ndos\n");
        let done = file.insert(3, "tres").unwrap();
        assert_eq!(file.lines(), &["uno", "dos", "tres"]);
        assert_eq!(done.span, Some(Span::one(3)));
        assert_eq!(done.lines_before, 2);
        assert_eq!(done.lines_after, 3);
    }

    #[test]
    fn inserting_two_past_the_end_is_refused() {
        let mut file = text("uno\ndos\n");
        assert_eq!(file.insert(4, "cuatro").unwrap_err().word(), "no_such_line");
    }

    #[test]
    fn text_with_breaks_in_it_goes_in_as_several_lines_and_the_span_says_so() {
        let mut file = text("uno\ndos\n");
        let done = file.insert(2, "a\nb\nc").unwrap();
        assert_eq!(file.lines(), &["uno", "a", "b", "c", "dos"]);
        assert_eq!(done.span, Some(Span { from: 2, to: 4 }));
    }

    #[test]
    fn deleting_the_last_line_leaves_a_file_that_can_still_be_edited() {
        let mut file = text("solo\n");
        let done = file.delete(Span::one(1)).unwrap();
        assert_eq!(done.lines_after, 1);
        assert_eq!(file.lines(), &[""]);
        // The property that matters is not the empty line, it is that the next
        // operation still has somewhere to go.
        assert!(file.insert(1, "otra").is_ok());
    }

    #[test]
    fn a_backwards_range_is_its_own_refusal_and_not_a_missing_line() {
        assert_eq!(span("20-12").unwrap_err().word(), "backwards");
        assert_eq!(span("0").unwrap_err().word(), "malformed_address");
        assert_eq!(span("doce").unwrap_err().word(), "malformed_address");
        assert_eq!(span("12").unwrap(), Span::one(12));
        assert_eq!(span("12-20").unwrap(), Span { from: 12, to: 20 });
    }

    #[test]
    fn undo_steps_back_one_change_and_says_when_there_are_none_left() {
        let mut file = text("uno\ndos\n");
        file.replace(Span::one(1), "UNO").unwrap();
        file.delete(Span::one(2)).unwrap();
        assert_eq!(file.lines(), &["UNO"]);

        assert!(file.undo());
        assert_eq!(file.lines(), &["UNO", "dos"]);
        assert!(file.undo());
        assert_eq!(file.lines(), &["uno", "dos"]);
        assert!(!file.undo());
    }

    #[test]
    fn the_undo_stack_does_not_grow_without_a_bound() {
        let mut file = text("uno\n");
        for n in 0..UNDO_DEPTH + 50 {
            file.replace(Span::one(1), &n.to_string()).unwrap();
        }
        assert_eq!(file.undo.len(), UNDO_DEPTH);
    }

    #[test]
    fn a_saved_file_keeps_the_mode_it_had_so_a_script_stays_runnable() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("script");
        std::fs::write(&path, "uno\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let mut file = Text::open(&path).unwrap();
        file.replace(Span::one(1), "UNO").unwrap();
        file.save().unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn a_save_leaves_no_staging_file_beside_the_real_one() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("notes.txt");
        std::fs::write(&path, "uno\n").unwrap();

        let mut file = Text::open(&path).unwrap();
        file.replace(Span::one(1), "UNO").unwrap();
        file.save().unwrap();

        let left: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left, vec!["notes.txt".to_string()]);
    }

    #[test]
    fn an_empty_file_opens_as_one_empty_line_rather_than_none() {
        let file = text("");
        assert_eq!(file.count(), 1);
        assert_eq!(file.lines(), &[""]);
    }

    #[test]
    fn the_weight_reported_before_a_save_is_the_size_the_save_produces() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("notes.txt");
        std::fs::write(&path, "uno\ndos\n").unwrap();

        let mut file = Text::open(&path).unwrap();
        file.insert(3, "tres").unwrap();
        let predicted = file.weight();
        let done = file.save().unwrap();

        assert_eq!(predicted, done.bytes);
        assert_eq!(predicted, std::fs::metadata(&path).unwrap().len());
    }

    #[test]
    fn a_symlink_is_edited_as_the_file_it_points_at_and_stays_a_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real.conf");
        let link = tmp.path().join("link.conf");
        std::fs::write(&real, "uno\n").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let mut file = Text::open(&link).unwrap();
        file.replace(Span::one(1), "UNO").unwrap();
        file.save().unwrap();

        // What a save through a link must not do is replace the link with a
        // regular file, which is what an unguarded rename onto the link path
        // does — the link is gone and the real file still says `uno`.
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "UNO\n");
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
