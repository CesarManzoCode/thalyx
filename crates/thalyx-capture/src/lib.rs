//! `thalyx-capture` — catching what a verb says, so the screen can show it.
//!
//! Every verb in this program answers by printing. That is not an accident of
//! how they were written — it is what makes them the same code for a person at
//! a terminal, for a program reading one object per line, and now for the
//! screen. `vault/02-Arquitectura/La-Pantalla.md` says the screen costs the
//! system nothing precisely because it reuses those answers rather than growing
//! a second set of them.
//!
//! So the screen does not ask the verbs to change. It moves the descriptor they
//! print to.
//!
//! ## Why the descriptor and not a `Write` handed down
//!
//! Threading a writer through six hundred lines of dispatch and every function
//! under it would be the large edit that the first screen delivery deliberately
//! did not make, and it would still miss the half that matters most: `correr`
//! and `ejecutar` start **other programs**, and a module's output is on file
//! descriptor 1 of a process this one does not control. Redirecting the
//! descriptor catches those too, for free, and catches anything a future verb
//! prints without that verb having to know a screen exists.
//!
//! ## The half that is not about output at all
//!
//! Stdin is redirected to `/dev/null`, and that is the part that keeps the
//! machine alive. Several verbs stop and ask — `instalar`, `observar`,
//! `instalar-en`, `ejecutar` — and every one of them asks by reading a line
//! from a terminal, having first checked `is_terminal`. Under the screen that
//! check would say yes, the question would be printed into a buffer nobody can
//! see, and the machine would sit there forever with no keyboard left to answer
//! it: a hang with a picture on it. With `/dev/null` on descriptor 0 those
//! checks say no and every one of them takes the refusal path it already has —
//! which is rule 9 of `CLAUDE.md`, fail closed, using the code that was already
//! written and already tested rather than a new branch that says the same
//! thing.
//!
//! ## Why this is a crate and not a module
//!
//! Because of what the test below found: descriptors 0, 1 and 2 are the
//! process's, so a test that redirects them has changed the process every other
//! test is being measured in — rule 11 of `CLAUDE.md`, in a place it had not
//! been looked for. A crate of its own is a test binary of its own.
//!
//! ## Fail closed here as well
//!
//! If the redirection cannot be set up, the body **does not run**. Running it
//! anyway would print onto a console that is in graphics mode — invisible — and
//! leave a confirmation prompt waiting on a keyboard that is somewhere else.
//! An error the screen can draw is the cautious answer; a machine that stopped
//! responding is not.

use std::cell::RefCell;
use std::io::{Read, Seek, Write};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::fs::FileExt;

/// The three standard descriptors, put back however this leaves.
///
/// A guard rather than three statements at the end of a function, for the
/// reason [`thalyx_syscall::RawMode`] is one: a `?` or a panic between the
/// redirection and the restoration would leave this process with its output in
/// a buffer nobody reads and its input on `/dev/null`, which on the machine's
/// own session is a machine that has gone quiet for good.
struct Restored {
    input: OwnedFd,
    output: OwnedFd,
    errors: OwnedFd,
}

impl Drop for Restored {
    fn drop(&mut self) {
        // Whatever the verb left in Rust's own buffers belongs to the answer,
        // not to whatever is printed after the descriptors go back.
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        let _ = thalyx_syscall::place_on(self.input.as_raw_fd(), 0);
        let _ = thalyx_syscall::place_on(self.output.as_raw_fd(), 1);
        let _ = thalyx_syscall::place_on(self.errors.as_raw_fd(), 2);
    }
}

thread_local! {
    /// The sink the running verb is printing into, on this thread.
    ///
    /// **Thread-local and not a global, and that is the whole reason this is
    /// safe to have at all.** Rule 11 of `CLAUDE.md` names the failure: a switch
    /// with no owner, whose value is some other check's precondition. `cargo
    /// test` runs one binary's tests as threads inside one process, so a global
    /// here would let one test read the answer another test was in the middle of
    /// writing. The verb runs on the thread that redirected the descriptors —
    /// `screen::run_one` calls `session::act_on` directly — so the thread is
    /// exactly the right scope, and a thread that never captured anything sees
    /// `None` rather than somebody else's buffer.
    static SAID: RefCell<Option<std::fs::File>> = const { RefCell::new(None) };
}

