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
        channel_fd: None,
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
        channel_fd: None,
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
        channel_fd: None,
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
            channel_fd: None,
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

    let output = remapped(&arena, &module, &[grant(target.path(), "write")]).expect("launch");
    if remap_refused(
        &output,
        "a write onto someone else's directory went unchecked",
    ) {
        return;
    }

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
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

    let output = remapped(&arena, &module, &[grant(target.path(), "read")]).expect("launch");
    if remap_refused(&output, "a read of a private directory went unchecked") {
        return;
    }

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "{stderr}");
    let seen = stdout(&output);
    assert!(seen.contains("granted content"), "{seen}");
    assert!(seen.contains("read-only"), "{seen}");
    assert!(!target.path().join("more").exists());
}

/// Launch a module in a root remapped to its own uid, and hand back what it said.
///
/// Extracted when this became the third copy of the same eight lines. It is
/// `create_target_like`'s lesson one crate over: two pieces of code that must
/// agree about the same kernel protocol, kept apart, stop agreeing — and here
/// the protocol is which uid the root was built for, which has to be the same
/// uid the launch runs as or the remapping proves nothing.
fn remapped(
    arena: &Arena,
    module: &Module,
    grants: &[thalyx_manifest::Permission],
) -> std::io::Result<Output> {
    let rootfs = thalyx_sandbox::RootFs::for_module_as(
        module.dir(),
        grants,
        Some(thalyx_core::uids::FIRST_UID),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;

    let spec = thalyx_sandbox::LaunchSpec {
        cgroup: arena.0.join("org.thalyx.demo"),
        profile: profile::MODULE_STANDARD.to_string(),
        namespaces: standard(),
        rootfs: Some(rootfs),
        program: module.program(),
        uid: Some(thalyx_core::uids::FIRST_UID),
        channel_fd: None,
    };

    Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .args(
            thalyx_sandbox::launch::argv(thalyx_sandbox::launch::ENTER_MARKER, &spec, &[])
                .map_err(|error| std::io::Error::other(error.to_string()))?,
        )
        .output()
}

/// Whether this kernel or filesystem turned the remapping down.
///
/// Keyed on what the refusal actually says, and nothing else. A looser match
/// caught the helper's own goodbye and reported `NOT PROVEN` for a run that had
/// worked — the eleventh time the instrument was the thing that was wrong.
fn remap_refused(output: &Output, what: &str) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.contains("the kernel refused to remap") {
        return false;
    }

    eprintln!("NOT PROVEN: this kernel or filesystem refused the idmapped mount, so {what}");
    eprintln!("  {}", stderr.trim());
    eprintln!("  This test did not run. It did not pass.");
    assert!(
        std::env::var_os("THALYX_REQUIRE_CGROUP_TESTS").is_none(),
        "{stderr}"
    );
    true
}

#[test]
fn a_write_grant_on_a_single_file_lands_through_the_remapped_mount() {
    // The shape every other grant in this file happened not to have. On
    // 2026-08-04 a granted path that named one file — the greeter's
    // `notes.txt` — got a directory for its mount point, and the remapped bind
    // died at its last syscall with `EINVAL` on the machine's own console,
    // while this suite was green: every permission in every test here was a
    // directory, so the kernel rule that a bind's target must be the same kind
    // as its source was never asked about.
    //
    // `create_target_like` has held that rule in one place since, and a unit
    // test covers it without mounting anything. This is the same claim made
    // where it broke — a real remapped root, a real bind, one real file.
    let Some(arena) = arena("idmap-file") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let home = tempfile::tempdir().unwrap();
    let note = home.path().join("notes.txt");
    std::fs::write(&note, "what was already there\n").unwrap();
    set_mode(&note, 0o600); // the human's file, and nobody else's

    let module = Module::with(&format!(
        "cat {note}; echo 'and what the module added' >> {note} 2>/dev/null \
         && echo appended || echo DENIED",
        note = note.display()
    ));

    let output = remapped(&arena, &module, &[grant(&note, "write")]).expect("launch");
    if remap_refused(&output, "a granted file was never bound") {
        return;
    }

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "{stderr}");

    let seen = stdout(&output);
    assert!(
        seen.contains("what was already there"),
        "the granted file was not readable inside: {seen}"
    );
    assert!(seen.contains("appended"), "the write was refused: {seen}");

    // And on the host it is the same file, not a copy and not a directory with
    // something in it: the bind has to have landed on the file itself.
    let after = std::fs::read_to_string(&note).unwrap();
    assert_eq!(after, "what was already there\nand what the module added\n");

    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        std::fs::metadata(&note).unwrap().uid(),
        current_uid(),
        "the file changed hands; the module's user does not exist outside Thalyx"
    );
}

