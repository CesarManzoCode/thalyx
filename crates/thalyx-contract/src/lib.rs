//! The structured contract.
//!
//! The agent does not act. It produces one of these, and `thalyx-core`
//! decides whether it may be executed. Everything the agent can influence
//! passes through this type, which makes it the boundary between the component
//! that is not trusted and the one that is.
//!
//! The property that makes that boundary hold is **per-field provenance**:
//! every field that has an effect on the system declares where it came from,
//! and the core refuses any effectful field sourced from content Thalyx does
//! not control. A well-formed contract carrying an attacker's intent is
//! rejected on structure alone — no judgement about the text is required.
//!
//! See `vault/04-Flujo-Canonico/Contrato-Estructurado.md` and
//! `vault/11-Seguridad/Marcado-de-Origen.md`.

mod origins;

pub use origins::{EFFECTFUL_FIELDS, Origin, Origins};

use serde::{Deserialize, Serialize};
use thalyx_manifest::{Manifest, Permission, PermissionKind};

/// Contract schema version this crate understands.
pub const SUPPORTED_VERSION: &str = "1.0";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ContractError {
    #[error("contract is not valid JSON: {0}")]
    Malformed(String),

    #[error("unsupported contract version `{found}`, this build understands {supported}")]
    UnsupportedVersion { found: String, supported: String },

    #[error(
        "field `{field}` has effect on the system but declares no origin.\n  \
         A field with no declared provenance is indistinguishable from one whose \
         provenance was stripped, so it is refused rather than assumed."
    )]
    MissingOrigin { field: &'static str },

    #[error(
        "field `{field}` originates in untrusted content and cannot have effect.\n  \
         Repository text, third-party manifests and network responses may inform \
         what the user is shown. They may never determine what an operation does."
    )]
    UntrustedOrigin { field: &'static str },

    #[error("contract has no targets")]
    NoTargets,

    #[error("version constraint `{constraint}` is not valid: {reason}")]
    InvalidConstraint { constraint: String, reason: String },

    #[error(
        "contract requests permissions the module does not declare: {}.\n  \
         The manifest is the authority on what a module may hold.",
        .undeclared.join(", ")
    )]
    PermissionsExceedManifest { undeclared: Vec<String> },

    #[error(
        "contract requests a permission of type `{kind}` without requiring confirmation.\n  \
         Persistent permissions always need explicit human confirmation, with no exceptions \
         and regardless of the module's reputation."
    )]
    ConfirmationNotRequired { kind: PermissionKind },
}

