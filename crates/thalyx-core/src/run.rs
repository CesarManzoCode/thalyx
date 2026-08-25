//! Running an installed module under the policy it was granted.
//!
//! This is where the enforcement loop closes. Until now `thalyx-permd` could
//! write a policy into the kernel and `thalyx enforce apply` could be typed by
//! hand, which meant the ordinary path — install a module, run it — enforced
//! nothing at all. The registry said one thing and the machine did another.
//!
//! ## Why this lives in the core and not in the sandbox
//!
//! `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md` places the sandbox outside
//! the TCB: it contains module code, and it is not trusted. Deciding *what a
//! module may do* is therefore not its job. The core resolves the entrypoint,
//! reads the permissions actually in force, refuses what cannot be enforced,
//! and records the attempt; the sandbox is handed a decision already made and
//! only has to contain and launch.
//!
//! That is also why the dependency points this way. Reversing it would put the
//! decision inside the component that is not trusted to make it.
//!
//! ## Why not in the CLI
//!
//! `vault/04-Flujo-Canonico/Coherencia-Doble-Ruta.md` requires the human's
//! route and the agent's route to leave the system in the same state. An
//! orchestration written in the CLI would have to be written a second time for
//! the agent, and the two would drift. There is one implementation, and both
//! routes call it.

use crate::permissions::Registry;
use crate::store::Store;
use crate::{CoreError, Result};
use std::ffi::OsString;
use std::path::PathBuf;
use thalyx_journal::{Entry, Journal, Origin, Outcome};
use thalyx_permd::PolicyStore;
use thalyx_sandbox::Confinement;

/// The entrypoint used when the caller does not name one.
pub const DEFAULT_ENTRYPOINT: &str = "run";

pub struct RunRequest<'a> {
    pub module_id: &'a str,
    /// Which of the manifest's entrypoints to start.
    pub entrypoint: &'a str,
    pub args: Vec<OsString>,
    /// The binary that re-executes itself into the cgroup before becoming the
    /// module. In practice `thalyx` itself; see `thalyx_sandbox::launch`.
    pub helper: PathBuf,
    pub request_id: String,
    pub origin: Origin,
    /// The sandbox profile to run under.
    ///
    /// Named by the caller rather than read from the module: a module choosing
    /// its own isolation is a module choosing not to be isolated. See
    /// `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md`.
    pub profile: &'a str,
    /// Run with no confinement at all: no kernel policy, no sandbox.
    ///
    /// Honoured whether or not the kernel side is available. It exists so the
    /// state can be reached deliberately and named in the journal, rather than
    /// reached by accident and named nothing — and a flag that quietly did
    /// nothing on a capable machine would be that same failure wearing the
    /// opposite hat.
    pub unconfined: bool,
}

#[derive(Debug)]
pub struct RunOutcome {
    pub module_id: String,
    pub version: String,
    pub program: PathBuf,
    /// `None` when the module ran unconfined.
    pub cgroup_id: Option<u64>,
    pub policy: Option<thalyx_permd::Policy>,
    /// How the module was isolated, as it ended up after the grant-dependent
    /// adjustments. `None` when it ran unconfined.
    pub isolation: Option<String>,
    /// Whether the profile in force actually isolated anything.
    pub isolated: bool,
    pub permissions: Vec<thalyx_manifest::Permission>,
    /// The user the module ran as. `None` when it ran as Thalyx itself.
    pub uid: Option<u32>,
    pub exit_code: Option<i32>,
    /// What the module said to the human over its channel, in order.
    pub said: Vec<(thalyx_abi::Level, String)>,
    /// What the module wrote to its own `stdout` and `stderr`.
    ///
    /// Not the channel, and reported as a different thing: the channel is the
    /// surface Thalyx mediates, and this is a module writing bytes at a
    /// descriptor. Kept because it is the only way to ask a confined program
    /// what it can see — which is how the sandbox is shown to work at all —
    /// and because a module that dies with a message on `stderr` has to be
    /// able to say why.
    ///
    /// The two are held apart rather than interleaved. They arrive on separate
    /// pipes read by separate threads, so any order across them would be an
    /// order Thalyx made up.
    pub wrote: ModuleOutput,
    /// Notices refused because the module was past the ceiling Thalyx holds
    /// for one run. Carried so the caller can say so: a truncated list and a
    /// module that went quiet look identical otherwise.
    pub dropped_notices: usize,
    /// Why the channel stopped, when it stopped badly.
    ///
    /// Carried rather than logged because a module whose requests went
    /// unanswered halfway through exits looking exactly like one that
    /// finished, and only this distinguishes them.
    pub channel_error: Option<String>,
}

