//! Running a program nobody signed — `vault/02-Arquitectura/Programas-Ajenos.md`.
//!
//! This is **G1** of `Superficie-para-el-LLM.md`, the point that has blocked the
//! project's own bar since it was measured on 2026-08-23: a foreign agent
//! running here, better than on Linux. The measurement found that the seccomp
//! filter already covers 41 of the 41 syscalls Claude Code makes to start, and
//! that what was missing was not a syscall or a path — it was that `correr`
//! only ever launches modules that are **installed and signed**, and a foreign
//! program is neither.
//!
//! ## Why this is not a hole in the signature decree
//!
//! Because it is not the same verb, and a guest never becomes a module.
//!
//! A module's signature means *somebody answered for this*. Signing whatever it
//! is handed would turn that into *this passed through here*, which leaves the
//! word meaningless for whoever reads the next one. So a foreign program gets
//! no signature, no install, no store entry, no persistent grant — and, above
//! all, **no channel**:
//!
//! A module is born holding a socket to Thalyx's API. This hands out none. The
//! API is the surface Thalyx gives something that was signed, installed and
//! granted permissions by name; a guest runs, it is not given the house. That
//! single omission is what keeps this verb from being a back door into
//! everything the module system checks.
//!
//! ## And why there is no unconfined mode here
//!
//! `run::run` has one, and it earns it: a bad mode reached on purpose and named
//! in the journal beats one reached by accident and named nowhere. The
//! justification is that a human read that module's manifest and its publisher
//! answered for it. Nobody answered for a foreign program, so the justification
//! is absent and so is the mode. If nothing can enforce, this refuses.

use crate::store::Store;
use crate::{CoreError, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use thalyx_journal::{Entry, Journal, Origin, Outcome};
use thalyx_manifest::Permission;
use thalyx_permd::PolicyStore;
use thalyx_sandbox::Confinement;

/// The prefix under which a foreign program's user id is remembered.
///
/// A module is keyed by its id; a foreign program has none, so the key is the
/// canonical path of the binary. The prefix is what makes the two spaces
/// impossible to confuse — without it a module called `/usr/bin/node` would be
/// handed the same user as the binary at that path, and the whole point of
/// one-uid-per-thing is that two things are never the same one.
pub const FOREIGN_UID_PREFIX: &str = "foreign:";

/// One request to run a program that is not a module.
pub struct ForeignRequest<'a> {
    /// The program, as the human named it. Canonicalised here rather than by
    /// the caller: everything downstream is keyed on the resolved path, and a
    /// symlink resolved in one place and not another is two identities for one
    /// file.
    pub program: &'a Path,
    pub args: Vec<OsString>,
    /// What the human confirmed this run may reach, and nothing else.
    pub grants: Vec<Permission>,
    /// The binary that re-executes itself into the cgroup. In practice
    /// `thalyx` itself; see `thalyx_sandbox::launch`.
    pub helper: PathBuf,
    pub request_id: String,
    pub profile: &'a str,
    /// Environment variables the program is started with.
    ///
    /// **Not a widening.** A variable is a string, the root filesystem holds
    /// only what was granted, and the LSM refuses every open the policy does
    /// not name — so a path in here that nobody granted names something that
    /// is not there. What it buys is the case Thalyx cannot avoid: a toolchain
    /// installed by one user and run by another has to be *told* where its
    /// registry is, and a `cargo` that cannot find one reports that as the
    /// change not compiling.
    pub environment: Vec<(String, String)>,
}

/// What happened, in the shape both faces read.
#[derive(Debug)]
pub struct ForeignOutcome {
    /// The resolved program, which is what actually ran.
    pub program: PathBuf,
    /// The name its cgroup was given, so a human can find it in the tree while
    /// it is still running.
    pub name: String,
    pub cgroup_id: u64,
    pub policy: thalyx_permd::Policy,
    pub isolation: String,
    pub isolated: bool,
    pub uid: Option<u32>,
    pub grants: Vec<Permission>,
    pub exit_code: Option<i32>,
    pub wrote: crate::run::ModuleOutput,
}

impl ForeignOutcome {
    /// Whether the program exited on its own rather than being killed.
    ///
    /// Said as its own question because `exit_code: None` and `Some(0)` are
    /// often read as "no news" and "fine" respectively, and one of those two
    /// readings is wrong: `None` means a signal, and under this profile the
    /// signal a program is most likely to have met is `SIGSYS`.
    pub fn exited(&self) -> bool {
        self.exit_code.is_some()
    }
}

