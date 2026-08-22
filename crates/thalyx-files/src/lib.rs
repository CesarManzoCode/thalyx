//! Looking at files and folders from inside Thalyx.
//!
//! ## Why this exists, and what it is not
//!
//! `vault/01-Filosofia/Principio-Doble-Ruta.md` decrees, as non-negotiable, that
//! **everything the agent can do the human can do directly, without the agent
//! and without loss of capability** — and its first layer of use cases is plain
//! file work: create, move, copy, delete, without touching the agent.
//!
//! Until this crate, Thalyx could not list a directory. The session had thirteen
//! verbs and not one of them touched a file, so layer 1 of a decree marked
//! non-negotiable had no implementation at all. A machine that installs and
//! administers itself, and in which nobody can work.
//!
//! ## The distinction this crate is built on
//!
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md` says the image carries no
//! shell and no set of system utilities — `ls`, `cat`, `id`, busybox. Read as a
//! ban on the *capability*, that contradicts the decree above. Cesar settled it
//! on 2026-08-09, and the reading is the narrow one:
//!
//! > lo que está prohibido no es la shell, lo que está prohibido es incrustarnos
//! > en la shell de otro sistema […] no es `ls` ni `cat`, está prohibido
//! > meternos en un sistema ya hecho, porque si es así, no seremos un sistema
//! > operativo, seremos una distro parcheada con IA.
//!
//! So: **the capability is required and the foreign program is forbidden.** This
//! crate is why `make -C image count` still says one. Every byte of it compiles
//! into `thalyx`; nothing here shells out, and nothing here is busybox wearing a
//! Spanish name.
//!
//! ## What governs the code below
//!
//! **Rule 10 — a failure to read is not a failure to exist.** A directory with
//! one unreadable entry lists the rest and says which one it could not read.
//! Silently dropping it would report a smaller directory than the one on disk,
//! and the person would delete a folder believing it empty.

//! ## The two faces
//!
//! Nothing in this crate prints. Everything it can answer is a value — a
//! [`Listing`], an [`Excerpt`], a [`Done`], a [`FileError`] — and the two faces
//! are two readers of that one value: the human printer in
//! `thalyx-cli/src/files.rs`, and [`machine`] here. That is the whole reason
//! they cannot drift apart, and it is what the objective decree asks for by
//! name.

pub mod machine;
pub mod search;
pub mod window;

use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Where a session starts, and what `ir` with no argument returns to.
///
/// `/home` is the `user` subvolume, mounted by PID 1 from the store — see
/// `store_disk.rs`. It is the one place on the machine decreed to be the
/// person's own, and the one place no rollback of ours may touch
/// (`vault/04-Flujo-Canonico/Coherencia-Doble-Ruta.md`).
pub const HOME: &str = "/home";

/// Refusing to print more than this from one file.
///
/// Not a limit on what may be read — it is what keeps `leer` from filling a
/// screen nobody can scroll back. On the image there is no pager to pipe into
/// and no scrollback worth the name, so a file that overruns this is announced
/// and truncated rather than allowed to bury everything printed before it.
pub const EXCERPT: u64 = 64 * 1024;

/// How many bytes are inspected before calling a file text or not.
///
/// A prefix, not the whole file, because the question is "will printing this
/// wreck the terminal" and the first bytes answer it. Reading a gigabyte to find
/// out would cost more than the answer is worth.
const SNIFF: usize = 8192;

// ───────────────────────────────────────────────────────────── where a person is

/// Turn what a person typed into an absolute path, without asking the kernel.
///
/// Purely lexical: `..` is folded here rather than resolved through the
/// filesystem. That is the opposite of what `thalyx-core`'s module API does, and
/// the difference is the point. There, `..` is **refused**, because a module is
/// confined to a grant and letting two different pieces of code compute what a
/// path means is where escapes live. Here there is no grant to escape — this is
/// the owner of the machine navigating their own system — and `ir ..` is the
/// only thing `..` can mean to a person.
///
/// Folding `..` lexically also gives the answer a person expects when a symlink
/// is involved: `ir` into a linked directory and back out returns where they
/// came from, not wherever the link physically lives.
pub fn resolve(cwd: &Path, named: &str) -> PathBuf {
    let named = Path::new(named);
    let joined = if named.is_absolute() {
        named.to_path_buf()
    } else {
        cwd.join(named)
    };

    let mut out = PathBuf::from("/");
    for component in joined.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => {}
            Component::CurDir => {}
            // `..` at the root stays at the root rather than erroring. There is
            // nowhere above `/`, and a person typing it means "up" and not "fail".
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────── what is there

/// What one entry in a directory turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Directory,
    File {
        bytes: u64,
    },
    /// Where it points, as written, plus whether that destination exists now.
    ///
    /// Both are carried because a broken link and a working one are different
    /// facts about the machine, and a listing that showed only the arrow would
    /// let a person follow one into nothing without warning.
    Link {
        to: PathBuf,
        broken: bool,
    },
    /// A socket, a pipe, a device node — something that is not the two things
    /// people mean by "file". Named rather than lumped in, because `leer` on a
    /// device node can block forever and the listing is where that is visible.
    Other(&'static str),
}

/// One line of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: OsString,
    pub kind: Kind,
}

/// A name the system keeps for itself rather than one the person made.
///
/// Not cosmetic. Cesar's own home directory holds **thirty-five** of these
/// before the first folder he put there, so showing them by default buries
/// everything he was looking for under configuration he never asked to see.
/// Every system hides them, and this is why.
pub fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

impl Entry {
    /// Files and directories sort apart, then by name, because that is how a
    /// person scans a listing — the folders they might enter, then the files
    /// they might open.
    fn ordering_key(&self) -> (u8, OsString) {
        let group = match self.kind {
            Kind::Directory => 0,
            Kind::Link { .. } => 1,
            _ => 2,
        };
        (group, self.name.clone())
    }
}

/// A directory, and everything about it that could not be established.
///
/// The second field is the whole reason this is a struct and not a `Vec`. Rule
/// 10: an entry whose metadata could not be read is **not** an entry that is not
/// there, and the two must not arrive as the same thing. A caller that ignores
/// `unreadable` prints a short directory; a caller that reads it prints an
/// honest one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// Name, and what went wrong when it was asked about.
    pub unreadable: Vec<(OsString, String)>,
}

/// A listing after the window has been applied: one page of entries, and every
/// name that could not be read.
///
/// The second half is never paged, which is why it travels beside the page
/// rather than inside it. It is short, and it is the half rule 10 exists for — a
/// directory whose unreadable entries fell off the end of page one would report
/// itself as fully understood.
pub type Paged = (window::Page<Entry>, Vec<(OsString, String)>);

impl Listing {
    /// The bytes a cursor names for one entry.
    ///
    /// The same key the entries are already sorted by — the group first, then
    /// the name — flattened into bytes so a cursor can carry it verbatim. Bytes
    /// and not text on purpose: a name on Linux is bytes, and a cursor that
    /// could only carry valid UTF-8 would stop paging at the first badly named
    /// file in a directory and leave the caller with no way past it.
    ///
    /// It must agree with [`Entry::ordering_key`] or a cursor would name a
    /// position that moves between calls, which is why it is here next to it
    /// rather than wherever paging happens to be wanted.
    pub fn key_of(entry: &Entry) -> Vec<u8> {
        use std::os::unix::ffi::OsStrExt;
        let (group, name) = entry.ordering_key();
        let mut key = Vec::with_capacity(1 + name.len());
        key.push(group);
        key.extend_from_slice(name.as_bytes());
        key
    }

    /// Cut the entries to a window, keeping everything unreadable.
    ///
    /// `Superficie-para-el-LLM.md`, punto **B1**. What could not be read is
    /// never paged: it is short, and it is the half of a listing rule 10 exists
    /// for — a directory whose unreadable entries fell off the end of page one
    /// would report itself as fully understood.
    pub fn paged(self, asked: &window::Asked) -> Result<Paged, window::Cut> {
        let page = window::page(self.entries, Self::key_of, asked)?;
        Ok((page, self.unreadable))
    }
}

/// Everything looking at a file can fail with, kept apart from what it can say.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("{0} is not there")]
    Absent(PathBuf),

    #[error("{0} is a directory, and `ls` is what lists one")]
    IsDirectory(PathBuf),

    /// Deliberately not "could not read". The bytes arrived; they are simply not
    /// something a terminal survives being handed.
    #[error("{path} does not look like text ({why}), so printing it would wreck the terminal")]
    NotText { path: PathBuf, why: &'static str },

    #[error("{path} could not be read: {detail}")]
    Unreadable { path: PathBuf, detail: String },

    /// Something is already there.
    ///
    /// Its own variant because every operation that writes has to refuse rather
    /// than assume. Overwriting is a different request, and one that costs
    /// somebody a file when it is guessed at.
    #[error("{0} is already there, and I will not write over it")]
    Exists(PathBuf),

    /// A search was pointed at something that is not a directory.
    ///
    /// Its own variant rather than folded into [`Self::IsDirectory`]'s
    /// opposite-shaped sibling, because the remedy differs: this one is fixed
    /// by naming a folder, and the caller that gets it typed a file where a
    /// tree goes.
    #[error("{0} is not a directory, and a search walks a tree")]
    NotADirectory(PathBuf),

    /// A verb that needs something to look for was given nothing.
    ///
    /// Refused rather than answered with everything. `contenido` with an empty
    /// text matches every line of every file — a technically correct answer
    /// that is never what anybody meant, and one that costs the whole context
    /// window to receive.
    #[error("say what to look for")]
    NothingAsked,

    /// A tree nobody would wait for.
    ///
    /// Deliberately not [`Self::Unreadable`]. A caller told "unreadable" about
    /// `/home` goes looking for a permission problem that does not exist; the
    /// tree is perfectly readable and simply too big to answer about.
    #[error(
        "`{root}` holds more than {ceiling} files.\n  \
         Searching it would take long enough that nobody would wait for the \
         answer. Name a smaller tree."
    )]
    TreeTooLarge { root: PathBuf, ceiling: usize },
}

/// List a directory, keeping what could not be established rather than dropping it.
pub fn list(path: &Path) -> Result<Listing, FileError> {
    let reader = std::fs::read_dir(path).map_err(|error| classify(path, error))?;

    let mut entries = Vec::new();
    let mut unreadable = Vec::new();

    for item in reader {
        let item = match item {
            Ok(item) => item,
            // The iterator itself failing mid-walk has no name to report, so it
            // is recorded under the directory rather than silently ending the
            // listing — a truncated list that looks complete is the failure
            // this arm exists to prevent.
            Err(error) => {
                unreadable.push((OsString::from("…"), error.to_string()));
                continue;
            }
        };

        let name = item.file_name();
        // `symlink_metadata`, not `metadata`: a listing must say that a thing is
        // a link. `metadata` follows it and would describe the destination,
        // reporting a link to a directory as a directory — and then `ver` on a
        // broken link would report it as absent rather than as broken.
        match item.path().symlink_metadata() {
            Ok(meta) => entries.push(Entry {
                name,
                kind: kind_of(&item.path(), &meta),
            }),
            Err(error) => unreadable.push((name, error.to_string())),
        }
    }

    entries.sort_by_key(Entry::ordering_key);
    unreadable.sort();
    Ok(Listing {
        entries,
        unreadable,
    })
}

/// `ls` aimed at one thing that is not a directory.
///
/// Answered as a [`Listing`] of one rather than as its own shape, so that both
/// faces keep reading the same kind of fact. The earlier version had the human
/// printer read the metadata a second time on this path — a second code path
/// describing the same file, which is exactly what the objective decree calls a
/// second version of events, and it was already drifting: it reported a size
/// where the listing reports a kind.
pub fn list_one(path: &Path) -> Result<Listing, FileError> {
    let meta = path
        .symlink_metadata()
        .map_err(|error| classify(path, error))?;
    Ok(Listing {
        entries: vec![Entry {
            // A path ending in `..` or `/` has no file name of its own; naming
            // it by the whole path is worse than naming it by nothing.
            name: path.file_name().unwrap_or(path.as_os_str()).to_os_string(),
            kind: kind_of(path, &meta),
        }],
        unreadable: Vec::new(),
    })
}

pub(crate) fn kind_of(path: &Path, meta: &std::fs::Metadata) -> Kind {
    let file_type = meta.file_type();
    if file_type.is_symlink() {
        let to = std::fs::read_link(path).unwrap_or_else(|_| PathBuf::from("?"));
        // `metadata` follows the link, so its failure is what says the
        // destination is not there. Asked of the link itself this would always
        // succeed and never report a broken one.
        let broken = path.metadata().is_err();
        return Kind::Link { to, broken };
    }
    if file_type.is_dir() {
        return Kind::Directory;
    }
    if file_type.is_file() {
        return Kind::File { bytes: meta.len() };
    }
    Kind::Other(other_kind(meta))
}

#[cfg(unix)]
fn other_kind(meta: &std::fs::Metadata) -> &'static str {
    use std::os::unix::fs::FileTypeExt;
    let file_type = meta.file_type();
    if file_type.is_socket() {
        "socket"
    } else if file_type.is_fifo() {
        "pipe"
    } else if file_type.is_block_device() {
        "block device"
    } else if file_type.is_char_device() {
        "character device"
    } else {
        "not a file"
    }
}

/// What `leer` got, and whether that is all of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excerpt {
    pub text: String,
    /// Bytes the file actually holds, which is not the length of `text` when the
    /// excerpt was cut short. Both are needed to say "showing 64 kB of 900 kB"
    /// instead of leaving a person believing they saw the file.
    pub of_bytes: u64,
    pub truncated: bool,
    /// SHA-256 of the **whole** file, not of the excerpt.
    ///
    /// `Superficie-para-el-LLM.md`, punto **B2**: the second cost an LLM pays is
    /// context, and the commonest way it pays it is re-reading a file to find
    /// out whether what it read twenty steps ago is still true. With this, that
    /// question is a comparison instead of a second read.
    ///
    /// Of the whole file **because a hash of the excerpt would be a hash of
    /// Thalyx's answer rather than of the machine**: two different files that
    /// share their first 64 kB would hash the same, and the caller would carry
    /// on believing nothing had changed.
    pub digest: String,
}

/// SHA-256, lowercase hex.
///
/// The same algorithm the rest of Thalyx already verifies artefacts with, so a
/// caller that has one of these can compare it against anything else the system
/// says about the same bytes without converting between two families of hash.
fn digest_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Read a file as text, refusing rather than spraying a terminal with bytes.
///
/// Refusing is the part worth defending. `cat` on a binary is a rite of passage
/// that leaves a terminal unusable, and on the image there is no second terminal
/// to recover from — the session is the machine. So a file that does not look
/// like text is named as such and not printed, which costs a person one extra
/// word and saves them a reboot.
pub fn read(path: &Path) -> Result<Excerpt, FileError> {
    let meta = path.metadata().map_err(|error| classify(path, error))?;
    if meta.is_dir() {
        return Err(FileError::IsDirectory(path.to_path_buf()));
    }

    let bytes = std::fs::read(path).map_err(|error| classify(path, error))?;

    if let Some(why) = not_text(&bytes) {
        return Err(FileError::NotText {
            path: path.to_path_buf(),
            why,
        });
    }

    let of_bytes = bytes.len() as u64;
    let truncated = of_bytes > EXCERPT;
    let shown = if truncated {
        // Cut on a character boundary, not a byte one. Slicing UTF-8 mid-character
        // would make a valid file arrive as invalid and get reported as binary —
        // the instrument accusing the file of what the instrument did.
        // A continuation byte is `10xxxxxx`; a boundary is anything else. Walking
        // back to one is what keeps a cut from turning valid text into what the
        // check below would call a binary.
        let mut cut = EXCERPT as usize;
        while cut > 0 && bytes[cut] & 0b1100_0000 == 0b1000_0000 {
            cut -= 1;
        }
        &bytes[..cut]
    } else {
        &bytes[..]
    };

    match std::str::from_utf8(shown) {
        Ok(text) => Ok(Excerpt {
            text: text.to_string(),
            of_bytes,
            truncated,
            digest: digest_of(&bytes),
        }),
        Err(_) => Err(FileError::NotText {
            path: path.to_path_buf(),
            why: "not valid UTF-8",
        }),
    }
}

/// Why this should not be printed, or `None` if it is fine to print.
///
/// Two questions, and both are needed. A NUL byte is the one thing no text file
/// has and every executable does. Control characters catch the rest: a file can
/// be valid UTF-8 and still be full of escape sequences that repaint the screen,
/// and "valid UTF-8" alone would wave those straight through to the terminal.
pub(crate) fn not_text(bytes: &[u8]) -> Option<&'static str> {
    let head = &bytes[..bytes.len().min(SNIFF)];

    if head.contains(&0) {
        return Some("it contains a zero byte");
    }

    let control = head
        .iter()
        .filter(|byte| byte.is_ascii_control() && !matches!(byte, b'\n' | b'\r' | b'\t'))
        .count();

    // A threshold rather than a ban, because one stray control byte in a log is
    // not a binary, and refusing to show a log over one byte would be the
    // cautious answer to the wrong question.
    if control * 100 > head.len().max(1) {
        return Some("it is full of control characters");
    }

    None
}

/// Turn an OS error into the one thing it means here.
///
/// `NotFound` is split out from everything else because "it is not there" and
/// "it is there and something went wrong" send a person to opposite places, and
/// an error type that merges them sends them to the wrong one half the time.
pub(crate) fn classify(path: &Path, error: std::io::Error) -> FileError {
    if error.kind() == std::io::ErrorKind::NotFound {
        FileError::Absent(path.to_path_buf())
    } else {
        FileError::Unreadable {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
    }
}

// ────────────────────────────────────────────────────────────────── saying sizes

/// A size a person reads at a glance, with the exact byte count still available.
///
/// Rounded for the listing and never for a decision: `2.0 GB` is what a person
/// wants to see and exactly the wrong thing to compare two files with. Anything
/// that has to be exact reads `Kind::File { bytes }` instead.
pub struct Size(pub u64);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const STEP: f64 = 1024.0;
        let bytes = self.0 as f64;
        // Bytes are printed whole. "1.0 B" claims a precision that a count of
        // things cannot have.
        if self.0 < 1024 {
            return write!(f, "{} B", self.0);
        }
        for (power, unit) in [(1u32, "kB"), (2, "MB"), (3, "GB")] {
            let scaled = bytes / STEP.powi(power as i32);
            if scaled < STEP || unit == "GB" {
                return write!(f, "{scaled:.1} {unit}");
            }
        }
        unreachable!("the GB arm returns for everything that reaches it")
    }
}

// ──────────────────────────────────────────────────────────── fitting on a screen

/// Lay names out in columns, the way a listing has to be to be read.
///
/// Found by running it on a real home directory: sixty-odd entries, one per
/// line, is four screens of scrolling for something every other system fits in
/// one. A listing nobody can take in at a glance does not do the job a listing
/// is for.
///
/// `width` is what the terminal said, and the caller supplies it — including the
/// fallback when nothing said anything, because what to do with no width depends
/// on what is being printed.
pub fn in_columns(names: &[String], width: usize, indent: usize) -> Vec<String> {
    if names.is_empty() {
        return Vec::new();
    }

    const GAP: usize = 2;
    let longest = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    let cell = longest + GAP;

    // At least one column always. A name wider than the terminal still has to be
    // printed — overrunning is the lesser failure, and printing nothing at all
    // would be the instrument deciding a file may not be seen.
    let columns = ((width.saturating_sub(indent)) / cell).max(1);
    if columns == 1 {
        return names.to_vec();
    }

    // Down each column and then across, which is how every listing anybody has
    // read is arranged: alphabetical order runs down the page, not along it.
    let rows = names.len().div_ceil(columns);
    let mut out = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut line = String::new();
        for column in 0..columns {
            let Some(name) = names.get(column * rows + row) else {
                continue;
            };
            if column * rows + row + rows >= names.len() {
                // Last cell of the row: no padding after it, so a line never
                // carries trailing spaces into somebody's terminal or diff.
                line.push_str(name);
            } else {
                let pad = cell.saturating_sub(name.chars().count());
                line.push_str(name);
                line.push_str(&" ".repeat(pad));
            }
        }
        out.push(line.trim_end().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────── finding a place

    #[test]
    fn a_relative_name_lands_inside_where_the_person_is() {
        let here = resolve(Path::new("/home/cesar"), "notas");
        assert_eq!(here, Path::new("/home/cesar/notas"));
    }

    #[test]
    fn an_absolute_name_ignores_where_the_person_is() {
        let there = resolve(Path::new("/home/cesar"), "/opt/thalyx");
        assert_eq!(there, Path::new("/opt/thalyx"));
    }

    #[test]
    fn two_dots_go_up_one_and_not_through_the_filesystem() {
        assert_eq!(
            resolve(Path::new("/home/cesar/notas"), ".."),
            Path::new("/home/cesar")
        );
        assert_eq!(resolve(Path::new("/home/cesar"), "../.."), Path::new("/"));
    }

    #[test]
    fn two_dots_at_the_root_stay_at_the_root_instead_of_failing() {
        // A person typing `ir ..` at `/` means "up", and there is nowhere up.
        // Erroring would be answering a question they did not ask.
        assert_eq!(resolve(Path::new("/"), ".."), Path::new("/"));
        assert_eq!(resolve(Path::new("/"), "../../.."), Path::new("/"));
    }

    #[test]
    fn a_single_dot_changes_nothing() {
        assert_eq!(
            resolve(Path::new("/home/cesar"), "."),
            Path::new("/home/cesar")
        );
        assert_eq!(
            resolve(Path::new("/home/cesar"), "./notas/./x"),
            Path::new("/home/cesar/notas/x")
        );
    }

    // ──────────────────────────────────────────────────────────── what is there

    #[test]
    fn a_listing_separates_folders_from_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.txt"), "x").unwrap();
        std::fs::create_dir(dir.path().join("z-folder")).unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();

        let listing = list(dir.path()).unwrap();
        let names: Vec<_> = listing
            .entries
            .iter()
            .map(|e| e.name.to_string_lossy().to_string())
            .collect();

        // The folder sorts first even though its name sorts last.
        assert_eq!(names, ["z-folder", "a.txt", "b.txt"]);
    }

    #[test]
    fn a_files_size_is_carried_exactly_and_not_only_rounded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f"), vec![0u8; 1234]).unwrap();

        let listing = list(dir.path()).unwrap();
        assert_eq!(listing.entries[0].kind, Kind::File { bytes: 1234 });
    }

    #[test]
    fn a_link_is_reported_as_a_link_and_not_as_what_it_points_at() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("real")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();

        let listing = list(dir.path()).unwrap();
        let link = listing
            .entries
            .iter()
            .find(|e| e.name == "link")
            .expect("the link is listed");

        // Following it would have reported a directory, and a person would never
        // learn the machine has a link there.
        match &link.kind {
            Kind::Link { broken, .. } => assert!(!broken, "it points at a directory that exists"),
            other => panic!("a link was reported as {other:?}"),
        }
    }

    #[test]
    fn a_broken_link_is_listed_as_broken_rather_than_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(dir.path().join("nowhere"), dir.path().join("dangling"))
            .unwrap();

        let listing = list(dir.path()).unwrap();
        let entry = &listing.entries[0];

        // Rule 10 in the small: the link exists, its destination does not, and
        // dropping it from the listing would report the first fact wrongly.
        match &entry.kind {
            Kind::Link { broken, to } => {
                assert!(broken);
                assert!(to.ends_with("nowhere"));
            }
            other => panic!("a dangling link was reported as {other:?}"),
        }
    }

    #[test]
    fn a_directory_that_is_not_there_says_so_rather_than_looking_empty() {
        let dir = tempfile::tempdir().unwrap();
        let error = list(&dir.path().join("nope")).unwrap_err();

        // An empty `Listing` would have been the comfortable answer and the
        // wrong one: "nothing here" and "no such folder" send a person to
        // opposite places.
        assert!(matches!(error, FileError::Absent(_)), "got {error:?}");
    }

    // ───────────────────────────────────────────────────────────────── reading

    #[test]
    fn a_text_file_comes_back_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notas.txt");
        std::fs::write(&path, "hola\nqué tal\n").unwrap();

        let excerpt = read(&path).unwrap();
        assert_eq!(excerpt.text, "hola\nqué tal\n");
        assert!(!excerpt.truncated);
    }

    #[test]
    fn a_file_with_a_zero_byte_is_refused_instead_of_printed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("program");
        std::fs::write(&path, b"\x7fELF\x02\x01\x01\x00\x00\x00").unwrap();

        // On the image the session *is* the machine: there is no second terminal
        // to recover from one wrecked by escape sequences.
        let error = read(&path).unwrap_err();
        assert!(matches!(error, FileError::NotText { .. }), "got {error:?}");
    }

    #[test]
    fn a_file_full_of_escape_sequences_is_refused_even_though_it_is_valid_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nasty");
        // Valid UTF-8 from end to end. Checking only for decodability would have
        // handed every one of these straight to the terminal.
        std::fs::write(&path, "\x1b[2J\x1b[H\x1b[2J\x1b[H\x1b[2J\x1b[H").unwrap();

        let error = read(&path).unwrap_err();
        assert!(matches!(error, FileError::NotText { .. }), "got {error:?}");
    }

    #[test]
    fn a_log_with_one_stray_control_byte_is_still_shown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log");
        let mut text = "a line that is perfectly ordinary\n".repeat(40);
        text.push('\x07');
        std::fs::write(&path, &text).unwrap();

        // The control-character check is a threshold and not a ban, because
        // refusing a log over one byte is the cautious answer to the wrong
        // question. This is the control for the test above it.
        let excerpt = read(&path).expect("an ordinary log is not a binary");
        assert!(excerpt.text.starts_with("a line"));
    }

    #[test]
    fn tabs_and_newlines_do_not_make_a_file_binary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("table.tsv");
        std::fs::write(&path, "a\tb\tc\r\n1\t2\t3\r\n").unwrap();

        assert!(
            read(&path).is_ok(),
            "tab and CRLF are text, not control noise"
        );
    }

    #[test]
    fn a_file_longer_than_the_excerpt_says_it_was_cut() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.txt");
        let content = "x".repeat(EXCERPT as usize + 500);
        std::fs::write(&path, &content).unwrap();

        let excerpt = read(&path).unwrap();
        assert!(excerpt.truncated);
        assert_eq!(excerpt.of_bytes, EXCERPT + 500);
        // The count of what the file holds, not of what was shown — otherwise a
        // person cannot tell how much they are missing.
        assert!(excerpt.text.len() <= EXCERPT as usize);
    }

    #[test]
    fn cutting_a_long_file_never_reports_it_as_binary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("acentos.txt");
        // Multi-byte characters packed so the cut lands mid-character. Slicing on
        // a byte boundary would produce invalid UTF-8 and the file would be
        // accused of being binary by the code that broke it.
        let content = "ñ".repeat(EXCERPT as usize);
        std::fs::write(&path, &content).unwrap();

        let excerpt = read(&path).expect("a cut must not turn text into a binary");
        assert!(excerpt.truncated);
        assert!(excerpt.text.chars().all(|c| c == 'ñ'));
    }

    #[test]
    fn reading_a_directory_says_which_verb_lists_one() {
        let dir = tempfile::tempdir().unwrap();
        let error = read(dir.path()).unwrap_err();
        assert!(matches!(error, FileError::IsDirectory(_)), "got {error:?}");
    }

    #[test]
    fn an_empty_file_reads_as_empty_and_not_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty");
        std::fs::write(&path, "").unwrap();

        let excerpt = read(&path).expect("an empty file is a file");
        assert_eq!(excerpt.text, "");
        assert_eq!(excerpt.of_bytes, 0);
    }

    // ──────────────────────────────────────────────────────── fitting on a screen

    #[test]
    fn a_name_that_starts_with_a_dot_is_one_the_system_keeps_for_itself() {
        assert!(is_hidden(std::ffi::OsStr::new(".bashrc")));
        assert!(is_hidden(std::ffi::OsStr::new(".config")));
        assert!(!is_hidden(std::ffi::OsStr::new("Documentos")));
        // A dot inside a name is an extension, not a hidden file.
        assert!(!is_hidden(std::ffi::OsStr::new("notas.txt")));
    }

    #[test]
    fn short_names_share_a_line_instead_of_taking_one_each() {
        let names: Vec<String> = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        // The failure this prevents, found on a real home directory: sixty
        // entries printed one per line is four screens of scrolling.
        let lines = in_columns(&names, 80, 4);
        assert!(lines.len() < names.len(), "got {lines:?}");
    }

    #[test]
    fn columns_run_down_the_page_and_not_along_it() {
        let names: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        // Seven columns wide fits exactly two cells, which is what makes this
        // test able to tell the two fill orders apart. At twenty they all fit on
        // one line and the question does not arise.
        let lines = in_columns(&names, 7, 0);

        // Alphabetical order reads downwards in every listing anybody has ever
        // read. Filling across would put `a b` on one line and `c d` below it,
        // and a person scanning the first column would see `a` then `c`.
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with('a'), "got {lines:?}");
        assert!(lines[1].starts_with('b'), "got {lines:?}");
    }

    #[test]
    fn a_name_wider_than_the_terminal_is_still_printed() {
        let names = vec!["a-name-far-longer-than-the-terminal-is-wide".to_string()];
        let lines = in_columns(&names, 20, 0);

        // Overrunning is the lesser failure. Printing nothing would be the
        // listing deciding a file may not be seen.
        assert_eq!(lines, names);
    }

    #[test]
    fn no_line_of_a_listing_carries_trailing_spaces() {
        let names: Vec<String> = ["aa", "b", "ccc", "d", "e"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        for line in in_columns(&names, 40, 2) {
            assert_eq!(line, line.trim_end(), "trailing space in {line:?}");
        }
    }

    #[test]
    fn every_name_survives_being_laid_out() {
        let names: Vec<String> = (0..37).map(|n| format!("archivo-{n}")).collect();
        let laid_out = in_columns(&names, 80, 4).join(" ");

        // The arithmetic of rows and columns is exactly where an entry goes
        // missing, and a listing short by one is how somebody deletes a folder
        // believing it empty.
        for name in &names {
            assert!(
                laid_out.split_whitespace().any(|w| w == name),
                "{name} was lost in the layout"
            );
        }
    }

    #[test]
    fn an_empty_directory_lays_out_to_nothing_rather_than_to_a_blank_line() {
        assert!(in_columns(&[], 80, 4).is_empty());
    }

    // ──────────────────────────────────────────────────── changing what is there

    #[test]
    fn making_a_folder_that_is_already_there_is_refused_and_not_reported_as_made() {
        let dir = tempfile::tempdir().unwrap();
        make_directory(&dir.path().join("notas")).unwrap();
        // "It is already there" and "I made it" are different facts, and an agent
        // that cannot tell them apart cannot tell whether it is repeating itself.
        let again = make_directory(&dir.path().join("notas"));
        assert!(matches!(again, Err(FileError::Exists(_))), "got {again:?}");
    }

    #[test]
    fn a_new_folder_brings_its_parents_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a/b/c");
        assert_eq!(make_directory(&deep).unwrap().what, Did::MadeDirectory);
        assert!(deep.is_dir());
    }

    #[test]
    fn making_a_file_never_flattens_one_that_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notas.txt");
        std::fs::write(&path, "no me borres").unwrap();

        assert!(make_file(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "no me borres");
    }

    #[test]
    fn a_copy_says_how_many_bytes_it_moved_exactly() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), vec![7u8; 1234]).unwrap();

        let done = copy(&dir.path().join("a"), &dir.path().join("b")).unwrap();
        // Exact, never the rounded form: a program comparing two rounded numbers
        // compares two lies.
        assert_eq!(done.bytes, 1234);
        assert_eq!(done.what, Did::Copied);
        assert_eq!(done.to.as_deref(), Some(dir.path().join("b").as_path()));
    }

    #[test]
    fn copying_over_something_is_refused_rather_than_assumed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), "nuevo").unwrap();
        std::fs::write(dir.path().join("b"), "viejo, y quiero conservarlo").unwrap();

        assert!(copy(&dir.path().join("a"), &dir.path().join("b")).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("b")).unwrap(),
            "viejo, y quiero conservarlo"
        );
    }

    #[test]
    fn a_whole_folder_copies_with_what_is_under_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src/hondo")).unwrap();
        std::fs::write(dir.path().join("src/uno.txt"), "aa").unwrap();
        std::fs::write(dir.path().join("src/hondo/dos.txt"), "bbb").unwrap();

        let done = copy(&dir.path().join("src"), &dir.path().join("dest")).unwrap();
        assert_eq!(done.bytes, 5);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("dest/hondo/dos.txt")).unwrap(),
            "bbb"
        );
    }

    #[test]
    fn copying_a_link_copies_the_link_and_not_what_it_points_at() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("real.txt"), "x").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real.txt"), dir.path().join("src/enlace"))
            .unwrap();

        copy(&dir.path().join("src"), &dir.path().join("dest")).unwrap();
        // Following it would duplicate the target, and a link to an ancestor
        // would loop until the disk filled.
        let copied = dir.path().join("dest/enlace");
        assert!(copied.symlink_metadata().unwrap().file_type().is_symlink());
    }

    #[test]
    fn removing_a_link_removes_the_link_and_not_somebody_elses_file() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        std::fs::write(&real, "x").unwrap();
        let link = dir.path().join("enlace");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        remove(&link).unwrap();
        assert!(link.symlink_metadata().is_err());
        assert!(real.is_file(), "the file it pointed at is not the link");
    }

    #[test]
    fn a_move_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), "contenido").unwrap();

        let done = move_to(&dir.path().join("a"), &dir.path().join("b")).unwrap();
        assert_eq!(done.what, Did::Moved);
        assert!(!dir.path().join("a").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("b")).unwrap(),
            "contenido"
        );
    }

    #[test]
    fn removing_a_folder_takes_what_is_under_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/b/c.txt"), "x").unwrap();

        assert_eq!(remove(&dir.path().join("a")).unwrap().what, Did::Removed);
        assert!(!dir.path().join("a").exists());
    }

    // ─────────────────────────────────────────── rehearsing before doing (D1)

    #[test]
    fn a_rehearsal_leaves_the_disk_exactly_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("origen"), "doce bytes").unwrap();
        std::fs::create_dir(dir.path().join("carpeta")).unwrap();

        let before: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();

        foresee_make_directory(&dir.path().join("nueva")).unwrap();
        foresee_make_file(&dir.path().join("nuevo.txt")).unwrap();
        foresee_copy(&dir.path().join("origen"), &dir.path().join("destino")).unwrap();
        foresee_move(&dir.path().join("origen"), &dir.path().join("otro")).unwrap();
        foresee_remove(&dir.path().join("carpeta")).unwrap();

        let after: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();

        // The whole promise of a rehearsal, and the one that cannot be assumed:
        // five of them, including one that rehearses deleting a folder.
        assert_eq!(before.len(), after.len(), "{before:?} became {after:?}");
        assert!(dir.path().join("origen").exists());
        assert!(dir.path().join("carpeta").exists());
        assert!(!dir.path().join("destino").exists());
    }

    #[test]
    fn a_rehearsal_and_the_operation_report_the_same_thing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("origen"), "doce bytes!").unwrap();

        let foreseen =
            foresee_copy(&dir.path().join("origen"), &dir.path().join("destino")).unwrap();
        let done = copy(&dir.path().join("origen"), &dir.path().join("destino")).unwrap();

        // Not a coincidence and not two implementations agreeing: `copy` calls
        // `foresee_copy`. A rehearsal that said "this would work" while the real
        // operation refused would be worse than having no rehearsal at all.
        assert_eq!(foreseen, done);
    }

    #[test]
    fn a_rehearsal_refuses_exactly_where_the_operation_would() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ya-esta"), "x").unwrap();

        let foreseen = foresee_make_file(&dir.path().join("ya-esta")).unwrap_err();
        let real = make_file(&dir.path().join("ya-esta")).unwrap_err();
        assert_eq!(foreseen.word(), real.word());

        let foreseen = foresee_remove(&dir.path().join("fantasma")).unwrap_err();
        let real = remove(&dir.path().join("fantasma")).unwrap_err();
        assert_eq!(foreseen.word(), real.word());
    }

    #[test]
    fn removing_a_folder_says_how_much_it_destroyed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/b/uno.txt"), "12345").unwrap();
        std::fs::write(dir.path().join("a/dos.txt"), "123").unwrap();

        // It used to report 0 for a folder, which tells a person nothing about
        // what they just lost and tells an agent rehearsing an `rm` nothing at
        // all about how much is at stake.
        assert_eq!(remove(&dir.path().join("a")).unwrap().bytes, 8);
    }

    #[test]
    fn weighing_a_tree_does_not_follow_a_link_out_of_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("carpeta")).unwrap();
        std::fs::write(dir.path().join("grande"), vec![b'x'; 5000]).unwrap();
        std::os::unix::fs::symlink(dir.path().join("grande"), dir.path().join("carpeta/enlace"))
            .unwrap();

        // Following it would report somebody else's file as part of what is
        // about to be destroyed — and on a link to an ancestor, forever.
        assert_eq!(
            foresee_remove(&dir.path().join("carpeta")).unwrap().bytes,
            0
        );
    }

    #[test]
    fn removing_something_that_is_not_there_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let error = remove(&dir.path().join("fantasma")).unwrap_err();
        assert!(matches!(error, FileError::Absent(_)), "got {error:?}");
    }

    #[test]
    fn the_word_for_each_operation_is_stable_and_not_a_sentence() {
        // What a program matches on. Changing one of these breaks every caller
        // that reads the structured face, which is why they are here.
        assert_eq!(Did::MadeDirectory.word(), "made_directory");
        assert_eq!(Did::Copied.word(), "copied");
        assert_eq!(Did::Moved.word(), "moved");
        assert_eq!(Did::Removed.word(), "removed");
        assert_eq!(Did::MadeFile.word(), "made_file");
    }

    // ──────────────────────────────────────────────────────────────── patterns

    #[test]
    fn a_star_stands_for_any_run_of_characters() {
        assert!(matches("*.txt", "notas.txt"));
        assert!(matches("*.txt", ".txt"));
        assert!(!matches("*.txt", "notas.md"));
        assert!(matches("notas*", "notas.txt"));
        assert!(matches("*", "cualquier-cosa"));
    }

    #[test]
    fn a_question_mark_stands_for_exactly_one() {
        assert!(matches("a?c", "abc"));
        assert!(!matches("a?c", "ac"));
        assert!(!matches("a?c", "abbc"));
    }

    #[test]
    fn a_star_never_crosses_a_folder_separator() {
        // Without this, deleting `*` in one folder reaches into every folder
        // below it.
        assert!(!matches("*.txt", "notas/x.txt"));
        assert!(!matches("*", "a/b"));
    }

    #[test]
    fn many_stars_do_not_take_exponential_time() {
        // A pattern somebody types by accident. The recursive form never returns
        // on this one.
        let pattern = "*".repeat(40) + "b";
        assert!(!matches(&pattern, &"a".repeat(60)));
    }

    #[test]
    fn a_pattern_matches_whole_names_and_not_pieces_of_them() {
        assert!(!matches("nota", "notas.txt"));
        assert!(matches("notas.txt", "notas.txt"));
    }

    #[test]
    fn expanding_a_pattern_never_reaches_a_hidden_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("visible.txt"), "x").unwrap();
        std::fs::write(dir.path().join(".oculto.txt"), "x").unwrap();

        let found = expand(dir.path(), "*.txt").unwrap();
        // The rule that keeps `rm *` from deleting somebody's configuration.
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("visible.txt"));
    }

    #[test]
    fn a_pattern_that_starts_with_a_dot_does_reach_hidden_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".bashrc"), "x").unwrap();
        // Typing the dot is the person saying they mean those.
        assert_eq!(expand(dir.path(), ".*").unwrap().len(), 1);
    }

    #[test]
    fn a_pattern_that_matches_nothing_is_an_answer_and_not_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        // The caller is the one that knows whether zero results is a problem.
        assert!(expand(dir.path(), "*.rs").unwrap().is_empty());
    }

    // ────────────────────────────────────────────────────────────── saying sizes

    #[test]
    fn a_size_under_a_kilobyte_is_printed_whole() {
        assert_eq!(Size(0).to_string(), "0 B");
        assert_eq!(Size(937).to_string(), "937 B");
    }

    #[test]
    fn larger_sizes_round_to_something_a_person_reads_at_a_glance() {
        assert_eq!(Size(1024).to_string(), "1.0 kB");
        assert_eq!(Size(1536).to_string(), "1.5 kB");
        assert_eq!(Size(2 * 1024 * 1024).to_string(), "2.0 MB");
        assert_eq!(Size(3 * 1024 * 1024 * 1024).to_string(), "3.0 GB");
    }
}

// ─────────────────────────────────────────────────────── changing what is there
//
// `vault/01-Filosofia/Filosofia-Fundacional.md`: the objective is that an LLM
// works better here than anywhere else, and every one of these returns a
// [`Done`] describing exactly what happened rather than only succeeding. A
// program that has to re-list a directory to find out what a copy did is a
// program guessing, and guessing is what the structured face exists to remove.

/// What an operation actually did, in the terms it did it in.
///
/// Returned rather than printed. The human face formats this; the machine face
/// serialises it. Both read the same fact, which is the only way the two cannot
/// drift apart — a second code path that prints its own version of events is a
/// second version of events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub what: Did,
    pub path: PathBuf,
    /// Where it ended up, for the operations that move something.
    pub to: Option<PathBuf>,
    /// Bytes involved, exact. Never the rounded form — that is for a human eye
    /// and a program comparing two rounded numbers compares two lies.
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Did {
    MadeDirectory,
    MadeFile,
    Copied,
    Moved,
    Removed,
}

impl Did {
    /// The word a program matches on. Stable, lowercase, never translated —
    /// this is an identifier and not a sentence.
    pub fn word(self) -> &'static str {
        match self {
            Did::MadeDirectory => "made_directory",
            Did::MadeFile => "made_file",
            Did::Copied => "copied",
            Did::Moved => "moved",
            Did::Removed => "removed",
        }
    }
}

// ───────────────────────────────────────────────────── rehearsing before doing
//
// `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **D1**. The fourth
// cost an LLM pays is what a mistake costs, and it is the one that changes
// behaviour rather than efficiency: in a system where everything is
// irreversible a rational agent turns timid — it asks too much, tries too
// little — and that does not read as prudence, it reads as incapacity.
//
// Today the only way anything can find out what a command does is to run it.
//
// ## Why these are not a second implementation
//
// Each `foresee_*` is **the check half of the real operation**, and the real
// operation calls it. There is no path where a rehearsal and the thing it
// rehearses can disagree about whether something is allowed, because there is
// one piece of code deciding — a second copy that answered "this would work"
// while the real one refused would be worse than having no rehearsal at all.
//
// What a rehearsal cannot promise is the future. It reports what is true now,
// and the machine can change between the two — a rehearsal is a **prediction**
// and it is named `foresee` rather than `check` for that reason.

