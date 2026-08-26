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
//! enter   check the cgroup, unshare namespaces, spawn init
//!   └─ init   (PID 1 of the new namespace)
//!             assemble the root, JOIN THE CGROUP, pivot into it,
//!             mount /proc, set hostname, drop to the module's user,
//!             install seccomp, exec the module
//! ```
//!
//! Splitting it costs nothing that matters. The namespaces are inherited across
//! `fork`. What `init` adds is everything that can only be done from inside — a
//! `/proc` that reflects the new PID namespace, and a seccomp filter that must
//! not constrain the setup work that comes before it.
//!
//! ## Why the join is where it is
//!
//! The cgroup is the identity every kernel policy is keyed on, so joining it is
//! the moment the process starts being governed by the module's permissions.
//! Until 2026-08-26 it was the *first* thing `enter` did, and the consequence
//! was that Thalyx's own confinement work ran under the module's policy: making
//! the mount point for `/dev/null` is `open(O_WRONLY|O_CREAT)`, `lsm/file_open`
//! asked for `FS_WRITE`, and a module that was granted nothing to write got
//! `-EPERM` for a file it never named. Nothing could be launched on an
//! enforcing kernel at all.
//!
//! So the join moved to the one place that satisfies both halves: after every
//! write Thalyx has to do, and before anything of the module's own exists. The
//! ordering above is load-bearing, not incidental — `pivot_root`, `chdir`,
//! `umount2`, `rmdir`, `sethostname`, `setuid` and `seccomp` open no files, and
//! the two reads that remain (`cgroup.procs` and the module's own binary at
//! `execve`) are what `permd::CONFINED_FLOOR` exists to allow.
//!
//! ## Failure is always closed
//!
//! Every step returns instead of continuing. A module that could not be
//! confined does not run, because running it is the outcome the whole
//! mechanism exists to prevent.

use crate::cgroup::Cgroup;
use crate::profile::Namespaces;
use crate::rootfs::RootFs;
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

/// Everything the re-executed process needs to build the confinement.
///
/// Carried as one JSON argument rather than a growing row of positional ones.
/// The row worked while there were three things to say; it stopped working the
/// moment the root filesystem had to travel too, and a launch protocol that is
/// awkward to extend is a launch protocol that will be extended wrongly.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LaunchSpec {
    pub cgroup: PathBuf,
    pub profile: String,
    /// The namespace mask the parent settled on, after adjusting the named
    /// profile for what the module was actually granted.
    ///
    /// Travels explicitly because the child must not re-derive it: the parent
    /// already adjusted for the grants, and a second derivation from the
    /// profile name alone silently disagreed.
    pub namespaces: Namespaces,
    /// The root to pivot into. `None` leaves the module in the host tree.
    pub rootfs: Option<RootFs>,
    /// The entrypoint, as a path on the host.
    pub program: PathBuf,
    /// The unprivileged user the module becomes before it runs.
    ///
    /// `None` leaves it as whatever Thalyx runs as. The number is assigned by
    /// the core, which owns the map from module to user; the launcher only
    /// applies it.
    pub uid: Option<u32>,
    /// The descriptor carrying the module's channel to Thalyx, as numbered in
    /// *this* process.
    ///
    /// It travels as a number rather than as a descriptor because that is all
    /// there is to carry: `exec` preserves descriptor numbers, so the same
    /// integer names the same socket in every stage. What it does not preserve
    /// is `FD_CLOEXEC`, which is why the parent clears it before the first
    /// `exec` and why the last stage moves the descriptor onto
    /// [`thalyx_syscall::CHANNEL_FD`] with `dup2` rather than leaving it
    /// wherever it happened to land.
    ///
    /// `None` runs a module with no channel at all. That is not a degraded
    /// mode to reach for: a module without it has no system to talk to, and
    /// the only reason it exists is the sandbox's own tests, which launch
    /// programs that are not modules.
    #[serde(default)]
    pub channel_fd: Option<i32>,
}

