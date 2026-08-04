//! Parsing, validation and signature verification of `.thmod` manifests.
//!
//! The manifest is the authority on what a module is allowed to do. Everything
//! downstream — the permissions the user is asked to confirm, the artifact hash
//! the core verifies — comes from here, and only from here.
//!
//! See `vault/02-Arquitectura/Formato-Manifiesto-Thmod.md`.

mod signature;

pub use signature::{PublicKey, Signature, SigningKey, canonical_bytes};

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Manifest schema version this crate understands.
pub const SUPPORTED_FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("unsupported manifest format_version {found}, this build understands {supported}")]
    UnsupportedFormatVersion { found: u32, supported: u32 },

    #[error("module id `{0}` is not a valid reverse-DNS identifier")]
    InvalidId(String),

    #[error("version `{version}` is not valid semver: {source}")]
    InvalidVersion {
        version: String,
        source: semver::Error,
    },

    #[error("requires.thalyx `{req}` is not a valid version requirement: {source}")]
    InvalidRequirement { req: String, source: semver::Error },

    #[error(
        "permission {index} declares unknown type `{found}`, expected jit, session or persistent"
    )]
    UnknownPermissionType { index: usize, found: String },

    #[error("permission {index} declares unknown action `{found}`")]
    UnknownPermissionAction { index: usize, found: String },

    #[error("artifact hash must be `sha256:<64 hex chars>`, got `{0}`")]
    MalformedHash(String),

    #[error(
        "module declares a dependency on module `{0}`; inter-module dependencies are not \
             supported in Phase 1 (see Resolucion-de-Versiones)"
    )]
    InterModuleDependency(String),

    #[error(transparent)]
    Signature(#[from] signature::SignatureError),
}

/// A parsed and validated `.thmod` manifest.
///
/// Construction goes through [`Manifest::parse`], so an instance of this type is
/// always structurally valid. It says nothing about whether the signature checks
/// out — that is [`Manifest::verify_signature`].
///
/// ## Why unknown fields are refused
///
/// `deny_unknown_fields` here is doing the same work it does in
/// `thalyx-agent`'s proposal schema, for the same reason. A manifest is the
/// authority on what a module may do, and the failure mode of ignoring a field
/// is silent in the dangerous direction:
///
/// - A publisher writing `permision` instead of `permissions` ships a module
///   that asks for nothing, is confirmed by nobody, and does not work — which
///   is survivable. A publisher writing a field this build has never heard of
///   because it was added in a *later* schema is the other case, and there the
///   right answer is to refuse rather than to install something whose meaning
///   this build cannot see.
/// - The signature covers the canonical form derived from *these* fields.
///   Anything the parser drops is, by construction, outside what was signed —
///   so silently accepting unknown fields means a bundle can carry text no
///   signature vouches for, into a file Thalyx then keeps on disk.
///
/// `format_version` already refuses a schema from the future. This refuses the
/// case that slips past it: a field added without the version being bumped.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub license: String,
    pub publisher_key: String,
    #[serde(default)]
    pub distribution: Distribution,
    pub artifact: Artifact,
    #[serde(default)]
    pub requires: Requires,
    #[serde(default, rename = "permissions")]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub entrypoints: std::collections::BTreeMap<String, String>,
}

/// How the module reaches the user's machine.
///
/// Phase 1 only accepts `Prebuilt`: installation must not execute module code,
/// because a locally produced artifact has no expected hash to verify against.
/// See `vault/04-Flujo-Canonico/Verificacion-y-Distribucion.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Distribution {
    #[default]
    Prebuilt,
    Source,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    #[serde(default = "default_thalyx_req")]
    pub thalyx: String,
    /// Present only so that a manifest declaring inter-module dependencies is
    /// rejected with a clear message instead of silently ignored.
    #[serde(default)]
    pub modules: std::collections::BTreeMap<String, String>,
}

fn default_thalyx_req() -> String {
    "*".to_string()
}