/// The weight of something, following nothing.
///
/// A directory is the sum of what is under it. Both the rehearsal and the real
/// operation use this, so `rm` can say how much it destroyed instead of the `0`
/// it used to report for a folder — which told a person nothing about what they
/// had just lost.
fn weigh(path: &Path) -> u64 {
    let Ok(meta) = path.symlink_metadata() else {
        return 0;
    };
    if meta.file_type().is_symlink() {
        // A link weighs nothing of its own, and following it would report
        // somebody else's file as part of what is about to go.
        return 0;
    }
    if !meta.is_dir() {
        return meta.len();
    }
    let Ok(reader) = std::fs::read_dir(path) else {
        return 0;
    };
    reader
        .filter_map(Result::ok)
        .map(|entry| weigh(&entry.path()))
        .sum()
}

/// What [`make_directory`] would do, without doing it.
pub fn foresee_make_directory(path: &Path) -> Result<Done, FileError> {
    // Checked first so that an existing directory is refused rather than
    // reported as made. "It is already there" and "I made it" are different
    // facts, and an agent that cannot tell them apart cannot tell whether it is
    // repeating itself.
    if path.symlink_metadata().is_ok() {
        return Err(FileError::Exists(path.to_path_buf()));
    }
    Ok(Done {
        what: Did::MadeDirectory,
        path: path.to_path_buf(),
        to: None,
        bytes: 0,
    })
}

