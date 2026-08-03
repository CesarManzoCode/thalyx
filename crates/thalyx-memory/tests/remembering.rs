//! What the memory promises, checked end to end.
//!
//! Every test here is about one of the two decreed properties: the layers
//! cannot be confused for each other, and a fact stops being checkable without
//! ever becoming false.

use thalyx_memory::{Layer, LexicalEmbedder, Memory, Standing, Witness};

fn memory() -> Memory {
    Memory::in_memory(&LexicalEmbedder).expect("memory")
}

#[test]
fn facts_and_notes_come_back_in_different_places() {
    // The separation is in the types, not in a field. A caller that wants to
    // say "you installed X" and "maybe do Y" in one breath has to reach into
    // two different collections to do it.
    let memory = memory();

    memory
        .remember_fact(
            "refactor-auth",
            "installed org.publisher.pyassist 2.3.1",
            &Witness::nothing(),
            &LexicalEmbedder,
        )
        .unwrap();
    memory
        .note(
            "refactor-auth",
            "possible next step: ask whether to configure it",
            &LexicalEmbedder,
        )
        .unwrap();

    let recalled = memory.recall("refactor-auth").unwrap();

    assert_eq!(recalled.facts.len(), 1);
    assert_eq!(recalled.notes.len(), 1);
    assert_eq!(recalled.facts[0].record.layer, Layer::Fact);
    assert_eq!(recalled.notes[0].layer, Layer::Note);
}

#[test]
fn a_fact_about_an_untouched_file_still_checks_out() {
    let dir = tempfile::tempdir().unwrap();
    let subject = dir.path().join("auth.rs");
    std::fs::write(&subject, "fn login() {}\n").unwrap();

    let memory = memory();
    memory
        .remember_fact(
            "refactor-auth",
            "moved login() into auth.rs",
            &Witness::over([&subject]),
            &LexicalEmbedder,
        )
        .unwrap();

    let recalled = memory.recall("refactor-auth").unwrap();
    assert_eq!(recalled.facts[0].standing, Standing::Verified);
    assert_eq!(recalled.verified().count(), 1);
}

#[test]
fn the_human_editing_the_file_makes_the_fact_unverifiable_and_keeps_it() {
    // The double-route principle in one test: the human changed something
    // without telling the agent. The record survives, and it stops being
    // something the agent may assert.
    let dir = tempfile::tempdir().unwrap();
    let subject = dir.path().join("auth.rs");
    std::fs::write(&subject, "fn login() {}\n").unwrap();

    let memory = memory();
    memory
        .remember_fact(
            "refactor-auth",
            "moved login() into auth.rs",
            &Witness::over([&subject]),
            &LexicalEmbedder,
        )
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&subject, "fn login() { /* rewritten by hand */ }\n").unwrap();

    let recalled = memory.recall("refactor-auth").unwrap();

    assert_eq!(recalled.facts.len(), 1, "the fact must not be deleted");
    assert_eq!(recalled.verified().count(), 0);
    assert_eq!(recalled.no_longer_verifiable().count(), 1);

    let described = recalled.facts[0].standing.describe();
    assert!(described.contains("NO LONGER VERIFIABLE"), "{described}");
    assert!(described.contains("auth.rs"), "{described}");
}

#[test]
fn notes_can_be_dropped_and_facts_cannot() {
    // Inference belongs to the agent; the record of what happened does not.
    // There is no API to delete a fact, and this is the test that says so.
    let memory = memory();

    memory
        .remember_fact(
            "task",
            "this happened",
            &Witness::nothing(),
            &LexicalEmbedder,
        )
        .unwrap();
    memory
        .note("task", "and maybe this next", &LexicalEmbedder)
        .unwrap();
    memory
        .note("task", "or possibly this", &LexicalEmbedder)
        .unwrap();

    assert_eq!(memory.forget_notes("task").unwrap(), 2);

    let recalled = memory.recall("task").unwrap();
    assert_eq!(recalled.notes.len(), 0);
    assert_eq!(recalled.facts.len(), 1);
}

#[test]
fn memory_survives_the_process_that_wrote_it() {
    // The whole point of the primitive: the agent comes back days later.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory.db");
    let subject = dir.path().join("tracked");
    std::fs::write(&subject, "state").unwrap();

    {
        let memory = Memory::open(&path, &LexicalEmbedder).unwrap();
        memory
            .remember_fact(
                "long-task",
                "checkpoint reached",
                &Witness::over([&subject]),
                &LexicalEmbedder,
            )
            .unwrap();
        memory
            .note("long-task", "resume from the checkpoint", &LexicalEmbedder)
            .unwrap();
    }

    let reopened = Memory::open(&path, &LexicalEmbedder).unwrap();
    let recalled = reopened.recall("long-task").unwrap();

    assert_eq!(recalled.facts.len(), 1);
    assert_eq!(recalled.notes.len(), 1);
    assert_eq!(recalled.facts[0].standing, Standing::Verified);
    assert_eq!(reopened.tasks().unwrap(), vec!["long-task"]);
}

