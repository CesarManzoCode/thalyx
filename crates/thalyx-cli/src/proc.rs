//! `procesos`, `memoria` and `matar` — point 7 of the usable terminal.
//!
//! Three verbs over `/proc`, and the engine is `thalyx-proc`. Nothing here
//! decides what a process *is*; this file decides only how the two faces ask
//! and how each is answered.
//!
//! ## Why `matar` is the careful one
//!
//! It is the second verb whose ordinary use destroys something — the first was
//! `editar` — and unlike a file there is nothing to write back afterwards. So:
//!
//! - the signal goes through a pidfd, so it cannot land on a recycled number;
//! - `TERM` unless the word `forzar` is typed, because a program that can catch
//!   the signal gets to write what it was holding;
//! - PID 1 and this session are refused by name, each pointing at the verb that
//!   does that job properly — `apagar` and `salir`. Not a policy about who owns
//!   the machine, which is Cesar: it is that a signal does those two jobs in the
//!   one way that leaves nothing said about why the machine stopped;
//! - `ensayo matar <pid>` says exactly which process that number is, with its
//!   command line, and sends nothing. D1 of `Superficie-para-el-LLM.md`, and
//!   here it is worth more than anywhere else, because the mistake it prevents
//!   is unrecoverable and the input that causes it is four digits.

use crate::files::Face;
use serde_json::{Value, json};
use thalyx_files::Size;
use thalyx_proc::{Memory, ProcError, Process, Stopped};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// `procesos [patrón]` — what is running.
pub fn running(rest: &str, face: Face) -> Fallible {
    let op = "processes";
    let (pattern, window) = match crate::index::asked_of(rest) {
        Ok(both) => both,
        Err(why) => {
            declined(face, op, "bad_cursor", &why.to_string());
            return Ok(());
        }
    };

    let running = thalyx_proc::running();
    // Filtered on the name with the same `*` and `?` `rm` and `encontrar` use,
    // because a second spelling of "matches" is a discovery cost paid twice.
    let chosen: Vec<&Process> = running
        .processes
        .iter()
        .filter(|process| pattern.is_empty() || thalyx_files::matches(&pattern, &process.name))
        .collect();

    let page = match thalyx_files::window::page(chosen, pid_key, &window) {
        Ok(page) => page,
        Err(why) => {
            declined(face, op, "unordered", &why.to_string());
            return Ok(());
        }
    };

    if face.is_machine() {
        let rows: Vec<Value> = page.rows.iter().map(|process| object(process)).collect();
        let mut carried = vec![
            ("pattern", json!(pattern)),
            ("processes", json!(rows)),
            // Its own number, and not folded into anything: a process that
            // ended between being listed and being read is what processes do,
            // and a caller comparing two readings needs the difference to have
            // a stated reason.
            ("ended_while_reading", json!(running.gone)),
            (
                "unreadable",
                json!(
                    running
                        .unreadable
                        .iter()
                        .map(|(what, why)| json!({ "what": what, "why": why }))
                        .collect::<Vec<_>>()
                ),
            ),
        ];
        carried.extend(thalyx_files::machine::window_fields(&page));
        println!("{}", thalyx_files::machine::answer(op, carried));
        return Ok(());
    }

    println!();
    if page.rows.is_empty() {
        println!(
            "  nothing running is called `{pattern}` — {} process(es) looked at.",
            running.processes.len()
        );
        println!();
        return Ok(());
    }
    println!("    PID   PPID  STATE                RSS  COMMAND");
    let width = crate::files::screen_width();
    for process in &page.rows {
        let line = format!(
            "  {:>5}  {:>5}  {:<12} {:>10}  {}",
            process.pid,
            process.parent,
            process.state.word(),
            Size(process.resident).to_string(),
            shown_command(process),
        );
        println!("{}", clip(&line, width));
    }
    println!();
    if page.more {
        println!(
            "  showing {} of {}. `procesos cursor={} …` continues.",
            page.before + page.rows.len(),
            page.total,
            page.next.as_deref().unwrap_or("…")
        );
        println!();
    }
    if running.gone > 0 {
        println!("  {} ended while this was reading.", running.gone);
        println!();
    }
    for (what, why) in &running.unreadable {
        println!("  {what} could not be read — {why}");
    }
    Ok(())
}