/// Hand-written rather than derived: a derived `Default` would leave `thalyx`
/// as the empty string, which is not a valid version requirement. That only
/// shows up when `[requires]` is omitted entirely, which is exactly the case a
/// derived impl would get wrong.
impl Default for Requires {
    fn default() -> Self {
        Self {
            thalyx: default_thalyx_req(),
            modules: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permission {
    pub resource: String,
    pub action: String,
    #[serde(rename = "type")]
    pub kind: PermissionKind,
}

/// The three permission types are distinct security policies, not just
/// different durations. See `vault/03-Primitivas/Permisos-JIT.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionKind {
    /// Automatic, expires on its own, low risk.
    Jit,
    /// Lives as long as the session.
    Session,
    /// Never expires on its own. Always requires explicit human confirmation,
    /// with no exceptions and regardless of the module's reputation.
    Persistent,
}

impl PermissionKind {
    /// Whether a permission of this kind may ever be granted without asking a human.
    pub fn requires_confirmation(self) -> bool {
        matches!(self, PermissionKind::Persistent)
    }
}

impl std::fmt::Display for PermissionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PermissionKind::Jit => "jit",
            PermissionKind::Session => "session",
            PermissionKind::Persistent => "persistent",
        };
        f.write_str(s)
    }
}

impl Permission {
    /// Human-readable rendering, used by the trusted path.
    ///
    /// This text is produced by the core from validated fields, never composed
    /// by the agent. See `vault/11-Seguridad/Camino-Confiable.md`.
    pub fn describe(&self) -> String {
        match (self.resource.as_str(), self.action.as_str()) {
            ("net", "outbound") => "outbound network access".to_string(),
            ("net", a) => format!("network access ({a})"),
            (r, "read") => format!("read access to {r}"),
            (r, "write") => format!("write access to {r}"),
            (r, "execute") => format!("execute access to {r}"),
            (r, a) => format!("{a} access to {r}"),
        }
    }
}

impl Manifest {
    /// Parse and structurally validate a manifest.
    ///
    /// Rejects anything the rest of the system would have to guess about:
    /// unknown schema versions, malformed identifiers, unknown permission
    /// types, and inter-module dependencies (unsupported in Phase 1).
    pub fn parse(source: &str) -> Result<Self, ManifestError> {
        let manifest: Manifest = toml::from_str(source)?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if self.format_version != SUPPORTED_FORMAT_VERSION {
            return Err(ManifestError::UnsupportedFormatVersion {
                found: self.format_version,
                supported: SUPPORTED_FORMAT_VERSION,
            });
        }

        if !is_valid_module_id(&self.id) {
            return Err(ManifestError::InvalidId(self.id.clone()));
        }

        semver::Version::parse(&self.version).map_err(|source| ManifestError::InvalidVersion {
            version: self.version.clone(),
            source,
        })?;

        semver::VersionReq::from_str(&self.requires.thalyx).map_err(|source| {
            ManifestError::InvalidRequirement {
                req: self.requires.thalyx.clone(),
                source,
            }
        })?;

        if let Some(dep) = self.requires.modules.keys().next() {
            return Err(ManifestError::InterModuleDependency(dep.clone()));
        }

        for (index, permission) in self.permissions.iter().enumerate() {
            if permission.resource.trim().is_empty() {
                return Err(ManifestError::UnknownPermissionAction {
                    index,
                    found: permission.action.clone(),
                });
            }
            if !matches!(
                permission.action.as_str(),
                "read" | "write" | "execute" | "outbound" | "inbound"
            ) {
                return Err(ManifestError::UnknownPermissionAction {
                    index,
                    found: permission.action.clone(),
                });
            }
        }

        parse_sha256(&self.artifact.hash)?;
        signature::PublicKey::parse(&self.publisher_key)?;

        Ok(())
    }

    /// Parsed semantic version. Infallible: [`Manifest::parse`] already checked it.
    pub fn semver(&self) -> semver::Version {
        semver::Version::parse(&self.version).expect("validated at parse time")
    }

    /// The expected artifact digest, as raw bytes.
    pub fn artifact_digest(&self) -> [u8; 32] {
        parse_sha256(&self.artifact.hash).expect("validated at parse time")
    }

    /// The publisher's public key. Infallible: validated at parse time.
    pub fn public_key(&self) -> PublicKey {
        PublicKey::parse(&self.publisher_key).expect("validated at parse time")
    }

    /// Permissions that cannot be granted without asking a human first.
    pub fn permissions_requiring_confirmation(&self) -> Vec<&Permission> {
        self.permissions
            .iter()
            .filter(|p| p.kind.requires_confirmation())
            .collect()
    }

    /// Verify a detached signature over the canonical form of this manifest.
    pub fn verify_signature(&self, signature: &Signature) -> Result<(), ManifestError> {
        self.public_key()
            .verify(&canonical_bytes(self), signature)
            .map_err(ManifestError::from)
    }
}

