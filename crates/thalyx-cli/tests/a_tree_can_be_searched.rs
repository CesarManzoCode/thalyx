//! A tree can be searched by name and by content, at the real prompt.
//!
//! **Rule 1**: every real defect in this project came from running the system,
//! not from reading it. `thalyx-files::search` has its own tests and they run
//! the engine; these run the *session* — the line parsing, the two faces, the
//! paging and the refusals — because a verb that is perfect and unreachable is
//! how installed modules stayed unexecutable for weeks while every test passed.
//!
//! The tree searched here is written by this file, so what should be found is
//! known before the search runs rather than read out of its answer.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

fn typed(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session");

    let mut script = String::new();
    for line in lines {
        script.push_str(line);
        script.push('\n');
    }
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("feeding the session");
    child.wait_with_output().expect("waiting for the session")
}

fn objects(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(|line| serde_json::from_str(line).expect("one object per line"))
        .collect()
}

fn answer_to(output: &Output, op: &str) -> serde_json::Value {
    objects(output)
        .into_iter()
        .find(|value| value["op"] == op)
        .unwrap_or_else(|| {
            panic!(
                "nothing answered with op `{op}`; the session said:\n{}",
                String::from_utf8_lossy(&output.stdout)
            )
        })
}

/// A tree with something to find in it and something that must not be found.
fn a_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("src/auth.rs", "pub fn login() {}\n"),
        ("src/main.rs", "fn main() { login(); }\n"),
        ("src/deep/util.rs", "// login is called elsewhere\n"),
        ("notes.txt", "remember the login page\n"),
        (".git/config", "login = nobody\n"),
    ] {
        let full = dir.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    dir
}