impl RunOutcome {
    pub fn confined(&self) -> bool {
        self.cgroup_id.is_some()
    }
}

/// What a module wrote at its own descriptors, bounded.
#[derive(Debug, Default, Clone)]
pub struct ModuleOutput {
    pub stdout: String,
    pub stderr: String,
    /// Set when Thalyx stopped keeping what the module wrote before the module
    /// stopped writing.
    ///
    /// Said out loud by the caller, for the same reason `dropped_notices` is:
    /// output that silently stopped growing and a module that stopped talking
    /// look identical, and they are different events.
    pub truncated: bool,
}

impl ModuleOutput {
    pub fn is_empty(&self) -> bool {
        self.stdout.is_empty() && self.stderr.is_empty()
    }
}

/// The most Thalyx keeps of what one module wrote at one descriptor.
///
/// The bound is on what is *kept*, never on what is read — the drain below
/// goes on emptying the pipe after the ceiling, discarding as it goes. Those
/// are different things and confusing them is how a memory limit becomes a
/// hang: a reader that stops reading at the ceiling blocks the module on its
/// next write, for good.
///
/// This is the same ceiling `api.rs` puts on notices and for the same reason —
/// the 2026-08-04 audit found a module could grow Thalyx's memory without
/// limit by talking, and a module can talk at a descriptor too.
const MAX_KEPT_OUTPUT: usize = 64 * 1024;

/// Empty one of the module's pipes, keeping the first [`MAX_KEPT_OUTPUT`].
///
/// In a thread because both pipes and the `wait` have to be happening at once.
/// A module that fills the pipe buffer while Thalyx waits for it to exit is a
/// deadlock, and it is the ordinary case rather than a hostile one: the buffer
/// is 64 KiB on Linux and a module that prints a directory can reach it.
pub(crate) fn drain<R: std::io::Read + Send + 'static>(
    stream: Option<R>,
) -> std::thread::JoinHandle<(String, bool)> {
    std::thread::spawn(move || {
        let Some(mut stream) = stream else {
            return (String::new(), false);
        };

        let mut kept: Vec<u8> = Vec::new();
        let mut truncated = false;
        let mut buffer = [0u8; 8192];

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let room = MAX_KEPT_OUTPUT.saturating_sub(kept.len());
                    let take = room.min(read);
                    kept.extend_from_slice(&buffer[..take]);
                    truncated |= take < read;
                }
                // A read that fails is a failure to read, not a module that
                // said nothing. It ends the drain — the alternative is
                // spinning on the same error forever — and what was kept up to
                // here is still what the module wrote.
                Err(_) => {
                    truncated = true;
                    break;
                }
            }
        }

        // Lossy on purpose. A module is bytes, not text, and one that writes
        // something that is not UTF-8 must not be able to make Thalyx fail to
        // report the rest of what it wrote.
        (String::from_utf8_lossy(&kept).into_owned(), truncated)
    })
}

/// Join both drains into one [`ModuleOutput`].
///
/// A panicked drain thread is reported as truncation rather than propagated:
/// the module has already run, and losing its result because reading its
/// output went wrong would turn a reporting fault into a failed run.
pub(crate) fn collect(
    out: std::thread::JoinHandle<(String, bool)>,
    err: std::thread::JoinHandle<(String, bool)>,
) -> ModuleOutput {
    let (stdout, out_cut) = out.join().unwrap_or_else(|_| (String::new(), true));
    let (stderr, err_cut) = err.join().unwrap_or_else(|_| (String::new(), true));
    ModuleOutput {
        stdout,
        stderr,
        truncated: out_cut || err_cut,
    }
}

