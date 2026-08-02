//! Starting module code inside its confinement, with nothing running outside it.
//!
//! The problem is one of ordering. A process cannot be put into a cgroup before
//! it exists, so between `fork` and the join there is a moment where the child
//! is outside every policy. If the module's own code ran in that moment, it
//! would run unconfined — briefly, but enough.
//!
//! The way out is for that moment to belong to Thalyx. The parent starts the
//! `thalyx` binary again with a marker argument, and Thalyx's own code does the
//! confining before `exec` hands control to the module.
//!
//! It is the same re-exec container runtimes use, and it is chosen here for a
//! second reason: the alternative, `pre_exec`, is `unsafe`, and only
//! `thalyx-syscall` is allowed any.
//!
//! ## Why there are two stages and not one
//!
//! `CLONE_NEWPID` does not move the caller into a new PID namespace. It makes
//! the caller's *children* the first processes of one. So there has to be a
//! fork after the unshare, and the process that becomes the module cannot be
//! the process that unshared.
//!
//! ```text
//! enter   join cgroup, verify membership, unshare namespaces, spawn init
//!   └─ init   (PID 1 of the new namespace)
//!             mount /proc, set hostname, install seccomp, exec the module
//! ```
//!
//! Splitting it costs nothing that matters. The cgroup is inherited across
//! `fork`, so `init` is in it from its first instruction; the namespaces are
//! inherited too. What `init` adds is everything that can only be done from
//! inside — a `/proc` that reflects the new PID namespace, and a seccomp
//! filter that must not constrain the setup work `enter` still had to do.
//!
//! ## Failure is always closed
//!
//! Every step returns instead of continuing. A module that could not be
//! confined does not run, because running it is the outcome the whole
//! mechanism exists to prevent.

use crate::cgroup::Cgroup;
use crate::profile::{Namespaces, Profile};
use crate::{Result, SandboxError};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// First argument of the outer re-execution.
///
/// Deliberately not a clap subcommand: this is an internal entry point, not
/// something a user should find in `--help`, and it must be recognised before
/// any argument parsing that could interpret the module's own arguments.
pub const ENTER_MARKER: &str = "__thalyx_sandbox_enter";

/// First argument of the inner one, which becomes PID 1.
pub const INIT_MARKER: &str = "__thalyx_sandbox_init";

/// Where a fresh `proc` is mounted inside the module's mount namespace.
const PROC: &str = "/proc";

/// What a re-executed `thalyx` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Outer: join the cgroup and detach into namespaces.
    Enter {
        cgroup: PathBuf,
        profile: String,
        /// The namespace mask the parent settled on, after adjusting the named
        /// profile for what the module was actually granted.
        namespaces: Namespaces,
        program: PathBuf,
        args: Vec<OsString>,
    },
    /// Inner: finish the confinement from inside, then become the module.
    Init {
        profile: String,
        namespaces: Namespaces,
        program: PathBuf,
        args: Vec<OsString>,
    },
}

/// The argument vector for the outer stage.
///
/// The namespace mask travels explicitly. The child must not re-derive it: the
/// parent already adjusted the profile for what the module was granted, and a
/// second derivation from the profile name alone silently disagreed.
pub fn enter_argv(
    cgroup: &Path,
    profile: &str,
    namespaces: Namespaces,
    program: &Path,
    args: &[OsString],
) -> Vec<OsString> {
    let mut argv = vec![
        OsString::from(ENTER_MARKER),
        cgroup.as_os_str().to_os_string(),
        OsString::from(profile),
        OsString::from(namespaces.flags().to_string()),
        program.as_os_str().to_os_string(),
    ];
    argv.extend(args.iter().cloned());
    argv
}

/// The argument vector for the inner stage.
pub fn init_argv(
    profile: &str,
    namespaces: Namespaces,
    program: &Path,
    args: &[OsString],
) -> Vec<OsString> {
    let mut argv = vec![
        OsString::from(INIT_MARKER),
        OsString::from(profile),
        OsString::from(namespaces.flags().to_string()),
        program.as_os_str().to_os_string(),
    ];
    argv.extend(args.iter().cloned());
    argv
}

