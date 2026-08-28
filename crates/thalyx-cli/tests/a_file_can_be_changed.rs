//! Text in a file can be changed, by a person and by a program, and the file on
//! disk is what changed.
//!
//! **Rule 1**: every real defect in this project came from running the system,
//! not from reading it. So nothing here inspects `thalyx-edit`. Every test types
//! at the real prompt of the real binary and then reads the file with something
//! that is not Thalyx — because "the editor reported that it saved" and "the
//! bytes on disk changed" are two claims, and installed modules were
//! unexecutable for weeks while every test of the first kind passed.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// Type these lines at a session and give back everything it said.
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

/// The structured answers, one per line, already parsed.
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

#[test]
fn a_program_changes_a_line_and_the_bytes_on_disk_are_the_ones_it_asked_for() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("thalyx.conf");
    std::fs::write(&file, "uno\ndos\ntres\n").unwrap();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("editar {} cambiar 2 DOS", file.display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "edit");
    assert_eq!(said["ok"], true);
    assert_eq!(said["did"], "replaced");
    assert_eq!(said["lines_after"], 3);

    // Read with something that is not Thalyx. This is the assertion that
    // matters and the one a test of the return value cannot make.
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "uno\nDOS\ntres\n");
    assert_eq!(
        said["bytes"].as_u64().unwrap(),
        std::fs::metadata(&file).unwrap().len(),
        "the size it reported is the size it produced"
    );
}

#[test]
fn a_line_the_file_does_not_have_is_refused_and_the_file_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("short.conf");
    std::fs::write(&file, "uno\ndos\n").unwrap();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("editar {} cambiar 40 nada", file.display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "edit");
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"], "no_such_line");
    // The count is in the refusal, so the caller can fix its own address
    // without reading the file again. That is punto A2 and it is the whole
    // reason the field is there.
    assert_eq!(said["lines"], 2);
    assert_eq!(said["asked"], 40);

    assert_eq!(std::fs::read_to_string(&file).unwrap(), "uno\ndos\n");
}

#[test]
fn a_program_that_asks_for_the_screen_is_told_there_is_none_instead_of_waiting() {
    // The failure this prevents is the worst kind: a session down a pipe that
    // opened a screen editor would sit there forever, and the caller would see
    // a hang with no message. `no_screen` is an answer, and it names the way
    // out.
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notas.txt");
    std::fs::write(&file, "uno\n").unwrap();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("editar {}", file.display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "edit");
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"], "no_screen");
    assert_eq!(said["remedy"], "address_lines");
}

#[test]
fn a_binary_file_is_refused_rather_than_opened_and_saved_back_mangled() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("thing.bin");
    let original: Vec<u8> = vec![0x7f, b'E', b'L', b'F', 0, 0, 1, 2, 3];
    std::fs::write(&file, &original).unwrap();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("editar {} cambiar 1 texto", file.display()),
            "salir",
        ],
    );

    assert_eq!(answer_to(&output, "edit")["error"], "not_text");
    assert_eq!(
        std::fs::read(&file).unwrap(),
        original,
        "the file it refused must be byte-for-byte what it was"
    );
}

#[test]
fn a_person_is_answered_in_sentences_and_a_program_in_one_object() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notas.txt");
    std::fs::write(&file, "uno\ndos\n").unwrap();

    let human = typed(
        tmp.path(),
        &[&format!("editar {} ver", file.display()), "salir"],
    );
    let said = String::from_utf8_lossy(&human.stdout);
    assert!(said.contains("uno"), "a person sees the text: {said}");
    assert!(
        !said.lines().any(|line| line.starts_with('{')),
        "and no JSON leaks into it"
    );

    let program = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("editar {} ver", file.display()),
            "salir",
        ],
    );
    let shown = answer_to(&program, "edit_show");
    assert_eq!(shown["lines"], 2);
    assert_eq!(shown["rows"][0]["text"], "uno");
    // One typed line, exactly one object — even though two lines of the file
    // came back inside it.
    assert_eq!(
        objects(&program)
            .iter()
            .filter(|v| v["op"] == "edit_show")
            .count(),
        1
    );
}

#[test]
fn a_whole_block_goes_in_with_one_command_and_the_line_numbers_say_where() {
    // A typed line cannot contain a line break, so without `\n` meaning one the
    // structured face could only ever add a line at a time — five calls, four
    // of them leaving the file in a state nobody asked for, each one saved.
    // That is the double route broken, so the escape is not a convenience.
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("lista.txt");
    std::fs::write(&file, "primero\núltimo\n").unwrap();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("editar {} poner 2 a\\nb\\nc", file.display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "edit");
    assert_eq!(said["ok"], true);
    assert_eq!(said["lines_after"], 5);
    assert_eq!(said["at"], "2-4");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "primero\na\nb\nc\núltimo\n"
    );
}

