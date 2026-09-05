//! What a compiler tree may do that an ordinary module may not — asked of real
//! operations, under the two real filters, side by side.
//!
//! ## The failure this file exists to stop
//!
//! On 2026-08-30 `dev/verify.sh` ran Cargo and rust-analyzer under a kernel
//! that was finally *denying* rather than watching, and both died. What the
//! two stages could report was `cargo exited 159` and `` `initialize`: the
//! server stopped``. Neither sentence names a syscall, because a
//! `SECCOMP_RET_KILL_PROCESS` does not tell the program anything — it tells the
//! *kernel's audit log*, which is where the four numbers below were finally
//! read from:
//!
//! ```text
//! comm="cargo"         syscall=73   → flock
//! comm="rustc"         syscall=265  → linkat
//! comm="cargo"         syscall=219  → restart_syscall
//! comm="VfsLoader"     syscall=294  → inotify_init1
//! ```
//!
//! Rule 1 of `Estrategia-de-Pruebas.md`: every one of them came from running
//! the thing. Not one was visible in the source of anything Thalyx wrote.
//!
//! The fifth arrived on 2026-09-05 and had to be read somewhere else, because
//! the guest kernel keeps no audit log worth the name. It came out of `strace`
//! held around a process carrying this very filter, driving a real
//! rust-analyzer over LSP:
//!
//! ```text
//! 5217  socketpair(AF_UNIX, SOCK_SEQPACKET|SOCK_CLOEXEC, 0, [13, 14]) = 0
//! 5217  fork()                            = 57
//! 5217  +++ killed by SIGSYS +++
//! ```
//!
//! — `57` is `SYS_fork`, and `5217` was a thread of the server, so the whole
//! server went with it. What Thalyx reported was `rust-analyzer did not answer:
//! the server stopped listening: Broken pipe … status 159`, one request after
//! a request it had answered correctly.
//!
//! ## Why nothing here had ever asked
//!
//! Rule 12. **glibc's `fork()` is written on top of `clone(2)`**, so a test
//! binary compiled by `dev/verify.sh` — which is glibc, end to end — spawns a
//! child and never issues `SYS_fork` at all. **musl's `fork()` issues it
//! directly**, and the toolchain Thalyx ships is
//! `x86_64-unknown-linux-musl`. That is why the arm below goes through
//! [`thalyx_syscall::fork_directly`] rather than through `std::process`: a
//! `Command::spawn` here would prove that `clone` is permitted, which was never
//! in question, and would report `PROVEN` about the call that kills.
//!
//! ## Why every claim here is two columns
//!
//! Rule 4. `semantic_provider` permitting `flock` proves nothing on its own —
//! a filter that permitted everything would pass that assertion, and so would
//! a machine where seccomp does not install. So each operation is run under
//! **both** filters and the pair is asserted: permitted under the provider's,
//! killed under the module's. The second column is what makes the first mean
//! "this was added deliberately" rather than "nothing is being enforced".
//!
//! [`a_socket_pair_is_not_a_socket`] is the same shape pointed the other way,
//! and it is the one that guards the decree: `socketpair` was added, and a
//! network socket has to stay dead under the provider's filter too, or the
//! addition quietly became the thing `Sandbox-Ejecucion.md` refuses.
//!
//! ## Why it is a re-executed process and not a call
//!
//! A seccomp filter is irrevocable and inherited. Installing one inside the
//! test harness would confine every test that runs after it in the same
//! process — and `cargo test` runs a binary's tests as threads of one process.

use std::os::unix::process::ExitStatusExt;
use std::process::Command;

/// Set on the re-executed child, naming which operation it is to attempt.
const ARM: &str = "THALYX_FILTER_ARM";
/// Which of the two filters the arm installs.
const FILTER: &str = "THALYX_FILTER_WHICH";
/// A directory the arm may write in, made by the parent.
const WORK: &str = "THALYX_FILTER_WORK";

/// The filter would not install: a fact about the kernel, not about the list.
const NO_SECCOMP: i32 = 90;
/// The operation was refused by something that is not the filter.
const REFUSED: i32 = 93;

/// `SIGSYS` as the parent sees it. The filter's only action is to kill, so
/// nothing else on this path produces this number.
const KILLED_BY_THE_FILTER: i32 = 128 + libc::SIGSYS;

/// One operation, under one filter, in a process of its own.
///
/// Not a claim — it does nothing at all unless a test below re-executed this
/// binary. `std::process::exit` carries the answer out, because a panic here
/// would arrive as libtest's exit code and say nothing about the operation.
#[test]
fn the_arm_that_runs_under_a_filter() {
    let Ok(operation) = std::env::var(ARM) else {
        return;
    };
    let work = std::path::PathBuf::from(std::env::var(WORK).expect("a work directory"));

    let allowlist = match std::env::var(FILTER).as_deref() {
        Ok("module") => thalyx_sandbox::seccomp::module_standard(),
        Ok("provider") => thalyx_sandbox::seccomp::semantic_provider(),
        other => panic!("unknown filter `{other:?}`"),
    };
    if allowlist.install().is_err() {
        std::process::exit(NO_SECCOMP);
    }

    // Everything below is written in safe `std`, which is not a stylistic
    // choice: `unsafe` lives in `thalyx-syscall` and nowhere else. It is also
    // the more honest instrument — `File::try_lock` reaching `flock` and
    // `UnixStream::pair` reaching `socketpair` is exactly how the programs that
    // died reached them, rather than a hand-made call nothing really makes.
    let done = match operation.as_str() {
        // Cargo locks `Cargo.lock`, `$CARGO_HOME/.package-cache` and the build
        // directory. Twenty times in the smallest build there is.
        "flock" => {
            std::fs::File::create(work.join("locked")).is_ok_and(|file| file.try_lock().is_ok())
        }
        // `rustc` puts its output in place by hard-linking it.
        "hardlink" => std::fs::write(work.join("from"), b"output")
            .and_then(|()| std::fs::hard_link(work.join("from"), work.join("to")))
            .is_ok(),
        // `std::process` hands the child's `execve` errno back over one of
        // these, on the fork path Cargo's spawns take.
        "socketpair" => std::os::unix::net::UnixStream::pair().is_ok(),
        // What makes the child at the end of that same fork path. Written as
        // the raw call on purpose — see the note at the top of this file about
        // why `std::process::Command` would answer a different question here.
        "fork" => thalyx_syscall::fork_directly()
            .and_then(thalyx_syscall::wait_for)
            .is_ok_and(|status| status == 0),
        // The control. Not a capability being claimed — a door that has to
        // stay shut on both sides of every other column here.
        "network-socket" => std::net::UdpSocket::bind("127.0.0.1:0").is_ok(),
        other => panic!("unknown operation `{other}`"),
    };
    std::process::exit(if done { 0 } else { REFUSED });
}

