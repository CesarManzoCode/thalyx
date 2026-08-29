//! Several exact substitutions, in one call, and the file on disk is what changed.
//!
//! **Rule 1**: nothing here inspects `thalyx-edit`. Every test types at the real
//! prompt of the real binary and then reads the files with `std::fs`, because
//! "the batch reported five substitutions" and "the bytes on disk hold five
//! substitutions" are two different claims.
//!
//! ## What this is measuring, and why the fixture is not the benchmark's
//!
//! `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md`, the reversible run of
//! 2026-08-29: the agent working through Thalyx needed **five** `thalyx_edit`
//! calls to rename one type, because the API could carry one `old`/`new` pair
//! across many files and could not carry many pairs at all. Five round trips
//! into the machine, five answers to read, one plan.
//!
//! So the fixture below is that plan's *shape* — a qualified path, a definition,
//! an impl, a type inside a tuple and a bare name — with names of its own. It is
//! deliberately not the benchmark's symbol: a test written against the thing
//! being measured is a test that will keep passing after the measurement stops
//! meaning anything, and this project does not put the benchmark's vocabulary
//! into the system under test.

use std::io::Write;
use std::path::{Path, PathBuf};
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

/// Every structured answer the session gave with this `op`, in order.
fn answers_to(output: &Output, op: &str) -> Vec<serde_json::Value> {
    objects(output)
        .into_iter()
        .filter(|value| value["op"] == op)
        .collect()
}