/// Start a module, wait for it, and tear its confinement down.
pub fn run(
    store: &Store,
    policies: &dyn PolicyStore,
    request: RunRequest<'_>,
) -> Result<RunOutcome> {
    let journal = Journal::open(store.journal_path())?;

    match run_inner(store, policies, &request) {
        Ok(outcome) => {
            let mut notes = vec![match outcome.exit_code {
                Some(0) => "module exited cleanly".to_string(),
                Some(code) => format!("module exited with status {code}"),
                None => "module was terminated by a signal".to_string(),
            }];
            if let Some(uid) = outcome.uid {
                notes.push(format!("ran as user {uid}"));
            }
            notes.push(match outcome.cgroup_id {
                Some(id) => format!(
                    "confined to cgroup {id} with allowed=0x{:x}",
                    outcome.policy.map(|p| p.allowed).unwrap_or(0)
                ),
                None => "RAN UNCONFINED: no kernel policy was in force".to_string(),
            });
            if let Some(isolation) = &outcome.isolation {
                notes.push(isolation.clone());
            }

            journal.append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: "run_module".to_string(),
                module_id: Some(outcome.module_id.clone()),
                version: Some(outcome.version.clone()),
                // The module having exited non-zero is the module's business.
                // What succeeded or failed here is Thalyx's part: launching it
                // under the policy it was granted, with the isolation the
                // profile promised. Anything less is degraded and says so —
                // a run nobody can tell apart from a confined one is the
                // failure this project keeps arranging against.
                outcome: match (outcome.confined(), outcome.isolated) {
                    (true, true) => Outcome::Success,
                    (true, false) => Outcome::Degraded {
                        reason: "ran under a profile that isolates nothing".to_string(),
                    },
                    (false, _) => Outcome::Degraded {
                        reason: "ran without kernel enforcement".to_string(),
                    },
                },
                request_id: request.request_id.clone(),
                origin: request.origin,
                snapshot: None,
                notes,
            })?;
            Ok(outcome)
        }
        Err(error) => {
            let _ = journal.append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: "run_module".to_string(),
                module_id: Some(request.module_id.to_string()),
                version: None,
                outcome: Outcome::Rejected {
                    reason: error.to_string(),
                },
                request_id: request.request_id.clone(),
                origin: request.origin,
                snapshot: None,
                notes: vec![],
            });
            Err(error)
        }
    }
}

