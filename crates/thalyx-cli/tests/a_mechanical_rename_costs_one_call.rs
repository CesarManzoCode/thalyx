//! The same mechanical rename, asked for both ways, and counted.
//!
//! ## Why this file exists
//!
//! `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md` records the first
//! reversible benchmark run: the same model, the same task — rename one
//! identifier across a small Rust project and then put it back — on Linux with
//! its own tools and inside Thalyx with only Thalyx's. Both arms answered
//! correctly, both restored the tree, and Thalyx cost less money and **more
//! wall clock**. The trace said why. Arm A's editor replaces every occurrence
//! in a file in one call, so it made six calls; arm B could only address lines,
//! so it made sixteen, each carrying the whole new text of a line.
//!
//! Nothing about that was a defect in what Thalyx *does*. It was the shape of
//! the write surface, and this file is the regression that holds the fix in
//! place — **without Claude, without the API and without spending anything**.
//! It reproduces the *form* of that task (a definition with several mentions of
//! its own name, dependents in a second directory, one mechanical change), does
//! it both ways, and counts.
//!
//! ## What it measures, and what it deliberately does not
//!
//! It measures **logical operations** — how many times a caller has to speak to
//! the machine — and the bytes each surface costs in both directions. It does
//! not measure wall clock: this container's timings say nothing about the
//! machine the benchmark runs on, and a number that looks like a measurement
//! and is noise is worse than no number.
//!
//! It also is not a benchmark result. Whether the operation moves the numbers
//! in `Evidencia-de-Agentes.md` can only be answered by running that benchmark
//! again, with the harness frozen exactly as it is.
//!
//! Run it with the numbers on screen:
//!
//! ```sh
//! cargo test -p thalyx-cli --test a_mechanical_rename_costs_one_call -- --nocapture
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The name the fixture defines and mentions, and what it becomes.
///
/// Deliberately **not** `UidRegistry`. The benchmark is frozen and this must
/// not be a test that happens to work for the one identifier that suite uses —
/// what is being held in place is the shape of the operation, so the fixture
/// carries a different name in a different project.
const NAME: &str = "SlotTable";
const RENAMED: &str = "SlotTableRenamed";

/// A project shaped like the one the benchmark renames in: a definition file
/// that mentions its own name several times, four dependents beside it, and one
/// in a second crate that reaches the name without importing it by file.
const PROJECT: &[(&str, &str)] = &[
    (
        "core/src/slots.rs",
        "//! Where slots are handed out.\n\
         \n\
         /// Every slot this machine has given away.\n\
         pub struct SlotTable {\n\
         \x20   next: u32,\n\
         }\n\
         \n\
         impl SlotTable {\n\
         \x20   pub fn new() -> SlotTable {\n\
         \x20       SlotTable { next: 1 }\n\
         \x20   }\n\
         \n\
         \x20   pub fn merge(one: SlotTable, other: SlotTable) -> SlotTable {\n\
         \x20       let _ = (one, other);\n\
         \x20       SlotTable::new()\n\
         \x20   }\n\
         }\n",
    ),
    (
        "core/src/rollback.rs",
        "use crate::slots::SlotTable;\n\
         \n\
         pub fn undo(table: &SlotTable) -> bool {\n\
         \x20   let _ = table;\n\
         \x20   true\n\
         }\n",
    ),
    (
        "core/src/install.rs",
        "use crate::slots::SlotTable;\n\
         \n\
         pub fn install(table: &mut SlotTable) {\n\
         \x20   let _ = table;\n\
         }\n",
    ),
    (
        "core/src/run.rs",
        "use crate::slots::SlotTable;\n\
         \n\
         pub fn run(table: SlotTable) -> SlotTable {\n\
         \x20   table\n\
         }\n",
    ),
    (
        "core/src/foreign.rs",
        "use crate::slots::SlotTable;\n\
         \n\
         pub fn foreign(table: &SlotTable) {\n\
         \x20   let _ = table;\n\
         }\n",
    ),
    (
        // The one in a second crate, which reaches the name without an import
        // that says which file it lives in. It is the dependent a caller
        // working by grep has to search for, and it is why the fixture has two
        // directories rather than one.
        "cli/src/render.rs",
        "pub fn show(table: &core::slots::SlotTable) {\n\
         \x20   let _ = table;\n\
         }\n\
         // SlotTable is printed by the line above.\n",
    ),
];