fn one_answer(output: &Output, op: &str) -> serde_json::Value {
    let all = answers_to(output, op);
    assert_eq!(
        all.len(),
        1,
        "expected one `{op}`; the session said:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    all.into_iter().next().expect("just counted")
}

/// The shape a mechanical rename actually has: one name, spelled five ways.
///
/// Written out rather than generated, so that every count asserted below was
/// counted by hand off this text. A fixture a test computes its expectations
/// from proves that two walks agree, which is not the claim.
fn a_tree_that_needs_five_patterns(root: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(root.join("store/src")).unwrap();
    std::fs::create_dir_all(root.join("api/src")).unwrap();
    let files = [
        (
            "store/src/ledger.rs",
            "pub struct Ledger;\n\
             impl Ledger {\n\
             \x20   pub fn open() -> (Ledger, usize) {\n\
             \x20       (Ledger, 0)\n\
             \x20   }\n\
             }\n",
        ),
        (
            "api/src/serve.rs",
            "use crate::store;\n\
             fn start() {\n\
             \x20   let held = store::Ledger::open();\n\
             \x20   let again = store::Ledger::open();\n\
             }\n",
        ),
        (
            "api/src/report.rs",
            "use crate::store::Ledger;\n\
             fn count(from: (Ledger, usize)) -> usize {\n\
             \x20   let _ = Ledger::open();\n\
             \x20   from.1\n\
             }\n",
        ),
    ];
    files
        .iter()
        .map(|(name, body)| {
            let path = root.join(name);
            std::fs::write(&path, body).unwrap();
            path
        })
        .collect()
}

/// The five patterns, in the order they have to run in.
///
/// The order is the point and it is not an implementation detail: the qualified
/// spelling has to go first, because after `store::Ledger::open` has become
/// `store::LedgerRenamed::open` the bare `Ledger::open` matches only the ones
/// that were always bare. Reverse these two and the batch means something else —
/// which is exactly why the semantics are written down in
/// `edit::substitute_batch` rather than left to whatever the loop happens to do.
fn five_patterns(files: &[PathBuf]) -> [(String, String, Vec<String>); 5] {
    let ledger = files[0].display().to_string();
    let serve = files[1].display().to_string();
    let report = files[2].display().to_string();
    [
        (
            "store::Ledger::open".into(),
            "store::LedgerRenamed::open".into(),
            vec![serve.clone()],
        ),
        (
            "pub struct Ledger;".into(),
            "pub struct LedgerRenamed;".into(),
            vec![ledger.clone()],
        ),
        (
            "impl Ledger {".into(),
            "impl LedgerRenamed {".into(),
            vec![ledger.clone()],
        ),
        (
            "(Ledger, usize)".into(),
            "(LedgerRenamed, usize)".into(),
            vec![ledger.clone(), report.clone()],
        ),
        (
            "Ledger::open".into(),
            "LedgerRenamed::open".into(),
            vec![report.clone()],
        ),
    ]
}

/// The batch, spelled the way the session reads it.
fn batch_line(patterns: &[(String, String, Vec<String>)]) -> String {
    let mut line = format!("editar '{}' sustituir-lote", patterns[0].2[0]);
    for (n, (old, new, paths)) in patterns.iter().enumerate() {
        line.push_str(&format!(" {} '{old}' '{new}'", paths.len()));
        // The first operation's first file is the name before the subverb, so
        // it is not repeated here.
        let from = usize::from(n == 0);
        for path in &paths[from..] {
            line.push_str(&format!(" '{path}'"));
        }
    }
    line
}

#[test]
fn five_patterns_in_one_call_leave_the_same_bytes_as_five_calls() {
    // **The claim the whole change rests on**, and the only honest way to make
    // it is with two trees: the same fixture renamed by five calls and by one
    // batch, compared byte for byte at the end. A test that asserted the batch
    // "worked" would pass for a batch that had invented its own semantics.
    let by_five = tempfile::tempdir().unwrap();
    let by_one = tempfile::tempdir().unwrap();
    let five_files = a_tree_that_needs_five_patterns(by_five.path());
    let one_files = a_tree_that_needs_five_patterns(by_one.path());

    let mut lines = vec!["structured on".to_string()];
    for (old, new, paths) in five_patterns(&five_files) {
        let mut line = format!("editar '{}' sustituir '{old}' '{new}'", paths[0]);
        for path in &paths[1..] {
            line.push_str(&format!(" '{path}'"));
        }
        lines.push(line);
    }
    lines.push("salir".to_string());
    let sequential = typed(
        by_five.path(),
        &lines.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    assert_eq!(
        answers_to(&sequential, "edit").len(),
        5,
        "the five-call shape did not make five calls:\n{}",
        String::from_utf8_lossy(&sequential.stdout)
    );

    let patterns = five_patterns(&one_files);
    let batched = typed(
        by_one.path(),
        &["structured on", &batch_line(&patterns), "salir"],
    );
    let said = one_answer(&batched, "edit");
    assert_eq!(said["ok"], true, "{said}");

    for (five, one) in five_files.iter().zip(&one_files) {
        assert_eq!(
            std::fs::read_to_string(five).unwrap(),
            std::fs::read_to_string(one).unwrap(),
            "{} came out differently under the batch",
            one.display()
        );
    }
    // And the control: the rename actually happened, so the line above is not
    // two identical files nothing touched.
    let body = std::fs::read_to_string(&one_files[0]).unwrap();
    assert!(body.contains("pub struct LedgerRenamed;"), "{body}");
    assert!(!body.contains("pub struct Ledger;"), "{body}");
}

#[test]
fn the_batch_answers_what_each_pattern_did_and_what_each_file_now_is() {
    // The half of the change that is not the round trip. An answer a caller has
    // to follow with "and what changed?" has moved a call rather than removed
    // it, so every number a caller needs is asserted here by hand off the
    // fixture.
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let patterns = five_patterns(&files);
    let output = typed(
        tmp.path(),
        &["structured on", &batch_line(&patterns), "salir"],
    );
    let said = one_answer(&output, "edit");

    assert_eq!(said["did"], "substituted");
    assert_eq!(said["undo"], "attempt");
    // Three distinct files, not the eight (operation, file) pairs above them.
    assert_eq!(said["files"], 3, "{said}");
    // Two `store::Ledger::open`, one definition, one impl, two `(Ledger, usize)`
    // and one bare `Ledger::open`. Counted off the fixture.
    assert_eq!(said["replacements"], 7, "{said}");

    let operations = said["operations"].as_array().expect("operations");
    assert_eq!(operations.len(), 5);
    assert_eq!(operations[0]["old"], "store::Ledger::open");
    assert_eq!(operations[0]["replacements"], 2);
    assert_eq!(operations[0]["files"], 1);
    assert_eq!(operations[3]["old"], "(Ledger, usize)");
    assert_eq!(operations[3]["files"], 2);
    assert_eq!(operations[3]["replacements"], 2);
    // Where to look, without reading anything back.
    assert_eq!(operations[1]["in"][0]["first_line"], 1);

    // And the per-file half: `store/src/ledger.rs` is named by three of the five
    // patterns and appears once here, with the size it has now. Three places —
    // the definition, the impl and the one tuple type; `(Ledger, 0)` on the
    // line below it is a value and not a type, and no pattern here asks for it.
    let changed = said["changed"].as_array().expect("changed");
    assert_eq!(changed.len(), 3);
    let ledger = changed
        .iter()
        .find(|row| row["path"].as_str().unwrap().ends_with("ledger.rs"))
        .expect("the definition file");
    assert_eq!(ledger["replacements"], 3);
    assert_eq!(
        ledger["bytes"].as_u64().unwrap(),
        std::fs::metadata(&files[0]).unwrap().len(),
        "the size in the answer is not the size on disk"
    );
}

#[test]
fn one_pattern_that_matches_nothing_leaves_every_other_pattern_unwritten() {
    // **The property the batch's preflight exists for**, and the one a caller
    // cannot recover from on its own: four patterns applied and the fifth
    // refused would leave a workspace halfway through a rename, with nothing in
    // the answer saying which half.
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let before: Vec<String> = files
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect();

    let mut patterns = five_patterns(&files).to_vec();
    patterns[3].0 = "(Ledger, u128)".into();
    let output = typed(
        tmp.path(),
        &["structured on", &batch_line(&patterns), "salir"],
    );

    let said = one_answer(&output, "edit");
    assert_eq!(said["ok"], false, "{said}");
    assert_eq!(said["error"], "no_occurrences");
    assert_eq!(said["wrote"], false, "{said}");
    for (path, was) in files.iter().zip(&before) {
        assert_eq!(
            &std::fs::read_to_string(path).unwrap(),
            was,
            "{} was written by a batch that refused",
            path.display()
        );
    }
}

#[test]
fn a_batch_whose_second_pattern_eats_the_first_is_refused_before_anything_is_written() {
    // `A -> B` and then `B -> C`. Every `A` would quietly become a `C`, and
    // nothing in the call shows it. Refused with both strings named.
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let was = std::fs::read_to_string(&files[0]).unwrap();
    let ledger = files[0].display().to_string();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!(
                "editar '{ledger}' sustituir-lote 1 'Ledger' 'Book' 1 'Book' 'Tome' '{ledger}'"
            ),
            "salir",
        ],
    );
    let said = one_answer(&output, "edit");
    assert_eq!(said["ok"], false, "{said}");
    assert_eq!(said["error"], "chained_substitution", "{said}");
    assert_eq!(said["remedy"], "separate_the_calls");
    assert_eq!(said["wrote"], false);
    assert_eq!(std::fs::read_to_string(&files[0]).unwrap(), was);
}

