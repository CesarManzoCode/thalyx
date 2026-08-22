//! What is running, how much memory is left, and stopping something.
//!
//! Point 7 of the usable terminal, in `vault/06-Pendientes/Tareas-Pendientes.md`.
//! Everything here reads `/proc`, which is the kernel answering about itself —
//! there is no `ps`, no `free` and no `kill` on the image, and there is not
//! going to be one: the image carries the kernel and one program.
//!
//! ## What this is not
//!
//! It does not **launch** anything. Launching is G1 of
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md` and it is tangled with G2
//! and with the decree that Thalyx runs only signed modules; point 7 is the
//! narrower half that a person needs to answer *what is eating this machine*
//! and *make it stop*.
//!
//! ## The pid is not the process
//!
//! Between reading `/proc/4711` and signalling 4711, that process can exit and
//! the kernel can hand the number to something else. Every tool that takes a
//! pid on its command line has this hole. [`stop`] does not: it opens a pidfd
//! first and signals through it, so the signal reaches the process the handle
//! was opened for or it fails, and never a stranger.
//!
//! That also decides the order of operations, and it is the opposite of the
//! obvious one: **the handle is taken before the description is read**, so what
//! comes back in the answer is a description of the thing that was signalled
//! rather than of whatever held the number a moment earlier.
//!
//! ## Reading `/proc` is reading another program's output
//!
//! Rule 6 of `Estrategia-de-Pruebas.md`: a parser for another tool's format
//! needs one captured real sample, verbatim, because a hand-written fixture
//! proves the parser matches its author's idea of the format. The trap in
//! `/proc/<pid>/stat` is the second field:
//!
//! ```text
//! 4709 (we (ird) x) S 4706 4706 4350 0 -1 4194304 83 0 0 0 0 0 …
//! ```
//!
//! That is a real line, captured on 2026-08-23 from a real process. The name
//! is whatever the executable was called, in parentheses, and it can hold
//! spaces and more parentheses. Splitting the line on whitespace — which is
//! what every first attempt does — puts `S` five fields early and reports the
//! parent pid as `4350`. So the name is taken between the **first** `(` and the
//! **last** `)`, and everything after that is split.
//!
//! ## Rule 10, and it is the ordinary case here
//!
//! A process that exits while this is walking `/proc` is not an error: it is
//! what processes do. It is left out of the listing and counted, so that a
//! caller comparing two readings is not told the machine has fewer processes
//! than it counted. A directory that could not be read for any *other* reason
//! is reported by name.

use std::path::{Path, PathBuf};
use thalyx_syscall::Signal;

/// One running process, as `/proc` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub pid: i32,
    pub parent: i32,
    /// The executable's name, up to the 15 characters the kernel keeps.
    pub name: String,
    /// What it was started with, or `None` for a kernel thread — which has no
    /// command line at all, and is a different fact from an empty one.
    pub command: Option<String>,
    pub state: State,
    /// Resident memory in bytes. What it actually occupies, not what it asked
    /// for: a program that maps a hundred gigabytes and touches four pages is
    /// using four pages, and the number a person acts on is this one.
    pub resident: u64,
    pub threads: u32,
    pub uid: u32,
    /// Seconds since this process started, from the machine's own uptime.
    pub age: u64,
}

/// What the kernel says a process is doing.
///
/// Kept as a named thing rather than the raw letter, because `D` means
/// *uninterruptible sleep* and a person looking at it needs to know that
/// `matar` will not touch it — which is the single most confusing thing about
/// a process list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Sleeping,
    /// Waiting on the kernel, usually on disk. **Signals do not arrive here**,
    /// so a `matar` that appears to do nothing has this as its explanation.
    Uninterruptible,
    /// Exited, and nobody has collected the exit status yet. Killing one does
    /// nothing: it is already dead. Its parent is what has to go.
    Zombie,
    Stopped,
    Other(char),
}

impl State {
    /// The word a program matches on. Stable, never translated.
    pub fn word(self) -> &'static str {
        match self {
            State::Running => "running",
            State::Sleeping => "sleeping",
            State::Uninterruptible => "uninterruptible",
            State::Zombie => "zombie",
            State::Stopped => "stopped",
            State::Other(_) => "other",
        }
    }

    fn from_letter(letter: char) -> Self {
        match letter {
            'R' => State::Running,
            'S' | 'I' => State::Sleeping,
            'D' => State::Uninterruptible,
            'Z' => State::Zombie,
            'T' | 't' => State::Stopped,
            other => State::Other(other),
        }
    }
}

