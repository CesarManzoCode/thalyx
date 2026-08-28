//! Ten small trees whose right answers are written down, and the index run
//! against every one of them.
//!
//! ## Why this exists
//!
//! Because the alternative was a model. The claim this whole project rests on
//! is that a semantic index makes an agent better at programming than `grep`
//! does, and the only way that had ever been checked was to hand a task to
//! Claude and read what happened — which costs money, takes minutes, varies
//! between runs, and answers *how did that session go* rather than *what does
//! the index actually know*.
//!
//! These fixtures answer the second question for nothing. Each is a handful of
//! files small enough to hold in your head, each was written to put one shape
//! of dependency in front of the index, and each carries an `expected.json`
//! whose contents were worked out by reading the source and not by running the
//! code. A run takes milliseconds and the answer is a table.
//!
//! ## Exact sets, in both directions
//!
//! The expectations are equalities and not "must contain". A corpus that only
//! checked for the rows it wanted would pass just as happily on an index that
//! returned every file in the tree — and returning too much is the failure mode
//! a symbol-level index has, not returning too little. Two of the ten fixtures
//! exist only to be answered *narrowly*: `08-ambiguous`, where the right answer
//! is to refuse, and `09-noise`, where the right answer is to ignore four
//! shapes of text that are not code.
//!
//! ## Known limits are stated, not hidden
//!
//! A fixture may carry `known_limits`: something true about the tree that the
//! index does not know. The test asserts the limit is *still* a limit — so that
//! fixing it shows up here as loudly as breaking something else — prints
//! `NOT PROVEN` for it, and `THALYX_REQUIRE_FULL_CORPUS=1` turns those prints
//! into failures. `Estrategia-de-Pruebas.md` rule 3: a test that skips has to
//! say so, and there is a variable that demands what it skipped.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use thalyx_graph::Index;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

/// Every fixture, in name order, so the scoreboard reads the same every run.
fn fixtures() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(corpus_root())
        .expect("the corpus directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.join("expected.json").is_file())
        .collect();
    found.sort();
    found
}

/// `path:line via through` — one string per row, so a difference reads as a
/// difference rather than as two structs to compare by eye.
fn dependent_rows(index: &Index, path: &str) -> BTreeSet<String> {
    index
        .dependents_of(path)
        .expect("the index answers")
        .rows
        .into_iter()
        .map(|edge| match edge.via {
            thalyx_graph::Via::Import => format!("{} via import", edge.from),
            thalyx_graph::Via::Symbol => {
                format!("{} via symbol through {}", edge.from, edge.raw_target)
            }
        })
        .collect()
}

fn expected_dependent_rows(rows: &Value) -> BTreeSet<String> {
    rows.as_array()
        .expect("dependents are a list")
        .iter()
        .map(|row| {
            let from = row["from"].as_str().expect("from");
            match row["via"].as_str().expect("via") {
                "import" => format!("{from} via import"),
                "symbol" => format!(
                    "{from} via symbol through {}",
                    row["through"]
                        .as_str()
                        .expect("a symbol edge names its symbol")
                ),
                other => panic!("`via` is import or symbol, and was `{other}`"),
            }
        })
        .collect()
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().expect("a string").to_string())
        .collect()
}

