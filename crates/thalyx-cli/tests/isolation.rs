//! What the `module_standard` profile actually does to a running program.
//!
//! Every assertion here asks the confined program what it can see, and
//! compares it with what this test process sees. Asking Thalyx whether it
//! isolated something would prove nothing — the whole class of bug this
//! project keeps finding is the system reporting success for work it did not
//! do.
//!
//! These need a real kernel: namespaces, a cgroup2 mount, seccomp. Where any
//! of it is missing they print `NOT PROVEN` and say they did not run, rather
//! than passing quietly. `THALYX_REQUIRE_CGROUP_TESTS=1` turns that into a
//! failure.

use std::path::PathBuf;
use std::process::{Command, Output};
use thalyx_sandbox::profile::{self, Namespaces};

/// A scratch cgroup2 parent, cleaned up on drop.
struct Arena(PathBuf);

impl Drop for Arena {
    fn drop(&mut self) {
        if let Ok(entries) = std::fs::read_dir(&self.0) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let _ = std::fs::remove_dir(entry.path());
                }
            }
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn arena(label: &str) -> Option<Arena> {
    let mount = match thalyx_sandbox::cgroup::mount_point() {
        Ok(mount) => mount,
        Err(error) => return not_proven(&error.to_string()),
    };

    let path = mount.join(format!("thalyx-iso-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir(&path);

    match std::fs::create_dir(&path) {
        Ok(()) => Some(Arena(path)),
        Err(error) => not_proven(&format!("cannot create {}: {error}", path.display())),
    }
}

fn not_proven(reason: &str) -> Option<Arena> {
    let message = format!("NOT PROVEN: this test needs a writable cgroup2 mount ({reason})");
    assert!(
        std::env::var_os("THALYX_REQUIRE_CGROUP_TESTS").is_none(),
        "{message}"
    );
    eprintln!("{message}");
    eprintln!("  This test did not run. It did not pass.");
    None
}

/// Run a shell command inside the confinement, and return what it saw.
fn confined(arena: &Arena, namespaces: Namespaces, script: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .arg(thalyx_sandbox::launch::ENTER_MARKER)
        .arg(arena.0.join("org.thalyx.demo"))
        .arg(profile::MODULE_STANDARD)
        .arg(namespaces.flags().to_string())
        .arg("/bin/sh")
        .args(["-c", script])
        .output()
        .expect("launch")
}

fn standard() -> Namespaces {
    profile::module_standard().namespaces
}

fn cgroup_in(arena: &Arena) -> thalyx_sandbox::Cgroup {
    thalyx_sandbox::Cgroup::ensure(&arena.0, "org.thalyx.demo").expect("cgroup")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The exit status a seccomp `KILL_PROCESS` produces, seen from outside.
const KILLED_BY_SIGSYS: i32 = 128 + 31;

#[test]
fn a_confined_module_is_the_first_process_of_its_own_pid_namespace() {
    let Some(arena) = arena("pid") else { return };
    let _cgroup = cgroup_in(&arena);

    let inside = confined(&arena, standard(), "echo $$");
    assert!(
        inside.status.success(),
        "{}",
        String::from_utf8_lossy(&inside.stderr)
    );
    assert_eq!(stdout(&inside), "1");

    // The control: this process is emphatically not PID 1.
    assert_ne!(std::process::id(), 1);
}

#[test]
fn a_confined_module_cannot_see_the_processes_on_the_machine() {
    // A module that can enumerate every process on the host can read command
    // lines, and command lines carry secrets people did not mean to publish.
    let Some(arena) = arena("proc") else { return };
    let _cgroup = cgroup_in(&arena);

    let inside = confined(&arena, standard(), "ls /proc | grep -c '^[0-9]*$'");
    assert!(inside.status.success());
    let visible: usize = stdout(&inside).parse().expect("a count");

    let outside = std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .chars()
                .all(|c| c.is_ascii_digit())
        })
        .count();

    assert!(
        visible < outside,
        "the module saw {visible} processes and the host has {outside}; \
         /proc was not remounted for the new namespace"
    );
    assert!(
        visible <= 4,
        "the module saw {visible} processes; it should see itself and little else"
    );
}

#[test]
fn a_confined_module_without_network_permission_has_no_interface_but_loopback() {
    let Some(arena) = arena("net") else { return };
    let _cgroup = cgroup_in(&arena);

    let namespaces = standard();
    assert!(namespaces.network, "the profile should isolate the network");

    let inside = confined(
        &arena,
        namespaces,
        "tail -n +3 /proc/net/dev | awk '{print $1}' | tr -d ' \\n'",
    );
    assert!(inside.status.success());
    assert_eq!(
        stdout(&inside),
        "lo:",
        "the module can see host network interfaces"
    );

    // The control: outside there is more than loopback, so an empty result
    // would not have proved anything on its own.
    let host = std::fs::read_to_string("/proc/net/dev").unwrap();
    assert!(
        host.lines().skip(2).count() > 1,
        "this machine has only loopback, so this test proves nothing here"
    );
}

#[test]
fn a_module_granted_outbound_network_keeps_the_interfaces_it_was_granted() {
    // The bug this exists for: the parent adjusted the profile for the grant
    // and the child re-derived it from the profile name, so a module that was
    // *given* network was put in an empty namespace anyway. Silent, and only
    // visible by asking the module.
    let Some(arena) = arena("net-granted") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let granted = profile::module_standard()
        .for_permissions(&[thalyx_manifest::Permission {
            resource: "net".to_string(),
            action: "outbound".to_string(),
            kind: thalyx_manifest::PermissionKind::Persistent,
        }])
        .namespaces;
    assert!(!granted.network);

    let inside = confined(
        &arena,
        granted,
        "tail -n +3 /proc/net/dev | awk '{print $1}' | tr -d ' \\n'",
    );
    assert!(inside.status.success());
    assert_ne!(
        stdout(&inside),
        "lo:",
        "a module granted outbound network was put in an empty network namespace"
    );
}

#[test]
fn a_confined_module_sees_a_hostname_that_tells_it_nothing_about_the_machine() {
    let Some(arena) = arena("uts") else { return };
    let _cgroup = cgroup_in(&arena);

    let inside = confined(&arena, standard(), "hostname");
    assert!(inside.status.success());
    assert_eq!(stdout(&inside), "thalyx-module");

    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap();
    assert_ne!(
        host.trim(),
        "thalyx-module",
        "the host is already called that, so this test proves nothing here"
    );
}

#[test]
fn the_seccomp_filter_kills_a_module_that_creates_a_socket() {
    // `socket` is off the allowlist on purpose. A module without network
    // permission should not be able to construct one to be denied on.
    let Some(arena) = arena("seccomp-socket") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let inside = confined(
        &arena,
        standard(),
        "python3 -c 'import socket; socket.socket()' 2>/dev/null",
    );

    assert_eq!(
        inside.status.code(),
        Some(KILLED_BY_SIGSYS),
        "expected SIGSYS; got {:?}\n{}",
        inside.status,
        String::from_utf8_lossy(&inside.stderr)
    );
}

#[test]
fn the_seccomp_filter_kills_a_module_that_tries_to_change_its_own_confinement() {
    // The syscalls that would let a module undo the sandbox. `unshare` is the
    // sharpest: it is exactly what Thalyx used to build the confinement.
    let Some(arena) = arena("seccomp-unshare") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let inside = confined(&arena, standard(), "unshare --mount true 2>/dev/null");

    assert_eq!(
        inside.status.code(),
        Some(KILLED_BY_SIGSYS),
        "a module was able to call unshare; got {:?}",
        inside.status
    );
}

#[test]
fn ordinary_programs_still_run_under_the_filter() {
    // An allowlist that denied everything would pass every denial test above
    // and be useless. This is the control for all of them.
    let Some(arena) = arena("usable") else { return };
    let _cgroup = cgroup_in(&arena);

    for script in [
        "echo hello",
        "cat /etc/hostname",
        "ls /etc >/dev/null && echo listed",
        "echo x > /tmp/thalyx-test && cat /tmp/thalyx-test",
        "grep -q . /etc/hostname && echo matched",
        "python3 -c 'print(sum(range(100)))'",
    ] {
        let inside = confined(&arena, standard(), script);
        assert!(
            inside.status.success(),
            "`{script}` failed under the filter: {:?}\n{}",
            inside.status,
            String::from_utf8_lossy(&inside.stderr)
        );
    }
}

#[test]
fn a_program_launched_with_no_namespaces_at_all_still_lands_in_the_cgroup() {
    // The diagnostic profile: the launch ordering on its own, with nothing
    // else layered on. It is what isolates the failure when a confined run
    // goes wrong.
    let Some(arena) = arena("diagnostic") else {
        return;
    };
    let cgroup = cgroup_in(&arena);

    let output = Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .arg(thalyx_sandbox::launch::ENTER_MARKER)
        .arg(cgroup.path())
        .arg(profile::DIAGNOSTIC)
        .arg("0")
        .arg("/bin/sh")
        .args(["-c", "grep '^0::' /proc/self/cgroup"])
        .output()
        .expect("launch");

    assert!(output.status.success());
    assert!(
        stdout(&output).ends_with("org.thalyx.demo"),
        "ran in `{}`, not in its cgroup",
        stdout(&output)
    );
}