/// Everything that is running, and what could not be established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    /// Ascending by pid, which is what a cursor into them names.
    pub processes: Vec<Process>,
    /// How many disappeared between being listed and being read. Not an error
    /// — it is what processes do — but counted, so that a caller comparing two
    /// readings is not told the machine shrank.
    pub gone: usize,
    /// Rule 10: what could not be read for some other reason, and why.
    pub unreadable: Vec<(String, String)>,
}

/// How much memory the machine has and how much of it can still be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    /// **The number that answers the question.** The kernel's own estimate of
    /// what a new program could get without swapping, which counts the cache it
    /// would drop.
    pub available: u64,
    /// Untouched memory. Almost always alarming and almost never the answer: a
    /// healthy Linux keeps this near zero on purpose, because memory doing
    /// nothing is memory wasted. Reported next to `available` rather than
    /// instead of it, since a person who only sees this one concludes the
    /// machine is full.
    pub free: u64,
    pub cached: u64,
    pub buffers: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

impl Memory {
    /// Bytes in use, which is `total - available` and not `total - free`.
    pub fn in_use(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
}

/// What a process was, and what was done to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stopped {
    pub was: Process,
    pub signal: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcError {
    #[error("there is no process {0}")]
    NoSuchProcess(i32),

    /// Refused before the signal, and by identity rather than by number.
    #[error(
        "{0} is this machine's init, and stopping it stops the machine. `apagar` is how that is done"
    )]
    IsInit(i32),

    /// The session refusing to signal itself. `salir` ends a session and
    /// `apagar` ends the machine; a signal to this pid would end it in the one
    /// way that leaves nothing said about why.
    #[error("{0} is this session. `salir` leaves it and `apagar` turns the machine off")]
    IsSelf(i32),

    #[error("{pid} belongs to someone else and this session may not signal it")]
    NotAllowed { pid: i32 },

    #[error("{what} could not be read: {detail}")]
    Unreadable { what: String, detail: String },

    /// A number that is not a pid at all.
    #[error("`{0}` is not a process number")]
    NotANumber(String),

    #[error("say which process — `procesos` lists them with their numbers")]
    NothingAsked,
}

impl ProcError {
    pub fn word(&self) -> &'static str {
        match self {
            ProcError::NoSuchProcess(_) => "no_such_process",
            ProcError::IsInit(_) => "is_init",
            ProcError::IsSelf(_) => "is_self",
            ProcError::NotAllowed { .. } => "not_allowed",
            ProcError::Unreadable { .. } => "unreadable",
            ProcError::NotANumber(_) => "not_a_number",
            ProcError::NothingAsked => "nothing_asked",
        }
    }

    /// What would get past this, as a word. `Superficie-para-el-LLM.md`, A2.
    ///
    /// Two of them are `cannot`, and that is an answer: a caller told to retry
    /// something that will never work spends its cycles finding that out.
    pub fn remedy(&self) -> &'static str {
        match self {
            ProcError::NoSuchProcess(_) => "list_first",
            ProcError::IsInit(_) => "use_poweroff",
            ProcError::IsSelf(_) => "use_exit",
            ProcError::NotAllowed { .. } => "cannot",
            ProcError::Unreadable { .. } => "cannot",
            ProcError::NotANumber(_) => "give_a_number",
            ProcError::NothingAsked => "list_first",
        }
    }
}

/// Where `/proc` is. A parameter so the parsing can be tested against captured
/// trees rather than against whatever this machine happens to be running.
const PROC: &str = "/proc";

/// Everything running, read from `/proc`.
pub fn running() -> Running {
    from_proc(Path::new(PROC))
}

/// How much memory there is, and how much of it is usable.
pub fn memory() -> Result<Memory, ProcError> {
    memory_from(Path::new(PROC))
}

