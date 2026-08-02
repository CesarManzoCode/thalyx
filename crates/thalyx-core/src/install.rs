//! Installation and removal: the canonical flow, in order.
//!
//! This function is the executable form of
//! `vault/04-Flujo-Canonico/Caso-Instalar-Modulo.md`. The step numbers in the
//! comments below match the steps in that note, so a change to one should be a
//! change to the other.
//!
//! Nothing here trusts its input. Digests are recomputed, signatures are
//! checked against a pinned key, and the confirmation prompt is generated
//! locally rather than accepted from a caller.

use crate::bundle::{self, Bundle};
use crate::commit;
use crate::fault::{self, FaultPoint};
use crate::keystore::Keystore;
use crate::permissions::{PendingGrants, Registry};
use crate::reconcile;
use crate::store::Store;
use crate::trusted_path::CapabilityPrompt;
use crate::{CoreError, RUNTIME_VERSION, Result};
use std::path::Path;
use thalyx_contract::{Contract, Operation};
use thalyx_journal::{Entry, Journal, Origin, Outcome};
use thalyx_manifest::{Distribution, Manifest};

/// How the human answers a capability prompt.
///
/// A trait rather than a boolean so that the decision always comes from
/// somewhere explicit. The prompt passed in is the one the core generated.
pub trait Confirmer {
    fn confirm(&mut self, prompt: &CapabilityPrompt) -> bool;
}

/// Refuses everything. The safe default for non-interactive callers.
pub struct DenyAll;

impl Confirmer for DenyAll {
    fn confirm(&mut self, _prompt: &CapabilityPrompt) -> bool {
        false
    }
}

/// Accepts everything. Tests and explicitly non-interactive installs only.
pub struct AllowAll;

impl Confirmer for AllowAll {
    fn confirm(&mut self, _prompt: &CapabilityPrompt) -> bool {
        true
    }
}

pub struct InstallRequest<'a> {
    pub bundle_path: &'a Path,
    /// The contract authorising this operation.
    ///
    /// Carries the request id that ties the pending grants and the journal
    /// entry together, and the per-field provenance the core checks before
    /// anything else. See `vault/04-Flujo-Canonico/Contrato-Estructurado.md`.
    pub contract: Contract,
}

impl InstallRequest<'_> {
    fn request_id(&self) -> &str {
        &self.contract.caller.request_id
    }

    fn origin(&self) -> Origin {
        self.contract.effective_origin()
    }
}

#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub module_id: String,
    pub version: String,
    pub replaced: Option<String>,
    pub files: Vec<String>,
    pub granted: usize,
}

/// Install a module from a `.thmod` bundle.
pub fn install(
    store: &Store,
    request: InstallRequest<'_>,
    confirmer: &mut dyn Confirmer,
) -> Result<InstallOutcome> {
    let journal = Journal::open(store.journal_path())?;

    match install_inner(store, &request, confirmer) {
        Ok(outcome) => {
            journal.append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: "install_module".to_string(),
                module_id: Some(outcome.module_id.clone()),
                version: Some(outcome.version.clone()),
                outcome: Outcome::Success,
                request_id: request.request_id().to_string(),
                origin: request.origin(),
                snapshot: None,
                notes: outcome
                    .replaced
                    .as_ref()
                    .map(|previous| vec![format!("replaced version {previous}")])
                    .unwrap_or_default(),
            })?;
            Ok(outcome)
        }
        Err(error) => {
            // A failure is recorded, never erased. With build-then-commit there
            // is nothing to undo, but the attempt still has to be visible.
            let outcome = match &error {
                CoreError::ConfirmationDenied => Outcome::Rejected {
                    reason: error.to_string(),
                },
                CoreError::SignatureRejected { .. }
                | CoreError::PublisherKeyChanged { .. }
                | CoreError::ArtifactDigestMismatch { .. }
                | CoreError::ArtifactSizeMismatch { .. }
                | CoreError::RuntimeTooOld { .. }
                | CoreError::SourceDistributionUnsupported { .. }
                | CoreError::UnsafeArchivePath { .. }
                | CoreError::UnsafeArchiveEntry { .. }
                | CoreError::ReservedArchivePath { .. } => Outcome::NotCommitted {
                    reason: error.to_string(),
                },
                _ => Outcome::Rejected {
                    reason: error.to_string(),
                },
            };
            let _ = journal.append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: "install_module".to_string(),
                module_id: None,
                version: None,
                outcome,
                request_id: request.request_id().to_string(),
                origin: request.origin(),
                snapshot: None,
                notes: vec![],
            });
            Err(error)
        }
    }
}

