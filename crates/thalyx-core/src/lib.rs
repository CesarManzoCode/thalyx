//! `thalyx-core` — validation, verification, staging and atomic commit.
//!
//! This crate is the trusted computing base. Everything it is handed comes from
//! somewhere it does not trust, so every value is re-derived rather than
//! believed: digests are recomputed, signatures are checked against a pinned
//! key, and confirmation text is generated here rather than accepted from a
//! caller.
//!
//! See `vault/11-Seguridad/Modelo-de-Amenaza.md` and `vault/02-Arquitectura/Core.md`.

pub mod api;
pub mod attempt;
pub mod bundle;
pub mod commit;
pub mod fault;
pub mod foreign;
pub mod install;
pub mod keystore;
pub mod permissions;
pub mod reconcile;
pub mod repo;
pub mod restore;
pub mod rollback;
pub mod run;
pub mod session;
pub mod snapshots;
pub mod store;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod trusted_path;
pub mod uids;

pub use foreign::{ForeignOutcome, ForeignRequest, run_foreign};
pub use install::{InstallOutcome, InstallRequest, install, installed_manifest, remove};
pub use run::{RunOutcome, RunRequest, run};
pub use store::Store;

/// The permissions a module actually holds right now.
///
/// A grant is in force **only while the version it was confirmed for is the
/// one `current` points at**, and — for a `session` grant — only while the
/// session it was made in is still the current one. That is what makes the
/// `current` symlink the single atomic point deciding both "installed" and
/// "authorised": there is no second transition that could be interrupted
/// separately.
///
/// ## The upgrade this exists to get right
///
/// This used to ask only whether the module was installed at all, which meant
/// an interrupted **upgrade** — grants written for version 2, process killed
/// before the symlink swung — left version 1 running under version 2's
/// permissions. Everything downstream, the kernel policy included, read that
/// as authorised. Comparing the version is what closes it, and the record left
/// by an interrupted install is inert for the same reason it always was: no
/// version points at it. `thalyx store clean` reclaims it.
pub fn effective_permissions(
    store: &Store,
    registry: &permissions::Registry,
    module_id: &str,
) -> Vec<permissions::Grant> {
    let Some(version) = store.installed_version(module_id) else {
        return Vec::new();
    };
    let session = session::Session::current(store);

    registry
        .effective(module_id)
        .iter()
        .filter(|grant| grant.in_force(&version, &session))
        .cloned()
        .collect()
}