/// What one operation did under one filter, in a process of its own.
///
/// `None` when the filter would not install on this kernel — which is a skip
/// and not a pass, and is why every caller goes through [`not_proven`].
fn under(filter: &str, operation: &str) -> Option<i32> {
    let held = tempfile::tempdir().expect("a work directory");
    let binary = std::env::current_exe().expect("this test binary");
    let output = Command::new(&binary)
        .args(["the_arm_that_runs_under_a_filter", "--exact", "--nocapture"])
        .env(ARM, operation)
        .env(FILTER, filter)
        .env(WORK, held.path())
        .output()
        .expect("re-executing this test binary");

    match (output.status.code(), output.status.signal()) {
        (Some(NO_SECCOMP), _) => None,
        // The arm reports what happened to the operation in its own exit code,
        // so an arm killed on a call libtest makes *after* installing the
        // filter must never be read as the operation being killed. That would
        // be the harness answering a question about Thalyx — rule 5.
        (Some(code), _) => Some(code),
        (None, Some(signal)) => Some(128 + signal),
        (None, None) => unreachable!("a process that neither exited nor was signalled"),
    }
}

/// Rule 3: a skip that says it skipped, and one variable that demands the check.
fn not_proven(why: &str) {
    if std::env::var_os("THALYX_REQUIRE_SECCOMP_TESTS").is_some() {
        panic!("THALYX_REQUIRE_SECCOMP_TESTS is set and {why}");
    }
    eprintln!(
        "NOT PROVEN: {why}, so nothing here ran a real operation under a real \
         filter. Set THALYX_REQUIRE_SECCOMP_TESTS=1 to make this a failure."
    );
}

/// One capability: permitted for a compiler tree, killed for an ordinary module.
fn only_the_compiler_tree_may(operation: &str, what: &str) {
    let Some(provider) = under("provider", operation) else {
        not_proven("this kernel would not install a seccomp filter");
        return;
    };
    let module = under("module", operation).expect("the filter installed a moment ago");

    assert_eq!(
        provider, 0,
        "a semantic provider could not {what}: the arm exited {provider} \
         (128+31 is SIGSYS, which is the filter killing it). This is the call \
         that stopped the compiler tree, and permitting it is the whole point \
         of the provider having a filter of its own"
    );
    assert_eq!(
        module, KILLED_BY_THE_FILTER,
        "an ordinary module was allowed to {what} (the arm exited {module}). \
         Then the provider's filter is not a superset of the module's — it is \
         the module's, and this file proves nothing about what was added"
    );
}

#[test]
fn only_a_compiler_tree_may_lock_a_file_it_already_holds() {
    only_the_compiler_tree_may("flock", "lock a file it opened");
}

#[test]
fn only_a_compiler_tree_may_hard_link_inside_what_it_was_given() {
    only_the_compiler_tree_may("hardlink", "hard-link a file it wrote");
}

#[test]
fn only_a_compiler_tree_may_make_a_pair_of_connected_descriptors() {
    only_the_compiler_tree_may("socketpair", "make a socket pair");
}

#[test]
fn only_a_compiler_tree_may_fork_a_child_of_its_own() {
    // The call the shipping rust-analyzer makes, a few seconds after it loads
    // a workspace, to spawn the `cargo` that runs build scripts and the
    // proc-macro server. It is the one that killed the server on the machine
    // that actually denies, and it is the one no glibc test had ever made.
    only_the_compiler_tree_may("fork", "fork a child of its own");
}

#[test]
fn a_socket_pair_is_not_a_socket() {
    // The decree `socketpair` was added against, checked rather than promised.
    // A guard on the domain is only worth anything if the domains it leaves out
    // are still dead, and the cheapest way for this addition to have gone wrong
    // would have been to write `.allow(SYS_socket)` next to it and never notice.
    let Some(provider) = under("provider", "network-socket") else {
        not_proven("this kernel would not install a seccomp filter");
        return;
    };
    let module = under("module", "network-socket").expect("the filter installed a moment ago");

    assert_eq!(
        provider, KILLED_BY_THE_FILTER,
        "a semantic provider constructed a network socket (the arm exited \
         {provider}). `socketpair` was permitted for a pair of descriptors that \
         cannot leave the machine; if `socket` came with it, the provider has \
         the network `Sandbox-Ejecucion.md` denies it"
    );
    assert_eq!(
        module, KILLED_BY_THE_FILTER,
        "and an ordinary module still may not either"
    );
}
