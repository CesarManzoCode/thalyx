//! What survives when one request of a query answers and another does not.
//!
//! ## The physical evidence
//!
//! On 2026-09-05, on a fresh Thalyx VM with the guard armed, the very first
//! question of the boot — `context('LanternRegistry')` — came back as
//!
//! ```text
//! source=index  analyzer_confined=true
//! analyzer_error="rust-analyzer did not answer: `textDocument/hover` after 30s"
//! ```
//!
//! and seconds later, **in the same run**, the same machine renamed that same
//! symbol across two files semantically, resolved its definition exactly, and
//! rolled the change back. A second run without rebooting answered the same
//! question from rust-analyzer with twelve use sites.
//!
//! So nothing was broken. `workspace/symbol` had already resolved the name to
//! exactly one declaration — that is what `ask_about` asks first, and the
//! error names the request that comes *after* it. What happened is that a
//! `textDocument/hover` on a server that was still warming up ran out of
//! [`thalyx_rust::analyzer::ANSWER_CEILING`], and a `?` on the enrichment
//! turned the whole query into an error. `gather` then fell back to the index,
//! and a name a compiler had resolved was reported as a textual match.
//!
//! ## What is asserted here
//!
//! That the resolution survives the enrichment, and that what was not obtained
//! is **absent rather than invented**: no signature rather than an empty one,
//! and no count of uses rather than a zero. And that the reason is kept, in
//! [`thalyx_rust::Provider::shortfall`] — a timeout that is survived is still
//! a timeout, and hiding it would trade one silent wrong answer for another.
//!
//! ## Why there is a stand-in
//!
//! Rule 8. The property is *one request failing while another succeeds*, and
//! the only way to get that out of a real rust-analyzer is to catch it cold,
//! which is not a state a test can ask for. `tests/stand-in/server.py` answers
//! the resolution in every mode and is told which enrichment to withhold. The
//! control — the mode that answers everything — is rule 4: without it, an
//! absent signature would be evidence of a stand-in that cannot produce one.

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use thalyx_know::Knowledge;
use thalyx_rust::analyzer::{Launching, Spawn, Started};
use thalyx_rust::{Provider, Resolution};

/// A spawner that starts the stand-in instead of whatever this machine has.
///
/// `asked.program` is ignored on purpose: this is the one case where the
/// server under test is not the host's.
struct StandsIn {
    root: PathBuf,
    mode: &'static str,
}

