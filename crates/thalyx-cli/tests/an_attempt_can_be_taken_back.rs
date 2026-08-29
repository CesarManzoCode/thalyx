//! `intento`, driven from a real session.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **D2**, and the
//! sentence [[Filosofia-Fundacional]] uses for the advantage no other operating
//! system has: *«intenta esto y si sale mal deshazlo»*.
//!
//! ## What these prove and what they cannot
//!
//! The reasoning — which attempt is open, what a second one does, what an
//! abandon aims at, what happens when the snapshot is gone — is covered in
//! `thalyx_core::attempt` against the directory fake, which is the crate's own
//! split: *policy that can only be exercised on a Btrfs filesystem is policy
//! that is never exercised*.
//!
//! What is here is the half that has to be driven through a session: that the
//! verb is reachable, that both faces answer, and that where it must refuse it
//! refuses. That last one is not a lesser test, and 2026-08-10 is why.
//!
//! `without_a_subvolume_it_refuses_instead_of_copying_a_directory` passed here
//! and **failed on Cesar's machine** — because `intento empezar` did not refuse
//! there. It walked up from the scratch directory looking for a subvolume,
//! found `/`, and took a read-only snapshot of his entire root filesystem. The
//! answer said abandoning would delete 1,343,582 files, `/boot` among them.
//! Nothing was destroyed, because that test never abandons. What it destroyed
//! was the idea that a verb which can replace a whole subvolume may choose
//! which one by searching.
//!
//! The lesson for this file: a test that a refusal happens is only as good as
//! the machines it has run on. This container has no Btrfs and no subvolume
//! anywhere, so *every* path refuses here for the wrong reason, and the test
//! could not tell a correct refusal from an accident of the filesystem. The
//! guard itself is now unit-tested in `crate::attempt` against a fake where
//! everything is a subvolume, which is the machine where the dangerous answer
//! actually comes up.
//!
//! On Cesar's machine, `dev/verify.sh` stage 26 runs the other half — including
//! standing at `/` and requiring a refusal.

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

fn asked(root: &Path, at: &Path, line: &str) -> serde_json::Value {
    let output = piped(
        root,
        &[
            "structured on",
            &format!("cd {}", at.display()),
            line,
            "salir",
        ],
    );
    answer_to(&objects(&output), "attempt")
}

fn a_working_tree() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("a store");
    let work = root.path().join("work");
    std::fs::create_dir(&work).expect("the tree");
    std::fs::write(work.join("uno.txt"), "one").expect("a file");
    (root, work)
}

#[test]
fn a_machine_with_no_attempt_open_says_so_without_being_asked_twice() {
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, "intento");

    assert_eq!(answer["ok"], serde_json::json!(true));
    // `false` and not an absent key. A caller that had to infer "no attempt"
    // from a missing field is one that never wrote the branch.
    assert_eq!(answer["open"], serde_json::json!(false));
}

#[test]
fn the_words_that_abandon_in_one_call_are_not_refused_at_a_real_prompt() {
    // Not a test of what abandoning does — that needs Btrfs and is below — and
    // deliberately a weak one: `intento abandonar` alone answers `none_open`
    // too, so what this pins is only that the three extra words are **not**
    // refused as words this verb does not take. The test below is the one that
    // shows they are read at all, and the boundary that sits in front of them on
    // the agent's path is pinned in `external.rs`, which is a different layer
    // and a different test.
    let (root, work) = a_working_tree();
    let answer = asked(
        root.path(),
        &work,
        "intento abandonar snapshot=2026-08-29T11-04-02Z-rename delete=0 revert=3",
    );

    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(
        answer["error"],
        serde_json::json!("none_open"),
        "the one-call abandon was refused for its shape rather than answered: {answer}"
    );
}

#[test]
fn a_count_that_is_not_a_count_is_refused_at_a_real_prompt_too() {
    // The discriminating half. This answer can only be reached by the parser
    // having read `delete=`, so it is what says the words above arrive as words
    // this verb understands rather than as noise it ignores — and it comes back
    // before the attempt record is even looked at, which is why it is not
    // `none_open`.
    //
    // It is also the rule itself: a count this cannot read is refused *as that*,
    // never quietly dropped into "no claim was made". Dropped, it would answer
    // with the cost object, and read to the caller as the tree having changed
    // underneath it.
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, "intento abandonar delete=lots revert=3");

    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(
        answer["error"],
        serde_json::json!("bad_argument"),
        "{answer}"
    );
}