/// `memoria` — how much is left.
pub fn memory(face: Face) -> Fallible {
    let op = "memory";
    let memory = match thalyx_proc::memory() {
        Ok(memory) => memory,
        Err(error) => return refused(face, op, &error),
    };

    if face.is_machine() {
        println!(
            "{}",
            thalyx_files::machine::answer(op, memory_fields(&memory))
        );
        return Ok(());
    }

    println!();
    println!("  {:<11} {:>10}", "total", Size(memory.total).to_string());
    // Available first and free under it, deliberately. A person who reads the
    // first number and stops has to read the one that answers the question.
    println!(
        "  {:<11} {:>10}   what something new could get",
        "available",
        Size(memory.available).to_string()
    );
    println!(
        "  {:<11} {:>10}   in use, counting the cache as free",
        "in use",
        Size(memory.in_use()).to_string()
    );
    println!(
        "  {:<11} {:>10}   untouched — a healthy machine keeps this low",
        "free",
        Size(memory.free).to_string()
    );
    println!(
        "  {:<11} {:>10}",
        "cached",
        Size(memory.cached + memory.buffers).to_string()
    );
    if memory.swap_total == 0 {
        println!("  {:<11} {:>10}", "swap", "none");
    } else {
        println!(
            "  {:<11} {:>10}   of {}",
            "swap free",
            Size(memory.swap_free).to_string(),
            Size(memory.swap_total)
        );
    }
    println!();
    Ok(())
}

/// `matar <pid> [forzar]` — stop one.
pub fn stop(rest: &str, face: Face) -> Fallible {
    let op = "stop";
    let words: Vec<&str> = rest.split_whitespace().collect();
    let force = words.iter().any(|word| FORCE.contains(word));
    let named: Vec<&&str> = words.iter().filter(|word| !FORCE.contains(*word)).collect();

    let Some(number) = named.first() else {
        return refused(face, op, &ProcError::NothingAsked);
    };
    if named.len() > 1 {
        // One at a time, and refused rather than obeyed for the first. A line
        // that names two processes is a line whose author expected both to
        // stop, and stopping one of them silently is worse than stopping none.
        declined(
            face,
            op,
            "one_at_a_time",
            "one process at a time — `matar` takes one number",
        );
        return Ok(());
    }
    let Ok(pid) = number.parse::<i32>() else {
        return refused(face, op, &ProcError::NotANumber((*number).to_string()));
    };

    match thalyx_proc::stop(pid, force) {
        Ok(stopped) => {
            if face.is_machine() {
                println!(
                    "{}",
                    thalyx_files::machine::answer(op, stopped_fields(&stopped))
                );
            } else {
                say_stopped(&stopped);
            }
            Ok(())
        }
        Err(error) => refused(face, op, &error),
    }
}

/// `ensayo matar <pid>` — which process that number is, and nothing sent.
///
/// The rehearsal a person most needs and the one hardest to give: a file can be
/// put back and a process cannot. So this answers with everything that would
/// let somebody notice they typed the wrong four digits — the name, the whole
/// command line, how long it has been running, and its parent.
pub fn rehearse_stop(rest: &str, face: Face) -> Fallible {
    let op = "rehearse";
    let words: Vec<&str> = rest.split_whitespace().collect();
    let force = words.iter().any(|word| FORCE.contains(word));
    let Some(number) = words.iter().find(|word| !FORCE.contains(*word)) else {
        return refused(face, op, &ProcError::NothingAsked);
    };
    let Ok(pid) = number.parse::<i32>() else {
        return refused(face, op, &ProcError::NotANumber((*number).to_string()));
    };
    // The two refusals are part of the rehearsal, not a separate check: a
    // person rehearsing `matar 1` has to be told it will be refused, or they
    // learn it by typing the real thing.
    if pid == 1 {
        return refused(face, op, &ProcError::IsInit(pid));
    }
    if pid == std::process::id() as i32 {
        return refused(face, op, &ProcError::IsSelf(pid));
    }

    let process = match thalyx_proc::describe(pid) {
        Ok(process) => process,
        Err(error) => return refused(face, op, &error),
    };

    if face.is_machine() {
        let mut carried = vec![
            ("would", json!("stop")),
            ("signal", json!(if force { "kill" } else { "terminate" })),
            ("process", object(&process)),
            // Said out loud in a rehearsal, because it is the whole reason a
            // rehearsal of this verb exists: there is no taking it back.
            ("undo", json!("none")),
        ];
        carried.push(("changed", json!(false)));
        println!("{}", thalyx_files::machine::answer(op, carried));
        return Ok(());
    }

    println!();
    println!(
        "  {} would {} {} ({}), running for {}.",
        pid,
        if force { "make" } else { "ask" },
        if force { "stop" } else { "to stop" },
        process.name,
        for_how_long(process.age)
    );
    println!("    {}", shown_command(&process));
    println!(
        "    started by {}, {} thread(s)",
        process.parent, process.threads
    );
    println!();
    println!("  Nothing was sent. This cannot be undone once it is.");
    println!();
    Ok(())
}

/// Both spellings of the word that turns an ask into a kill.
const FORCE: &[&str] = &["forzar", "force"];