/// The runtime version that a manifest's `requires.thalyx` is matched against.
pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The trailing clause naming what was granted, or nothing at all when nothing
/// was.
///
/// Written out rather than left as `{n} thing(s)`, which at zero produced
/// «none of the 0 thing(s) this was granted would be enforced» — a sentence
/// that is ungrammatical and, worse, beside the point. With no map loaded what
/// is missing is every decision about what the program may touch, not the ones
/// the human happened to name. Cesar read that sentence on 2026-08-25 and it
/// was the first thing `ejecutar` ever said to him.
fn also_granted(count: usize, noun: &str) -> String {
    match count {
        0 => String::new(),
        1 => format!(" — the one {noun} included"),
        n => format!(" — the {n} {noun}s included"),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Manifest(#[from] thalyx_manifest::ManifestError),

    #[error(transparent)]
    Contract(#[from] thalyx_contract::ContractError),

    #[error(transparent)]
    Journal(#[from] thalyx_journal::JournalError),

    #[error(transparent)]
    Attempt(#[from] crate::attempt::AttemptError),

    #[error("bundle is malformed: {0}")]
    MalformedBundle(String),

    /// State that exists and cannot be understood.
    ///
    /// Deliberately not the same as state that is absent. Absent means nothing
    /// was ever recorded and the cautious answer is an empty set; unreadable
    /// means something *was* recorded and nobody knows what — and for the
    /// keystore those two answers are opposites, because an empty keystore
    /// trusts every publisher it is offered.
    #[error(
        "{path} exists but could not be read as Thalyx state: {reason}\n  \
         Refusing to continue: an unreadable record of what was authorised is \
         not the same as a record that nothing was, and treating it as empty \
         is what would re-trust a publisher key that was pinned."
    )]
    StateUnreadable {
        path: std::path::PathBuf,
        reason: String,
    },

    #[error("signature verification failed for `{module_id}`")]
    SignatureRejected { module_id: String },

    /// Key rotation for a known id is how publisher impersonation looks.
    /// It is a hard error, never a warning.
    #[error(
        "publisher key for `{module_id}` changed since it was first trusted.\n  \
         pinned: {pinned}\n  offered: {offered}\n\
         This is what impersonation looks like. Installation refused."
    )]
    PublisherKeyChanged {
        module_id: String,
        pinned: String,
        offered: String,
    },

    #[error(
        "artifact digest mismatch for `{module_id}`\n  declared: {declared}\n  computed: {computed}"
    )]
    ArtifactDigestMismatch {
        module_id: String,
        declared: String,
        computed: String,
    },

    #[error("artifact size mismatch for `{module_id}`: declared {declared} bytes, got {actual}")]
    ArtifactSizeMismatch {
        module_id: String,
        declared: u64,
        actual: u64,
    },

    #[error("`{module_id}` requires Thalyx {requirement}, but this runtime is {runtime}")]
    RuntimeTooOld {
        module_id: String,
        requirement: String,
        runtime: String,
    },

    #[error(
        "`{module_id}` is distributed from source; Phase 1 only installs prebuilt signed \
         artifacts, because a locally produced artifact has no expected digest to verify against"
    )]
    SourceDistributionUnsupported { module_id: String },

    #[error("archive entry `{path}` escapes the module tree")]
    UnsafeArchivePath { path: String },

    #[error(
        "archive entry `{path}` writes into `{reserved}/`, which is reserved for Thalyx's own \
         record of the module.\n  \
         A module that could write there could rewrite what it is allowed to do."
    )]
    ReservedArchivePath { path: String, reserved: String },

    #[error(
        "`{module_id}` is installed but its stored manifest is missing or unreadable: {reason}.\n  \
         Reinstall it; there is no way to know what it was allowed to do without it."
    )]
    ManifestUnavailable { module_id: String, reason: String },

    #[error("archive entry `{path}` is a {kind}; only regular files and directories are accepted")]
    UnsafeArchiveEntry { path: String, kind: String },

    #[error("no version of `{module_id}` satisfying `{constraint}` could be resolved: {reason}")]
    UnresolvableModule {
        module_id: String,
        constraint: String,
        reason: String,
    },

    #[error(
        "bundle member `{member}` is {found} bytes, past the {allowed} a bundle is read with.\n  \
         Nothing here has been verified yet, so the size is all there is to go on."
    )]
    BundleMemberTooLarge {
        member: String,
        found: u64,
        allowed: u64,
    },

    #[error(
        "the artifact expands past {allowed} bytes, and stopped being unpacked there.\n  \
         Its {compressed} compressed bytes are signed and their digest matches, so this is a \
         module that ships more than it declares, not a corrupted download."
    )]
    ArtifactExpandsTooFar { allowed: u64, compressed: u64 },

    #[error("the artifact holds more than {allowed} entries")]
    ArtifactTooManyEntries { allowed: usize },

    #[error("`{module_id}` version {version} is already installed")]
    AlreadyInstalled { module_id: String, version: String },

    #[error("`{module_id}` is not installed")]
    NotInstalled { module_id: String },

    #[error("the user did not confirm the requested capabilities")]
    ConfirmationDenied,

    #[error(
        "refusing to run `{module_id}`: the kernel policy map is not loaded, so nothing in the \
         kernel would decide what it may touch{}.\n  \
         Load it with `make -C lsm load`, or pass --unconfined to run it anyway and have the \
         journal say so.",
        also_granted(*permissions, "recorded permission")
    )]
    NothingCanEnforce {
        module_id: String,
        permissions: usize,
    },

    /// A path the human named that this will not execute, and why.
    ///
    /// One variant with a reason rather than four variants, because the caller
    /// does the same thing with all of them — says it back — and the human's
    /// next move depends on the reason and not on the discriminant.
    #[error("`{path}` is not something I can run: {reason}")]
    NotExecutable {
        path: std::path::PathBuf,
        reason: String,
    },

    /// The foreign half of `NothingCanEnforce`, and deliberately not the same
    /// variant.
    ///
    /// The module message ends by offering `--unconfined`. There is no such
    /// mode for a program nobody signed — see
    /// `vault/02-Arquitectura/Programas-Ajenos.md` — so pointing at one would
    /// be telling the human to do something that does not exist.
    #[error(
        "refusing to run `{program}`: the kernel policy map is not loaded, so nothing in the \
         kernel would decide what it may touch{}.\n  \
         Load it with `make -C lsm load`. Nobody signed this program, so there is no \
         unconfined mode to fall back to.",
        also_granted(*grants, "path you granted")
    )]
    NothingCanEnforceForeign {
        program: std::path::PathBuf,
        grants: usize,
    },

    /// Attached, and logging what it would have denied instead of denying it.
    ///
    /// A separate refusal from the one above because it is a separate mistake
    /// with a separate fix — `make -C lsm enforce`, not `make -C lsm load` —
    /// and because for three weeks nothing on this side could tell the two
    /// apart. `is_available()` answers "does the map open"; every caller read
    /// that as "the kernel is enforcing". `make -C lsm load` lands in observe
    /// mode on purpose, so the machine most likely to be running a guest was
    /// the one that would have enforced nothing.
    #[error(
        "refusing to run `{program}`: the kernel side is attached but only observing, so every \
         denial would be written to the log and none of them applied.\n  \
         Make it binding with `make -C lsm enforce`. Nobody signed this program, so there is \
         no unconfined mode to fall back to."
    )]
    ObservingNotEnforcing { program: std::path::PathBuf },

    /// Rule 9, and rule 10: the mode could not be read, which is not the same
    /// as its being off, and neither of them is a reason to start a program
    /// nobody signed.
    #[error(
        "refusing to run `{program}`: the kernel side is attached, but whether it is enforcing \
         or only observing could not be read — {reason}.\n  \
         `make -C lsm status` says which it is. A program nobody signed does not run on an \
         answer nobody got."
    )]
    EnforcementModeUnreadable {
        program: std::path::PathBuf,
        reason: String,
    },

    #[error(
        "no user ids left: Thalyx hands out {first}..{last} and has used them all.\n  \
         Uids are never reused, so this means that many modules have been installed \
         over the life of this system."
    )]
    UidRangeExhausted { first: u32, last: u32 },

    #[error(
        "nothing to roll back: the journal holds no committed installation.\n  \
         Build-then-commit means a failed install published nothing, so there is \
         usually nothing to undo — which is the design working."
    )]
    NothingToRollBack,

    #[error("no journal entry has request id `{request_id}`")]
    NoSuchRequest { request_id: String },

    #[error("`{operation}` cannot be rolled back: {reason}")]
    NotReversible { operation: String, reason: String },

    #[error(
        "`{module_id}` {version} is already gone; there is nothing left of that \
         installation to undo"
    )]
    AlreadyUndone { module_id: String, version: String },

    #[error(
        "refusing to roll back: that entry published `{module_id}` {published}, but \
         {installed} is what is installed now.\n  \
         Undoing it would delete a version this entry never put there. Remove it \
         deliberately with `thalyx module remove {module_id}` if that is what you want."
    )]
    RollbackSuperseded {
        module_id: String,
        published: String,
        installed: String,
    },

    #[error(transparent)]
    Snapshot(#[from] thalyx_snapshot::SnapshotError),

    #[error(transparent)]
    Sandbox(#[from] thalyx_sandbox::SandboxError),

    #[error("fault injected at {0:?}")]
    InjectedFault(fault::FaultPoint),
}

impl CoreError {
    pub(crate) fn io(path: impl Into<std::path::PathBuf>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
