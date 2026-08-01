//! Detached ed25519 signatures over the canonical form of a manifest.
//!
//! Signing a manifest's raw bytes would make the signature depend on whitespace
//! and key ordering, so a reformatted-but-identical manifest would fail to
//! verify. Instead we serialise the parsed manifest into a canonical byte
//! sequence and sign that.

use ed25519_dalek::{Signer, Verifier};
use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("public key must be `ed25519:<64 hex chars>`, got `{0}`")]
    MalformedKey(String),

    #[error("signature must be 128 hex chars, got `{0}`")]
    MalformedSignature(String),

    #[error("signature does not match the manifest")]
    Invalid,
}

/// An ed25519 public key, as it appears in a manifest's `publisher_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey(ed25519_dalek::VerifyingKey);

impl PublicKey {
    pub fn parse(value: &str) -> Result<Self, SignatureError> {
        let hex_part = value
            .strip_prefix("ed25519:")
            .ok_or_else(|| SignatureError::MalformedKey(value.to_string()))?;
        let bytes =
            hex::decode(hex_part).map_err(|_| SignatureError::MalformedKey(value.to_string()))?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| SignatureError::MalformedKey(value.to_string()))?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
            .map_err(|_| SignatureError::MalformedKey(value.to_string()))?;
        Ok(Self(key))
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), SignatureError> {
        self.0
            .verify(message, &signature.0)
            .map_err(|_| SignatureError::Invalid)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ed25519:{}", hex::encode(self.0.to_bytes()))
    }
}

/// A detached signature, stored alongside the manifest as `<name>.sig`.
#[derive(Debug, Clone)]
pub struct Signature(ed25519_dalek::Signature);

impl Signature {
    pub fn parse(value: &str) -> Result<Self, SignatureError> {
        let bytes = hex::decode(value.trim())
            .map_err(|_| SignatureError::MalformedSignature(value.to_string()))?;
        let bytes: [u8; 64] = bytes
            .try_into()
            .map_err(|_| SignatureError::MalformedSignature(value.to_string()))?;
        Ok(Self(ed25519_dalek::Signature::from_bytes(&bytes)))
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0.to_bytes()))
    }
}

/// A publisher's private key. Only used by the packaging tooling.
pub struct SigningKey(ed25519_dalek::SigningKey);

impl SigningKey {
    pub fn generate() -> Self {
        use rand_core::RngCore;
        let mut seed = [0u8; 32];
        rand_core::OsRng.fill_bytes(&mut seed);
        Self(ed25519_dalek::SigningKey::from_bytes(&seed))
    }

    pub fn from_hex(value: &str) -> Result<Self, SignatureError> {
        let bytes = hex::decode(value.trim())
            .map_err(|_| SignatureError::MalformedKey(value.to_string()))?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| SignatureError::MalformedKey(value.to_string()))?;
        Ok(Self(ed25519_dalek::SigningKey::from_bytes(&bytes)))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0.to_bytes())
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.verifying_key())
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        Signature(self.0.sign(message))
    }
}

/// The canonical byte sequence that a manifest signature covers.
///
/// Deliberately explicit rather than derived from a serialiser: the set of
/// signed fields is a security decision, so it should be readable at a glance
/// and should not change silently when a field is added to the struct.
///
/// Permissions are sorted before hashing so that reordering them in the TOML
/// cannot produce a different signature over the same effective grant.
pub fn canonical_bytes(manifest: &crate::Manifest) -> Vec<u8> {
    let mut out = String::new();

    out.push_str("thalyx-manifest-v1\n");
    out.push_str(&format!("format_version={}\n", manifest.format_version));
    out.push_str(&format!("id={}\n", manifest.id));
    out.push_str(&format!("name={}\n", manifest.name));
    out.push_str(&format!("version={}\n", manifest.version));
    out.push_str(&format!("license={}\n", manifest.license));
    out.push_str(&format!("publisher_key={}\n", manifest.publisher_key));
    out.push_str(&format!("distribution={:?}\n", manifest.distribution));
    out.push_str(&format!("artifact.hash={}\n", manifest.artifact.hash));
    out.push_str(&format!("artifact.size={}\n", manifest.artifact.size));
    out.push_str(&format!("requires.thalyx={}\n", manifest.requires.thalyx));

    let mut permissions: Vec<String> = manifest
        .permissions
        .iter()
        .map(|p| format!("{}|{}|{}", p.resource, p.action, p.kind))
        .collect();
    permissions.sort();
    for permission in permissions {
        out.push_str(&format!("permission={permission}\n"));
    }

    for (name, path) in &manifest.entrypoints {
        out.push_str(&format!("entrypoint.{name}={path}\n"));
    }

    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Manifest;

    fn manifest_with(permissions: &str) -> Manifest {
        let src = format!(
            r#"
format_version = 1
id             = "org.publisher.demo"
name           = "Demo"
version        = "1.0.0"
license        = "GPL-3.0-or-later"
publisher_key  = "ed25519:3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 1

{permissions}
"#
        );
        Manifest::parse(&src).expect("valid manifest")
    }

    #[test]
    fn round_trips_a_signature() {
        let key = SigningKey::generate();
        let message = b"some manifest bytes";
        let signature = key.sign(message);
        assert!(key.public_key().verify(message, &signature).is_ok());
    }

    #[test]
    fn rejects_a_signature_over_different_bytes() {
        let key = SigningKey::generate();
        let signature = key.sign(b"original");
        assert!(matches!(
            key.public_key().verify(b"tampered", &signature),
            Err(SignatureError::Invalid)
        ));
    }

    #[test]
    fn signature_survives_reordered_permissions() {
        let a = manifest_with(
            r#"
[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"

[[permissions]]
resource = "/home/user/projects"
action   = "read"
type     = "persistent"
"#,
        );
        let b = manifest_with(
            r#"
[[permissions]]
resource = "/home/user/projects"
action   = "read"
type     = "persistent"

[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"
"#,
        );
        assert_eq!(canonical_bytes(&a), canonical_bytes(&b));
    }

    #[test]
    fn canonical_form_changes_when_a_permission_changes() {
        let a = manifest_with(
            r#"
[[permissions]]
resource = "net"
action   = "outbound"
type     = "session"
"#,
        );
        let b = manifest_with(
            r#"
[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"
"#,
        );
        assert_ne!(
            canonical_bytes(&a),
            canonical_bytes(&b),
            "escalating a permission type must invalidate the signature"
        );
    }

    #[test]
    fn signature_parsing_rejects_wrong_lengths() {
        assert!(Signature::parse("abcd").is_err());
        assert!(PublicKey::parse("ed25519:abcd").is_err());
        assert!(PublicKey::parse("rsa:0000").is_err());
    }
}