/// What a re-executed `thalyx` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Outer: join the cgroup and detach into namespaces.
    Enter {
        spec: LaunchSpec,
        args: Vec<OsString>,
    },
    /// Inner: finish the confinement from inside, then become the module.
    Init {
        spec: LaunchSpec,
        args: Vec<OsString>,
    },
    /// A short-lived helper that exists only to hold a user namespace open
    /// while the launcher writes its id map. See [`crate::idmap`].
    Userns,
}

impl Stage {
    pub fn spec(&self) -> Option<&LaunchSpec> {
        match self {
            Stage::Enter { spec, .. } | Stage::Init { spec, .. } => Some(spec),
            Stage::Userns => None,
        }
    }
}

/// Build the argument vector for a re-execution.
///
/// The module's own arguments stay as trailing `OsString`s rather than going
/// into the JSON: JSON cannot carry a non-UTF-8 argument, and silently mangling
/// what a caller passed through is not a trade worth making for tidiness.
pub fn argv(marker: &str, spec: &LaunchSpec, args: &[OsString]) -> Result<Vec<OsString>> {
    let encoded = serde_json::to_string(spec).map_err(|source| SandboxError::Spec {
        direction: "encoded",
        source,
    })?;

    let mut argv = vec![OsString::from(marker), OsString::from(encoded)];
    argv.extend(args.iter().cloned());
    Ok(argv)
}

/// Recognise a re-execution, given the process's full argv.
///
/// Returns `None` for every ordinary invocation, so the caller falls through to
/// normal argument parsing.
pub fn parse_stage<I>(input: I) -> Option<Stage>
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    let mut input = input.into_iter().map(Into::into);
    let _executable = input.next()?;
    let marker = input.next()?;

    if marker == crate::idmap::USERNS_MARKER {
        return Some(Stage::Userns);
    }

    if marker != ENTER_MARKER && marker != INIT_MARKER {
        return None;
    }

    let spec: LaunchSpec = serde_json::from_str(input.next()?.to_str()?).ok()?;
    let args: Vec<OsString> = input.collect();

    if marker == ENTER_MARKER {
        Some(Stage::Enter { spec, args })
    } else {
        Some(Stage::Init { spec, args })
    }
}

/// Run whichever stage this process was re-executed as.
///
/// Returns the exit code the process should use. [`Stage::Init`] only returns
/// on failure — on success `exec` has already replaced the process.
pub fn run_stage(stage: &Stage) -> std::result::Result<u8, SandboxError> {
    match stage {
        Stage::Enter { spec, args } => enter(spec, args),
        Stage::Init { spec, args } => Err(init(spec, args)),
        Stage::Userns => crate::idmap::run_userns_helper(),
    }
}

/// Outer stage: get into the cgroup, detach, hand off.
fn enter(spec: &LaunchSpec, args: &[OsString]) -> std::result::Result<u8, SandboxError> {
    let profile = crate::profile::resolve(&spec.profile)?;
    let namespaces = spec.namespaces;

    // Checked here and joined in `init`. Attaching only reads — it is the
    // `cgroup.controllers` stat that refuses an ordinary directory — so this
    // costs nothing and fails before a child exists, which is a better place
    // to find out that the cgroup was never one.
    Cgroup::attach(&spec.cgroup)?;

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
        .args(argv(INIT_MARKER, spec, args)?)
        .spawn()
        .map_err(|source| SandboxError::Exec {
            program: spec.program.clone(),
            source,
        })?;

    let status = child.wait().map_err(|source| SandboxError::Exec {
        program: spec.program.clone(),
        source,
    })?;

    Ok(exit_code(&status))
}