/// What [`make_file`] would do, without doing it.
pub fn foresee_make_file(path: &Path) -> Result<Done, FileError> {
    if path.symlink_metadata().is_ok() {
        return Err(FileError::Exists(path.to_path_buf()));
    }
    Ok(Done {
        what: Did::MadeFile,
        path: path.to_path_buf(),
        to: None,
        bytes: 0,
    })
}

/// What [`copy`] would do, without doing it.
pub fn foresee_copy(from: &Path, to: &Path) -> Result<Done, FileError> {
    from.symlink_metadata().map_err(|e| classify(from, e))?;
    // Refused rather than merged or flattened. Overwriting is a separate
    // request, and one that costs somebody a file when it is assumed.
    if to.symlink_metadata().is_ok() {
        return Err(FileError::Exists(to.to_path_buf()));
    }
    Ok(Done {
        what: Did::Copied,
        path: from.to_path_buf(),
        to: Some(to.to_path_buf()),
        bytes: weigh(from),
    })
}

/// What [`move_to`] would do, without doing it.
pub fn foresee_move(from: &Path, to: &Path) -> Result<Done, FileError> {
    let meta = from.symlink_metadata().map_err(|e| classify(from, e))?;
    if to.symlink_metadata().is_ok() {
        return Err(FileError::Exists(to.to_path_buf()));
    }
    Ok(Done {
        what: Did::Moved,
        path: from.to_path_buf(),
        to: Some(to.to_path_buf()),
        bytes: if meta.is_dir() { 0 } else { meta.len() },
    })
}

