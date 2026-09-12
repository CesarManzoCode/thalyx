//! What `managed-local-v1` promises, asked of the Linux store through the same
//! client a transaction uses.
//!
//! Every claim here is checked from outside the thing making it: trees are
//! compared by walking the directories, generations by asking the store after
//! reopening it, and a stop is a store that is dropped and opened again — which
//! is when recovery really runs.

use std::path::Path;
use thalyx_managed::{Faults, LinuxStore};
use thalyx_platform::authority::RequestId;
use thalyx_platform::content::{Manifest, object_digest};
use thalyx_platform::evidence::{EvidenceSink, Fetched};
use thalyx_platform::managed::client::Managed;
use thalyx_platform::managed::protocol::{self, ObjectKind, Reply, Request};
use thalyx_platform::state::{AbandonFailure, Decision, Receipt, VersionedState};
use thalyx_platform::transport::{Loopback, Service};

type Client = Managed<Loopback<LinuxStore>>;

struct Place {
    _base: tempfile::TempDir,
    store: std::path::PathBuf,
    view: std::path::PathBuf,
    workspace: std::path::PathBuf,
}

fn place(files: &[(&str, &str)]) -> Place {
    let base = tempfile::tempdir().expect("a directory");
    let view = base.path().join("view");
    for (path, text) in files {
        let full = view.join(path);
        std::fs::create_dir_all(full.parent().expect("a parent")).expect("parents");
        std::fs::write(full, text).expect("a file");
    }
    std::fs::create_dir_all(&view).expect("the view");
    Place {
        store: base.path().join("store"),
        workspace: base.path().join("workspace"),
        view,
        _base: base,
    }
}

fn client(place: &Place, faults: Faults) -> Client {
    let store = LinuxStore::open(&place.store, &["session"])
        .expect("a store")
        .with_faults(faults);
    Managed::new(
        Loopback::new(store),
        &place.view,
        &place.workspace,
        "session",
    )
}

fn receipt(transaction: &str, decision: Decision) -> Receipt {
    Receipt {
        transaction: transaction.to_string(),
        label: "test".to_string(),
        decision,
        succeeded: decision == Decision::Commit,
        checks: Vec::new(),
        program: None,
    }
}

fn id_of(path: &Path) -> String {
    Manifest::of(path).id().expect("a readable tree")
}

fn ask(store: &mut LinuxStore, request: &Request) -> Reply {
    let bytes = store.serve(&protocol::encode(request)).expect("an answer");
    protocol::decode(&bytes).expect("a reply")
}

#[test]
fn work_is_private_until_it_is_published_and_then_the_view_holds_exactly_it() {
    let place = place(&[("src/lib.rs", "pub struct Keystore;\n")]);
    let found = id_of(&place.view);
    let mut client = client(&place, Faults::default());

    let opened = client.open("rename", "t-1").expect("opened");
    assert_eq!(opened.base, "generation-1");
    assert_eq!(opened.start_state.as_deref(), Some(found.as_str()));
    assert_eq!(id_of(&place.workspace), found, "the fork holds the seed");

    std::fs::write(place.workspace.join("src/lib.rs"), "pub struct KeyVault;\n").expect("edit");
    assert_eq!(
        id_of(&place.view),
        found,
        "nothing published sees private work"
    );
    let changes = client.changed();
    assert_eq!(changes.modified, vec!["src/lib.rs"]);

    let candidate = client.candidate().expect("frozen").expect("a candidate");
    let kept = client
        .keep(&receipt("t-1", Decision::Commit))
        .expect("published");
    assert_eq!(kept.generation, Some(2));
    assert_eq!(kept.root.as_deref(), Some(candidate.as_str()));
    assert_eq!(
        id_of(&place.view),
        candidate,
        "the view is the published root"
    );
    assert_eq!(client.end_state().as_deref(), Some(candidate.as_str()));
}

