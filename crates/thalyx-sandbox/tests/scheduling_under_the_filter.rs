//! The scheduling guard, asked of a real program instead of of the filter.
//!
//! `seccomp.rs` already tests the guard by evaluating the compiled program
//! against a syscall number and an argument, and that test passed on the day a
//! confined `chrt` was killed with `SIGSYS` setting an ordinary policy. It
//! passed because it asked the only question it knew about: *given a call to
//! `sched_setscheduler` with this policy, what does the filter answer?* A real
//! program does not arrive at that call first. `chrt` reads the legal priority
//! range before it sets anything —
//!
//! ```text
//! sched_get_priority_min(SCHED_IDLE)     = 0
//! sched_get_priority_max(SCHED_IDLE)     = 0
//! sched_setscheduler(0, SCHED_IDLE, [0]) = 0
//! ```
//!
//! — and neither of the first two lines was on the allowlist. Rule 1 of
//! `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md`, exactly: the defect came
//! from running the system, and a test of what was produced could not see it.
//!
//! ## Why `--idle` and not `--other`, which is the obvious one to write
//!
//! Because `chrt --other` stopped being one instrument and became two.
//! util-linux 2.41 added `supports_custom_slice`, and with it `--other` and
//! `--batch` set the policy through **`sched_setattr`** rather than
//! `sched_setscheduler`. The policy then lives in a struct behind a pointer,
//! and a seccomp filter cannot follow a pointer — so that call cannot be
//! guarded by argument the way `sched_setscheduler` is, and Thalyx denies it
//! outright rather than open a second door to a real-time policy.
//!
//! The first version of this test used `--other`, passed on util-linux 2.39 in
//! the development container, and failed on 2.41 on Cesar's machine. It read as
//! Thalyx denying what it exists to permit, and it was two programs wearing one
//! name — the rule about an instrument's version, in
//! `Estrategia-de-Pruebas.md`, for the second time.
//!
//! `--idle` is neither `SCHED_DEADLINE` nor a custom-slice policy, so every
//! util-linux to date sets it with `sched_setscheduler`, and `SCHED_IDLE` is
//! one of the three policies the guard permits. It is a real foreign program
//! walking the whole road to the guarded call, which is the property that made
//! `chrt` worth using and the one `--other` no longer has.
//!
//! ## Why it is worth two processes
//!
//! A seccomp filter is irrevocable and inherited, so installing one inside the
//! test harness would confine every test that runs after it in the same
//! process. This test re-executes its own binary with [`ARM`] set, and that
//! child installs the filter, runs `chrt`, and exits with what happened to it.
//!
//! ## What each column is for, and why the denial needs the other one
//!
//! Rule 4: a denial with no baseline proves nothing. Here the trap is sharper
//! than usual, because the two columns are the *same program*. When the
//! priority queries were missing, `chrt --fifo 1 true` died with `SIGSYS` too —
//! on `sched_get_priority_min`, before it ever named a real-time policy — and
//! that reads exactly like the guard refusing it. So the real-time column is
//! only allowed to mean anything when the ordinary column came back 0, and this
//! is one test rather than two so that they cannot be read apart.
//!
//! The outside controls are the other half: a `chrt` that cannot set a
//! real-time policy anywhere on this machine would make the denial inside the
//! sandbox free.

use std::os::unix::process::ExitStatusExt;
use std::process::Command;

/// Set on the re-executed child, naming which column it is running.
///
/// Its presence is also what keeps [`the_arm_that_runs_under_the_filter`] inert
/// during an ordinary run of the suite.
const ARM: &str = "THALYX_SECCOMP_ARM";

/// The filter would not install, which is a fact about the kernel, not the guard.
const NO_SECCOMP: i32 = 90;
/// `chrt` could not be started at all.
const NO_CHRT: i32 = 91;

/// The ordinary column: a policy every util-linux still sets with the call the
/// guard guards. See the module docs for why it is not `--other`.
const ORDINARY: [&str; 3] = ["--idle", "0", "true"];
/// The column the guard exists for: holding a processor against the machine.
const REAL_TIME: [&str; 3] = ["--fifo", "1", "true"];

