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

use std::io::{Read, Seek, Write};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};

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

/// Run something and hand back everything it printed, on either stream.
///
/// Both streams into one buffer and in order, because that is what the person
/// would have seen at a terminal. A verb that says why it refused on stderr and
/// what it did on stdout is one answer, and splitting it would put the reason
/// somewhere else on the screen from the thing it is the reason for.
pub fn what_it_says<T>(body: impl FnOnce() -> T) -> std::io::Result<(T, String)> {
    let sink = thalyx_syscall::memory_file("thalyx-answer")?;
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

    let outcome = body();

    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    drop(saved);

    let mut file = std::fs::File::from(sink);
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
    }
}
