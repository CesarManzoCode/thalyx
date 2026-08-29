//! `contexto`, driven from a real session.
//!
//! The claim is a comparison, so the test is one: the answer about a symbol,
//! against the file the answer is about. A surface that returned a description
//! nobody could act on would pass an assertion that it is short; a surface that
//! returned the file would pass an assertion that it is useful. Both numbers
//! are here, in the same test, over the same tree.
//!
//! It runs the session as a **separate process**, which is not a style choice:
//! descriptors 0, 1 and 2 belong to the process, `cargo test` runs a binary's
//! tests as threads of one, and a test that captured this one's stdout in
//! process would catch libtest's progress lines. That is written down in
//! `Estrategia-de-Pruebas` rule 11 and it cost a day.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

/// Rule 3: one variable per requirement, and a skip that says it skipped.
fn analyzer_or_skip(what: &str) -> bool {
    let found = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".rustup").join("toolchains"))
        .and_then(|toolchains| std::fs::read_dir(toolchains).ok())
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| entry.path().join("bin").join("rust-analyzer").is_file());
    if found {
        return true;
    }
    let message = format!(
        "NOT PROVEN: {what} — there is no rust-analyzer on this machine. \
         Set THALYX_REQUIRE_RUST_ANALYZER=1 to make this a failure."
    );
    assert!(
        std::env::var("THALYX_REQUIRE_RUST_ANALYZER").as_deref() != Ok("1"),
        "{message}"
    );
    eprintln!("{message}");
    false
}

fn piped(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session");
    let typed: String = lines.iter().map(|line| format!("{line}\n")).collect();
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(typed.as_bytes())
        .expect("feeding the session");
    child.wait_with_output().expect("waiting for the session")
}

fn objects(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .replace('\r', "")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .collect()
}

fn answers(output: &Output, op: &str) -> Vec<serde_json::Value> {
    objects(output)
        .into_iter()
        .filter(|value| value["op"] == serde_json::json!(op))
        .collect()
}

/// A Cargo package big enough that reading the file is a real cost.
fn a_crate() -> (tempfile::TempDir, PathBuf) {
    let held = tempfile::tempdir().expect("a store");
    let work = held.path().join("work");
    std::fs::create_dir_all(work.join("src")).expect("the tree");
    std::fs::write(
        work.join("Cargo.toml"),
        "[workspace]\n\n[package]\nname = \"mapped\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("a manifest");

    std::fs::write(work.join("src").join("lib.rs"), "pub mod keystore;\n").expect("a lib");

    // The padding goes in the same file as the symbol, deliberately: the
    // comparison this test makes is between the answer and **the file an agent
    // would have opened to get it**, and padding somewhere else would make that
    // comparison flattering rather than true.
    let mut keystore = String::from(
        "/// The one thing anybody is going to ask about.\npub struct Keystore {\n    pub opened: bool,\n}\n\nimpl Keystore {\n    pub fn unlock(&self) -> bool {\n        self.opened\n    }\n}\n",
    );
    for n in 0..120 {
        keystore.push_str(&format!(
            "\n/// Filler number {n}, which is what most of a real file is.\npub fn filler{n}() -> u32 {{\n    {n}\n}}\n"
        ));
    }
    std::fs::write(work.join("src").join("keystore.rs"), keystore).expect("a keystore");
    (held, work)
}

#[test]
fn a_context_answer_is_a_fraction_of_the_file_it_describes() {
    if !analyzer_or_skip("that a context answer is smaller than the file") {
        return;
    }
    let (held, work) = a_crate();
    let output = piped(
        held.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "contexto Keystore",
            "salir",
        ],
    );
    let answer = answers(&output, "context")
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("nothing answered `context`: {:#?}", objects(&output)));

    assert_eq!(answer["ok"], serde_json::json!(true), "{answer}");
    assert_eq!(
        answer["source"],
        serde_json::json!("rust-analyzer"),
        "the answer must say it was resolved rather than matched: {answer}"
    );
    assert_eq!(answer["fresh"], serde_json::json!("current"));

    let entry = &answer["entries"][0];
    assert_eq!(entry["name"], serde_json::json!("Keystore"));
    assert_eq!(entry["kind"], serde_json::json!("struct"));
    assert_eq!(entry["crate"], serde_json::json!("mapped"));
    assert_eq!(entry["file"], serde_json::json!("src/keystore.rs"));
    assert!(
        entry["handle"]
            .as_str()
            .is_some_and(|handle| handle.starts_with("ctx-")),
        "an entry with no handle is a summary with no way back to the source: {entry}"
    );

    // The comparison the whole verb exists for.
    let returned = answer["returned_bytes"].as_u64().expect("a byte count");
    let held = answer["held_bytes"].as_u64().expect("a byte count");
    let whole = std::fs::metadata(work.join("src").join("keystore.rs"))
        .expect("the file an agent would have opened")
        .len();
    assert_eq!(
        held, whole,
        "what the answer says it is holding back has to be what it is actually \
         holding back, or the measurement is decoration"
    );
    assert!(
        returned * 10 < whole,
        "the answer was {returned} bytes and the file an agent would otherwise \
         read is {whole}; this verb is supposed to be an order of magnitude \
         cheaper, not slightly cheaper"
    );
}

