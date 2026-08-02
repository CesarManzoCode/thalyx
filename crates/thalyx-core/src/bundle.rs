//! Reading and safely unpacking `.thmod` bundles.
//!
//! A `.thmod` file is a tar archive containing exactly three members:
//!
//! ```text
//! manifest.toml      the manifest
//! manifest.sig       detached ed25519 signature over its canonical form
//! artifact.tar.gz    the payload; this is what `artifact.hash` covers
//! ```
//!
//! Installation never executes module code. It unpacks and validates structure,
//! and that is the whole of it — see
//! `vault/04-Flujo-Canonico/Verificacion-y-Distribucion.md`.

use crate::{CoreError, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Component, Path};
use thalyx_manifest::{Manifest, Signature};

pub const MANIFEST_MEMBER: &str = "manifest.toml";
pub const SIGNATURE_MEMBER: &str = "manifest.sig";
pub const ARTIFACT_MEMBER: &str = "artifact.tar.gz";

/// Directory inside an installed module reserved for Thalyx's own records.
///
/// The manifest is kept there, alongside the files the module shipped. An
/// artifact that tried to write into it could replace the record of what the
/// module was allowed to do with one of its own choosing, so entries under this
/// name are refused rather than overwritten.
pub const RESERVED_DIR: &str = ".thalyx";

/// A bundle read into memory, not yet trusted in any way.
pub struct Bundle {
    pub manifest: Manifest,
    pub signature: Signature,
    pub artifact: Vec<u8>,
    /// The manifest exactly as it arrived.
    ///
    /// Kept verbatim rather than re-serialised from [`Bundle::manifest`], so
    /// that the stored copy is still the bytes the signature covers. A
    /// round-trip through a serialiser would produce something equivalent to a
    /// reader and unverifiable to a verifier.
    pub manifest_source: String,
    /// The detached signature, likewise verbatim.
    pub signature_source: String,
}

impl Bundle {
    pub fn read(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
        let mut archive = tar::Archive::new(file);

        let mut manifest_src = None;
        let mut signature_src = None;
        let mut artifact = None;

        for entry in archive
            .entries()
            .map_err(|e| CoreError::MalformedBundle(e.to_string()))?
        {
            let mut entry = entry.map_err(|e| CoreError::MalformedBundle(e.to_string()))?;
            let name = entry
                .path()
                .map_err(|e| CoreError::MalformedBundle(e.to_string()))?
                .to_string_lossy()
                .into_owned();

            let mut buffer = Vec::new();
            entry
                .read_to_end(&mut buffer)
                .map_err(|e| CoreError::MalformedBundle(e.to_string()))?;

            match name.as_str() {
                MANIFEST_MEMBER => manifest_src = Some(buffer),
                SIGNATURE_MEMBER => signature_src = Some(buffer),
                ARTIFACT_MEMBER => artifact = Some(buffer),
                _ => {} // unknown members are ignored, never executed
            }
        }

        let manifest_src = manifest_src
            .ok_or_else(|| CoreError::MalformedBundle(format!("missing {MANIFEST_MEMBER}")))?;
        let signature_src = signature_src
            .ok_or_else(|| CoreError::MalformedBundle(format!("missing {SIGNATURE_MEMBER}")))?;
        let artifact = artifact
            .ok_or_else(|| CoreError::MalformedBundle(format!("missing {ARTIFACT_MEMBER}")))?;

        let manifest_src = String::from_utf8(manifest_src)
            .map_err(|_| CoreError::MalformedBundle("manifest is not UTF-8".to_string()))?;
        let manifest = Manifest::parse(&manifest_src)?;

        let signature_src = String::from_utf8(signature_src)
            .map_err(|_| CoreError::MalformedBundle("signature is not UTF-8".to_string()))?;
        let signature = Signature::parse(&signature_src)
            .map_err(|e| CoreError::MalformedBundle(e.to_string()))?;

        Ok(Self {
            manifest,
            signature,
            artifact,
            manifest_source: manifest_src,
            signature_source: signature_src,
        })
    }
}

/// Compute the digest of an artifact.
///
/// The core always calls this itself. It never accepts a digest reported by a
/// component outside the TCB, which is what made the original design's
/// "verify the hash" step unimplementable.
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Unpack an artifact into a staging directory, rejecting anything that could
/// place a file outside it.
///
/// Phase 1 accepts only regular files and directories. Symlinks, hard links and
/// device nodes are refused outright: they are the classic way an archive
/// escapes its extraction root, and no module needs them yet.
pub fn unpack_artifact(artifact: &[u8], destination: &Path) -> Result<Vec<String>> {
    let decoder = flate2::read::GzDecoder::new(artifact);
    let mut archive = tar::Archive::new(decoder);
    let mut written = Vec::new();

    for entry in archive
        .entries()
        .map_err(|e| CoreError::MalformedBundle(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| CoreError::MalformedBundle(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| CoreError::MalformedBundle(e.to_string()))?
            .into_owned();
        let display = path.to_string_lossy().into_owned();

        check_contained(&path)?;

        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(CoreError::UnsafeArchiveEntry {
                path: display,
                kind: "link".to_string(),
            });
        }
        if !kind.is_file() && !kind.is_dir() {
            return Err(CoreError::UnsafeArchiveEntry {
                path: display,
                kind: format!("{kind:?}"),
            });
        }

        let target = destination.join(&path);
        if kind.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| CoreError::io(&target, e))?;
            continue;
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| CoreError::io(&target, e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| CoreError::io(&target, e))?;

        let declared = entry.header().mode().unwrap_or(0o644);
        set_mode(&target, safe_mode(declared))?;

        written.push(display);
    }

    written.sort();
    Ok(written)
}

