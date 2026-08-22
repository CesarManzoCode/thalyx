//! The face a program reads, built from the same facts the person is shown.
//!
//! `vault/01-Filosofia/Filosofia-Fundacional.md` decrees the objective: an LLM
//! works better here than anywhere else, and everything else is a means. The
//! engineering consequence written there is that **every thing is born with two
//! faces** — the human one and a structured one a program can ask for and parse
//! — and that the second is not added afterwards, because added afterwards is
//! not added.
//!
//! Until this module it had been added afterwards. The operations already
//! returned a [`Done`] instead of printing, which is the hard half; but the only
//! thing that ever read one was the human printer, so the decree was written
//! down and not built.
//!
//! ## What makes this the machine face and not just another printer
//!
//! Three differences from what a person sees, and all three are the tie-break
//! rule of the decree — when the two faces disagree, the LLM wins and the human
//! keeps full access by another route:
//!
//! 1. **Nothing is hidden.** `ls` hides dotfiles from a person because Cesar's
//!    home holds thirty-five of them before the first folder he made. Hiding
//!    them from a program that asked is taking capability away, so here every
//!    entry is present and the caller filters if it wants to.
//! 2. **Sizes are exact.** `1.2 kB` is a number that lost precision; two
//!    programs comparing two rounded numbers compare two lies.
//! 3. **Silence is never an answer.** `cd` prints nothing to a person because
//!    the next prompt already says where they are. A parser waiting on a stream
//!    cannot tell silence from a hang, so every operation answers.
//!
//! ## One line in, exactly one object out
//!
//! This is the framing contract, and it is here because the project has already
//! paid for getting it wrong once. On 2026-08-08 the agent's prompt carried a
//! random marker saying where an answer *began* and nothing saying where it
//! ended, and the conclusion written into the vault was that **a boundary
//! defined on one side only is not a boundary**.
//!
//! An operation with several targets is the case that would have repeated it:
//! `rm *.log` catching three files could print three objects, and a caller has
//! no way to know it should read three. So a verb that can touch more than one
//! thing answers with one object carrying [`batch`] results, and the count is
//! inside the answer rather than something the reader has to discover.
//!
//! ## Why the shape is written out rather than derived
//!
//! Nothing here is `#[derive(Serialize)]`. A derived shape is decided by Rust's
//! variant names, so renaming `Did::Copied` would silently rename a field that
//! something else parses. The wire shape is a decision, and decisions in this
//! project are written down where they can be read and tested.

use crate::search::{Found, Hit, Named};
use crate::window::Page;
use crate::{Did, Done, Entry, Excerpt, FileError, Kind};
use serde_json::{Map, Value, json};
use std::ffi::OsStr;
use std::path::Path;

/// The words a program matches on. Stable, lowercase, never translated.
///
/// Separate from the sentences in [`FileError`]'s `Display`, which are English
/// prose meant for a person and will be reworded. A caller that matched on the
/// sentence would break the first time somebody improved it.
impl FileError {
    pub fn word(&self) -> &'static str {
        match self {
            FileError::Absent(_) => "absent",
            FileError::IsDirectory(_) => "is_directory",
            FileError::NotText { .. } => "not_text",
            FileError::Unreadable { .. } => "unreadable",
            FileError::Exists(_) => "exists",
            FileError::NotADirectory(_) => "not_a_directory",
            FileError::NothingAsked => "nothing_asked",
            FileError::TreeTooLarge { .. } => "tree_too_large",
        }
    }

    /// What would get past this, as a word.
    ///
    /// `Superficie-para-el-LLM.md`, punto **A2**. An error that only says what
    /// went wrong costs the caller a whole cycle of reasoning and trying; one
    /// that names the way out is documentation delivered at the exact moment it
    /// is useful, and it costs one field.
    ///
    /// A word rather than a sentence, for the same reason as [`Self::word`] —
    /// the sentence in `message` is English that will be reworded, and anything
    /// matching on it breaks the first time somebody improves it.
    ///
    /// **`cannot` is an answer.** An unreadable path and a binary file have no
    /// remedy inside Thalyx, and inventing an encouraging one would send a
    /// caller into a loop retrying something that will never work.
    pub fn remedy(&self) -> &'static str {
        match self {
            FileError::Absent(_) => "look_first",
            FileError::IsDirectory(_) => "use_list",
            FileError::NotText { .. } => "cannot",
            FileError::Unreadable { .. } => "cannot",
            FileError::Exists(_) => "remove_or_rename",
            FileError::NotADirectory(_) => "name_a_folder",
            FileError::NothingAsked => "say_what_to_look_for",
            // The one remedy that is a real instruction rather than a hint,
            // because it is the only thing that works: no flag, no retry and
            // no amount of patience turns a million-file tree into an answer.
            FileError::TreeTooLarge { .. } => "name_a_smaller_tree",
        }
    }
}