#[test]
fn every_fixture_answers_exactly_what_it_says_it_should() {
    let demanded = std::env::var("THALYX_REQUIRE_FULL_CORPUS").is_ok();
    let mut unproven: Vec<String> = Vec::new();
    let mut checks = 0usize;

    eprintln!();
    eprintln!("  the corpus, {} fixtures", fixtures().len());
    eprintln!();

    for fixture in fixtures() {
        let name = fixture.file_name().unwrap().to_string_lossy().into_owned();
        let expected: Value =
            serde_json::from_str(&std::fs::read_to_string(fixture.join("expected.json")).unwrap())
                .unwrap_or_else(|why| panic!("{name}: expected.json is not JSON: {why}"));

        let mut index = Index::in_memory(&fixture.join("tree")).expect("an index over the tree");
        index.build().expect("the tree indexes");

        if let Some(dependents) = expected.get("dependents") {
            for (path, rows) in dependents.as_object().expect("dependents is an object") {
                let want = expected_dependent_rows(rows);
                let got = dependent_rows(&index, path);
                assert_eq!(
                    got, want,
                    "\n{name}: what depends on `{path}`\n  expected {want:?}\n  got      {got:?}\n"
                );
                checks += 1;
            }
        }

        if let Some(symbols) = expected.get("symbols") {
            for (symbol, wanted) in symbols.as_object().expect("symbols is an object") {
                let answer = index.symbol(symbol).expect("the index answers");

                let definitions: BTreeSet<String> = answer
                    .rows
                    .definitions
                    .iter()
                    .map(|found| format!("{}:{} {}", found.path, found.line, found.kind))
                    .collect();
                assert_eq!(
                    definitions,
                    strings(&wanted["definitions"]),
                    "\n{name}: where `{symbol}` is defined"
                );

                let uses: BTreeSet<String> = answer
                    .rows
                    .uses
                    .iter()
                    .map(|used| format!("{}:{}", used.path, used.line))
                    .collect();
                assert_eq!(
                    uses,
                    strings(&wanted["uses"]),
                    "\n{name}: where `{symbol}` is used"
                );
                checks += 2;
            }
        }

        // A limit is only a limit while it is still true. Asserting that the
        // row is *absent* is what makes an improvement show up here instead of
        // quietly leaving a stale caveat in the corpus.
        if let Some(limits) = expected.get("known_limits") {
            for limit in limits.as_array().expect("known_limits is a list") {
                let symbol = limit["symbol"].as_str().expect("which symbol");
                let answer = index.symbol(symbol).expect("the index answers");
                let uses: BTreeSet<String> = answer
                    .rows
                    .uses
                    .iter()
                    .map(|used| format!("{}:{}", used.path, used.line))
                    .collect();
                for missing in strings(&limit["uses_missing"]) {
                    assert!(
                        !uses.contains(&missing),
                        "\n{name}: `{missing}` is recorded as a known limit and the index now \
                         finds it. That is good news — update expected.json so the corpus stops \
                         claiming otherwise.\n"
                    );
                }
                unproven.push(format!(
                    "{name}: {}",
                    limit["about"].as_str().unwrap_or("a stated limit")
                ));
            }
        }

        eprintln!(
            "    {name:<18} {:<16} {}",
            expected["category"].as_str().unwrap_or("—"),
            expected["about"].as_str().unwrap_or("")
        );
    }

    eprintln!();
    eprintln!("  {checks} exact answers checked");

    if unproven.is_empty() {
        eprintln!();
        return;
    }

    eprintln!();
    for limit in &unproven {
        eprintln!("  NOT PROVEN  {limit}");
    }
    eprintln!();
    assert!(
        !demanded,
        "{} stated limit(s) and THALYX_REQUIRE_FULL_CORPUS=1 demands them",
        unproven.len()
    );
}

#[test]
fn the_corpus_covers_the_shapes_it_claims_to() {
    // Pinned so that deleting a fixture is a decision rather than an accident.
    // Every category here is a way one file comes to depend on another that an
    // import list does not state, or a way an index that guessed would be
    // wrong — which are the only two things this corpus is for.
    let mut categories: Vec<String> = fixtures()
        .iter()
        .map(|fixture| {
            let expected: Value = serde_json::from_str(
                &std::fs::read_to_string(fixture.join("expected.json")).unwrap(),
            )
            .unwrap();
            expected["category"].as_str().unwrap().to_string()
        })
        .collect();
    categories.sort();
    categories.dedup();

    for wanted in [
        "direct call",
        "field access",
        "method",
        "trait",
        "alias / import",
        "re-export",
        "module",
        "generic",
        "precision guard",
    ] {
        assert!(
            categories.contains(&wanted.to_string()),
            "the corpus no longer covers `{wanted}`"
        );
    }
}
