//! The claim this whole crate exists to make good on.
//!
//! `crates/thalyx-graph/corpus/05-alias/expected.json` has carried a
//! `known_limits` entry since the index was written: *«`Keys` at src/boot.rs:3
//! is a use of `Keystore` and the index does not say so.»* The tree here is
//! that corpus case, made into a real Cargo package so that a real compiler
//! frontend can be asked about it.
//!
//! Every test below has the **control** rule 4 demands: the same question put
//! to the old scanner, in the same test, so that "the provider answers it" is a
//! comparison and not an assertion about a number nobody can place.

mod support;

use std::path::Path;
use thalyx_know::{Knowledge, Standing};
use thalyx_rust::Provider;

fn provider(root: &Path) -> Provider {
    Provider::open(root, Knowledge::in_memory().expect("a knowledge store"))
}

#[test]
fn the_index_cannot_say_that_keys_is_a_keystore() {
    // The baseline. Without it, "rust-analyzer resolves the alias" is a
    // sentence about a tool nobody compared against anything.
    let (_held, root) = support::tree("alias");
    let mut index = thalyx_graph::Index::in_memory(&root).expect("an index");
    index.build().expect("a build");

    let found = index.symbol("Keystore").expect("an answer");
    let uses = found.regardless_of_freshness().uses;
    let names_the_alias_use = uses
        .iter()
        .any(|use_here| use_here.path.ends_with("boot.rs") && use_here.line == 3);
    assert!(
        !names_the_alias_use,
        "the scan has started resolving aliases. If that is real, the corpus's \
         known_limits entry and this test are both out of date — but check \
         first that the fixture still spells the alias, because a test that \
         passes for a new reason is rule 5"
    );
}

#[test]
fn the_provider_says_keys_is_a_keystore() {
    if !support::analyzer_or_skip("that an alias resolves to what it names") {
        return;
    }
    let (_held, root) = support::tree("alias");
    let mut provider = provider(&root);

    // `Keys` in `pub fn boot() -> Keys`: line 3, column 22, one-based, which is
    // exactly the position the corpus says nobody could resolve.
    let boot = root.join("src").join("boot.rs");
    let found = provider
        .identity_at(&boot, 3, 22)
        .expect("rust-analyzer to answer");

    assert_eq!(
        found.len(),
        1,
        "one definition was expected and {found:?} came back"
    );
    assert!(
        found[0].path.ends_with("keystore.rs"),
        "the alias should resolve into keystore.rs, and resolved into {:?}",
        found[0]
    );
    assert_eq!(found[0].line, 1, "the declaration is on the first line");
}

#[test]
fn a_reference_list_includes_the_import_that_renames_it() {
    if !support::analyzer_or_skip("that references follow a renaming import") {
        return;
    }
    let (_held, root) = support::tree("alias");
    let mut provider = provider(&root);

    let (known, standing, source) = provider.known("Keystore").expect("an answer");
    let known = known.expect("Keystore is declared in this tree");

    assert_eq!(source, "rust-analyzer");
    assert_eq!(standing, Standing::Current);
    assert_eq!(known.kind, "struct");
    assert_eq!(known.package.as_deref(), Some("alias-fixture"));
    assert!(
        known
            .used
            .iter()
            .any(|at| at.path.ends_with("boot.rs") && at.line == 1),
        "the `use … as Keys` line is a use of Keystore, and the answer was {:?}",
        known.used
    );
}