/// Recognise a re-execution, given the process's full argv.
///
/// Returns `None` for every ordinary invocation, so the caller falls through to
/// normal argument parsing.
pub fn parse_stage<I>(argv: I) -> Option<Stage>
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    let mut argv = argv.into_iter().map(Into::into);
    let _executable = argv.next()?;
    let marker = argv.next()?;

    if marker == ENTER_MARKER {
        let cgroup = PathBuf::from(argv.next()?);
        let profile = argv.next()?.into_string().ok()?;
        let namespaces = parse_namespaces(&argv.next()?)?;
        let program = PathBuf::from(argv.next()?);
        return Some(Stage::Enter {
            cgroup,
            profile,
            namespaces,
            program,
            args: argv.collect(),
        });
    }

    if marker == INIT_MARKER {
        let profile = argv.next()?.into_string().ok()?;
        let namespaces = parse_namespaces(&argv.next()?)?;
        let program = PathBuf::from(argv.next()?);
        return Some(Stage::Init {
            profile,
            namespaces,
            program,
            args: argv.collect(),
        });
    }

    None
}

fn parse_namespaces(field: &OsStr) -> Option<Namespaces> {
    Some(Namespaces::from_flags(field.to_str()?.parse().ok()?))
}

/// Run whichever stage this process was re-executed as.
///
/// Returns the exit code the process should use. [`Stage::Init`] only returns
/// on failure — on success `exec` has already replaced the process.
pub fn run_stage(stage: &Stage) -> std::result::Result<u8, SandboxError> {
    match stage {
        Stage::Enter {
            cgroup,
            profile,
            namespaces,
            program,
            args,
        } => enter(cgroup, profile, *namespaces, program, args),
        Stage::Init {
            profile,
            namespaces,
            program,
            args,
        } => Err(init(profile, *namespaces, program, args)),
    }
}

/// Outer stage: get into the cgroup, detach, hand off.
fn enter(
    cgroup_path: &Path,
    profile_name: &str,
    namespaces: Namespaces,
    program: &Path,
    args: &[OsString],
) -> std::result::Result<u8, SandboxError> {
    let profile = crate::profile::resolve(profile_name)?;
    let cgroup = Cgroup::attach(cgroup_path)?;

    let pid = std::process::id();
    cgroup.join(pid)?;

    // Read it back rather than trusting that a successful write means
    // membership. This is the check that catches a target that was never a
    // cgroup at all — every earlier step would have reported success.
    if !cgroup.contains(pid)? {
        return Err(SandboxError::JoinNotEffective {
            cgroup: cgroup_path.to_path_buf(),
            pid,
        });
    }

    if namespaces.any() {
        thalyx_syscall::unshare(namespaces.flags()).map_err(|source| {
            SandboxError::NamespacesUnavailable {
                profile: profile.name.to_string(),
                source,
            }
        })?;

        // Detach mount propagation before mounting anything.
        //
        // Without this, the `/proc` the inner stage mounts propagates back to
        // whatever the host's root is shared with — the sandbox would reach
        // out and change the system it is supposed to be contained by.
        if namespaces.mount {
            thalyx_syscall::mount(
                None,
                Path::new("/"),
                None,
                thalyx_syscall::MS_REC | thalyx_syscall::MS_PRIVATE,
                None,
            )
            .map_err(|source| SandboxError::MountFailed {
                what: "make / private".to_string(),
                source,
            })?;
        }
    }

    // The child is the first process of the new PID namespace, and inherits
    // the cgroup and every other namespace already established.
    let mut child = std::process::Command::new(current_exe()?)
        .args(init_argv(profile_name, namespaces, program, args))
        .spawn()
        .map_err(|source| SandboxError::Exec {
            program: program.to_path_buf(),
            source,
        })?;

    let status = child.wait().map_err(|source| SandboxError::Exec {
        program: program.to_path_buf(),
        source,
    })?;

    Ok(exit_code(&status))
}