#[test]
fn the_accented_text_a_spanish_machine_is_used_in_survives_a_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("acentos.txt");
    std::fs::write(&file, "contraseña\nmañana\n").unwrap();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("editar {} cambiar 1 año", file.display()),
            &format!("editar {} ver", file.display()),
            "salir",
        ],
    );

    assert_eq!(answer_to(&output, "edit")["ok"], true);
    let shown = answer_to(&output, "edit_show");
    assert_eq!(shown["rows"][0]["text"], "año");
    assert_eq!(shown["rows"][1]["text"], "mañana");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "año\nmañana\n");
}

#[test]
fn a_misspelt_action_is_not_reported_as_a_missing_file() {
    // The two failures look identical to a caller that only checks `ok`, and
    // one of them sends it looking at its filesystem for a file that is fine.
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notas.txt");
    std::fs::write(&file, "uno\n").unwrap();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("editar {} cambair 1 x", file.display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "edit");
    assert_eq!(said["error"], "unknown_action");
    assert_eq!(said["asked"], "cambair");
}

// ──────────────────────────────────────────────────────────────── the screen
//
// The half that looked untestable. It is not: `thalyx dev pty` supplies a real
// terminal, so the screen editor can be driven with the bytes a keyboard sends
// and the file it wrote read back with something that is not Thalyx. Rule 1
// again — a screen editor nobody ever ran is a screen editor nobody has checked,
// and the visual half is exactly where "it looked right" substitutes for proof.

/// How long a screen editor may take before it counts as stuck.
///
/// It exists because this test hung the first time it was written — the pty had
/// no window size, the editor correctly refused to draw on a screen of no rows,
/// and the keystrokes meant for it were typed at the prompt instead, so `salir`
/// never arrived and nothing ever ended the session. `cargo test` sat there
/// naming nothing.
///
/// That is the failure of 2026-08-10 in miniature and the rule it produced: a
/// hang that names nothing costs a run and teaches nothing. This one kills the
/// child and reports what it had drawn by then.
const PATIENCE: Duration = Duration::from_secs(60);

/// Drive a session through a real terminal, sending `keys` verbatim.
fn at_a_keyboard(root: &Path, keys: &str) -> Output {
    let mut child = Command::new(thalyx())
        .args(["dev", "pty", "--", thalyx(), "session"])
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("thalyx dev pty");

    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(keys.as_bytes())
        .expect("typing");

    // Drained on its own thread. Waiting first and reading after is how a child
    // that fills the pipe buffer deadlocks against the test that is waiting for
    // it to exit.
    let drawn = Arc::new(Mutex::new(Vec::new()));
    let reading = {
        let drawn = Arc::clone(&drawn);
        let mut out = child.stdout.take().expect("stdout");
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = std::io::Read::read_to_end(&mut out, &mut buffer);
            *drawn.lock().expect("what was drawn") = buffer;
        })
    };

    let began = Instant::now();
    loop {
        match child.try_wait().expect("asking whether it finished") {
            Some(status) => {
                let _ = reading.join();
                return Output {
                    status,
                    stdout: drawn.lock().expect("what was drawn").clone(),
                    stderr: Vec::new(),
                };
            }
            None if began.elapsed() > PATIENCE => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reading.join();
                panic!(
                    "the editor stopped answering after {PATIENCE:?}. What it had \
                     drawn by then:\n{}",
                    String::from_utf8_lossy(&drawn.lock().expect("what was drawn"))
                );
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Ctrl-O, which is what saves. Deliberately not Ctrl-S: raw mode leaves `IXON`
/// on, so the line discipline would eat that one as XOFF and the terminal would
/// appear to freeze.
const SAVE: &str = "\x0f";
/// Ctrl-X, which is what leaves.
const LEAVE: &str = "\x18";

#[test]
fn a_person_types_into_the_screen_and_what_they_typed_is_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notas.txt");
    std::fs::write(&file, "uno\ndos\n").unwrap();

    // Open it, type at the very start of line 1, write it, leave, quit.
    let keys = format!("editar {}\nHOLA {SAVE}{LEAVE}salir\n", file.display());
    let output = at_a_keyboard(tmp.path(), &keys);

    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "HOLA uno\ndos\n",
        "the screen editor did not write what was typed; it said:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn leaving_without_saving_leaves_the_file_exactly_as_it_was() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notas.txt");
    std::fs::write(&file, "uno\ndos\n").unwrap();

    // Type something, then Ctrl-X twice: the first asks, the second confirms.
    // A single Ctrl-X on a changed file must not lose the work, and a single
    // one that *did* leave would make this test pass for the wrong reason —
    // which is why the file's contents are the assertion and not the count of
    // keystrokes.
    let keys = format!("editar {}\nBASURA{LEAVE}{LEAVE}salir\n", file.display());
    at_a_keyboard(tmp.path(), &keys);

    assert_eq!(std::fs::read_to_string(&file).unwrap(), "uno\ndos\n");
}

