//! A sentence nobody spelled as a verb reaches a model, and the machine acts.
//!
//! This is the last link of the chain `vault/09-Notas-Tecnicas/Agente-Minimo.md`
//! describes and the one that was missing until 2026-08-28: the agent could
//! turn an utterance into a plan, and nothing ran the plan. A person at the
//! machine who did not know the word `mkdir` was told that Thalyx had no model
//! loaded — which was true, and was also the entire agent being unreachable
//! from the only face the machine has.
//!
//! ## What is measured here, and what deliberately is not
//!
//! **Measured:** that the session hands a non-verb to the agent, that what the
//! agent produces is turned into a verb of the session's own vocabulary, that
//! the verb is *run*, and that the machine changed as a result. The engine is a
//! stand-in program, and that is the point of the file: this is a claim about
//! the wiring, not about a model's judgement.
//!
//! **Not measured, and not measurable here:** whether a real Qwen2.5 answers
//! that sentence with that proposal. There is no llama.cpp and no weights in a
//! development container. `dev/verify.sh` §46 asks his machine, and prints
//! NOT PROVEN when the machine cannot answer.
//!
//! Rule 4 all through: every claim has the control beside it. Without the
//! second test, a machine that had simply grown an `mkdir` alias would pass the
//! first; without the third, "the agent no longer looks on `PATH`" is a
//! sentence nothing checks.

use std::path::Path;
use std::process::{Command, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// A program that behaves the way llama.cpp does: echo the prompt, then answer.
///
/// The echo is not decoration. `prompt.rs` mints a marker per invocation and
/// `llama.rs` takes the answer to be whatever follows it — a stand-in that
/// printed only the answer would be a stand-in for a tool Thalyx refuses, which
/// is rule 8: a fake that fails the property under test is a different system.
fn engine_that_answers(dir: &Path, answer: &str) -> std::path::PathBuf {
    let path = dir.join("stand-in-engine.sh");
    std::fs::write(
        &path,
        format!("#!/bin/sh\ncat \"$4\"\nprintf '%s' '{answer}'\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

/// Enough of a GGUF for the settings to record. Never read: the engine is a
/// stand-in and does not open it.
fn weights(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("weights.gguf");
    std::fs::write(&path, b"not a real model, and never opened by a stand-in").unwrap();
    path
}

/// Type at a session standing somewhere this test owns.
///
/// The session always opens at `/home` — `thalyx_files::HOME` is a constant,
/// not an environment variable — so the first line typed is always a `cd` into
/// a directory this test made. That is rule 11 and it is not a formality: the
/// first version of this file made `/home/pruebas` on the machine running the
/// suite, twice, and every check after it was measuring a machine nobody had
/// asked for.
fn typed(root: &Path, home: &Path, lines: &[&str]) -> String {
    use std::io::Write;

    let mut child = Command::new(thalyx())
        .args(["--root"])
        .arg(root)
        .arg("session")
        .current_dir(home)
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("a session");

    let mut script = format!("cd {}\n", home.display());
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

    let output = child.wait_with_output().expect("the session finishing");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn configure(root: &Path, weights: &Path, engine: &str, module: Option<&str>) {
    let mut command = Command::new(thalyx());
    command
        .args(["--root"])
        .arg(root)
        .args(["agent", "model", "use", "ligera", "--weights"])
        .arg(weights)
        .args(["--binary", engine]);
    if let Some(module) = module {
        command.args(["--module", module]);
    }
    let out = command.output().expect("recording the model");
    assert!(
        out.status.success(),
        "could not record the model: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_sentence_that_is_not_a_verb_is_understood_and_carried_out() {
    let scratch = tempfile::tempdir().unwrap();
    let home = scratch.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let root = scratch.path().join("store");

    let engine = engine_that_answers(
        scratch.path(),
        r#"{ "operation": "make_directory", "targets": ["pruebas"] }"#,
    );
    configure(
        &root,
        &weights(scratch.path()),
        engine.to_str().unwrap(),
        None,
    );

    let said = typed(&root, &home, &["crea una carpeta llamada pruebas", "salir"]);

    // Said out loud before it happened. A machine that silently turns one
    // sentence into another command is one nobody can trust with the next.
    assert!(
        said.contains("I understood that as: mkdir pruebas"),
        "the session never said what it made of the sentence:\n{said}"
    );
    assert!(
        home.join("pruebas").is_dir(),
        "the verb was named and not run:\n{said}"
    );
}

#[test]
fn a_machine_with_no_model_is_left_exactly_as_usable_as_it_was() {
    // The control for the test above, and `Principio-Doble-Ruta.md` itself:
    // without it, an agent that had quietly become a second `mkdir` would pass.
    // Nothing is configured here, so nothing may be understood — and the
    // session must still be standing afterwards.
    let scratch = tempfile::tempdir().unwrap();
    let home = scratch.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let root = scratch.path().join("store");

    let said = typed(
        &root,
        &home,
        &["crea una carpeta llamada pruebas", "mkdir a-mano", "salir"],
    );

    assert!(
        !home.join("pruebas").exists(),
        "something acted on a sentence with no model to understand it:\n{said}"
    );
    assert!(
        home.join("a-mano").is_dir(),
        "the human's own route stopped working when the model was absent:\n{said}"
    );
}

#[test]
fn the_engine_named_by_the_settings_is_a_module_and_not_a_name_on_path() {
    // What Cesar decreed on 2026-08-28. Until then the agent ran whatever
    // `llama-completion` `PATH` resolved to, which on the machine itself
    // resolves to nothing — there is no `PATH` in there and no libc.
    //
    // The claim is negative and needs to be: an engine named as a module must
    // be *looked for in the store*, so a module nobody installed fails saying
    // so. A `PATH` lookup would either find a llama.cpp on the developer's
    // machine and pass for the wrong reason, or fail with the wrong sentence.
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("store");
    let engine = engine_that_answers(scratch.path(), "{}");
    configure(
        &root,
        &weights(scratch.path()),
        engine.to_str().unwrap(),
        Some("dev.thalyx.engine"),
    );

    let out = Command::new(thalyx())
        .args(["--root"])
        .arg(&root)
        .args(["agent", "model", "show"])
        .output()
        .expect("agent model show");
    let said = String::from_utf8_lossy(&out.stdout);

    assert!(
        said.contains("module dev.thalyx.engine"),
        "the settings do not say the engine is a module:\n{said}"
    );
    assert!(
        said.contains("NOT READY") && said.contains("dev.thalyx.engine"),
        "a module nobody installed was reported as ready, or named as something \
         else entirely:\n{said}"
    );
}
