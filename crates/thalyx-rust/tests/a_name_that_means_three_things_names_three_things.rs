//! What happens when a name is not one name.
//!
//! `vault/03-Primitivas/Semantica-Compilada.md`. Until 2026-08-30 the provider
//! answered a workspace with three `Config` declarations in it by taking the
//! first exact match rust-analyzer happened to list, and describing it as *the*
//! `Config` — with `source: "rust-analyzer"` on the answer, which is true and
//! is exactly why it would have been believed.
//!
//! What acts on that answer is `renombrar-simbolo`. So the failure mode was:
//! one of three crates gets rewritten everywhere it is used, chosen by index
//! order, with nothing anywhere saying a choice was made.
//!
//! Every test here has the control rule 4 asks for: the same question about a
//! name that really is unique, in the same tree, so that "it refused" is a
//! discrimination and not a machine that refuses everything.

mod support;

use std::path::Path;
use thalyx_know::Knowledge;
use thalyx_rust::{Provider, Resolution};

fn provider(root: &Path) -> Provider {
    Provider::open(root, Knowledge::in_memory().expect("a knowledge store"))
}

#[test]
fn three_declarations_of_one_name_come_back_as_three() {
    if !support::analyzer_or_skip("that an ambiguous name is answered as ambiguous") {
        return;
    }
    let (_held, root) = support::tree("three-configs");
    let mut provider = provider(&root);

    let (resolution, _, source) = provider.known("Config").expect("an answer");

    assert!(
        resolution.is_ambiguous(),
        "`Config` is declared in three crates of this workspace and the provider \
         answered {resolution:?}"
    );
    assert_eq!(source, "rust-analyzer");

    let candidates = resolution.candidates();
    assert_eq!(candidates.len(), 3, "{candidates:?}");

    // Each names its own crate, which is the fact that makes the list usable:
    // a caller choosing between three identical names chooses by where they
    // are, and an answer that did not say would be three of the same thing.
    let mut packages: Vec<&str> = candidates
        .iter()
        .filter_map(|candidate| candidate.package.as_deref())
        .collect();
    packages.sort_unstable();
    assert_eq!(packages, ["alpha", "beta", "gamma"], "{candidates:?}");

    // And each carries a handle that resolves exactly one of them — the same
    // `file:line:column` shape `renombrar` and `contexto` already take, so
    // resolving an ambiguity needs nothing new to be learned.
    for candidate in candidates {
        let parts: Vec<&str> = candidate.handle.rsplitn(3, ':').collect();
        assert_eq!(parts.len(), 3, "{}", candidate.handle);
        assert!(parts[0].parse::<u32>().is_ok(), "{}", candidate.handle);
        assert!(parts[1].parse::<u32>().is_ok(), "{}", candidate.handle);
        assert!(
            root.join(parts[2]).is_file(),
            "the handle names {} which is not a file of this tree",
            parts[2]
        );
    }
}

#[test]
fn a_name_that_really_is_unique_is_not_called_ambiguous() {
    // **The control.** Without it a provider that answered `Several` to every
    // question would pass the test above, and the whole programming face would
    // stop working while looking careful.
    if !support::analyzer_or_skip("that a unique name still resolves") {
        return;
    }
    let (_held, root) = support::tree("three-configs");
    let mut provider = provider(&root);

    let (resolution, _, _) = provider.known("Unmistakable").expect("an answer");

    let known = match &resolution {
        Resolution::One { known } => known,
        other => panic!("`Unmistakable` is declared once and came back as {other:?}"),
    };
    assert_eq!(known.kind, "struct");
    assert_eq!(known.package.as_deref(), Some("gamma"));
    assert!(
        !known.used.is_empty(),
        "a single resolution still carries its use sites: {known:?}"
    );
}

#[test]
fn a_name_nothing_declares_is_nothing_and_not_an_ambiguity() {
    // The third of the three answers, and the one that is easiest to lose:
    // "nothing" and "several" are both "not one", and a caller that could only
    // tell them apart by counting an empty list would read a typo as a choice
    // it has to make.
    if !support::analyzer_or_skip("that an absent name is absent") {
        return;
    }
    let (_held, root) = support::tree("three-configs");
    let mut provider = provider(&root);

    let (resolution, _, _) = provider.known("NothingIsCalledThis").expect("an answer");

    assert_eq!(resolution, Resolution::Nothing, "{resolution:?}");
    assert!(!resolution.is_ambiguous());
    assert!(resolution.only().is_none());
}

#[test]
fn what_is_remembered_about_an_ambiguous_name_is_still_ambiguous() {
    // The cache is the place this contract could be lost quietly. If the second
    // question came back from `thalyx-know` as a single `Config`, the refusal
    // would hold for the first call of a session and not for the second — which
    // is the worst possible schedule for it, because the first call is the one
    // somebody is watching.
    if !support::analyzer_or_skip("that an ambiguity survives being remembered") {
        return;
    }
    let (_held, root) = support::tree("three-configs");
    let store = tempfile::tempdir().expect("a temp dir");
    let database = store.path().join("known.db");

    let first = {
        let mut provider = Provider::open(&root, Knowledge::open(&database).expect("a store"));
        let (resolution, _, _) = provider.known("Config").expect("an answer");
        assert!(resolution.is_ambiguous(), "{resolution:?}");
        resolution
    };

    // A second provider over the same store, and no rust-analyzer question:
    // `hits` proves it came from memory rather than from a second 25-second
    // start, which is the difference between testing the cache and testing the
    // analyzer twice.
    let mut second = Provider::open(&root, Knowledge::open(&database).expect("a store"));
    let (again, _, _) = second.known("Config").expect("an answer");

    assert_eq!(again, first, "the remembered answer is a different answer");
    assert!(again.is_ambiguous(), "{again:?}");
    assert_eq!(
        second.tally.hits, 1,
        "the second question started an analyzer instead of reading what was kept"
    );
    assert!(!second.analyzer_running());
}

#[test]
fn the_sentence_an_ambiguity_produces_names_every_candidate() {
    // A refusal a model can act on. It has to say how many, which ones, and —
    // the part that decides whether the next call succeeds — the shape of the
    // handle that resolves it.
    let resolution = Resolution::Several {
        candidates: vec![
            thalyx_rust::Candidate {
                name: "Config".into(),
                kind: "struct".into(),
                package: Some("alpha".into()),
                container: None,
                at: thalyx_rust::At {
                    path: "alpha/src/lib.rs".into(),
                    line: 2,
                    column: 12,
                },
                signature: None,
                handle: "alpha/src/lib.rs:2:12".into(),
            },
            thalyx_rust::Candidate {
                name: "Config".into(),
                kind: "struct".into(),
                package: Some("beta".into()),
                container: None,
                at: thalyx_rust::At {
                    path: "beta/src/lib.rs".into(),
                    line: 4,
                    column: 12,
                },
                signature: None,
                handle: "beta/src/lib.rs:4:12".into(),
            },
        ],
    };

    let said = resolution.ambiguity("Config");
    assert!(said.contains("alpha/src/lib.rs:2:12"), "{said}");
    assert!(said.contains("beta/src/lib.rs:4:12"), "{said}");
    assert!(said.contains("alpha"), "{said}");
    assert!(said.contains("path:line:column"), "{said}");
    assert!(
        !said.contains("probably") && !said.contains("likely"),
        "the refusal is hedging towards a candidate: {said}"
    );
}
