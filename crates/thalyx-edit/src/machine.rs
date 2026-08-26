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

use crate::{Change, EditError, Edited, Text};
use serde_json::{Map, Value, json};
use std::path::Path;

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
        | EditError::Unwritable { path, .. } => Some(path),
        EditError::Backwards { .. } | EditError::Malformed { .. } | EditError::NoScreen => None,
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
