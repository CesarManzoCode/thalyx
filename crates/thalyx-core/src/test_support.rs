//! Building real bundles for tests.
//!
//! Everything here produces the genuine article: a tar holding the three
//! members, a manifest that passes full validation, a real ed25519 signature
//! over its canonical form, and a digest the core will recompute and agree
//! with. Nothing is stubbed.
//!
//! That matters more than convenience. A helper that produced a bundle the core
//! accepts through a shortcut would make every test using it a test of the
//! shortcut, and the one property most worth checking here — that a bundle
//! which does *not* hold up is refused — cannot be checked at all against a
//! fixture that was never really signed.

use std::io::Write;
use std::path::{Path, PathBuf};
use thalyx_manifest::SigningKey;

/// Write a signed `.thmod` into `dir` and return its path.
///
/// With `honest` false the signature is made with a different key than the one
/// the manifest names, which is exactly what a repository trying to publish a
/// version it cannot sign would produce.
pub fn write_bundle(dir: &Path, module_id: &str, version: &str, honest: bool) -> PathBuf {
    let key = SigningKey::generate();

    let artifact = artifact_tar_gz();
    let digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&artifact);
        let out: [u8; 32] = hasher.finalize().into();
        out
    };

    let manifest_toml = format!(
        r#"format_version = 1
id = "{module_id}"
name = "Demo"
version = "{version}"
license = "GPL-3.0-or-later"
publisher_key = "{publisher}"

[artifact]
hash = "sha256:{hash}"
size = {size}

[entrypoints]
default = "run.sh"
"#,
        publisher = key.public_key(),
        hash = hex::encode(digest),
        size = artifact.len(),
    );

    // Parsed before signing, so what gets signed is a manifest that passed
    // validation rather than a string that happens to look like one.
    let manifest = thalyx_manifest::Manifest::parse(&manifest_toml)
        .expect("the test helper must produce a valid manifest");

    let signing = if honest { key } else { SigningKey::generate() };
    let signature = signing.sign(&thalyx_manifest::canonical_bytes(&manifest));

    let path = dir.join(format!("{module_id}-{version}.thmod"));
    let mut builder = tar::Builder::new(std::fs::File::create(&path).expect("create bundle"));
    append(
        &mut builder,
        crate::bundle::MANIFEST_MEMBER,
        manifest_toml.as_bytes(),
    );
    append(
        &mut builder,
        crate::bundle::SIGNATURE_MEMBER,
        signature.to_string().as_bytes(),
    );
    append(&mut builder, crate::bundle::ARTIFACT_MEMBER, &artifact);
    builder.finish().expect("finish bundle");

    path
}

fn append<W: Write>(builder: &mut tar::Builder<W>, name: &str, contents: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, name, contents)
        .expect("append member");
}

fn artifact_tar_gz() -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let script = b"#!/bin/sh\necho demo\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(script.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    builder
        .append_data(&mut header, "run.sh", &script[..])
        .expect("append entrypoint");
    let tar = builder.into_inner().expect("finish artifact");

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar).expect("compress artifact");
    encoder.finish().expect("finish compression")
}
