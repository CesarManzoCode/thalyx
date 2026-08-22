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
                            // What C2 rests on. A tree with zero symbols is one
                            // the parser has no language for, and a caller that
                            // only learned that by searching and finding nothing
                            // would blame its own spelling.
                            ("symbols", json!(report.symbols)),
                            ("mentions", json!(report.mentions)),
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
                println!(
                    "  {} names declared, used in {} places",
                    report.symbols, report.mentions
                );
                if report.skipped > 0 {
                    println!("  {} skipped — not a language I can read", report.skipped);
                }
                println!();
            }
        }
        // Told apart, because they are different things to do next. Rule 10:
        // a tree nobody should wait for is not a tree that could not be read,
        // and a caller that heard `unreadable` about `/home` would go looking
        // for a permission problem that does not exist.
        Err(error) => {
            let word = match &error {
                thalyx_graph::GraphError::TreeTooLarge { .. } => "tree_too_large",
                _ => "unreadable",
            };
            declined(face, "index_build", word, &error.to_string());
        }
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

    let Some(given) = crate::words::asked(face, op, rest) else {
        return Ok(());
    };
    let (path, window) = match asked_of(&given) {
        Ok(both) => both,
        Err(why) => {
            declined(face, op, "bad_cursor", &why.to_string());
            return Ok(());
        }
    };
    let path = path.as_str();
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
        // Sorted here and nowhere else, because the window pages by a key and a
        // key into rows whose order SQLite chose would name a different place on
        // every call. `thalyx-files` refuses unordered rows rather than paging
        // them anyway, so this is the sort that makes the refusal impossible
        // instead of the sort that avoids it by luck.
        let mut edges = answer.rows;
        edges.sort_by_key(edge_key);

        let page = match thalyx_files::window::page(edges, edge_key, &window) {
            Ok(page) => page,
            Err(why) => {
                declined(face, op, "unordered", &why.to_string());
                return Ok(());
            }
        };

        let rows: Vec<serde_json::Value> = page
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
            ("edges", json!(rows)),
        ];
        carried.extend(thalyx_files::machine::window_fields(&page));
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