#[test]
fn a_rename_is_described_and_nothing_is_written() {
    if !support::analyzer_or_skip("that a rename is planned without being applied") {
        return;
    }
    let (_held, root) = support::tree("alias");
    let keystore = root.join("src").join("keystore.rs");
    let before = std::fs::read_to_string(&keystore).expect("the file");

    let mut provider = provider(&root);
    let texts = provider
        .rename_texts(&keystore, 1, 12, "KeyVault")
        .expect("a plan");

    assert!(
        std::fs::read_to_string(&keystore).expect("the file") == before,
        "planning a rename wrote to the tree. The provider is a reader; the \
         authority above it is the only thing that writes"
    );

    let mut changed: Vec<String> = texts
        .iter()
        .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    changed.sort();
    assert_eq!(
        changed,
        vec!["boot.rs".to_string(), "keystore.rs".to_string()],
        "the alias site is in another file, and a rename that missed it would \
         leave a tree that does not compile"
    );

    let boot = texts
        .iter()
        .find(|(path, _)| path.ends_with("boot.rs"))
        .expect("boot.rs");
    assert!(
        boot.1.contains("use crate::keystore::KeyVault as Keys;"),
        "the import should be rewritten and the local alias left alone, and it \
         came back as:\n{}",
        boot.1
    );
    assert!(
        boot.1.contains("-> Keys"),
        "`Keys` is a different name and renaming `Keystore` must not touch it"
    );
}

#[test]
fn a_second_session_answers_from_what_the_first_learned() {
    if !support::analyzer_or_skip("that repository knowledge outlives a session") {
        return;
    }
    let (_held, root) = support::tree("alias");
    let store = root.join("..").join("knowledge.db");

    let first = {
        let mut provider = Provider::open(&root, Knowledge::open(&store).expect("a store"));
        let (known, _, _) = provider.known("Keystore").expect("an answer");
        assert_eq!(provider.tally.analyzer_starts, 1, "the first pays for it");
        known.expect("Keystore")
    };

    // A whole new Provider over a whole new connection: the only thing shared
    // is the file on disk, which is what "survives a session" means.
    let mut second = Provider::open(&root, Knowledge::open(&store).expect("a store"));
    let (again, standing, source) = second.known("Keystore").expect("an answer");

    assert_eq!(standing, Standing::Current);
    assert_eq!(
        source, "rust-analyzer",
        "the answer keeps saying who made it"
    );
    assert_eq!(again.expect("Keystore"), first);
    assert_eq!(
        second.tally.analyzer_starts, 0,
        "the second session started a rust-analyzer, which is the 25 seconds \
         this cache exists to not pay again"
    );
    assert_eq!(
        second.tally.hits, 2,
        "the workspace and the symbol, both known"
    );
}

#[test]
fn a_changed_source_file_makes_what_was_learned_stale() {
    if !support::analyzer_or_skip("that a changed tree invalidates a semantic answer") {
        return;
    }
    let (_held, root) = support::tree("alias");
    let store = root.join("..").join("knowledge.db");
    {
        let mut provider = Provider::open(&root, Knowledge::open(&store).expect("a store"));
        provider.known("Keystore").expect("an answer");
    }

    // One line, in a file the answer never mentioned. It is still a source file
    // of the same crate, and rust-analyzer resolves names using everything it
    // can see — so nothing narrower than "the sources moved" can be *proved*
    // to leave the answer standing.
    let lib = root.join("src").join("lib.rs");
    let text = std::fs::read_to_string(&lib).expect("the file");
    std::fs::write(&lib, format!("// a comment nobody asked for\n{text}")).expect("the write");

    let knowledge = Knowledge::open(&store).expect("a store");
    let mut provider = Provider::open(&root, knowledge);
    let witness = provider.source_witness().expect("a witness");
    let held = provider
        .knowledge()
        .recall(thalyx_rust::KIND_SYMBOL, "Keystore", &witness)
        .expect("a recall")
        .expect("something was remembered");

    assert!(
        matches!(held.standing, Standing::Stale { .. }),
        "the sources changed and the remembered answer still calls itself {}",
        held.standing.word()
    );
    assert!(
        provider
            .knowledge()
            .recall_current(thalyx_rust::KIND_SYMBOL, "Keystore", &witness)
            .expect("a recall")
            .is_none(),
        "a stale answer was handed out as current, which is the one thing this \
         store exists to make impossible"
    );
}