fn install_inner(
    store: &Store,
    request: &InstallRequest<'_>,
    confirmer: &mut dyn Confirmer,
) -> Result<InstallOutcome> {
    // Settle anything a previous run left hanging before adding to it, so the
    // journal never accumulates ambiguity.
    reconcile::reconcile(store)?;

    // Step 4a — the contract itself: schema, provenance of every effectful
    // field, and policy. Runs before the bundle is even opened, so a hostile
    // contract is refused before it causes any work.
    request.contract.validate()?;

    if request.contract.operation != Operation::InstallModule {
        return Err(CoreError::MalformedBundle(format!(
            "contract operation is `{}`, not install_module",
            request.contract.operation
        )));
    }

    let bundle = Bundle::read(request.bundle_path)?;
    let manifest = &bundle.manifest;

    // Step 4b — distribution. Installation must not execute module code, so a
    // source-distributed module has no verifiable artifact and is refused.
    if manifest.distribution != Distribution::Prebuilt {
        return Err(CoreError::SourceDistributionUnsupported {
            module_id: manifest.id.clone(),
        });
    }

    // Step 4c — signature over the canonical manifest form.
    manifest
        .verify_signature(&bundle.signature)
        .map_err(|_| CoreError::SignatureRejected {
            module_id: manifest.id.clone(),
        })?;

    // Step 4d — publisher key pinning. Checked, not yet recorded: a failed
    // install must not leave a key pinned behind it.
    let mut keystore = Keystore::load(store.keystore_path())?;
    keystore.check(&manifest.id, &manifest.publisher_key)?;

    // Step 4e — containment. The manifest is the authority on what a module
    // may hold; a contract asking for more is refused outright.
    request.contract.validate_against_manifest(manifest)?;

    // Step 4f — runtime requirement.
    check_runtime(manifest)?;

    if store.installed_version(&manifest.id).as_deref() == Some(manifest.version.as_str()) {
        return Err(CoreError::AlreadyInstalled {
            module_id: manifest.id.clone(),
            version: manifest.version.clone(),
        });
    }

    // Step 10, brought forward — the core computes the digest itself, and
    // before anything is written. It never accepts a digest reported by a
    // component outside the TCB.
    let computed = bundle::digest(&bundle.artifact);
    if computed != manifest.artifact_digest() {
        return Err(CoreError::ArtifactDigestMismatch {
            module_id: manifest.id.clone(),
            declared: manifest.artifact.hash.clone(),
            computed: thalyx_manifest::format_sha256(&computed),
        });
    }
    if bundle.artifact.len() as u64 != manifest.artifact.size {
        return Err(CoreError::ArtifactSizeMismatch {
            module_id: manifest.id.clone(),
            declared: manifest.artifact.size,
            actual: bundle.artifact.len() as u64,
        });
    }

    // Step 6 — confirmation, generated and rendered by the core.
    if let Some(prompt) = CapabilityPrompt::for_manifest(manifest)
        && !confirmer.confirm(&prompt)
    {
        return Err(CoreError::ConfirmationDenied);
    }

    // Step 7 — permissions become pending, not effective. Dropping this value
    // on any error path is what makes a failed install leave no live grant.
    let pending = PendingGrants::new(
        &manifest.id,
        request.request_id(),
        manifest.permissions.clone(),
    );

    fault::checkpoint(FaultPoint::PostVerify)?;

    // Step 9 — staging, in the same subvolume as the destination.
    let staging = store.new_staging_dir()?;
    let files = match bundle::unpack_artifact(&bundle.artifact, &staging) {
        Ok(files) => files,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
    };

    // The manifest travels with the module.
    //
    // Written into staging, so the same `rename` that publishes the files
    // publishes the record of what they are: there is no ordering in which a
    // version exists without its manifest. The runtime needs it to know the
    // module's entrypoint, and the signature is kept alongside so that the
    // record can be checked again later against the pinned publisher key
    // rather than believed because it is on our own disk.
    if let Err(error) = write_record(&staging, &bundle) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    fault::checkpoint(FaultPoint::PostStage)?;

    let replaced = store.installed_version(&manifest.id);

    // Step 12, written *before* the commit on purpose.
    //
    // The `current` symlink is the single atomic point that decides both
    // "installed" and "authorised": a grant is in force only while the module
    // it belongs to is current (see `effective_permissions`). Recording the
    // grants first means that the instant the symlink swings, the module and
    // its permissions are consistent — and if the process dies before the
    // swap, the record is inert because no module points at it.
    //
    // Writing them afterwards would leave a window where the module is
    // installed and holds nothing, and would need a second atomic step to
    // close. This way there is only ever one.
    let mut registry = Registry::load(store.permissions_path())?;
    registry.make_effective(&pending)?;

    // Pin the publisher key here too, for a sharper reason.
    //
    // Pinning after the commit meant that a crash in between left the module
    // installed with no key pinned to its id — so the next bundle offered for
    // that id, signed by anyone, would be accepted as a first sighting. That is
    // the publisher impersonation path the threat model names as adversary 3,
    // opened by an interruption.
    //
    // Pinning here is safe: nothing reaches this point without a valid
    // signature, a matching digest and human confirmation. An attacker who got
    // this far already convinced the user, so recording which key they
    // convinced them with costs nothing and closes the window.
    keystore.pin(&manifest.id, &manifest.publisher_key)?;

    // Step 11a — announce the intent before anything moves.
    //
    // If the process dies after the commit but before the terminal entry is
    // written, this is what survives: an unresolved intent that reconciliation
    // settles against the disk. Without it, the installation would be real and
    // unrecorded.
    let journal = Journal::open(store.journal_path())?;
    reconcile::record_intent(
        &journal,
        "install_module",
        &manifest.id,
        &manifest.version,
        request.request_id(),
        request.origin(),
    )?;

    // Step 11b — the atomic commit. FaultPoint::MidCommit lives inside.
    commit::publish(store, &staging, &manifest.id, &manifest.version)?;

    fault::checkpoint(FaultPoint::PostCommit)?;

    // A replaced version is only removed after the new one is live.
    if let Some(previous) = &replaced
        && previous != &manifest.version
    {
        let previous_dir = store.version_dir(&manifest.id, previous);
        let _ = std::fs::remove_dir_all(previous_dir);
    }

    Ok(InstallOutcome {
        module_id: manifest.id.clone(),
        version: manifest.version.clone(),
        replaced,
        files,
        granted: pending.permissions().len(),
    })
}

