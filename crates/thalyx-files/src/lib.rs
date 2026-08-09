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

fn kind_of(path: &Path, meta: &std::fs::Metadata) -> Kind {
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
fn not_text(bytes: &[u8]) -> Option<&'static str> {
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
fn classify(path: &Path, error: std::io::Error) -> FileError {
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
