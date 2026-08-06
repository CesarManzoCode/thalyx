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

    // The module's own view of where it is. cgroup v2 lines are `0::<path>`,
    // and Thalyx reprints what the module wrote behind `> ` — the module never
    // reaches the terminal itself. Matching the marker rather than skipping
    // past it on purpose: a `0::` at the start of a line would mean the module
    // had written straight to the screen, which is a different and worse fact
    // than this test failing.
    let reported = status
        .stdout()
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("> 0::").map(str::to_string))
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

/// A profile name nothing resolves is wrong everywhere, not only where the
/// kernel can enforce.
///
/// The lookup used to happen after the kernel side was found present, and the
/// ordering was what let `thalyx session` ask for a profile called `default`
/// for as long as the prompt could run a module: every machine without the
/// policy map — every machine but the image — answered with the honest gap and
/// never reached the name. The image answered with the name, on its own
/// console, after an install had already succeeded.
///
/// So this asserts the ordering rather than the message: on a machine that
/// cannot enforce anything, a bad profile name still comes back as a bad
/// profile name.
#[test]
fn a_profile_no_profile_is_called_is_refused_before_the_kernel_is_asked() {
    let fixture = Fixture::new();
    assert!(fixture.install().success());

    let status = fixture.run(&[
        "module",
        "run",
        Fixture::MODULE_ID,
        "--profile",
        "no-profile-is-called-this",
    ]);

    assert!(!status.success());
    assert!(
        status.stderr().contains("is not a sandbox profile"),
        "the name was never looked at; the run failed for some other reason: {}",
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

    // The proof that it ran is its **exit code**, not anything it printed.
    //
    // This test used to `echo` and look for the text on Thalyx's stdout, which
    // worked because the module inherited Thalyx's terminal. It no longer has
    // one, deliberately: a module sharing a terminal with the trusted path can
    // read the human's answer to a confirmation prompt and can draw Thalyx's
    // own frame. See `thalyx_sandbox::launch::spawn`.
    //
    // So the observable had to change, and the claim did not. A distinctive
    // exit status is a signal only the module's own code can produce, and it
    // travels the way a module's result is supposed to — reported by Thalyx,
    // not written to a screen by the module.
    const PROOF: i32 = 7;

    std::fs::write(
        fixture.base().join("payload/bin/demo"),
        format!("#!/bin/sh\nexit {PROOF}\n"),
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
        status
            .stdout()
            .contains(&format!("exited with status {PROOF}")),
        "the module's own code never ran — an unrunnable entrypoint and a \
         module that did nothing look identical without this:\n{}",
        status.stdout()
    );
    assert!(
        status.stdout().contains("RAN UNCONFINED"),
        "an unenforced run must say so:\n{}",
        status.stdout()
    );
}

#[test]
fn a_module_never_gets_the_terminal_the_trusted_path_uses() {
    // The confirmation prompt is drawn on Thalyx's stdout and answered on
    // Thalyx's stdin. A module that inherited those could read what the human
    // types at a prompt meant for Thalyx, and could draw the prompt itself —
    // and the frame is the whole mechanism by which a human tells Thalyx apart
    // from what runs inside it.
    //
    // Checked from outside, by having the module try: a module that only ever
    // behaved would demonstrate nothing about whether it could misbehave.
    //
    // ## What is asserted, and what deliberately is not
    //
    // Not that the module's words vanish. That was asserted once, by giving it
    // the null device, and it cost the answers `dev/verify.sh` proves the
    // sandbox with — see `thalyx_sandbox::launch::spawn`. It is also not the
    // property: the same words come back through the channel, on purpose,
    // because a module has things to say.
    //
    // The property is that a module cannot produce a **line of its own**. Every
    // line it wrote is drawn behind Thalyx's marker, so nothing it writes can
    // start a line, and the frame stays something only Thalyx can draw. That is
    // the same claim the prompt's own sanitiser makes about a publisher's name.
    let fixture = Fixture::new();
    std::fs::write(
        fixture.base().join("payload/bin/demo"),
        "#!/bin/sh\n\
         echo '┌─ Thalyx — capability authorisation'\n\
         echo 'FORGED BY THE MODULE'\n\
         printf 'and \\033[2Kthis repaints the line\\n'\n\
         exit 0\n",
    )
    .unwrap();
    make_executable(&fixture.base().join("payload/bin/demo"));

    let bundle = fixture.build_bundle("1.0.0");
    assert!(fixture.install_bundle_at(&bundle).success());

    let status = fixture.run(&["module", "run", Fixture::MODULE_ID, "--unconfined"]);

    for line in status.stdout().lines().chain(status.stderr().lines()) {
        assert!(
            !line.starts_with('┌') && !line.starts_with("FORGED"),
            "a module drew a line of its own on the terminal Thalyx draws the \
             trusted path on:\n{line}"
        );
    }

    // No escape reaches the screen either. A module that could move the cursor
    // needs no forged frame — it can repaint the one Thalyx drew.
    assert!(
        !status.stdout().contains('\u{1b}') && !status.stderr().contains('\u{1b}'),
        "an escape sequence reached the terminal through a module's output"
    );

    // The controls. Without the first, a module that failed to start looks
    // exactly like one that was contained. Without the second, so does a
    // Thalyx that went back to discarding the output — which passed every
    // assertion above and lost six checks in `verify.sh`.
    assert!(
        status.stdout().contains("exited cleanly"),
        "the module did not run at all, so this proves nothing:\n{}",
        status.stdout()
    );
    assert!(
        status.stdout().contains("> FORGED BY THE MODULE"),
        "what the module wrote was thrown away rather than marked, so this \
         test would pass on a Thalyx that can no longer show what a confined \
         program sees:\n{}",
        status.stdout()
    );
}

#[test]
fn a_module_that_writes_more_than_thalyx_keeps_still_finishes() {
    // The reason the output is a pipe Thalyx *drains* and not a pipe Thalyx
    // holds. A reader that stops at its own ceiling stops emptying the buffer,
    // and the module blocks on its next write for good: Thalyx waits for a
    // module that is waiting for Thalyx. The ceiling is on what is kept, never
    // on what is read, and this is the difference showing.
    //
    // 200k lines is several megabytes, far past both the 64 KiB pipe buffer
    // and the 64 KiB Thalyx keeps. Before the drain existed this hung rather
    // than failed, which is why the test asserts an exit at all.
    let fixture = Fixture::new();
    std::fs::write(
        fixture.base().join("payload/bin/demo"),
        "#!/bin/sh\nawk 'BEGIN{for(i=0;i<200000;i++)print \"noise\"}'\nexit 0\n",
    )
    .unwrap();
    make_executable(&fixture.base().join("payload/bin/demo"));

    let bundle = fixture.build_bundle("1.0.0");
    assert!(fixture.install_bundle_at(&bundle).success());

    let status = fixture.run(&["module", "run", Fixture::MODULE_ID, "--unconfined"]);

    assert!(
        status.stdout().contains("exited cleanly"),
        "the module did not finish:\n{}",
        status.stdout()
    );

    // And Thalyx says it stopped keeping. Output that silently stopped growing
    // and a module that stopped writing look identical otherwise.
    assert!(
        status.stdout().contains("past what Thalyx keeps"),
        "output was dropped without saying so:\n{}",
        status.stdout()
    );

    // Bounded on the screen too, which is the other half: a module must not be
    // able to become the terminal by writing enough.
    assert!(
        status.stdout().lines().count() < 200,
        "a module wrote {} lines onto the screen",
        status.stdout().lines().count()
    );
}

#[test]
fn a_module_that_fails_can_still_say_why() {
    // `CLAUDE.md` rule 10: a failure to read is not a failure to exist, and
    // saying which one happened is the whole job. A module whose `stderr` went
    // to the null device made *it failed* and *it failed for this reason* the
    // same event, with the reason unrecoverable — the module had already exited
    // by the time anyone noticed.
    let fixture = Fixture::new();
    std::fs::write(
        fixture.base().join("payload/bin/demo"),
        "#!/bin/sh\necho 'the config file is malformed at line 3' >&2\nexit 2\n",
    )
    .unwrap();
    make_executable(&fixture.base().join("payload/bin/demo"));

    let bundle = fixture.build_bundle("1.0.0");
    assert!(fixture.install_bundle_at(&bundle).success());

    let status = fixture.run(&["module", "run", Fixture::MODULE_ID, "--unconfined"]);

    assert!(
        status.stdout().contains("malformed at line 3"),
        "the module's own diagnostic was lost:\n{}",
        status.stdout()
    );
    // Marked as stderr rather than merged into stdout: a module's complaint
    // and a module's answer are different things, and the streams arrive
    // separately.
    assert!(
        status.stdout().contains("! the config file is malformed"),
        "a diagnostic was reported as ordinary output:\n{}",
        status.stdout()
    );
    assert!(
        status.stdout().contains("exited with status 2"),
        "the failure itself went unreported:\n{}",
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