/// What [`remove`] would do, without doing it.
///
/// The one worth rehearsing most, and the only one whose rehearsal cannot be
/// checked afterwards: `/home` is decreed to be the one place no rollback of
/// ours can put back.
pub fn foresee_remove(path: &Path) -> Result<Done, FileError> {
    path.symlink_metadata().map_err(|e| classify(path, e))?;
    Ok(Done {
        what: Did::Removed,
        path: path.to_path_buf(),
        to: None,
        bytes: weigh(path),
    })
}

/// Make a directory, and every parent it needs.
///
/// Making the parents is what a person and an agent both mean by "make this
/// folder": failing on a missing parent forces a loop that recreates exactly
/// this, one level at a time.
pub fn make_directory(path: &Path) -> Result<Done, FileError> {
    let done = foresee_make_directory(path)?;
    std::fs::create_dir_all(path).map_err(|error| classify(path, error))?;
    Ok(done)
}

/// Make an empty file, refusing to flatten one that is already there.
pub fn make_file(path: &Path) -> Result<Done, FileError> {
    let done = foresee_make_file(path)?;
    // `create_new`, not `create`: the check above and the creation are two
    // moments, and between them something else can appear. The kernel deciding
    // is the only version with no gap in it.
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| classify(path, error))?;
    Ok(done)
}

