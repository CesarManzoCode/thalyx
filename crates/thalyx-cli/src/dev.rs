//! Publisher tooling: generate a key, pack a signed `.thmod` bundle.
//!
//! This is the publisher's side of the trust model. It exists so the install
//! path can be exercised end to end without a repository, which is what the
//! Phase 1 exit criterion requires.

use clap::Subcommand;
use std::io::Write;
use std::path::{Path, PathBuf};
use thalyx_manifest::{Manifest, SigningKey, canonical_bytes, format_sha256};

type Fallible = Result<(), Box<dyn std::error::Error>>;

#[derive(Subcommand)]
pub enum DevCommand {
    /// Generate an ed25519 publisher key pair
    Keygen {
        /// Where to write the private key
        #[arg(long, default_value = "publisher.key")]
        out: PathBuf,
    },
    /// Pack a directory into a signed .thmod bundle
    Pack {
        /// Directory containing the module payload
        source: PathBuf,
        /// Manifest to use. The artifact hash and size are filled in here.
        #[arg(long)]
        manifest: PathBuf,
        /// Private key produced by `thalyx dev keygen`
        #[arg(long)]
        key: PathBuf,
        /// Where to write the bundle
        #[arg(long)]
        out: PathBuf,
    },
    /// Show what a bundle contains, without installing it
    Inspect { bundle: PathBuf },
}

pub fn run(command: DevCommand) -> Fallible {
    match command {
        DevCommand::Keygen { out } => keygen(&out),
        DevCommand::Pack {
            source,
            manifest,
            key,
            out,
        } => pack(&source, &manifest, &key, &out),
        DevCommand::Inspect { bundle } => inspect(&bundle),
    }
}

fn keygen(out: &Path) -> Fallible {
    let key = SigningKey::generate();
    std::fs::write(out, format!("{}\n", key.to_hex()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o600))?;
    }

    println!("private key  {}", out.display());
    println!("public key   {}", key.public_key());
    println!();
    println!("Put the public key in your manifest's `publisher_key` field.");
    println!("Thalyx pins it to the module id on first install: after that, a");
    println!("different key for the same id is refused outright.");
    Ok(())
}

fn pack(source: &Path, manifest_path: &Path, key_path: &Path, out: &Path) -> Fallible {
    let key = SigningKey::from_hex(&std::fs::read_to_string(key_path)?)?;

    // Build the payload archive first: the manifest has to describe it.
    let artifact = build_artifact(source)?;
    let digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&artifact);
        let out: [u8; 32] = hasher.finalize().into();
        out
    };

    // Fill in hash, size and publisher key, then re-parse so that what gets
    // signed is a manifest that passed full validation.
    let source_toml = std::fs::read_to_string(manifest_path)?;
    let mut document: toml::Value = toml::from_str(&source_toml)?;
    {
        let table = document
            .as_table_mut()
            .ok_or("manifest is not a TOML table")?;
        table.insert(
            "publisher_key".to_string(),
            toml::Value::String(key.public_key().to_string()),
        );
        let artifact_table = table
            .entry("artifact")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .ok_or("[artifact] is not a table")?;
        artifact_table.insert(
            "hash".to_string(),
            toml::Value::String(format_sha256(&digest)),
        );
        artifact_table.insert(
            "size".to_string(),
            toml::Value::Integer(artifact.len() as i64),
        );
    }

    let manifest_toml = toml::to_string_pretty(&document)?;
    let manifest = Manifest::parse(&manifest_toml)?;
    let signature = key.sign(&canonical_bytes(&manifest));

    let file = std::fs::File::create(out)?;
    let mut builder = tar::Builder::new(file);
    append_bytes(&mut builder, "manifest.toml", manifest_toml.as_bytes())?;
    append_bytes(
        &mut builder,
        "manifest.sig",
        signature.to_string().as_bytes(),
    )?;
    append_bytes(&mut builder, "artifact.tar.gz", &artifact)?;
    builder.finish()?;

    println!("packed {}", out.display());
    println!("  module    {} {}", manifest.id, manifest.version);
    println!(
        "  artifact  {} ({} bytes)",
        format_sha256(&digest),
        artifact.len()
    );
    println!("  signed by {}", key.public_key());
    Ok(())
}

fn inspect(bundle: &Path) -> Fallible {
    let bundle = thalyx_core::bundle::Bundle::read(bundle)?;
    let manifest = &bundle.manifest;

    println!("{} {}", manifest.id, manifest.version);
    println!("  name         {}", manifest.name);
    println!("  license      {}", manifest.license);
    println!("  distribution {:?}", manifest.distribution);
    println!("  requires     thalyx {}", manifest.requires.thalyx);
    println!("  publisher    {}", manifest.publisher_key);

    let computed = thalyx_core::bundle::digest(&bundle.artifact);
    let matches = computed == manifest.artifact_digest();
    println!(
        "  artifact     {} ({})",
        manifest.artifact.hash,
        if matches {
            "digest matches"
        } else {
            "DIGEST MISMATCH"
        }
    );

    let signature_ok = manifest.verify_signature(&bundle.signature).is_ok();
    println!(
        "  signature    {}",
        if signature_ok { "valid" } else { "INVALID" }
    );

    if manifest.permissions.is_empty() {
        println!("  permissions  none");
    } else {
        println!("  permissions");
        for permission in &manifest.permissions {
            println!("    {} ({})", permission.describe(), permission.kind);
        }
    }
    Ok(())
}

/// tar+gzip a directory tree, deterministically.
///
/// Timestamps and ownership are zeroed so that packing the same tree twice
/// yields the same bytes, and therefore the same digest and signature.
fn build_artifact(source: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    collect_files(source, source, &mut paths)?;
    paths.sort();

    let mut builder = tar::Builder::new(Vec::new());
    for relative in &paths {
        let absolute = source.join(relative);
        let contents = std::fs::read(&absolute)?;

        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(if is_executable(&absolute) {
            0o755
        } else {
            0o644
        });
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_cksum();

        builder.append_data(&mut header, relative, contents.as_slice())?;
    }
    let tar = builder.into_inner()?;

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar)?;
    Ok(encoder.finish()?)
}

fn collect_files(
    root: &Path,
    directory: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.is_symlink() {
            return Err(format!(
                "{}: symlinks are not accepted in a module payload",
                path.display()
            )
            .into());
        }
        if metadata.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            out.push(path.strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(())
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

fn append_bytes<W: Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    contents: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    header.set_cksum();
    builder.append_data(&mut header, name, contents)?;
    Ok(())
}