#[test]
fn without_a_subvolume_it_refuses_instead_of_copying_a_directory() {
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, "intento empezar refactor");

    // The claim: a copy is not a snapshot. It is not atomic, it takes time
    // proportional to the data, and something that took twenty minutes is a
    // picture of twenty minutes rather than of an instant. An implementation
    // that fell back to one would hand a caller a way back that is not there —
    // and the caller would find out only when it needed it.
    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("not_a_subvolume"));
    assert!(
        answer["message"]
            .as_str()
            .unwrap()
            .contains("no attempt was started"),
        "the refusal does not say nothing was started: {answer}"
    );
}

#[test]
fn a_refusal_to_start_leaves_nothing_open_behind_it() {
    let (root, work) = a_working_tree();
    let output = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "intento empezar refactor",
            "intento",
            "salir",
        ],
    );
    let all = objects(&output);
    let status = all
        .iter()
        .rfind(|value| value["op"] == serde_json::json!("attempt"))
        .expect("a status");

    // The half that would be worse than the refusal itself: an attempt written
    // down for a snapshot that was never taken is one that can never be
    // abandoned and blocks every following one.
    assert_eq!(status["open"], serde_json::json!(false), "{status}");
}

#[test]
fn settling_something_that_was_never_started_says_which_rather_than_succeeding() {
    let (root, work) = a_working_tree();

    for line in ["intento confirmar", "intento abandonar"] {
        let answer = asked(root.path(), &work, line);
        assert_eq!(answer["ok"], serde_json::json!(false), "{line}: {answer}");
        assert_eq!(
            answer["error"],
            serde_json::json!("none_open"),
            "{line}: {answer}"
        );
    }
}

#[test]
fn a_word_that_is_not_one_of_the_three_is_named_rather_than_guessed_at() {
    let (root, work) = a_working_tree();
    let answer = asked(root.path(), &work, "intento borrar-todo");

    // Guessing which of three consequential words was meant is not a service
    // anybody wants from the verb that can replace a whole subvolume.
    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("unknown_argument"));
}

#[test]
fn a_person_is_told_how_to_start_one_rather_than_only_that_none_is_open() {
    let (root, work) = a_working_tree();
    let output = piped(
        root.path(),
        &[&format!("cd {}", work.display()), "intento", "salir"],
    );
    let said = String::from_utf8_lossy(&output.stdout);

    assert!(
        said.contains("No attempt is open"),
        "the person was not told the state:\n{said}"
    );
    // The half `Principio-Doble-Ruta` is about: on the image there is no second
    // terminal and no manual, so a verb whose way in is not on screen is a verb
    // the person does not have.
    assert!(
        said.contains("intento empezar"),
        "the person was not told how to start one:\n{said}"
    );
}

#[test]
fn rehearsing_it_sends_the_caller_to_the_verb_that_already_answers_that() {
    let (root, work) = a_working_tree();
    let output = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "ensayo intento",
            "salir",
        ],
    );
    let answer = answer_to(&objects(&output), "rehearse");

    // A2 applied to a rehearsal: `intento` alone already says what abandoning
    // would cost, so refusing without naming that would send a caller looking
    // for something it already has.
    assert_eq!(answer["ok"], serde_json::json!(false));
    assert_eq!(answer["error"], serde_json::json!("ask_attempt_itself"));
}

