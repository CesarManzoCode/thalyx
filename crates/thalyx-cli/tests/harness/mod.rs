//! Shared test fixture: a throwaway store, a signed bundle, and a way to run
//! the real `thalyx` binary against them.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use thalyx_core::Store;
use thalyx_core::fault::{FAULT_MODE_ENV, FAULT_POINT_ENV, FaultPoint};
use thalyx_core::permissions::{Grant, Registry};

pub struct Fixture {
    directory: tempfile::TempDir,
    key_path: PathBuf,
    bundle_path: PathBuf,
}

pub struct RunStatus(Output);

impl RunStatus {
    /// Whether the process died by `SIGABRT`, which is how an injected fault
    /// kills it. Anything else means the fault did not fire as intended.
    pub fn aborted(&self) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            self.0.status.signal() == Some(6)
        }
        #[cfg(not(unix))]
        {
            !self.0.status.success()
        }
    }

    pub fn success(&self) -> bool {
        self.0.status.success()
    }

    pub fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.0.stdout).into_owned()
    }

    pub fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.0.stderr).into_owned()
    }
}

impl Fixture {
    pub const MODULE_ID: &'static str = "org.thalyx.demo";
    pub const VERSION: &'static str = "1.0.0";

    pub fn new() -> Self {
        let directory = tempfile::tempdir().expect("temp dir");
        let base = directory.path().to_path_buf();
        let base = base.as_path();

        std::fs::create_dir_all(base.join("store")).unwrap();
        std::fs::create_dir_all(base.join("payload/bin")).unwrap();
        std::fs::write(base.join("payload/bin/demo"), "#!/bin/sh\necho demo\n").unwrap();
        std::fs::write(base.join("payload/README"), "demo module\n").unwrap();

        let key_path = base.join("publisher.key");
        let status = Command::new(binary())
            .args(["dev", "keygen", "--out"])
            .arg(&key_path)
            .output()
            .expect("keygen");
        assert!(status.status.success(), "keygen failed");

        let fixture = Self {
            directory,
            key_path,
            bundle_path: base.join("demo-1.0.0.thmod"),
        };
        let built = fixture.build_bundle(Self::VERSION);
        std::fs::rename(built, &fixture.bundle_path).unwrap();
        fixture
    }

    pub fn base(&self) -> &Path {
        self.directory.path()
    }

    pub fn root(&self) -> PathBuf {
        self.base().join("store")
    }

    pub fn store(&self) -> Store {
        Store::open(self.root()).expect("store")
    }

    pub fn bundle(&self) -> &Path {
        &self.bundle_path
    }

    /// Permissions actually in force, which is what the invariant checks.
    pub fn effective_permissions(&self) -> Vec<Grant> {
        let store = self.store();
        let registry = Registry::load(store.permissions_path()).expect("registry");
        thalyx_core::effective_permissions(&store, &registry, Self::MODULE_ID)
    }

    /// Whatever the permissions file records, in force or not.
    pub fn recorded_permissions(&self) -> usize {
        let store = self.store();
        let registry = Registry::load(store.permissions_path()).expect("registry");
        registry.effective(Self::MODULE_ID).len()
    }