#[test]
fn abandoning_discards_the_private_work_and_moves_nothing_anybody_can_see() {
    let place = place(&[("a.txt", "one\n")]);
    let found = id_of(&place.view);
    let mut client = client(&place, Faults::default());
    client.open("try", "t-1").expect("opened");
    std::fs::write(place.workspace.join("b.txt"), "two\n").expect("add");
    client.candidate().expect("frozen");

    client
        .abandon(&receipt("t-1", Decision::Rollback))
        .expect("abandoned");
    assert_eq!(id_of(&place.workspace), found, "the workspace went back");
    assert_eq!(id_of(&place.view), found);
    assert_eq!(
        client.published().expect("asked").0,
        1,
        "no generation was made"
    );
}

#[test]
fn a_workspace_written_after_the_rollback_was_authorised_is_not_destroyed() {
    let place = place(&[("a.txt", "one\n")]);
    let mut client = client(&place, Faults::default());
    client.open("try", "t-1").expect("opened");
    std::fs::write(place.workspace.join("a.txt"), "two\n").expect("edit");
    client.candidate().expect("frozen");
    // Somebody with the path writes after the state was taken.
    std::fs::write(place.workspace.join("a.txt"), "three\n").expect("a later write");

    match client.abandon(&receipt("t-1", Decision::Rollback)) {
        Err(AbandonFailure::NotPutBack(why)) => assert!(why.contains("written to"), "{why}"),
        other => panic!("the abandon went ahead: {other:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(place.workspace.join("a.txt")).expect("read"),
        "three\n"
    );
}

#[test]
fn a_view_written_outside_the_store_is_refused_and_not_overwritten() {
    let place = place(&[("a.txt", "one\n")]);
    {
        let mut client = client(&place, Faults::default());
        client.open("first", "t-1").expect("opened");
        client
            .keep(&receipt("t-1", Decision::Commit))
            .expect("kept");
    }
    std::fs::write(place.view.join("a.txt"), "a person's edit\n").expect("an edit");

    let mut client = client(&place, Faults::default());
    let refused = client.open("second", "t-2").expect_err("refused");
    assert!(refused.contains("written outside the store"), "{refused}");
    assert_eq!(
        std::fs::read_to_string(place.view.join("a.txt")).expect("read"),
        "a person's edit\n",
        "the refusal must not cost the person their edit"
    );
}

#[test]
fn a_publication_stopped_after_prepare_did_not_happen_and_recovery_says_so() {
    let place = place(&[("a.txt", "one\n")]);
    let found = id_of(&place.view);
    {
        let mut client = client(
            &place,
            Faults {
                stop_after_prepare: true,
                ..Faults::default()
            },
        );
        client.open("cut", "t-1").expect("opened");
        std::fs::write(place.workspace.join("a.txt"), "two\n").expect("edit");
        let lost = client
            .keep(&receipt("t-1", Decision::Commit))
            .expect_err("nobody answered");
        assert!(lost.contains("is not known"), "{lost}");
    }

    let mut store = LinuxStore::open(&place.store, &["session"]).expect("reopened");
    assert!(store.recovery().aborted_prepare, "{:?}", store.recovery());
    assert_eq!(store.generation(), 1);
    // The publish was sequence 3 of `session`: seed, fork, publish.
    match ask(
        &mut store,
        &Request::Result {
            request: RequestId {
                principal: "session".to_string(),
                sequence: 3,
            },
        },
    ) {
        Reply::Aborted { reason } => assert!(reason.contains("recovered"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(id_of(&place.view), found);
}

#[test]
fn a_publication_whose_reply_was_lost_is_committed_and_the_view_catches_up() {
    let place = place(&[("a.txt", "one\n")]);
    let published;
    {
        let mut client = client(
            &place,
            Faults {
                stop_after_commit: true,
                ..Faults::default()
            },
        );
        client.open("cut", "t-1").expect("opened");
        std::fs::write(place.workspace.join("a.txt"), "two\n").expect("edit");
        published = client.candidate().expect("frozen").expect("a candidate");
        client
            .keep(&receipt("t-1", Decision::Commit))
            .expect_err("the reply was lost");
    }

    let mut client = client(&place, Faults::default());
    assert_eq!(
        client.published().expect("asked"),
        (2, Some(published.clone())),
        "the commit was durable"
    );
    // The view still holds generation 1, which this store published: behind,
    // not written, and brought up rather than refused.
    client
        .open("after", "t-2")
        .expect("opened after catching up");
    assert_eq!(id_of(&place.view), published);
}

#[test]
fn a_rival_that_forked_the_same_generation_is_stale_and_changes_nothing() {
    let place = place(&[("a.txt", "one\n")]);
    let mut store = LinuxStore::open(&place.store, &["pub", "riv"]).expect("a store");

    let tree = Manifest::of(&place.view);
    for path in tree.nodes.keys() {
        let bytes = std::fs::read(place.view.join(path)).expect("read");
        ask(
            &mut store,
            &Request::Put {
                kind: ObjectKind::Bytes,
                hex: hex::encode(bytes),
            },
        );
    }
    ask(
        &mut store,
        &Request::Put {
            kind: ObjectKind::Tree,
            hex: hex::encode(tree.encode()),
        },
    );
    let receipt_bytes = b"{}".to_vec();
    let receipt = object_digest("receipt", &receipt_bytes);
    ask(
        &mut store,
        &Request::Put {
            kind: ObjectKind::Receipt,
            hex: hex::encode(&receipt_bytes),
        },
    );
    let root = tree.id().expect("an id");
    let request = |principal: &str, sequence| RequestId {
        principal: principal.to_string(),
        sequence,
    };

    assert!(matches!(
        ask(
            &mut store,
            &Request::Seed {
                request: request("pub", 1),
                root: root.clone(),
                receipt: receipt.clone()
            }
        ),
        Reply::Committed { generation: 1, .. }
    ));
    for (who, transaction) in [("pub", "p"), ("riv", "r")] {
        let sequence = if who == "pub" { 2 } else { 1 };
        assert!(matches!(
            ask(
                &mut store,
                &Request::Fork {
                    request: request(who, sequence),
                    transaction: transaction.to_string(),
                    generation: 1
                }
            ),
            Reply::Forked { generation: 1 }
        ));
    }

    let winner = Request::Publish {
        request: request("pub", 3),
        transaction: "p".to_string(),
        expected_generation: 1,
        candidate: root.clone(),
        receipt: receipt.clone(),
    };
    assert!(matches!(
        ask(&mut store, &winner),
        Reply::Committed { generation: 2, .. }
    ));
    let loser = Request::Publish {
        request: request("riv", 2),
        transaction: "r".to_string(),
        expected_generation: 1,
        candidate: root.clone(),
        receipt: receipt.clone(),
    };
    assert_eq!(
        ask(&mut store, &loser),
        Reply::Stale {
            expected: 1,
            current: 2
        }
    );
    assert_eq!(store.generation(), 2);

    // A retry is answered from the log, not carried out again.
    assert!(matches!(
        ask(&mut store, &winner),
        Reply::Committed { generation: 2, .. }
    ));
    assert_eq!(store.generation(), 2, "the retry published nothing");

    // And the same sequence for a different request is a conflict.
    let reused = Request::Publish {
        request: request("pub", 3),
        transaction: "p".to_string(),
        expected_generation: 2,
        candidate: root.clone(),
        receipt: receipt.clone(),
    };
    assert!(matches!(
        ask(&mut store, &reused),
        Reply::Refused { word, .. } if word == "conflict"
    ));

    // Somebody the store does not name cannot publish at all.
    let stranger = Request::Seed {
        request: request("stranger", 1),
        root,
        receipt,
    };
    assert!(matches!(
        ask(&mut store, &stranger),
        Reply::Refused { word, .. } if word == "forbidden"
    ));
}

#[test]
fn a_torn_last_record_is_cut_and_damage_before_the_end_refuses_the_store() {
    let place = place(&[("a.txt", "one\n")]);
    {
        let mut client = client(&place, Faults::default());
        client.open("first", "t-1").expect("opened");
        client
            .keep(&receipt("t-1", Decision::Commit))
            .expect("kept");
    }
    let log = place.store.join(thalyx_managed::LOG);
    let whole = std::fs::read(&log).expect("the log");

    // A write that never finished.
    let mut torn = whole.clone();
    torn.extend_from_slice(b"{\"seq\":99,\"prev\":\"00");
    std::fs::write(&log, &torn).expect("tear it");
    {
        let store = LinuxStore::open(&place.store, &["session"]).expect("reopened");
        assert!(
            store.recovery().truncated_bytes > 0,
            "{:?}",
            store.recovery()
        );
        assert!(store.recovery().integrity.is_none());
        assert_eq!(store.generation(), 2);
    }
    assert_eq!(
        std::fs::read(&log).expect("the log"),
        whole,
        "cut back to the last record"
    );

    // Damage in the middle is not a crash.
    let text = String::from_utf8(whole).expect("text");
    let damaged = text.replacen("\"generation\":1", "\"generation\":7", 1);
    assert_ne!(damaged, text, "the damage has to land somewhere");
    std::fs::write(&log, damaged).expect("damage it");
    let mut store = LinuxStore::open(&place.store, &["session"]).expect("opened, and refusing");
    assert!(store.recovery().integrity.is_some());
    assert!(matches!(
        ask(&mut store, &Request::Published),
        Reply::Refused { word, .. } if word == "integrity"
    ));
}

#[test]
fn an_object_rewritten_behind_the_services_back_is_refused_when_it_is_read() {
    let place = place(&[("a.txt", "one\n")]);
    let digest;
    {
        let mut client = client(&place, Faults::default());
        client.open("first", "t-1").expect("opened");
        digest = match Manifest::of(&place.view).nodes.get("a.txt") {
            Some(thalyx_platform::content::Node::File { digest, .. }) => digest.clone(),
            other => panic!("{other:?}"),
        };
        client
            .abandon(&receipt("t-1", Decision::Rollback))
            .expect("abandoned");
    }
    let object = place
        .store
        .join(thalyx_managed::OBJECTS)
        .join(&digest[..2])
        .join(format!("{digest}.bytes"));
    std::fs::write(&object, "not one\n").expect("a foreign write");

    let mut store = LinuxStore::open(&place.store, &["session"]).expect("reopened");
    assert!(matches!(
        ask(&mut store, &Request::Get { digest }),
        Reply::Refused { word, .. } if word == "integrity"
    ));
}

#[test]
fn evidence_outlives_the_work_it_describes_and_is_found_by_its_handle() {
    let place = place(&[("a.txt", "one\n")]);
    let body = br#"{"transaction":"t-1","status":"rolled_back"}"#;
    {
        let mut client = client(&place, Faults::default());
        client.open("try", "t-1").expect("opened");
        std::fs::write(place.workspace.join("a.txt"), "two\n").expect("edit");
        client.candidate().expect("frozen");
        client
            .abandon(&receipt("t-1", Decision::Rollback))
            .expect("abandoned");
        client.record("t-1", body).expect("kept");
        assert_eq!(client.fetch("t-1"), Fetched::Found(body.to_vec()));
    }
    assert_eq!(
        thalyx_managed::read_evidence(&place.store, "t-1"),
        Fetched::Found(body.to_vec()),
        "a reader that opens no service finds it too"
    );
    assert_eq!(
        thalyx_managed::read_evidence(&place.store, "t-2"),
        Fetched::Absent
    );
}