#[test]
fn a_granted_file_does_not_bring_the_directory_it_lives_in() {
    // The control for the one above. Without it, "the granted file is
    // reachable" would also be true of a sandbox that bound its whole
    // directory — which is the obvious way to make a file grant work and the
    // one that hands over everything beside it.
    let Some(arena) = arena("idmap-file-only") else {
        return;
    };
    let _cgroup = cgroup_in(&arena);

    let home = tempfile::tempdir().unwrap();
    let note = home.path().join("notes.txt");
    std::fs::write(&note, "granted\n").unwrap();
    std::fs::write(home.path().join("private.txt"), "never granted\n").unwrap();

    let module = Module::with(&format!(
        "cat {note}; [ -e {other} ] && echo REACHABLE || echo absent",
        note = note.display(),
        other = home.path().join("private.txt").display()
    ));

    let output = remapped(&arena, &module, &[grant(&note, "read")]).expect("launch");
    if remap_refused(&output, "the neighbour of a granted file went unchecked") {
        return;
    }

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let seen = stdout(&output);
    assert!(seen.contains("granted"), "{seen}");
    assert!(
        seen.contains("absent"),
        "the file's neighbour came along with it: {seen}"
    );
}

// ---------------------------------------------------------------------------
// The order the confinement is built in, measured rather than reasoned about.
// ---------------------------------------------------------------------------

/// A launch traced by `strace`, so the *order* of its syscalls can be read.
///
/// `strace` and not Thalyx, and that is the whole design of this check: rule 5
/// says the instrument includes the harness, and asking Thalyx whether it did
/// its work in the right order would pass on any build where the ordering and
/// the belief about it are wrong together — which is exactly what happened.
/// `-y` is what makes it readable at all: a `write` carries a descriptor
/// number, and `-y` annotates it with the path it was opened from.
fn traced_launch(arena: &Arena, module: &Module) -> Option<(Output, String)> {
    let rootfs = thalyx_sandbox::RootFs::for_module(module.dir(), &[]).expect("rootfs");
    let spec = thalyx_sandbox::LaunchSpec {
        cgroup: arena.0.join("org.thalyx.demo"),
        profile: profile::MODULE_STANDARD.to_string(),
        namespaces: standard(),
        rootfs: Some(rootfs),
        program: module.program(),
        uid: None,
        channel_fd: None,
    };

    let scratch = tempfile::tempdir().expect("temp dir");
    let trace = scratch.path().join("trace");

    let output = Command::new("strace")
        .args(["-f", "-y", "-o"])
        .arg(&trace)
        .arg("--")
        .arg(env!("CARGO_BIN_EXE_thalyx"))
        .args(
            thalyx_sandbox::launch::argv(thalyx_sandbox::launch::ENTER_MARKER, &spec, &[])
                .expect("argv"),
        )
        .output();

    let output = match output {
        Ok(output) => output,
        Err(error) => return unmeasurable(&format!("strace could not be run: {error}")),
    };

    let text = std::fs::read_to_string(&trace).unwrap_or_default();
    if text.is_empty() {
        // `strace` exists but could not attach — a container without
        // `CAP_SYS_PTRACE`, or `yama/ptrace_scope`. Never a pass: nothing was
        // measured.
        return unmeasurable("strace produced no trace, so it could not attach");
    }

    Some((output, text))
}

fn unmeasurable(reason: &str) -> Option<(Output, String)> {
    let message = format!("NOT PROVEN: the order of the launch could not be traced ({reason})");
    assert!(
        std::env::var_os("THALYX_REQUIRE_STRACE_TESTS").is_none(),
        "{message}"
    );
    eprintln!("{message}");
    eprintln!("  This test did not run. It did not pass.");
    None
}

/// The pid `strace -f` prefixes a line with.
fn pid_of(line: &str) -> Option<&str> {
    let pid = line.split_whitespace().next()?;
    pid.chars().all(|c| c.is_ascii_digit()).then_some(pid)
}

/// The syscall name, as it appears after the pid.
fn call_on(line: &str) -> &str {
    line.split_whitespace().nth(1).unwrap_or("")
}

fn is_an_open(line: &str) -> bool {
    let call = call_on(line);
    call.starts_with("open(") || call.starts_with("openat(") || call.starts_with("openat2(")
}

/// Whether this line opens something in a mode the LSM counts as writing.
///
/// The same reading `lsm/file_open` does — `flags & O_ACCMODE`, where
/// `O_RDONLY` is 0 — so what this test calls a write is what the kernel calls
/// one, rather than a second opinion about it.
fn opens_for_writing(line: &str) -> bool {
    let call = call_on(line);
    if call.starts_with("creat(") {
        return true;
    }
    is_an_open(line) && (line.contains("O_WRONLY") || line.contains("O_RDWR"))
}

