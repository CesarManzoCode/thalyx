//! The face a program reads, built from the same facts the person is shown.
//!
//! `thalyx-files/src/machine.rs` set the shape and this follows it exactly:
//! nothing is derived, every word is a decision written down, and the framing
//! contract is **one typed line, exactly one object**.
//!
//! ## What is different here, and why
//!
//! Every other verb answers about something that already exists. An edit
//! *changes* a file, and the caller's next decision depends on what the file now
//! says — so every answer carries the line count before and after and the exact
//! byte size, which is what saves a caller from re-reading the file to find out
//! whether its own edit did what it asked.
//!
//! And every answer carries `undo`. `Superficie-para-el-LLM.md` calls the cost
//! of being wrong the one that changes an agent's *behaviour* rather than its
//! efficiency: in a system where everything is irreversible a rational agent
//! turns timid, and timid does not read as prudence, it reads as incapacity. The
//! honest value here is `attempt` and not something cheaper — once a file is
//! saved the previous text is gone, and only a snapshot has it.

use crate::{Change, EditError, Edited, Substituted, Text};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// The one shape every answer from this module has.
///
/// Copied in structure from `thalyx-files`, deliberately, rather than shared
/// through a common crate: the two are the same *today*, and a caller reading
/// both has to be able to trust that `ok` and `op` mean the same thing in each.
/// If they ever diverge, they diverge visibly here instead of by changing a
/// shared function and moving every verb at once.
fn object(op: &str, ok: bool, fields: Map<String, Value>) -> String {
    let mut out = Map::new();
    out.insert("op".into(), json!(op));
    out.insert("ok".into(), json!(ok));
    for (key, value) in fields {
        out.insert(key, value);
    }
    Value::Object(out).to_string()
}