/// A module id must be reverse-DNS: at least three dot-separated segments, each
/// starting with a lowercase letter and containing only lowercase alphanumerics,
/// hyphens and underscores.
///
/// The id is immutable for the life of the module and is what a publisher key is
/// pinned against, so it must not be ambiguous.
fn is_valid_module_id(id: &str) -> bool {
    let segments: Vec<&str> = id.split('.').collect();
    if segments.len() < 3 {
        return false;
    }
    segments.iter().all(|segment| {
        !segment.is_empty()
            && segment.starts_with(|c: char| c.is_ascii_lowercase())
            && segment
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    })
}

fn parse_sha256(value: &str) -> Result<[u8; 32], ManifestError> {
    let hex_part = value
        .strip_prefix("sha256:")
        .ok_or_else(|| ManifestError::MalformedHash(value.to_string()))?;
    let bytes =
        hex::decode(hex_part).map_err(|_| ManifestError::MalformedHash(value.to_string()))?;
    bytes
        .try_into()
        .map_err(|_| ManifestError::MalformedHash(value.to_string()))
}

/// Format a digest the way manifests spell it.
pub fn format_sha256(digest: &[u8; 32]) -> String {
    format!("sha256:{}", hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
format_version = 1
id             = "org.publisher.pyassist"
name           = "PyAssist Core"
version        = "2.3.1"
description    = "Python assistance module"
license        = "GPL-3.0-or-later"
publisher_key  = "ed25519:3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29"
distribution   = "prebuilt"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 4823910

[requires]
thalyx = "^1.0"

[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"

[[permissions]]
resource = "/home/user/projects"
action   = "read"
type     = "persistent"

[entrypoints]
run = "bin/pyassist"
"#;

    #[test]
    fn parses_a_valid_manifest() {
        let m = Manifest::parse(VALID).expect("valid manifest");
        assert_eq!(m.id, "org.publisher.pyassist");
        assert_eq!(m.semver(), semver::Version::new(2, 3, 1));
        assert_eq!(m.permissions.len(), 2);
        assert_eq!(m.distribution, Distribution::Prebuilt);
    }

    #[test]
    fn rejects_future_format_versions() {
        let src = VALID.replace("format_version = 1", "format_version = 2");
        assert!(matches!(
            Manifest::parse(&src),
            Err(ManifestError::UnsupportedFormatVersion { found: 2, .. })
        ));
    }

    #[test]
    fn rejects_ids_that_are_not_reverse_dns() {
        for bad in [
            "pyassist",
            "org.pyassist",
            "Org.Publisher.Pyassist",
            "org..x",
        ] {
            let src = VALID.replace("org.publisher.pyassist", bad);
            assert!(
                matches!(Manifest::parse(&src), Err(ManifestError::InvalidId(_))),
                "expected `{bad}` to be rejected"
            );
        }
    }

    #[test]
    fn rejects_inter_module_dependencies() {
        let src = VALID.replace(
            "[requires]\nthalyx = \"^1.0\"",
            "[requires]\nthalyx = \"^1.0\"\n\n[requires.modules]\n\"org.other.thing\" = \"^1.0\"",
        );
        assert!(matches!(
            Manifest::parse(&src),
            Err(ManifestError::InterModuleDependency(_))
        ));
    }

    #[test]
    fn rejects_unknown_permission_types() {
        let src = VALID.replace(r#"type     = "persistent""#, r#"type     = "forever""#);
        assert!(Manifest::parse(&src).is_err());
    }

    #[test]
    fn rejects_malformed_hashes() {
        for bad in ["deadbeef", "sha256:zz", "sha1:0000"] {
            let src = VALID.replace(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                bad,
            );
            assert!(
                matches!(Manifest::parse(&src), Err(ManifestError::MalformedHash(_))),
                "expected `{bad}` to be rejected"
            );
        }
    }

    #[test]
    fn persistent_permissions_always_require_confirmation() {
        let m = Manifest::parse(VALID).unwrap();
        assert_eq!(m.permissions_requiring_confirmation().len(), 2);
        assert!(PermissionKind::Persistent.requires_confirmation());
        assert!(!PermissionKind::Jit.requires_confirmation());
        assert!(!PermissionKind::Session.requires_confirmation());
    }
}