fn run_inner(
    store: &Store,
    policies: &dyn PolicyStore,
    request: &RunRequest<'_>,
) -> Result<RunOutcome> {
    // The manifest is re-verified against the pinned publisher key on this
    // read. It is the authority on the entrypoint, so believing a tampered
    // copy would mean launching something else with this module's permissions.
    let manifest = crate::installed_manifest(store, request.module_id)?;

    let entrypoint = manifest
        .entrypoints
        .get(request.entrypoint)
        .ok_or_else(|| {
            CoreError::Sandbox(thalyx_sandbox::SandboxError::NoSuchEntrypointName {
                module_id: request.module_id.to_string(),
                entrypoint: request.entrypoint.to_string(),
            })
        })?;

    // The resolved version directory, not the `current` symlink: an upgrade
    // while this module is running must not change which bytes get executed.
    let module_dir = store.version_dir(&manifest.id, &manifest.version);
    let program = thalyx_sandbox::launch::entrypoint_path(&module_dir, entrypoint)?;

    // What the module actually holds — not what its manifest asks for. A grant
    // is in force only while the module is the current version.
    let registry = Registry::load(store.permissions_path())?;
    let permissions: Vec<thalyx_manifest::Permission> =
        crate::effective_permissions(store, &registry, &manifest.id)
            .iter()
            .map(|grant| thalyx_manifest::Permission {
                resource: grant.resource.clone(),
                action: grant.action.clone(),
                kind: grant.kind,
            })
            .collect();

    // `--unconfined` means what it says, always.
    //
    // It used to apply only when the kernel side happened to be missing, so on
    // a machine where enforcement worked the flag silently did nothing. A flag
    // that does nothing is the same failure as a permission that enforces
    // nothing: the system does something other than what it was told, and says
    // so nowhere. Asking for an unconfined run gets one, and the journal
    // records it as degraded.
    if request.unconfined {
        return run_unconfined(&manifest, &program, request, permissions);
    }

    // The profile name, before the kernel is asked anything.
    //
    // This used to sit below the `is_available` gate, and the ordering hid a
    // caller that asked for a profile no name matches: on every machine that
    // could not enforce — which was every machine but the image — the honest
    // "nothing can enforce this" came back first and the name was never looked
    // at. The one place the lookup ran was the machine's own console, after an
    // install had already succeeded.
    //
    // A name nothing resolves is wrong on a machine with no BPF at all, so it
    // is reported there too. Nothing is established yet, so there is nothing
    // to unwind.
    let profile = thalyx_sandbox::profile::resolve(request.profile)?;

    if !policies.is_available() {
        return Err(CoreError::NothingCanEnforce {
            module_id: manifest.id.clone(),
            permissions: permissions.len(),
        });
    }

    // The user this module runs as, assigned once and kept forever.
    //
    // Before the confinement is established, so a module that cannot be given
    // a user does not get a cgroup and a policy first.
    // Assigned under the global lock, and only for as long as that takes.
    //
    // The uid registry is shared mutable state — two first runs of different
    // modules racing here could be handed the same number, which is exactly
    // the sharing `uids.rs` exists to prevent. But a run must not hold the
    // lock while the module executes: a module that runs for an hour would
    // block every install for an hour, and the decree serialises contracts,
    // not the programs they start.
    let uid = if profile.own_user {
        let _lock = store.lock()?;
        let mut uids = crate::uids::UidRegistry::load(store.uids_path())?;
        Some(uids.assign(&manifest.id)?)
    } else {
        None
    };

    let parent = thalyx_sandbox::cgroup::parent()?;
    let confinement = Confinement::establish(
        policies,
        &parent,
        &manifest.id,
        profile,
        &permissions,
        thalyx_permd::boot_ns(),
        thalyx_permd::DEFAULT_JIT_LIFETIME_NS,
    )?;

    let cgroup_id = confinement.cgroup_id();
    let policy = confinement.policy();
    let isolation = confinement.profile().describe();
    let isolated = confinement.profile().isolates();

    // The channel, before the module exists.
    //
    // Both ends are made here so that the module's end can be handed across
    // `exec` and Thalyx's end can be served from this process. A module never
    // opens a channel; it is born holding one.
    let (thalyx_end, module_end) = std::os::unix::net::UnixStream::pair()
        .map_err(|source| CoreError::io(&request.helper, source))?;

    let mut child = {
        use std::os::fd::AsFd;
        confinement.spawn(
            &request.helper,
            &module_dir,
            &program,
            uid,
            &request.args,
            Some(module_end.as_fd()),
        )?
    };

    // Thalyx keeps no copy of the module's end. Without this the server below
    // would never see the connection close, because one writer would still be
    // open — in this process — and it would wait for a module that had already
    // exited.
    drop(module_end);

    // Before the wait, not after. The module holds the writing end of two
    // pipes; if nobody is emptying them it stops on a full buffer and Thalyx
    // waits for a module that is waiting for Thalyx.
    let draining_out = drain(child.stdout.take());
    let draining_err = drain(child.stderr.take());

    // Serve while it runs, in a thread, because both have to be happening at
    // once: a module that asks something before Thalyx starts listening would
    // block, and a Thalyx that waited for the child before listening would
    // deadlock against it.
    let mut api = crate::api::ModuleApi::for_module(&manifest, &permissions);
    let serving = std::thread::spawn(move || {
        let mut stream = thalyx_end;
        let outcome = thalyx_abi::serve(&mut stream, &mut api);
        (api, outcome)
    });

    let status = child
        .wait()
        .map_err(|source| CoreError::io(&request.helper, source))?;

    // The module is gone, so its end of the socket is closed and the server
    // has returned or is about to.
    let (api, served) = serving
        .join()
        .map_err(|_| CoreError::io(&program, std::io::Error::other("the API thread panicked")))?;

    // A channel that broke is reported, not swallowed. A module whose requests
    // stopped being answered halfway looks, from its own exit code, exactly
    // like one that finished — and the difference is whether the work happened.
    let channel_error = served.err().map(|error| error.to_string());
    let said = api.said().to_vec();
    let dropped_notices = api.dropped_notices();
    let wrote = collect(draining_out, draining_err);

    // Teardown happens whatever the module did. `release` is a no-op while
    // another instance is still inside, so a second run is not stripped of its
    // permissions when the first one ends.
    confinement.release()?;

    Ok(RunOutcome {
        module_id: manifest.id.clone(),
        version: manifest.version.clone(),
        program,
        cgroup_id: Some(cgroup_id),
        policy: Some(policy),
        isolation: Some(isolation),
        isolated,
        uid,
        permissions,
        exit_code: status.code(),
        said,
        wrote,
        dropped_notices,
        channel_error,
    })
}