/// Stop a process, by identity and not by number.
///
/// The order matters and is the reverse of the obvious one: the handle comes
/// first, then the description, then the signal. Reading `/proc/<pid>` first
/// and opening the handle after would describe whatever held the number at the
/// moment of reading, which is exactly the process this exists to rule out.
pub fn stop(pid: i32, force: bool) -> Result<Stopped, ProcError> {
    if pid == 1 {
        return Err(ProcError::IsInit(pid));
    }
    if pid == std::process::id() as i32 {
        return Err(ProcError::IsSelf(pid));
    }
    if pid <= 0 {
        // Negative numbers are process *groups* to `kill(2)` and 0 is "everything
        // I can reach". Neither is what anybody typed, and both are how a typo
        // takes down more than it named — so they are refused as not being a
        // process number rather than obeyed as a wider target.
        return Err(ProcError::NotANumber(pid.to_string()));
    }

    let handle = thalyx_syscall::open_process(pid).map_err(|error| classify(pid, error))?;
    let was = one(Path::new(PROC), pid).ok_or(ProcError::NoSuchProcess(pid))?;
    let signal = if force {
        Signal::Kill
    } else {
        Signal::Terminate
    };

    thalyx_syscall::signal_process(&handle, signal).map_err(|error| classify(pid, error))?;
    Ok(Stopped {
        was,
        signal: if force { "kill" } else { "terminate" },
    })
}

/// What a process is, without signalling it. What `ensayo matar` answers with.
pub fn describe(pid: i32) -> Result<Process, ProcError> {
    if pid <= 0 {
        return Err(ProcError::NotANumber(pid.to_string()));
    }
    one(Path::new(PROC), pid).ok_or(ProcError::NoSuchProcess(pid))
}

/// The two `errno` values that mean something a person can act on, captured
/// from `/usr/include/asm-generic/errno-base.h` on 2026-08-23 rather than
/// recalled:
///
/// ```text
/// #define EPERM     1  /* Operation not permitted */
/// #define ESRCH     3  /* No such process */
/// ```
///
/// Written out rather than taken from `libc`, so that a crate which forbids
/// `unsafe` does not grow a dependency on one that is made of it — for two
/// integers that Linux has never changed on any architecture.
const EPERM: i32 = 1;
const ESRCH: i32 = 3;

/// Which refusal an OS error means.
///
/// `ESRCH` and `EPERM` send a person to opposite places — one to check the
/// number and one to check who they are — and merging them sends half of them
/// to the wrong one.
fn classify(pid: i32, error: std::io::Error) -> ProcError {
    match error.raw_os_error() {
        Some(ESRCH) => ProcError::NoSuchProcess(pid),
        Some(EPERM) => ProcError::NotAllowed { pid },
        _ => ProcError::Unreadable {
            what: format!("process {pid}"),
            detail: error.to_string(),
        },
    }
}

// ─────────────────────────────────────────────────────── the parsing, testable

/// Read a whole `/proc`-shaped tree.
///
/// Takes the root as an argument so the parsing can be run against a captured
/// tree. What it cannot be run against is a hand-written one that only holds
/// what its author expected — which is why the fixtures in the tests below are
/// real lines, copied verbatim.
pub fn from_proc(root: &Path) -> Running {
    let mut processes = Vec::new();
    let mut gone = 0;
    let mut unreadable = Vec::new();

    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            unreadable.push((root.display().to_string(), error.to_string()));
            return Running {
                processes,
                gone,
                unreadable,
            };
        }
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|text| text.parse::<i32>().ok()) else {
            // Everything in `/proc` that is not a number is not a process:
            // `meminfo`, `uptime`, `self`. Skipped rather than reported, since
            // there is nothing wrong with them being there.
            continue;
        };
        match one(root, pid) {
            Some(process) => processes.push(process),
            // It exited between the listing and the read, which is the ordinary
            // case and not a failure. Counted so that two readings that
            // disagree have a stated reason to.
            None => gone += 1,
        }
    }

    processes.sort_by_key(|process| process.pid);
    unreadable.sort();
    Running {
        processes,
        gone,
        unreadable,
    }
}

