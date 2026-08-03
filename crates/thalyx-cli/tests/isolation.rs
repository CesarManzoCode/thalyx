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

use std::path::{Path, PathBuf};
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
        Ok(()) => {
            // Hand the controllers down.
            //
            // A real system has systemd delegating these at the root, so the
            // `thalyx` cgroup created under it inherits them. A scratch arena
            // is a hierarchy nobody set up, and without this the resource
            // limits fail — reporting a defect in Thalyx for something the
            // test harness had not done.
            let _ = std::fs::write(path.join("cgroup.subtree_control"), "+memory +pids");
            Some(Arena(path))
        }
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
///
/// `/bin/sh` rather than a module of our own, and therefore no pivot: the
/// root filesystem is exercised separately, by tests that build a module tree
/// to be pivoted into.
fn confined(arena: &Arena, namespaces: Namespaces, script: &str) -> Output {
    launch(
        arena,
        namespaces,
        None,
        Path::new("/bin/sh"),
        &["-c", script],
    )
}

fn launch(
    arena: &Arena,
    namespaces: Namespaces,
    rootfs: Option<thalyx_sandbox::RootFs>,
    program: &Path,
    args: &[&str],
) -> Output {
    let spec = thalyx_sandbox::LaunchSpec {
        cgroup: arena.0.join("org.thalyx.demo"),
        profile: profile::MODULE_STANDARD.to_string(),
        namespaces,
        rootfs,
        program: program.to_path_buf(),
        uid: None,
    };

    Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .args(
            thalyx_sandbox::launch::argv(
                thalyx_sandbox::launch::ENTER_MARKER,
                &spec,
                &thalyx_sandbox::launch::to_args(args),
            )
            .expect("argv"),
        )
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

    if !which("python3") {
        eprintln!("NOT PROVEN: python3 is not installed, and it is what asks for a socket here.");
        eprintln!("  This test did not run. It did not pass.");
        return;
    }

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
    //
    // "Runs under the filter" means the filter did not kill it — not that it
    // exited zero. The first version asserted success, and one of its scripts
    // was `grep -q . /etc/hostname`: `-q` turns an error opening the file into
    // exit 1, indistinguishable from "no lines matched", and the test reported
    // the seccomp filter for something that was not the filter at all. That is
    // the `curl -s` mistake from the LSM demo, in a different costume.
    let Some(arena) = arena("usable") else { return };
    let _cgroup = cgroup_in(&arena);

    // Self-contained wherever possible: a script that reads a host file is a
    // script whose result depends on the machine it runs on.
    let mut scripts = vec![
        "echo hello",
        "echo x > /tmp/thalyx-probe && cat /tmp/thalyx-probe",
        "printf 'a\nb\n' > /tmp/thalyx-lines && grep b /tmp/thalyx-lines",
        "mkdir -p /tmp/thalyx-dir && ls /tmp/thalyx-dir",
        "cat /etc/passwd > /dev/null",
        "ls /etc > /dev/null",
    ];
    if which("python3") {
        scripts.push("python3 -c 'print(sum(range(100)))'");
    }

    for script in scripts {
        let inside = confined(&arena, standard(), script);

        assert!(
            !killed_by_signal(&inside),
            "the filter killed `{script}`: {:?}\n{}",
            inside.status,
            String::from_utf8_lossy(&inside.stderr)
        );

        // A non-zero exit is not a filter problem, but it does mean this
        // script proved less than it was meant to — so it is reported with
        // everything the program said, rather than swallowed.
        assert!(
            inside.status.success(),
            "`{script}` ran under the filter and exited {:?}.\n  \
             stdout: {}\n  stderr: {}\n  \
             The filter did not kill it, so this is the script or the machine, \
             not seccomp.",
            inside.status.code(),
            stdout(&inside),
            String::from_utf8_lossy(&inside.stderr).trim()
        );
    }
}

