//! Quoting, at the real prompt and against the real filesystem.
//!
//! Point 9 of the usable terminal, decided by Cesar on 2026-08-23: quoting now,
//! a whole shell language later, and nothing learned now unlearned then. The
//! decree is `vault/02-Arquitectura/Palabras.md`.
//!
//! **Rule 1**: `words.rs` has unit tests and they check the splitting. These
//! check that a file with a space in its name can actually be copied, moved and
//! removed — which is the hole, and which the splitting alone does not prove.
//!
//! **Rule 4**: every claim here has a control, and two of them are the whole
//! point.
//!
//! - that quoting groups → an unquoted line still splits exactly as it did;
//! - that a quoted `*` is a name → a file that a real pattern *would* have
//!   caught is still there afterwards. Without that, a `rm` that had simply
//!   stopped matching anything would pass.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

fn typed(root: &Path, here: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root)
        .current_dir(here)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session");

    let mut script = format!("cd {}\n", here.display());
    for line in lines {
        script.push_str(line);
        script.push('\n');
    }
    script.push_str("salir\n");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("feeding the session");
    child.wait_with_output().expect("waiting for the session")
}

fn answer_to(output: &Output, op: &str) -> serde_json::Value {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with('{'))
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value["op"] == op)
        .unwrap_or_else(|| {
            panic!(
                "nothing answered with op `{op}`; the session said:\n{}",
                String::from_utf8_lossy(&output.stdout)
            )
        })
}

/// A tree with the awkward names in it, written here so what is there is known
/// before anything is typed rather than read out of the answer.
fn a_tree() -> (tempfile::TempDir, tempfile::TempDir) {
    let root = tempfile::tempdir().expect("a store root");
    let here = tempfile::tempdir().expect("somewhere to work");
    for name in ["mi archivo.txt", "a b.log", "otro.log", "sencillo.txt"] {
        std::fs::write(here.path().join(name), b"hola\n").expect("writing the tree");
    }
    (root, here)
}

#[test]
fn a_file_whose_name_has_a_space_can_be_copied_moved_and_removed() {
    // The hole, and it was a hole and not an inconvenience: before this the
    // three verbs refused — `cp mi archivo.txt x` is three words — and there was
    // no way to name the file at all.
    let (root, here) = a_tree();
    let at = here.path();

    typed(
        root.path(),
        at,
        &[
            r#"cp "mi archivo.txt" copia.txt"#,
            r#"mv "mi archivo.txt" "otro nombre.txt""#,
            r#"rm "a b.log""#,
        ],
    );

    assert!(at.join("copia.txt").exists(), "the copy was not made");
    assert!(
        at.join("otro nombre.txt").exists(),
        "the move did not happen"
    );
    assert!(
        !at.join("mi archivo.txt").exists(),
        "the original is still there, so the move only copied"
    );
    assert!(!at.join("a b.log").exists(), "the removal did not happen");
    // The control: nothing else was touched.
    assert!(at.join("otro.log").exists());
    assert!(at.join("sencillo.txt").exists());
}

#[test]
fn a_line_with_no_quotes_in_it_still_means_exactly_what_it_meant() {
    // The control for the whole change. Every line typed at this prompt until
    // 2026-08-23 had no quotes in it, and none of them may have changed meaning.
    let (root, here) = a_tree();
    let at = here.path();

    typed(root.path(), at, &["rm otro.log sencillo.txt"]);

    assert!(!at.join("otro.log").exists());
    assert!(!at.join("sencillo.txt").exists());
    assert!(
        at.join("mi archivo.txt").exists(),
        "something nobody named was removed"
    );
}