/// Copy a file or a whole directory.
pub fn copy(from: &Path, to: &Path) -> Result<Done, FileError> {
    let foreseen = foresee_copy(from, to)?;
    let meta = from.symlink_metadata().map_err(|e| classify(from, e))?;

    let bytes = if meta.is_dir() {
        copy_tree(from, to)?
    } else {
        std::fs::copy(from, to).map_err(|error| classify(from, error))?
    };
    // The measured count, not the predicted one. They agree in every ordinary
    // case; when they do not, something changed between the two moments and the
    // truth is what landed on the disk.
    Ok(Done { bytes, ..foreseen })
}

fn copy_tree(from: &Path, to: &Path) -> Result<u64, FileError> {
    std::fs::create_dir_all(to).map_err(|error| classify(to, error))?;
    let mut bytes = 0;
    for entry in std::fs::read_dir(from).map_err(|error| classify(from, error))? {
        let entry = entry.map_err(|error| classify(from, error))?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        let meta = source
            .symlink_metadata()
            .map_err(|e| classify(&source, e))?;
        if meta.is_dir() {
            bytes += copy_tree(&source, &target)?;
        } else if meta.file_type().is_symlink() {
            // Copied as a link, not as what it points at. Following it would
            // duplicate the target and could loop forever on a link to an
            // ancestor.
            let dest = std::fs::read_link(&source).map_err(|e| classify(&source, e))?;
            std::os::unix::fs::symlink(dest, &target).map_err(|e| classify(&target, e))?;
        } else {
            bytes += std::fs::copy(&source, &target).map_err(|error| classify(&source, error))?;
        }
    }
    Ok(bytes)
}