fn pid_key(process: &&Process) -> Vec<u8> {
    // Fixed-width and big-endian, so byte order and numeric order are the same
    // thing. As decimal text, pid 10 sorts before pid 9 and the window refuses
    // the whole answer as unordered.
    (process.pid as u32).to_be_bytes().to_vec()
}

fn object(process: &Process) -> Value {
    json!({
        "pid": process.pid,
        "parent": process.parent,
        "name": process.name,
        // `null` for a kernel thread, and that is an answer: it has no command
        // line at all, which is a different fact from an empty one.
        "command": process.command,
        "state": process.state.word(),
        "resident": process.resident,
        "threads": process.threads,
        "uid": process.uid,
        "age": process.age,
    })
}

fn memory_fields(memory: &Memory) -> Vec<(&'static str, Value)> {
    vec![
        ("total", json!(memory.total)),
        // First, because it is the one that answers the question. A caller that
        // reads `free` and decides is a caller that decided on the wrong number.
        ("available", json!(memory.available)),
        ("in_use", json!(memory.in_use())),
        ("free", json!(memory.free)),
        ("cached", json!(memory.cached)),
        ("buffers", json!(memory.buffers)),
        ("swap_total", json!(memory.swap_total)),
        ("swap_free", json!(memory.swap_free)),
    ]
}

fn stopped_fields(stopped: &Stopped) -> Vec<(&'static str, Value)> {
    vec![
        ("signal", json!(stopped.signal)),
        // What was signalled, described from a handle taken *before* the
        // signal, so this is the process that was stopped and not whatever held
        // the number a moment earlier.
        ("was", object(&stopped.was)),
        ("undo", json!("none")),
    ]
}

fn say_stopped(stopped: &Stopped) {
    println!();
    if stopped.signal == "kill" {
        println!(
            "  {} ({}) was made to stop. Nothing it was holding was written.",
            stopped.was.pid, stopped.was.name
        );
    } else {
        println!(
            "  {} ({}) was asked to stop. `matar {} forzar` if it does not.",
            stopped.was.pid, stopped.was.name, stopped.was.pid
        );
    }
    println!();
}

/// What to show for a process: its command line, or its name in brackets.
///
/// The brackets are what `ps` uses for a kernel thread and they are worth
/// keeping: a person who sees `[kworker/0:1]` knows not to try to kill it.
fn shown_command(process: &Process) -> String {
    match &process.command {
        Some(command) => command.clone(),
        None => format!("[{}]", process.name),
    }
}

fn for_how_long(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m", seconds / 60),
        3600..=86_399 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86_400),
    }
}

/// Cut a line to the terminal, by characters and never by bytes.
fn clip(line: &str, width: usize) -> String {
    if line.chars().count() <= width {
        return line.to_string();
    }
    line.chars().take(width).collect()
}

fn refused(face: Face, op: &str, error: &ProcError) -> Fallible {
    if face.is_machine() {
        println!(
            "{}",
            thalyx_files::machine::refused(op, error.word(), error.remedy(), &error.to_string())
        );
    } else {
        println!("\n  {error}\n");
    }
    Ok(())
}

fn declined(face: Face, op: &str, word: &str, why: &str) {
    if face.is_machine() {
        println!("{}", thalyx_files::machine::declined(op, word, why));
    } else {
        println!("\n  {why}\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_long_something_has_run_is_said_in_the_unit_a_person_reads() {
        assert_eq!(for_how_long(9), "9s");
        assert_eq!(for_how_long(90), "1m");
        assert_eq!(for_how_long(7200), "2h");
        assert_eq!(for_how_long(200_000), "2d");
    }

    #[test]
    fn a_kernel_thread_is_shown_in_brackets_and_not_as_an_empty_line() {
        let thread = Process {
            pid: 12,
            parent: 2,
            name: "kworker/0:1".to_string(),
            command: None,
            state: thalyx_proc::State::Sleeping,
            resident: 0,
            threads: 1,
            uid: 0,
            age: 5,
        };
        assert_eq!(shown_command(&thread), "[kworker/0:1]");
    }

    #[test]
    fn a_pid_key_sorts_the_way_numbers_do_and_not_the_way_text_does() {
        // As decimal text, `10` sorts before `9`, the keys stop ascending, and
        // the window refuses the entire answer as unordered. Found this way
        // once already, in `index.rs`.
        let key = |pid: i32| {
            let process = Process {
                pid,
                parent: 1,
                name: String::new(),
                command: None,
                state: thalyx_proc::State::Sleeping,
                resident: 0,
                threads: 1,
                uid: 0,
                age: 0,
            };
            pid_key(&&process)
        };
        assert!(key(9) < key(10));
        assert!(key(2) < key(100));
    }
}
