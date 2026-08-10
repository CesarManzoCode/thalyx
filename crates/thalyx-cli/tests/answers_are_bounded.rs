//! Long answers, cut to something a context window survives.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **B1**. The failure
//! is the quiet one of the five costs: `ls` on a directory of forty thousand
//! files does not fail and does not warn, it produces a caller that spent its
//! whole window on names and forgot the task — which reads from outside as a
//! stupid agent rather than as a system that handed one too much.
//!
//! Every test here types at a real session and parses what comes back, because
//! rule 1 of `Estrategia-de-Pruebas.md` is that every real defect came from
//! running the system. The paging arithmetic already has unit tests in
//! `thalyx-files::window`, and all of them would stay green while nothing at a
//! prompt could ask for a second page.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

fn piped(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session");

    let mut typed = String::new();
    for line in lines {
        typed.push_str(line);
        typed.push('\n');
    }
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(typed.as_bytes())
        .expect("feeding the session");

    child.wait_with_output().expect("waiting for the session")
}

fn objects(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .replace('\r', "")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(|value| value.is_object())
        .collect()
}

fn answer_to(objects: &[serde_json::Value], op: &str) -> serde_json::Value {
    objects
        .iter()
        .find(|value| value["op"] == serde_json::json!(op))
        .unwrap_or_else(|| panic!("nothing answered `{op}`; got {objects:#?}"))
        .clone()
}

/// One question, asked from a session that starts and ends around it.
///
/// A session per page rather than one session driven line by line, because the
/// cursor has to survive being carried out of the machine and back in — that is
/// what a caller does, and a cursor that only worked inside one process would
/// pass a test that drove the session as one conversation.
fn ask(root: &Path, at: &Path, line: &str, op: &str) -> serde_json::Value {
    let output = piped(
        root,
        &[
            "structured on",
            &format!("cd {}", at.display()),
            line,
            "salir",
        ],
    );
    answer_to(&objects(&output), op)
}

/// A directory with more in it than one answer is allowed to carry.
fn a_crowded_directory(how_many: usize) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a store");
    let crowd = root.path().join("crowd");
    std::fs::create_dir(&crowd).expect("the directory");
    for n in 0..how_many {
        std::fs::write(crowd.join(format!("file-{n:05}.txt")), "x").expect("a file");
    }
    root
}

#[test]
fn a_directory_bigger_than_a_window_arrives_cut_with_the_whole_count_said() {
    let root = a_crowded_directory(500);
    let answer = ask(root.path(), &root.path().join("crowd"), "ls", "list");

    assert_eq!(answer["sent"], serde_json::json!(200));
    // The number that makes the cut honest. Without it this answer and a
    // directory of exactly two hundred files are the same answer, and there is
    // no cheaper way for the caller to find out which one it got.
    assert_eq!(answer["total"], serde_json::json!(500));
    assert_eq!(answer["more"], serde_json::json!(true));
    assert!(answer["cursor"].is_string(), "no way to ask for the rest");
    assert_eq!(answer["entries"].as_array().unwrap().len(), 200);
}

#[test]
fn a_directory_that_fits_says_so_instead_of_offering_a_page_that_is_not_there() {
    let root = a_crowded_directory(3);
    let answer = ask(root.path(), &root.path().join("crowd"), "ls", "list");

    assert_eq!(answer["total"], serde_json::json!(3));
    assert_eq!(answer["more"], serde_json::json!(false));
    // `null` and not a token: a caller that follows a cursor it was handed at
    // the end of a collection asks forever for nothing.
    assert_eq!(answer["cursor"], serde_json::Value::Null);
    // And it is the first page, which is not the same claim as "nothing
    // changed" — no cursor was given, so nothing was compared.
    assert_eq!(answer["continuity"], serde_json::json!("first_page"));
}

#[test]
fn the_pages_together_are_the_whole_directory_and_no_name_twice() {
    let root = a_crowded_directory(450);
    let crowd = root.path().join("crowd");

    let mut seen: Vec<String> = Vec::new();
    let mut line = "ls limite=100".to_string();
    for _ in 0..10 {
        let answer = ask(root.path(), &crowd, &line, "list");
        for entry in answer["entries"].as_array().unwrap() {
            seen.push(entry["name"].as_str().unwrap().to_string());
        }
        match answer["cursor"].as_str() {
            Some(token) => line = format!("ls limite=100 cursor={token}"),
            None => break,
        }
    }

    let mut expected: Vec<String> = (0..450).map(|n| format!("file-{n:05}.txt")).collect();
    expected.sort();
    seen.sort();
    assert_eq!(seen, expected, "paging lost or repeated names");
}