/// Whether a process was killed by a signal, and by which.
///
/// The distinction the control tests rest on: `SIGSYS` means the seccomp
/// filter refused a syscall, and any ordinary non-zero exit means the program
/// ran to completion and disagreed with something.
fn killed_by_signal(output: &Output) -> bool {
    use std::os::unix::process::ExitStatusExt;
    output.status.signal().is_some()
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
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

    let spec = thalyx_sandbox::LaunchSpec {
        cgroup: cgroup.path().to_path_buf(),
        profile: profile::DIAGNOSTIC.to_string(),
        namespaces: Namespaces::NONE,
        rootfs: None,
        program: PathBuf::from("/bin/sh"),
        uid: None,
    };
    let output = Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .args(
            thalyx_sandbox::launch::argv(
                thalyx_sandbox::launch::ENTER_MARKER,
                &spec,
                &thalyx_sandbox::launch::to_args(&["-c", "grep '^0::' /proc/self/cgroup"]),
            )
            .expect("argv"),
        )
        .output()
        .expect("launch");

    assert!(output.status.success());
    assert!(
        stdout(&output).ends_with("org.thalyx.demo"),
        "ran in `{}`, not in its cgroup",
        stdout(&output)
    );
}

/// A module tree with a script that reports what it can see.
struct Module {
    directory: tempfile::TempDir,
}

impl Module {
    fn with(script: &str) -> Self {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(directory.path().join("bin")).unwrap();
        let program = directory.path().join("bin/probe");
        std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();

        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();

        std::fs::write(directory.path().join("data.txt"), "the module's own file\n").unwrap();
        Self { directory }
    }

    fn dir(&self) -> &Path {
        self.directory.path()
    }

    fn program(&self) -> PathBuf {
        self.directory.path().join("bin/probe")
    }
}

/// Launch a module pivoted into a root of its own.
fn pivoted(
    arena: &Arena,
    module: &Module,
    grants: &[thalyx_manifest::Permission],
) -> std::io::Result<Output> {
    let rootfs = thalyx_sandbox::RootFs::for_module(module.dir(), grants)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(launch(
        arena,
        standard(),
        Some(rootfs),
        &module.program(),
        &[],
    ))
}

fn grant(path: &Path, action: &str) -> thalyx_manifest::Permission {
    thalyx_manifest::Permission {
        resource: path.display().to_string(),
        action: action.to_string(),
        kind: thalyx_manifest::PermissionKind::Persistent,
    }
}