/// Move something, across filesystems if it has to.
pub fn move_to(from: &Path, to: &Path) -> Result<Done, FileError> {
    let done = foresee_move(from, to)?;

    match std::fs::rename(from, to) {
        Ok(()) => {}
        // `EXDEV`: the two are on different filesystems and the kernel will not
        // rename across them. `/home` and `/opt/thalyx` are separate subvolumes,
        // so this is the ordinary case here and not an exotic one — copy, then
        // remove, and only remove once the copy is on disk.
        Err(error) if error.raw_os_error() == Some(libc_exdev()) => {
            copy(from, to)?;
            remove(from)?;
        }
        Err(error) => return Err(classify(from, error)),
    }
    Ok(done)
}

/// `EXDEV`, spelled out rather than linked: this crate has no libc dependency
/// and one number is not worth one.
fn libc_exdev() -> i32 {
    18
}

/// Delete a file, a link, or a directory and what is under it.
pub fn remove(path: &Path) -> Result<Done, FileError> {
    let done = foresee_remove(path)?;
    let meta = path.symlink_metadata().map_err(|e| classify(path, e))?;

    if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path).map_err(|error| classify(path, error))?;
    } else {
        // `remove_file` on a symlink deletes the link. Anything that resolved it
        // first would delete what it points at, which is somebody else's file.
        std::fs::remove_file(path).map_err(|error| classify(path, error))?;
    }
    Ok(done)
}