/// One process, or `None` if it is not there any more.
fn one(root: &Path, pid: i32) -> Option<Process> {
    let directory = root.join(pid.to_string());
    let stat = std::fs::read_to_string(directory.join("stat")).ok()?;
    let parsed = parse_stat(&stat)?;

    // The owner comes from the directory itself rather than from `status`,
    // because it is one `stat(2)` instead of parsing fifty lines, and the
    // kernel sets it to the process's real uid.
    let uid = std::fs::metadata(&directory)
        .map(|meta| std::os::unix::fs::MetadataExt::uid(&meta))
        .unwrap_or(0);

    let resident = std::fs::read_to_string(directory.join("statm"))
        .ok()
        .and_then(|text| parse_resident(&text))
        .unwrap_or(0);

    let command = std::fs::read(directory.join("cmdline"))
        .ok()
        .and_then(|bytes| parse_command(&bytes));

    let age = uptime_of(root)
        .map(|up| up.saturating_sub(parsed.started_at))
        .unwrap_or(0);

    Some(Process {
        pid,
        parent: parsed.parent,
        name: parsed.name,
        command,
        state: parsed.state,
        resident,
        threads: parsed.threads,
        uid,
        age,
    })
}

struct FromStat {
    name: String,
    state: State,
    parent: i32,
    threads: u32,
    /// Seconds after boot at which this process started.
    started_at: u64,
}

/// Parse one line of `/proc/<pid>/stat`.
///
/// The name is taken between the **first** `(` and the **last** `)`, and that
/// is the whole difficulty. A real captured line:
///
/// ```text
/// 4709 (we (ird) x) S 4706 4706 4350 …
/// ```
///
/// Splitting on whitespace makes the state `(ird)` and the parent `x`. Taking
/// the name up to the first `)` makes the state `x)` and the parent `S`. Only
/// the last `)` is right, because a process name cannot contain a newline and
/// the line has exactly one closing parenthesis after the name — the kernel
/// writes no others.
fn parse_stat(line: &str) -> Option<FromStat> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close < open {
        return None;
    }
    let name = line[open + 1..close].to_string();
    let rest: Vec<&str> = line[close + 1..].split_whitespace().collect();

    // Field 3 of the manual page is the first one here, so every index below is
    // the manual's number minus three. Written out rather than named by a
    // constant each, because the manual's numbering is the only documentation
    // there is and matching it is what makes this checkable.
    let state = State::from_letter(rest.first()?.chars().next()?);
    let parent = rest.get(1)?.parse().ok()?;
    let threads = rest.get(17)?.parse().ok()?;
    let started_ticks: u64 = rest.get(19)?.parse().ok()?;

    Some(FromStat {
        name,
        state,
        parent,
        threads,
        started_at: started_ticks / thalyx_syscall::clock_ticks(),
    })
}

/// Resident bytes, from the second field of `/proc/<pid>/statm`.
///
/// In pages, so the page size is asked of the kernel. Assuming 4096 would
/// under-report by a factor of four on an aarch64 kernel with 16k pages, and a
/// memory figure that is wrong in the reassuring direction is one somebody acts
/// on.
fn parse_resident(text: &str) -> Option<u64> {
    let pages: u64 = text.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * thalyx_syscall::page_size() as u64)
}

/// The command line, NUL-separated, or `None` for a kernel thread.
///
/// A kernel thread has an empty `cmdline`, and that is a different fact from a
/// process whose command line is a single empty argument. `None` says which.
fn parse_command(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    let joined: Vec<&str> = text.split('\0').filter(|piece| !piece.is_empty()).collect();
    if joined.is_empty() {
        return None;
    }
    Some(joined.join(" "))
}

/// Seconds since boot, from `/proc/uptime`.
fn uptime_of(root: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(root.join("uptime")).ok()?;
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(seconds as u64)
}