/// A throwaway subvolume on a real Btrfs filesystem, or nothing.
///
/// The same shape `thalyx-snapshot`'s tests use, and the same variable, so a
/// machine that can prove one of them can prove both without being configured
/// twice.
fn btrfs_scratch(label: &str) -> Option<std::path::PathBuf> {
    let base = std::env::var("THALYX_BTRFS_SCRATCH").ok()?;
    // The name carries the caller's label and not only the pid, and rule 11 is
    // why: `cargo test` runs every test in this file as a **thread of one
    // process**, so a name made of the pid alone is one name shared by all of
    // them. On 2026-08-29 that turned one subvolume into two failures on Cesar's
    // machine: whichever test got there second deleted the tree the first was
    // working in and then could not create what was already there, so it
    // reported "no Btrfs subvolume could be made" on a machine that has one.
    //
    // The label was not enough, and the second half of the same day says why: an
    // attempt snapshots the work tree, `thalyx_snapshot::Snapshots` puts snapshots
    // in the source's **parent**, and a subvolume made directly in
    // `THALYX_BTRFS_SCRATCH` therefore snapshots into one
    // `THALYX_BTRFS_SCRATCH/.thalyx-snapshots` shared with `thalyx-snapshot`'s own
    // Btrfs tests — which `cargo test` runs as a separate binary at the same time,
    // and one of which used to remove that directory outright. So each test gets a
    // private root and the work tree lives inside it.
    let root = Path::new(&base).join(format!("thalyx-{label}-{}", std::process::id()));
    // Nothing is deleted first: a path this helper did not make is not a path it
    // may remove, and `create_dir` refusing an existing one is the guarantee.
    std::fs::create_dir(&root).ok()?;

    let subvolume = root.join("work");
    let made = std::process::Command::new("btrfs")
        .args(["subvolume", "create"])
        .arg(&subvolume)
        .output()
        .ok()?;
    made.status.success().then_some(subvolume)
}

/// Take away the arena [`btrfs_scratch`] made, and nothing outside it.
///
/// The snapshots go one at a time and by `btrfs subvolume delete`, because
/// `remove_dir_all` cannot take a read-only subvolume away: it begins by
/// unlinking the files inside.
fn discard(work: &Path) {
    let root = work.parent().expect("the arena holding the work tree");
    if let Ok(entries) = std::fs::read_dir(root.join(thalyx_snapshot::SNAPSHOT_DIR)) {
        for entry in entries.flatten() {
            let _ = std::process::Command::new("btrfs")
                .args(["subvolume", "delete"])
                .arg(entry.path())
                .output();
        }
    }
    let _ = std::process::Command::new("btrfs")
        .args(["subvolume", "delete"])
        .arg(work)
        .output();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn abandoning_an_attempt_takes_back_a_substitution_across_files_byte_for_byte() {
    // The whole of the `reversible` benchmark task, in one session and with no
    // agent: open an attempt, make the mechanical change in one call, take it
    // back. It is here rather than in the substitution's own test file because
    // it is the one claim about substitution that this container cannot check —
    // an attempt needs a Btrfs subvolume, and there is none here.
    //
    // `THALYX_REQUIRE_BTRFS_TESTS=1` turns the skip into a failure, which is
    // what `dev/verify.sh` sets on the machine that has one.
    let Some(work) = btrfs_scratch("substitute") else {
        assert!(
            std::env::var("THALYX_REQUIRE_BTRFS_TESTS").is_err(),
            "THALYX_REQUIRE_BTRFS_TESTS is set and no Btrfs subvolume could be made"
        );
        eprintln!(
            "NOT PROVEN: abandoning an attempt was never shown to take back a substitution. \
             It needs a writable Btrfs filesystem (THALYX_BTRFS_SCRATCH=<path on btrfs>) \
             and btrfs-progs."
        );
        return;
    };
    let root = tempfile::tempdir().expect("a store");

    std::fs::create_dir_all(work.join("src")).expect("a directory");
    let one = work.join("src/slots.rs");
    let two = work.join("src/run.rs");
    std::fs::write(&one, "pub struct SlotTable;\nimpl SlotTable {}\n").expect("a file");
    std::fs::write(&two, "use crate::SlotTable;\n").expect("a file");
    let before = (
        std::fs::read(&one).expect("a file"),
        std::fs::read(&two).expect("a file"),
    );

    let output = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "intento empezar rename",
            &format!(
                "editar {} sustituir SlotTable SlotTableRenamed {}",
                one.display(),
                two.display()
            ),
            "intento abandonar si",
            "salir",
        ],
    );
    let said = objects(&output);

    // The baseline. Without it, a substitution that never happened and one that
    // was taken back leave the same tree, and this would pass on a machine
    // where the edit was refused.
    //
    // The counts are the fixture's own, read off a real answer: two `SlotTable`
    // in `slots.rs`, one in `run.rs`. The `4` that stood here until 2026-08-29
    // was arithmetic nobody had run — this test only runs where there is a
    // Btrfs, so the first machine to reach the assertion was the one that
    // reported it wrong. And `files` is here because *across files* is the
    // claim the name makes: a substitution that only ever touched `slots.rs`
    // gives back the same tree just as well.
    let edited = answer_to(&said, "edit");
    assert_eq!(edited["ok"], serde_json::json!(true), "{edited}");
    assert_eq!(edited["files"], serde_json::json!(2), "{edited}");
    assert_eq!(edited["replacements"], serde_json::json!(3), "{edited}");

    assert_eq!(
        (
            std::fs::read(&one).expect("a file"),
            std::fs::read(&two).expect("a file")
        ),
        before,
        "abandoning the attempt did not put the substitution back"
    );

    discard(&work);
}

