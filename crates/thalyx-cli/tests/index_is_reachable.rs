//! The semantic index, asked by something that is not Thalyx's own CLI.
//!
//! `vault/03-Primitivas/FS-en-Grafo.md` calls itself the founding example of a
//! primitive native to the AI rather than inherited from a design meant for
//! humans. It was built, it was tested, and for as long as it existed the only
//! thing that could ask it anything was `thalyx graph` — which is not something
//! an agent living in a session can reach.
//!
//! These drive the session, so what they prove is reachability and not
//! correctness of the index itself; `thalyx-graph` has its own tests for the
//! second. Reachable and correct are different claims and this project has been
//! caught by the gap between them before: modules were installed, correct, and
//! unexecutable for weeks.

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
        .filter(serde_json::Value::is_object)
        .collect()
}

fn answer_to(objects: &[serde_json::Value], op: &str) -> serde_json::Value {
    objects
        .iter()
        .find(|value| value["op"] == serde_json::json!(op))
        .unwrap_or_else(|| panic!("nothing answered `{op}`; got {objects:#?}"))
        .clone()
}

/// A store, and beside it a small Rust tree with a real dependency in it.
///
/// Two crates' worth of files would be noise; two files with one `mod` between
/// them is the whole question — `uno.rs` refers to `dos.rs`, and nothing in the
/// name or the location of either says so.
fn a_tree_with_a_dependency() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("a store");
    let tree = root.path().join("proyecto");
    std::fs::create_dir_all(tree.join("src")).expect("the tree");
    std::fs::write(
        tree.join("src/uno.rs"),
        "mod dos;\n\npub fn arranca() { dos::hace(); }\n",
    )
    .expect("a file");
    std::fs::write(tree.join("src/dos.rs"), "pub fn hace() {}\n").expect("a file");
    (root, tree)
}

fn inside(place: &Path) -> String {
    format!("cd {}", place.display())
}

#[test]
fn a_program_can_index_a_tree_from_the_session() {
    let (root, tree) = a_tree_with_a_dependency();
    let output = piped(
        root.path(),
        &["structured on", &inside(&tree), "indexar", "salir"],
    );
    let built = answer_to(&objects(&output), "index_build");

    assert_eq!(built["ok"], serde_json::json!(true));
    assert!(
        built["files_indexed"].as_u64().unwrap() >= 2,
        "indexed {}",
        built["files_indexed"]
    );
    // Named rather than dropped: a file the parser could not read is not a file
    // with no dependencies, and a caller reading the second would conclude
    // things about a tree it has not seen.
    assert!(built.get("skipped").is_some(), "{built}");
}

#[test]
fn the_question_no_directory_walk_can_answer_is_answered() {
    let (root, tree) = a_tree_with_a_dependency();
    let output = piped(
        root.path(),
        &[
            "structured on",
            &inside(&tree),
            "indexar",
            "usan src/dos.rs",
            "salir",
        ],
    );
    let answer = answer_to(&objects(&output), "depended_on_by");

    // Nothing about the name or the location of `dos.rs` says that `uno.rs`
    // refers to it. That is the whole reason the index exists, and until now
    // nothing living in a session could ask it.
    assert_eq!(answer["ok"], serde_json::json!(true));
    assert_eq!(answer["count"], serde_json::json!(1));
    assert_eq!(answer["edges"][0]["from"], serde_json::json!("src/uno.rs"));
}

#[test]
fn the_other_direction_works_too_and_says_where_the_reference_is() {
    let (root, tree) = a_tree_with_a_dependency();
    let output = piped(
        root.path(),
        &[
            "structured on",
            &inside(&tree),
            "indexar",
            "depende src/uno.rs",
            "salir",
        ],
    );
    let answer = answer_to(&objects(&output), "depends_on");

    assert_eq!(answer["count"], serde_json::json!(1));
    // The line number is what makes this cheaper than reading the file: a
    // caller that wants the context knows exactly which line to ask for.
    assert_eq!(answer["edges"][0]["line"], serde_json::json!(1));
}

#[test]
fn every_answer_carries_the_indexs_freshness_in_the_same_object_as_the_rows() {
    let (root, tree) = a_tree_with_a_dependency();
    let output = piped(
        root.path(),
        &[
            "structured on",
            &inside(&tree),
            "indexar",
            "usan src/dos.rs",
            "salir",
        ],
    );
    let answer = answer_to(&objects(&output), "depended_on_by");

    // The decreed rule of honesty of FS-en-Grafo, on the wire. Not two calls:
    // whoever wants the rows receives the caveat whether they asked or not,
    // because making it separable is exactly how a cache starts being mistaken
    // for the truth.
    assert_eq!(answer["fresh"], serde_json::json!("current"));
    assert!(answer.get("freshness_detail").is_some(), "{answer}");
}

#[test]
fn a_tree_that_moved_on_is_reported_as_stale_rather_than_answered_as_if_it_had_not() {
    let (root, tree) = a_tree_with_a_dependency();

    // Indexed, then changed behind Thalyx's back — which is the ordinary case,
    // because the human is free to edit files without telling anybody.
    let indexed = piped(
        root.path(),
        &["structured on", &inside(&tree), "indexar", "salir"],
    );
    assert_eq!(
        answer_to(&objects(&indexed), "index_build")["ok"],
        serde_json::json!(true)
    );

    std::fs::write(tree.join("src/tres.rs"), "pub fn nueva() {}\n").expect("a new file");

    let output = piped(
        root.path(),
        &["structured on", &inside(&tree), "usan src/dos.rs", "salir"],
    );
    let answer = answer_to(&objects(&output), "depended_on_by");

    // The rows are still returned — they are not wrong, they are incomplete —
    // and the answer says so in the same breath. An agent that reads this can
    // decide; one that was never told would trust an answer about a tree that
    // has moved on.
    assert_eq!(answer["fresh"], serde_json::json!("stale"));
    assert!(
        answer["freshness_detail"].as_str().expect("a detail").len() > 5,
        "the staleness said nothing useful: {answer}"
    );
}

#[test]
fn asking_without_naming_a_file_answers_instead_of_waiting() {
    let (root, tree) = a_tree_with_a_dependency();
    let output = piped(
        root.path(),
        &["structured on", &inside(&tree), "usan", "salir"],
    );
    let answer = answer_to(&objects(&output), "depended_on_by");

    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("incomplete"));
}
