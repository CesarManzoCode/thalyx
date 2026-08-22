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

    /// Build the machine's root filesystem, or count what is in one
    ///
    /// The image is the Linux kernel and one program. This is what makes the
    /// second half of that, and what checks it.
    Image {
        /// The statically linked `thalyx` to put in it
        #[arg(long)]
        binary: Option<PathBuf>,
        /// Where to write the archive
        #[arg(long)]
        out: Option<PathBuf>,
        /// Read an archive back and list what is really in it
        #[arg(long)]
        list: Option<PathBuf>,
    },

    /// Drive the agent with a model that misbehaves on purpose.
    ///
    /// This exists because of rule 4. Until `llama.cpp` is wired in there is no
    /// model at all, so every attempt to reach the system through inference
    /// fails with "no model is configured" — and a denial that happens because
    /// nothing was there to ask looks exactly like a denial by the provenance
    /// check, while proving nothing about it. This supplies a model that really
    /// does what a hostile page told it to, so the refusal has to come from
    /// somewhere real.
    AgentProbe {
        /// What the human typed
        utterance: String,
        /// Text Thalyx did not get from the human
        #[arg(long)]
        foreign: Vec<String>,
        /// How the stand-in model misbehaves
        #[arg(long, default_value = "obeys-foreign-text")]
        behaviour: String,
    },

    /// Run a command with a terminal of its own, feeding it this process's stdin
    ///
    /// The session prompt refuses to confirm anything when stdin is not a
    /// terminal, because silence is not consent. So anything that drives it —
    /// a test, `dev/verify.sh` — has to supply a real one.
    ///
    /// That used to be `script(1)`, and the dependency cost more than it looked
    /// like it would: Fedora ships `script` in `util-linux-script`, which is not
    /// installed by default, so on the one machine that can actually verify
    /// Thalyx the stage covering four of the six exit-criterion steps skipped
    /// itself entirely and said NOT PROVEN. The criterion that ends Phase 1 went
    /// unchecked because of a subpackage.
    ///
    /// Rule 5: the instrument includes the harness. Thalyx writes its own
    /// initramfs and loads its own BPF rather than inherit a tool it did not
    /// choose; this is the same decision, and it means the exit criterion can be
    /// verified on any machine that can run Thalyx at all.
    Pty {
        /// The command, then its arguments
        #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
        argv: Vec<std::ffi::OsString>,
    },
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
        DevCommand::Image { binary, out, list } => match (binary, out, list) {
            (_, _, Some(archive)) => crate::image::list(&archive),
            (Some(binary), Some(out), None) => crate::image::build(&binary, &out),
            _ => Err("give --binary and --out to build, or --list to inspect".into()),
        },
        DevCommand::AgentProbe {
            utterance,
            foreign,
            behaviour,
        } => agent_probe(&utterance, &foreign, &behaviour),
        DevCommand::Pty { argv } => pty(&argv),
    }
}

