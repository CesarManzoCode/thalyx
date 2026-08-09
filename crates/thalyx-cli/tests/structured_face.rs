//! The structured face, driven the way a program would drive it.
//!
//! `vault/01-Filosofia/Filosofia-Fundacional.md` decrees that the objective is
//! an LLM working better here than anywhere else, and that every thing is born
//! with two faces — the human one and a structured one a program can ask for and
//! parse. The operations have returned facts rather than printing since
//! 2026-08-09; what was missing until this file existed was anything that could
//! ask for one.
//!
//! ## Why these are end-to-end and not unit tests
//!
//! Rule 1 of `Estrategia-de-Pruebas.md`: **every real defect came from running
//! the system.** The shape of one object is already covered by unit tests in
//! `thalyx-files::machine`, and those would all pass while the face was
//! unreachable — which is precisely the failure this project has already had
//! once, when installed modules were unexecutable for weeks with every test
//! green.
//!
//! So these type at the prompt, through a real terminal, and parse what comes
//! back with a JSON parser rather than with `grep`. A test that searched the
//! output for a substring would pass on a line that is not an object at all.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// Type `lines` at the session prompt and keep everything it said.
///
/// Through `thalyx dev pty`, because parts of the session refuse a stdin that
/// is not a terminal — silence is not consent — and a program driving Thalyx on
/// a real machine has one.
fn at_the_prompt(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .args(["dev", "pty", "--", thalyx(), "session"])
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session, on a terminal of Thalyx's own making");

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
        .expect("typing at the prompt");

    child.wait_with_output().expect("waiting for the session")
}

/// Every line of the output that parses as a JSON object, in order.
///
/// The banner is printed before anything is typed and is prose, so this is not
/// "all the output" — it is the answers. A program reading the stream does the
/// same thing: it turns the face on and reads objects from there.
fn objects(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .replace('\r', "")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(|value| value.is_object())
        .collect()
}

fn answer_to<'a>(objects: &'a [serde_json::Value], op: &str) -> &'a serde_json::Value {
    objects
        .iter()
        .find(|value| value["op"] == serde_json::json!(op))
        .unwrap_or_else(|| panic!("nothing answered `{op}`; got {objects:#?}"))
}

/// The same, down a plain pipe with no terminal anywhere.
///
/// This is how a program actually drives Thalyx, and it is a different stream
/// from the pty one: with no terminal there is no echo, so every line after the
/// face is on is an answer. The pty version above is what a person's session
/// looks like; both have to work and they fail differently.
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

/// A machine with a home directory holding things worth asking about.
fn a_home_with_things_in_it() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a store");
    std::fs::write(root.path().join("notas.txt"), "hola\n").expect("a file");
    std::fs::write(root.path().join(".oculto"), "xx").expect("a hidden file");
    std::fs::create_dir(root.path().join("Documentos")).expect("a folder");
    root
}

/// `cd` into the fixture, since a session always starts in `/home`.
fn inside(place: &Path) -> String {
    format!("cd {}", place.display())
}

// ───────────────────────────────────────────────── the face can be asked for at all