/// Inner stage: PID 1 of the module's namespace.
///
/// Only returns on failure; on success it has become the module.
fn init(
    profile_name: &str,
    namespaces: Namespaces,
    program: &Path,
    args: &[OsString],
) -> SandboxError {
    use std::os::unix::process::CommandExt;

    let profile = match crate::profile::resolve(profile_name) {
        Ok(profile) => profile,
        Err(error) => return error,
    };

    // A `/proc` that reflects this PID namespace rather than the host's.
    //
    // Only mountable from in here: the kernel binds a `proc` mount to the PID
    // namespace of the process that mounts it. Doing it in the outer stage
    // would have given the module a view of every process on the machine.
    if namespaces.pid && namespaces.mount {
        let flags =
            thalyx_syscall::MS_NOSUID | thalyx_syscall::MS_NODEV | thalyx_syscall::MS_NOEXEC;
        if let Err(source) = thalyx_syscall::mount(
            Some(Path::new("proc")),
            Path::new(PROC),
            Some("proc"),
            flags,
            None,
        ) {
            return SandboxError::MountFailed {
                what: format!("mount a fresh {PROC}"),
                source,
            };
        }
    }

    if namespaces.uts
        && let Err(source) = thalyx_syscall::sethostname(profile.hostname)
    {
        return SandboxError::HostnameNotSet { source };
    }

    // Last, because everything above needs syscalls the filter denies. From
    // here the process may only do what the allowlist permits — including the
    // `execve` on the next line, and nothing after it that the module has not
    // been vouched for.
    if let Some(allowlist) = &profile.seccomp
        && let Err(error) = allowlist.install()
    {
        return SandboxError::Seccomp(error);
    }

    let error = std::process::Command::new(program).args(args).exec();

    SandboxError::Exec {
        program: program.to_path_buf(),
        source: error,
    }
}

/// The parent half: start the outer stage.
pub fn spawn(
    helper: &Path,
    cgroup: &Cgroup,
    profile: &Profile,
    program: &Path,
    args: &[OsString],
) -> Result<std::process::Child> {
    std::process::Command::new(helper)
        .args(enter_argv(
            cgroup.path(),
            profile.name,
            profile.namespaces,
            program,
            args,
        ))
        .spawn()
        .map_err(|source| SandboxError::Exec {
            program: helper.to_path_buf(),
            source,
        })
}

/// Resolve a module's entrypoint to a path inside its own tree.
///
/// A `..` or an absolute path here would run something outside the module,
/// with the module's permissions. The manifest is signed, so this is not the
/// first line of defence — but a signed manifest only proves who wrote it, not
/// that what they wrote was sane.
pub fn entrypoint_path(module_dir: &Path, entrypoint: &str) -> Result<PathBuf> {
    let relative = Path::new(entrypoint);

    let contained = !relative.as_os_str().is_empty()
        && !relative.is_absolute()
        && relative.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        });

    if !contained {
        return Err(SandboxError::EntrypointEscapes(entrypoint.to_string()));
    }

    let path = module_dir.join(relative);
    if !path.is_file() {
        return Err(SandboxError::NoSuchEntrypoint(path));
    }
    Ok(path)
}

/// Turn a slice of string-ish arguments into what [`spawn`] wants.
pub fn to_args<S: AsRef<OsStr>>(args: &[S]) -> Vec<OsString> {
    args.iter().map(|a| a.as_ref().to_os_string()).collect()
}

/// The exit code to report for a process that has finished.
///
/// A module killed by a signal is reported as `128 + signal`, the shell
/// convention — and the case that matters most here, because that is what a
/// seccomp `KILL_PROCESS` looks like from outside: `SIGSYS`, so 159.
fn exit_code(status: &std::process::ExitStatus) -> u8 {
    use std::os::unix::process::ExitStatusExt;

    if let Some(code) = status.code() {
        return code as u8;
    }
    match status.signal() {
        Some(signal) => 128u8.saturating_add(signal as u8),
        None => 1,
    }
}