#[test]
fn abandoning_an_attempt_takes_back_a_whole_batch_and_not_only_its_last_operation() {
    // The same claim one level up, and it is not implied by the one above. A
    // batch writes each file **once**, with every operation already in it, so a
    // snapshot taken before it holds a state no single operation ever produced —
    // and "the last substitution was taken back" and "all five were" are two
    // different facts about that. The one this container cannot check.
    //
    // `THALYX_REQUIRE_BTRFS_TESTS=1` turns the skip into a failure, which is
    // what `dev/verify.sh` sets on the machine that has a Btrfs to do it on.
    let Some(work) = btrfs_scratch("substitute-batch") else {
        assert!(
            std::env::var("THALYX_REQUIRE_BTRFS_TESTS").is_err(),
            "THALYX_REQUIRE_BTRFS_TESTS is set and no Btrfs subvolume could be made"
        );
        eprintln!(
            "NOT PROVEN: abandoning an attempt was never shown to take back a batch of \
             substitutions. It needs a writable Btrfs filesystem \
             (THALYX_BTRFS_SCRATCH=<path on btrfs>) and btrfs-progs."
        );
        return;
    };
    let root = tempfile::tempdir().expect("a store");

    std::fs::create_dir_all(work.join("src")).expect("a directory");
    let one = work.join("src/slots.rs");
    let two = work.join("src/run.rs");
    std::fs::write(
        &one,
        "pub struct SlotTable;\nimpl SlotTable {\n    fn open() -> (SlotTable, usize) {          (SlotTable, 0) }\n}\n",
    )
    .expect("a file");
    std::fs::write(
        &two,
        "use crate::slots::SlotTable;\nfn go(t: (SlotTable, usize)) {}\n",
    )
    .expect("a file");
    let before = (
        std::fs::read(&one).expect("a file"),
        std::fs::read(&two).expect("a file"),
    );

    let output = piped(
        root.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "intento empezar rename",
            // Only the **first** operation borrows the file named before the
            // subverb; every one after it lists its own files in full. The line
            // that stood here until 2026-08-29 named `one` once and let the
            // second and third operations inherit it, so the machine read
            // `'(SlotTable, usize)'` as operation 2's file name and refused the
            // whole batch with `bad_batch`. Nobody saw it because the test
            // never reached this line: it needs a Btrfs, and the machine that
            // has one was failing to make the subvolume first.
            &format!(
                "editar {one} sustituir-lote 1 'pub struct SlotTable;' 'pub struct Table;' \
                 1 'impl SlotTable {{' 'impl Table {{' {one} \
                 2 '(SlotTable, usize)' '(Table, usize)' {one} {two}",
                one = one.display(),
                two = two.display()
            ),
            "intento abandonar si",
            "salir",
        ],
    );
    let said = objects(&output);

    // The baseline, and here it is doing more work than usual: without it a
    // batch the machine refused and a batch it took back leave the same two
    // files, and this would pass on a machine where the grammar was broken.
    let edited = answer_to(&said, "edit");
    assert_eq!(edited["ok"], serde_json::json!(true), "{edited}");
    assert_eq!(edited["files"], serde_json::json!(2), "{edited}");
    assert_eq!(edited["replacements"], serde_json::json!(4), "{edited}");
    assert_eq!(
        edited["operations"].as_array().map(Vec::len),
        Some(3),
        "{edited}"
    );

    assert_eq!(
        (
            std::fs::read(&one).expect("a file"),
            std::fs::read(&two).expect("a file")
        ),
        before,
        "abandoning the attempt did not put the whole batch back"
    );

    discard(&work);
}