/// One column, run under the real filter, in a process of its own.
///
/// Not a claim — it is the other half of the test below, and it does nothing at
/// all unless that test re-executed this binary. `std::process::exit` carries
/// the answer out, because a panic here would arrive as libtest's exit code and
/// say nothing about what happened to `chrt`.
#[test]
fn the_arm_that_runs_under_the_filter() {
    let Ok(column) = std::env::var(ARM) else {
        return;
    };

    if thalyx_sandbox::seccomp::module_standard()
        .install()
        .is_err()
    {
        std::process::exit(NO_SECCOMP);
    }

    let arguments = match column.as_str() {
        "ordinary" => ORDINARY,
        "realtime" => REAL_TIME,
        other => panic!("unknown column `{other}`"),
    };

    match Command::new("chrt").args(arguments).status() {
        Ok(status) => std::process::exit(match (status.code(), status.signal()) {
            (Some(code), _) => code,
            (None, Some(signal)) => 128 + signal,
            (None, None) => NO_CHRT,
        }),
        Err(_) => std::process::exit(NO_CHRT),
    }
}

/// What `chrt` did in a process the filter was installed in.
///
/// `None` when the filter could not be installed on this kernel; `Some(status)`
/// otherwise, where a status of 128 + 31 is `SIGSYS` and nothing else produces
/// one — the filter's only action is to kill.
fn under_the_filter(column: &str) -> Option<i32> {
    let binary = std::env::current_exe().expect("this test binary");
    let output = Command::new(&binary)
        .args([
            "the_arm_that_runs_under_the_filter",
            "--exact",
            "--nocapture",
        ])
        .env(ARM, column)
        .output()
        .expect("re-executing this test binary");

    // The arm encodes what happened to `chrt` in its own exit code, so an arm
    // that was itself killed — by the filter, on a call libtest makes after
    // installing it — must not be read as `chrt` being killed. That would be
    // the harness answering a question about Thalyx.
    if let Some(signal) = output.status.signal() {
        panic!(
            "the arm itself died on signal {signal} rather than reporting what \
             happened to chrt, so the filter denies something the test harness \
             needs and this says nothing about the guard"
        );
    }

    match output.status.code() {
        Some(NO_SECCOMP) => None,
        Some(NO_CHRT) => panic!("the arm could not start chrt at all"),
        Some(code) => Some(code),
        None => unreachable!("a process that neither exited nor was signalled"),
    }
}

/// Whether `chrt` can do this outside any sandbox, which is what makes the
/// answer inside one mean something.
fn works_outside(arguments: &[&str]) -> bool {
    Command::new("chrt")
        .args(arguments)
        .status()
        .is_ok_and(|status| status.success())
}

/// Say a claim could not be checked here — or fail, if this machine was
/// declared able to check it.
///
/// Rule 3: one variable per requirement, and a skip that stays silent is a test
/// reporting the absence of a check as the absence of a risk.
fn not_proven(why: &str) {
    if std::env::var_os("THALYX_REQUIRE_SECCOMP_TESTS").is_some() {
        panic!(
            "THALYX_REQUIRE_SECCOMP_TESTS is set and {why}. Unset it and accept \
             that this claim is NOT PROVEN on this machine."
        );
    }
    eprintln!(
        "NOT PROVEN: {why}, so nothing here ran a real program under the real \
         filter. Set THALYX_REQUIRE_SECCOMP_TESTS=1 to make this a failure \
         instead of a skip."
    );
}

/// `SIGSYS`, as an exit status seen from the parent. The only thing the filter
/// does is kill, so this number is the guard and nothing else produces it.
const KILLED_BY_THE_FILTER: i32 = 128 + libc::SIGSYS;

#[test]
fn a_real_program_arranges_its_own_threads_and_cannot_take_the_machine() {
    if !works_outside(&ORDINARY) {
        not_proven("chrt cannot set an ordinary policy outside any sandbox either");
        return;
    }

    let Some(ordinary) = under_the_filter("ordinary") else {
        not_proven("this kernel would not install a seccomp filter");
        return;
    };

    assert_eq!(
        ordinary,
        0,
        "a confined program could not put its own thread on an ordinary policy: \
         `chrt {}` exited {ordinary}, and {KILLED_BY_THE_FILTER} is the filter \
         killing it. The guard exists to permit this call",
        ORDINARY.join(" ")
    );

    // Only now does a denial mean anything: the same program, under the same
    // filter, got all the way through with an ordinary policy.
    if !works_outside(&REAL_TIME) {
        not_proven(
            "chrt cannot set a real-time policy outside the sandbox either, so a \
             refusal inside it would prove nothing",
        );
        return;
    }

    let realtime = under_the_filter("realtime").expect("the filter installed a moment ago");
    assert_eq!(
        realtime,
        KILLED_BY_THE_FILTER,
        "a confined program took a real-time policy and can hold a processor \
         against the machine: `chrt {}` exited {realtime}",
        REAL_TIME.join(" ")
    );
}
