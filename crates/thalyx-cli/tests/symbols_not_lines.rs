//! Searching by symbol instead of by text, driven from a real session.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **C2**: *`grep`
//! contesta con renglones porque no sabe qué es un símbolo*. The parser does, in
//! five languages, so an answer here says "function `login`, `src/auth.rs`, line
//! 1, used in these two places" where `grep -r login` says three lines and
//! leaves the caller to work out which one is the definition — and catches every
//! comment that mentions the word.
//!
//! These type at a real prompt because rule 1 says every real defect came from
//! running the system. The `thalyx-graph` unit tests already cover what the
//! index stores, and all of them would stay green while nothing at a prompt
//! could ask it anything — which is precisely the state C1 was in for months.

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

/// A small project where the same word appears as a definition, as two calls,
/// and — the part that matters — inside a comment and inside a string.
fn a_project_with_a_word_used_four_ways() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("a store");
    let tree = root.path().join("proyecto");
    std::fs::create_dir_all(tree.join("src")).expect("the tree");

    std::fs::write(tree.join("src/auth.rs"), "pub fn login() {}\n").expect("a file");
    std::fs::write(
        tree.join("src/one.rs"),
        "use crate::auth;\nfn a() { login(); }\n",
    )
    .expect("a file");
    std::fs::write(
        tree.join("src/two.rs"),
        "use crate::auth;\nfn b() { login(); }\n",
    )
    .expect("a file");
    // Neither of these is a use of `login`, and `grep` cannot tell.
    std::fs::write(
        tree.join("src/noise.rs"),
        "// login is handled in auth.rs\nfn c() { println!(\"login failed\"); }\n",
    )
    .expect("a file");

    (root, tree)
}

fn asked(root: &Path, tree: &Path, line: &str) -> serde_json::Value {
    let output = piped(
        root,
        &[
            "structured on",
            &format!("cd {}", tree.display()),
            "indexar",
            line,
            "salir",
        ],
    );
    answer_to(&objects(&output), "symbol")
}

#[test]
fn a_name_answers_with_where_it_comes_from_and_who_uses_it() {
    let (root, tree) = a_project_with_a_word_used_four_ways();
    let answer = asked(root.path(), &tree, "buscar login");

    assert_eq!(answer["ok"], serde_json::json!(true));
    assert_eq!(answer["definitions"].as_array().unwrap().len(), 1);
    assert_eq!(
        answer["definitions"][0]["path"],
        serde_json::json!("src/auth.rs")
    );
    assert_eq!(answer["definitions"][0]["line"], serde_json::json!(1));
    // The kind is what makes this a symbol answer rather than a location. A
    // caller deciding whether it can call something needs to know it is
    // callable, and finding out by reading the file is the trip this saves.
    assert_eq!(
        answer["definitions"][0]["kind"],
        serde_json::json!("function")
    );
}