/// Parse `/proc/meminfo`, which is `Name: N kB`.
pub fn memory_from(root: &Path) -> Result<Memory, ProcError> {
    let path: PathBuf = root.join("meminfo");
    let text = std::fs::read_to_string(&path).map_err(|error| ProcError::Unreadable {
        what: path.display().to_string(),
        detail: error.to_string(),
    })?;

    let found = |wanted: &str| -> u64 {
        text.lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name != wanted {
                    return None;
                }
                let kb: u64 = value.split_whitespace().next()?.parse().ok()?;
                Some(kb * 1024)
            })
            .unwrap_or(0)
    };

    let total = found("MemTotal");
    if total == 0 {
        // A meminfo with no MemTotal is not a meminfo. Refused rather than
        // answered with zeros, which would read as a machine with no memory.
        return Err(ProcError::Unreadable {
            what: path.display().to_string(),
            detail: "no MemTotal line, so this is not a meminfo".to_string(),
        });
    }

    let free = found("MemFree");
    Ok(Memory {
        total,
        // Kernels before 3.14 have no MemAvailable. Falling back to MemFree is
        // wrong in the pessimistic direction — it under-reports what a program
        // could get, never over-reports it — which is the direction rule 9
        // asks for when a field cannot be read.
        available: match found("MemAvailable") {
            0 => free,
            available => available,
        },
        free,
        cached: found("Cached"),
        buffers: found("Buffers"),
        swap_total: found("SwapTotal"),
        swap_free: found("SwapFree"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real line from a real `/proc/<pid>/stat`, captured on 2026-08-23 from
    /// a process deliberately named `we (ird) x`.
    ///
    /// Rule 6: a hand-written fixture proves the parser matches its author's
    /// idea of the format. This project has been caught by that twice, both
    /// times on somebody else's output, and the second time it accused
    /// llama.cpp of ignoring a grammar it had just obeyed.
    const WEIRD: &str = "4709 (we (ird) x) S 4706 4706 4350 0 -1 4194304 83 0 0 0 0 0 0 0 20 0 1 0 16703 2772992 397 18446744073709551615 94342949076992 94342949090993 140731176927040 0 0 0 0 0 0 1 0 0 17 1 0 0 0 0 0 94342949100496 94342949101720 94343469514752 140731176931976 140731176932069 140731176932069 140731176939421 0";

    /// The ordinary case, captured the same day.
    const PLAIN: &str = "4341 (cat) R 3985 4341 3985 0 -1 4194304 83 0 0 0 0 0 0 0 20 0 1 0 15529 2920448 367 18446744073709551615 94835620790272 94835620807857 140722402230496 0 0 0 0 0 0 0 0 0 17 2 0 0 0 0 0 94835620821648 94835620823144 94836491714560 140722402239341 140722402239361 140722402239361 140722402246635 0";

    #[test]
    fn a_name_with_spaces_and_parentheses_in_it_is_read_whole() {
        // Splitting on whitespace gives state `(ird)` and parent `x`. Stopping
        // at the first `)` gives state `x)` and parent `S`. Both are wrong in
        // the worst way available: they answer confidently.
        let parsed = parse_stat(WEIRD).expect("a real line");
        assert_eq!(parsed.name, "we (ird) x");
        assert_eq!(parsed.state, State::Sleeping);
        assert_eq!(parsed.parent, 4706);
        assert_eq!(parsed.threads, 1);
    }

    #[test]
    fn the_ordinary_line_reads_the_same_way_as_the_awkward_one() {
        let parsed = parse_stat(PLAIN).expect("a real line");
        assert_eq!(parsed.name, "cat");
        assert_eq!(parsed.state, State::Running);
        assert_eq!(parsed.parent, 3985);
    }

    #[test]
    fn a_line_that_is_not_one_is_refused_rather_than_half_read() {
        // Rule 9. A truncated read of `/proc` — which happens, because the file
        // is generated as it is read — must not produce a process with a
        // plausible parent and an invented state.
        assert!(parse_stat("").is_none());
        assert!(parse_stat("4709 (sleep)").is_none());
        assert!(parse_stat("4709 sleep S 1 1").is_none());
        assert!(parse_stat("4709 (sleep) S 4706").is_none());
    }

    #[test]
    fn resident_memory_is_counted_in_the_kernels_pages_and_not_in_assumed_ones() {
        // The captured statm of the same `cat`: 713 pages mapped, 393 resident.
        let bytes = parse_resident("713 393 367 5 0 124 0\n").expect("a real statm");
        assert_eq!(bytes, 393 * thalyx_syscall::page_size() as u64);
    }

    #[test]
    fn a_kernel_thread_has_no_command_line_and_that_is_not_an_empty_one() {
        // `None` and `Some("")` send a person to different conclusions: the
        // first says "this is the kernel", the second says "somebody ran a
        // program with no name".
        assert_eq!(parse_command(b""), None);
        // `\x00` and not `\0`: written the short way, `\0600` reads to a human —
        // and to clippy — as an octal escape, and the day somebody "fixes" it
        // this test would be about a different byte string than it says.
        assert_eq!(
            parse_command(b"/usr/bin/sleep\x00600\x00"),
            Some("/usr/bin/sleep 600".to_string())
        );
        // Trailing NULs are the ordinary shape and must not become an empty
        // argument on the end.
        assert_eq!(parse_command(b"x\x00\x00\x00"), Some("x".to_string()));
    }

    /// A captured `/proc/meminfo`, head of a real one from 2026-08-23.
    const MEMINFO: &str = "MemTotal:       16461084 kB
MemFree:        15511544 kB
MemAvailable:   15801752 kB
Buffers:           18636 kB
Cached:           525480 kB
SwapCached:            0 kB
Active:            62864 kB
Inactive:         780972 kB
SwapTotal:             0 kB
SwapFree:              0 kB
";

    fn a_proc_with(meminfo: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("meminfo"), meminfo).unwrap();
        dir
    }

    #[test]
    fn free_and_available_stay_two_different_numbers() {
        // The one thing a memory reading has to get right. A healthy Linux
        // keeps `free` near zero on purpose, and a person shown only that
        // concludes the machine is full and starts killing things.
        let dir = a_proc_with(MEMINFO);
        let memory = memory_from(dir.path()).unwrap();
        assert_eq!(memory.total, 16_461_084 * 1024);
        assert_eq!(memory.free, 15_511_544 * 1024);
        assert_eq!(memory.available, 15_801_752 * 1024);
        assert!(memory.available > memory.free, "cache counts as available");
        assert_eq!(memory.in_use(), memory.total - memory.available);
    }

    #[test]
    fn a_kernel_with_no_available_line_is_answered_low_rather_than_optimistically() {
        // Rule 9: the cautious answer. `MemFree` under-reports what a program
        // could get and never over-reports it, so a caller that acts on it
        // decides not to start something it could have started — which is
        // recoverable, unlike the other direction.
        let dir = a_proc_with("MemTotal: 100 kB\nMemFree: 40 kB\n");
        let memory = memory_from(dir.path()).unwrap();
        assert_eq!(memory.available, 40 * 1024);
    }

    #[test]
    fn a_meminfo_with_no_total_is_refused_rather_than_reported_as_no_memory() {
        let dir = a_proc_with("Committed_AS: 12 kB\n");
        assert!(matches!(
            memory_from(dir.path()).unwrap_err(),
            ProcError::Unreadable { .. }
        ));
    }

    #[test]
    fn what_is_not_a_number_in_proc_is_not_a_process() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("uptime"), "155.30 569.56\n").unwrap();
        std::fs::write(dir.path().join("meminfo"), MEMINFO).unwrap();
        std::fs::create_dir(dir.path().join("self")).unwrap();
        for (pid, line) in [(12, PLAIN), (7, WEIRD)] {
            let at = dir.path().join(pid.to_string());
            std::fs::create_dir(&at).unwrap();
            std::fs::write(at.join("stat"), line).unwrap();
            std::fs::write(at.join("statm"), "713 393 367 5 0 124 0\n").unwrap();
            std::fs::write(at.join("cmdline"), "x\0").unwrap();
        }

        let running = from_proc(dir.path());
        // Ascending by pid, because that is what a cursor into them names and
        // the window refuses rows that are not.
        assert_eq!(
            running.processes.iter().map(|p| p.pid).collect::<Vec<_>>(),
            vec![7, 12]
        );
        assert_eq!(running.processes[0].name, "we (ird) x");
        assert!(running.unreadable.is_empty());
        assert_eq!(running.gone, 0);
    }

    #[test]
    fn a_process_that_ended_mid_walk_is_counted_and_not_reported_as_broken() {
        // It is what processes do, so it is not an error — but it is not
        // nothing either: two readings that disagree about how many there are
        // need a stated reason, or the difference reads as a defect.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("uptime"), "155.30 569.56\n").unwrap();
        std::fs::create_dir(dir.path().join("99")).unwrap(); // no stat: it is gone
        let at = dir.path().join("12");
        std::fs::create_dir(&at).unwrap();
        std::fs::write(at.join("stat"), PLAIN).unwrap();

        let running = from_proc(dir.path());
        assert_eq!(running.processes.len(), 1);
        assert_eq!(running.gone, 1);
    }

    // ─────────────────────────────────────────────── against the real kernel

    #[test]
    fn this_very_process_is_in_the_list_the_kernel_gives() {
        // Rule 1. Everything above parses text; this asks the kernel.
        let mine = std::process::id() as i32;
        let running = running();
        let me = running
            .processes
            .iter()
            .find(|process| process.pid == mine)
            .expect("the test's own process is running");
        assert!(me.resident > 0, "a live process occupies memory");
        assert!(me.threads >= 1);
        assert_eq!(me.parent, std::os::unix::process::parent_id() as i32);
    }

    #[test]
    fn stopping_a_process_stops_it_and_the_kernel_agrees_it_is_gone() {
        // The claim that matters, checked with something that is not Thalyx:
        // `waitpid` through the standard library, which is the kernel saying
        // the child died and how.
        let mut child = std::process::Command::new("sleep")
            .arg("600")
            .spawn()
            .expect("sleep(1) is present");
        let pid = child.id() as i32;

        let stopped = stop(pid, false).expect("a process this test just started");
        assert_eq!(stopped.was.pid, pid);
        assert_eq!(stopped.signal, "terminate");
        assert_eq!(stopped.was.name, "sleep");

        let status = child.wait().expect("reaping it");
        assert!(!status.success(), "it did not exit on its own terms");
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(libc_sigterm()),
            "it was asked to stop rather than made to"
        );
    }

    #[test]
    fn forcing_it_sends_the_signal_nothing_can_catch() {
        let mut child = std::process::Command::new("sleep")
            .arg("600")
            .spawn()
            .expect("sleep(1) is present");
        let stopped = stop(child.id() as i32, true).expect("a process this test just started");
        assert_eq!(stopped.signal, "kill");

        let status = child.wait().expect("reaping it");
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(9)
        );
    }

    /// 15, everywhere Linux runs. Written as a function so the number appears
    /// once and next to the reason it is that number.
    fn libc_sigterm() -> i32 {
        15
    }

    #[test]
    fn init_and_this_session_are_refused_before_any_signal_is_sent() {
        // Not a policy about who owns the machine — Cesar owns it. It is that
        // both of these have a verb of their own that does the thing properly,
        // and a signal does it in the one way that leaves nothing said.
        assert!(matches!(stop(1, true).unwrap_err(), ProcError::IsInit(1)));
        let mine = std::process::id() as i32;
        assert!(matches!(
            stop(mine, false).unwrap_err(),
            ProcError::IsSelf(_)
        ));
    }

    #[test]
    fn zero_and_negative_are_refused_rather_than_taken_as_a_whole_group() {
        // `kill(2)` reads 0 as "every process I can reach" and a negative
        // number as a process group. Nobody types either on purpose, and both
        // are how one wrong keystroke takes down more than it named.
        assert!(matches!(
            stop(0, true).unwrap_err(),
            ProcError::NotANumber(_)
        ));
        assert!(matches!(
            stop(-1, true).unwrap_err(),
            ProcError::NotANumber(_)
        ));
    }

    #[test]
    fn a_number_no_process_holds_is_refused_and_not_reported_as_stopped() {
        // Picked high and checked, rather than assumed free: on a machine that
        // happened to be running it, a test that skipped the check would kill
        // something real.
        let mut candidate = 4_000_000;
        while describe(candidate).is_ok() {
            candidate += 1;
        }
        assert!(matches!(
            stop(candidate, false).unwrap_err(),
            ProcError::NoSuchProcess(_)
        ));
    }

    #[test]
    fn the_memory_the_kernel_reports_is_a_machine_that_could_exist() {
        let memory = memory().expect("this machine has a /proc/meminfo");
        assert!(memory.total > 0);
        assert!(memory.available <= memory.total);
        assert!(memory.free <= memory.total);
        assert!(memory.swap_free <= memory.swap_total);
    }
}
