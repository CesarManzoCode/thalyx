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

/// What a manifest is allowed to weigh.
///
/// It is TOML describing a module. The largest one this project has produced is
/// under two kilobytes, so this is three orders of magnitude of headroom and
/// still small enough that reading it costs nothing.
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// What a detached ed25519 signature is allowed to weigh.
const MAX_SIGNATURE_BYTES: u64 = 8 * 1024;

/// What a compressed artifact is allowed to weigh.
///
/// A Phase 1 module larger than this is outside what the phase is for, and the
/// number is a decree that can be raised when a real module needs it. What it
/// must not be is absent: see the note on [`Bundle::read`].
const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;

/// How many members a bundle may hold before it stops being a bundle.
const MAX_BUNDLE_MEMBERS: usize = 64;

/// How much larger than its compressed self an artifact may unpack.
///
/// Fifty is generous for anything real: source text gzips around 4:1, and even
/// a tree of near-identical files rarely passes twenty. It is the ratio and not
/// an absolute number because the absolute number would have to be large enough
/// for the biggest legitimate module, which makes it useless against the
/// smallest bomb.
const MAX_EXPANSION_RATIO: u64 = 50;

/// The smallest budget any artifact gets, however tiny it is compressed.
const MIN_EXPANSION_BUDGET: u64 = 32 * 1024 * 1024;