/// Write Thalyx's own record into a staged module tree.
fn write_record(staging: &Path, bundle: &Bundle) -> Result<()> {
    let reserved = staging.join(bundle::RESERVED_DIR);
    std::fs::create_dir_all(&reserved).map_err(|e| CoreError::io(&reserved, e))?;

    let manifest_path = reserved.join(bundle::MANIFEST_MEMBER);
    std::fs::write(&manifest_path, &bundle.manifest_source)
        .map_err(|e| CoreError::io(&manifest_path, e))?;

    let signature_path = reserved.join(bundle::SIGNATURE_MEMBER);
    std::fs::write(&signature_path, &bundle.signature_source)
        .map_err(|e| CoreError::io(&signature_path, e))?;

    Ok(())
}

/// The manifest of the version of a module that is currently installed.
///
/// Re-verified on every read, against the key pinned for that id, rather than
/// trusted because it came off our own disk. The threat model does not grant
/// the store integrity — an attacker who can write to it could otherwise widen
/// a module's declared permissions or point its entrypoint elsewhere, and
/// nothing downstream would notice.
pub fn installed_manifest(store: &Store, module_id: &str) -> Result<Manifest> {
    let version = store
        .installed_version(module_id)
        .ok_or_else(|| CoreError::NotInstalled {
            module_id: module_id.to_string(),
        })?;

    let unavailable = |reason: String| CoreError::ManifestUnavailable {
        module_id: module_id.to_string(),
        reason,
    };

    let manifest_path = store.manifest_path(module_id, &version);
    let source = std::fs::read_to_string(&manifest_path).map_err(|e| unavailable(e.to_string()))?;
    let manifest = Manifest::parse(&source)?;

    let signature_path = store.manifest_signature_path(module_id, &version);
    let signature_source =
        std::fs::read_to_string(&signature_path).map_err(|e| unavailable(e.to_string()))?;
    let signature = thalyx_manifest::Signature::parse(&signature_source)
        .map_err(|e| unavailable(e.to_string()))?;

    manifest
        .verify_signature(&signature)
        .map_err(|_| CoreError::SignatureRejected {
            module_id: module_id.to_string(),
        })?;

    // The signature only says *someone* vouched for these bytes. The keystore
    // says which someone was accepted for this id, and that is the check that
    // makes re-verification worth anything.
    let keystore = Keystore::load(store.keystore_path())?;
    keystore.check(&manifest.id, &manifest.publisher_key)?;

    if manifest.id != module_id || manifest.version != version {
        return Err(unavailable(format!(
            "stored manifest describes `{}` {}, not `{module_id}` {version}",
            manifest.id, manifest.version
        )));
    }

    Ok(manifest)
}