#[test]
fn a_program_can_ask_for_the_structured_face_and_is_answered_in_it() {
    let home = a_home_with_things_in_it();
    let output = at_the_prompt(home.path(), &["structured on", "pwd", "salir"]);
    let objects = objects(&output);

    // The whole point of 4b. Before this, every one of these operations returned
    // a fact and the only thing that ever read one was the human printer, so the
    // decree was written down and not built.
    assert!(
        !objects.is_empty(),
        "the session answered nothing a program can parse:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let acknowledged = answer_to(&objects, "structured");
    assert_eq!(acknowledged["structured"], serde_json::json!(true));
    // The way out travels with the acknowledgement: on the image there is no
    // second terminal, so a mode with no visible exit can strand somebody.
    assert_eq!(acknowledged["off"], serde_json::json!("structured off"));
}

#[test]
fn the_face_can_be_turned_back_off_and_the_sentences_return() {
    let home = a_home_with_things_in_it();
    let output = at_the_prompt(
        home.path(),
        &["structured on", "structured off", "pwd", "salir"],
    );

    let said = String::from_utf8_lossy(&output.stdout).replace('\r', "");
    // The human half of the decree: the person keeps everything, and a session
    // that could not be brought back would be taking it away.
    assert!(
        said.contains("Answers are for a person again"),
        "no way back out of the structured face:\n{said}"
    );
    // And `pwd` after it is prose, not an object.
    let after = said.split("Answers are for a person again").nth(1).unwrap();
    assert!(
        !after.lines().any(|line| line.trim().starts_with('{')),
        "still answering in JSON after being told not to:\n{after}"
    );
}

#[test]
fn a_person_who_never_asked_is_never_handed_json() {
    let home = a_home_with_things_in_it();
    let output = at_the_prompt(home.path(), &[&inside(home.path()), "ls", "pwd", "salir"]);

    // The decree requires the human keep everything they have in Linux, and
    // being unable to read the answers is not keeping it. A default that leaked
    // the structured face would break that for every existing user at once.
    assert!(
        objects(&output).is_empty(),
        "a session nobody asked answered in JSON:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn down_a_pipe_every_line_after_the_switch_is_an_answer_and_nothing_else() {
    let home = a_home_with_things_in_it();
    let output = piped(
        home.path(),
        &["structured on", &inside(home.path()), "ls", "pwd", "salir"],
    );
    let said = String::from_utf8_lossy(&output.stdout).replace('\r', "");

    // Everything from the acknowledgement to the goodbye. The banner above it is
    // prose printed before anything was typed, and is not an answer to anything.
    let stream = said
        .split_once("{\"off\"")
        .map(|(_, rest)| format!("{{\"off\"{rest}"))
        .unwrap_or_else(|| panic!("the face never came on:\n{said}"));

    for line in stream.lines() {
        if line.trim().is_empty() || line.contains("Back to") {
            continue;
        }
        // The defect this caught, and it was the acknowledgement itself: the
        // human prompt for the switching command had already been printed
        // without a newline, so the first object landed as
        // `  /home > {"op":"structured",…` — the one line a caller must parse to
        // know the mode is on was the one line that would not parse.
        serde_json::from_str::<serde_json::Value>(line.trim()).unwrap_or_else(|error| {
            panic!("{line:?} is not an object: {error}\n\nwhole stream:\n{stream}")
        });
    }
}

// ─────────────────────────────────────── what the structured face refuses to withhold

#[test]
fn the_structured_listing_shows_a_hidden_name_the_person_is_not_shown() {
    let home = a_home_with_things_in_it();

    let human = at_the_prompt(home.path(), &[&inside(home.path()), "ls", "salir"]);
    let human_said = String::from_utf8_lossy(&human.stdout).replace('\r', "");
    assert!(
        !human_said.contains(".oculto"),
        "the human listing stopped hiding dotfiles:\n{human_said}"
    );

    let machine = at_the_prompt(
        home.path(),
        &["structured on", &inside(home.path()), "ls", "salir"],
    );
    let objects = objects(&machine);
    let listing = answer_to(&objects, "list");
    let names: Vec<&str> = listing["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["name"].as_str().expect("a name"))
        .collect();

    // The tie-break rule of the decree, end to end: when the two faces disagree
    // the LLM wins, and hiding something from a program that asked is taking
    // capability away. Both halves in one test because either alone would pass
    // while the property was absent.
    assert!(
        names.contains(&".oculto"),
        "the structured face hid a name from something that asked: {names:?}"
    );
    assert!(names.contains(&"notas.txt"), "{names:?}");
}

#[test]
fn a_flag_that_shapes_the_human_listing_does_not_change_the_structured_one() {
    let home = a_home_with_things_in_it();
    let with = at_the_prompt(
        home.path(),
        &["structured on", &inside(home.path()), "ls -la", "salir"],
    );
    let without = at_the_prompt(
        home.path(),
        &["structured on", &inside(home.path()), "ls", "salir"],
    );

    // `-a` and `-l` are about how much of the truth reaches a person. To a
    // program they are neither an error nor a change, because it was never
    // being given less.
    assert_eq!(
        answer_to(&objects(&with), "list"),
        answer_to(&objects(&without), "list"),
    );
}

#[test]
fn sizes_arrive_exact_where_the_person_is_shown_a_rounded_one() {
    let home = tempfile::tempdir().expect("a store");
    std::fs::write(home.path().join("big"), vec![b'x'; 1536]).expect("a file");

    let output = at_the_prompt(
        home.path(),
        &["structured on", &inside(home.path()), "ls", "salir"],
    );
    let objects = objects(&output);
    let entry = answer_to(&objects, "list")["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["name"] == serde_json::json!("big"))
        .expect("the file")
        .clone();

    // A person is shown `1.5 kB`. Two programs comparing two rounded numbers
    // compare two lies.
    assert_eq!(entry["bytes"], serde_json::json!(1536));
}

// ──────────────────────────────────────────── one typed line, exactly one object

#[test]
fn a_line_that_touches_three_files_is_still_one_answer() {
    let home = tempfile::tempdir().expect("a store");
    for name in ["uno.log", "dos.log", "tres.log"] {
        std::fs::write(home.path().join(name), "x").expect("a file");
    }

    let output = at_the_prompt(
        home.path(),
        &["structured on", &inside(home.path()), "rm *.log", "salir"],
    );
    let objects = objects(&output);
    let removals: Vec<&serde_json::Value> = objects
        .iter()
        .filter(|value| value["op"] == serde_json::json!("remove"))
        .collect();

    // The failure this prevents is the one the agent's prompt marker already
    // taught this project once: a boundary defined on one side only is not a
    // boundary. Three objects for one line, with no count anywhere, would leave
    // a caller reading the third as the answer to its next command.
    assert_eq!(removals.len(), 1, "got {removals:#?}");
    assert_eq!(removals[0]["count"], serde_json::json!(3));
    assert_eq!(removals[0]["ok"], serde_json::json!(true));

    // And against the disk, from outside the session that claimed it — asking
    // the system whether it worked proves nothing.
    for name in ["uno.log", "dos.log", "tres.log"] {
        assert!(
            !home.path().join(name).exists(),
            "{name} is still there after the session said it removed it"
        );
    }
}

#[test]
fn one_target_failing_out_of_several_does_not_report_the_line_as_ok() {
    let home = tempfile::tempdir().expect("a store");
    std::fs::write(home.path().join("real"), "x").expect("a file");

    let output = at_the_prompt(
        home.path(),
        &[
            "structured on",
            &inside(home.path()),
            "rm real imaginario",
            "salir",
        ],
    );
    let objects = objects(&output);
    let removal = answer_to(&objects, "remove");

    // A partial success reported as a success is how a caller moves on
    // believing a loop finished.
    assert_eq!(removal["ok"], serde_json::json!(false));
    assert_eq!(removal["count"], serde_json::json!(2));
    let results = removal["results"].as_array().expect("results");
    assert_eq!(results[0]["ok"], serde_json::json!(true));
    assert_eq!(results[1]["ok"], serde_json::json!(false));
    assert_eq!(results[1]["error"], serde_json::json!("absent"));
}

// ──────────────────────────────────────────────────── silence is never an answer

#[test]
fn moving_answers_a_program_where_it_says_nothing_to_a_person() {
    let home = a_home_with_things_in_it();
    let output = at_the_prompt(
        home.path(),
        &[
            "structured on",
            &inside(home.path()),
            "cd Documentos",
            "salir",
        ],
    );
    let objects = objects(&output);
    let moved = objects
        .iter()
        .rfind(|value| value["op"] == serde_json::json!("go"))
        .expect("nothing answered the move");

    // `cd` prints nothing to a person because the next prompt already says where
    // they are. A parser has no prompt to read it off, and cannot tell a silence
    // that means "moved" from one that means the session stopped.
    assert_eq!(moved["ok"], serde_json::json!(true));
    assert!(
        moved["path"]
            .as_str()
            .expect("a path")
            .ends_with("Documentos"),
        "got {moved}"
    );
}

#[test]
fn a_move_that_failed_says_where_the_session_still_is() {
    let home = a_home_with_things_in_it();
    let output = at_the_prompt(
        home.path(),
        &[
            "structured on",
            &inside(home.path()),
            "cd no-existe",
            "salir",
        ],
    );
    let objects = objects(&output);
    let refused = objects
        .iter()
        .rfind(|value| {
            value["op"] == serde_json::json!("go") && value["ok"] == serde_json::json!(false)
        })
        .expect("nothing answered the failed move");

    // Without this a program aims its next command at a place it is not — which
    // is exactly why the human face prints "You are still in …".
    assert_eq!(refused["error"], serde_json::json!("absent"));
    assert_eq!(
        refused["still_at"],
        serde_json::json!(home.path().display().to_string())
    );
}

#[test]
fn a_verb_typed_with_nothing_after_it_still_answers() {
    let home = a_home_with_things_in_it();
    let output = at_the_prompt(home.path(), &["structured on", "rm", "cat", "salir"]);
    let objects = objects(&output);

    // These never reach the filesystem, so there is no file error to report. A
    // program that got silence here would wait forever for an answer that was
    // never coming — the hang is the failure, not the refusal.
    let removal = answer_to(&objects, "remove");
    assert_eq!(removal["ok"], serde_json::json!(false));
    assert_eq!(removal["error"], serde_json::json!("incomplete"));

    let read = answer_to(&objects, "read");
    assert_eq!(read["ok"], serde_json::json!(false));
    assert_eq!(read["error"], serde_json::json!("incomplete"));
}

/// The same claim about the human face, and it lives here because this is what
/// found it.
///
/// Every verb arm matches on a **trailing space**, so `rm` typed alone was not a
/// verb at all: it fell through to "I have no model loaded". Five verbs had it,
/// and it is the same defect Cesar hit with `clear` on the first real session.
///
/// It took the structured face to surface it. A person seeing a paragraph about
/// the agent reads it as the machine being odd and types something else; a test
/// that demanded an answer had nowhere to put that.
#[test]
fn a_verb_typed_alone_is_a_verb_and_not_a_sentence_for_the_agent() {
    let home = a_home_with_things_in_it();
    let output = at_the_prompt(home.path(), &["rm", "mkdir", "touch", "cp", "mv", "salir"]);
    let said = String::from_utf8_lossy(&output.stdout).replace('\r', "");

    assert!(
        !said.contains("no model"),
        "a bare verb fell through to the agent:\n{said}"
    );
    // The control: it has to have answered *something*, or a session that
    // silently ignored all five would pass the assertion above.
    assert_eq!(
        said.matches("Which one").count(),
        3,
        "mkdir, touch and rm each owe a `which one`:\n{said}"
    );
    assert_eq!(
        said.matches("Two names").count(),
        2,
        "cp and mv each owe their own hint:\n{said}"
    );
}

// ────────────────────────────────────────────────────── facts, checked against disk

#[test]
fn what_a_copy_reports_is_what_is_on_the_disk_afterwards() {
    let home = tempfile::tempdir().expect("a store");
    std::fs::write(home.path().join("origen"), "doce bytes").expect("a file");

    let output = at_the_prompt(
        home.path(),
        &[
            "structured on",
            &inside(home.path()),
            "cp origen destino",
            "salir",
        ],
    );
    let objects = objects(&output);
    let copy = answer_to(&objects, "copy");
    let result = &copy["results"][0];

    assert_eq!(result["did"], serde_json::json!("copied"));

    // Checked against the disk from outside, because asking the system whether
    // it worked proves nothing. The byte count is the part worth checking: it is
    // what a caller would otherwise have to re-list the directory to learn.
    let landed = std::fs::metadata(home.path().join("destino")).expect("the copy");
    assert_eq!(result["bytes"], serde_json::json!(landed.len()));
    assert_eq!(
        result["to"],
        serde_json::json!(home.path().join("destino").display().to_string())
    );
}

#[test]
fn a_file_that_is_not_there_arrives_as_a_word_and_not_only_a_sentence() {
    let home = a_home_with_things_in_it();
    let output = at_the_prompt(
        home.path(),
        &[
            "structured on",
            &inside(home.path()),
            "cat fantasma",
            "salir",
        ],
    );
    let objects = objects(&output);
    let read = answer_to(&objects, "read");

    // The word is the contract; the sentence is English that will be reworded,
    // and a caller matching on it breaks the first time somebody improves it.
    assert_eq!(read["ok"], serde_json::json!(false));
    assert_eq!(read["error"], serde_json::json!("absent"));
    assert!(
        read["message"]
            .as_str()
            .expect("a message")
            .contains("fantasma")
    );
}

#[test]
fn a_files_contents_come_through_a_parser_and_not_through_the_layout() {
    let home = tempfile::tempdir().expect("a store");
    // Quotes and a newline, which is what would end a value early in any format
    // that is not escaped — and an ñ, because the terminal work already proved
    // this codebase has to carry more than ASCII.
    std::fs::write(
        home.path().join("dicho.txt"),
        "dijo \"hola\"\ny se fue — ñ\n",
    )
    .expect("a file");

    let output = at_the_prompt(
        home.path(),
        &[
            "structured on",
            &inside(home.path()),
            "cat dicho.txt",
            "salir",
        ],
    );
    let objects = objects(&output);
    let read = answer_to(&objects, "read");

    assert_eq!(read["ok"], serde_json::json!(true));
    assert_eq!(
        read["text"],
        serde_json::json!("dijo \"hola\"\ny se fue — ñ\n")
    );
    assert_eq!(read["truncated"], serde_json::json!(false));
}