/// A name or path, and whether it survived the trip to text.
///
/// Paths on Linux are bytes, not text, so a name that is not valid UTF-8 can
/// only be shown lossily — and a lossy name fed back as an argument names a
/// different file or none. A person seeing `?` in a listing works that out; a
/// program cannot, so it is told.
fn as_text(raw: &OsStr) -> (String, bool) {
    match raw.to_str() {
        Some(text) => (text.to_string(), true),
        None => (raw.to_string_lossy().into_owned(), false),
    }
}

/// Assemble one object, so that `op` and `ok` cannot be forgotten by any caller.
///
/// `exact` is false when any path in the object had to be converted lossily; it
/// is the flag that tells a program these strings must not be handed back as
/// arguments. It is folded in here rather than by each caller because the
/// caller that forgets it is the one that reports a wrong name confidently.
fn object(op: &str, ok: bool, exact: bool, fields: Map<String, Value>) -> String {
    let mut out = Map::new();
    out.insert("op".into(), json!(op));
    out.insert("ok".into(), json!(ok));
    if !exact {
        out.insert("exact".into(), json!(false));
    }
    out.extend(fields);
    // `to_string`, never pretty: one object per line is what makes a stream
    // readable without a parser that tracks nesting across lines.
    Value::Object(out).to_string()
}

