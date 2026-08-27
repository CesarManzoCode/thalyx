//! The one thing a test may not do to the machine it is measuring.
//!
//! Rule 11 of `CLAUDE.md`, found on 2026-08-27 and then found again the same
//! day, which is why it lives here instead of inside one test file.
//!
//! `THALYX_ROOT` gives a session its own store and isolates it from **nothing
//! else**. The kernel guard is four bytes in bpffs, beside
//! `KernelStore::DEFAULT_MAP`, and it belongs to the machine running the
//! suite — no environment variable moves it. So `negar` typed at a real prompt
//! by a test does what `negar` is for: on Cesar's machine, as root and with
//! `thalyx-lsm` attached, it armed his kernel, and every stage of `verify.sh`
//! after the suite measured a machine nobody had asked for.
//!
//! The first time, the culprit was `the_guard_can_be_switched.rs`, which types
//! the verb on purpose. The second time it was `catalogue_is_true.rs`, which
//! types **every name the catalogue advertises** and had no idea one of them
//! was that one. That is the reason this is a shared module and not a fix in
//! one place: the danger is not "a test about the guard", it is any test that
//! reaches the prompt.

#![allow(dead_code)]

use thalyx_permd::{Enforcement, PolicyStore};

/// Whether a `negar` typed here would move the guard of this machine.
///
/// Asked exactly as `crate::guard::set` asks it, and asked of the kernel: that
/// verb writes when the mode flag reads as something and refuses without
/// writing when it does not, so this is the same boundary and not a guess at
/// it. Deliberately not an existence check on the pin — bpffs is mode 700, and
/// a path test answers «missing» for a map that is there, which is the mistake
/// that once made this project's tooling read as disarmed while it was armed.
pub fn the_guard_of_this_machine_is_real() -> bool {
    would_switch_this_machine(&thalyx_permd::KernelStore::default_map().enforcement())
}

/// The decision, apart from the reading, so that something can check it.
///
/// The reading needs BPF and this container has none, so the half that can be
/// wrong with no kernel at all is the half that gets a test: an `Unreadable`
/// counted as a real guard would skip every check that consults this on every
/// machine there is, and they would go on printing NOT PROVEN for as long as
/// anybody let them — a skip nobody asked for looks exactly like a machine
/// that cannot do the check.
pub fn would_switch_this_machine(reading: &Enforcement) -> bool {
    !matches!(reading, Enforcement::Unreadable(_))
}

/// Rule 3: a skip says it skipped, and says what went unproven.
///
/// With no `THALYX_REQUIRE_*` beside it, and that is not an oversight. Every
/// other skip in this project is a machine that can do *less* than the check
/// needs, and the variable exists so a machine that can do it is never quietly
/// let off. This one is the mirror: the machine can do *more*, and what is
/// missing is not a capability but an empty kernel. A variable that turned
/// this skip into a failure would demand that the only machine that matters
/// stop being able to enforce.
///
/// Where the guard is measured instead: §37 of `dev/verify.sh`, which arms the
/// machine on purpose, measures it with `bpftool` rather than with Thalyx, and
/// puts it back however the stage ended.
pub fn not_proven(claim: &str) {
    eprintln!("NOT PROVEN: this machine's kernel guard is real, so {claim}.");
    eprintln!("  Typing it would arm this machine for real, and the next thing");
    eprintln!("  to run would be measuring a kernel nobody asked for. §37 of");
    eprintln!("  dev/verify.sh is where the guard verbs are checked on such a machine.");
}

#[test]
fn a_flag_that_cannot_be_read_is_not_a_guard_these_tests_would_move() {
    assert!(!would_switch_this_machine(&Enforcement::Unreadable(
        "there is no bpffs here".into()
    )));
    // Both of the other two, because the danger is the write and not the mode
    // it would write over: a machine already enforcing is still a machine
    // `negar` reaches.
    assert!(would_switch_this_machine(&Enforcement::Observing));
    assert!(would_switch_this_machine(&Enforcement::Enforcing));
}
