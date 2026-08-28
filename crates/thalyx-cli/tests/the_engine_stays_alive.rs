//! Two sentences, one engine process, one load of the weights.
//!
//! This is the claim Cesar asked for on 2026-08-28 in the words he used: *no me
//! digas "persistent" porque existe un objeto Rust persistente mientras el
//! proceso sigue muriendo*. So what is measured here is not an abstraction. It
//! is **how many processes existed**, counted by the engine itself, in a file
//! it appends to every time it starts.
//!
//! ## Why this file runs its own `main`
//!
//! A module's entrypoint is an executable file, and the engine speaks a binary
//! protocol that no shell script can write. The stand-in therefore has to be a
//! real program — and the one program this test can be sure exists is itself.
//! So the test binary is packed as the module, and `THALYX_STANDIN_ENGINE` is
//! what tells a copy of it to be the engine instead of the test. That needs a
//! `main` of its own, which is why `Cargo.toml` gives this target
//! `harness = false`.
//!
//! ## What is measured, and what deliberately is not
//!
//! **Measured:** that the second sentence is answered by the same process as
//! the first; that the weights are therefore not reloaded; that an engine which
//! dies is started again exactly once and the sentence still lands; and that
//! both sentences reached the real dispatch and changed the machine.
//!
//! **Not measured here, and not measurable here:** the confinement. There is no
//! BPF LSM in a development container, so this runs with
//! `THALYX_ENGINE_UNCONFINED=1` and `thalyx_core::run` records every one of
//! these as degraded. Residency and confinement are established by the same
//! call — `run::start` — but only Cesar's machine can watch the second half of
//! that sentence. `dev/verify.sh` is where it is asked.
//!
//! **Nor the model.** The stand-in answers with a contract this file wrote. A
//! real Qwen2.5 answering these sentences is §46 of `verify.sh` and the QEMU
//! run in `Punto-Actual.md`.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Set on a copy of this binary to make it the engine rather than the test.
const STANDIN: &str = "THALYX_STANDIN_ENGINE";
/// Where the stand-in appends its pid, once per process. The whole measurement.
const PID_LOG: &str = "THALYX_STANDIN_PIDS";
/// What it answers with: `word=contract` pairs separated by `|`, and the pair
/// whose word appears in the prompt is the one it uses.
///
/// Keyed on the prompt rather than counted, and that is the fake modelling the
/// property rather than merely satisfying the caller — rule 8. A stand-in that
/// answered the *n*th contract to the *n*th request would answer the second
/// sentence with the first contract after a restart, because a restarted
/// process counts from zero. Which would make the restart test fail for a
/// reason that has nothing to do with restarting.
const ANSWERS: &str = "THALYX_STANDIN_ANSWERS";
/// How many answers it gives before exiting, for the restart test. Unset means
/// it never exits on its own.
const DIE_AFTER: &str = "THALYX_STANDIN_DIE_AFTER";

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

// ───────────────────────────────────────────────── the stand-in engine itself

