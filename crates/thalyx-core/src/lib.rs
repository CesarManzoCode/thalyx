//! `thalyx-core` — validation, verification, staging and atomic commit.
//!
//! This crate is the trusted computing base. Everything it is handed comes from
//! somewhere it does not trust, so every value is re-derived rather than
//! believed: digests are recomputed, signatures are checked against a pinned
//! key, and confirmation text is generated here rather than accepted from a
//! caller.
//!
//! See `vault/11-Seguridad/Modelo-de-Amenaza.md` and `vault/02-Arquitectura/Core.md`.

pub mod bundle;
pub mod commit;
pub mod fault;
pub mod install;
pub mod keystore;
pub mod permissions;
pub mod reconcile;
pub mod run;
pub mod store;
pub mod trusted_path;

pub use install::{InstallOutcome, InstallRequest, install, installed_manifest, remove};
pub use run::{RunOutcome, RunRequest, run};
pub use store::Store;

/// The permissions a module actually holds right now.
///
/// A grant recorded in the registry is in force **only while the module it
/// belongs to is the current version**. That is what makes the `current`
/// symlink the single atomic point deciding both "installed" and "authorised":
/// there is no second transition that could be interrupted separately.
///
/// A record left behind by an interrupted install is therefore inert, not a
/// live grant. `thalyx store clean` reclaims it.
pub fn effective_permissions(
    store: &Store,
    registry: &permissions::Registry,
    module_id: &str,
) -> Vec<permissions::Grant> {
    if !store.is_installed(module_id) {
        return Vec::new();
    }
    registry.effective(module_id).to_vec()
}

/// The runtime version that a manifest's `requires.thalyx` is matched against.
pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");

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

    #[error("bundle is malformed: {0}")]
    MalformedBundle(String),

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

    #[error("`{module_id}` version {version} is already installed")]
    AlreadyInstalled { module_id: String, version: String },

    #[error("`{module_id}` is not installed")]
    NotInstalled { module_id: String },

    #[error("the user did not confirm the requested capabilities")]
    ConfirmationDenied,

    #[error(
        "refusing to run `{module_id}`: the kernel policy map is not loaded, so none of its \
         {permissions} permission(s) would be enforced.\n  \
         Load it with `make -C lsm load`, or pass --unconfined to run it anyway and have the \
         journal say so."
    )]
    NothingCanEnforce {
        module_id: String,
        permissions: usize,
    },

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