#[test]
fn a_pivoted_module_cannot_reach_the_host_tree() {
    // The gap this closes: a mount namespace isolates the mount *table*, not
    // the files. Before the pivot the module had a namespace of its own and
    // the entire host filesystem inside it.
    let Some(arena) = arena("pivot") else { return };
    let _cgroup = cgroup_in(&arena);

    let canary = tempfile::tempdir().unwrap();
    let secret = canary.path().join("secret");
    std::fs::write(&secret, "should be unreachable").unwrap();

    let module = Module::with(&format!(
        "ls / | tr '\\n' ' '; echo; \
         [ -e {secret} ] && echo CANARY-REACHABLE || echo canary-gone",
        secret = secret.display()
    ));

    let output = pivoted(&arena, &module, &[]).expect("rootfs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let seen = stdout(&output);
    assert!(seen.contains("canary-gone"), "{seen}");
    assert!(
        !seen.contains("home"),
        "the host's /home is visible: {seen}"
    );
    assert!(
        !seen.contains("root "),
        "the host's /root is visible: {seen}"
    );
    assert!(
        seen.contains("module"),
        "the module's own tree is missing: {seen}"
    );
}

#[test]
fn a_pivoted_module_finds_its_own_files_under_a_stable_path() {
    // The module's host path carries a version number. Nothing inside should
    // have to know it, so the tree appears at /module whatever version it is.
    let Some(arena) = arena("pivot-own") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let module = Module::with("cat /module/data.txt");
    let output = pivoted(&arena, &module, &[]).expect("rootfs");

    assert!(output.status.success());
    assert_eq!(stdout(&output), "the module's own file");
}

#[test]
fn a_granted_path_is_reachable_inside_under_its_own_name() {
    let Some(arena) = arena("pivot-grant") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let granted = tempfile::tempdir().unwrap();
    std::fs::write(granted.path().join("note"), "granted content\n").unwrap();

    let module = Module::with(&format!("cat {}/note", granted.path().display()));
    let output = pivoted(&arena, &module, &[grant(granted.path(), "read")]).expect("rootfs");

    assert!(
        output.status.success(),
        "a granted path was not reachable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), "granted content");
}

#[test]
fn a_path_that_was_not_granted_simply_does_not_exist_inside() {
    // The control for the test above. Without it, "the granted path works"
    // would also be true of a sandbox that bound everything.
    let Some(arena) = arena("pivot-ungranted") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let elsewhere = tempfile::tempdir().unwrap();
    std::fs::write(elsewhere.path().join("note"), "not granted\n").unwrap();

    let module = Module::with(&format!(
        "[ -e {}/note ] && echo REACHABLE || echo absent",
        elsewhere.path().display()
    ));
    let output = pivoted(&arena, &module, &[]).expect("rootfs");

    assert!(output.status.success());
    assert_eq!(stdout(&output), "absent");
}

#[test]
fn a_path_granted_only_for_reading_cannot_be_written_to() {
    // Two mechanisms say this: the LSM refuses the open, and the bind is
    // mounted read-only. This tests the second, which is the one that holds
    // even where the LSM is not loaded.
    let Some(arena) = arena("pivot-ro") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let granted = tempfile::tempdir().unwrap();
    let module = Module::with(&format!(
        "touch {}/written 2>/dev/null && echo WRITABLE || echo read-only",
        granted.path().display()
    ));

    let output = pivoted(&arena, &module, &[grant(granted.path(), "read")]).expect("rootfs");
    assert!(output.status.success());
    assert_eq!(stdout(&output), "read-only");
    assert!(!granted.path().join("written").exists());
}

#[test]
fn a_path_granted_for_writing_can_be_written_to() {
    // The control for the one above, and the check that the read-only remount
    // is applied per bind rather than to everything.
    let Some(arena) = arena("pivot-rw") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let granted = tempfile::tempdir().unwrap();
    let module = Module::with(&format!(
        "touch {}/written && echo wrote || echo DENIED",
        granted.path().display()
    ));

    let output = pivoted(&arena, &module, &[grant(granted.path(), "write")]).expect("rootfs");
    assert!(output.status.success());
    assert_eq!(stdout(&output), "wrote");
    assert!(
        granted.path().join("written").exists(),
        "the write did not reach the host path that was granted"
    );
}

#[test]
fn the_root_and_the_system_paths_are_read_only_but_tmp_is_not() {
    let Some(arena) = arena("pivot-seal") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let module = Module::with(
        "touch /nope       2>/dev/null && echo ROOT-WRITABLE || echo root-sealed; \
         touch /usr/nope   2>/dev/null && echo USR-WRITABLE  || echo usr-sealed; \
         touch /module/x   2>/dev/null && echo MOD-WRITABLE  || echo module-sealed; \
         touch /tmp/ok     2>/dev/null && echo tmp-writable  || echo TMP-SEALED",
    );

    let output = pivoted(&arena, &module, &[]).expect("rootfs");
    assert!(output.status.success());

    let seen = stdout(&output);
    assert!(seen.contains("root-sealed"), "{seen}");
    assert!(seen.contains("usr-sealed"), "{seen}");
    assert!(seen.contains("module-sealed"), "{seen}");
    assert!(seen.contains("tmp-writable"), "{seen}");
}

#[test]
fn the_old_root_is_detached_and_leaves_no_way_back() {
    // Between `pivot_root` and the unmount, the host tree is still reachable
    // through the parked mount point. If that step were skipped everything
    // above would still pass and the sandbox would be a door with no lock.
    let Some(arena) = arena("pivot-old") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let module = Module::with(
        "[ -e /.old-root ] && echo OLD-ROOT-PRESENT || echo detached; \
         ls / | tr '\n' ' '",
    );

    let output = pivoted(&arena, &module, &[]).expect("rootfs");
    assert!(output.status.success());

    let seen = stdout(&output);
    assert!(seen.contains("detached"), "{seen}");
    assert!(
        !seen.contains("old-root"),
        "the parked old root is still there: {seen}"
    );
}

#[test]
fn a_granted_path_under_tmp_survives_the_module_getting_its_own_tmp() {
    // The regression this exists for: the module's writable `/tmp` was mounted
    // after the binds, so it covered any granted path underneath it. The
    // module got "no such file or directory" for something the human had
    // confirmed — and it was found only because a test's granted path happened
    // to be a temporary directory.
    let Some(arena) = arena("pivot-tmp-grant") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let granted = PathBuf::from(format!("/tmp/thalyx-grant-{}", std::process::id()));
    std::fs::create_dir_all(&granted).unwrap();
    std::fs::write(granted.join("note"), "under tmp\n").unwrap();

    let module = Module::with(&format!("cat {}/note", granted.display()));
    let output = pivoted(&arena, &module, &[grant(&granted, "read")]).expect("rootfs");

    let _ = std::fs::remove_dir_all(&granted);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), "under tmp");
}