fn current_exe() -> Result<PathBuf> {
    std::env::current_exe().map_err(|source| SandboxError::Exec {
        program: PathBuf::from("<current executable>"),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_enter_invocation_round_trips() {
        // The two halves of the launch have to agree on this format exactly.
        // They are built and parsed by the same module for that reason, and
        // this is what keeps them in step.
        let expected = crate::profile::module_standard().namespaces;
        let argv = enter_argv(
            Path::new("/sys/fs/cgroup/thalyx/org.thalyx.demo"),
            crate::profile::MODULE_STANDARD,
            expected,
            Path::new("/opt/thalyx/modules/org.thalyx.demo/current/bin/demo"),
            &to_args(&["--flag", "value with spaces"]),
        );

        let mut full = vec![OsString::from("/usr/bin/thalyx")];
        full.extend(argv);

        match parse_stage(full).expect("recognised") {
            Stage::Enter {
                cgroup,
                profile,
                namespaces,
                program,
                args,
            } => {
                assert_eq!(cgroup, Path::new("/sys/fs/cgroup/thalyx/org.thalyx.demo"));
                assert_eq!(profile, crate::profile::MODULE_STANDARD);
                assert_eq!(namespaces, expected);
                assert_eq!(
                    program,
                    Path::new("/opt/thalyx/modules/org.thalyx.demo/current/bin/demo")
                );
                assert_eq!(args, to_args(&["--flag", "value with spaces"]));
            }
            other => panic!("parsed as {other:?}"),
        }
    }

    #[test]
    fn an_init_invocation_round_trips() {
        let expected = crate::profile::module_standard().namespaces;
        let argv = init_argv(
            crate::profile::MODULE_STANDARD,
            expected,
            Path::new("/bin/demo"),
            &to_args(&["-x"]),
        );

        let mut full = vec![OsString::from("/usr/bin/thalyx")];
        full.extend(argv);

        match parse_stage(full).expect("recognised") {
            Stage::Init {
                profile,
                namespaces,
                program,
                args,
            } => {
                assert_eq!(profile, crate::profile::MODULE_STANDARD);
                assert_eq!(namespaces, expected);
                assert_eq!(program, Path::new("/bin/demo"));
                assert_eq!(args, to_args(&["-x"]));
            }
            other => panic!("parsed as {other:?}"),
        }
    }

    #[test]
    fn an_ordinary_invocation_is_not_mistaken_for_a_re_execution() {
        for argv in [
            vec!["thalyx"],
            vec!["thalyx", "module", "list"],
            vec!["thalyx", "enforce", "status"],
        ] {
            assert!(parse_stage(argv).is_none());
        }
    }

    #[test]
    fn a_module_argument_that_looks_like_a_marker_is_not_confused_for_one() {
        // The marker is only ever the first argument. A module invoked with it
        // as a parameter must not be treated as a re-execution.
        for marker in [ENTER_MARKER, INIT_MARKER] {
            let argv = vec!["thalyx", "module", "run", "org.thalyx.demo", marker];
            assert!(parse_stage(argv).is_none());
        }
    }

    #[test]
    fn a_truncated_re_execution_is_refused_rather_than_guessed() {
        assert!(parse_stage(vec!["thalyx", ENTER_MARKER]).is_none());
        assert!(parse_stage(vec!["thalyx", ENTER_MARKER, "/cgroup"]).is_none());
        assert!(parse_stage(vec!["thalyx", ENTER_MARKER, "/cgroup", "profile"]).is_none());
        assert!(parse_stage(vec!["thalyx", ENTER_MARKER, "/cgroup", "profile", "0"]).is_none());
        assert!(parse_stage(vec!["thalyx", INIT_MARKER]).is_none());
        assert!(parse_stage(vec!["thalyx", INIT_MARKER, "profile"]).is_none());
        assert!(parse_stage(vec!["thalyx", INIT_MARKER, "profile", "0"]).is_none());

        // A mask that is not a number is refused rather than read as zero:
        // zero means "no namespaces", which would silently run unisolated.
        assert!(parse_stage(vec!["thalyx", INIT_MARKER, "profile", "x", "/bin/sh"]).is_none());
    }

    #[test]
    fn an_entrypoint_stays_inside_the_module_tree() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(dir.path().join("bin/demo"), "#!/bin/sh\n").unwrap();

        let resolved = entrypoint_path(dir.path(), "bin/demo").unwrap();
        assert_eq!(resolved, dir.path().join("bin/demo"));
    }

    #[test]
    fn an_entrypoint_that_points_outside_the_module_is_refused() {
        // It would run with the module's granted permissions while not being
        // the module's code, which is the whole of the containment rule
        // inverted.
        let dir = tempfile::tempdir().unwrap();
        for entrypoint in ["../../bin/sh", "/bin/sh", "bin/../../escape", ""] {
            assert!(
                matches!(
                    entrypoint_path(dir.path(), entrypoint),
                    Err(SandboxError::EntrypointEscapes(_))
                ),
                "`{entrypoint}` should be refused"
            );
        }
    }

    #[test]
    fn a_missing_entrypoint_says_so_rather_than_failing_at_exec() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            entrypoint_path(dir.path(), "bin/absent"),
            Err(SandboxError::NoSuchEntrypoint(_))
        ));
    }
}