/// The ceiling no artifact passes, whatever its ratio works out to.
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// How many files an artifact may hold.
const MAX_ARTIFACT_ENTRIES: usize = 100_000;

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
    /// Read a bundle off disk. **Nothing here has been verified yet.**
    ///
    /// This is the first thing that touches an attacker-supplied file and it
    /// runs before the signature is checked, before the digest is recomputed,
    /// and before any key is consulted — it has to, because those checks need
    /// the bytes this produces. So every bound in this function is a bound that
    /// applies to a stranger's file.
    ///
    /// It used to have none. A 768 MB `.thmod` with no signature at all drove
    /// the peak RSS of the process to a gigabyte, because each member was read
    /// whole into memory *before* the match that decides whether the member is
    /// even one of the three that matter — so padding the archive with a member
    /// Thalyx ignores was enough. A machine that can be pushed out of memory by
    /// a file it was about to reject has no signature check worth the name in
    /// front of it.
    pub fn read(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
        let mut archive = tar::Archive::new(file);

        let mut manifest_src = None;
        let mut signature_src = None;
        let mut artifact = None;
        let mut seen = 0usize;

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

            seen += 1;
            if seen > MAX_BUNDLE_MEMBERS {
                return Err(CoreError::MalformedBundle(format!(
                    "more than {MAX_BUNDLE_MEMBERS} members"
                )));
            }

            let allowed = match name.as_str() {
                MANIFEST_MEMBER => MAX_MANIFEST_BYTES,
                SIGNATURE_MEMBER => MAX_SIGNATURE_BYTES,
                ARTIFACT_MEMBER => MAX_ARTIFACT_BYTES,
                // Unknown members are skipped without being read. Reading one
                // to ignore it is doing an attacker's work for them.
                _ => continue,
            };

            // The header's size is a claim, so it is checked *and* the read is
            // capped. Believing the header would let a small declared size hide
            // a large body; capping alone would silently truncate rather than
            // refuse. Both, and they disagree only on a malformed archive.
            let declared = entry.header().size().unwrap_or(u64::MAX);
            if declared > allowed {
                return Err(CoreError::BundleMemberTooLarge {
                    member: name,
                    found: declared,
                    allowed,
                });
            }

            let mut buffer = Vec::new();
            let read = (&mut entry)
                .take(allowed.saturating_add(1))
                .read_to_end(&mut buffer)
                .map_err(|e| CoreError::MalformedBundle(e.to_string()))?
                as u64;
            if read > allowed {
                return Err(CoreError::BundleMemberTooLarge {
                    member: name,
                    found: read,
                    allowed,
                });
            }

            // A second copy of a member is refused, never preferred.
            //
            // `tar` permits repeated names and every reader picks a different
            // one: this loop took the last, `tar -x` writes the last to disk,
            // and a reader that stopped at the first would take the first. So
            // a bundle carrying two `manifest.toml` members is one that
            // *means different things to different tools* — the signature
            // Thalyx checks would cover one of them while a person inspecting
            // the file by hand reads the other.
            //
            // Nothing legitimate produces one, so there is no cost to refusing
            // and no way to be wrong about which copy was meant.
            let already = match name.as_str() {
                MANIFEST_MEMBER => manifest_src.replace(buffer),
                SIGNATURE_MEMBER => signature_src.replace(buffer),
                ARTIFACT_MEMBER => artifact.replace(buffer),
                _ => unreachable!("every other name was skipped above"),
            };
            if already.is_some() {
                return Err(CoreError::MalformedBundle(format!(
                    "`{name}` appears more than once; a bundle that means different \
                     things to different readers is refused rather than resolved"
                )));
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

    // What the artifact is allowed to become.
    //
    // Measured before this existed: 510 KB of `artifact.tar.gz` wrote 512 MB to
    // disk in under four seconds, and would have kept going. Everything here is
    // signed and its digest matched, so this is not an unauthenticated attack —
    // it is a module shipping far more than it declared, and the ratio is the
    // only thing that can tell the two apart, because the *compressed* size is
    // pinned twice over and says nothing about the expanded one.
    //
    // The floor exists so a small module is not squeezed by its own smallness:
    // a 4 KB artifact that unpacks to 2 MB of text is ordinary, not an attack.
    let budget = (artifact.len() as u64)
        .saturating_mul(MAX_EXPANSION_RATIO)
        .clamp(MIN_EXPANSION_BUDGET, MAX_UNPACKED_BYTES);
    let mut spent: u64 = 0;
    let mut entries = 0usize;

    for entry in archive
        .entries()
        .map_err(|e| CoreError::MalformedBundle(e.to_string()))?
    {
        entries += 1;
        if entries > MAX_ARTIFACT_ENTRIES {
            return Err(CoreError::ArtifactTooManyEntries {
                allowed: MAX_ARTIFACT_ENTRIES,
            });
        }
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
        // Capped at what is left of the budget rather than checked afterwards:
        // a check after the fact still writes the bytes it then complains
        // about, which on a full disk is the whole of the damage.
        let remaining = budget - spent;
        let copied = std::io::copy(
            &mut (&mut entry).take(remaining.saturating_add(1)),
            &mut out,
        )
        .map_err(|e| CoreError::io(&target, e))?;
        if copied > remaining {
            return Err(CoreError::ArtifactExpandsTooFar {
                allowed: budget,
                compressed: artifact.len() as u64,
            });
        }
        spent += copied;

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

    /// Write a `.thmod` by hand, so a member can be any size and any name.
    #[test]
    fn a_bundle_carrying_two_manifests_is_refused_rather_than_resolved() {
        // `tar` allows repeated names and no two tools agree on which one
        // wins: this reader took the last, `tar -x` writes the last to disk, a
        // reader that stopped early would take the first. So a bundle like
        // this means one thing to Thalyx's signature check and another to
        // whoever inspects the file by hand — which is the whole substance of
        // the attack, and the reason resolving it either way is wrong.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("two-manifests.thmod");

        thmod_with(
            &path,
            &[
                (MANIFEST_MEMBER, b"format_version = 1"),
                (SIGNATURE_MEMBER, b"ed25519:00"),
                (ARTIFACT_MEMBER, b"not really a tarball"),
                (MANIFEST_MEMBER, b"format_version = 1 # the other one"),
            ],
        );

        match Bundle::read(&path) {
            Err(CoreError::MalformedBundle(message)) => {
                assert!(
                    message.contains(MANIFEST_MEMBER),
                    "the refusal should name the duplicated member: {message}"
                );
            }
            Err(other) => panic!("expected a refusal for the duplicate, got {other:?}"),
            Ok(_) => panic!("a bundle with two manifests was accepted"),
        }
    }

    fn thmod_with(path: &Path, members: &[(&str, &[u8])]) {
        let mut builder = tar::Builder::new(std::fs::File::create(path).unwrap());
        for (name, contents) in members {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *contents).unwrap();
        }
        builder.finish().unwrap();
    }

    /// Peak resident memory of this process, in bytes.
    fn peak_rss() -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("VmHWM"))
            .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
            .map(|kb| kb * 1024)
            .unwrap_or(0)
    }

    #[test]
    fn a_member_thalyx_ignores_is_never_read_into_memory() {
        // The bug this is here for: every member was read whole *before* the
        // match that decides whether it is one of the three that matter, so
        // padding a bundle with a member Thalyx ignores drove peak RSS to a
        // gigabyte on a 768 MB file — with no signature on it, and before any
        // key was consulted.
        //
        // On the direction of the measurement, per rule 7: `VmHWM` is a high
        // water mark for the whole process and other tests share it, so noise
        // can only push it *up*. That makes this test unable to produce a false
        // failure, at the cost of being able to pass without proving anything
        // if some other test has already allocated more. It fails reliably when
        // run alone, which is what a regression test for this is worth.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("padded.thmod");
        let padding = vec![0u8; 192 * 1024 * 1024];
        thmod_with(&path, &[("padding.bin", &padding)]);
        drop(padding);

        let before = peak_rss();
        let outcome = Bundle::read(&path);
        let grew = peak_rss().saturating_sub(before);

        assert!(
            matches!(outcome, Err(CoreError::MalformedBundle(_))),
            "it should still be rejected for having no manifest"
        );
        assert!(
            grew < 64 * 1024 * 1024,
            "reading a 192 MB member Thalyx ignores grew peak memory by {} MB",
            grew / 1024 / 1024
        );
    }

    #[test]
    fn a_manifest_too_large_to_be_a_manifest_is_refused_by_size_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fat.thmod");
        let fat = vec![b'#'; (MAX_MANIFEST_BYTES + 1) as usize];
        thmod_with(&path, &[(MANIFEST_MEMBER, &fat)]);

        match Bundle::read(&path) {
            Err(CoreError::BundleMemberTooLarge {
                member,
                found,
                allowed,
            }) => {
                assert_eq!(member, MANIFEST_MEMBER);
                assert_eq!(allowed, MAX_MANIFEST_BYTES);
                assert!(found > allowed);
            }
            Err(other) => panic!("expected a size refusal before any parsing, got {other:?}"),
            Ok(_) => panic!("a manifest past every bound was accepted"),
        }
    }

    #[test]
    fn a_bundle_of_nothing_but_members_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("many.thmod");
        let names: Vec<String> = (0..MAX_BUNDLE_MEMBERS + 5)
            .map(|i| format!("filler-{i}.bin"))
            .collect();
        let members: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        thmod_with(&path, &members);

        assert!(
            matches!(Bundle::read(&path), Err(CoreError::MalformedBundle(_))),
            "a bundle has three members; thousands of them is not a bundle"
        );
    }

    #[test]
    fn an_artifact_that_expands_far_past_itself_stops_being_unpacked() {
        // Measured before the bound existed: 510 KB wrote 512 MB in under four
        // seconds and would have kept going.
        let bomb = tar_gz_with(&[("payload.bin", &vec![0u8; 128 * 1024 * 1024])]);
        let dir = tempfile::tempdir().unwrap();

        let outcome = unpack_artifact(&bomb, dir.path());
        assert!(
            matches!(outcome, Err(CoreError::ArtifactExpandsTooFar { .. })),
            "expected the expansion bound to stop it, got {outcome:?}"
        );

        // And it stopped *while* writing rather than after: the file on disk is
        // the budget, not the bomb. A check that ran afterwards would have
        // written every byte it then complained about.
        let written: u64 = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok()?.metadata().ok())
            .map(|m| m.len())
            .sum();
        assert!(
            written < 64 * 1024 * 1024,
            "{} MB reached the disk before it was refused",
            written / 1024 / 1024
        );
    }

    #[test]
    fn an_ordinary_small_module_is_not_squeezed_by_the_bound() {
        // The control. Without it, a bound that refused everything would look
        // exactly like one that works: a few kilobytes of source expanding to a
        // couple of megabytes is a normal module, not a bomb.
        let text = "fn main() { println!(\"hello\"); }\n".repeat(20_000);
        let ordinary = tar_gz_with(&[("src/main.rs", text.as_bytes())]);
        let dir = tempfile::tempdir().unwrap();

        assert!(
            ordinary.len() < 64 * 1024,
            "the fixture has to be small compressed for this to mean anything"
        );
        assert!(
            unpack_artifact(&ordinary, dir.path()).is_ok(),
            "{} KB expanding to {} KB must stay allowed",
            ordinary.len() / 1024,
            text.len() / 1024
        );
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