#[test]
fn a_module_runs_as_a_user_of_its_own() {
    // The decree: modules are isolated from each other, not only from the
    // system. Asking the module who it is, because asking Thalyx whether it
    // dropped privilege would prove nothing.
    let Some(arena) = arena("uid") else { return };
    let _cgroup = cgroup_in(&arena);

    let module = Module::with("id -u");
    let rootfs = thalyx_sandbox::RootFs::for_module(module.dir(), &[]).expect("rootfs");

    let spec = thalyx_sandbox::LaunchSpec {
        cgroup: arena.0.join("org.thalyx.demo"),
        profile: profile::MODULE_STANDARD.to_string(),
        namespaces: standard(),
        rootfs: Some(rootfs),
        program: module.program(),
        uid: Some(thalyx_core::uids::FIRST_UID),
    };

    let output = Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .args(
            thalyx_sandbox::launch::argv(thalyx_sandbox::launch::ENTER_MARKER, &spec, &[])
                .expect("argv"),
        )
        .output()
        .expect("launch");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), thalyx_core::uids::FIRST_UID.to_string());

    // The control: the launcher itself is root, so a module that had *not*
    // dropped would have printed 0. Without this the assertion above would
    // also pass on a machine where everything already runs unprivileged.
    assert_eq!(
        current_uid(),
        0,
        "this test only means something when the launcher is privileged"
    );
}

#[test]
fn dropping_to_its_own_user_is_what_stops_the_module_writing() {
    // With a baseline and a control, because the first attempt at this test
    // wrote to `/module` — which is bound read-only, so root would have been
    // denied too and the test proved nothing about the user at all.
    //
    // Here the target is root-owned, mode 0755, and bound *writable*. The only
    // thing standing between the module and that directory is who it is.
    let Some(arena) = arena("uid-write") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let target = tempfile::tempdir().unwrap();
    set_mode(target.path(), 0o755);

    let module = Module::with(&format!(
        "touch {}/written 2>/dev/null && echo wrote || echo denied",
        target.path().display()
    ));

    let run_as = |uid: Option<u32>| -> String {
        let rootfs =
            thalyx_sandbox::RootFs::for_module(module.dir(), &[grant(target.path(), "write")])
                .expect("rootfs");
        let spec = thalyx_sandbox::LaunchSpec {
            cgroup: arena.0.join("org.thalyx.demo"),
            profile: profile::MODULE_STANDARD.to_string(),
            namespaces: standard(),
            rootfs: Some(rootfs),
            program: module.program(),
            uid,
        };
        let output = Command::new(env!("CARGO_BIN_EXE_thalyx"))
            .args(
                thalyx_sandbox::launch::argv(thalyx_sandbox::launch::ENTER_MARKER, &spec, &[])
                    .expect("argv"),
            )
            .output()
            .expect("launch");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        stdout(&output)
    };

    // Baseline: as the user Thalyx runs as, the write goes through. Without
    // this, "denied" could mean the bind was read-only, or the path was wrong,
    // or the sandbox was broken in some way that has nothing to do with users.
    assert_eq!(run_as(None), "wrote", "the baseline write should succeed");
    std::fs::remove_file(target.path().join("written")).unwrap();

    // And with a user of its own, the same write is refused.
    assert_eq!(run_as(Some(thalyx_core::uids::FIRST_UID)), "denied");
    assert!(!target.path().join("written").exists());
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// The uid this test process runs as.
fn current_uid() -> u32 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("Uid:"))
                .and_then(|line| line.split_whitespace().nth(2))
                .and_then(|uid| uid.parse().ok())
        })
        .unwrap_or(u32::MAX)
}