/// What the fixture holds, counted by hand from the text above.
///
/// Sixteen lines is the number that matters, and it is the one the benchmark
/// produced: arm B made sixteen mutations because sixteen lines held the name.
/// Nineteen places on those sixteen lines is what makes the two counts
/// different, which they have to be — a surface that reported one number for
/// both would be right by accident on a fixture where every line held one.
///
/// Written down rather than computed. A test that works out its own expectation
/// with the same walk the code makes is a test of nothing, and this project has
/// paid twice for a parser checked only against what its author assumed.
const PLACES: usize = 19;
const LINES: usize = 16;
const FILES: usize = 6;

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// Type these lines at one session and give back everything it said.
fn typed(root: &Path, lines: &[String]) -> Output {
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

/// Lay the fixture down and give back its files in the order they are written.
fn project(root: &Path) -> Vec<PathBuf> {
    PROJECT
        .iter()
        .map(|(name, body)| {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).expect("a directory");
            std::fs::write(&path, body).expect("a file");
            path
        })
        .collect()
}

/// Every file of the tree with its exact bytes, read with something that is not
/// Thalyx — which is the only reading that can say a tree came back.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    PROJECT
        .iter()
        .map(|(name, _)| {
            (
                (*name).to_string(),
                std::fs::read(root.join(name)).expect("a file"),
            )
        })
        .collect()
}

/// Every place the name occurs, as `(file, line number, the whole new line)`.
///
/// This is the arithmetic of the **old** surface: a caller that can only
/// address lines has to know each one and send its whole new text.
fn every_line_to_rewrite(root: &Path) -> Vec<(PathBuf, usize, String)> {
    let mut found = Vec::new();
    for (name, _) in PROJECT {
        let path = root.join(name);
        let body = std::fs::read_to_string(&path).expect("a file");
        for (index, line) in body.lines().enumerate() {
            if line.contains(NAME) {
                found.push((path.clone(), index + 1, line.replace(NAME, RENAMED)));
            }
        }
    }
    found
}

#[test]
fn the_fixture_is_the_shape_the_benchmark_renames_in() {
    // The control for every number below. Without it, a fixture that quietly
    // lost half its mentions would make the new surface look better and the old
    // one look cheaper, and nothing would say so.
    let tmp = tempfile::tempdir().unwrap();
    project(tmp.path());

    let places: usize = PROJECT
        .iter()
        .map(|(name, _)| {
            std::fs::read_to_string(tmp.path().join(name))
                .unwrap()
                .matches(NAME)
                .count()
        })
        .sum();
    assert_eq!(
        places, PLACES,
        "the fixture no longer holds {PLACES} places"
    );
    assert_eq!(every_line_to_rewrite(tmp.path()).len(), LINES);
    assert_eq!(PROJECT.len(), FILES);
    // Two directories, because a dependent in a second crate is the one a
    // caller cannot find by looking beside the definition.
    assert!(PROJECT.iter().any(|(name, _)| name.starts_with("cli/")));
}

