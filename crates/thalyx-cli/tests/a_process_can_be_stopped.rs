//! What is running can be seen and stopped, at the real prompt.
//!
//! **Rule 1**: every real defect in this project came from running the system.
//! So nothing here inspects `thalyx-proc`. Every test starts a real process,
//! types at the real prompt of the real binary, and then asks the *kernel*
//! — through `wait`, which is the standard library and not Thalyx — whether the
//! process died and which signal did it.
//!
//! **Rule 4**: every one of the three claims here has a control.
//!
//! - that `matar` stops something → a process nobody signalled, still running
//!   at the end of the same session;
//! - that `forzar` means something → the same process, sent `TERM` first, which
//!   it ignores because it was started to ignore it;
//! - that `ensayo matar` sends nothing → the process is alive afterwards, which
//!   is the only assertion that separates a rehearsal from the real thing.

use std::io::Write;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

fn typed(lines: &[&str]) -> Output {
    let root = tempfile::tempdir().expect("a store root");
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root.path())
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

/// A process that waits, and can be told to stop.
fn a_waiting_process() -> Child {
    Command::new("sleep")
        .arg("600")
        .spawn()
        .expect("sleep(1) is present")
}

/// A process that has been told to ignore being asked politely.
///
/// The `trap` is what makes the `forzar` test mean something: without it,
/// `TERM` and `KILL` are indistinguishable from outside.
fn a_stubborn_process() -> Child {
    Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; while :; do sleep 0.2; done")
        .spawn()
        .expect("sh(1) is present")
}

/// Wait for a child to die, up to a bounded patience.
///
/// Named rather than an unqualified `sleep`, because a test that hangs has to
/// say what it was waiting for — the rule this project wrote on 2026-08-10
/// after a hang that named nothing.
fn died_within(child: &mut Child, patience: Duration) -> Option<std::process::ExitStatus> {
    let until = Instant::now() + patience;
    while Instant::now() < until {
        match child.try_wait().expect("asking the kernel") {
            Some(status) => return Some(status),
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    None
}

const PATIENCE: Duration = Duration::from_secs(10);

#[test]
fn a_program_is_listed_with_its_number_and_what_it_occupies() {
    let mut waiting = a_waiting_process();
    let pid = waiting.id() as i32;

    let output = typed(&["structured on", "procesos sleep", "salir"]);
    let said = answer_to(&output, "processes");
    assert_eq!(said["ok"], true);

    let row = said["processes"]
        .as_array()
        .expect("processes is an array")
        .iter()
        .find(|row| row["pid"] == pid)
        .unwrap_or_else(|| panic!("the process this test started is not listed: {said}"));
    assert_eq!(row["name"], "sleep");
    assert_eq!(row["state"], "sleeping");
    assert!(row["resident"].as_u64().unwrap() > 0);
    assert!(
        row["command"].as_str().unwrap().contains("600"),
        "the command line is what it was started with"
    );

    waiting.kill().ok();
    waiting.wait().ok();
}

#[test]
fn a_pattern_narrows_the_list_and_the_count_says_what_was_looked_at() {
    let mut waiting = a_waiting_process();

    let output = typed(&["structured on", "procesos zzznosuchname", "salir"]);
    let said = answer_to(&output, "processes");
    assert_eq!(said["total"], 0);
    assert_eq!(said["pattern"], "zzznosuchname");
    // Silence is never an answer, and neither is a zero with nothing beside it:
    // the same object still carries what could not be read and what ended while
    // the list was being taken.
    assert!(said["unreadable"].is_array());
    assert!(said["ended_while_reading"].is_number());

    waiting.kill().ok();
    waiting.wait().ok();
}

#[test]
fn asking_a_process_to_stop_stops_it_and_the_kernel_says_which_signal_did_it() {
    let mut asked = a_waiting_process();
    // The control: nobody signals this one, and it must be alive at the end.
    // Without it, a `matar` that killed everything would pass.
    let mut untouched = a_waiting_process();

    let output = typed(&["structured on", &format!("matar {}", asked.id()), "salir"]);
    let said = answer_to(&output, "stop");
    assert_eq!(said["ok"], true);
    assert_eq!(said["signal"], "terminate");
    assert_eq!(said["was"]["pid"], asked.id());
    assert_eq!(said["was"]["name"], "sleep");
    // Said in the answer because it is true and nothing else would say it.
    assert_eq!(said["undo"], "none");

    let status = died_within(&mut asked, PATIENCE).expect("it stopped");
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&status),
        Some(15),
        "asked, not made"
    );
    assert!(
        untouched.try_wait().expect("asking the kernel").is_none(),
        "a process nobody named was signalled"
    );

    untouched.kill().ok();
    untouched.wait().ok();
}