/// `buscar <nombre>` — where a name comes from, and everywhere it is used.
///
/// `Superficie-para-el-LLM.md`, punto **C2**. `grep` answers with lines because
/// it does not know what a symbol is; the [[Parser-Mecanico]] does, in five
/// languages, so this answers with the definition and the call sites separately
/// and with neither comments nor strings in either list.
///
/// The two lists are one answer and not two questions, because *where does this
/// come from and who uses it* is one thought — and a caller that had to ask
/// twice would pay two round trips for it.
pub fn symbol(store_root: &Path, here: &Where, rest: &str, face: Face) -> Fallible {
    let op = "symbol";

    let Some(given) = crate::words::asked(face, op, rest) else {
        return Ok(());
    };
    let (name, window) = match asked_of(&given) {
        Ok(both) => both,
        Err(why) => {
            declined(face, op, "bad_cursor", &why.to_string());
            return Ok(());
        }
    };
    if name.is_empty() {
        declined(face, op, "incomplete", "which name");
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

    let answer = match index.symbol(&name) {
        Ok(answer) => answer,
        Err(error) => {
            declined(face, op, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    if face == Face::Machine {
        // Only the uses are paged. Definitions are few by nature — a name with
        // two hundred definitions is a fact worth seeing whole — and paging a
        // list of one would put a cursor in every answer for nothing.
        let uses = answer.rows.uses;
        let page = match thalyx_files::window::page(uses, use_key, &window) {
            Ok(page) => page,
            Err(why) => {
                declined(face, op, "unordered", &why.to_string());
                return Ok(());
            }
        };

        let definitions: Vec<serde_json::Value> = answer
            .rows
            .definitions
            .iter()
            .map(|found| {
                json!({
                    "path": found.path,
                    "line": found.line,
                    "kind": found.kind,
                })
            })
            .collect();
        let uses: Vec<serde_json::Value> = page
            .rows
            .iter()
            .map(|used| json!({ "path": used.path, "line": used.line }))
            .collect();

        let mut carried = vec![
            ("name", json!(name)),
            ("tree", json!(tree.display().to_string())),
            ("definitions", json!(definitions)),
            ("uses", json!(uses)),
            // Which list the window is about. Two lists and one set of paging
            // fields would otherwise be a guess, and a caller that guessed the
            // wrong one would page a list that was never cut.
            ("window_of", json!("uses")),
        ];
        carried.extend(thalyx_files::machine::window_fields(&page));
        carried.extend(freshness_fields(&answer.freshness));
        println!("{}", thalyx_files::machine::answer(op, carried));
        return Ok(());
    }

    println!();
    if !answer.freshness.is_current() {
        println!("  [{}]", answer.freshness.describe());
        println!();
    }
    if answer.rows.definitions.is_empty() && answer.rows.uses.is_empty() {
        // Two facts and not one. Nothing in the index is not the same as
        // nothing in the tree, and a person told the second would stop looking.
        println!("  nothing in the index declares or uses `{name}`.");
        println!("  `indexar` reads the tree; a name from outside it is not in here.");
        println!();
        return Ok(());
    }

    for found in &answer.rows.definitions {
        println!(
            "  {} {}  —  {}:{}",
            found.kind, name, found.path, found.line
        );
    }
    if answer.rows.definitions.is_empty() {
        println!("  `{name}` is used here but nothing in this tree declares it.");
    }
    if !answer.rows.uses.is_empty() {
        println!();
        println!("  used in {} places:", answer.rows.uses.len());
        for used in &answer.rows.uses {
            println!("    {}:{}", used.path, used.line);
        }
    }
    println!();
    Ok(())
}

/// What a cursor into a list of uses names.
fn use_key(used: &thalyx_graph::Use) -> Vec<u8> {
    let mut key = used.path.as_bytes().to_vec();
    key.push(0);
    // Fixed-width and big-endian, so byte order and numeric order agree. As
    // decimal text, line 10 sorts before line 9 and the window would refuse the
    // whole answer as unordered.
    key.extend_from_slice(&(used.line as u64).to_be_bytes());
    key
}

/// Split what was typed into the file being asked about and the window asked for.
///
/// The same two words `ls` takes — `limite=` and `cursor=` — because a caller
/// that has to learn a second spelling of "give me the next page" pays the
/// discovery cost twice for one idea, and `Superficie-para-el-LLM.md` exists to
/// stop exactly that.
///
/// Takes the words rather than the line, because the splitting belongs in one
/// place: `words.rs`. What comes back is the rest joined with single spaces,
/// which is what a subject made of several words means.
pub(crate) fn asked_of(
    given: &[crate::words::Word],
) -> Result<(String, thalyx_files::window::Asked), thalyx_files::window::Cut> {
    let mut window = thalyx_files::window::Asked::default();
    let mut named = Vec::new();

    for word in given.iter().map(crate::words::Word::as_str) {
        match word.split_once('=') {
            Some(("limite" | "limit", count)) if count.parse::<usize>().is_ok() => {
                window.limit = count.parse().expect("just checked");
            }
            Some(("cursor" | "desde", token)) if !token.is_empty() => {
                window.after = Some(thalyx_files::window::Cursor::parse(token)?);
            }
            _ => named.push(word),
        }
    }

    Ok((named.join(" "), window))
}

/// What a cursor into a list of edges names.
///
/// Every field of the edge, because two references that differ only in where
/// they point are two rows, and a key that collided would page past one of them
/// without ever sending it. The line number is fixed-width and big-endian so
/// that byte order and numeric order are the same thing — a decimal `10` sorts
/// before `9` as text, and the window would refuse the whole answer as
/// unordered.
fn edge_key(edge: &thalyx_graph::Edge) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend_from_slice(edge.from.as_bytes());
    key.push(0);
    key.extend_from_slice(&(edge.line as u64).to_be_bytes());
    key.extend_from_slice(edge.raw_target.as_bytes());
    key.push(0);
    key.extend_from_slice(edge.to.as_deref().unwrap_or("").as_bytes());
    key
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
