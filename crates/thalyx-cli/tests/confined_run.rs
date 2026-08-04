//! The enforcement loop, end to end, with the real binary.
//!
//! `thalyx-sandbox` proves the cgroup mechanics against the kernel. This
//! proves the part only the whole system can: that `thalyx module run` puts
//! the module's own process inside the cgroup Thalyx created for it, before
//! the module's first instruction.
//!
//! The module reports its own cgroup from `/proc/self/cgroup`. Asking the
//! module rather than asking Thalyx is the point — a test where the system
//! confirms its own claim proves nothing.

mod harness;

use harness::Fixture;
use std::path::{Path, PathBuf};

/// A scratch cgroup2 parent, or a reason there is none.
struct Arena(PathBuf);

impl Drop for Arena {
    fn drop(&mut self) {
        // Whatever the run left behind, plus the arena itself.
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
        Err(error) => return unavailable(&error.to_string()),
    };

    let path = mount.join(format!("thalyx-cli-{}-{label}", std::process::id()));
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
        Err(error) => unavailable(&format!("cannot create {}: {error}", path.display())),
    }
}

fn unavailable(reason: &str) -> Option<Arena> {
    let message = format!("NOT PROVEN: no writable cgroup2 filesystem for this test ({reason})");
    assert!(
        std::env::var_os("THALYX_REQUIRE_CGROUP_TESTS").is_none(),
        "{message}"
    );
    eprintln!("{message}");
    eprintln!("  This test did not run. It did not pass.");
    None
}

/// Replace the module's payload with a script that reports its own cgroup.
fn payload_that_reports_its_cgroup(fixture: &Fixture) {
    std::fs::write(
        fixture.base().join("payload/bin/demo"),
        "#!/bin/sh\ncat /proc/self/cgroup\n",
    )
    .unwrap();
    make_executable(&fixture.base().join("payload/bin/demo"));
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn a_module_runs_inside_the_cgroup_thalyx_created_for_it() {
    let Some(arena) = arena("run") else { return };

    let fixture = Fixture::new();
    payload_that_reports_its_cgroup(&fixture);
    let bundle = fixture.build_bundle("1.0.0");
    assert!(fixture.install_bundle_at(&bundle).success());

    // A MemoryStore is not reachable from the binary, so the kernel side has
    // to be present for a confined run. Where it is not, this exercises the
    // rest of the path and says so.
    let status = fixture.run_with_cgroup_root(&arena.0, &["module", "run", Fixture::MODULE_ID]);

    if status.stderr().contains("policy map is not loaded") {
        eprintln!("NOT PROVEN: thalyx-lsm is not loaded, so no policy could be written.");
        eprintln!("  The confinement itself was not exercised. This test did not pass.");
        // A different variable from the cgroup one on purpose. A machine can
        // easily have cgroup2 and namespaces without the BPF side loaded, and
        // one flag for both would mean the only way to demand the parts that
        // *are* available is to demand the parts that are not.
        assert!(
            std::env::var_os("THALYX_REQUIRE_LSM_TESTS").is_none(),
            "{}",
            status.stderr()
        );
        return;
    }

    assert!(
        status.success(),
        "run failed: {}\n{}",
        status.stdout(),
        status.stderr()
    );

    // The module's own view of where it is. cgroup v2 lines are `0::<path>`.
    let reported = status
        .stdout()
        .lines()
        .find_map(|line| line.strip_prefix("0::").map(str::to_string))
        .unwrap_or_else(|| {
            panic!(
                "module did not report a cgroup v2 path:\n{}",
                status.stdout()
            )
        });

    assert!(
        reported.ends_with(Fixture::MODULE_ID),
        "the module ran in `{reported}`, not in the cgroup Thalyx made for it"
    );
}

#[test]
fn running_without_the_kernel_side_is_refused_rather_than_silently_unenforced() {
    // The failure this prevents has no symptom. A module with permissions
    // recorded, running with nothing enforcing them, behaves exactly like one
    // that is properly confined until the moment it does something it should
    // not have been able to do.
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    // The precondition, checked directly. Inferring it from "the run failed"
    // meant that a run failing for any *other* reason was read as the refusal
    // this test is looking for — and on a machine with the LSM loaded it read
    // as a failure of Thalyx.
    if std::path::Path::new(thalyx_permd::KernelStore::DEFAULT_MAP).exists() {
        eprintln!("NOT PROVEN: the kernel policy map is present, so the refusal path");
        eprintln!("  cannot be reached here. This test did not exercise what it names.");
        return;
    }

    let status = fixture.run(&["module", "run", Fixture::MODULE_ID]);
    assert!(!status.success());
    assert!(
        status.stderr().contains("would be enforced"),
        "expected a refusal naming the missing enforcement, got: {}",
        status.stderr()
    );
}

#[test]
fn running_a_module_that_is_not_installed_says_so() {
    let fixture = Fixture::new();
    let status = fixture.run(&["module", "run", "org.thalyx.absent"]);
    assert!(!status.success());
    assert!(
        status.stderr().contains("not installed"),
        "{}",
        status.stderr()
    );
}

#[test]
fn an_entrypoint_the_module_does_not_declare_is_refused() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let status = fixture.run(&[
        "module",
        "run",
        Fixture::MODULE_ID,
        "--entrypoint",
        "not-declared",
    ]);

    assert!(!status.success());
    assert!(
        status.stderr().contains("no entrypoint named"),
        "{}",
        status.stderr()
    );
}