#[test]
fn nothing_is_opened_for_writing_after_the_launcher_takes_the_module_s_identity() {
    // The defect of 2026-08-26, and the only shape of test that could have
    // caught it a day earlier.
    //
    // Joining the cgroup is the moment the process starts being governed by
    // the module's permissions: `lsm/file_open` looks up the policy by cgroup
    // id, asks for `FS_WRITE` on anything not opened read-only, and answers
    // `-EPERM`. So every write Thalyx has to do to *build* the confinement has
    // to happen before that line. It did not: the launcher joined first and
    // assembled afterwards, the LSM denied Thalyx creating the mount point for
    // `/dev/null`, and on an enforcing kernel nothing could be launched at all
    // — not a guest, not a signed module.
    //
    // No test saw it because this container has no BPF LSM, so `check()`
    // returns 0 and every one of those opens succeeds. The property that
    // survives without an LSM is the **order**, and strace can read it.
    let Some(arena) = arena("order") else { return };
    let _cgroup = cgroup_in(&arena);

    let module = Module::with("echo inside");
    let Some((output, trace)) = traced_launch(&arena, &module) else {
        return;
    };

    // The control, first. Everything below is a claim about a window in a
    // trace, and a launch that died early would have a very quiet one.
    assert_eq!(
        stdout(&output),
        "inside",
        "the module did not run, so the order of a launch is not what was measured: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let lines: Vec<&str> = trace.lines().collect();

    // Rule 10, and this is exactly the shape it warns about: a failure to read
    // is not a failure to exist. The whole check hangs on `-y`, which annotates
    // a descriptor with the path it was opened from — without it a `write` is a
    // number and the join is invisible. An old `strace` would then look
    // identical to a launcher that never joined its cgroup, which is a very
    // loud accusation to make about somebody else's tooling. This project has
    // already been caught twice measuring the instrument's version instead of
    // the subject.
    let Some(join) = lines
        .iter()
        .position(|line| line.contains("write(") && line.contains("cgroup.procs>"))
    else {
        if !trace.contains("cgroup.procs>") {
            unmeasurable(
                "this strace does not annotate descriptors with their paths, so the join cannot be located",
            );
            return;
        }
        panic!("the launcher never wrote its pid into cgroup.procs");
    };
    let pid = pid_of(lines[join]).expect("a pid on the join line");

    let exec = lines[join + 1..]
        .iter()
        .position(|line| pid_of(line) == Some(pid) && call_on(line).starts_with("execve("))
        .map(|offset| join + 1 + offset)
        .expect("the launcher never reached execve after joining the cgroup");

    // The baseline, and rule 4 is the whole reason it is here: "no writes
    // after the join" is also true of a launcher that never wrote anything,
    // and of one that never assembled a root at all.
    //
    // Asked in two halves on purpose. The mount point for `/dev/null` is the
    // exact write the machine denied, so "it was never created" and "it was
    // created on the wrong side of the join" are different findings and must
    // not arrive as the same sentence — the first says this trace proves
    // nothing, the second is the defect.
    let created: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            pid_of(line) == Some(pid) && opens_for_writing(line) && line.contains("/dev/null")
        })
        .map(|(at, _)| at)
        .collect();
    assert!(
        !created.is_empty(),
        "the launcher never created a mount point for /dev/null anywhere in this trace, \
         so the ordering was not exercised and nothing below means anything"
    );
    let late: Vec<&usize> = created.iter().filter(|&&at| at > join).collect();
    assert!(
        late.is_empty(),
        "the mount point for /dev/null was created after the launcher joined the cgroup, \
         which is the -EPERM that stopped every launch on an enforcing kernel:\n{}",
        late.iter()
            .map(|&&at| format!("  {}", lines[at]))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // And the claim.
    let window = &lines[join + 1..exec];

    let writes: Vec<&&str> = window
        .iter()
        .filter(|line| pid_of(line) == Some(pid) && opens_for_writing(line))
        .collect();
    assert!(
        writes.is_empty(),
        "the launcher opened something for writing after joining the cgroup, \
         which an enforcing kernel answers with -EPERM:\n{}",
        writes
            .iter()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Fail closed, rule 9. `strace -f` can split a syscall across two lines
    // when another process interleaves, and half an `openat` does not say what
    // it was opened for. An unreadable line is not a line that said no.
    let unreadable: Vec<&&str> = window
        .iter()
        .filter(|line| {
            pid_of(line) == Some(pid) && is_an_open(line) && line.contains("<unfinished")
        })
        .collect();
    assert!(
        unreadable.is_empty(),
        "an open in the window was cut in half by strace, so what it asked for cannot be read:\n{}",
        unreadable
            .iter()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