#[test]
fn a_file_deleted_behind_the_cursor_does_not_make_the_next_page_skip_one() {
    // The whole reason the cursor is a key and not an offset. With `skip(5)` a
    // deletion earlier in the directory shifts everything up by one, and the
    // name that was at position five is never sent to anybody — no error, no
    // gap, and a caller that concludes the file is not there.
    let root = a_crowded_directory(10);
    let crowd = root.path().join("crowd");

    let first = ask(root.path(), &crowd, "ls limite=5", "list");
    let token = first["cursor"].as_str().expect("a cursor").to_string();

    std::fs::remove_file(crowd.join("file-00000.txt")).expect("removing one from behind");

    let second = ask(
        root.path(),
        &crowd,
        &format!("ls limite=5 cursor={token}"),
        "list",
    );
    let names: Vec<&str> = second["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();

    assert_eq!(
        names,
        vec![
            "file-00005.txt",
            "file-00006.txt",
            "file-00007.txt",
            "file-00008.txt",
            "file-00009.txt"
        ]
    );
}

#[test]
fn a_directory_that_moved_between_pages_says_so_in_the_same_object_as_the_rows() {
    let root = a_crowded_directory(10);
    let crowd = root.path().join("crowd");

    let token = ask(root.path(), &crowd, "ls limite=5", "list")["cursor"]
        .as_str()
        .expect("a cursor")
        .to_string();

    std::fs::remove_file(crowd.join("file-00000.txt")).expect("a change behind the cursor");
    let moved = ask(
        root.path(),
        &crowd,
        &format!("ls limite=5 cursor={token}"),
        "list",
    );
    // The rule of honesty `FS-en-Grafo` decrees for the index, generalised: the
    // caveat and the rows arrive together, because a caveat sent separately is
    // one that gets dropped.
    assert_eq!(moved["continuity"], serde_json::json!("changed"));

    // The control, without which `changed` is indistinguishable from a stamp
    // that never agrees with itself.
    let root = a_crowded_directory(10);
    let crowd = root.path().join("crowd");
    let token = ask(root.path(), &crowd, "ls limite=5", "list")["cursor"]
        .as_str()
        .expect("a cursor")
        .to_string();
    let untouched = ask(
        root.path(),
        &crowd,
        &format!("ls limite=5 cursor={token}"),
        "list",
    );
    assert_eq!(untouched["continuity"], serde_json::json!("unchanged"));
}

#[test]
fn asking_for_none_of_it_answers_how_many_there_are() {
    // A caller that only wants to know how big a directory is should not have to
    // receive any of it to find out. It is the cheapest possible answer to the
    // second cost, and it exists because the alternative — read the page and
    // count — is the caller paying for names it did not want.
    let root = a_crowded_directory(1000);
    let answer = ask(
        root.path(),
        &root.path().join("crowd"),
        "ls limite=0",
        "list",
    );

    assert_eq!(answer["total"], serde_json::json!(1000));
    assert_eq!(answer["sent"], serde_json::json!(0));
    assert_eq!(answer["entries"], serde_json::json!([]));
    assert_eq!(answer["more"], serde_json::json!(true));
}

#[test]
fn a_cursor_this_machine_did_not_write_is_refused_and_not_quietly_ignored() {
    // Rule 9: fail closed. Starting over from the top would look like an answer
    // and be a different one — a caller resuming a walk would silently process
    // the first page twice and never notice.
    let root = a_crowded_directory(10);
    let answer = ask(
        root.path(),
        &root.path().join("crowd"),
        "ls cursor=w9.deadbeefdeadbeef.61",
        "list",
    );

    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("bad_cursor"));
    assert!(
        answer["entries"].is_null(),
        "a refusal carried rows: {answer}"
    );
}

#[test]
fn a_limit_that_is_not_a_number_is_a_place_and_says_so() {
    // The alternative is worse than it looks: swallowing `limite=dos` would hand
    // back a default window the caller never asked for, and nothing in the
    // answer would say the number had been ignored.
    let root = a_crowded_directory(3);
    let answer = ask(
        root.path(),
        &root.path().join("crowd"),
        "ls limite=dos",
        "list",
    );

    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("absent"));
}

#[test]
fn the_person_is_never_handed_a_cut_listing() {
    // The control for every test above, and the half of the decree that is not
    // about the model: `Principio-Doble-Ruta` says the human keeps everything,
    // and a window is a fact about a context window — which a person does not
    // have. On the image there is no pager to get the rest back with, so a cut
    // human listing would be capability taken away with nothing given back.
    let root = a_crowded_directory(500);
    let crowd = root.path().join("crowd");
    let output = piped(
        root.path(),
        &[&format!("cd {}", crowd.display()), "ls", "salir"],
    );
    let said = String::from_utf8_lossy(&output.stdout);

    assert!(said.contains("file-00000.txt"), "the first name is missing");
    assert!(
        said.contains("file-00499.txt"),
        "the person was cut off at the window: {}",
        said.lines().count()
    );
}

#[test]
fn the_index_pages_with_the_same_words_as_the_listing() {
    // One idea, one spelling. A caller that has to learn `limite=` for `ls` and
    // something else for `usan` pays the discovery cost twice for the same
    // thought, which is the cost `Superficie-para-el-LLM.md` exists to lower.
    let root = tempfile::tempdir().expect("a store");
    let tree = root.path().join("proyecto");
    std::fs::create_dir_all(tree.join("src")).expect("the tree");
    std::fs::write(tree.join("src/dos.rs"), "pub fn f() {}\n").expect("a file");
    for n in 0..4 {
        std::fs::write(
            tree.join(format!("src/uno{n}.rs")),
            "use crate::dos;\npub fn g() {}\n",
        )
        .expect("a file");
    }

    let output = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", tree.display()),
            "indexar",
            "usan src/dos.rs limite=2",
            "salir",
        ],
    );
    let answer = answer_to(&objects(&output), "depended_on_by");

    assert_eq!(answer["total"], serde_json::json!(4));
    assert_eq!(answer["sent"], serde_json::json!(2));
    assert_eq!(answer["more"], serde_json::json!(true));
    assert!(answer["cursor"].is_string(), "{answer}");
    // And the freshness is still in the same object: paging must not have
    // pushed out the field that says whether any of this is still true.
    assert!(answer.get("fresh").is_some(), "{answer}");
}