// ──────────────────────────────────────────────────────────────────── `*.txt`

// ─────────────────────────────────────────────────────── walking a whole tree

/// How many files a whole-tree walk will look at before refusing.
///
/// Twenty thousand, and the number is about time rather than memory: past it,
/// every answer this machine can give arrives after the person has stopped
/// waiting for it. `Superficie-para-el-LLM.md` counts an answer that never
/// arrives as the most expensive kind of being wrong, because it costs the
/// whole session and leaves nothing to learn from.
///
/// It moved here from `thalyx-graph` when a second thing started walking trees.
/// Two ceilings that had to be the same number would have drifted the first
/// time one of them was tuned, and the symptom would have been `buscar` and
/// `contenido` disagreeing about whether a tree is searchable.
pub const CEILING: usize = 20_000;

/// Directories never worth walking into.
///
/// Build outputs and version control internals would swamp any answer with
/// paths no one asks about, and `.git` alone can be larger than the project.
///
/// **Anything beginning with a dot**, and that rule is the one that matters.
/// The named list was a list of the things that had gone wrong so far, and it
/// went wrong again on 2026-08-10: a session starts at `/home`, `indexar` with
/// nothing after it indexes where it stands, and on Cesar's machine that walked
/// into `.cargo/registry` and `.rustup` — every source file of every crate he
/// has ever downloaded, plus the whole Rust standard library. The run never
/// finished.
///
/// A hidden directory is where a machine keeps what it manages for itself. A
/// person indexing their work does not mean their caches, and adding `.cargo`
/// to a list of names would only have waited for `.local/share` to be next.
fn is_ignored(path: &Path) -> bool {
    match path.file_name().and_then(|n| n.to_str()) {
        Some("target" | "node_modules" | "__pycache__" | "dist" | "build") => true,
        Some(name) => name.starts_with('.'),
        None => false,
    }
}

