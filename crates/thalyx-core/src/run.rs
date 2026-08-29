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
    /// How the module's own descriptors are wired. See [`Wiring`].
    pub wiring: Wiring,
}

/// What Thalyx does with the module's `stdin` and `stdout`.
///
/// [`Wiring::Collected`] is what `correr` has always had and what every module
/// but one gets: descriptor 0 closed, and both output pipes drained into
/// [`RunOutcome::wrote`].
///
/// [`Wiring::Talks`] is for a **resident** module — one that is started once
/// and asked many things. Descriptor 0 becomes a pipe whose only writer is
/// Thalyx, and descriptor 1 is handed to the caller instead of being drained,
/// because for such a module `stdout` is the answers rather than a transcript.
/// `stderr` is still drained: a module that dies with a message has to be able
/// to say why, and that is the descriptor it says it on.
///
/// It is a field of the request rather than a second launcher because that is
/// the whole point — the cgroup, the policy, the seccomp filter, the pivoted
/// root, the uid, the channel and the journal entry are the ones every module
/// gets, established by this file and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wiring {
    #[default]
    Collected,
    Talks,
}

/// What a run would be, answered before it is one.
///
/// Every field here is read by the same code that would run it, in the same
/// order — see [`foresee_run`]. The two things it deliberately does not
/// contain are the two a rehearsal cannot know: the exit code and what the
/// module would say. Nothing is invented for them.
#[derive(Debug)]
pub struct RunForeseen {
    pub module_id: String,
    pub version: String,
    pub program: PathBuf,
    /// How the resolved profile would isolate it, in the profile's own words.
    pub isolation: String,
    /// Whether that profile isolates anything at all.
    pub isolates: bool,
    /// Whether it would be given a user of its own.
    pub own_user: bool,
    /// What it holds **in force**, which is not what its manifest asks for.
    pub permissions: Vec<thalyx_manifest::Permission>,
    /// What the kernel is doing. `None` when there is no policy map to ask.
    pub enforcement: Option<thalyx_permd::Enforcement>,
    /// Whether the run would be recorded as degraded — asked for unconfined,
    /// or confined under a kernel that is not denying.
    pub degraded: bool,
    pub unconfined: bool,
    /// Whether it would start at all.
    pub would_run: bool,
    /// Why not, in the words the real verb would use. `None` when it would run.
    pub refusal: Option<String>,
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
    /// Whether the kernel side was denying or only watching while it ran.
    /// `None` when the module ran unconfined, where the question does not
    /// arise.
    ///
    /// Carried rather than assumed, because `make -C lsm load` lands in
    /// observe mode and a run under an observing kernel is a run nobody could
    /// tell apart from a confined one — which is the failure the journal block
    /// below has a comment about.
    pub enforcement: Option<thalyx_permd::Enforcement>,
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