#[test]
fn forzar_is_the_difference_between_asking_and_making() {
    // The baseline: this process ignores `TERM`, so `matar` alone must leave it
    // running. Without that half, a `forzar` that quietly always sent `KILL`
    // and a `matar` that quietly always sent `KILL` look the same.
    let mut stubborn = a_stubborn_process();
    let pid = stubborn.id();

    let asked = typed(&["structured on", &format!("matar {pid}"), "salir"]);
    assert_eq!(answer_to(&asked, "stop")["signal"], "terminate");
    assert!(
        died_within(&mut stubborn, Duration::from_millis(400)).is_none(),
        "it was built to ignore TERM and it did not"
    );

    let made = typed(&["structured on", &format!("matar {pid} forzar"), "salir"]);
    assert_eq!(answer_to(&made, "stop")["signal"], "kill");

    let status = died_within(&mut stubborn, PATIENCE).expect("KILL cannot be ignored");
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&status),
        Some(9)
    );
}

#[test]
fn a_rehearsal_says_which_process_that_number_is_and_sends_nothing() {
    // The assertion that separates a rehearsal from the real thing, and the
    // only one that matters: the process is still there afterwards.
    let mut waiting = a_waiting_process();
    let pid = waiting.id();

    let output = typed(&["structured on", &format!("ensayo matar {pid}"), "salir"]);
    let said = answer_to(&output, "rehearse");
    assert_eq!(said["would"], "stop");
    assert_eq!(said["changed"], false);
    assert_eq!(said["undo"], "none");
    assert_eq!(said["process"]["pid"], pid);
    assert_eq!(said["process"]["name"], "sleep");

    assert!(
        died_within(&mut waiting, Duration::from_millis(300)).is_none(),
        "the rehearsal killed the process it was rehearsing"
    );
    waiting.kill().ok();
    waiting.wait().ok();
}

#[test]
fn init_is_refused_with_the_verb_that_does_that_job_properly() {
    let output = typed(&["structured on", "matar 1", "salir"]);
    let said = answer_to(&output, "stop");
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"], "is_init");
    // A2: an error that names the way out costs one field and saves a cycle of
    // guessing. This one is not a restriction — `apagar` does the same thing
    // properly — so an error that only refused would be hiding the answer.
    assert_eq!(said["remedy"], "use_poweroff");
}

#[test]
fn a_line_naming_two_processes_stops_neither() {
    // Refused rather than obeyed for the first: whoever wrote that line
    // expected both to stop, and stopping one of them silently is the worst
    // available outcome.
    let mut one = a_waiting_process();
    let mut two = a_waiting_process();

    let output = typed(&[
        "structured on",
        &format!("matar {} {}", one.id(), two.id()),
        "salir",
    ]);
    let said = answer_to(&output, "stop");
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"], "one_at_a_time");

    for child in [&mut one, &mut two] {
        assert!(
            child.try_wait().expect("asking the kernel").is_none(),
            "one of them was stopped anyway"
        );
        child.kill().ok();
        child.wait().ok();
    }
}

#[test]
fn something_that_is_not_a_number_is_refused_by_name() {
    let output = typed(&["structured on", "matar elprograma", "salir", "salir"]);
    let said = answer_to(&output, "stop");
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"], "not_a_number");
    assert_eq!(said["remedy"], "give_a_number");
}

#[test]
fn saying_nothing_is_refused_rather_than_taken_as_anything() {
    let output = typed(&["structured on", "matar", "salir"]);
    let said = answer_to(&output, "stop");
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"], "nothing_asked");
}

#[test]
fn the_memory_reading_keeps_free_and_available_apart() {
    let output = typed(&["structured on", "memoria", "salir"]);
    let said = answer_to(&output, "memory");
    assert_eq!(said["ok"], true);

    let total = said["total"].as_u64().expect("a total");
    let available = said["available"].as_u64().expect("an available");
    let free = said["free"].as_u64().expect("a free");
    assert!(total > 0);
    assert!(available <= total);
    assert!(free <= total);
    // The one arithmetic claim: what is in use is what is not available, and
    // never what is not free. A machine with a large cache and this wrong looks
    // like a machine about to run out.
    assert_eq!(said["in_use"].as_u64().unwrap(), total - available);
}

#[test]
fn a_person_gets_a_table_and_a_program_gets_objects_for_the_same_question() {
    let mut waiting = a_waiting_process();

    let output = typed(&["procesos sleep", "memoria", "salir"]);
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(said.contains("COMMAND"), "a person gets a heading: {said}");
    assert!(
        said.contains("what something new could get"),
        "the number that answers the question is the one named: {said}"
    );
    assert!(
        objects(&output)
            .iter()
            .all(|value| value["op"] != "processes"),
        "the human face printed an object"
    );

    waiting.kill().ok();
    waiting.wait().ok();
}