#[test]
fn a_comment_and_a_string_that_say_the_word_are_not_uses_of_it() {
    let (root, tree) = a_project_with_a_word_used_four_ways();
    let answer = asked(root.path(), &tree, "buscar login");

    let used: Vec<&str> = answer["uses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();

    // This is the whole difference from `grep`, and it is not cosmetic: a
    // caller that has to filter comments out itself is paying the ambiguity
    // cost the system was supposed to absorb, and it cannot filter what it
    // cannot see without opening every file.
    assert_eq!(used, vec!["src/one.rs", "src/two.rs"], "{answer}");
}

#[test]
fn a_name_nothing_declares_says_so_instead_of_answering_with_nothing() {
    let (root, tree) = a_project_with_a_word_used_four_ways();
    let answer = asked(root.path(), &tree, "buscar nombre_que_no_existe");

    // `ok` and empty is the right answer, and it is a different fact from a
    // failure. A caller that read a refusal here would retry; one that read an
    // empty success knows the tree does not have it.
    assert_eq!(answer["ok"], serde_json::json!(true));
    assert_eq!(answer["definitions"], serde_json::json!([]));
    assert_eq!(answer["uses"], serde_json::json!([]));
    assert_eq!(answer["total"], serde_json::json!(0));
}

#[test]
fn the_answer_carries_the_freshness_like_every_other_index_answer() {
    let (root, tree) = a_project_with_a_word_used_four_ways();
    let answer = asked(root.path(), &tree, "buscar login");

    // The decreed rule of `FS-en-Grafo`: the caveat travels in the same object
    // as the rows, because separating them is how a cache starts being mistaken
    // for the truth.
    assert_eq!(answer["fresh"], serde_json::json!("current"));
    assert!(answer.get("freshness_detail").is_some(), "{answer}");
}

#[test]
fn a_tree_that_changed_after_indexing_says_stale_and_still_answers() {
    let (root, tree) = a_project_with_a_word_used_four_ways();
    let output = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", tree.display()),
            "indexar",
            &format!("touch {}", tree.join("src/nuevo.rs").display()),
            "buscar login",
            "salir",
        ],
    );
    let answer = answer_to(&objects(&output), "symbol");

    // Both halves. Refusing would leave a caller with nothing on a tree
    // somebody is working in, which is every real tree; answering silently
    // would let it believe an index that has moved on.
    assert_eq!(answer["fresh"], serde_json::json!("stale"));
    assert_eq!(answer["definitions"].as_array().unwrap().len(), 1);
}

#[test]
fn the_uses_are_paged_with_the_same_words_as_every_other_long_answer() {
    let root = tempfile::tempdir().expect("a store");
    let tree = root.path().join("proyecto");
    std::fs::create_dir_all(tree.join("src")).expect("the tree");
    std::fs::write(tree.join("src/auth.rs"), "pub fn login() {}\n").expect("a file");
    for n in 0..6 {
        std::fs::write(
            tree.join(format!("src/caller{n}.rs")),
            "use crate::auth;\nfn c() { login(); }\n",
        )
        .expect("a file");
    }

    let answer = asked(root.path(), &tree, "buscar login limite=2");

    assert_eq!(answer["total"], serde_json::json!(6));
    assert_eq!(answer["sent"], serde_json::json!(2));
    assert_eq!(answer["more"], serde_json::json!(true));
    // Which list the window describes. Two lists and one set of paging fields
    // would otherwise be a guess, and a caller that guessed wrong would page a
    // list that was never cut.
    assert_eq!(answer["window_of"], serde_json::json!("uses"));
    // And the definitions are never cut: a name with one definition would
    // otherwise carry a cursor for nothing.
    assert_eq!(answer["definitions"].as_array().unwrap().len(), 1);
}

#[test]
fn a_person_asking_the_same_thing_gets_sentences_and_all_of_it() {
    let (root, tree) = a_project_with_a_word_used_four_ways();
    let output = piped(
        root.path(),
        &[
            &format!("cd {}", tree.display()),
            "indexar",
            "buscar login",
            "salir",
        ],
    );
    let said = String::from_utf8_lossy(&output.stdout);

    // The half of `Principio-Doble-Ruta` that is not about the model. A verb
    // built for an agent that answers a person in JSON is a verb that took
    // something away from them.
    assert!(
        said.contains("src/auth.rs:1"),
        "the person was not told where it is defined:\n{said}"
    );
    assert!(
        said.contains("src/one.rs:2") && said.contains("src/two.rs:2"),
        "the person was not told who uses it:\n{said}"
    );
    assert!(
        !said
            .lines()
            .any(|line| line.trim().starts_with("{\"op\":\"symbol\"")),
        "a person who never asked was handed JSON:\n{said}"
    );
}

#[test]
fn asking_with_no_name_says_which_rather_than_answering_about_everything() {
    let (root, tree) = a_project_with_a_word_used_four_ways();
    let output = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", tree.display()),
            "indexar",
            "buscar",
            "salir",
        ],
    );
    let answer = answer_to(&objects(&output), "symbol");

    // Silence is never an answer: a program waiting on the stream cannot tell
    // it from a hang.
    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("incomplete"));
}