/// The walk, in one place, because everything that walks has to agree exactly.
///
/// The index records the set of files a tree holds and the freshness check
/// counts that set again. If the two ever disagree about which files belong,
/// **every index is stale the moment it is written** — and it looks like a
/// staleness bug rather than like two walks. That is what happened the first
/// time the hidden-directory rule was added to only one of them: eleven tests
/// failed, none of them about hidden directories, because `tempfile` names its
/// directories `.tmpXXXXXX`.
///
/// There are now four callers and not two — the index build, the freshness
/// count, `encontrar` and `contenido` — and the third and fourth are the ones a
/// person compares against the first. A `contenido` that reached into `.git`
/// where `buscar` does not would answer about a file the index has never heard
/// of, and the person would conclude the index is broken.
///
/// The root itself is never filtered. A person who names `~/.config` has named
/// it on purpose, and a filter that refused the tree it was handed would answer
/// "nothing here" about a directory full of files.
pub fn walk(
    root: &Path,
) -> walkdir::FilterEntry<walkdir::IntoIter, fn(&walkdir::DirEntry) -> bool> {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !is_ignored(entry.path()))
}

/// Whether a name matches a pattern of `*` and `?`.
///
/// Written here rather than pulled in, for the same reason as the cpio and the
/// Btrfs writer: the image holds the kernel and one program.
///
/// `*` does not cross a `/`, which is the rule every shell follows and the one
/// that keeps `*.txt` from reaching into subdirectories a person did not name.
pub fn matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();

    // Iterative with a backtrack point rather than recursion: a pattern of forty
    // stars against a long name is a stack the recursive form cannot afford, and
    // a pattern is exactly the kind of input somebody types by accident.
    let (mut p, mut n) = (0usize, 0usize);
    let (mut star, mut retry) = (None, 0usize);

    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            retry = n;
            p += 1;
        } else if let Some(at) = star {
            // A `*` never swallows a separator: `*.txt` must not match
            // `notas/x.txt`, or a person deleting `*` in one folder would reach
            // into every folder below it.
            if name[retry] == '/' {
                return false;
            }
            p = at + 1;
            retry += 1;
            n = retry;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// Everything in `folder` whose name matches, in listing order.
///
/// Empty when nothing matches, and that is the answer rather than an error: the
/// caller is the one that knows whether zero results is a problem.
pub fn expand(folder: &Path, pattern: &str) -> Result<Vec<PathBuf>, FileError> {
    let listing = list(folder)?;
    Ok(listing
        .entries
        .iter()
        .filter(|entry| {
            let name = entry.name.to_string_lossy();
            // A pattern that does not itself begin with a dot never matches a
            // hidden name — the same rule as `ls`, and the one that keeps
            // `rm *` from deleting somebody's configuration.
            (!is_hidden(&entry.name) || pattern.starts_with('.')) && matches(pattern, &name)
        })
        .map(|entry| folder.join(&entry.name))
        .collect())
}