fn fields(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

/// What a file looks like right now, line by line, with the count that makes
/// every later address answerable.
///
/// This is what a caller runs *first*, and the reason the address language is as
/// small as it is: a caller that has been told there are 214 lines never needs
/// `$` to mean the last one.
pub fn shown(text: &Text, from: usize, rows: &[(usize, &str)], more: bool) -> String {
    let listed: Vec<Value> = rows
        .iter()
        .map(|(number, body)| json!({ "line": number, "text": body }))
        .collect();

    let mut carried = fields([
        ("path", json!(text.path().to_string_lossy())),
        ("lines", json!(text.count())),
        ("bytes", json!(text.weight())),
        ("from", json!(from)),
        ("shown", json!(rows.len())),
        // Said on every answer, not only when it is true. A caller that has to
        // infer "there is more" from a count it has to do itself is a caller
        // that will get it wrong once.
        ("more", json!(more)),
        ("ending", json!(text.ending().word())),
        ("rows", Value::Array(listed)),
    ]);
    if text.through_link() {
        carried.insert("writes_to".into(), json!(text.target().to_string_lossy()));
    }
    object("edit_show", true, carried)
}

/// What an edit did.
pub fn did(edited: &Edited, text: &Text) -> String {
    let mut carried = fields([
        ("path", json!(edited.path.to_string_lossy())),
        ("did", json!(edited.what.word())),
        ("lines_before", json!(edited.lines_before)),
        ("lines_after", json!(edited.lines_after)),
        // Exact, always. The rounded form is for a human eye, and two programs
        // comparing two rounded numbers compare two lies.
        ("bytes", json!(edited.bytes)),
        (
            "undo",
            match edited.what {
                // Nothing changed, so there is nothing to take back — and
                // `none` is a different answer from "use an attempt", which
                // would send a caller to take a snapshot it does not need.
                Change::Unchanged => json!("none"),
                _ => json!("attempt"),
            },
        ),
    ]);
    if let Some(span) = edited.span {
        carried.insert("at".into(), json!(span.to_string()));
        carried.insert("first_line".into(), json!(span.from));
        carried.insert("last_line".into(), json!(span.to));
    }
    if text.through_link() {
        carried.insert("writes_to".into(), json!(text.target().to_string_lossy()));
    }
    object("edit", true, carried)
}

/// One row per file of what a substitution did, which is the whole of the
/// evidence a caller needs and nothing more.
///
/// **This shape is the second half of the change it belongs to.** The thing a
/// line-by-line rename cost was not only round trips: every one of those calls
/// answered with the file's whole new state, and the caller had to send the
/// whole new text of a line to ask for it. Answering here with the changed lines
/// would put all of that back — the point is that a caller learns *what* changed
/// and *where to look*, and looks only if it wants to.
///
/// So: how many places, on how many lines, starting at which one, and how big
/// the file is now. A caller that wants to see the result asks `ver` once,
/// knowing exactly where.
fn rows(done: &[Substituted]) -> Value {
    Value::Array(
        done.iter()
            .map(|one| {
                json!({
                    "path": one.path.to_string_lossy(),
                    "replacements": one.replacements,
                    "lines": one.lines,
                    "first_line": one.first_line,
                    "bytes": one.bytes,
                })
            })
            .collect(),
    )
}

fn totals(old: &str, new: &str, done: &[Substituted]) -> Map<String, Value> {
    // Said on every answer and not only when it is true, which is the rule this
    // file already follows for `more`: a caller that has to infer a fact from a
    // missing key is a caller that will get it wrong once, and the fact here is
    // whether its own undo is exact.
    let overlapping: Vec<Value> = done
        .iter()
        .filter(|one| one.new_already > 0)
        .map(|one| json!(one.path.to_string_lossy()))
        .collect();
    let mut carried = fields([
        ("old", json!(old)),
        ("new", json!(new)),
        ("files", json!(done.len())),
        (
            "replacements",
            json!(done.iter().map(|one| one.replacements).sum::<usize>()),
        ),
        ("new_was_present", json!(!overlapping.is_empty())),
    ]);
    if !overlapping.is_empty() {
        // Named, because "somewhere" is not something a caller can act on. It
        // now knows that substituting back is not the inverse of this call, and
        // in which files, so it can reach for the attempt instead.
        carried.insert("new_was_present_in".into(), Value::Array(overlapping));
    }
    carried
}

/// What a substitution did, across every file it was given.
///
/// The `op` is `edit`, not a word of its own, because `describe` says this verb
/// answers with `edit` and a verb that answered with two ops would make that
/// catalogue entry a lie. What tells the two apart is `did`, which every answer
/// from this verb already carries.
pub fn substituted(old: &str, new: &str, done: &[Substituted]) -> String {
    let mut carried = totals(old, new, done);
    carried.insert("did".into(), json!(Change::Substituted.word()));
    carried.insert("changed".into(), rows(done));
    carried.insert("undo".into(), json!("attempt"));
    object("edit", true, carried)
}

/// The same, with nothing written.
pub fn would_substitute(old: &str, new: &str, done: &[Substituted]) -> String {
    let mut carried = fields([("verb", json!("edit"))]);
    carried.extend(totals(old, new, done));
    carried.insert("would".into(), json!(Change::Substituted.word()));
    carried.insert("changed".into(), rows(done));
    carried.insert("wrote".into(), json!(false));
    object("rehearse", true, carried)
}

/// One batch's worth of substitutions, told twice: by pattern and by file.
///
/// **Twice, and neither half is redundant.** A caller that asked for five
/// patterns wants to know that each of the five found what it was looking for —
/// that is `operations`, and a pattern that matched three places instead of the
/// thirty it expected is the answer that saves it a wrong diff. A caller that
/// wants to check the work opens *files*, and a file touched by four of the five
/// patterns appears in four places under `operations` and once under `changed`,
/// with the size it actually has now.
///
/// What is deliberately **not** here is any of the text. The whole reason a
/// line-by-line rename cost what it did is that every one of its sixteen calls
/// answered with a file's new state and was sent a file's new line; answering
/// here with the changed lines would put all of that back one level up. A
/// caller that wants to look knows exactly where: `first_line`, per pattern,
/// per file.
///
/// `did`, `undo` and `changed` mean the same thing they mean in [`substituted`],
/// on purpose. A caller that already reads the single-substitution answer reads
/// this one without learning a second shape; `operations` is the only key it has
/// to know is new.
pub struct Batched<'a> {
    pub old: &'a str,
    pub new: &'a str,
    pub done: &'a [Substituted],
}