#[test]
fn the_line_by_line_surface_needs_one_call_for_every_line_and_the_new_surface_needs_one() {
    let old_tree = tempfile::tempdir().unwrap();
    let new_tree = tempfile::tempdir().unwrap();
    project(old_tree.path());
    let files = project(new_tree.path());
    let original = tree(new_tree.path());

    // ── the surface as it was: one call per line, each carrying a whole line ──
    let rewrites = every_line_to_rewrite(old_tree.path());
    let mut old_lines = vec!["structured on".to_string()];
    for (path, number, body) in &rewrites {
        old_lines.push(format!("editar {} cambiar {number} {body}", path.display()));
    }
    old_lines.push("salir".to_string());
    let old_said = typed(old_tree.path(), &old_lines);
    let old_answers: Vec<serde_json::Value> = objects(&old_said)
        .into_iter()
        .filter(|value| value["op"] == "edit")
        .collect();
    assert_eq!(old_answers.len(), LINES);
    for answer in &old_answers {
        assert_eq!(answer["ok"], true, "{answer}");
    }

    // ── the surface as it is: one call, every file, every place ──────────────
    let mut naming = format!("editar {} sustituir {NAME} {RENAMED}", files[0].display());
    for path in &files[1..] {
        naming.push(' ');
        naming.push_str(&path.display().to_string());
    }
    let new_said = typed(
        new_tree.path(),
        &[
            "structured on".to_string(),
            naming.clone(),
            "salir".to_string(),
        ],
    );
    let new_answers: Vec<serde_json::Value> = objects(&new_said)
        .into_iter()
        .filter(|value| value["op"] == "edit")
        .collect();

    // **The claim of this whole delivery**, and it is one number.
    assert_eq!(new_answers.len(), 1, "{new_answers:?}");
    let done = &new_answers[0];
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(done["files"], FILES);
    assert_eq!(done["replacements"], PLACES);

    // ── and the two surfaces produced the same tree ──────────────────────────
    //
    // The half that makes the count worth having. Sixteen calls and one call
    // are only comparable if they did the same thing, and both trees are read
    // here with `std::fs` rather than asked about.
    assert_eq!(
        tree(old_tree.path()),
        tree(new_tree.path()),
        "the two surfaces disagree about what the rename is"
    );

    // ── put it back, which is the fifth step of the benchmark's own task ─────
    let mut back = format!("editar {} sustituir {RENAMED} {NAME}", files[0].display());
    for path in &files[1..] {
        back.push(' ');
        back.push_str(&path.display().to_string());
    }
    let restored = typed(
        new_tree.path(),
        &["structured on".to_string(), back, "salir".to_string()],
    );
    let put_back: Vec<serde_json::Value> = objects(&restored)
        .into_iter()
        .filter(|value| value["op"] == "edit")
        .collect();
    assert_eq!(put_back.len(), 1);
    assert_eq!(put_back[0]["replacements"], PLACES);
    assert_eq!(
        tree(new_tree.path()),
        original,
        "the tree did not come back byte for byte"
    );

    // ── what it cost, in both directions ─────────────────────────────────────
    let old_sent: usize = old_lines.iter().map(String::len).sum();
    let new_sent = naming.len();
    let old_returned: usize = old_answers
        .iter()
        .map(|value| value.to_string().len())
        .sum();
    let new_returned = done.to_string().len();
    println!(
        "\n  a mechanical rename of `{NAME}`: {PLACES} places on {LINES} lines in {FILES} files\n\
         \x20   line by line   {LINES} calls   {old_sent} bytes sent   {old_returned} bytes back\n\
         \x20   substitution    1 call    {new_sent} bytes sent   {new_returned} bytes back\n"
    );

    // Read as claims rather than as decoration. The first is the operation
    // count, which is what the trace blamed for the wall clock. The second is
    // the model's own output: on the old surface the caller has to *write* the
    // new text of every line, which is where the 51% more output tokens went.
    assert!(
        old_answers.len() >= 10 * new_answers.len(),
        "this fixture no longer reproduces the shape it was built for"
    );
    assert!(
        new_sent * 4 < old_sent,
        "the new surface is not meaningfully cheaper to ask for: {new_sent} against {old_sent}"
    );
}

#[test]
fn one_file_that_does_not_hold_the_name_costs_a_refusal_and_not_a_half_rename() {
    // The other half of "one call": a caller that names six files and is wrong
    // about one of them must not end up with five renamed. That is the state
    // that costs a reconstruction, and it is the state the preflight exists to
    // make impossible.
    let tmp = tempfile::tempdir().unwrap();
    let files = project(tmp.path());
    let stranger = tmp.path().join("core/src/nothing.rs");
    std::fs::write(&stranger, "pub fn nothing() {}\n").unwrap();
    let original = tree(tmp.path());

    let mut naming = format!("editar {} sustituir {NAME} {RENAMED}", files[0].display());
    for path in files[1..].iter().chain(std::iter::once(&stranger)) {
        naming.push(' ');
        naming.push_str(&path.display().to_string());
    }
    let said = typed(
        tmp.path(),
        &["structured on".to_string(), naming, "salir".to_string()],
    );
    let answer = objects(&said)
        .into_iter()
        .find(|value| value["op"] == "edit")
        .expect("an answer");

    assert_eq!(answer["ok"], false, "{answer}");
    assert_eq!(answer["error"], "no_occurrences");
    assert_eq!(answer["wrote"], false);
    assert_eq!(tree(tmp.path()), original, "a refused call wrote something");
}