#[test]
fn a_handle_fetches_the_exact_lines_and_nothing_around_them() {
    if !analyzer_or_skip("that a handle expands to exactly its declaration") {
        return;
    }
    let (held, work) = a_crate();
    let first = piped(
        held.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "contexto Keystore",
            "salir",
        ],
    );
    let handle = answers(&first, "context")[0]["entries"][0]["handle"]
        .as_str()
        .expect("a handle")
        .to_string();

    let second = piped(
        held.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            &format!("contexto expandir={handle}"),
            "salir",
        ],
    );
    let expanded = answers(&second, "context")
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("nothing answered: {:#?}", objects(&second)));

    assert_eq!(expanded["ok"], serde_json::json!(true), "{expanded}");
    assert_eq!(expanded["file"], serde_json::json!("src/keystore.rs"));
    assert_eq!(
        expanded["fresh"],
        serde_json::json!("current"),
        "a handle issued against a tree that has not moved is current"
    );
    let text = expanded["text"].as_str().expect("the source");
    assert!(text.contains("pub struct Keystore"), "{text}");
    assert!(
        !text.contains("filler0"),
        "the expansion handed back more than the declaration it names: {text}"
    );

    // The second process knew what the first one issued, which is the point of
    // keeping it in the machine rather than in the conversation.
    assert!(
        expanded["returned_bytes"].as_u64().expect("a count") > 0,
        "the handle resolved to nothing: {expanded}"
    );
}

#[test]
fn a_budget_that_is_named_is_a_budget_that_is_kept() {
    if !analyzer_or_skip("that a named budget bounds the answer") {
        return;
    }
    let (held, work) = a_crate();
    let output = piped(
        held.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "contexto src/keystore.rs presupuesto=300",
            "salir",
        ],
    );
    let answer = answers(&output, "context")
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("nothing answered: {:#?}", objects(&output)));

    assert_eq!(answer["budget_bytes"], serde_json::json!(300));
    let shown = answer["shown"].as_u64().expect("a count");
    let omitted = answer["omitted_for_budget"].as_u64().expect("a count");
    assert!(
        shown >= 1 && omitted >= 1,
        "a file with 122 declarations under a 300-byte budget should show some \
         and hold the rest: {answer}"
    );
    assert!(
        answer["returned_bytes"].as_u64().expect("a count") <= 400,
        "the budget was not kept: {answer}"
    );
    assert!(
        answer["held_bytes"].as_u64().expect("a count") > 0,
        "what was not returned has to be counted, or the loss is silent: {answer}"
    );
}

#[test]
fn a_handle_from_a_tree_that_moved_says_so_rather_than_lying() {
    if !analyzer_or_skip("that a stale handle says it is stale") {
        return;
    }
    let (held, work) = a_crate();
    let first = piped(
        held.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "contexto Keystore",
            "salir",
        ],
    );
    let handle = answers(&first, "context")[0]["entries"][0]["handle"]
        .as_str()
        .expect("a handle")
        .to_string();

    // Somebody else edits the tree between the two calls, which is the ordinary
    // case and not the exotic one.
    let keystore = work.join("src").join("keystore.rs");
    let text = std::fs::read_to_string(&keystore).expect("the file");
    std::fs::write(
        &keystore,
        format!("// a line nobody told Thalyx about\n{text}"),
    )
    .expect("the edit");

    let second = piped(
        held.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            &format!("contexto expandir={handle}"),
            "salir",
        ],
    );
    let expanded = answers(&second, "context")
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("nothing answered: {:#?}", objects(&second)));

    assert_eq!(
        expanded["fresh"],
        serde_json::json!("stale"),
        "the tree moved and the handle still called itself current, which is \
         how a model ends up editing the wrong lines: {expanded}"
    );
    assert_eq!(
        expanded["ok"],
        serde_json::json!(true),
        "a stale answer is still handed over — marked. Refusing it would cost a \
         caller the ability to say `this is what I knew, and it moved`"
    );
}

#[test]
fn the_use_sites_are_asked_for_rather_than_assumed() {
    if !analyzer_or_skip("that use sites come back when they are asked for") {
        return;
    }
    let (held, work) = a_crate();

    // The default: the count and nothing else. A symbol with two hundred uses
    // would otherwise be the whole budget, and "is this used at all" and "is
    // this used a lot" — which is most of what the list gets asked — are both
    // answered by the number.
    let quiet = piped(
        held.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "contexto Keystore",
            "salir",
        ],
    );
    let entry = answers(&quiet, "context")[0]["entries"][0].clone();
    assert!(entry["uses"].as_u64().expect("a count") >= 1);
    assert!(
        entry["used_at"].is_null(),
        "the list came back without being asked for: {entry}"
    );

    let asked = piped(
        held.path(),
        &[
            "structured on",
            &format!("cd {}", work.display()),
            "contexto Keystore usos=10",
            "salir",
        ],
    );
    let entry = answers(&asked, "context")[0]["entries"][0].clone();
    let sites = entry["used_at"].as_array().expect("the sites").clone();
    assert!(!sites.is_empty(), "{entry}");
    assert_eq!(
        sites.len() as u64,
        entry["uses"].as_u64().expect("a count").min(10),
        "the list and the count have to be about the same thing: {entry}"
    );
    assert!(
        sites
            .iter()
            .all(|site| site.as_str().is_some_and(|text| text.contains(".rs:"))),
        "a use site is a file and a line: {sites:?}"
    );
}