#[test]
fn search_finds_records_and_says_what_kind_of_matching_it_did() {
    // The rows cannot be had without the caveat — the same shape as the
    // graph's freshness. Today the caveat is that this is word overlap and
    // not meaning.
    let memory = memory();

    memory
        .remember_fact(
            "refactor-auth",
            "moved the login handler into the auth module",
            &Witness::nothing(),
            &LexicalEmbedder,
        )
        .unwrap();
    memory
        .remember_fact(
            "build-iso",
            "wrote the bootloader configuration",
            &Witness::nothing(),
            &LexicalEmbedder,
        )
        .unwrap();

    let recall = memory
        .search("auth module login", 5, &LexicalEmbedder)
        .unwrap();

    assert!(!recall.semantic, "the shipped embedder is lexical");
    assert!(
        recall.describe().contains("not by meaning"),
        "{}",
        recall.describe()
    );
    assert!(!recall.hits.is_empty());
    assert!(
        recall.hits[0].record.text.contains("auth module"),
        "the closest hit was {:?}",
        recall.hits[0].record.text
    );
}

#[test]
fn search_carries_the_standing_of_every_fact_it_returns() {
    // A search result is a place an agent could quote a fact from, so it needs
    // the same caveat `recall` attaches. Without it the honesty would hold on
    // one path and leak on the other.
    let dir = tempfile::tempdir().unwrap();
    let subject = dir.path().join("subject");
    std::fs::write(&subject, "before").unwrap();

    let memory = memory();
    memory
        .remember_fact(
            "task",
            "recorded against a file",
            &Witness::over([&subject]),
            &LexicalEmbedder,
        )
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&subject, "after the human edited it").unwrap();

    let recall = memory
        .search("recorded against a file", 5, &LexicalEmbedder)
        .unwrap();
    assert_eq!(recall.hits.len(), 1);
    assert!(matches!(
        recall.hits[0].standing,
        Some(Standing::Unverified { .. })
    ));
}

#[test]
fn a_note_in_search_results_carries_no_standing_because_there_is_nothing_to_check() {
    let memory = memory();
    memory
        .note("task", "an inference about the module", &LexicalEmbedder)
        .unwrap();

    let recall = memory
        .search("inference about the module", 5, &LexicalEmbedder)
        .unwrap();
    assert_eq!(recall.hits.len(), 1);
    assert!(recall.hits[0].standing.is_none());
}

#[test]
fn records_with_nothing_in_common_are_left_out_rather_than_ranked_last() {
    // Returning them padded the list with noise ranked above nothing at all,
    // which reads as "here is what I found" when nothing was found.
    let memory = memory();
    memory
        .remember_fact(
            "task",
            "compile the kernel",
            &Witness::nothing(),
            &LexicalEmbedder,
        )
        .unwrap();

    let recall = memory
        .search("entirely unrelated vocabulary here", 5, &LexicalEmbedder)
        .unwrap();
    assert!(recall.hits.is_empty());
}

#[test]
fn a_memory_written_by_one_embedder_refuses_to_be_read_by_another() {
    // Stored vectors mean nothing to a different embedder, and searching
    // anyway would return confident nonsense rather than an error.
    struct Impostor;
    impl thalyx_memory::Embedder for Impostor {
        fn dimensions(&self) -> usize {
            8
        }
        fn embed(&self, _text: &str) -> thalyx_memory::Embedding {
            thalyx_memory::Embedding::new(vec![1.0; 8])
        }
        fn is_semantic(&self) -> bool {
            true
        }
        fn name(&self) -> &str {
            "pretend-model-v9"
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory.db");

    {
        let memory = Memory::open(&path, &LexicalEmbedder).unwrap();
        memory
            .remember_fact("task", "something", &Witness::nothing(), &LexicalEmbedder)
            .unwrap();
    }

    let refused = Memory::open(&path, &Impostor);
    assert!(matches!(
        refused,
        Err(thalyx_memory::MemoryError::DifferentEmbedder { .. })
    ));
}

#[test]
fn recall_of_a_task_nobody_recorded_is_empty_rather_than_an_error() {
    let memory = memory();
    assert!(memory.recall("never-happened").unwrap().is_empty());
}