#[test]
fn a_program_asks_for_files_by_name_and_gets_them_relative_to_what_it_named() {
    let dir = a_project();
    let output = typed(
        dir.path(),
        &[
            "structured on",
            &format!("encontrar en={} *.rs", dir.path().display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "find");
    assert_eq!(said["ok"], true);
    let found: Vec<&str> = said["matches"]
        .as_array()
        .expect("matches is an array")
        .iter()
        .map(|row| row["path"].as_str().expect("a path"))
        .collect();
    assert_eq!(
        found,
        vec!["src/auth.rs", "src/deep/util.rs", "src/main.rs"]
    );
    assert_eq!(said["total"], 3);
    assert_eq!(said["more"], false);
    // The count is the answer's, not the caller's. Without it, "three matches"
    // and "three matches out of four files" are the same sentence.
    assert!(said["looked_at"].as_u64().unwrap() >= 5);
}

#[test]
fn a_program_asks_which_files_say_something_and_is_told_the_line_as_well() {
    let dir = a_project();
    let output = typed(
        dir.path(),
        &[
            "structured on",
            &format!("contenido en={} login()", dir.path().display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "grep");
    assert_eq!(said["ok"], true);
    let hits: Vec<(&str, u64)> = said["hits"]
        .as_array()
        .expect("hits is an array")
        .iter()
        .map(|row| {
            (
                row["path"].as_str().expect("a path"),
                row["line"].as_u64().expect("a line"),
            )
        })
        .collect();
    // `login()` with its parentheses, literally: `auth.rs` declares it and
    // `main.rs` calls it, and the two prose mentions of the bare word do not
    // match. That is the whole claim of the text being literal.
    assert_eq!(hits, vec![("src/auth.rs", 1), ("src/main.rs", 1)]);
    assert_eq!(said["text"], "login()");
}

#[test]
fn the_hidden_directory_is_not_searched_and_neither_verb_reaches_into_it() {
    // `.git/config` says `login` and is the one file in this tree that must
    // never come back. A search that reached into it would answer about a file
    // the index has never heard of, and the person would conclude the index is
    // broken rather than that two walks disagree.
    let dir = a_project();
    let output = typed(
        dir.path(),
        &[
            "structured on",
            &format!("contenido en={} login", dir.path().display()),
            &format!("encontrar en={} config", dir.path().display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "grep");
    let paths: Vec<&str> = said["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();
    assert!(
        !paths.iter().any(|path| path.starts_with(".git")),
        "the hidden directory was searched: {paths:?}"
    );
    // The control: the same search did find the four ordinary files, so this is
    // a walk that skipped `.git` and not a walk that found nothing.
    assert_eq!(paths.len(), 4, "{paths:?}");

    assert_eq!(answer_to(&output, "find")["total"], 0);
}

#[test]
fn a_person_gets_sentences_and_a_program_gets_objects_for_the_same_search() {
    let dir = a_project();
    let output = typed(
        dir.path(),
        &[
            &format!("contenido en={} login()", dir.path().display()),
            "salir",
        ],
    );

    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        said.contains("src/auth.rs:1:"),
        "a person is told the file and the line: {said}"
    );
    assert!(
        objects(&output).iter().all(|value| value["op"] != "grep"),
        "the human face printed an object"
    );
}

#[test]
fn a_search_that_found_nothing_says_how_much_it_read_rather_than_going_quiet() {
    // Silence is never an answer — a parser cannot tell it from a hang, and a
    // person cannot tell "no match" from "wrong folder".
    let dir = a_project();
    let output = typed(
        dir.path(),
        &[
            "structured on",
            &format!("contenido en={} zzzznotinhere", dir.path().display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "grep");
    assert_eq!(said["ok"], true);
    assert_eq!(said["total"], 0);
    assert!(said["looked_at"].as_u64().unwrap() >= 4);
    // Rule 10, and always present: a caller that only sees the key when
    // something failed is a caller that never wrote the branch.
    assert!(said["unreadable"].is_array());
    assert!(said["not_text"].is_number());
}

#[test]
fn a_long_answer_arrives_cut_with_its_total_and_a_cursor_that_resumes() {
    // Punto B1. The failure is the quiet one: a search of a large tree does not
    // fail and does not warn, it just spends the caller's whole context window.
    let dir = tempfile::tempdir().unwrap();
    for n in 0..250 {
        std::fs::write(dir.path().join(format!("f{n:04}.txt")), "needle\n").unwrap();
    }

    let first = typed(
        dir.path(),
        &[
            "structured on",
            &format!("contenido en={} needle", dir.path().display()),
            "salir",
        ],
    );
    let said = answer_to(&first, "grep");
    assert_eq!(said["total"], 250);
    assert_eq!(said["sent"], 200);
    assert_eq!(said["more"], true);
    let cursor = said["cursor"].as_str().expect("a cursor to resume from");

    let second = typed(
        dir.path(),
        &[
            "structured on",
            &format!(
                "contenido en={} cursor={cursor} needle",
                dir.path().display()
            ),
            "salir",
        ],
    );
    let rest = answer_to(&second, "grep");
    assert_eq!(rest["sent"], 50);
    assert_eq!(rest["before"], 200);
    assert_eq!(rest["more"], false);
    assert_eq!(rest["continuity"], "unchanged");
    // The two pages together are the whole thing and nothing twice, which is
    // the only claim a cursor is for.
    assert_eq!(rest["hits"][0]["path"], "f0200.txt");
}

#[test]
fn a_flag_typed_after_the_text_is_searched_for_rather_than_obeyed() {
    // The rule this file exists to pin down, at the prompt rather than in a
    // unit test: a flag recognised anywhere would turn a search for the text
    // `en=produccion` into a search of a folder — an answer that looks right,
    // arrives fast, and is about a different question.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("a.txt"), "deploy en=produccion\n").unwrap();
    std::fs::write(dir.path().join("src/b.txt"), "deploy\n").unwrap();

    let output = typed(
        dir.path(),
        &[
            "structured on",
            &format!("contenido en={} deploy en=produccion", dir.path().display()),
            "salir",
        ],
    );
    let said = answer_to(&output, "grep");
    assert_eq!(said["text"], "deploy en=produccion");
    assert_eq!(said["total"], 1);
    assert_eq!(said["hits"][0]["path"], "a.txt");
}

#[test]
fn asking_for_nothing_is_refused_by_name_rather_than_answered_with_everything() {
    let dir = a_project();
    let output = typed(
        dir.path(),
        &["structured on", "contenido", "encontrar", "salir"],
    );

    let objects = objects(&output);
    let refusals: Vec<&serde_json::Value> = objects
        .iter()
        .filter(|value| value["op"] == "grep" || value["op"] == "find")
        .collect();
    assert_eq!(refusals.len(), 2, "both verbs answered");
    for refusal in refusals {
        assert_eq!(refusal["ok"], false);
        assert_eq!(refusal["error"], "nothing_asked");
    }
}

#[test]
fn a_binary_in_the_tree_is_counted_and_never_printed_at_the_prompt() {
    // On the image there is no second terminal to recover in, so a screenful of
    // an ELF is not a cosmetic problem.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("real.txt"), "needle\n").unwrap();
    std::fs::write(
        dir.path().join("thing.bin"),
        b"needle\x00\x01\x02\x03needle",
    )
    .unwrap();

    let output = typed(
        dir.path(),
        &[
            "structured on",
            &format!("contenido en={} needle", dir.path().display()),
            "salir",
        ],
    );
    let said = answer_to(&output, "grep");
    assert_eq!(said["total"], 1);
    assert_eq!(said["hits"][0]["path"], "real.txt");
    assert_eq!(said["not_text"], 1);
}

#[test]
fn the_session_searches_where_it_is_standing_when_no_folder_is_named() {
    // The double-route point: a person moves with `ir` and then searches, and a
    // program that cannot move says `en=`. Both have to reach the same tree, or
    // one route has quietly lost a capability.
    let dir = a_project();
    let output = typed(
        dir.path(),
        &[
            "structured on",
            &format!("ir {}", dir.path().join("src").display()),
            "encontrar *.rs",
            "salir",
        ],
    );
    let said = answer_to(&output, "find");
    let found: Vec<&str> = said["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();
    // Relative to `src`, because that is what the session was standing in.
    assert_eq!(found, vec!["auth.rs", "deep/util.rs", "main.rs"]);
}

#[test]
fn a_file_named_where_a_folder_goes_is_refused_with_a_word_and_a_remedy() {
    let dir = a_project();
    let file = dir.path().join("notes.txt");
    let output = typed(
        dir.path(),
        &[
            "structured on",
            &format!("contenido en={} login", file.display()),
            "salir",
        ],
    );
    let said = answer_to(&output, "grep");
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"], "not_a_directory");
    // A2: an error that names the way out is documentation delivered at the
    // moment it is useful, and it costs one field.
    assert_eq!(said["remedy"], "name_a_folder");
}