/// The permission bits an unpacked file is allowed to keep.
///
/// The archive's mode has to be honoured at all — without it every entrypoint
/// installs unrunnable, which is how this was found: by installing a module and
/// trying to run it.
///
/// But it is honoured through a mask, never verbatim. `0o755` drops setuid,
/// setgid and the sticky bit, and drops write for group and other. A setuid
/// binary shipped inside a module would be a privilege escalation that walks
/// straight past every permission the human was asked about — the manifest
/// would say "read access to /home/user" and the module would be root.
///
/// Owner read is forced on: a file the installer cannot read is of no use to
/// anyone and only makes the module look broken.
fn safe_mode(declared: u32) -> u32 {
    (declared & 0o755) | 0o400
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| CoreError::io(path, e))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// Reject absolute paths, `..` traversal, anything with a root or prefix, and
/// anything that would land in Thalyx's own reserved directory.
fn check_contained(path: &Path) -> Result<()> {
    let display = path.to_string_lossy().into_owned();
    if path.is_absolute() {
        return Err(CoreError::UnsafeArchivePath { path: display });
    }
    let mut normal = path
        .components()
        .filter(|c| !matches!(c, Component::CurDir));
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::UnsafeArchivePath { path: display });
            }
        }
    }
    // Staying inside the tree is not enough: the reserved directory is inside
    // it, and that is where the record of what this module may do is kept.
    if normal
        .next()
        .is_some_and(|first| first.as_os_str() == RESERVED_DIR)
    {
        return Err(CoreError::ReservedArchivePath {
            path: display,
            reserved: RESERVED_DIR.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn tar_gz_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        tar_gz_with_modes(
            &entries
                .iter()
                .map(|(n, c)| (*n, *c, 0o644))
                .collect::<Vec<_>>(),
        )
    }

    fn tar_gz_with_modes(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, contents, mode) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(*mode);
            header.set_cksum();
            builder.append_data(&mut header, name, *contents).unwrap();
        }
        gzip(&builder.into_inner().unwrap())
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    /// Build a tar archive by hand, so a hostile entry name can be used.
    ///
    /// The `tar` crate refuses to *write* `..` or absolute paths, which is
    /// helpful for well-behaved producers and useless for testing an
    /// extractor: a real attacker writes the archive by hand too.
    fn hostile_tar_gz(name: &str, contents: &[u8]) -> Vec<u8> {
        fn octal(field: &mut [u8], value: u64) {
            let text = format!("{:0width$o}", value, width = field.len() - 1);
            field[..text.len()].copy_from_slice(text.as_bytes());
            field[field.len() - 1] = 0;
        }

        let mut header = [0u8; 512];
        let name_bytes = name.as_bytes();
        header[..name_bytes.len()].copy_from_slice(name_bytes);
        octal(&mut header[100..108], 0o644); // mode
        octal(&mut header[108..116], 0); // uid
        octal(&mut header[116..124], 0); // gid
        octal(&mut header[124..136], contents.len() as u64); // size
        octal(&mut header[136..148], 0); // mtime
        header[156] = b'0'; // typeflag: regular file
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");

        header[148..156].fill(b' '); // checksum field counts as spaces
        let checksum: u64 = header.iter().map(|b| u64::from(*b)).sum();
        octal(&mut header[148..155], checksum);
        header[155] = b' ';

        let mut archive = header.to_vec();
        archive.extend_from_slice(contents);
        archive.resize(archive.len().div_ceil(512) * 512, 0);
        archive.extend_from_slice(&[0u8; 1024]); // end-of-archive marker
        gzip(&archive)
    }

    #[test]
    fn unpacks_a_normal_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = tar_gz_with(&[("bin/demo", b"#!/bin/sh\n"), ("README", b"hello")]);
        let written = unpack_artifact(&artifact, dir.path()).unwrap();
        assert_eq!(written, vec!["README".to_string(), "bin/demo".to_string()]);
        assert!(dir.path().join("bin/demo").is_file());
    }

    #[test]
    fn rejects_parent_directory_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = hostile_tar_gz("../escaped", b"owned");
        assert!(matches!(
            unpack_artifact(&artifact, dir.path()),
            Err(CoreError::UnsafeArchivePath { .. })
        ));
        assert!(!dir.path().parent().unwrap().join("escaped").exists());
    }

    #[test]
    fn rejects_deeply_nested_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = hostile_tar_gz("bin/../../../etc/cron.d/pwned", b"owned");
        assert!(matches!(
            unpack_artifact(&artifact, dir.path()),
            Err(CoreError::UnsafeArchivePath { .. })
        ));
    }

    #[test]
    fn rejects_absolute_paths() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = hostile_tar_gz("/etc/passwd", b"owned");
        assert!(matches!(
            unpack_artifact(&artifact, dir.path()),
            Err(CoreError::UnsafeArchivePath { .. })
        ));
    }

    #[test]
    fn rejects_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_mode(0o777);
        builder
            .append_link(&mut header, "link", "/etc/shadow")
            .unwrap();
        let tar = builder.into_inner().unwrap();

        use flate2::write::GzEncoder;
        use std::io::Write;
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar).unwrap();
        let artifact = encoder.finish().unwrap();

        assert!(matches!(
            unpack_artifact(&artifact, dir.path()),
            Err(CoreError::UnsafeArchiveEntry { .. })
        ));
    }

    #[test]
    fn rejects_entries_that_write_into_the_reserved_directory() {
        // Contained, well-formed, and still refused: this is where the record
        // of what the module may do is kept, so a module that could write
        // there could rewrite its own permissions and entrypoint.
        let dir = tempfile::tempdir().unwrap();
        for name in [".thalyx/manifest.toml", ".thalyx/anything", "./.thalyx/x"] {
            let artifact = tar_gz_with(&[(name, b"forged")]);
            assert!(
                matches!(
                    unpack_artifact(&artifact, dir.path()),
                    Err(CoreError::ReservedArchivePath { .. })
                ),
                "`{name}` should be refused as reserved"
            );
        }
        assert!(!dir.path().join(RESERVED_DIR).exists());
    }

    #[test]
    fn a_reserved_name_deeper_in_the_tree_is_fine() {
        // Only the top level is reserved. Refusing the name everywhere would
        // be a rule the module author cannot predict from anything visible.
        let dir = tempfile::tempdir().unwrap();
        let artifact = tar_gz_with(&[("data/.thalyx/notes", b"harmless")]);
        assert!(unpack_artifact(&artifact, dir.path()).is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn an_executable_entry_installs_executable() {
        // Found by installing a module and trying to run it: every file was
        // being created with the default mode, so no entrypoint could ever be
        // executed. Every test passed while the system could not run anything.
        let dir = tempfile::tempdir().unwrap();
        let artifact = tar_gz_with_modes(&[
            ("bin/demo", b"#!/bin/sh\n".as_slice(), 0o755),
            ("README", b"hello".as_slice(), 0o644),
        ]);
        unpack_artifact(&artifact, dir.path()).unwrap();

        assert_eq!(mode_of(&dir.path().join("bin/demo")), 0o755);
        assert_eq!(mode_of(&dir.path().join("README")), 0o644);
    }

    #[test]
    #[cfg(unix)]
    fn a_setuid_entry_installs_without_setuid() {
        // A setuid binary inside a module walks straight past every permission
        // the human was asked about: the manifest would say "read access to
        // /home/user" and the module would be root.
        let dir = tempfile::tempdir().unwrap();
        let artifact = tar_gz_with_modes(&[
            ("bin/root", b"".as_slice(), 0o4755),
            ("bin/sgid", b"".as_slice(), 0o2755),
            ("bin/sticky", b"".as_slice(), 0o1777),
        ]);
        unpack_artifact(&artifact, dir.path()).unwrap();

        assert_eq!(mode_of(&dir.path().join("bin/root")), 0o755);
        assert_eq!(mode_of(&dir.path().join("bin/sgid")), 0o755);
        assert_eq!(mode_of(&dir.path().join("bin/sticky")), 0o755);
    }

    #[test]
    fn the_mode_mask_keeps_execute_and_drops_everything_dangerous() {
        assert_eq!(safe_mode(0o755), 0o755);
        assert_eq!(safe_mode(0o644), 0o644);
        assert_eq!(safe_mode(0o4755), 0o755, "setuid must not survive");
        assert_eq!(safe_mode(0o2755), 0o755, "setgid must not survive");
        assert_eq!(safe_mode(0o1755), 0o755, "the sticky bit must not survive");
        assert_eq!(safe_mode(0o777), 0o755, "group and other write are dropped");
        assert_eq!(safe_mode(0o000), 0o400, "owner read is always kept");
    }

    #[test]
    fn digest_is_stable_and_distinguishing() {
        assert_eq!(digest(b"same"), digest(b"same"));
        assert_ne!(digest(b"same"), digest(b"different"));
    }
}