/// Start a foreign program, wait for it, and tear its confinement down.
///
/// The journal entry is written whichever way it ends, and it is written by
/// this function rather than by the caller for the same reason `run::run` does
/// it: a run that is recorded only when somebody remembers to record it is a
/// run that is not recorded.
pub fn run_foreign(
    store: &Store,
    policies: &dyn PolicyStore,
    request: ForeignRequest<'_>,
) -> Result<ForeignOutcome> {
    let journal = Journal::open(store.journal_path())?;

    match run_inner(store, policies, &request) {
        Ok(outcome) => {
            let mut notes = vec![
                match outcome.exit_code {
                    Some(0) => "program exited cleanly".to_string(),
                    Some(code) => format!("program exited with status {code}"),
                    None => "program was terminated by a signal".to_string(),
                },
                format!(
                    "confined to cgroup {} with allowed=0x{:x}",
                    outcome.cgroup_id, outcome.policy.allowed
                ),
                outcome.isolation.clone(),
            ];
            if let Some(uid) = outcome.uid {
                notes.push(format!("ran as user {uid}"));
            }
            // The grants belong in the record and not only in the confirmation
            // the human saw. A confirmation is a thing that happened once, on a
            // screen; this is the only place that can still answer, months
            // later, what this program was allowed to reach.
            for grant in &outcome.grants {
                notes.push(format!("granted {} for {}", grant.resource, grant.action));
            }
            if outcome.grants.is_empty() {
                notes.push("granted nothing beyond the system paths".to_string());
            }

            journal.append(&Entry {
                timestamp: thalyx_journal::now(),
                // Never `run_module`, and this is the whole of
                // `Marcado-de-Origen`'s ask at this layer: what a program
                // nobody signed did has to be separable from what Thalyx did,
                // by reading the record rather than by remembering.
                operation: "run_foreign".to_string(),
                module_id: Some(outcome.program.display().to_string()),
                version: None,
                // What succeeded here is Thalyx's part — launching it under the
                // policy the human confirmed, isolated as the profile promised.
                // The program's own exit code is the program's business.
                outcome: if outcome.isolated {
                    Outcome::Success
                } else {
                    Outcome::Degraded {
                        reason: outcome.isolation.clone(),
                    }
                },
                request_id: request.request_id.clone(),
                origin: Origin::UserUtterance,
                snapshot: None,
                notes,
            })?;
            Ok(outcome)
        }
        Err(error) => {
            let _ = journal.append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: "run_foreign".to_string(),
                module_id: Some(request.program.display().to_string()),
                version: None,
                outcome: Outcome::Rejected {
                    reason: error.to_string(),
                },
                request_id: request.request_id.clone(),
                origin: Origin::UserUtterance,
                snapshot: None,
                notes: vec![],
            });
            Err(error)
        }
    }
}

/// A confined program that is still running, and everything needed to end it.
///
/// **Holding one owns the teardown.** [`ForeignProcess::shutdown`] kills the
/// process — which, under a profile with a pid namespace, kills every
/// descendant with it, because the kernel reaps a namespace when its init
/// dies — and then withdraws the policy and removes the cgroup. Dropping one
/// without that kills the process and leaves the cgroup and its kernel policy
/// behind, which the [`Drop`] below does as the half that can be done without
/// a policy store.
///
/// It exists for the semantic provider: rust-analyzer costs about 25 seconds to
/// start and 20 milliseconds to answer, so it is started once and asked many
/// times — its confinement outlives the call that established it, and a
/// [`thalyx_sandbox::Confinement`] borrows the store and cannot be kept
/// anywhere but in that frame. That is exactly what `Held` is for, and this is
/// the second caller of it after the resident engine.
pub struct ForeignProcess {
    /// The process Thalyx spawned, until somebody takes it.
    ///
    /// An `Option` because the one caller for this needs to *own* the child —
    /// `Analyzer` holds a conversation over its pipes — while the confinement
    /// stays here to be torn down. The first shape of this handed the child out
    /// and left a placeholder process behind, which meant `thalyx` depended on
    /// there being a `/bin/true` on the machine. The image holds the Linux
    /// kernel and one program. There is no `/bin/true`.
    child: Option<std::process::Child>,
    held: Option<thalyx_sandbox::Held>,
    pub program: PathBuf,
    pub name: String,
    pub cgroup_id: u64,
    pub policy: thalyx_permd::Policy,
    pub isolation: String,
    pub isolated: bool,
    pub uid: Option<u32>,
}

impl ForeignProcess {
    /// Take the process, leaving the confinement here to be torn down.
    ///
    /// For a caller that talks to what it started: it needs the pipes, and the
    /// cgroup and the policy are not its to hold. Whoever takes the child still
    /// has to call [`ForeignProcess::shutdown`] — and killing the child alone
    /// is not enough, because the cgroup and its kernel policy would be left.
    pub fn take_child(&mut self) -> Option<std::process::Child> {
        self.child.take()
    }