#[test]
fn the_journal_records_that_a_run_happened_and_whether_it_was_enforced() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    // Refused before launching, so the journal must still carry the attempt.
    let _ = fixture.run(&["module", "run", Fixture::MODULE_ID]);

    let journal = fixture.run(&["journal", "--limit", "10"]);
    assert!(journal.success());
    assert!(
        journal.stdout().contains("run_module"),
        "a run attempt left no trace in the journal:\n{}",
        journal.stdout()
    );
}

#[test]
fn the_launch_helper_puts_the_program_inside_the_cgroup_before_it_runs() {
    // The ordering rule, tested on its own and against the real kernel.
    //
    // `module run` needs the BPF map to exist before it will launch anything,
    // which means on a machine without thalyx-lsm loaded the launch mechanism
    // is never exercised. It is the piece most worth proving — everything else
    // rests on the module's first instruction executing inside its cgroup —
    // so it is driven here directly, through the same entry point the parent
    // half uses.
    let Some(arena) = arena("helper") else { return };

    let cgroup = thalyx_sandbox::Cgroup::ensure(&arena.0, "org.thalyx.demo").expect("cgroup");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .args(diagnostic_argv(
            cgroup.path(),
            "/bin/sh",
            &["-c", "cat /proc/self/cgroup"],
        ))
        .output()
        .expect("helper");

    assert!(
        output.status.success(),
        "helper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let reported = stdout
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .unwrap_or_else(|| panic!("no cgroup v2 line in:\n{stdout}"));

    assert!(
        reported.ends_with("org.thalyx.demo"),
        "the program ran in `{reported}`, not in the cgroup it was launched into"
    );

    let _ = cgroup.remove();
}

#[test]
fn the_helper_refuses_to_run_the_program_when_it_cannot_confine_it() {
    // Fail closed. If the join does not happen, the program must not run at
    // all — running it outside the cgroup is precisely the outcome the
    // mechanism exists to prevent, and it would look like success.
    let directory = tempfile::tempdir().expect("temp dir");
    let not_a_cgroup = directory.path().join("plain");
    std::fs::create_dir(&not_a_cgroup).unwrap();

    let marker = directory.path().join("the-program-ran");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .args(diagnostic_argv(
            &not_a_cgroup,
            "/bin/sh",
            &["-c", &format!("touch {}", marker.display())],
        ))
        .output()
        .expect("helper");

    assert!(!output.status.success());
    assert!(
        !marker.exists(),
        "the program ran even though it could not be confined"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("confine nothing"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn an_installed_module_is_actually_executable() {
    // This is the test that was missing. Every install test checked that the
    // files arrived; none checked that the entrypoint could run, and it could
    // not — the unpacker was dropping the archive's mode, so every module
    // installed unrunnable while the whole suite stayed green.
    let fixture = Fixture::new();
    std::fs::write(
        fixture.base().join("payload/bin/demo"),
        "#!/bin/sh\necho the module ran\n",
    )
    .unwrap();
    make_executable(&fixture.base().join("payload/bin/demo"));

    let bundle = fixture.build_bundle("1.0.0");
    assert!(fixture.install_bundle_at(&bundle).success());

    // Unconfined on purpose: this test is about the module being runnable at
    // all, and it must not silently turn into a no-op on a machine where the
    // kernel side happens not to be loaded.
    let status = fixture.run(&["module", "run", Fixture::MODULE_ID, "--unconfined"]);

    assert!(
        status.success(),
        "the installed module could not be run: {}",
        status.stderr()
    );
    assert!(
        status.stdout().contains("the module ran"),
        "the module produced no output:\n{}",
        status.stdout()
    );
    assert!(
        status.stdout().contains("RAN UNCONFINED"),
        "an unenforced run must say so:\n{}",
        status.stdout()
    );
}

/// Re-execution arguments for the diagnostic profile: cgroup only.
fn diagnostic_argv(cgroup: &Path, program: &str, args: &[&str]) -> Vec<std::ffi::OsString> {
    let spec = thalyx_sandbox::LaunchSpec {
        cgroup: cgroup.to_path_buf(),
        profile: thalyx_sandbox::profile::DIAGNOSTIC.to_string(),
        namespaces: thalyx_sandbox::profile::Namespaces::NONE,
        rootfs: None,
        program: PathBuf::from(program),
        uid: None,
        channel_fd: None,
    };
    thalyx_sandbox::launch::argv(
        thalyx_sandbox::launch::ENTER_MARKER,
        &spec,
        &thalyx_sandbox::launch::to_args(args),
    )
    .expect("argv")
}