#[test]
fn one_ctrl_x_on_a_changed_file_asks_instead_of_throwing_the_work_away() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notas.txt");
    std::fs::write(&file, "uno\n").unwrap();

    // One Ctrl-X, then Ctrl-O to save after being asked, then Ctrl-X to go.
    let keys = format!(
        "editar {}\nNUEVO {LEAVE}{SAVE}{LEAVE}salir\n",
        file.display()
    );
    let output = at_a_keyboard(tmp.path(), &keys);

    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "NUEVO uno\n",
        "the first Ctrl-X threw the work away instead of asking; it said:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn the_screen_says_which_file_and_how_many_lines_before_anybody_types() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notas.txt");
    std::fs::write(&file, "uno\ndos\ntres\n").unwrap();

    let keys = format!("editar {}\n{LEAVE}salir\n", file.display());
    let drawn = String::from_utf8_lossy(&at_a_keyboard(tmp.path(), &keys).stdout).to_string();

    assert!(
        drawn.contains("notas.txt"),
        "no file name on screen:\n{drawn}"
    );
    assert!(
        drawn.contains("3 lines"),
        "no line count on screen:\n{drawn}"
    );
    // The legend, because on a machine with no shell an editor whose keys are
    // not on screen is an editor nobody can leave.
    assert!(
        drawn.contains("Ctrl-O") && drawn.contains("Ctrl-X"),
        "the way out is not on screen:\n{drawn}"
    );
}

#[test]
fn ctrl_u_takes_back_the_last_change_on_the_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notas.txt");
    std::fs::write(&file, "uno\n").unwrap();

    // Type a character, take it back, save, leave. What lands on disk is the
    // original text — which is the only way to tell an undo that worked from a
    // screen that merely stopped showing the character.
    let keys = format!("editar {}\nX\x15{SAVE}{LEAVE}salir\n", file.display());
    at_a_keyboard(tmp.path(), &keys);

    assert_eq!(std::fs::read_to_string(&file).unwrap(), "uno\n");
}

/// D1 for the last verb that changes the machine and could not be rehearsed,
/// closed on 2026-08-26.
///
/// The assertion that matters is the bytes on disk, read with something that
/// is not Thalyx — for the reason this whole file exists: "it reported that it
/// wrote nothing" and "it wrote nothing" are different claims, and a rehearsal
/// is entirely the second one.
#[test]
fn a_rehearsed_edit_answers_what_it_would_do_and_the_bytes_on_disk_do_not_move() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("thalyx.conf");
    std::fs::write(&file, "uno\ndos\ntres\n").unwrap();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("ensayo editar {} cambiar 2 DOS", file.display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "rehearse");
    assert_eq!(said["verb"], "edit");
    assert_eq!(said["would"], "replaced");
    assert_eq!(said["wrote"], false);
    // The same arithmetic the verb does, because it is the verb's own code:
    // the size it foresees is the size the real edit would produce.
    assert_eq!(said["lines_after"], 3);

    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "uno\ndos\ntres\n",
        "the rehearsal wrote to the file"
    );
}

/// The control, and without it the test above passes on an `editar` that
/// stopped working entirely.
#[test]
fn the_same_words_without_ensayo_do_change_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("thalyx.conf");
    std::fs::write(&file, "uno\ndos\ntres\n").unwrap();

    let rehearsed = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("ensayo editar {} cambiar 2 DOS", file.display()),
            "salir",
        ],
    );
    answer_to(&rehearsed, "rehearse");
    let after_rehearsal = std::fs::read_to_string(&file).unwrap();

    let real = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("editar {} cambiar 2 DOS", file.display()),
            "salir",
        ],
    );
    answer_to(&real, "edit");
    let after_the_verb = std::fs::read_to_string(&file).unwrap();

    assert_eq!(after_rehearsal, "uno\ndos\ntres\n");
    assert_eq!(after_the_verb, "uno\nDOS\ntres\n");
    assert_ne!(
        after_rehearsal, after_the_verb,
        "the verb and its rehearsal did the same thing, so one of them is wrong"
    );
}

/// A refusal from a rehearsal comes back under `rehearse` and not under `edit`.
///
/// `describe` promises that, and a caller that saw `op: edit` here would read
/// it as the file having been touched and the write having failed.
#[test]
fn a_rehearsal_that_cannot_be_done_still_answers_as_a_rehearsal() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("short.conf");
    std::fs::write(&file, "uno\ndos\n").unwrap();

    let output = typed(
        tmp.path(),
        &[
            "structured on",
            &format!("ensayo editar {} cambiar 40 nada", file.display()),
            "salir",
        ],
    );

    let said = answer_to(&output, "rehearse");
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"], "no_such_line");
}