#[test]
fn a_quoted_star_is_a_name_and_a_bare_star_is_still_a_pattern() {
    let (root, here) = a_tree();
    let at = here.path();
    // A file actually called `*.log`, which is the only way to tell the two
    // apart: without it, "the quoted one matched nothing" and "the quoted one
    // was taken literally" look the same.
    std::fs::write(at.join("*.log"), b"raro\n").expect("a file called *.log");

    typed(root.path(), at, &[r#"rm "*.log""#]);
    assert!(
        !at.join("*.log").exists(),
        "the literal name was not removed"
    );
    assert!(
        at.join("otro.log").exists() && at.join("a b.log").exists(),
        "the quoted star was expanded as a pattern"
    );

    // And the other half, in a second session so the two are separate events.
    typed(root.path(), at, &["rm *.log"]);
    assert!(
        !at.join("otro.log").exists() && !at.join("a b.log").exists(),
        "an unquoted star stopped being a pattern"
    );
    assert!(at.join("sencillo.txt").exists());
}

#[test]
fn a_quote_that_is_never_closed_refuses_and_leaves_everything_alone() {
    // The failure that matters. A shell asks for another line; a session has one
    // line, and guessing where the name ends is how `rm` acts on something
    // nobody named.
    let (root, here) = a_tree();
    let at = here.path();

    let output = typed(root.path(), at, &[r#"structured on"#, r#"rm "a b.log"#]);
    let said = answer_to(&output, "remove");
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"], "unclosed_quote");
    assert_eq!(said["remedy"], "close_the_quote");

    // The control, and the only one that proves the refusal was worth making.
    assert!(at.join("a b.log").exists(), "it removed something anyway");
    assert!(at.join("otro.log").exists());
}

#[test]
fn a_line_that_ends_on_a_backslash_is_refused_with_its_own_word() {
    // Its own word, because it is its own mistake: a quote is missing a partner
    // and a backslash is waiting for a character. A caller told the same thing
    // for both has to work out which.
    let (root, here) = a_tree();
    let output = typed(
        root.path(),
        here.path(),
        &["structured on", r"rm sencillo.txt\"],
    );
    let said = answer_to(&output, "remove");
    assert_eq!(said["error"], "trailing_backslash");
    assert_eq!(said["remedy"], "finish_the_escape");
    assert!(here.path().join("sencillo.txt").exists());
}

#[test]
fn a_rehearsal_of_a_quoted_name_names_the_same_file_the_verb_would() {
    // The rehearsal has to split the line the same way, or it is a rehearsal of
    // a different command — the mistake `matar` was caught making on the same
    // day, in the other direction.
    let (root, here) = a_tree();
    let output = typed(
        root.path(),
        here.path(),
        &["structured on", r#"ensayo rm "a b.log""#],
    );
    let said = answer_to(&output, "rehearse");
    assert_eq!(said["ok"], true);
    // `op` is what tells a rehearsal from the verb here — the rows say what
    // would be done, in the same shape the real answer uses.
    let named = said["results"][0]["path"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(named.ends_with("a b.log"), "it rehearsed on `{named}`");
    assert!(
        here.path().join("a b.log").exists(),
        "the rehearsal removed it"
    );
}

#[test]
fn a_search_takes_its_subject_whole_and_quoting_is_how_spaces_are_kept() {
    // `contenido fn main` looks for `fn main` — several words, one subject. What
    // changed on 2026-08-23 is that a run of spaces collapses, the way it does
    // in every terminal, and the quotes are how the other thing is said.
    let (root, here) = a_tree();
    std::fs::write(here.path().join("uno.rs"), "fn main() {}\nfn  main() {}\n")
        .expect("a file to search");

    let output = typed(
        root.path(),
        here.path(),
        &[
            "structured on",
            "contenido fn main",
            r#"contenido "fn  main""#,
        ],
    );
    let answers: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with('{'))
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value["op"] == "grep")
        .collect();
    assert_eq!(answers.len(), 2);
    assert_eq!(answers[0]["text"], "fn main");
    assert_eq!(answers[1]["text"], "fn  main");
    // One line each, and they are different lines: the collapse is real and the
    // quotes really do hold the run.
    assert_eq!(answers[0]["total"], 1);
    assert_eq!(answers[1]["total"], 1);
}

#[test]
fn editar_takes_its_name_as_a_word_and_its_text_as_the_line() {
    // The one carve-out, and it is deliberate: the third part of `editar` is
    // content going into a file, and a configuration line that begins with four
    // spaces means something with them and something else without.
    let (root, here) = a_tree();
    let at = here.path();
    std::fs::write(at.join("mi nota.txt"), "uno\n").expect("a file with a space");

    typed(
        root.path(),
        at,
        &[r#"editar "mi nota.txt" poner 2     sangrado"#],
    );

    let written = std::fs::read_to_string(at.join("mi nota.txt")).expect("reading it back");
    assert!(
        written.contains("    sangrado"),
        "the indentation did not survive: {written:?}"
    );
}