#[test]
fn two_patterns_that_overlap_are_settled_by_the_order_and_not_refused() {
    // The control beside the refusal above, and the case a rename actually is:
    // `store::Ledger::open` contains `Ledger::open`, so the two overlap. They
    // are **not** a chain — neither writes what the other looks for — and
    // refusing them would refuse the shape this whole change exists for.
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let serve = files[1].display().to_string();
    let report = files[2].display().to_string();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!(
                "editar '{serve}' sustituir-lote 1 'store::Ledger::open' \
                 'store::LedgerRenamed::open' 1 'Ledger::open' 'LedgerRenamed::open' '{report}'"
            ),
            "salir",
        ],
    );
    let said = one_answer(&output, "edit");
    assert_eq!(said["ok"], true, "{said}");
    // The qualified file kept its qualified spelling and did not get the bare
    // rule applied to it a second time.
    let body = std::fs::read_to_string(&files[1]).unwrap();
    assert_eq!(
        body.matches("store::LedgerRenamed::open").count(),
        2,
        "{body}"
    );
    assert_eq!(body.matches("LedgerRenamedRenamed").count(), 0, "{body}");
}

#[test]
fn two_operations_looking_for_the_same_text_are_refused_rather_than_ordered() {
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let ledger = files[0].display().to_string();
    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!(
                "editar '{ledger}' sustituir-lote 1 'Ledger' 'Book' 1 'Ledger' 'Tome' '{ledger}'"
            ),
            "salir",
        ],
    );
    let said = one_answer(&output, "edit");
    assert_eq!(said["ok"], false, "{said}");
    assert_eq!(said["error"], "bad_batch", "{said}");
}