/// The deliberate, named exception: run with nothing enforcing.
///
/// **The channel is still there**, and that is not an oversight. Unconfined
/// means no cgroup and no kernel policy; it does not mean no system. The API's
/// own checks live in [`crate::api`] and do not lean on the sandbox for
/// anything — they cannot, because the code answering a module runs outside it
/// either way. Withholding the channel here would have made the degraded mode
/// differ from the real one in a second, unrelated way, and the whole reason
/// this mode is named in the journal is so that what it degrades is exactly
/// one thing.
fn run_unconfined(
    manifest: &thalyx_manifest::Manifest,
    program: &std::path::Path,
    request: &RunRequest<'_>,
    permissions: Vec<thalyx_manifest::Permission>,
) -> Result<RunOutcome> {
    let (thalyx_end, module_end) =
        std::os::unix::net::UnixStream::pair().map_err(|source| CoreError::io(program, source))?;

    // No re-execution on this path, so there is no later stage to renumber the
    // descriptor. It has to happen between `fork` and `exec`, which is what
    // `spawn_with_channel` is for and why it lives in the crate that is allowed
    // `unsafe`.
    let mut child = {
        use std::os::fd::AsRawFd;
        let mut command = std::process::Command::new(program);
        command.args(&request.args);

        // The terminal is withheld here too, and that is not a second thing
        // being degraded — it is the same thing not being degraded.
        // `--unconfined` means no cgroup and no kernel policy. It has never
        // meant "may forge the trusted path", and a module that could draw
        // Thalyx's confirmation frame on the human's screen would be doing
        // exactly that. See `thalyx_sandbox::launch::spawn`, which explains
        // why these are pipes Thalyx drains rather than the null device.
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        thalyx_syscall::spawn_with_channel(&mut command, module_end.as_raw_fd())
            .map_err(|source| CoreError::io(program, source))?
    };
    drop(module_end);

    let draining_out = drain(child.stdout.take());
    let draining_err = drain(child.stderr.take());

    let mut api = crate::api::ModuleApi::for_module(manifest, &permissions);
    let serving = std::thread::spawn(move || {
        let mut stream = thalyx_end;
        let outcome = thalyx_abi::serve(&mut stream, &mut api);
        (api, outcome)
    });

    let status = child
        .wait()
        .map_err(|source| CoreError::io(program, source))?;

    let (api, served) = serving
        .join()
        .map_err(|_| CoreError::io(program, std::io::Error::other("the API thread panicked")))?;

    Ok(RunOutcome {
        module_id: manifest.id.clone(),
        version: manifest.version.clone(),
        program: program.to_path_buf(),
        cgroup_id: None,
        policy: None,
        isolation: None,
        isolated: false,
        uid: None,
        permissions,
        exit_code: status.code(),
        said: api.said().to_vec(),
        wrote: collect(draining_out, draining_err),
        dropped_notices: api.dropped_notices(),
        channel_error: served.err().map(|error| error.to_string()),
    })
}