/// Run a command on a pseudoterminal, and get out of the way.
///
/// Two directions, and both have to happen at once: what arrives on this
/// process's stdin goes to the child's terminal, and what the child writes
/// comes back out on this process's stdout. A copy that did one and then the
/// other would deadlock against a program that speaks before it is spoken to —
/// which is exactly what a prompt is.
///
/// The exit status is passed through, including the signal case: a child killed
/// by a signal exits `128 + n`, the shell convention, so a caller reading `$?`
/// can tell "refused" from "died".
fn pty(argv: &[std::ffi::OsString]) -> Fallible {
    use std::io::Read;
    use std::os::fd::AsFd;

    let (program, arguments) = argv.split_first().ok_or("no command to run")?;

    let terminal = thalyx_syscall::open_pty()?;

    // A pty the kernel has just made reports zero rows, and a full-screen
    // program that asks honestly refuses to draw on a screen of no size — so
    // without this the editor could not be exercised through the harness at
    // all, and the half of it that a person sees would have no test that ran it.
    //
    // 24x80 because that is what a terminal has meant since the VT100 and what
    // the image's serial console actually is. Fixed rather than copied from
    // whatever terminal happens to be running the suite: a test whose screen
    // size depends on the window somebody left open is a test that passes on one
    // machine and not the next.
    thalyx_syscall::set_terminal_size(terminal.follower.as_fd(), 24, 80)?;

    let mut command = std::process::Command::new(program);
    command.args(arguments);

    let mut child = thalyx_syscall::spawn_with_terminal(&mut command, terminal.follower.as_fd())?;

    // The follower is closed here on purpose, and it is the whole reason the
    // loop below ever ends. The child holds its own copy; while this process
    // also held one, reading the controller would block forever waiting for a
    // writer that is this process — the same shape as the module channel in
    // `thalyx_core::run`, and the same fix.
    drop(terminal.follower);

    let controller = std::sync::Arc::new(terminal.controller);

    // Feeding happens on a thread because the child may write before it reads,
    // and it may never read at all.
    let writing = {
        let controller = std::sync::Arc::clone(&controller);
        std::thread::spawn(move || {
            use std::io::Write;
            let mut input = Vec::new();
            if std::io::stdin().read_to_end(&mut input).is_err() {
                return;
            }
            let mut sink = std::fs::File::from(
                controller
                    .try_clone()
                    .expect("a descriptor that is open can be cloned"),
            );
            let _ = sink.write_all(&input);
            let _ = sink.flush();
        })
    };

    // And this side reads until the child's terminal has no writers left.
    //
    // `EIO` is the *normal* end of a pty read, not a failure: it is what the
    // kernel returns once the last process holding the follower is gone.
    // Reporting it as an error would make every successful run look broken.
    let mut source = std::fs::File::from(
        controller
            .try_clone()
            .expect("a descriptor that is open can be cloned"),
    );
    let mut buffer = [0u8; 4096];
    loop {
        match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                use std::io::Write;
                std::io::stdout().write_all(&buffer[..count])?;
                std::io::stdout().flush()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.raw_os_error() == Some(5) => break,
            Err(error) => return Err(error.into()),
        }
    }

    let status = child.wait()?;
    let _ = writing.join();

    let code = {
        use std::os::unix::process::ExitStatusExt;
        match (status.code(), status.signal()) {
            (Some(code), _) => code,
            (None, Some(signal)) => 128 + signal,
            (None, None) => 1,
        }
    };

    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

fn agent_probe(utterance: &str, foreign: &[String], behaviour: &str) -> Fallible {
    use thalyx_agent::{ForeignText, HostileModel, Misbehaviour, Segment, Transcript};

    let misbehaviour = match behaviour {
        "faithful" => Misbehaviour::Faithful,
        "garbage" => Misbehaviour::Garbage,
        "wrong-shape" => Misbehaviour::WrongShape,
        "writes-provenance" => Misbehaviour::WritesProvenance,
        "hallucinates" => Misbehaviour::Hallucinates,
        "obeys-foreign-text" => Misbehaviour::ObeysForeignText,
        "silence" => Misbehaviour::Silence,
        "never-stops" => Misbehaviour::NeverStops,
        "fails" => Misbehaviour::Fails,
        other => return Err(format!("unknown behaviour `{other}`").into()),
    };

    let mut transcript = Transcript::new().with(Segment::typed(utterance));
    for text in foreign {
        transcript = transcript.with(Segment::foreign(text));
    }

    let caller = thalyx_contract::Caller {
        module_id: "thalyx-dev-probe".to_string(),
        request_id: "probe".to_string(),
    };

    match thalyx_agent::plan(
        &transcript,
        &HostileModel::new(misbehaviour),
        ForeignText::NeverActs,
        caller,
    ) {
        Ok(plan) => {
            println!("A CONTRACT WAS PRODUCED — the model got through.");
            println!("{}", plan.contract.to_json());
            Err("the probe produced a contract".into())
        }
        Err(error) => {
            println!("refused: {error}");
            Ok(())
        }
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