impl Spawn for StandsIn {
    fn start(&self, _asked: Launching<'_>) -> thalyx_rust::Result<Started> {
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("stand-in")
            .join("server.py");
        let child = Command::new("python3")
            .arg(&script)
            .arg(&self.root)
            .arg(self.mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("a stand-in server");
        Ok(Started {
            child,
            release: None,
            how: "a stand-in".to_string(),
            confined: false,
        })
    }
}

/// A provider over the `counted` tree whose server is the stand-in.
///
/// The binary is named rather than looked for: this container has no
/// rust-analyzer, and a check that could only run on a machine that has one
/// would be a check that never ran where the code is written.
fn asking(root: &Path, mode: &'static str) -> Provider {
    Provider::open(root, Knowledge::in_memory().expect("a knowledge store"))
        .analyzing_with(PathBuf::from("/nonexistent-the-spawner-decides"))
        .spawning(Arc::new(StandsIn {
            root: root.to_path_buf(),
            mode,
        }))
}

fn one(resolution: &Resolution) -> &thalyx_rust::Known {
    resolution
        .only()
        .unwrap_or_else(|| panic!("`Flywheel` is declared once and came back as {resolution:?}"))
}

/// The control, and it runs first for a reason: everything below is an absence,
/// and an absence is only evidence next to the presence it is missing.
#[test]
fn a_server_that_answers_everything_gives_the_signature_and_the_uses() {
    if !support::cargo_or_skip("that a whole answer carries all of itself")
        || !support::stand_in_or_skip("that a whole answer carries all of itself")
    {
        return;
    }
    let (_held, root) = support::tree("counted");
    let mut provider = asking(&root, "whole");

    let (resolution, _, _) = provider.known("Flywheel").expect("an answer");
    let known = one(&resolution);
    assert_eq!(known.defined[0].path, "src/hub.rs");
    assert_eq!(
        known.signature.as_deref(),
        Some("pub struct Flywheel"),
        "the stand-in answers hovers in this mode, so a `None` here would mean \
         the reader is broken rather than the server silent"
    );
    assert_eq!(
        known.used.as_deref().map(<[_]>::len),
        Some(2),
        "the stand-in answers references in this mode: {known:?}"
    );
    assert_eq!(
        provider.shortfall(),
        None,
        "nothing went unanswered and something says it did"
    );
}

/// **The defect of 2026-09-05, as a claim.**
///
/// It costs [`thalyx_rust::analyzer::ANSWER_CEILING`] of wall clock, because
/// silence is what the machine measured: a hover that comes back as an error
/// is a different event, and the one that happened was a request nobody
/// answered.
#[test]
fn a_hover_that_never_answers_does_not_unresolve_the_name() {
    if !support::cargo_or_skip("that a silent hover keeps the resolution")
        || !support::stand_in_or_skip("that a silent hover keeps the resolution")
    {
        return;
    }
    let (_held, root) = support::tree("counted");
    let mut provider = asking(&root, "no-hover");

    let (resolution, _, _) = provider
        .known("Flywheel")
        .expect("a query whose resolution answered is not an error");

    let known = one(&resolution);
    assert_eq!(
        known.defined[0].path, "src/hub.rs",
        "the declaration `workspace/symbol` resolved was thrown away: {known:?}"
    );
    assert_eq!(
        known.signature, None,
        "a signature nobody obtained: {known:?}"
    );
    // The whole point of asking for both: the request after the silent one
    // still ran, and its answer is here.
    assert_eq!(
        known.used.as_deref().map(<[_]>::len),
        Some(2),
        "the use sites came back and were dropped with the hover: {known:?}"
    );

    let shortfall = provider.shortfall().expect("the reason, kept");
    assert!(
        shortfall.contains("textDocument/hover") && shortfall.contains("did not answer"),
        "a timeout that is survived is still a timeout and has to be reported \
         as one, and this said: {shortfall}"
    );
}

#[test]
fn use_sites_that_never_came_back_are_unknown_and_not_zero() {
    if !support::cargo_or_skip("that uses nobody counted are unknown")
        || !support::stand_in_or_skip("that uses nobody counted are unknown")
    {
        return;
    }
    let (_held, root) = support::tree("counted");
    let mut provider = asking(&root, "no-references");

    let (resolution, _, _) = provider.known("Flywheel").expect("an answer");
    let known = one(&resolution);
    assert_eq!(
        known.signature.as_deref(),
        Some("pub struct Flywheel"),
        "the hover answered in this mode: {known:?}"
    );
    // Rule 10, in the one field where the difference decides something: `0`
    // says nothing uses this, which is what a model reads to decide a symbol
    // is dead. Nobody counted, so nobody may say.
    assert!(
        known.used.is_none(),
        "an empty list of uses is a finding, and nothing here found it: {known:?}"
    );
    assert!(
        provider
            .shortfall()
            .is_some_and(|why| why.contains("textDocument/references")),
        "the refusal was survived without being reported: {:?}",
        provider.shortfall()
    );
}

/// A partial answer must not be the answer to every later question.
///
/// What makes an enrichment fail is a server still warming up — gone by the
/// next question — while what the machine remembers lives until the sources
/// move. Remembering "the uses of `Flywheel` are unknown" would answer every
/// question for the rest of the tree's life from the one moment the machine
/// was cold.
#[test]
fn a_partial_answer_is_asked_again_rather_than_remembered() {
    if !support::cargo_or_skip("that a partial answer is not remembered")
        || !support::stand_in_or_skip("that a partial answer is not remembered")
    {
        return;
    }
    let (_held, root) = support::tree("counted");

    // The control first: the same two questions against a server that answers
    // everything really are remembered, so that the zero below is this rule
    // and not a cache that never worked.
    let mut whole = asking(&root, "whole");
    whole.known("Flywheel").expect("an answer");
    whole.known("Flywheel").expect("an answer");
    assert!(
        whole.tally.hits >= 1,
        "a whole answer was not remembered either, so the check below is \
         measuring the cache and not the rule: {:?}",
        whole.tally
    );

    let mut partial = asking(&root, "no-references");
    partial.known("Flywheel").expect("an answer");
    let (again, _, _) = partial.known("Flywheel").expect("an answer");
    assert_eq!(
        partial.tally.hits, 0,
        "the partial answer was remembered, and now the tree has one it cannot \
         get rid of: {:?}",
        partial.tally
    );
    assert!(
        one(&again).used.is_none() && partial.shortfall().is_some(),
        "the second answer stopped saying what it is missing"
    );
}