#[test]
fn a_write_grant_on_someone_elses_directory_works_through_an_idmapped_mount() {
    // The whole cost of the one-uid-per-module decree, paid off. The directory
    // belongs to somebody else and is not world-writable; the module runs as
    // its own user; and the write still lands, owned by the directory's owner.
    //
    // Without the remapping this is exactly the case that fails.
    let Some(arena) = arena("idmap") else { return };
    let _cgroup = cgroup_in(&arena);

    let target = tempfile::tempdir().unwrap();
    set_mode(target.path(), 0o700); // not world-anything

    let module = Module::with(&format!(
        "touch {}/written 2>/dev/null && echo wrote || echo DENIED",
        target.path().display()
    ));

    let rootfs = thalyx_sandbox::RootFs::for_module_as(
        module.dir(),
        &[grant(target.path(), "write")],
        Some(thalyx_core::uids::FIRST_UID),
    );

    let rootfs = match rootfs {
        Ok(rootfs) => rootfs,
        Err(error) => panic!("{error}"),
    };

    let spec = thalyx_sandbox::LaunchSpec {
        cgroup: arena.0.join("org.thalyx.demo"),
        profile: profile::MODULE_STANDARD.to_string(),
        namespaces: standard(),
        rootfs: Some(rootfs),
        program: module.program(),
        uid: Some(thalyx_core::uids::FIRST_UID),
    };

    let output = Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .args(
            thalyx_sandbox::launch::argv(thalyx_sandbox::launch::ENTER_MARKER, &spec, &[])
                .expect("argv"),
        )
        .output()
        .expect("launch");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    // Keyed on what the kernel refusing actually says, and nothing else. A
    // looser match caught the helper's own goodbye and reported NOT PROVEN for
    // a run that had worked.
    if stderr.contains("the kernel refused to remap") {
        eprintln!("NOT PROVEN: this kernel or filesystem refused the idmapped mount.");
        eprintln!("  {}", stderr.trim());
        eprintln!("  This test did not run. It did not pass.");
        assert!(
            std::env::var_os("THALYX_REQUIRE_CGROUP_TESTS").is_none(),
            "{stderr}"
        );
        return;
    }

    assert!(output.status.success(), "{stderr}");
    assert_eq!(stdout(&output), "wrote");

    // And on the host the file is real, owned by whoever owns the directory —
    // not by the module's user, which does not exist outside Thalyx.
    let written = target.path().join("written");
    assert!(written.exists(), "the write did not reach the host");

    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        std::fs::metadata(&written).unwrap().uid(),
        std::fs::metadata(target.path()).unwrap().uid(),
        "the file landed owned by the module's user instead of the directory's owner"
    );
}

#[test]
fn a_read_grant_on_a_private_directory_is_readable_and_still_not_writable() {
    // Reads need the remapping as much as writes do: a directory the human
    // keeps at mode 0700 is unreadable to uid 700000 however clearly the grant
    // was confirmed. And remapping makes the module the apparent owner, which
    // would hand it write access nobody granted — so the bind is remounted
    // read-only afterwards.
    let Some(arena) = arena("idmap-read") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let target = tempfile::tempdir().unwrap();
    std::fs::write(target.path().join("secret"), "granted content\n").unwrap();
    set_mode(target.path(), 0o700);

    let module = Module::with(&format!(
        "cat {dir}/secret; touch {dir}/more 2>/dev/null && echo WRITABLE || echo read-only",
        dir = target.path().display()
    ));

    let rootfs = thalyx_sandbox::RootFs::for_module_as(
        module.dir(),
        &[grant(target.path(), "read")],
        Some(thalyx_core::uids::FIRST_UID),
    )
    .expect("rootfs");

    let spec = thalyx_sandbox::LaunchSpec {
        cgroup: arena.0.join("org.thalyx.demo"),
        profile: profile::MODULE_STANDARD.to_string(),
        namespaces: standard(),
        rootfs: Some(rootfs),
        program: module.program(),
        uid: Some(thalyx_core::uids::FIRST_UID),
    };

    let output = Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .args(
            thalyx_sandbox::launch::argv(thalyx_sandbox::launch::ENTER_MARKER, &spec, &[])
                .expect("argv"),
        )
        .output()
        .expect("launch");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if stderr.contains("the kernel refused to remap") {
        eprintln!("NOT PROVEN: idmapped mounts refused here. This test did not pass.");
        assert!(
            std::env::var_os("THALYX_REQUIRE_CGROUP_TESTS").is_none(),
            "{stderr}"
        );
        return;
    }

    assert!(output.status.success(), "{stderr}");
    let seen = stdout(&output);
    assert!(seen.contains("granted content"), "{seen}");
    assert!(seen.contains("read-only"), "{seen}");
    assert!(!target.path().join("more").exists());
}