/// The other side of `engine_module`'s protocol, in as few lines as it takes.
///
/// A fake, and it models the property under test rather than merely satisfying
/// the caller — rule 8. The property is *the process stays and the weights are
/// loaded once*, so the ready frame is written exactly once, before the loop,
/// and the pid is recorded exactly once, at the same place. A stand-in that
/// re-announced itself per request would be a stand-in for the thing this
/// change replaced.
fn be_the_engine() -> ! {
    let mut input = std::io::stdin();
    let mut output = std::io::stdout();

    if let Ok(path) = std::env::var(PID_LOG) {
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("the pid log");
        writeln!(log, "{}", std::process::id()).expect("recording the pid");
    }

    let answers: Vec<(String, String)> = std::env::var(ANSWERS)
        .unwrap_or_default()
        .split('|')
        .filter_map(|pair| pair.split_once('='))
        .map(|(word, contract)| (word.to_string(), contract.to_string()))
        .collect();
    let die_after: usize = std::env::var(DIE_AFTER)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(usize::MAX);

    // The ready frame, once. "THR1", load ms, pid, threads, context.
    let mut ready = Vec::from(*b"THR1");
    ready.extend_from_slice(&1u64.to_le_bytes());
    ready.extend_from_slice(&std::process::id().to_le_bytes());
    ready.extend_from_slice(&1u32.to_le_bytes());
    ready.extend_from_slice(&4096u32.to_le_bytes());
    output.write_all(&ready).expect("the ready frame");
    output.flush().expect("flushing it");

    let mut served = 0usize;
    loop {
        let mut magic = [0u8; 4];
        if input.read_exact(&mut magic).is_err() {
            std::process::exit(0); // Thalyx closed the pipe.
        }
        assert_eq!(&magic, b"THQ1", "that is not a request frame");

        let mut small = [0u8; 4];
        input.read_exact(&mut small).expect("predict");
        let mut wide = [0u8; 8];
        input.read_exact(&mut wide).expect("seed");
        let prompt_path = read_field(&mut input);
        let _grammar_path = read_field(&mut input);

        // The prompt, echoed. `Prompt::answer_in` finds the marker the prompt
        // ends with and takes what follows it, so a stand-in that answered
        // without the echo would be a stand-in for a tool Thalyx refuses.
        let prompt = std::fs::read_to_string(&prompt_path).expect("the prompt file");
        let answer = answers
            .iter()
            .find(|(word, _)| prompt.contains(word.as_str()))
            .map(|(_, contract)| contract.clone())
            .unwrap_or_default();
        let body = format!("{prompt}{answer}");

        let mut frame = Vec::from(*b"THA1");
        frame.push(0);
        frame.extend_from_slice(&2u64.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(body.as_bytes());
        output.write_all(&frame).expect("the answer frame");
        output.flush().expect("flushing it");

        served += 1;
        if served >= die_after {
            // The crash this file is about: an engine that goes away between
            // two sentences, without saying anything.
            std::process::exit(9);
        }
    }
}

fn read_field(input: &mut std::io::Stdin) -> String {
    let mut length = [0u8; 4];
    input.read_exact(&mut length).expect("a field length");
    let mut bytes = vec![0u8; u32::from_le_bytes(length) as usize];
    input.read_exact(&mut bytes).expect("a field");
    String::from_utf8(bytes).expect("a path this test wrote")
}

// ──────────────────────────────────────────────────────────── the arrangement

struct Machine {
    scratch: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    data: PathBuf,
    pids: PathBuf,
}

impl Machine {
    /// A store with this binary installed as the engine module.
    fn new() -> Machine {
        let scratch = tempfile::tempdir().expect("scratch");
        let base = scratch.path().to_path_buf();
        let root = base.join("store");
        let home = base.join("home");
        let data = base.join("engine-data");
        std::fs::create_dir_all(&home).unwrap();
        // `THALYX_ENGINE_DATA` moves both of these off `/opt/thalyx`, which on
        // the machine running this suite belongs to a real installation. Rule
        // 11: a test that writes something machine-global has changed the
        // machine it was measuring.
        std::fs::create_dir_all(data.join("run")).unwrap();
        std::fs::create_dir_all(data.join("models")).unwrap();

        let payload = base.join("payload/bin");
        std::fs::create_dir_all(&payload).unwrap();
        std::fs::copy(std::env::current_exe().unwrap(), payload.join("engine")).unwrap();

        let key = base.join("publisher.key");
        run(Command::new(thalyx())
            .args(["dev", "keygen", "--out"])
            .arg(&key));

        let manifest = base.join("engine.toml");
        std::fs::write(
            &manifest,
            format!(
                r#"
format_version = 1
id             = "dev.thalyx.engine"
name           = "stand-in engine"
version        = "1.0.0"
description    = "A program that speaks the engine protocol and counts its own starts"
license        = "GPL-3.0-or-later"
publisher_key  = "ed25519:0000000000000000000000000000000000000000000000000000000000000000"
distribution   = "prebuilt"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 0

[requires]
thalyx = ">=0.1.0"

[[permissions]]
resource = "{run}"
action   = "read"
type     = "persistent"

[entrypoints]
run = "bin/engine"
"#,
                run = data.join("run").display(),
            ),
        )
        .unwrap();

        let bundle = base.join("engine.thmod");
        run(Command::new(thalyx())
            .args(["dev", "pack"])
            .arg(base.join("payload"))
            .arg("--manifest")
            .arg(&manifest)
            .arg("--key")
            .arg(&key)
            .arg("--out")
            .arg(&bundle));
        run(Command::new(thalyx())
            .arg("--root")
            .arg(&root)
            .args(["module", "install"])
            .arg(&bundle)
            .arg("--yes"));

        let weights = data.join("models/model.gguf");
        std::fs::write(&weights, b"never opened by a stand-in").unwrap();
        run(Command::new(thalyx())
            .arg("--root")
            .arg(&root)
            .args(["agent", "model", "use", "ligera", "--weights"])
            .arg(&weights)
            .args(["--binary", "/nonexistent-on-purpose"])
            .args(["--module", "dev.thalyx.engine"]));

        Machine {
            pids: base.join("pids"),
            scratch,
            root,
            home,
            data,
        }
    }

    /// Type lines into one session process, which is what makes this a test
    /// about residency: the engine lives inside that process's lifetime.
    fn session(&self, answers: &[&str], die_after: Option<usize>, lines: &[&str]) -> String {
        let mut command = Command::new(thalyx());
        command
            .arg("--root")
            .arg(&self.root)
            .arg("session")
            .current_dir(&self.home)
            .env("HOME", &self.home)
            .env("THALYX_ENGINE_DATA", &self.data)
            // No BPF LSM here. See the module docs: this is the half of
            // `run::start` a container can answer.
            .env("THALYX_ENGINE_UNCONFINED", "1")
            .env(STANDIN, "1")
            .env(PID_LOG, &self.pids)
            .env(ANSWERS, answers.join("|"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(after) = die_after {
            command.env(DIE_AFTER, after.to_string());
        }

        let mut child = command.spawn().expect("a session");
        let mut script = format!("cd {}\n", self.home.display());
        for line in lines {
            script.push_str(line);
            script.push('\n');
        }
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let out = child.wait_with_output().expect("the session finishing");
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }

    /// One line per engine process that ever started. The measurement.
    fn engine_pids(&self) -> Vec<String> {
        std::fs::read_to_string(&self.pids)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn made(&self, name: &str) -> bool {
        self.home.join(name).exists()
    }
}

fn run(command: &mut Command) {
    let out = command.output().expect("running thalyx");
    assert!(
        out.status.success(),
        "{command:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A contract the stand-in answers with, keyed on the word that asks for it.
fn contract(operation: &str, target: &str) -> String {
    format!(r#"{target}={{ "operation": "{operation}", "targets": ["{target}"] }}"#)
}

// ───────────────────────────────────────────────────────────────── the claims

/// The whole point: two sentences, one engine.
fn two_sentences_are_answered_by_one_engine_process() {
    let machine = Machine::new();
    let said = machine.session(
        &[
            &contract("make_directory", "primera"),
            &contract("make_directory", "segunda"),
        ],
        None,
        &[
            "crea una carpeta llamada primera",
            "crea una carpeta llamada segunda",
            "salir",
        ],
    );

    assert!(
        machine.made("primera") && machine.made("segunda"),
        "both sentences did not reach the machine:\n{said}"
    );

    let pids = machine.engine_pids();
    assert_eq!(
        pids.len(),
        1,
        "the engine was started {} times for two sentences — the weights were \
         loaded again. Session said:\n{said}",
        pids.len()
    );
}

/// An engine that dies is started again once, and the sentence still lands.
///
/// The control for the test above at the same time: without this, an engine
/// that was never restarted at all would also produce one pid, and the machine
/// would silently stop understanding anything after its first crash.
fn an_engine_that_died_is_started_again_and_the_sentence_still_lands() {
    let machine = Machine::new();
    let said = machine.session(
        &[
            &contract("make_directory", "primera"),
            &contract("make_directory", "segunda"),
        ],
        Some(1),
        &[
            "crea una carpeta llamada primera",
            "crea una carpeta llamada segunda",
            "salir",
        ],
    );

    assert!(
        machine.made("primera"),
        "the first sentence did not land:\n{said}"
    );
    assert!(
        machine.made("segunda"),
        "the engine died and the machine never came back:\n{said}"
    );
    assert_eq!(
        machine.engine_pids().len(),
        2,
        "an engine that dies should be started again exactly once per sentence \
         that needs it. Session said:\n{said}"
    );
}

/// The control for both: a machine whose engine cannot start says so and stays
/// usable, rather than hanging on a frame that will never arrive.
fn an_engine_that_never_says_it_is_ready_does_not_take_the_machine_with_it() {
    let machine = Machine::new();
    // No `THALYX_STANDIN_ENGINE`, so the copy of this binary runs as a test
    // harness, prints something that is not a ready frame, and exits.
    let mut command = Command::new(thalyx());
    command
        .arg("--root")
        .arg(&machine.root)
        .arg("session")
        .current_dir(&machine.home)
        .env("HOME", &machine.home)
        .env("THALYX_ENGINE_DATA", &machine.data)
        .env("THALYX_ENGINE_UNCONFINED", "1")
        .env_remove(STANDIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("a session");
    let script = format!(
        "cd {}\ncrea una carpeta llamada primera\nmkdir a-mano\nsalir\n",
        machine.home.display()
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("the session finishing");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        !machine.made("primera"),
        "something acted on a sentence no engine ever answered:\n{said}"
    );
    assert!(
        machine.made("a-mano"),
        "the human's own route stopped working when the engine would not \
         start — which is `Principio-Doble-Ruta.md` broken:\n{said}"
    );
    let _ = &machine.scratch;
}

fn main() {
    if std::env::var(STANDIN).as_deref() == Ok("1") {
        be_the_engine();
    }

    let checks: Vec<(&str, fn())> = vec![
        (
            "two_sentences_are_answered_by_one_engine_process",
            two_sentences_are_answered_by_one_engine_process,
        ),
        (
            "an_engine_that_died_is_started_again_and_the_sentence_still_lands",
            an_engine_that_died_is_started_again_and_the_sentence_still_lands,
        ),
        (
            "an_engine_that_never_says_it_is_ready_does_not_take_the_machine_with_it",
            an_engine_that_never_says_it_is_ready_does_not_take_the_machine_with_it,
        ),
    ];

    // One at a time and in this process, which is what `harness = false` costs
    // and what it buys: the module's entrypoint is a copy of this file's
    // binary, and libtest's own arguments would reach it.
    for (name, check) in checks {
        print!("{name} ... ");
        std::io::stdout().flush().unwrap();
        check();
        println!("ok");
    }
    println!("\nall {} checks passed", 3);
}