/// Inner stage: PID 1 of the module's namespace.
///
/// Only returns on failure; on success it has become the module.
fn init(spec: &LaunchSpec, args: &[OsString]) -> SandboxError {
    use std::os::unix::process::CommandExt;

    let profile = match crate::profile::resolve(&spec.profile) {
        Ok(profile) => profile,
        Err(error) => return error,
    };
    let namespaces = spec.namespaces;

    // A `/proc` that matches this process's PID namespace, before anything
    // that reads it.
    //
    // `enter` unshared `CLONE_NEWPID`, so this process is PID 1 of a new
    // namespace while `/proc` is still the one the host mounted. Every pid in
    // that `/proc` belongs to a different namespace — so `/proc/<child>` for a
    // child of this process names *somebody else's* process entirely.
    //
    // That is not theoretical. The idmapped mounts below write a child's
    // `uid_map` through `/proc`, and the first attempt wrote it for whatever
    // unrelated process happened to hold that number on the host. It failed
    // with `EPERM`, which is the good outcome; the bad one was available.
    //
    // Mounting here is invisible outside: `enter` already made the mount
    // namespace private.
    if namespaces.pid
        && namespaces.mount
        && let Err(source) = mount_private_proc()
    {
        return SandboxError::MountFailed {
            what: format!("mount a {PROC} matching this PID namespace"),
            source,
        };
    }

    // Build the root, but do not enter it yet.
    //
    // Every write Thalyx does on the module's behalf is in here: the mount
    // points for `/dev/null` and the four device nodes beside it, and the
    // `uid_map` of the helper that makes a remapped bind. All of it happens
    // before the join below, and that ordering is the fix for the whole class.
    // See `RootFs::assemble`.
    if let Some(rootfs) = &spec.rootfs
        && let Err(error) = rootfs.assemble()
    {
        return error;
    }

    // The cgroup, and with it the identity every kernel policy is keyed on.
    //
    // Here and not in `enter`, which is where it used to be. The launcher used
    // to take on the module's policy first and do its setup afterwards, so the
    // LSM answered `-EPERM` to Thalyx creating a mount point — a write the
    // module never asked for, could not have been granted, and which the
    // confinement itself requires. `verify.sh` came back with twelve `FAILED`
    // reading `I/O error at /run/thalyx/sandbox/dev/null: Operation not
    // permitted`, all of them this.
    //
    // Nothing of the module's runs any earlier because of this. `execve` is
    // still the last line of this function, and everything between here and it
    // — the pivot, `/proc`, the hostname, the user, the filter — is Thalyx's.
    // The moment before the join was always meant to belong to Thalyx; it is
    // now long enough to include the work Thalyx actually has to do.
    let cgroup = match Cgroup::attach(&spec.cgroup) {
        Ok(cgroup) => cgroup,
        Err(error) => return error,
    };
    let pid = std::process::id();
    if let Err(error) = cgroup.join(pid) {
        return error;
    }

    // Read it back rather than trusting that a successful write means
    // membership. This is the check that catches a target that was never a
    // cgroup at all — every earlier step would have reported success.
    //
    // It is also the read `CONFINED_FLOOR` was written for on 2026-08-25: the
    // process is inside the cgroup now, and this is a file open.
    match cgroup.contains(pid) {
        Ok(true) => {}
        Ok(false) => {
            return SandboxError::JoinNotEffective {
                cgroup: spec.cgroup.clone(),
                pid,
            };
        }
        Err(error) => return error,
    }

    // And now enter the root. Nothing below opens a file for writing, which is
    // what makes it safe on this side of the join.
    //
    // After this the host tree is gone: the module can only reach its own
    // files, the read-only system paths it needs to start, and exactly what it
    // was granted.
    let mut program = spec.program.clone();
    if let Some(rootfs) = &spec.rootfs {
        if let Err(error) = rootfs.pivot_into() {
            return error;
        }
        match rootfs.program_inside(&spec.program) {
            Ok(inside) => program = inside,
            Err(error) => return error,
        }
    }

    // A `/proc` that reflects this PID namespace rather than the host's.
    //
    // Only mountable from in here: the kernel binds a `proc` mount to the PID
    // namespace of the process that mounts it. Doing it in the outer stage
    // would have given the module a view of every process on the machine.
    if namespaces.pid
        && namespaces.mount
        && let Err(source) = mount_private_proc()
    {
        return SandboxError::MountFailed {
            what: format!("mount a fresh {PROC} inside the module's root"),
            source,
        };
    }

    if namespaces.uts
        && let Err(source) = thalyx_syscall::sethostname(profile.hostname)
    {
        return SandboxError::HostnameNotSet { source };
    }

    // Become the module's own user.
    //
    // After everything that needed privilege — the mounts, the hostname — and
    // before the seccomp filter, which denies `setuid` outright so the module
    // can never do this itself, in either direction.
    //
    // Groups first, then group, then user: once the process stops being root
    // it can change none of them, so the order is the whole of the safety.
    if let Some(uid) = spec.uid {
        if let Err(source) = thalyx_syscall::drop_supplementary_groups() {
            return SandboxError::UserNotDropped { uid, source };
        }
        if let Err(source) = thalyx_syscall::set_gid(uid) {
            return SandboxError::UserNotDropped { uid, source };
        }
        if let Err(source) = thalyx_syscall::set_uid(uid) {
            return SandboxError::UserNotDropped { uid, source };
        }

        // Read it back. A `setresuid` that reported success and left the
        // process as root would hand the module everything, and every step
        // after this looks identical either way.
        let effective = thalyx_syscall::effective_uid();
        if effective != uid {
            return SandboxError::UserNotEffective {
                wanted: uid,
                actual: effective,
            };
        }
    }

    // The channel, onto the number the module looks for.
    //
    // Before the seccomp filter, because `dup2` is on the allowlist but this
    // does not need to depend on that staying true — and after everything that
    // could still fail, so a module never starts holding a channel to a system
    // that then refused to confine it.
    //
    // The socket itself was created by the parent and inherited across both
    // `exec`s. Nothing here opens anything: `socket` is deliberately absent
    // from the allowlist, so a module can use the channel it was given and
    // cannot make itself another.
    if let Some(number) = spec.channel_fd
        && let Err(source) = thalyx_syscall::place_on(number, thalyx_syscall::CHANNEL_FD)
    {
        return SandboxError::ChannelNotPlaced {
            number: thalyx_syscall::CHANNEL_FD,
            source,
        };
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

    let error = std::process::Command::new(&program).args(args).exec();

    SandboxError::Exec {
        program,
        source: error,
    }
}

/// The parent half: start the outer stage.
///
/// ## Why the module does not get the terminal
///
/// `stdin` is closed and `stdout`/`stderr` are pipes Thalyx holds the other
/// end of, and that is a security property rather than tidiness.
///
/// The module used to inherit all three from Thalyx, which meant it shared a
/// terminal with the trusted path. Two things follow from that, and both
/// undo what `vault/11-Seguridad/Camino-Confiable.md` is for:
///
/// - **It could read `stdin`.** The confirmation prompt is answered by typing
///   at that same descriptor. A running module racing the human's `y` gets to
///   consume it, or worse, gets to see what the human is answering to a
///   different question.
/// - **It could write `stdout`.** Anything at all, including a box-drawn frame
///   that says `┌─ Thalyx — capability authorisation`. The frame exists so the
///   human can tell Thalyx apart from what runs inside it, and a module with
///   the terminal can draw the frame itself.
///
/// The channel on descriptor 3 is how a module talks to the human, through
/// Thalyx, labelled and bounded. That is the whole design — a module has a
/// channel precisely so it does not need a terminal, and `dev.thalyx.greeter`
/// says so in its own module docs: *over the channel rather than to a terminal,
/// because a terminal is not something it has.* It had one.
///
/// ## Why a pipe and not the null device
///
/// It was the null device, and that threw away the answer to the only question
/// that establishes any of this. `dev/verify.sh` proves the sandbox by asking
/// the confined program what it can see — its pid, its uid, its hostname, its
/// interfaces — because asking Thalyx whether it confined something proves
/// nothing. Every one of those answers travelled on `stdout`. Discarding it
/// made six checks report `nothing`, which is also what a sandbox that
/// isolated nothing would report: the containment measure blinded the
/// instrument that shows containment works.
///
/// It cost the other direction too. A module that dies with a message on
/// `stderr` left no message, so *it failed* and *it failed for this reason*
/// became the same event — the distinction `CLAUDE.md` rule 10 is about.
///
/// A pipe keeps both without giving anything back. The module still cannot
/// read what the human types, and what it writes reaches the screen only
/// through Thalyx, one line at a time behind a marker and with control
/// characters replaced — the same treatment the channel already gets, for the
/// same reason. **The caller must drain both pipes while the module runs**: a
/// pipe whose reader never empties it blocks the module forever on its first
/// write, which is what the null device was avoiding.
pub fn spawn(helper: &Path, spec: &LaunchSpec, args: &[OsString]) -> Result<std::process::Child> {
    use std::process::Stdio;

    std::process::Command::new(helper)
        .args(argv(ENTER_MARKER, spec, args)?)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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

/// A `proc` bound to the calling process's PID namespace.
fn mount_private_proc() -> std::io::Result<()> {
    thalyx_syscall::mount(
        Some(Path::new("proc")),
        Path::new(PROC),
        Some("proc"),
        thalyx_syscall::MS_NOSUID | thalyx_syscall::MS_NODEV | thalyx_syscall::MS_NOEXEC,
        None,
    )
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

    fn spec() -> LaunchSpec {
        LaunchSpec {
            cgroup: PathBuf::from("/sys/fs/cgroup/thalyx/org.thalyx.demo"),
            profile: crate::profile::MODULE_STANDARD.to_string(),
            namespaces: crate::profile::module_standard().namespaces,
            rootfs: Some(
                crate::rootfs::RootFs::for_module(
                    Path::new("/opt/thalyx/modules/org.thalyx.demo/1.0.0"),
                    &[],
                )
                .unwrap(),
            ),
            program: PathBuf::from("/opt/thalyx/modules/org.thalyx.demo/1.0.0/bin/demo"),
            uid: Some(700_000),
            channel_fd: Some(7),
        }
    }

    #[test]
    fn a_re_execution_round_trips_with_everything_it_carries() {
        // The two halves of the launch have to agree on this exactly. They are
        // built and parsed by the same module for that reason, and this is
        // what keeps them in step as the spec grows.
        let spec = spec();
        let args = to_args(&["--flag", "value with spaces"]);

        for (marker, expected_init) in [(ENTER_MARKER, false), (INIT_MARKER, true)] {
            let mut full = vec![OsString::from("/usr/bin/thalyx")];
            full.extend(argv(marker, &spec, &args).unwrap());

            let stage = parse_stage(full).expect("recognised");
            assert_eq!(stage.spec(), Some(&spec));
            assert_eq!(matches!(stage, Stage::Init { .. }), expected_init);

            match stage {
                Stage::Enter { args: got, .. } | Stage::Init { args: got, .. } => {
                    assert_eq!(got, args);
                }
                Stage::Userns => panic!("a launch is not a namespace helper"),
            }
        }
    }

    #[test]
    fn module_arguments_survive_even_when_they_are_not_utf8() {
        // They travel outside the JSON for exactly this reason. Mangling what a
        // caller passed through would be a silent corruption of the module's
        // own input.
        use std::os::unix::ffi::OsStringExt;
        let odd = OsString::from_vec(vec![0xff, 0xfe, b'a']);

        let mut full = vec![OsString::from("/usr/bin/thalyx")];
        full.extend(argv(ENTER_MARKER, &spec(), std::slice::from_ref(&odd)).unwrap());

        match parse_stage(full).expect("recognised") {
            Stage::Enter { args, .. } => assert_eq!(args, vec![odd]),
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
    fn a_malformed_re_execution_is_refused_rather_than_guessed() {
        // Every one of these would, if guessed at, produce a launch with less
        // confinement than the parent asked for.
        assert!(parse_stage(vec!["thalyx", ENTER_MARKER]).is_none());
        assert!(parse_stage(vec!["thalyx", INIT_MARKER]).is_none());
        assert!(parse_stage(vec!["thalyx", ENTER_MARKER, "not json"]).is_none());
        assert!(parse_stage(vec!["thalyx", ENTER_MARKER, "{}"]).is_none());
        assert!(
            parse_stage(vec![
                "thalyx",
                ENTER_MARKER,
                r#"{"cgroup":"/c","profile":"module_standard","program":"/bin/sh"}"#
            ])
            .is_none(),
            "a spec missing the namespace mask must not be read as no namespaces"
        );
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