/// Remove a module and revoke everything it held.
pub fn remove(store: &Store, module_id: &str, request_id: &str) -> Result<String> {
    let journal = Journal::open(store.journal_path())?;

    match commit::unpublish(store, module_id) {
        Ok(version) => {
            let mut registry = Registry::load(store.permissions_path())?;
            registry.revoke_all(module_id)?;

            journal.append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: "remove_module".to_string(),
                module_id: Some(module_id.to_string()),
                version: Some(version.clone()),
                outcome: Outcome::Success,
                request_id: request_id.to_string(),
                origin: Origin::UserUtterance,
                snapshot: None,
                notes: vec!["all permissions revoked".to_string()],
            })?;
            Ok(version)
        }
        Err(error) => {
            let _ = journal.append(&Entry {
                timestamp: thalyx_journal::now(),
                operation: "remove_module".to_string(),
                module_id: Some(module_id.to_string()),
                version: None,
                outcome: Outcome::Rejected {
                    reason: error.to_string(),
                },
                request_id: request_id.to_string(),
                origin: Origin::UserUtterance,
                snapshot: None,
                notes: vec![],
            });
            Err(error)
        }
    }
}

fn check_runtime(manifest: &Manifest) -> Result<()> {
    let requirement = semver::VersionReq::parse(&manifest.requires.thalyx)
        .expect("validated at manifest parse time");
    let runtime = semver::Version::parse(RUNTIME_VERSION).expect("crate version is valid semver");
    if requirement.matches(&runtime) {
        Ok(())
    } else {
        Err(CoreError::RuntimeTooOld {
            module_id: manifest.id.clone(),
            requirement: manifest.requires.thalyx.clone(),
            runtime: RUNTIME_VERSION.to_string(),
        })
    }
}