#[test]
fn the_same_file_named_twice_in_one_operation_is_refused_and_twice_across_two_is_not() {
    // Two halves of one rule, tested together because apart they look like the
    // same test. Inside one operation a repeated file is a caller that has lost
    // count. Across two operations it is the ordinary case — a rename touches
    // one file with several patterns — and the file is opened once and written
    // once, which is what stops the second save throwing the first away.
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let ledger = files[0].display().to_string();

    let twice_in_one = typed(
        tmp.path(),
        &[
            "structured on",
            // The same absolute path twice inside one operation. Refused on the
            // file's identity, so two *names* for one file would be refused the
            // same way — which is the check the single substitution already has
            // and the reason it has it.
            &format!("editar '{ledger}' sustituir-lote 2 'Ledger;' 'Book;' '{ledger}'"),
            "salir",
        ],
    );
    let said = one_answer(&twice_in_one, "edit");
    assert_eq!(said["ok"], false, "{said}");
    assert_eq!(said["error"], "repeated_path", "{said}");

    let across_two = typed(
        tmp.path(),
        &[
            "structured on",
            &format!(
                "editar '{ledger}' sustituir-lote 1 'pub struct Ledger;' \
                 'pub struct Book;' 1 'impl Ledger {{' 'impl Book {{' '{ledger}'"
            ),
            "salir",
        ],
    );
    let said = one_answer(&across_two, "edit");
    assert_eq!(said["ok"], true, "{said}");
    let body = std::fs::read_to_string(&files[0]).unwrap();
    // Both patterns are in the file. One save, not two, and neither lost.
    assert!(body.contains("pub struct Book;"), "{body}");
    assert!(body.contains("impl Book {"), "{body}");
}

#[test]
fn a_batch_asking_for_more_substitutions_than_the_ceiling_is_refused_by_the_count() {
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let ledger = files[0].display().to_string();
    let mut line = format!("editar '{ledger}' sustituir-lote");
    // Seventeen, where the ceiling is sixteen. Each looks for something
    // different, so nothing but the ceiling can refuse this.
    for n in 0..17 {
        line.push_str(&format!(" 1 'nothing{n}' 'something{n}'"));
        if n > 0 {
            line.push_str(&format!(" '{ledger}'"));
        }
    }
    let output = typed(tmp.path(), &["structured on", &line, "salir"]);
    let said = one_answer(&output, "edit");
    assert_eq!(said["ok"], false, "{said}");
    assert_eq!(said["error"], "too_much", "{said}");
    assert_eq!(said["asked"], 17);
    assert_eq!(said["most"], 16);
}

#[test]
fn the_ceiling_on_places_to_change_is_the_batch_and_not_each_operation_alone() {
    // The one ceiling a batch can cross that none of its operations can, and the
    // reason a batch has to be bounded as a whole: five patterns of five hundred
    // places each is two and a half thousand changes asked for in one call, and
    // every one of them is under the limit on its own.
    //
    // It is checked after the whole batch has been applied in memory and before
    // anything is saved, so the refusal costs the caller a corrected call and
    // not a reconstruction.
    let tmp = tempfile::tempdir().unwrap();
    let wide = tmp.path().join("wide.rs");
    let mut body = String::new();
    for n in 0..500 {
        body.push_str(&format!("let a{n} = 1; let b{n} = 2; let c{n} = 3;                                 let d{n} = 4; let e{n} = 5;\n"));
    }
    std::fs::write(&wide, &body).unwrap();
    let named = wide.display().to_string();

    let mut line = format!("editar '{named}' sustituir-lote");
    for (n, letter) in ["let a", "let b", "let c", "let d", "let e"]
        .iter()
        .enumerate()
    {
        line.push_str(&format!(" 1 '{letter}' 'const {n}x'"));
        if n > 0 {
            line.push_str(&format!(" '{named}'"));
        }
    }
    let output = typed(tmp.path(), &["structured on", &line, "salir"]);
    let said = one_answer(&output, "edit");
    assert_eq!(said["ok"], false, "{said}");
    assert_eq!(said["error"], "too_much", "{said}");
    assert_eq!(said["asked"], 2500, "{said}");
    assert_eq!(said["most"], 2000, "{said}");
    assert_eq!(said["wrote"], false, "{said}");
    // And nothing was written, which is what "before anything is saved" means.
    assert_eq!(std::fs::read_to_string(&wide).unwrap(), body);
}

