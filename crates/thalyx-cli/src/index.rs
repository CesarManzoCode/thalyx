//! Asking about structure instead of grepping for it.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **C1**, and the one
//! Cesar named when he asked for the catalogue — *«así como los grafos»*.
//! [[FS-en-Grafo]] calls itself *the founding example of what it means for a
//! primitive to be native to the AI rather than inherited from a design meant
//! for humans*, and until this file it had never been reachable by anything
//! except Thalyx's own CLI.
//!
//! ## What it is for, in terms of the five costs
//!
//! *Who calls this function* is not a question a directory walk can answer,
//! because dependency is not a property of location. In Linux an agent asks it
//! with `grep -r`, which costs a few hundred lines of which two matter — the
//! second cost, context, paid in full — and which cannot tell a call from the
//! same word inside a comment, which is the third, ambiguity.
//!
//! ## The rule of honesty, which is not optional here
//!
//! Every answer carries the index's freshness **in the same object as the
//! rows**. That is the decreed rule of [[FS-en-Grafo]] and the reason it is
//! decreed: the index is a cache over a filesystem the human is free to change
//! without telling Thalyx, and separating the caveat from the data is how a
//! cache starts being mistaken for the truth.
//!
//! An agent that reads `stale` can decide what to do. One that was never told
//! will trust an answer about a tree that has moved on.

use crate::files::{Face, Where};
use serde_json::json;
use std::path::Path;
use thalyx_graph::{Freshness, Index};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// One index per tree, kept in the store rather than inside the tree itself.
///
/// The same rule and the same key as `thalyx graph`, deliberately: two places
/// computing where an index lives is two indexes, and the second one is always
/// the empty one somebody is confused by.
fn open(store_root: &Path, tree: &Path) -> Result<Index, Box<dyn std::error::Error>> {
    let key = tree
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let database = store_root
        .join("state")
        .join("graph")
        .join(format!("{key}.db"));
    Ok(Index::open(&database, tree)?)
}

/// Freshness as a caller reads it: a word to match on and a sentence to relay.
fn freshness_fields(freshness: &Freshness) -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "fresh",
            json!(if freshness.is_current() {
                "current"
            } else {
                "stale"
            }),
        ),
        // Present in both cases. A field that only appears on the bad day is a
        // field nobody handles on the bad day.
        ("freshness_detail", json!(freshness.describe())),
    ]
}

fn declined(face: Face, op: &str, word: &str, why: &str) {
    if face == Face::Machine {
        println!("{}", thalyx_files::machine::declined(op, word, why));
    } else {
        println!("\n  {why}\n");
    }
}

/// `indexar [ruta]` — read the tree and record what refers to what.
pub fn build(store_root: &Path, here: &Where, rest: &str, face: Face) -> Fallible {
    let tree = tree_of(here, rest);

    let mut index = match open(store_root, &tree) {
        Ok(index) => index,
        Err(error) => {
            declined(face, "index_build", "unreadable", &error.to_string());
            return Ok(());
        }
    };

    match index.build() {
        Ok(report) => {
            if face == Face::Machine {
                println!(
                    "{}",
                    thalyx_files::machine::answer(
                        "index_build",
                        vec![
                            ("tree", json!(tree.display().to_string())),
                            ("files_indexed", json!(report.files_indexed)),
                            ("files_parsed", json!(report.files_parsed)),
                            ("edges", json!(report.edges)),
                            ("edges_resolved", json!(report.edges_resolved)),
                            // Named rather than dropped: a file the parser did
                            // not understand is not a file with no dependencies,
                            // and a caller that read the second would conclude
                            // things about a tree it has not actually seen.
                            ("skipped", json!(report.skipped)),
                        ],
                    )
                );
            } else {
                println!();
                println!(
                    "  {} indexed, {} parsed",
                    report.files_indexed, report.files_parsed
                );
                println!(
                    "  {} references, {} of them inside the tree",
                    report.edges, report.edges_resolved
                );
                if report.skipped > 0 {
                    println!("  {} skipped — not a language I can read", report.skipped);
                }
                println!();
            }
        }
        Err(error) => declined(face, "index_build", "unreadable", &error.to_string()),
    }
    Ok(())
}

/// `depende <ruta>` and `usan <ruta>`.
///
/// The second is the question the index exists for. The first is answerable by
/// reading one file; the second is not answerable by reading any number of them
/// without reading all of them.
pub fn edges(store_root: &Path, here: &Where, rest: &str, incoming: bool, face: Face) -> Fallible {
    let op = if incoming {
        "depended_on_by"
    } else {
        "depends_on"
    };

    let path = rest.trim();
    if path.is_empty() {
        declined(face, op, "incomplete", "which file");
        return Ok(());
    }

    let tree = tree_of(here, "");
    let index = match open(store_root, &tree) {
        Ok(index) => index,
        Err(error) => {
            declined(face, op, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    let answer = if incoming {
        index.dependents_of(path)
    } else {
        index.dependencies_of(path)
    };
    let answer = match answer {
        Ok(answer) => answer,
        Err(error) => {
            declined(face, op, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    if face == Face::Machine {
        let rows: Vec<serde_json::Value> = answer
            .rows
            .iter()
            .map(|edge| {
                json!({
                    "from": edge.from,
                    // The reference as written and the file it resolves to are
                    // two different facts, and a caller that only got the second
                    // would think an unresolved reference was no reference.
                    "raw_target": edge.raw_target,
                    "to": edge.to,
                    "line": edge.line,
                })
            })
            .collect();

        let mut carried = vec![
            ("path", json!(path)),
            ("tree", json!(tree.display().to_string())),
            ("count", json!(rows.len())),
            ("edges", json!(rows)),
        ];
        carried.extend(freshness_fields(&answer.freshness));
        println!("{}", thalyx_files::machine::answer(op, carried));
        return Ok(());
    }

    println!();
    if !answer.freshness.is_current() {
        println!("  [{}]", answer.freshness.describe());
        println!();
    }
    if answer.rows.is_empty() {
        if incoming {
            println!("  nothing in the tree refers to {path}");
        } else {
            println!("  {path} declares no dependencies");
        }
        println!();
        return Ok(());
    }
    if incoming {
        println!("  these refer to {path}:");
        for edge in &answer.rows {
            println!("    {}  (line {})", edge.from, edge.line);
        }
    } else {
        println!("  {path} depends on:");
        for edge in &answer.rows {
            match &edge.to {
                Some(target) => println!("    {target}  (line {})", edge.line),
                None => println!(
                    "    {}  (line {}, outside the tree)",
                    edge.raw_target, edge.line
                ),
            }
        }
    }
    println!();
    Ok(())
}

/// The tree a question is about: what was named, or where the session stands.
///
/// Standing in it is what an agent does anyway — it `cd`s into a project and
/// starts asking — and making it type the tree every time would be a discovery
/// cost charged on every single call.
fn tree_of(here: &Where, named: &str) -> std::path::PathBuf {
    let named = named.trim();
    if named.is_empty() {
        here.at().to_path_buf()
    } else {
        thalyx_files::resolve(here.at(), named)
    }
}