/// The sink, lent to the thread for the duration of one verb and taken back.
///
/// A guard and not two statements, for the reason [`Restored`] is one: the body
/// of a verb can panic, and a lend that outlived the capture would leave the
/// next question on this thread reading a buffer nobody is writing to any more.
struct Lent {
    /// `None` only between [`Lent::take`] and the drop that follows it.
    previous: Option<std::fs::File>,
}

impl Lent {
    fn of(sink: std::fs::File) -> Self {
        let previous = SAID.with(|said| said.borrow_mut().replace(sink));
        Self { previous }
    }

    /// Take the sink back to read the whole answer out of it.
    fn take(mut self) -> std::fs::File {
        let sink = SAID
            .with(|said| said.borrow_mut().take())
            .expect("the sink this capture lent out");
        // Whatever was lent before this capture began goes back, which is what
        // makes one verb captured inside another give the outer one its buffer
        // back rather than nothing.
        SAID.with(|said| *said.borrow_mut() = self.previous.take());
        sink
    }
}

impl Drop for Lent {
    fn drop(&mut self) {
        // Only reached when `take` was never called — that is, when the body
        // unwound. The lend still has to come back.
        SAID.with(|said| *said.borrow_mut() = self.previous.take());
    }
}

/// What the verb running on this thread has printed **so far**, if one is.
///
/// This exists for the one thing the screen could not do without it: a verb that
/// stops to ask has already printed the reason it is asking, and under the
/// screen that reason is in a buffer nobody has read yet. Drawing the question
/// without it would put *«Type the disk's path to confirm»* on the glass with no
/// disk named anywhere near it — a confirmation with its context missing, which
/// is the one thing `Camino-Confiable.md` says a confirmation may never be.
///
/// `None` means no capture is running on this thread, which is a different fact
/// from an empty answer and is reported as one — rule 10.
///
/// Read with `pread` and not by seeking. Descriptors 1 and 2 point at *this
/// same open file*, so its position is the position the verb's next `println!`
/// writes at — reading it by rewinding means moving that, and the only thing
/// putting it back is the read happening to run all the way to the end.
///
/// That was written here first as *«a seek would land the rest of the answer on
/// top of what it had already said»*, and the test written to prove it **did
/// not fail** when the seek was put back: `read_to_end` leaves the position at
/// the end, which is where it started. Rule 5 — the instrument includes the
/// harness, and a justification that survives being disproved is a story. The
/// true reason is narrower and still decides it: `pread` cannot move that
/// position at all, so it holds for a partial read, for a read that stops on an
/// error halfway, and for anyone who later reads only the tail.
pub fn said_so_far() -> Option<String> {
    SAID.with(|said| {
        let borrowed = said.borrow();
        let sink = borrowed.as_ref()?;

        // Anything sitting in Rust's own buffer has not reached the file yet,
        // and the last line before a question is exactly the line most likely to
        // be sitting there: `print!("  Type the disk's path: ")` has no newline
        // to flush it.
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();

        let len = sink.metadata().ok()?.len();
        let mut bytes = vec![0u8; len as usize];
        let mut filled = 0usize;
        while filled < bytes.len() {
            match sink.read_at(&mut bytes[filled..], filled as u64) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
        bytes.truncate(filled);
        Some(String::from_utf8_lossy(&bytes).into_owned())
    })
}

/// Run something and hand back everything it printed, on either stream.
///
/// Both streams into one buffer and in order, because that is what the person
/// would have seen at a terminal. A verb that says why it refused on stderr and
/// what it did on stdout is one answer, and splitting it would put the reason
/// somewhere else on the screen from the thing it is the reason for.
pub fn what_it_says<T>(body: impl FnOnce() -> T) -> std::io::Result<(T, String)> {
    let sink = std::fs::File::from(thalyx_syscall::memory_file("thalyx-answer")?);
    // Read-only: nothing should be writing to stdin, and a descriptor opened
    // for writing would let a verb that got confused about which way its own
    // stream points scribble somewhere.
    let quiet = std::fs::OpenOptions::new().read(true).open("/dev/null")?;

    // Before the swap, or what is sitting in Rust's buffer from before the verb
    // ran ends up inside the verb's answer.
    std::io::stdout().flush()?;
    std::io::stderr().flush()?;

    let saved = Restored {
        input: thalyx_syscall::duplicate(std::io::stdin().as_fd())?,
        output: thalyx_syscall::duplicate(std::io::stdout().as_fd())?,
        errors: thalyx_syscall::duplicate(std::io::stderr().as_fd())?,
    };

    thalyx_syscall::place_on(quiet.as_raw_fd(), 0)?;
    thalyx_syscall::place_on(sink.as_raw_fd(), 1)?;
    thalyx_syscall::place_on(sink.as_raw_fd(), 2)?;

    // The sink is lent to the thread for as long as the verb runs, so that a
    // confirmation raised from inside it can read back the context the verb has
    // already printed. See `said_so_far`.
    let lent = Lent::of(sink);

    let outcome = body();

    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    drop(saved);

    let mut file = lent.take();
    file.rewind()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    // Lossy on purpose: a module nobody signed can print anything at all, and a
    // screen that refused to draw an answer because a foreign program emitted a
    // stray byte would be a screen that hides the output of exactly the programs
    // worth watching.
    Ok((outcome, String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, in a crate of its own, on purpose.
    ///
    /// Rule 11's shape applied to file descriptors instead of to the kernel:
    /// **0, 1 and 2 belong to the process**, and `cargo test` runs the tests of
    /// one binary as threads inside one of them. This started life as a module
    /// inside `thalyx-cli` and failed exactly that way — beside a hundred and
    /// thirty other tests it caught `libtest`'s own progress lines instead of
    /// what it had printed, while passing on its own and passing with
    /// `--test-threads=1`. A redirection with no owner is a redirection every
    /// other test in the process is standing in.
    ///
    /// So the code lives in its own crate, which gives it its own test process,
    /// and the parts run in order inside one test rather than as four that could
    /// be scheduled against each other.
    #[test]
    fn the_screen_can_catch_an_answer_without_losing_the_streams_it_borrowed() {
        // `write!` to `io::stdout()` rather than `println!`, and this is the
        // harness being part of the instrument. `libtest` captures the `print!`
        // family by swapping a **thread-local sink** inside `std::io::_print` —
        // it never touches descriptor 1. So a test written with `println!`
        // would measure `libtest`'s capture and pass whether this file worked
        // or not. `io::stdout()` is the real descriptor in both worlds, and
        // outside a test that is exactly where `println!` ends up.
        let (value, said) = what_it_says(|| {
            let _ = writeln!(std::io::stdout(), "ok  store  /dev/sdb2");
            let _ = writeln!(std::io::stderr(), "refused  fs/write  /");
            41 + 1
        })
        .expect("the streams could not be redirected");

        assert_eq!(value, 42);
        assert!(said.contains("ok  store  /dev/sdb2"), "{said:?}");
        // Both streams, in one answer: a verb that says what it did on one and
        // why it refused on the other is one thing the person read, and the
        // screen has one place to put it.
        assert!(said.contains("refused  fs/write  /"), "{said:?}");

        // The half that matters more than the capture. If the restoration were
        // broken the assertions above would still pass, and every later write
        // in this process would go into a buffer nobody reads.
        let (_, again) = what_it_says(|| {
            let _ = writeln!(std::io::stdout(), "second");
        })
        .expect("the streams were not given back");
        assert!(again.contains("second"), "{again:?}");

        // The whole reason stdin is redirected. Every confirmation in this
        // program guards itself with `is_terminal`, so this one property is what
        // turns «the machine hangs with a picture on it» into «the verb refused
        // and said why».
        use std::io::IsTerminal;
        let (was_a_terminal, _) =
            what_it_says(|| std::io::stdin().is_terminal()).expect("redirectable");
        assert!(
            !was_a_terminal,
            "a verb run under the screen still thinks it can stop and ask a question"
        );

        // `correr` and `ejecutar` start processes this one does not control, and
        // their output is on descriptor 1 of somebody else. A capture that only
        // caught this process's own writing would draw an empty answer for the
        // two verbs whose entire point is running something.
        let (status, from_elsewhere) = what_it_says(|| {
            std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg("echo from-another-program")
                .status()
        })
        .expect("redirectable");
        assert!(status.expect("sh ran").success());
        assert!(
            from_elsewhere.contains("from-another-program"),
            "{from_elsewhere:?}"
        );

        // Nothing is being captured out here, and that is not the same fact as
        // an empty answer — rule 10, in the one place a caller could act on the
        // difference: a confirmation that got `Some("")` would draw a question
        // with no context, and one that got `None` knows it is at a terminal.
        assert!(said_so_far().is_none());

        // What the screen needs and could not have without this: the context a
        // verb printed *before* it stopped to ask.
        let (_, whole) = what_it_says(|| {
            let _ = writeln!(std::io::stdout(), "/dev/sdb  7 GiB  btrfs `fedora`");
            // No newline, exactly like every confirmation in this program. If
            // `said_so_far` did not flush, this line — the question itself —
            // would be the one line missing from the question.
            let _ = write!(std::io::stdout(), "  Type the disk's path to confirm: ");

            let asked = said_so_far().expect("a capture is running");
            assert!(asked.contains("/dev/sdb  7 GiB"), "{asked:?}");
            assert!(asked.ends_with("to confirm: "), "{asked:?}");

            // The half that a seek would have broken. Descriptors 1 and 2 point
            // at this same open file, so reading it by seeking to the start
            // would move the position this next line writes at, and it would
            // land on top of the disk. Checked on the whole answer below rather
            // than here, because that is where the damage would show.
            let _ = writeln!(std::io::stdout(), "\n  not confirmed");
        })
        .expect("redirectable");
        // Not proof that `pread` was needed — a rewind-and-read-to-end passes
        // this too, which is how the comment on `said_so_far` got corrected.
        // What it does hold is the property the screen depends on: asking what
        // has been said leaves the answer being written intact and in order.
        assert!(
            whole.starts_with("/dev/sdb  7 GiB  btrfs `fedora`\n  Type the disk's path"),
            "reading the answer disturbed what the verb was writing: {whole:?}"
        );
        assert!(whole.trim_end().ends_with("not confirmed"), "{whole:?}");

        // And the lend came back when the capture ended.
        assert!(said_so_far().is_none());

        // Rule 9 applied to this file's own failure mode: the restoration is a
        // `Drop` and not three statements at the end of the function, and the
        // difference is only ever visible when something unwinds through it.
        let panicked = std::panic::catch_unwind(|| {
            let _ = what_it_says(|| panic!("a verb came apart"));
        });
        assert!(panicked.is_err());
        let (_, after_the_panic) = what_it_says(|| {
            let _ = writeln!(std::io::stdout(), "still here");
        })
        .expect("a panic took the streams with it");
        assert!(
            after_the_panic.contains("still here"),
            "{after_the_panic:?}"
        );
        // The lend is a `Drop` for the same reason the restoration is: a verb
        // that comes apart mid-question would otherwise leave this thread
        // holding a sink nobody writes to, and the next confirmation would draw
        // the previous verb's output as its context.
        assert!(said_so_far().is_none());
    }
}