fn fields(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

/// How much of a long answer this is, and how to ask for the rest.
///
/// `Superficie-para-el-LLM.md`, punto **B1**. The same six keys on every answer
/// that can be long, because a caller that has to learn a different paging shape
/// per verb pays the discovery cost again for each one — and the point of the
/// catalogue is that it pays it once.
///
/// All six are always present, including the two that are `null` or `false` on
/// the ordinary day. A key that only appears when a directory is enormous is a
/// key nobody writes the branch for, and then the branch is missing on exactly
/// the day it was needed.
pub fn window_fields<T>(page: &crate::window::Page<T>) -> Vec<(&'static str, Value)> {
    vec![
        ("total", json!(page.total)),
        ("sent", json!(page.rows.len())),
        // Where this page starts inside the total, so a caller can say how far
        // through it is without keeping a running count of its own.
        ("before", json!(page.before)),
        ("more", json!(page.more)),
        ("cursor", json!(page.next)),
        // The rule of honesty of `FS-en-Grafo`, generalised: whether the
        // collection is still the one this cursor came from travels in the same
        // object as the rows, because a caveat sent separately is one that gets
        // dropped.
        ("continuity", json!(page.continuity.word())),
    ]
}

/// A directory, whole: nothing filtered, sizes exact, and what could not be read
/// kept apart from what is not there.
///
/// The entries arrive already cut to a window. What could not be read does not:
/// it is short by nature and it is the half of the answer rule 10 is about, so
/// paging it away would be turning "I could not tell" into silence.
pub fn listing(
    path: &Path,
    page: &crate::window::Page<Entry>,
    unreadable: &[(std::ffi::OsString, String)],
) -> String {
    let (place, place_exact) = as_text(path.as_os_str());
    let mut exact = place_exact;

    let entries: Vec<Value> = page
        .rows
        .iter()
        .map(|entry| {
            let (value, entry_exact) = entry_object(entry);
            exact &= entry_exact;
            value
        })
        .collect();

    let unreadable: Vec<Value> = unreadable
        .iter()
        .map(|(name, why)| {
            let (name, name_exact) = as_text(name);
            exact &= name_exact;
            json!({ "name": name, "why": why })
        })
        .collect();

    let mut carried = vec![
        ("path", json!(place)),
        ("entries", json!(entries)),
        // Always present, even empty. A caller that only sees the key when
        // something failed is a caller that never wrote the branch — and
        // rule 10 says a failure to read must not arrive as an absence.
        ("unreadable", json!(unreadable)),
    ];
    carried.extend(window_fields(page));

    object("list", true, exact, fields(carried))
}

fn entry_object(entry: &Entry) -> (Value, bool) {
    let (name, mut exact) = as_text(&entry.name);
    let mut out = Map::new();
    out.insert("name".into(), json!(name));
    exact &= kind_into(&mut out, &entry.kind);
    (Value::Object(out), exact)
}

/// What a thing is, in the keys every face uses for it.
///
/// One function because a listing row and a search row describe the same fact
/// about the same filesystem. Written twice, the second one would have grown a
/// `size` where the first has `bytes` — which is precisely the drift that made
/// `list_one` answer a `Listing` instead of a shape of its own.
///
/// Returns whether every path it wrote survived the trip to text.
fn kind_into(out: &mut Map<String, Value>, kind: &Kind) -> bool {
    let mut exact = true;
    match kind {
        Kind::Directory => {
            out.insert("kind".into(), json!("directory"));
        }
        Kind::File { bytes } => {
            out.insert("kind".into(), json!("file"));
            out.insert("bytes".into(), json!(bytes));
        }
        Kind::Link { to, broken } => {
            let (destination, to_exact) = as_text(to.as_os_str());
            exact &= to_exact;
            out.insert("kind".into(), json!("link"));
            out.insert("to".into(), json!(destination));
            // Carried as its own field rather than by omitting `to`: a broken
            // link and an absent one are different facts about the machine, and
            // a caller that saw only "no destination" would report the wrong one.
            out.insert("broken".into(), json!(broken));
        }
        Kind::Other(what) => {
            out.insert("kind".into(), json!("other"));
            out.insert("what".into(), json!(what));
        }
    }
    exact
}

/// Files whose name matched, as one object.
///
/// `looked_at` is here and is not the same number as `total`. A search of
/// eleven thousand files that matched nothing and a search of four that matched
/// nothing are different answers, and only the first one means *this pattern is
/// wrong*; without the count the caller cannot tell them apart and re-runs the
/// search somewhere else to find out.
pub fn found_by_name(
    root: &Path,
    pattern: &str,
    found: &Found<Named>,
    page: &Page<&Named>,
) -> String {
    let (place, mut exact) = as_text(root.as_os_str());

    let rows: Vec<Value> = page
        .rows
        .iter()
        .map(|row| {
            let mut out = Map::new();
            out.insert("path".into(), json!(row.path));
            exact &= kind_into(&mut out, &row.kind);
            Value::Object(out)
        })
        .collect();

    let mut carried = vec![
        ("root", json!(place)),
        ("pattern", json!(pattern)),
        ("matches", json!(rows)),
        ("looked_at", json!(found.looked_at)),
        ("unreadable", json!(unreadable_rows(&found.unreadable))),
    ];
    carried.extend(window_fields(page));
    object("find", true, exact, fields(carried))
}

/// Lines that held the text, as one object.
pub fn found_in_contents(root: &Path, text: &str, found: &Found<Hit>, page: &Page<&Hit>) -> String {
    let (place, exact) = as_text(root.as_os_str());

    let rows: Vec<Value> = page
        .rows
        .iter()
        .map(|row| {
            json!({
                "path": row.path,
                "line": row.line,
                "text": row.text,
                // Whether the line arrived whole. A caller that pasted a cut
                // line back into a file would be writing something the file
                // never said, and nothing else in the row would have told it.
                "cut": row.cut,
            })
        })
        .collect();

    let mut carried = vec![
        ("root", json!(place)),
        ("text", json!(text)),
        ("hits", json!(rows)),
        ("looked_at", json!(found.looked_at)),
        // Its own number and not part of `unreadable`: a binary was read
        // perfectly well and simply has no lines to answer with. A caller told
        // these were unreadable would go looking for a permission problem.
        ("not_text", json!(found.not_text)),
        ("unreadable", json!(unreadable_rows(&found.unreadable))),
    ];
    carried.extend(window_fields(page));
    object("grep", true, exact, fields(carried))
}

/// What could not be read, never paged away.
///
/// It is short by nature and it is the half of the answer rule 10 is about, so
/// cutting it to a window would turn "I could not tell" into silence — and
/// silence is what a caller reads as "not there".
fn unreadable_rows(unreadable: &[(String, String)]) -> Vec<Value> {
    unreadable
        .iter()
        .map(|(path, why)| json!({ "path": path, "why": why }))
        .collect()
}

/// A file's contents, with the two byte counts kept apart.
pub fn excerpt(path: &Path, excerpt: &Excerpt) -> String {
    let (place, exact) = as_text(path.as_os_str());
    object(
        "read",
        true,
        exact,
        fields([
            ("path", json!(place)),
            // What the file holds, not what was handed over. A caller reading
            // only one number and getting the shorter one believes it has the
            // file.
            ("bytes", json!(excerpt.of_bytes)),
            ("truncated", json!(excerpt.truncated)),
            // Of the whole file even when the text above is a piece of it, so
            // that "is what I read still true" is a comparison instead of a
            // second read of the same bytes.
            ("sha256", json!(excerpt.digest)),
            ("text", json!(excerpt.text)),
        ]),
    )
}

/// Where the session is standing.
///
/// `op` says which question was asked — `pwd` and a successful `cd` produce the
/// same fact and are not the same event, and a program driving a session needs
/// to know which of its lines was answered.
pub fn location(op: &str, path: &Path) -> String {
    let (place, exact) = as_text(path.as_os_str());
    object(op, true, exact, fields([("path", json!(place))]))
}

/// One element of a [`batch`]: what happened to one of the things named.
///
/// `did` is what happened, and it is not the same question as the `op` on the
/// answer around it. `mv` across a filesystem boundary falls back to
/// copy-and-delete — which `/home` and `/opt/thalyx` being separate subvolumes
/// makes the ordinary case here, not an exotic one — and `did` stays `moved`,
/// because that is what is true of the disk.
pub fn fact(done: &Done) -> Value {
    let (path, mut exact) = as_text(done.path.as_os_str());
    let mut out = Map::new();
    out.insert("ok".into(), json!(true));
    out.insert("did".into(), json!(done.what.word()));
    out.insert("path".into(), json!(path));
    out.insert("bytes".into(), json!(done.bytes));

    if let Some(to) = &done.to {
        let (destination, to_exact) = as_text(to.as_os_str());
        exact &= to_exact;
        out.insert("to".into(), json!(destination));
    }
    out.insert("undo".into(), undo_for(done));
    if !exact {
        out.insert("exact".into(), json!(false));
    }

    Value::Object(out)
}

/// How to put this back, or `null` and the reason there is no way.
///
/// `Superficie-para-el-LLM.md`, punto **D3**. It is the same idea as
/// [`FileError::remedy`] pointed at a success instead of a failure, and the
/// `null` case is the one that earns it: `/home` is decreed to be the one place
/// no rollback of ours can put back, so a delete there is final — and an agent
/// has to know that **before**, not after.
///
/// Rule 10 applied to what can be reversed: *no way to undo this* and *nothing
/// said about undoing this* are two different facts, and only one of them is
/// acceptable to leave a caller holding.
fn undo_for(done: &Done) -> Value {
    let made = match done.what {
        // What a copy created is the destination, not the source it was read
        // from. Undoing a copy by removing `path` would delete the original.
        Did::Copied => done.to.as_ref(),
        Did::MadeDirectory | Did::MadeFile => Some(&done.path),
        _ => None,
    };

    if let Some(made) = made {
        let (path, _) = as_text(made.as_os_str());
        return json!({ "op": "remove", "path": path });
    }

    if done.what == Did::Moved
        && let Some(to) = &done.to
    {
        let (from, _) = as_text(done.path.as_os_str());
        let (landed, _) = as_text(to.as_os_str());
        return json!({ "op": "move", "from": landed, "to": from });
    }

    // A delete. Nothing here can put it back, and saying so is the point.
    Value::Null
}

/// One element of a [`batch`] that did not happen, and why.
///
/// Both a word and a sentence. The word is what a program matches on; the
/// sentence is the same English a person would have been shown, carried so that
/// an agent relaying a failure to somebody does not have to invent the wording.
pub fn problem(error: &FileError) -> Value {
    let mut out = Map::new();
    out.insert("ok".into(), json!(false));
    out.insert("error".into(), json!(error.word()));
    out.insert("remedy".into(), json!(error.remedy()));
    out.insert("message".into(), json!(error.to_string()));

    if let Some(raw) = path_in(error) {
        let (text, exact) = as_text(raw);
        out.insert("path".into(), json!(text));
        if !exact {
            out.insert("exact".into(), json!(false));
        }
    }

    Value::Object(out)
}

fn path_in(error: &FileError) -> Option<&OsStr> {
    match error {
        FileError::Absent(path)
        | FileError::IsDirectory(path)
        | FileError::Exists(path)
        | FileError::NotADirectory(path) => Some(path.as_os_str()),
        FileError::NotText { path, .. } | FileError::Unreadable { path, .. } => {
            Some(path.as_os_str())
        }
        FileError::TreeTooLarge { root, .. } => Some(root.as_os_str()),
        // The only error with no path in it, because nothing was named. A
        // `path` key carrying an empty string would read as a file called
        // nothing rather than as a question that was never asked.
        FileError::NothingAsked => None,
    }
}

/// Everything one typed line did, as one object.
///
/// The count lives in the answer. Without that, `rm *.log` catching three files
/// would print three objects and a caller would have no way to know it should
/// read three — the one-sided boundary this module's header is about.
///
/// `ok` is true only when **every** element succeeded. A partial success
/// reported as a success is how a caller moves on believing a loop finished.
pub fn batch(op: &str, results: Vec<Value>) -> String {
    let all_ok = results.iter().all(|item| item["ok"] == json!(true));
    let exact = results.iter().all(|item| item.get("exact").is_none());
    object(
        op,
        all_ok,
        exact,
        fields([("count", json!(results.len())), ("results", json!(results))]),
    )
}

/// A whole operation that failed before touching anything.
pub fn failure(op: &str, error: &FileError) -> String {
    let mut carried = Map::new();
    if let Value::Object(problem) = problem(error) {
        carried.extend(problem);
    }
    let exact = carried.remove("exact").is_none();
    carried.remove("ok");
    object(op, false, exact, carried)
}

/// The same, plus where the session is still standing.
///
/// Only `cd` needs it, and it needs it for the reason the human face prints
/// "You are still in …": somebody who is not told they did not move aims the
/// next thing they do at a place they are not. A program is worse off than a
/// person here, having no prompt to read the answer off.
pub fn stayed(op: &str, error: &FileError, still_at: &Path) -> String {
    let line = failure(op, error);
    let mut answer: Map<String, Value> = serde_json::from_str(&line).unwrap_or_default();
    let (place, exact) = as_text(still_at.as_os_str());
    answer.insert("still_at".into(), json!(place));
    if !exact {
        answer.insert("exact".into(), json!(false));
    }
    Value::Object(answer).to_string()
}

/// Which face is on, answered in the structured one.
///
/// `off` carries the words that turn it back, so the acknowledgement a person
/// gets after typing this by accident is also the way out of it. On the image
/// there is no second terminal to recover from, so a mode with no visible exit
/// is a mode that can strand somebody.
pub fn state(on: bool) -> String {
    object(
        "structured",
        true,
        true,
        fields([("structured", json!(on)), ("off", json!("structured off"))]),
    )
}

/// An answer from a part of the system that is not the file verbs.
///
/// This module started as the structured face of `thalyx-files` and is now the
/// structured face of the session, which is why this exists: the shape of an
/// answer — `op`, `ok`, `exact` — is one decision and must have one
/// implementation, and a second builder somewhere else would be a second shape
/// three months later. When a third crate needs it, this moves out to its own;
/// until then, moving it would be churn for a caller that does not exist.
pub fn answer(op: &str, carried: Vec<(&'static str, Value)>) -> String {
    object(op, true, true, fields(carried))
}

/// Something that did not work, from a part of the system with its own errors.
///
/// The general form of [`failure`], which only knows [`FileError`]. It exists
/// because [`declined`] has nowhere to put a remedy, and the two verbs of
/// `search` shipped for a day answering `remedy: null` — punto **A2** lost
/// quietly, in the one field that tells a caller what to do next. A second
/// crate needing this is what turned that into a shape rather than a patch.
pub fn refused(op: &str, word: &str, remedy: &str, message: &str) -> String {
    object(
        op,
        false,
        true,
        fields([
            ("error", json!(word)),
            ("remedy", json!(remedy)),
            ("message", json!(message)),
        ]),
    )
}

/// The same, for something that did not work, has no remedy to offer, and is
/// not a [`FileError`].
pub fn declined(op: &str, word: &str, message: &str) -> String {
    object(
        op,
        false,
        true,
        fields([("error", json!(word)), ("message", json!(message))]),
    )
}

/// Something the session refused to attempt, which is not a file error.
///
/// `rm` with nothing after it never reaches the filesystem, so there is no
/// [`FileError`] to report — and a program that got silence there would wait
/// forever for an answer that was never coming.
pub fn refusal(op: &str, why: &str) -> String {
    object(
        op,
        false,
        true,
        fields([("error", json!("incomplete")), ("message", json!(why))]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn parse(line: &str) -> Value {
        serde_json::from_str(line).unwrap_or_else(|error| panic!("{line:?} is not JSON: {error}"))
    }

    /// A whole listing, through the window, which is the only way one is
    /// answered now.
    fn shown(path: &str, listing_of: crate::Listing) -> Value {
        let (page, unreadable) = listing_of
            .paged(&crate::window::Asked::default())
            .expect("a listing is sorted");
        parse(&listing(Path::new(path), &page, &unreadable))
    }

    // ─────────────────────────────────────── the shape every answer has in common

    #[test]
    fn every_answer_is_one_object_on_one_line() {
        let lines = [
            location("where", Path::new("/home")),
            failure("read", &FileError::Absent(PathBuf::from("/nope"))),
            refusal("remove", "which one"),
        ];
        for line in lines {
            // One line is the whole contract of the stream: a caller can read to
            // the newline and parse, with no bracket counting across lines.
            assert!(!line.contains('\n'), "{line:?} spans lines");
            assert!(parse(&line).is_object());
        }
    }

    #[test]
    fn an_answer_says_whether_it_worked_without_the_caller_inferring_it() {
        // Inferring success from the absence of an `error` key is how a caller
        // reads a shape it has not met yet as a success.
        assert_eq!(
            parse(&location("where", Path::new("/home")))["ok"],
            json!(true)
        );
        assert_eq!(
            parse(&failure("read", &FileError::Absent(PathBuf::from("/x"))))["ok"],
            json!(false)
        );
    }

    // ───────────────────────────────────────────── what the machine face refuses to do

    #[test]
    fn the_machine_face_hides_nothing_a_person_would_not_be_shown() {
        let listing_of = crate::Listing {
            entries: vec![
                Entry {
                    name: OsString::from(".bashrc"),
                    kind: Kind::File { bytes: 12 },
                },
                Entry {
                    name: OsString::from("notas.txt"),
                    kind: Kind::File { bytes: 3 },
                },
            ],
            unreadable: Vec::new(),
        };

        let answer = shown("/home", listing_of);
        let names: Vec<&str> = answer["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap())
            .collect();

        // The tie-break rule of the decree, in one assertion. A person does not
        // see this name by default and a program always does: hiding it from
        // something that asked is taking capability away.
        assert_eq!(names, vec![".bashrc", "notas.txt"]);
    }

    #[test]
    fn sizes_come_through_exact_and_never_in_the_rounded_form() {
        let listing_of = crate::Listing {
            entries: vec![Entry {
                name: OsString::from("big"),
                kind: Kind::File { bytes: 1536 },
            }],
            unreadable: Vec::new(),
        };

        let answer = shown("/home", listing_of);
        // A person is shown `1.5 kB`. Two programs comparing two rounded numbers
        // compare two lies.
        assert_eq!(answer["entries"][0]["bytes"], json!(1536));
        assert!(!answer.to_string().contains("kB"));
    }

    #[test]
    fn a_broken_link_arrives_as_broken_and_not_as_missing() {
        let listing_of = crate::Listing {
            entries: vec![Entry {
                name: OsString::from("dangling"),
                kind: Kind::Link {
                    to: PathBuf::from("/gone"),
                    broken: true,
                },
            }],
            unreadable: Vec::new(),
        };

        let answer = shown("/home", listing_of);
        assert_eq!(answer["entries"][0]["kind"], json!("link"));
        assert_eq!(answer["entries"][0]["to"], json!("/gone"));
        assert_eq!(answer["entries"][0]["broken"], json!(true));
    }

    #[test]
    fn a_name_that_could_not_be_read_is_not_dropped_from_the_listing() {
        let listing_of = crate::Listing {
            entries: Vec::new(),
            unreadable: vec![(OsString::from("locked"), "Permission denied".to_string())],
        };

        let answer = shown("/home", listing_of);
        // Rule 10, on the wire. A listing that dropped it would report a smaller
        // directory than the one on the disk.
        assert_eq!(answer["unreadable"][0]["name"], json!("locked"));
        assert_eq!(answer["unreadable"][0]["why"], json!("Permission denied"));
    }

    #[test]
    fn the_unreadable_list_is_present_even_when_it_is_empty() {
        let answer = shown(
            "/home",
            crate::Listing {
                entries: Vec::new(),
                unreadable: Vec::new(),
            },
        );
        // Present-when-empty is what makes a caller write the branch. A key that
        // appears only on the bad day is a key nobody handles on the bad day.
        assert_eq!(answer["unreadable"], json!([]));
    }

    // ──────────────────────────────────────────────────── names that are not text

    #[test]
    fn a_name_that_is_not_text_is_flagged_rather_than_quietly_mangled() {
        use std::os::unix::ffi::OsStringExt;
        let listing_of = crate::Listing {
            entries: vec![Entry {
                // Invalid UTF-8, which a Linux filename is allowed to be.
                name: OsString::from_vec(vec![b'a', 0xff, b'b']),
                kind: Kind::File { bytes: 1 },
            }],
            unreadable: Vec::new(),
        };

        let answer = shown("/home", listing_of);
        // The failure this prevents: an agent takes the name out of a listing,
        // passes it back to `rm`, and removes a different file or none — with
        // nothing anywhere saying the name it was handed was not the name.
        assert_eq!(answer["exact"], json!(false));
    }

    #[test]
    fn ordinary_names_do_not_carry_the_flag_at_all() {
        let answer = parse(&location("where", Path::new("/home/cesar")));
        // Absent rather than `true`, so the common case costs nothing and the
        // flag is only ever there when it means something.
        assert!(answer.get("exact").is_none(), "got {answer}");
    }

    // ─────────────────────────────────────────────────────────── facts and failures

    #[test]
    fn what_was_asked_and_what_happened_are_two_fields() {
        let moved = Done {
            what: Did::Moved,
            path: PathBuf::from("/home/a"),
            to: Some(PathBuf::from("/opt/b")),
            bytes: 42,
        };
        let answer = parse(&batch("move", vec![fact(&moved)]));

        assert_eq!(answer["op"], json!("move"));
        let only = &answer["results"][0];
        // `moved` survives even when the disk work was a copy and a delete,
        // because that is what is true of the disk. The two fields answer two
        // questions and collapsing them would lose one.
        assert_eq!(only["did"], json!("moved"));
        assert_eq!(only["path"], json!("/home/a"));
        assert_eq!(only["to"], json!("/opt/b"));
        assert_eq!(only["bytes"], json!(42));
    }

    #[test]
    fn an_operation_that_moves_nothing_carries_no_destination() {
        let made = Done {
            what: Did::MadeFile,
            path: PathBuf::from("/home/a"),
            to: None,
            bytes: 0,
        };
        let answer = parse(&batch("make_file", vec![fact(&made)]));
        assert!(answer["results"][0].get("to").is_none(), "got {answer}");
    }

    #[test]
    fn a_failure_carries_a_word_to_match_on_and_not_only_a_sentence() {
        let answer = parse(&failure(
            "copy",
            &FileError::Exists(PathBuf::from("/home/there")),
        ));
        // The word is the contract; the sentence is English that will be
        // reworded, and a caller matching on it breaks the first time it is.
        assert_eq!(answer["error"], json!("exists"));
        assert_eq!(answer["path"], json!("/home/there"));
        assert!(answer["message"].as_str().unwrap().contains("/home/there"));
    }

    // ────────────────────────────────── one typed line answered by exactly one object

    #[test]
    fn several_things_touched_by_one_line_come_back_as_one_answer() {
        let results = vec![
            fact(&Done {
                what: Did::Removed,
                path: PathBuf::from("/home/a.log"),
                to: None,
                bytes: 10,
            }),
            fact(&Done {
                what: Did::Removed,
                path: PathBuf::from("/home/b.log"),
                to: None,
                bytes: 20,
            }),
        ];
        let line = batch("remove", results);

        // The failure this prevents: `rm *.log` catches three files, prints
        // three objects, and a caller with no way to know it should read three
        // takes the second answer as the reply to its next command.
        assert!(!line.contains('\n'), "{line:?} spans lines");
        let answer = parse(&line);
        assert_eq!(answer["count"], json!(2));
        assert_eq!(answer["results"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn one_thing_touched_is_still_shaped_like_a_batch() {
        let answer = parse(&batch(
            "remove",
            vec![fact(&Done {
                what: Did::Removed,
                path: PathBuf::from("/home/only"),
                to: None,
                bytes: 1,
            })],
        ));
        // Uniform on purpose. A caller that reads `results` sometimes and a bare
        // fact other times has two code paths, and the rarer one is the one
        // nobody exercises.
        assert_eq!(answer["count"], json!(1));
        assert!(answer["results"].is_array());
    }

    #[test]
    fn one_thing_failing_out_of_several_does_not_report_the_line_as_ok() {
        let answer = parse(&batch(
            "remove",
            vec![
                fact(&Done {
                    what: Did::Removed,
                    path: PathBuf::from("/home/went"),
                    to: None,
                    bytes: 1,
                }),
                problem(&FileError::Absent(PathBuf::from("/home/never-was"))),
            ],
        ));

        // A partial success reported as a success is how a caller moves on
        // believing a loop finished.
        assert_eq!(answer["ok"], json!(false));
        assert_eq!(answer["results"][0]["ok"], json!(true));
        assert_eq!(answer["results"][1]["ok"], json!(false));
        assert_eq!(answer["results"][1]["error"], json!("absent"));
    }

    #[test]
    fn a_refused_move_says_where_the_session_still_is() {
        let answer = parse(&stayed(
            "go",
            &FileError::Absent(PathBuf::from("/home/nowhere")),
            Path::new("/home/cesar"),
        ));

        // The human face prints "You are still in …" for a reason, and a program
        // is worse off than a person here: it has no prompt to read the answer
        // off. Without this it would aim its next command at a place it is not.
        assert_eq!(answer["ok"], json!(false));
        assert_eq!(answer["still_at"], json!("/home/cesar"));
        assert_eq!(answer["path"], json!("/home/nowhere"));
    }

    #[test]
    fn every_file_error_has_its_own_word() {
        let words = [
            FileError::Absent(PathBuf::new()).word(),
            FileError::IsDirectory(PathBuf::new()).word(),
            FileError::NotText {
                path: PathBuf::new(),
                why: "nul bytes",
            }
            .word(),
            FileError::Unreadable {
                path: PathBuf::new(),
                detail: String::new(),
            }
            .word(),
            FileError::Exists(PathBuf::new()).word(),
        ];
        let unique: std::collections::BTreeSet<&str> = words.iter().copied().collect();
        // Two errors sharing a word would arrive as the same thing, which is
        // the whole failure this module exists to stop somewhere else.
        assert_eq!(unique.len(), words.len(), "got {words:?}");
    }

    #[test]
    fn text_with_quotes_and_newlines_survives_the_trip() {
        let answer = parse(&excerpt(
            Path::new("/home/notas.txt"),
            &Excerpt {
                text: "he said \"hola\"\nand left\t— ñ".to_string(),
                of_bytes: 30,
                truncated: false,
                digest: String::new(),
            },
        ));
        // Escaping is the reason this is JSON and not a line of fields: a file
        // holding a quote would otherwise end the value early and every field
        // after it would be read as part of the text.
        assert_eq!(answer["text"], json!("he said \"hola\"\nand left\t— ñ"));
    }

    #[test]
    fn a_cut_excerpt_reports_the_size_of_the_file_and_not_of_the_piece() {
        let answer = parse(&excerpt(
            Path::new("/home/big"),
            &Excerpt {
                text: "abc".to_string(),
                of_bytes: 900_000,
                truncated: true,
                digest: String::new(),
            },
        ));
        // Reporting 3 here would leave a caller believing it has the file.
        assert_eq!(answer["bytes"], json!(900_000));
        assert_eq!(answer["truncated"], json!(true));
    }

    #[test]
    fn a_refusal_that_never_reached_the_disk_still_answers() {
        let answer = parse(&refusal("remove", "which one"));
        assert_eq!(answer["ok"], json!(false));
        assert_eq!(answer["error"], json!("incomplete"));
        // The failure this prevents: a program types `rm` with no argument, the
        // human face prints a hint, and the parser waits forever for an object.
        assert_eq!(answer["message"], json!("which one"));
    }
}
