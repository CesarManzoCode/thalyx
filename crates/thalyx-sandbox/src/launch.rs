//! Starting module code inside a cgroup, with nothing running outside it.
//!
//! The problem is one of ordering. A process cannot be put into a cgroup before
//! it exists, so between `fork` and the join there is a moment where the child
//! is outside every policy. If the module's own code ran in that moment, it
//! would run unconfined — briefly, but enough.
//!
//! The way out is for that moment to belong to Thalyx. The parent starts the
//! `thalyx` binary again with a marker argument; that child joins the cgroup,
//! confirms it is in it, and only then *becomes* the module through `exec`.
//! The module's first instruction is executed by a process that is already
//! confined, because the process that joined and the process that runs the
//! module are the same one.
//!
//! It is the same re-exec that container runtimes use, and it is chosen here
//! for a second reason: the alternative, `pre_exec`, is `unsafe`, and this
//! workspace forbids `unsafe` outright.

use crate::cgroup::Cgroup;
use crate::{Result, SandboxError};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// First argument of the re-executed helper.
///
/// Deliberately not a subcommand: this is an internal entry point, not
/// something a user should find in `--help`, and it must be recognised before
/// any argument parsing that could interpret the module's own arguments.
pub const ENTER_MARKER: &str = "__thalyx_sandbox_enter";

/// What the helper was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnterRequest {
    pub cgroup: PathBuf,
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

/// The argument vector the parent passes to the helper.
pub fn enter_argv(cgroup: &Path, program: &Path, args: &[OsString]) -> Vec<OsString> {
    let mut argv = vec![
        OsString::from(ENTER_MARKER),
        cgroup.as_os_str().to_os_string(),
        program.as_os_str().to_os_string(),
    ];
    argv.extend(args.iter().cloned());
    argv
}

/// Recognise a helper invocation, given the process's full argv.
///
/// Returns `None` for every ordinary invocation, so the caller falls through to
/// normal argument parsing.
pub fn parse_enter<I>(argv: I) -> Option<EnterRequest>
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    let mut argv = argv.into_iter().map(Into::into);
    let _executable = argv.next()?;

    if argv.next()? != ENTER_MARKER {
        return None;
    }

    let cgroup = PathBuf::from(argv.next()?);
    let program = PathBuf::from(argv.next()?);
    let args: Vec<OsString> = argv.collect();

    Some(EnterRequest {
        cgroup,
        program,
        args,
    })
}

/// The child half: join the cgroup, verify it, then become the program.
///
/// Never returns on success — `exec` replaces this process. Every failure path
/// returns instead of continuing, so a cgroup that could not be joined means
/// the module does not run at all. Running it unconfined would be the outcome
/// the whole mechanism exists to prevent.
pub fn enter_and_exec(request: &EnterRequest) -> SandboxError {
    use std::os::unix::process::CommandExt;

    let cgroup = match Cgroup::attach(&request.cgroup) {
        Ok(cgroup) => cgroup,
        Err(error) => return error,
    };

    let pid = std::process::id();
    if let Err(error) = cgroup.join(pid) {
        return error;
    }

    // Read it back rather than trusting that a successful write means
    // membership. This is the check that catches the case where the target was
    // never a cgroup at all — every earlier step would have reported success.
    match cgroup.contains(pid) {
        Ok(true) => {}
        Ok(false) => {
            return SandboxError::JoinNotEffective {
                cgroup: request.cgroup.clone(),
                pid,
            };
        }
        Err(error) => return error,
    }

    let error = std::process::Command::new(&request.program)
        .args(&request.args)
        .exec();

    SandboxError::Exec {
        program: request.program.clone(),
        source: error,
    }
}

/// The parent half: start the helper, which will confine itself and exec.
pub fn spawn(
    helper: &Path,
    cgroup: &Cgroup,
    program: &Path,
    args: &[OsString],
) -> Result<std::process::Child> {
    std::process::Command::new(helper)
        .args(enter_argv(cgroup.path(), program, args))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_helper_invocation_round_trips() {
        // The two halves of the launch have to agree on this format exactly.
        // They are built and parsed by the same module for that reason, and
        // this is what keeps them in step.
        let argv = enter_argv(
            Path::new("/sys/fs/cgroup/thalyx/org.thalyx.demo"),
            Path::new("/opt/thalyx/modules/org.thalyx.demo/current/bin/demo"),
            &to_args(&["--flag", "value with spaces"]),
        );

        let mut full = vec![OsString::from("/usr/bin/thalyx")];
        full.extend(argv);

        let request = parse_enter(full).expect("recognised");
        assert_eq!(
            request.cgroup,
            Path::new("/sys/fs/cgroup/thalyx/org.thalyx.demo")
        );
        assert_eq!(
            request.program,
            Path::new("/opt/thalyx/modules/org.thalyx.demo/current/bin/demo")
        );
        assert_eq!(request.args, to_args(&["--flag", "value with spaces"]));
    }

    #[test]
    fn an_ordinary_invocation_is_not_mistaken_for_a_helper_one() {
        for argv in [
            vec!["thalyx"],
            vec!["thalyx", "module", "list"],
            vec!["thalyx", "enforce", "status"],
        ] {
            assert!(parse_enter(argv).is_none());
        }
    }

    #[test]
    fn a_module_argument_that_looks_like_the_marker_is_not_confused_for_it() {
        // The marker is only ever the first argument. A module invoked with it
        // as a parameter must not be treated as a helper invocation.
        let argv = vec!["thalyx", "module", "run", "org.thalyx.demo", ENTER_MARKER];
        assert!(parse_enter(argv).is_none());
    }

    #[test]
    fn a_truncated_helper_invocation_is_refused_rather_than_guessed() {
        assert!(parse_enter(vec!["thalyx", ENTER_MARKER]).is_none());
        assert!(parse_enter(vec!["thalyx", ENTER_MARKER, "/cgroup"]).is_none());
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