pub type Result<T> = std::result::Result<T, ContractError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    InstallModule,
    RemoveModule,
    DeleteFiles,
    BuildGraph,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Operation::InstallModule => "install_module",
            Operation::RemoveModule => "remove_module",
            Operation::DeleteFiles => "delete_files",
            Operation::BuildGraph => "build_graph",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rollback {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub snapshot_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caller {
    pub module_id: String,
    /// Ties this contract to its journal entry and its pending grants.
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub version: String,
    pub operation: Operation,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub constraint: Option<String>,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub sandbox_profile: Option<String>,
    #[serde(default)]
    pub rollback: Rollback,
    pub caller: Caller,
    /// Where each field came from. Not optional for anything with effect.
    pub origins: Origins,
}

impl Contract {
    pub fn parse(source: &str) -> Result<Self> {
        let contract: Contract =
            serde_json::from_str(source).map_err(|e| ContractError::Malformed(e.to_string()))?;
        contract.validate()?;
        Ok(contract)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a contract is always serialisable")
    }

    /// Checks that need nothing but the contract itself.
    ///
    /// Deliberately separate from [`Contract::validate_against_manifest`]:
    /// these can run the instant a contract arrives, before anything has been
    /// fetched, so a hostile contract is rejected before it causes any work.
    pub fn validate(&self) -> Result<()> {
        if self.version != SUPPORTED_VERSION {
            return Err(ContractError::UnsupportedVersion {
                found: self.version.clone(),
                supported: SUPPORTED_VERSION.to_string(),
            });
        }

        // Provenance, before anything else. A contract that fails this is not
        // examined further: there is nothing to learn from the rest of it.
        self.origins.validate()?;

        if matches!(
            self.operation,
            Operation::InstallModule | Operation::RemoveModule | Operation::DeleteFiles
        ) && self.targets.is_empty()
        {
            return Err(ContractError::NoTargets);
        }

        if let Some(constraint) = &self.constraint
            && let Err(error) = semver::VersionReq::parse(constraint)
        {
            return Err(ContractError::InvalidConstraint {
                constraint: constraint.clone(),
                reason: error.to_string(),
            });
        }

        // A contract asking for a permission that always needs confirmation,
        // while declaring that confirmation is not required, is either a bug
        // or an attempt. Either way it does not proceed.
        for permission in &self.permissions {
            if permission.kind.requires_confirmation() && !self.requires_confirmation {
                return Err(ContractError::ConfirmationNotRequired {
                    kind: permission.kind,
                });
            }
        }

        Ok(())
    }

    /// Checks that need the module's manifest.
    ///
    /// The containment rule works in both directions. Asking for more than the
    /// manifest declares is refused. Asking for *less* is allowed but does not
    /// shrink what gets confirmed: the human is shown the manifest's full set,
    /// because that is what the module will actually be able to hold.
    pub fn validate_against_manifest(&self, manifest: &Manifest) -> Result<()> {
        let declared: std::collections::HashSet<(&str, &str)> = manifest
            .permissions
            .iter()
            .map(|p| (p.resource.as_str(), p.action.as_str()))
            .collect();

        let undeclared: Vec<String> = self
            .permissions
            .iter()
            .filter(|p| !declared.contains(&(p.resource.as_str(), p.action.as_str())))
            .map(|p| format!("{} {}", p.action, p.resource))
            .collect();

        if !undeclared.is_empty() {
            return Err(ContractError::PermissionsExceedManifest { undeclared });
        }

        Ok(())
    }

    /// What the human must be shown: the manifest's set, not the contract's.
    pub fn permissions_to_confirm<'a>(&self, manifest: &'a Manifest) -> Vec<&'a Permission> {
        manifest.permissions_requiring_confirmation()
    }

    /// Permissions in the manifest that this contract did not mention.
    ///
    /// Surfaced so the confirmation can point them out. Under-reporting is the
    /// dangerous direction — a human who confirms read access while the module
    /// also holds network access has authorised something never shown to them.
    pub fn unmentioned_permissions<'a>(&self, manifest: &'a Manifest) -> Vec<&'a Permission> {
        let mentioned: std::collections::HashSet<(&str, &str)> = self
            .permissions
            .iter()
            .map(|p| (p.resource.as_str(), p.action.as_str()))
            .collect();

        manifest
            .permissions
            .iter()
            .filter(|p| !mentioned.contains(&(p.resource.as_str(), p.action.as_str())))
            .collect()
    }

    /// The single origin to record in the journal for this operation.
    ///
    /// The strongest claim a contract can make about itself is only as good as
    /// its weakest field, so this reports the least trusted origin present.
    pub fn effective_origin(&self) -> thalyx_journal::Origin {
        self.origins.least_trusted().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract_json(origins: &str) -> String {
        format!(
            r#"{{
  "version": "1.0",
  "operation": "install_module",
  "targets": ["org.publisher.pyassist"],
  "constraint": "^2.3",
  "permissions": [
    {{"resource": "net", "action": "outbound", "type": "persistent"}}
  ],
  "requires_confirmation": true,
  "sandbox_profile": "module_standard",
  "caller": {{"module_id": "thalyx-agent", "request_id": "abc-123"}},
  "origins": {origins}
}}"#
        )
    }

    const GOOD_ORIGINS: &str = r#"{
    "operation": "user_utterance",
    "targets": "user_utterance",
    "constraint": "system_state",
    "permissions": "system_state"
  }"#;

    fn manifest_with(permissions: &str) -> Manifest {
        let src = format!(
            r#"
format_version = 1
id             = "org.publisher.pyassist"
name           = "PyAssist Core"
version        = "2.3.1"
license        = "GPL-3.0-or-later"
publisher_key  = "ed25519:3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 1

{permissions}
"#
        );
        Manifest::parse(&src).unwrap()
    }

    #[test]
    fn parses_a_well_formed_contract() {
        let contract = Contract::parse(&contract_json(GOOD_ORIGINS)).expect("valid");
        assert_eq!(contract.operation, Operation::InstallModule);
        assert_eq!(contract.targets, vec!["org.publisher.pyassist"]);
        assert_eq!(contract.caller.request_id, "abc-123");
    }

    #[test]
    fn rejects_an_effectful_field_sourced_from_untrusted_content() {
        // The attack the whole mechanism exists for: a repository description
        // that talks the agent into naming a different module. The contract is
        // perfectly well-formed; only its provenance gives it away.
        let origins = r#"{
    "operation": "user_utterance",
    "targets": "untrusted_content",
    "constraint": "system_state",
    "permissions": "system_state"
  }"#;

        assert_eq!(
            Contract::parse(&contract_json(origins)),
            Err(ContractError::UntrustedOrigin { field: "targets" })
        );
    }

    #[test]
    fn rejects_untrusted_origin_on_every_effectful_field() {
        for field in EFFECTFUL_FIELDS {
            let mut map = serde_json::json!({
                "operation": "user_utterance",
                "targets": "user_utterance",
                "constraint": "system_state",
                "permissions": "system_state"
            });
            map[field] = serde_json::json!("untrusted_content");

            let result = Contract::parse(&contract_json(&map.to_string()));
            assert_eq!(
                result,
                Err(ContractError::UntrustedOrigin { field }),
                "`{field}` must not be allowed to come from untrusted content"
            );
        }
    }

    #[test]
    fn rejects_a_missing_origin_rather_than_assuming_one() {
        // A stripped origin and an absent one look identical from here, so
        // absent is refused. Defaulting to "trusted" would make the whole
        // mechanism opt-in for the attacker.
        let origins = r#"{
    "operation": "user_utterance",
    "constraint": "system_state",
    "permissions": "system_state"
  }"#;

        assert_eq!(
            Contract::parse(&contract_json(origins)),
            Err(ContractError::MissingOrigin { field: "targets" })
        );
    }

    #[test]
    fn untrusted_content_is_allowed_on_fields_without_effect() {
        // The point is not to keep repository text out. It is to keep it from
        // deciding what happens. A description the agent read may perfectly
        // well shape what the user is shown.
        let origins = r#"{
    "operation": "user_utterance",
    "targets": "user_utterance",
    "constraint": "system_state",
    "permissions": "system_state",
    "description": "untrusted_content"
  }"#;

        assert!(Contract::parse(&contract_json(origins)).is_ok());
    }

    #[test]
    fn rejects_unsupported_versions() {
        let source = contract_json(GOOD_ORIGINS).replace("\"1.0\"", "\"2.0\"");
        assert!(matches!(
            Contract::parse(&source),
            Err(ContractError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn rejects_a_persistent_permission_that_waives_confirmation() {
        let source = contract_json(GOOD_ORIGINS).replace(
            "\"requires_confirmation\": true",
            "\"requires_confirmation\": false",
        );
        assert!(matches!(
            Contract::parse(&source),
            Err(ContractError::ConfirmationNotRequired { .. })
        ));
    }

    #[test]
    fn rejects_permissions_the_manifest_does_not_declare() {
        let contract = Contract::parse(&contract_json(GOOD_ORIGINS)).unwrap();
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "/home/user/projects"
action   = "read"
type     = "persistent"
"#,
        );

        match contract.validate_against_manifest(&manifest) {
            Err(ContractError::PermissionsExceedManifest { undeclared }) => {
                assert_eq!(undeclared, vec!["outbound net"]);
            }
            other => panic!("expected the request to be refused, got {other:?}"),
        }
    }

    #[test]
    fn a_contract_asking_for_less_still_confirms_the_manifests_full_set() {
        // Under-reporting is the dangerous direction. The contract mentions
        // network access only; the module also holds read access to a user
        // directory, and the human has to see both.
        let contract = Contract::parse(&contract_json(GOOD_ORIGINS)).unwrap();
        let manifest = manifest_with(
            r#"
[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"

[[permissions]]
resource = "/home/user/secrets"
action   = "read"
type     = "persistent"
"#,
        );

        assert!(contract.validate_against_manifest(&manifest).is_ok());
        assert_eq!(contract.permissions_to_confirm(&manifest).len(), 2);

        let unmentioned = contract.unmentioned_permissions(&manifest);
        assert_eq!(unmentioned.len(), 1);
        assert_eq!(unmentioned[0].resource, "/home/user/secrets");
    }

    #[test]
    fn rejects_an_invalid_version_constraint() {
        let source = contract_json(GOOD_ORIGINS).replace("\"^2.3\"", "\"not-a-constraint\"");
        assert!(matches!(
            Contract::parse(&source),
            Err(ContractError::InvalidConstraint { .. })
        ));
    }

    #[test]
    fn rejects_an_operation_with_nothing_to_act_on() {
        let source = contract_json(GOOD_ORIGINS).replace(
            "\"targets\": [\"org.publisher.pyassist\"]",
            "\"targets\": []",
        );
        assert_eq!(Contract::parse(&source), Err(ContractError::NoTargets));
    }

    #[test]
    fn the_recorded_origin_is_the_least_trusted_field() {
        // A contract is only as trustworthy as its weakest field, so that is
        // what the journal records.
        let origins = r#"{
    "operation": "user_utterance",
    "targets": "user_utterance",
    "constraint": "system_state",
    "permissions": "system_state"
  }"#;
        let contract = Contract::parse(&contract_json(origins)).unwrap();
        assert_eq!(
            contract.effective_origin(),
            thalyx_journal::Origin::SystemState
        );
    }

    #[test]
    fn round_trips_through_json() {
        let contract = Contract::parse(&contract_json(GOOD_ORIGINS)).unwrap();
        let reparsed = Contract::parse(&contract.to_json()).unwrap();
        assert_eq!(contract, reparsed);
    }
}