/// What every file in a batch looks like now, once, with its final size.
///
/// Built from the per-operation rows rather than measured again: a file the
/// fourth operation touched carries the size the fourth operation left, and
/// summing the replacements across the operations that named it is what "this
/// file changed in seven places" means when seven places came from five
/// patterns.
fn changed_across(batch: &[Batched<'_>]) -> Value {
    let mut order: Vec<PathBuf> = Vec::new();
    let mut replacements: std::collections::BTreeMap<PathBuf, usize> = Default::default();
    let mut bytes: std::collections::BTreeMap<PathBuf, u64> = Default::default();
    for operation in batch {
        for one in operation.done {
            if !replacements.contains_key(&one.path) {
                order.push(one.path.clone());
            }
            *replacements.entry(one.path.clone()).or_default() += one.replacements;
            // The last operation to touch it is the one whose size is current.
            bytes.insert(one.path.clone(), one.bytes);
        }
    }
    Value::Array(
        order
            .into_iter()
            .map(|path| {
                json!({
                    "path": path.to_string_lossy(),
                    "replacements": replacements.get(&path).copied().unwrap_or(0),
                    "bytes": bytes.get(&path).copied().unwrap_or(0),
                })
            })
            .collect(),
    )
}

fn operations_of(batch: &[Batched<'_>]) -> Value {
    Value::Array(
        batch
            .iter()
            .map(|operation| {
                let overlapping: Vec<Value> = operation
                    .done
                    .iter()
                    .filter(|one| one.new_already > 0)
                    .map(|one| json!(one.path.to_string_lossy()))
                    .collect();
                let mut carried = serde_json::Map::new();
                carried.insert("old".into(), json!(operation.old));
                carried.insert("new".into(), json!(operation.new));
                carried.insert("files".into(), json!(operation.done.len()));
                carried.insert(
                    "replacements".into(),
                    json!(
                        operation
                            .done
                            .iter()
                            .map(|one| one.replacements)
                            .sum::<usize>()
                    ),
                );
                // Per operation and not only for the batch: substituting back is
                // the inverse of *this pattern* or it is not, and a caller told
                // only that some pattern somewhere overlapped cannot act on it.
                carried.insert("new_was_present".into(), json!(!overlapping.is_empty()));
                if !overlapping.is_empty() {
                    carried.insert("new_was_present_in".into(), Value::Array(overlapping));
                }
                carried.insert(
                    "in".into(),
                    Value::Array(
                        operation
                            .done
                            .iter()
                            .map(|one| {
                                json!({
                                    "path": one.path.to_string_lossy(),
                                    "replacements": one.replacements,
                                    "lines": one.lines,
                                    "first_line": one.first_line,
                                })
                            })
                            .collect(),
                    ),
                );
                Value::Object(carried)
            })
            .collect(),
    )
}

fn batch_totals(batch: &[Batched<'_>]) -> Map<String, Value> {
    let changed = changed_across(batch);
    let files = changed.as_array().map(Vec::len).unwrap_or(0);
    let mut carried = fields([
        ("operations", operations_of(batch)),
        ("changed", changed),
        // The two totals a caller checks against what it expected, and they are
        // about the batch: files is **distinct** files, not the sum of each
        // operation's, which would count a file five patterns touched five
        // times.
        ("files", json!(files)),
    ]);
    carried.insert(
        "replacements".into(),
        json!(
            batch
                .iter()
                .flat_map(|operation| operation.done.iter())
                .map(|one| one.replacements)
                .sum::<usize>()
        ),
    );
    carried
}

/// What a batch of substitutions did.
pub fn substituted_batch(batch: &[Batched<'_>]) -> String {
    let mut carried = batch_totals(batch);
    carried.insert("did".into(), json!(Change::Substituted.word()));
    carried.insert("undo".into(), json!("attempt"));
    object("edit", true, carried)
}

/// The same, with nothing written.
pub fn would_substitute_batch(batch: &[Batched<'_>]) -> String {
    let mut carried = fields([("verb", json!("edit"))]);
    carried.extend(batch_totals(batch));
    carried.insert("would".into(), json!(Change::Substituted.word()));
    carried.insert("wrote".into(), json!(false));
    object("rehearse", true, carried)
}

/// A batch that passed its preflight and then could not finish writing.
///
/// The batch's whole point is that its preflight is complete: every file is
/// opened, every operation applied **in memory**, and only then is anything
/// saved. So reaching here means a save failed — a full disk, a file that
/// became read-only — with earlier files already replaced, and the same thing
/// [`half_substituted`] says has to be said: exactly which files carry the new
/// text and which do not, and that only an attempt puts all of them back.
pub fn half_substituted_batch(
    batch: &[Batched<'_>],
    written: &[PathBuf],
    left: &[PathBuf],
    error: &EditError,
) -> String {
    let mut carried = batch_totals(batch);
    carried.insert("error".into(), json!(error.word()));
    carried.insert("remedy".into(), json!(error.remedy()));
    carried.insert("message".into(), json!(error.to_string()));
    carried.insert("did".into(), json!(Change::Substituted.word()));
    carried.insert("wrote".into(), json!(true));
    carried.insert(
        "written".into(),
        Value::Array(
            written
                .iter()
                .map(|path| json!(path.to_string_lossy()))
                .collect(),
        ),
    );
    carried.insert(
        "not_written".into(),
        Value::Array(
            left.iter()
                .map(|path| json!(path.to_string_lossy()))
                .collect(),
        ),
    );
    carried.insert("undo".into(), json!("attempt"));
    object("edit", false, carried)
}

/// Why a substitution did not happen, and the one fact that makes it recoverable.
///
/// `wrote: false` is the field, and it is said out loud rather than inferred
/// from `ok`. A caller that has just asked to change six files and got a
/// refusal has to know whether it is looking at a workspace nobody touched or
/// at one that is halfway through a rename, and "ok is false" does not answer
/// that: [`half_substituted`] is also not ok, and it means the opposite.
pub fn not_substituted(op: &str, error: &EditError) -> String {
    let refused = problem(op, error);
    // Built by adding to the object every other refusal already produces,
    // rather than by writing a second one: a refusal from this verb that
    // disagreed with the others about what `error` or `remedy` mean would be
    // the two-faces problem inside one face.
    let Ok(Value::Object(mut carried)) = serde_json::from_str::<Value>(&refused) else {
        return refused;
    };
    carried.insert("wrote".into(), json!(false));
    Value::Object(carried).to_string()
}

/// A substitution that passed its preflight and then could not finish writing.
///
/// **The one answer in this crate that reports a workspace in a state nobody
/// asked for**, and it exists because pretending otherwise is worse. Every
/// precondition is checked before a byte is written, so reaching here means a
/// save failed — a full disk, a file that became read-only — after earlier files
/// were already replaced. Each individual save is still atomic, so no file is
/// half-written; what is split is the set.
///
/// So it says exactly which files carry the new text and which do not, and it
/// says `undo: attempt`, which is the one thing that puts all of them back.
pub fn half_substituted(
    old: &str,
    new: &str,
    done: &[Substituted],
    left: &[PathBuf],
    error: &EditError,
) -> String {
    let mut carried = totals(old, new, done);
    carried.insert("error".into(), json!(error.word()));
    carried.insert("remedy".into(), json!(error.remedy()));
    carried.insert("message".into(), json!(error.to_string()));
    carried.insert("did".into(), json!(Change::Substituted.word()));
    carried.insert("wrote".into(), json!(true));
    carried.insert("changed".into(), rows(done));
    carried.insert(
        "not_written".into(),
        Value::Array(
            left.iter()
                .map(|path| json!(path.to_string_lossy()))
                .collect(),
        ),
    );
    carried.insert("undo".into(), json!("attempt"));
    object("edit", false, carried)
}

/// What an edit **would** do, with nothing written.
///
/// The same fields as [`did`], because a rehearsal that answered a different
/// shape from the verb would have to be read by a caller twice. The two that
/// change are the ones that would be lies: the `op` is `rehearse`, so nothing
/// reads it as an edit that happened, and `wrote` says plainly that the file on
/// disk is still the one that was there.
pub fn would(edited: &Edited, text: &Text) -> String {
    let mut carried = fields([
        ("verb", json!("edit")),
        ("path", json!(edited.path.to_string_lossy())),
        ("would", json!(edited.what.word())),
        ("lines_before", json!(edited.lines_before)),
        ("lines_after", json!(edited.lines_after)),
        ("bytes", json!(edited.bytes)),
        ("wrote", json!(false)),
    ]);
    if let Some(span) = edited.span {
        carried.insert("at".into(), json!(span.to_string()));
        carried.insert("first_line".into(), json!(span.from));
        carried.insert("last_line".into(), json!(span.to));
    }
    if text.through_link() {
        // The one fact a rehearsal of this verb exists for. Somebody editing a
        // link is about to change a file they did not name, and the moment to
        // find that out is before it happens.
        carried.insert("writes_to".into(), json!(text.target().to_string_lossy()));
    }
    object("rehearse", true, carried)
}

/// Why an edit did not happen.
pub fn problem(op: &str, error: &EditError) -> String {
    let mut carried = fields([
        ("error", json!(error.word())),
        ("remedy", json!(error.remedy())),
        // The English sentence, for a person reading a log. Nothing should match
        // on it — that is what `error` is for — and it is carried anyway because
        // a word with no sentence beside it is a support question.
        ("message", json!(error.to_string())),
    ]);
    // The two facts a caller most often needs to recover from a bad address, put
    // where it does not have to parse the sentence to get them.
    if let EditError::NoSuchLine { asked, has, .. } = error {
        carried.insert("asked".into(), json!(asked));
        carried.insert("lines".into(), json!(has));
    }
    // The same idea for the ceilings: a caller told only that it asked for too
    // much has to guess how much less is enough, and guessing costs a call.
    if let EditError::TooMuch { given, most, .. } = error {
        carried.insert("asked".into(), json!(given));
        carried.insert("most".into(), json!(most));
    }
    if let Some(path) = path_in(error) {
        carried.insert("path".into(), json!(path.to_string_lossy()));
    }
    object(op, false, carried)
}

fn path_in(error: &EditError) -> Option<&Path> {
    match error {
        EditError::Absent(path)
        | EditError::IsDirectory(path)
        | EditError::NotText { path, .. }
        | EditError::TooLarge { path, .. }
        | EditError::NoSuchLine { path, .. }
        | EditError::Unreadable { path, .. }
        | EditError::Unwritable { path, .. }
        | EditError::NoOccurrences { path, .. }
        | EditError::RepeatedPath { path } => Some(path),
        EditError::Backwards { .. }
        | EditError::Malformed { .. }
        | EditError::BadText { .. }
        | EditError::SameText { .. }
        | EditError::TooMuch { .. }
        | EditError::Incomplete { .. }
        | EditError::BadBatch { .. }
        | EditError::Chained { .. }
        | EditError::NoScreen => None,
    }
}

/// The answer to `editar` with something after it that is not a subverb.
///
/// Its own function because "I do not know that word" is a different fact from
/// "that file is not there", and a caller that gets `absent` for a misspelt
/// subverb goes looking for the wrong problem.
pub fn unknown(word: &str, known: &[&str]) -> String {
    object(
        "edit",
        false,
        fields([
            ("error", json!("unknown_action")),
            ("remedy", json!("describe")),
            ("asked", json!(word)),
            ("known", json!(known)),
            (
                "message",
                json!(format!("`{word}` is not one of {}", known.join(", "))),
            ),
        ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Span;
    use serde_json::Value;
    use std::path::Path;

    fn parse(line: &str) -> Value {
        // Every answer is parsed in its test rather than compared as a string. A
        // string comparison passes on a document that is not valid JSON, which
        // is the only property a caller actually needs.
        serde_json::from_str(line).expect("one object per line")
    }

    fn text(body: &str) -> Text {
        Text::from_str(Path::new("/tmp/notes.txt"), body, None)
    }

    #[test]
    fn an_edit_says_what_the_file_looked_like_before_and_after_without_a_second_call() {
        let mut file = text("uno\ndos\n");
        let done = file.insert(3, "tres").unwrap();
        let answer = parse(&did(&done, &file));

        assert_eq!(answer["ok"], true);
        assert_eq!(answer["did"], "inserted");
        assert_eq!(answer["lines_before"], 2);
        assert_eq!(answer["lines_after"], 3);
        assert_eq!(answer["at"], "3");
        assert_eq!(answer["undo"], "attempt");
    }

    #[test]
    fn a_refused_address_carries_the_count_that_makes_the_next_attempt_right() {
        let mut file = text("uno\ndos\n");
        let error = file.replace(Span::one(9), "x").unwrap_err();
        let answer = parse(&problem("edit", &error));

        assert_eq!(answer["ok"], false);
        assert_eq!(answer["error"], "no_such_line");
        assert_eq!(answer["asked"], 9);
        // The whole point of A2: the caller can fix its own address from this
        // object without reading the file again.
        assert_eq!(answer["lines"], 2);
        assert_eq!(answer["remedy"], "read_the_count");
    }

    #[test]
    fn a_listing_says_how_many_lines_there_are_and_whether_it_sent_them_all() {
        let file = text("uno\ndos\ntres\n");
        let answer = parse(&shown(&file, 1, &[(1, "uno"), (2, "dos")], true));

        assert_eq!(answer["lines"], 3);
        assert_eq!(answer["shown"], 2);
        assert_eq!(answer["more"], true);
        assert_eq!(answer["rows"][1]["line"], 2);
        assert_eq!(answer["rows"][1]["text"], "dos");
    }

    #[test]
    fn a_misspelt_action_is_not_reported_as_a_missing_file() {
        let answer = parse(&unknown("insertaar", &["ver", "poner"]));
        assert_eq!(answer["error"], "unknown_action");
        assert_eq!(answer["asked"], "insertaar");
    }

    #[test]
    fn a_substitution_answers_with_counts_rather_than_with_the_lines_it_changed() {
        // The reason this shape exists. An answer that echoed the new text of
        // every line would put back exactly the cost the operation was built to
        // remove — so what comes back is where to look and how much moved.
        let mut one = text("Uid\nno\nUid Uid\n");
        let done = one.substitute("Uid", "Renamed").unwrap();
        let answer = parse(&substituted("Uid", "Renamed", std::slice::from_ref(&done)));

        assert_eq!(answer["ok"], true);
        assert_eq!(answer["did"], "substituted");
        assert_eq!(answer["files"], 1);
        assert_eq!(answer["replacements"], 3);
        assert_eq!(answer["changed"][0]["lines"], 2);
        assert_eq!(answer["changed"][0]["first_line"], 1);
        assert_eq!(answer["undo"], "attempt");
        assert!(
            !answer.to_string().contains("Renamed\\nno"),
            "the answer is carrying the file's text back to the caller"
        );
    }

    #[test]
    fn a_substitution_says_whether_substituting_back_would_be_its_inverse() {
        // The trap this closes: renaming `Slot` to `Table` in a file that
        // already said `Table` cannot be undone by renaming `Table` to `Slot` —
        // that second call would also move the `Table`s that were always there,
        // and afterwards nothing could tell them apart. Reported and not
        // refused, because normalising two spellings into one is exactly this
        // and is a legitimate thing to ask for.
        let mut one = text("Slot and Table\n");
        let done = one.substitute("Slot", "Table").unwrap();
        assert_eq!(done.new_already, 1);
        let answer = parse(&substituted("Slot", "Table", std::slice::from_ref(&done)));
        assert_eq!(answer["new_was_present"], true);
        assert_eq!(answer["new_was_present_in"][0], "/tmp/notes.txt");

        // And the ordinary case says so too, rather than leaving the caller to
        // infer safety from a key that is not there.
        let mut two = text("Slot only\n");
        let clean = two.substitute("Slot", "Table").unwrap();
        assert_eq!(clean.new_already, 0);
        let answer = parse(&substituted("Slot", "Table", std::slice::from_ref(&clean)));
        assert_eq!(answer["new_was_present"], false);
        assert!(answer.get("new_was_present_in").is_none());
    }

    #[test]
    fn a_refused_substitution_says_out_loud_that_nothing_was_written() {
        // `ok: false` does not answer the question a caller actually has, which
        // is whether it is looking at an untouched workspace or a half-renamed
        // one. `half_substituted` is also `ok: false` and means the opposite.
        let error = EditError::NoOccurrences {
            path: Path::new("/w/src/other.rs").to_path_buf(),
            old: "Uid".to_string(),
        };
        let answer = parse(&not_substituted("edit", &error));
        assert_eq!(answer["ok"], false);
        assert_eq!(answer["wrote"], false);
        assert_eq!(answer["error"], "no_occurrences");
        assert_eq!(answer["remedy"], "drop_that_file");
        // Which file, so the next call is a correction and not a search.
        assert_eq!(answer["path"], "/w/src/other.rs");
    }

    #[test]
    fn a_substitution_that_stopped_halfway_names_both_halves() {
        let mut one = text("Uid\n");
        let done = one.substitute("Uid", "Renamed").unwrap();
        let error = EditError::Unwritable {
            path: Path::new("/w/src/b.rs").to_path_buf(),
            detail: "No space left on device".to_string(),
        };
        let answer = parse(&half_substituted(
            "Uid",
            "Renamed",
            std::slice::from_ref(&done),
            &[Path::new("/w/src/b.rs").to_path_buf()],
            &error,
        ));

        assert_eq!(answer["ok"], false);
        // The field that separates this from every other refusal.
        assert_eq!(answer["wrote"], true);
        assert_eq!(answer["changed"][0]["replacements"], 1);
        assert_eq!(answer["not_written"][0], "/w/src/b.rs");
        assert_eq!(answer["undo"], "attempt");
    }

    #[test]
    fn a_ceiling_refusal_says_how_much_was_asked_for_and_how_much_is_allowed() {
        let error = EditError::TooMuch {
            what: "files named in one substitution",
            given: 90,
            most: crate::MOST_FILES,
        };
        let answer = parse(&not_substituted("edit", &error));
        assert_eq!(answer["asked"], 90);
        assert_eq!(answer["most"], crate::MOST_FILES);
        assert_eq!(answer["remedy"], "send_less");
    }

    #[test]
    fn editing_through_a_symlink_says_which_file_actually_gets_written() {
        let mut file = text("uno\n");
        // The engine sets this from the filesystem; here it is set directly
        // because the fact under test is that the answer carries it.
        let done = file.replace(Span::one(1), "UNO").unwrap();
        let answer = parse(&did(&done, &file));
        assert!(
            answer.get("writes_to").is_none(),
            "a file that is not a link must not carry the field at all"
        );
    }
}