    /// Pack a signed bundle at the given version.
    pub fn build_bundle(&self, version: &str) -> PathBuf {
        let manifest_path = self.base().join(format!("manifest-{version}.toml"));
        std::fs::write(
            &manifest_path,
            format!(
                r#"
format_version = 1
id             = "{id}"
name           = "Thalyx Demo"
version        = "{version}"
description    = "A module that exists so the install path can be exercised"
license        = "GPL-3.0-or-later"
publisher_key  = "ed25519:0000000000000000000000000000000000000000000000000000000000000000"
distribution   = "prebuilt"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 0

[requires]
thalyx = ">=0.1.0"

[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"

[[permissions]]
resource = "/home/user/projects"
action   = "read"
type     = "persistent"

[entrypoints]
run = "bin/demo"
"#,
                id = Self::MODULE_ID,
            ),
        )
        .unwrap();

        let out = self.base().join(format!("demo-{version}.thmod"));
        let result = Command::new(binary())
            .args(["dev", "pack"])
            .arg(self.base().join("payload"))
            .arg("--manifest")
            .arg(&manifest_path)
            .arg("--key")
            .arg(&self.key_path)
            .arg("--out")
            .arg(&out)
            .output()
            .expect("pack");
        assert!(
            result.status.success(),
            "pack failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        out
    }

    pub fn install(&self) -> RunStatus {
        self.install_bundle(&self.bundle_path.clone(), None)
    }

    pub fn install_with_fault(&self, point: FaultPoint) -> RunStatus {
        self.install_bundle(&self.bundle_path.clone(), Some(point))
    }

    pub fn install_bundle_with_fault(&self, bundle: &Path, point: FaultPoint) -> RunStatus {
        self.install_bundle(bundle, Some(point))
    }

    pub fn install_bundle_at(&self, bundle: &Path) -> RunStatus {
        self.install_bundle(bundle, None)
    }

    fn install_bundle(&self, bundle: &Path, fault: Option<FaultPoint>) -> RunStatus {
        let mut command = Command::new(binary());
        command
            .args(["--root"])
            .arg(self.root())
            .args(["module", "install"])
            .arg(bundle)
            .arg("--yes");

        if let Some(point) = fault {
            command
                .env(FAULT_POINT_ENV, point.to_string())
                .env(FAULT_MODE_ENV, "abort");
        }

        RunStatus(command.output().expect("install"))
    }

    /// A bundle whose payload was altered after it was signed.
    ///
    /// The manifest and its signature stay valid, which is the point: only
    /// recomputing the artifact digest catches this.
    pub fn tamper_with_artifact(&self) -> PathBuf {
        let out = self.base().join("tampered.thmod");
        rewrite_bundle(&self.bundle_path, &out, |name, contents| {
            if name == "artifact.tar.gz" {
                let mut altered = contents.to_vec();
                altered.extend_from_slice(b"extra bytes appended after signing");
                altered
            } else {
                contents.to_vec()
            }
        });
        out
    }

    /// A bundle whose manifest names one publisher but is signed by another.
    pub fn bundle_signed_by_a_different_key(&self) -> PathBuf {
        let other_key = self.base().join("impostor.key");
        let ok = Command::new(binary())
            .args(["dev", "keygen", "--out"])
            .arg(&other_key)
            .output()
            .expect("keygen");
        assert!(ok.status.success());

        // Pack with the impostor key, then splice in the original manifest so
        // that publisher_key and the signature disagree.
        let impostor_bundle = self.pack_with_key(&other_key, "1.0.0", Self::MODULE_ID);
        let original_manifest = read_member(&self.bundle_path, "manifest.toml");

        let out = self.base().join("forged.thmod");
        rewrite_bundle(&impostor_bundle, &out, |name, contents| {
            if name == "manifest.toml" {
                original_manifest.clone()
            } else {
                contents.to_vec()
            }
        });
        out
    }

    /// A well-formed bundle for the same module id, signed by a key the store
    /// has never seen. Exactly what publisher impersonation looks like.
    pub fn bundle_from_a_new_publisher(&self, version: &str) -> PathBuf {
        let other_key = self.base().join("newpublisher.key");
        let ok = Command::new(binary())
            .args(["dev", "keygen", "--out"])
            .arg(&other_key)
            .output()
            .expect("keygen");
        assert!(ok.status.success());
        self.pack_with_key(&other_key, version, Self::MODULE_ID)
    }

    fn pack_with_key(&self, key: &Path, version: &str, id: &str) -> PathBuf {
        let manifest_path = self.base().join(format!("manifest-alt-{version}.toml"));
        std::fs::write(&manifest_path, manifest_source(id, version)).unwrap();

        let out = self.base().join(format!("alt-{version}.thmod"));
        let result = Command::new(binary())
            .args(["dev", "pack"])
            .arg(self.base().join("payload"))
            .arg("--manifest")
            .arg(&manifest_path)
            .arg("--key")
            .arg(key)
            .arg("--out")
            .arg(&out)
            .output()
            .expect("pack");
        assert!(
            result.status.success(),
            "pack failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        out
    }

    pub fn run(&self, args: &[&str]) -> RunStatus {
        let mut command = Command::new(binary());
        command.args(["--root"]).arg(self.root()).args(args);
        RunStatus(command.output().expect("command"))
    }
}

fn manifest_source(id: &str, version: &str) -> String {
    format!(
        r#"
format_version = 1
id             = "{id}"
name           = "Thalyx Demo"
version        = "{version}"
description    = "A module that exists so the install path can be exercised"
license        = "GPL-3.0-or-later"
publisher_key  = "ed25519:0000000000000000000000000000000000000000000000000000000000000000"
distribution   = "prebuilt"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 0

[requires]
thalyx = ">=0.1.0"

[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"

[[permissions]]
resource = "/home/user/projects"
action   = "read"
type     = "persistent"

[entrypoints]
run = "bin/demo"
"#
    )
}

/// Read one member out of a bundle.
fn read_member(bundle: &Path, member: &str) -> Vec<u8> {
    use std::io::Read;
    let file = std::fs::File::open(bundle).expect("bundle");
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries().expect("entries") {
        let mut entry = entry.expect("entry");
        let name = entry.path().expect("path").to_string_lossy().into_owned();
        if name == member {
            let mut buffer = Vec::new();
            entry.read_to_end(&mut buffer).expect("read");
            return buffer;
        }
    }
    panic!("member {member} not found in {}", bundle.display());
}

/// Rebuild a bundle, passing each member through `transform`.
///
/// Used to forge bundles that violate exactly one property, so a rejection
/// test proves the check it names and not some unrelated malformation.
fn rewrite_bundle(source: &Path, destination: &Path, transform: impl Fn(&str, &[u8]) -> Vec<u8>) {
    use std::io::Read;

    let file = std::fs::File::open(source).expect("bundle");
    let mut archive = tar::Archive::new(file);
    let mut members: Vec<(String, Vec<u8>)> = Vec::new();

    for entry in archive.entries().expect("entries") {
        let mut entry = entry.expect("entry");
        let name = entry.path().expect("path").to_string_lossy().into_owned();
        let mut buffer = Vec::new();
        entry.read_to_end(&mut buffer).expect("read");
        let transformed = transform(&name, &buffer);
        members.push((name, transformed));
    }

    let out = std::fs::File::create(destination).expect("create");
    let mut builder = tar::Builder::new(out);
    for (name, contents) in members {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        builder
            .append_data(&mut header, &name, contents.as_slice())
            .expect("append");
    }
    builder.finish().expect("finish");
}

/// The `thalyx` binary built alongside these tests.
fn binary() -> PathBuf {
    // env!("CARGO_BIN_EXE_thalyx") points at the binary cargo built for this
    // test run, so the tests always exercise the current code.
    PathBuf::from(env!("CARGO_BIN_EXE_thalyx"))
}