#[test]
fn a_count_that_is_not_a_number_says_so_instead_of_being_read_as_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let ledger = files[0].display().to_string();
    for tail in [
        // A count that is a word.
        "many 'Ledger' 'Book'",
        // A count with no operation behind it.
        "3 'Ledger' 'Book'",
        // An operation that names no file at all.
        "0 'Ledger' 'Book'",
        // Nothing after the subverb.
        "",
    ] {
        let output = typed(
            tmp.path(),
            &[
                "structured on",
                &format!("editar '{ledger}' sustituir-lote {tail}"),
                "salir",
            ],
        );
        let said = one_answer(&output, "edit");
        assert_eq!(said["ok"], false, "`{tail}` was accepted: {said}");
        assert_eq!(said["error"], "bad_batch", "`{tail}`: {said}");
        assert_eq!(said["wrote"], false, "`{tail}`: {said}");
    }
}

#[test]
fn a_batch_that_climbs_out_of_the_workspace_is_refused_by_name() {
    // The check the external boundary cannot make for this subverb, because a
    // batch's positions mean nothing until its counts are read. It lives in the
    // verb instead, and this is the test that says it is still there.
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let ledger = files[0].display().to_string();
    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!(
                "editar '{ledger}' sustituir-lote 1 'Ledger' 'Book' 1 'a' 'b' '../outside.rs'"
            ),
            "salir",
        ],
    );
    let said = one_answer(&output, "edit");
    assert_eq!(said["ok"], false, "{said}");
    assert_eq!(said["error"], "bad_batch", "{said}");
    assert!(
        said["message"].as_str().unwrap().contains(".."),
        "the refusal does not name what was wrong: {said}"
    );
}

#[test]
fn a_rehearsed_batch_answers_the_same_shape_and_the_bytes_do_not_move() {
    let tmp = tempfile::tempdir().unwrap();
    let files = a_tree_that_needs_five_patterns(tmp.path());
    let before: Vec<String> = files
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect();
    let patterns = five_patterns(&files);
    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("ensayo {}", batch_line(&patterns)),
            "salir",
        ],
    );
    let said = one_answer(&output, "rehearse");
    assert_eq!(said["wrote"], false, "{said}");
    assert_eq!(said["would"], "substituted");
    assert_eq!(said["files"], 3);
    assert_eq!(said["replacements"], 7);
    for (path, was) in files.iter().zip(&before) {
        assert_eq!(&std::fs::read_to_string(path).unwrap(), was);
    }
}

#[test]
fn one_batch_answers_in_fewer_bytes_than_the_five_calls_it_replaces() {
    // §13's table, asserted rather than reported. The direction is the claim —
    // one answer instead of five, and smaller than the five together — and the
    // exact numbers are printed by the assertion when it fails, which is the
    // only time anybody needs them.
    let by_five = tempfile::tempdir().unwrap();
    let by_one = tempfile::tempdir().unwrap();
    let five_files = a_tree_that_needs_five_patterns(by_five.path());
    let one_files = a_tree_that_needs_five_patterns(by_one.path());

    let mut lines = vec!["structured on".to_string()];
    for (old, new, paths) in five_patterns(&five_files) {
        let mut line = format!("editar '{}' sustituir '{old}' '{new}'", paths[0]);
        for path in &paths[1..] {
            line.push_str(&format!(" '{path}'"));
        }
        lines.push(line);
    }
    lines.push("salir".to_string());
    let sequential = typed(
        by_five.path(),
        &lines.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let sequential_bytes: usize = answers_to(&sequential, "edit")
        .iter()
        .map(|said| said.to_string().len())
        .sum();

    let patterns = five_patterns(&one_files);
    let batched = typed(
        by_one.path(),
        &["structured on", &batch_line(&patterns), "salir"],
    );
    let batched_answers = answers_to(&batched, "edit");
    assert_eq!(batched_answers.len(), 1);
    let batched_bytes = batched_answers[0].to_string().len();

    assert!(
        batched_bytes < sequential_bytes,
        "one batch answered in {batched_bytes} bytes and five calls in {sequential_bytes}; \
         the batch is supposed to be the smaller of the two"
    );
}