    /// Kill it and everything it started, then take the confinement down.
    pub fn shutdown(mut self, policies: &dyn PolicyStore) {
        self.end(policies);
    }

    fn end(&mut self, policies: &dyn PolicyStore) {
        // The cgroup first, and it is not belt and braces. A pid namespace's
        // init dying takes the namespace with it — but the window between
        // `spawn` and the re-exec that *becomes* that init is a window where
        // the tree is ordinary processes, and a `cargo` started in it would
        // outlive the kill. `cgroup.kill` covers every process in the cgroup
        // whatever stage it is at, and it covers the case where the child was
        // taken by somebody else and is not here to be killed.
        if let Some(held) = &self.held {
            let _ = held.cgroup().kill();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        // `release` is a no-op while anything is still inside, and `SIGKILL` is
        // delivered rather than completed: a compiler tree of forty processes
        // does not vanish on the instruction after the write. Without this wait
        // the release finds the cgroup occupied, declines, and leaves the
        // directory and its map entry behind — which is not a leak of memory,
        // it is a kernel policy outliving what it was written for.
        //
        // Bounded, and it gives up rather than blocking: a cgroup that will not
        // empty is a fact to report, not a reason for Thalyx to stop.
        if let Some(held) = &self.held {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < deadline {
                if held.cgroup().is_empty().unwrap_or(true) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }

        if let Some(held) = self.held.take() {
            let _ = held.release(policies);
        }
    }
}

impl Drop for ForeignProcess {
    /// Half a teardown, which is all that can be done from here.
    ///
    /// Withdrawing a policy needs a policy store and this holds no borrow of
    /// one — that is the whole reason `Held` exists. So the process is killed,
    /// and the cgroup and its map entry are left for the next start of the same
    /// program to reuse. A handle dropped without
    /// [`ForeignProcess::shutdown`] is a bug; this keeps it from being a live
    /// compiler nobody is holding.
    fn drop(&mut self) {
        if let Some(held) = &self.held {
            let _ = held.cgroup().kill();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Start a foreign program under confinement and hand it back still running.
///
/// The same establishment `run_foreign` does — the same enforcement gate, the
/// same user, the same cgroup, the same root filesystem, the same filter — up
/// to the moment the process exists. `stdin` is a pipe whose only writer is
/// Thalyx, because the one caller for this talks to what it starts.
pub fn start_foreign(
    store: &Store,
    policies: &dyn PolicyStore,
    request: &ForeignRequest<'_>,
) -> Result<ForeignProcess> {
    let (program, home, name, profile, uid) = establish(store, policies, request)?;

    let parent = thalyx_sandbox::cgroup::parent()?;
    let confinement = Confinement::establish(
        policies,
        &parent,
        &name,
        profile,
        &request.grants,
        thalyx_permd::boot_ns(),
        thalyx_permd::DEFAULT_JIT_LIFETIME_NS,
    )?;

    let cgroup_id = confinement.cgroup_id();
    let policy = confinement.policy();
    let isolation = confinement.profile().describe();
    let isolated = confinement.profile().isolates();

    let child = confinement.spawn_talking(
        &request.helper,
        &home,
        &program,
        uid,
        &request.args,
        &request.environment,
    )?;

    let journal = Journal::open(store.journal_path())?;
    let _ = journal.append(&Entry {
        timestamp: thalyx_journal::now(),
        operation: "start_foreign".to_string(),
        module_id: Some(program.display().to_string()),
        version: None,
        outcome: if isolated {
            Outcome::Success
        } else {
            Outcome::Degraded {
                reason: isolation.clone(),
            }
        },
        request_id: request.request_id.clone(),
        origin: Origin::UserUtterance,
        snapshot: None,
        notes: vec![format!("started under {isolation}, held open")],
    });

    Ok(ForeignProcess {
        child: Some(child),
        held: Some(confinement.detach()),
        program,
        name,
        cgroup_id,
        policy,
        isolation,
        isolated,
        uid,
    })
}

/// Everything both starts do before a cgroup exists.
///
/// Lifted out so there is one enforcement gate and one uid assignment rather
/// than two, which is the same argument `run::start` makes for the resident
/// module: a second launcher is a second place for the checks to drift.
type Established = (
    PathBuf,
    PathBuf,
    String,
    thalyx_sandbox::Profile,
    Option<u32>,
);

fn establish(
    store: &Store,
    policies: &dyn PolicyStore,
    request: &ForeignRequest<'_>,
) -> Result<Established> {
    let program = resolve_program(request.program)?;

    // The directory the binary lives in becomes its `/module` inside the pivot.
    //
    // Not a choice of convenience: `RootFs` refuses a program that is not under
    // the tree it mounts, which is the rule that stops a caller naming a root
    // that has nothing to do with what it is about to execute. A foreign
    // program has no tree of its own, so its tree is where it is.
    let home = program
        .parent()
        .ok_or_else(|| CoreError::NotExecutable {
            path: program.clone(),
            reason: "it has no directory to run in".to_string(),
        })?
        .to_path_buf();

    let name = cgroup_name(&program);

    // Resolved before the kernel is asked anything, for the reason `run.rs`
    // records: a profile name nothing matches is wrong on every machine, and
    // checking it after the enforcement gate means the only machine that ever
    // notices is the one that can enforce.
    let profile = thalyx_sandbox::profile::resolve(request.profile)?;

    if !policies.is_available() {
        return Err(CoreError::NothingCanEnforceForeign {
            program: program.clone(),
            grants: request.grants.len(),
        });
    }

    // And then whether what is loaded is denying. `make -C lsm load` lands in
    // observe mode deliberately, so "the map opens" and "a denial is real" are
    // two questions, and until 2026-08-25 only the first one was ever asked.
    //
    // A module may run under an observing kernel — degraded, and the journal
    // says so, because somebody signed it and a human read its manifest. A
    // guest may not: the whole of what stands behind it is the confinement,
    // and `vault/02-Arquitectura/Programas-Ajenos.md` decrees no degraded mode
    // for a program nobody signed.
    match policies.enforcement() {
        thalyx_permd::Enforcement::Enforcing => {}
        thalyx_permd::Enforcement::Observing => {
            return Err(CoreError::ObservingNotEnforcing {
                program: program.clone(),
            });
        }
        thalyx_permd::Enforcement::Unreadable(reason) => {
            return Err(CoreError::EnforcementModeUnreadable {
                program: program.clone(),
                reason,
            });
        }
    }

    // Its own user, keyed on the path rather than on an id it does not have.
    //
    // Assigned under the global lock and only for as long as that takes — two
    // first runs racing here could otherwise be handed the same number, which
    // is the sharing `uids.rs` exists to prevent — and never held while the
    // program runs.
    let uid = if profile.own_user {
        let key = format!("{FOREIGN_UID_PREFIX}{}", program.display());
        let _lock = store.lock()?;
        let mut uids = crate::uids::UidRegistry::load(store.uids_path())?;
        Some(uids.assign(&key)?)
    } else {
        None
    };

    Ok((program, home, name, profile, uid))
}

fn run_inner(
    store: &Store,
    policies: &dyn PolicyStore,
    request: &ForeignRequest<'_>,
) -> Result<ForeignOutcome> {
    // The same establishment `start_foreign` does, through the same function.
    // Two copies of the enforcement gate and the uid assignment would be two
    // places for them to drift, and the one that drifts is always the one
    // nobody runs on the machine that can enforce.
    let (program, home, name, profile, uid) = establish(store, policies, request)?;

    let parent = thalyx_sandbox::cgroup::parent()?;
    let confinement = Confinement::establish(
        policies,
        &parent,
        &name,
        profile,
        &request.grants,
        thalyx_permd::boot_ns(),
        thalyx_permd::DEFAULT_JIT_LIFETIME_NS,
    )?;

    let cgroup_id = confinement.cgroup_id();
    let policy = confinement.policy();
    let isolation = confinement.profile().describe();
    let isolated = confinement.profile().isolates();

    // No channel, and that is the decree rather than an omission. See this
    // module's header: a guest is not handed the API.
    let mut child = confinement.spawn_with(
        &request.helper,
        &home,
        &program,
        uid,
        &request.args,
        &request.environment,
    )?;

    // Before the wait, never after. The program holds the writing end of two
    // pipes, and nobody emptying them means it stops on a full buffer while
    // Thalyx waits for a program that is waiting for Thalyx.
    let draining_out = crate::run::drain(child.stdout.take());
    let draining_err = crate::run::drain(child.stderr.take());

    let status = child
        .wait()
        .map_err(|source| CoreError::io(&program, source))?;

    let wrote = crate::run::collect(draining_out, draining_err);

    // Teardown happens whatever the program did, and is a no-op while another
    // instance is still inside.
    confinement.release()?;

    Ok(ForeignOutcome {
        program,
        name,
        cgroup_id,
        policy,
        isolation,
        isolated,
        uid,
        grants: request.grants.clone(),
        exit_code: status.code(),
        wrote,
    })
}

/// The path this will actually execute, or why it will not.
///
/// Three questions, each answered separately, because "that did not run" covers
/// four different mistakes and the human made exactly one of them.
fn resolve_program(named: &Path) -> Result<PathBuf> {
    let program = named
        .canonicalize()
        .map_err(|source| CoreError::NotExecutable {
            path: named.to_path_buf(),
            reason: format!("it cannot be resolved: {source}"),
        })?;

    let metadata = std::fs::metadata(&program).map_err(|source| CoreError::NotExecutable {
        path: program.clone(),
        reason: format!("it cannot be read: {source}"),
    })?;

    if metadata.is_dir() {
        return Err(CoreError::NotExecutable {
            path: program,
            reason: "it is a directory".to_string(),
        });
    }

    if !metadata.is_file() {
        return Err(CoreError::NotExecutable {
            path: program,
            reason: "it is not a regular file".to_string(),
        });
    }

    // Asked here as well as by the kernel at `exec`. The kernel's answer
    // arrives after the cgroup, the policy and the user have all been created,
    // and unwinding those to report `EACCES` tells the human less than this
    // does, later.
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(CoreError::NotExecutable {
            path: program,
            reason: "nothing about it is marked executable".to_string(),
        });
    }

    Ok(program)
}

/// A cgroup directory name for a path, which cannot contain a path.
///
/// A module's cgroup is named after the module, which is already a single
/// component. A foreign program's identity is its whole path, and a directory
/// name may not carry one — so the basename says which program it is at a
/// glance, and the digest says *which* of the ones with that name.
///
/// Hand-rolled FNV-1a rather than `DefaultHasher`, whose output the standard
/// library explicitly does not promise to keep between releases. This name ends
/// up in a directory on the machine and in a journal a human reads months
/// later; a name that silently changes with a toolchain upgrade would split one
/// program's history in two with nothing saying so.
fn cgroup_name(program: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in program.as_os_str().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    let base = program
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Kept to what a directory name allows and what a person can read back.
    // Everything that is not a letter, a digit, a dash or a dot becomes a dash,
    // so a program called `my program` cannot produce a name with a space in
    // it that half the tooling then quotes differently.
    let safe: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect();

    format!("foreign.{safe}.{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_programs_with_the_same_basename_do_not_share_a_cgroup() {
        // The mistake this prevents: naming the cgroup after the binary alone.
        // Every machine has several `node`, and two of them in one cgroup share
        // a policy — so the one granted nothing would run under whatever the
        // other one was confirmed for.
        let one = cgroup_name(Path::new("/usr/bin/node"));
        let two = cgroup_name(Path::new("/home/someone/.local/bin/node"));

        assert_ne!(one, two);
        assert!(one.starts_with("foreign.node."), "{one}");
        assert!(two.starts_with("foreign.node."), "{two}");
    }

    #[test]
    fn the_same_path_is_the_same_name_every_time() {
        // What the journal leans on. A name that changed between runs would
        // split one program's history with nothing saying it had.
        assert_eq!(
            cgroup_name(Path::new("/usr/bin/node")),
            cgroup_name(Path::new("/usr/bin/node"))
        );
    }

    #[test]
    fn a_name_that_could_not_be_a_directory_becomes_one_that_can() {
        let name = cgroup_name(Path::new("/tmp/my program/../weird name!"));
        assert!(!name.contains(' '), "{name}");
        assert!(!name.contains('/'), "{name}");
        assert!(name.starts_with("foreign.weird-name-."), "{name}");
    }

    #[test]
    fn a_directory_is_not_a_program_and_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let error = resolve_program(directory.path()).unwrap_err();
        assert!(error.to_string().contains("directory"), "{error}");
    }

    #[test]
    fn a_file_nobody_marked_executable_is_refused_before_anything_is_created() {
        // The order is the claim. The kernel would refuse this too, at `exec`,
        // after a cgroup, a policy and a user had been made for it — and the
        // human would be told `EACCES` about a path they had already been told
        // was fine.
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("not-a-program");
        std::fs::write(&file, "text\n").unwrap();

        let error = resolve_program(&file).unwrap_err();
        assert!(error.to_string().contains("executable"), "{error}");
    }

    #[test]
    fn a_path_that_is_not_there_says_that_rather_than_that_it_failed() {
        // Rule 10: a failure to read is not a failure to exist, and the two
        // remedies are different — one is a typo and the other is a permission.
        let error = resolve_program(Path::new("/nonexistent/program")).unwrap_err();
        assert!(error.to_string().contains("cannot be resolved"), "{error}");
    }
}