    match start(store, policies, &request).and_then(|running| running.wait(policies)) {
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
            if let Some(enforcement) = &outcome.enforcement {
                notes.push(format!("kernel enforcement: {}", enforcement.describe()));
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
                outcome: match (
                    outcome.confined(),
                    outcome.enforcement.clone(),
                    outcome.isolated,
                ) {
                    (true, Some(thalyx_permd::Enforcement::Enforcing), true) => Outcome::Success,
                    // Ahead of the isolation clause below: a kernel that
                    // denies nothing is the larger of the two gaps, and a
                    // journal that named only the smaller one would be
                    // describing the wrong run.
                    (true, Some(thalyx_permd::Enforcement::Observing), _) => Outcome::Degraded {
                        reason: "the kernel side was attached but only observing: the policy was \
                                 written and no denial would have been applied"
                            .to_string(),
                    },
                    (true, Some(thalyx_permd::Enforcement::Unreadable(reason)), _) => {
                        Outcome::Degraded {
                            reason: format!(
                                "whether the kernel was enforcing could not be read — {reason}"
                            ),
                        }
                    }
                    (true, _, false) => Outcome::Degraded {
                        reason: "ran under a profile that isolates nothing".to_string(),
                    },
                    (true, None, true) => Outcome::Degraded {
                        reason: "confined, but the kernel's mode was never read".to_string(),
                    },
                    (false, _, _) => Outcome::Degraded {
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

/// Everything a run works out before anything of it has happened.
///
/// Split out of [`run_inner`] on 2026-08-26 so that `ensayo correr` could exist
/// without being a second implementation of it. D1 of
/// `vault/02-Arquitectura/Superficie-para-el-LLM.md` had been eight of nine for
/// that reason: a rehearsal built beside the verb answers a question about
/// itself, and this project has already paid twice for two copies of one
/// judgement drifting apart.
struct Resolved {
    manifest: thalyx_manifest::Manifest,
    module_dir: std::path::PathBuf,
    program: std::path::PathBuf,
    permissions: Vec<thalyx_manifest::Permission>,
}

fn resolve(store: &Store, request: &RunRequest<'_>) -> Result<Resolved> {
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

    Ok(Resolved {
        manifest,
        module_dir,
        program,
        permissions,
    })
}

/// What `correr <id>` would do, worked out by the code that would do it.
///
/// D1 says every verb that changes the machine can be rehearsed, and `correr`
/// was the one that could not. The reason written beside it was real at the
/// time — *"what a run would be allowed to do is a question for the kernel
/// side, and answering it from the manifest would describe a run that the
/// machine may not be able to give"* — and it stopped being real on
/// 2026-08-25, when Thalyx learned to read the mode. This asks the kernel the
/// same two questions the run asks, in the same order, and stops one line
/// before the program exists.
pub fn foresee_run(
    store: &Store,
    policies: &dyn PolicyStore,
    request: &RunRequest<'_>,
) -> Result<RunForeseen> {
    let resolved = resolve(store, request)?;

    // In the order the run asks them, because the order is a decision this
    // file's comments defend twice: a profile nothing resolves is wrong on a
    // machine with no BPF at all, so it is reported before the kernel is asked
    // anything.
    let profile = thalyx_sandbox::profile::resolve(request.profile)?;

    let available = policies.is_available();
    let enforcement = available.then(|| policies.enforcement());
    // Worked out before the value moves into the answer, and worth naming: a
    // module may run under a kernel that denies nothing — somebody signed it —
    // and the run is recorded as degraded. So is this, before it happens.
    let degraded = request.unconfined
        || (available && !matches!(enforcement, Some(thalyx_permd::Enforcement::Enforcing)));

    // Exactly the gate `run_inner` applies, and nothing else: a rehearsal that
    // invented a reason to refuse would be worse than none, because the person
    // reading it would go and fix something that was never in the way.
    let refusal = (!available && !request.unconfined).then(|| {
        CoreError::NothingCanEnforce {
            module_id: resolved.manifest.id.clone(),
            permissions: resolved.permissions.len(),
        }
        .to_string()
    });

    Ok(RunForeseen {
        module_id: resolved.manifest.id,
        version: resolved.manifest.version.to_string(),
        program: resolved.program,
        isolation: profile.describe(),
        isolates: profile.isolates(),
        own_user: profile.own_user,
        permissions: resolved.permissions,
        enforcement,
        degraded,
        unconfined: request.unconfined,
        would_run: refusal.is_none(),
        refusal,
    })
}

/// A module that is running, with everything holding it up still owned.
///
/// The split this type exists for: `run` used to be one function that
/// established a confinement, spawned, waited, and tore the confinement down —
/// which is exactly right for `correr` and cannot express the engine, whose
/// whole point since 2026-08-28 is that the process outlives the answer. Rather
/// than write a second launcher for it — a second place to get the cgroup, the
/// policy, the seccomp filter, the pivoted root and the uid right, and a second
/// place for them to drift — the middle of the one launcher was given a name.
///
/// [`run`] is now `start` followed by [`RunningModule::wait`], so the ordinary
/// path exercises the same code the resident path holds open.
///
/// **Holding one of these owns the teardown.** Dropping it without
/// [`RunningModule::wait`] or [`RunningModule::shutdown`] leaves a live process,
/// a cgroup and a policy in the kernel behind it.
pub struct RunningModule {
    /// `None` when the module ran unconfined, where there is nothing to release.
    held: Option<thalyx_sandbox::Held>,
    child: std::process::Child,
    serving: Option<std::thread::JoinHandle<(crate::api::ModuleApi, Option<String>)>>,
    draining_out: Option<std::thread::JoinHandle<(String, bool)>>,
    draining_err: Option<std::thread::JoinHandle<(String, bool)>>,
    /// Only under [`Wiring::Talks`]: the pipe requests are written to.
    stdin: Option<std::process::ChildStdin>,
    /// Only under [`Wiring::Talks`]: the pipe answers are read from. Under
    /// `Collected` this is `None` because a drain thread has it.
    stdout: Option<std::process::ChildStdout>,
    module_id: String,
    version: String,
    program: PathBuf,
    cgroup_id: Option<u64>,
    policy: Option<thalyx_permd::Policy>,
    isolation: Option<String>,
    isolated: bool,
    enforcement: Option<thalyx_permd::Enforcement>,
    permissions: Vec<thalyx_manifest::Permission>,
    uid: Option<u32>,
}

impl RunningModule {
    /// The pid Thalyx holds for this module, in Thalyx's own namespace.
    ///
    /// What makes "the same engine answered both sentences" a thing somebody
    /// can check rather than a thing this project asserts. It is the pid of the
    /// process Thalyx spawned — the outer stage of `thalyx_sandbox::launch`,
    /// which `exec`s its way down to the module — so it is the module's pid
    /// from out here even where the module has a pid namespace of its own and
    /// believes it is 1.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn cgroup_id(&self) -> Option<u64> {
        self.cgroup_id
    }

    /// The request pipe, taken once. See [`Wiring::Talks`].
    pub fn take_stdin(&mut self) -> Option<std::process::ChildStdin> {
        self.stdin.take()
    }

    /// The answer pipe, taken once. See [`Wiring::Talks`].
    pub fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.stdout.take()
    }

    /// Whether it is still there, without blocking on it.
    ///
    /// A resident module that died is not an error until the next request
    /// needs it, and this is how that is asked. `Err` from `try_wait` is
    /// treated as gone: a process Thalyx can no longer ask about is not one it
    /// can go on sending requests to. Rule 9 — the cautious answer.
    pub fn still_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Wait for it to end, collect what it said, and tear the confinement down.
    pub fn wait(mut self, policies: &dyn PolicyStore) -> Result<RunOutcome> {
        let status = self
            .child
            .wait()
            .map_err(|source| CoreError::io(&self.program, source))?;
        Ok(self.finish(policies, status.code()))
    }

    /// Whether [`RunningModule::finish`] has already run.
    fn ended(&self) -> bool {
        self.serving.is_none()
    }

    /// Kill it and tear the confinement down, without waiting for an answer.
    ///
    /// For a resident module Thalyx is done with, or one that stopped making
    /// sense. The pipes are dropped first: a module blocked writing into a pipe
    /// nobody empties would not notice a `SIGTERM` until it did, and this is
    /// the one path where Thalyx is holding a pipe rather than draining it.
    pub fn shutdown(mut self, policies: &dyn PolicyStore) -> RunOutcome {
        self.stdin = None;
        self.stdout = None;
        let _ = self.child.kill();
        let code = self.child.wait().ok().and_then(|status| status.code());
        self.finish(policies, code)
    }

    /// Everything both endings share: join the threads, release, report.
    ///
    /// Takes `&mut self` rather than `self` so that [`Drop`] can tell whether
    /// it has run. See the `Drop` impl: a handle that is dropped without
    /// either ending would otherwise leave a live process behind, and the one
    /// module this matters for is the one that stays alive on purpose.
    fn finish(&mut self, policies: &dyn PolicyStore, exit_code: Option<i32>) -> RunOutcome {
        // The module is gone, so its end of the socket is closed and the server
        // has returned or is about to.
        let (said, dropped_notices, channel_error) = match self.serving.take() {
            Some(handle) => match handle.join() {
                Ok((api, error)) => (api.said().to_vec(), api.dropped_notices(), error),
                Err(_) => (Vec::new(), 0, Some("the API thread panicked".to_string())),
            },
            None => (Vec::new(), 0, None),
        };

        let wrote = match (self.draining_out.take(), self.draining_err.take()) {
            (Some(out), Some(err)) => collect(out, err),
            // `Wiring::Talks` keeps `stdout` for the protocol, so only `stderr`
            // was drained. Reported as an empty `stdout` rather than as
            // truncation: nothing was cut, it was never Thalyx's to keep.
            (None, Some(err)) => {
                let (stderr, cut) = err.join().unwrap_or_else(|_| (String::new(), true));
                ModuleOutput {
                    stdout: String::new(),
                    stderr,
                    truncated: cut,
                }
            }
            _ => ModuleOutput::default(),
        };

        // Teardown happens whatever the module did. `release` is a no-op while
        // another instance is still inside, so a second run is not stripped of
        // its permissions when the first one ends.
        if let Some(held) = self.held.take() {
            let _ = held.release(policies);
        }

        RunOutcome {
            module_id: self.module_id.clone(),
            version: self.version.clone(),
            program: self.program.clone(),
            cgroup_id: self.cgroup_id,
            policy: self.policy,
            isolation: self.isolation.clone(),
            isolated: self.isolated,
            enforcement: self.enforcement.clone(),
            uid: self.uid,
            permissions: std::mem::take(&mut self.permissions),
            exit_code,
            said,
            wrote,
            dropped_notices,
            channel_error,
        }
    }
}

impl Drop for RunningModule {
    /// The last resort, and it is only half of one.
    ///
    /// A handle dropped without [`RunningModule::wait`] or
    /// [`RunningModule::shutdown`] is a bug, and it happened on 2026-08-28: an
    /// `if let (false, Some(stale)) = (usable, held.take())` in the engine took
    /// the live resident out of its slot on every call, whether or not the arm
    /// fired, and dropped it. The process stayed — nothing killed it — so the
    /// machine quietly ran one engine per sentence with the old ones still in
    /// memory, which is worse than the failure it was replacing.
    ///
    /// So a drop kills the process, which is the half that can be done from
    /// here. The cgroup and the kernel policy cannot be: withdrawing a policy
    /// needs the policy store, and this has no borrow of one — that is the
    /// whole reason [`thalyx_sandbox::Held`] exists. They are left, and the
    /// next run of the same module reuses the cgroup rather than failing.
    fn drop(&mut self) {
        if self.ended() {
            return;
        }
        self.stdin = None;
        self.stdout = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Serve the module's channel on a thread, for as long as it holds its end.
///
/// In a thread because both have to be happening at once: a module that asks
/// something before Thalyx starts listening would block, and a Thalyx that
/// waited for the child before listening would deadlock against it.
fn serve_channel(
    manifest: &thalyx_manifest::Manifest,
    permissions: &[thalyx_manifest::Permission],
    thalyx_end: std::os::unix::net::UnixStream,
) -> std::thread::JoinHandle<(crate::api::ModuleApi, Option<String>)> {
    let mut api = crate::api::ModuleApi::for_module(manifest, permissions);
    std::thread::spawn(move || {
        let mut stream = thalyx_end;
        // A channel that broke is reported, not swallowed. A module whose
        // requests stopped being answered halfway looks, from its own exit
        // code, exactly like one that finished — and the difference is whether
        // the work happened.
        let outcome = thalyx_abi::serve(&mut stream, &mut api)
            .err()
            .map(|error| error.to_string());
        (api, outcome)
    })
}

/// Start a module and hand back the handle that holds it up.
///
/// Everything [`run`] did up to the `wait`, and nothing after it. See
/// [`RunningModule`] for why the split exists.
pub fn start(
    store: &Store,
    policies: &dyn PolicyStore,
    request: &RunRequest<'_>,
) -> Result<RunningModule> {
    let Resolved {
        manifest,
        module_dir,
        program,
        permissions,
    } = resolve(store, request)?;

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

    // Read once, here, rather than after the run: the answer has to be about
    // the kernel the module ran under, and `make -C lsm enforce` in another
    // terminal halfway through would otherwise have the journal describe a run
    // that never happened.
    //
    // Unlike a guest, a module is allowed to run under an observing kernel —
    // somebody signed it and a human read its manifest — but the run is
    // degraded and says so. See `run_foreign`, which refuses instead.
    let enforcement = policies.enforcement();

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
    // Detached the moment it is established, because the handle this returns
    // may outlive the frame that made it — see [`RunningModule`]. Nothing about
    // the confinement changes; what is given up is the borrow of the policy
    // store, which is named again at release.
    let held = Confinement::establish(
        policies,
        &parent,
        &manifest.id,
        profile,
        &permissions,
        thalyx_permd::boot_ns(),
        thalyx_permd::DEFAULT_JIT_LIFETIME_NS,
    )?
    .detach();

    let cgroup_id = held.cgroup_id();
    let policy = held.policy();
    let isolation = held.profile().describe();
    let isolated = held.profile().isolates();

    // The channel, before the module exists.
    //
    // Both ends are made here so that the module's end can be handed across
    // `exec` and Thalyx's end can be served from this process. A module never
    // opens a channel; it is born holding one.
    let (thalyx_end, module_end) = std::os::unix::net::UnixStream::pair()
        .map_err(|source| CoreError::io(&request.helper, source))?;

    let mut child = {
        use std::os::fd::AsFd;
        held.spawn(
            &request.helper,
            thalyx_sandbox::Launch {
                module_dir: &module_dir,
                program: &program,
                uid,
                args: &request.args,
                channel: Some(module_end.as_fd()),
                stdin: match request.wiring {
                    Wiring::Collected => thalyx_sandbox::Stdin::Closed,
                    Wiring::Talks => thalyx_sandbox::Stdin::Piped,
                },
            },
        )?
    };

    // Thalyx keeps no copy of the module's end. Without this the server below
    // would never see the connection close, because one writer would still be
    // open — in this process — and it would wait for a module that had already
    // exited.
    drop(module_end);

    // Before the wait, not after. The module holds the writing end of two
    // pipes; if nobody is emptying them it stops on a full buffer and Thalyx
    // waits for a module that is waiting for Thalyx. Under `Wiring::Talks`
    // `stdout` is not drained because it is the caller who empties it, one
    // answer at a time.
    let (draining_out, stdout) = match request.wiring {
        Wiring::Collected => (Some(drain(child.stdout.take())), None),
        Wiring::Talks => (None, child.stdout.take()),
    };
    let draining_err = Some(drain(child.stderr.take()));

    Ok(RunningModule {
        held: Some(held),
        serving: Some(serve_channel(&manifest, &permissions, thalyx_end)),
        stdin: child.stdin.take(),
        stdout,
        child,
        draining_out,
        draining_err,
        module_id: manifest.id.clone(),
        version: manifest.version.clone(),
        program,
        cgroup_id: Some(cgroup_id),
        policy: Some(policy),
        isolation: Some(isolation),
        isolated,
        enforcement: Some(enforcement),
        permissions,
        uid,
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
) -> Result<RunningModule> {
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
        // why these are pipes Thalyx drains rather than the null device — and
        // why a request pipe from Thalyx is not the terminal either.
        command
            .stdin(match request.wiring {
                Wiring::Collected => std::process::Stdio::null(),
                Wiring::Talks => std::process::Stdio::piped(),
            })
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        thalyx_syscall::spawn_with_channel(&mut command, module_end.as_raw_fd())
            .map_err(|source| CoreError::io(program, source))?
    };
    drop(module_end);

    let (draining_out, stdout) = match request.wiring {
        Wiring::Collected => (Some(drain(child.stdout.take())), None),
        Wiring::Talks => (None, child.stdout.take()),
    };

    Ok(RunningModule {
        held: None,
        serving: Some(serve_channel(manifest, &permissions, thalyx_end)),
        stdin: child.stdin.take(),
        stdout,
        draining_out,
        draining_err: Some(drain(child.stderr.take())),
        child,
        module_id: manifest.id.clone(),
        version: manifest.version.clone(),
        program: program.to_path_buf(),
        cgroup_id: None,
        policy: None,
        isolation: None,
        isolated: false,
        enforcement: None,
        permissions,
        uid: None,
    })
}
