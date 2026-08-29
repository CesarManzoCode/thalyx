//! The programming face: a repo map under a budget, and a rename that resolves
//! before it writes.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md` names the five costs, and
//! the second one — context — is the one every existing tool pays worst. An
//! agent that wants to know what `Store::lock` is reads a nine-hundred-line
//! file, of which four lines were the answer. It pays for the other eight
//! hundred and ninety-six in tokens, in attention, and in the risk that
//! something in them changes what it does next.
//!
//! So the default answer here is **not the file**. It is the name, the kind,
//! the crate, the signature, where it is, how many places use it, where the
//! answer came from and whether it is still true — and a handle. The handle
//! fetches the exact lines the declaration occupies, when and only when the
//! model decides it needs them. `Aider`'s repo map and every progressive
//! disclosure system since made the same discovery independently; what is new
//! here is that the compact answer is **exact** rather than heuristic, because
//! it comes from a compiler frontend, and that it carries its own freshness.
//!
//! ## Two sources, and the answer always says which
//!
//! rust-analyzer when there is one and the tree is Rust; Thalyx's own index
//! otherwise. They are not equally good and the answer never pretends they are:
//! `source: "rust-analyzer"` means a name was resolved, and
//! `source: "index"` means it was matched. A surface that hid the difference
//! would let a model act on a scan believing it had a compiler.
//!
//! ## The one live process
//!
//! Starting rust-analyzer on this workspace costs about 25 seconds and every
//! question after that costs 20 milliseconds, so one is kept for the life of
//! the process, keyed by the tree it was started on. That is machine-global
//! state and it is named as such: it holds no answer anybody depends on — every
//! answer is written into the knowledge store before it is returned — so the
//! worst a test can do to another test is make it slow.

use crate::files::{Face, Where};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use thalyx_know::{Knowledge, Standing};
use thalyx_rust::{At, Provider};

type Fallible = Result<(), Box<dyn std::error::Error>>;

pub const CONTEXT_OP: &str = "context";
pub const RENAME_OP: &str = "rename";

/// How many bytes of entries a context answer returns when nobody said.
///
/// Two kilobytes is about twenty entries, which is more than any single
/// question needs and far less than one file. A number rather than "as much as
/// fits" because a budget nobody can see is not a budget.
pub const BUDGET: usize = 2000;

/// The most a budget may be raised to. Past this a caller is asking for a file,
/// and there is a verb for that which does not pretend to be a summary.
pub const MOST_BUDGET: usize = 32_000;

/// The most lines one expansion hands back.
pub const MOST_LINES: usize = 400;

/// The kind under which a handle's span is remembered.
const KIND_SPAN: &str = "context.span";

// ── where the machine keeps what it knows ────────────────────────────────────

/// One knowledge store per tree, beside the index and keyed the same way.
///
/// The same derivation as `crate::index::open` on purpose: two places computing
/// where a tree's state lives is two states, and the second one is always the
/// empty one somebody is confused by.
pub fn knowledge_path(store_root: &Path, tree: &Path) -> PathBuf {
    let key = tree
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    store_root
        .join("state")
        .join("knowledge")
        .join(format!("{key}.db"))
}

/// The knowledge store for a tree, or one that lives only in memory.
///
/// A store that cannot be written to is not a machine that cannot answer: the
/// answers are still correct, they are just not kept. Rule 10 — a failure to
/// write is not a failure to know — and the alternative is a verb that refuses
/// because a cache directory is read-only.
pub fn knowledge(store_root: &Path, tree: &Path) -> Knowledge {
    Knowledge::open(&knowledge_path(store_root, tree))
        .or_else(|_| Knowledge::in_memory())
        .unwrap_or_else(|_| unreachable!("an in-memory SQLite database always opens"))
}

/// The providers this process is holding, most recently used last.
///
/// A list and not one slot, and the reason is rule 11: this is machine-global
/// state, and two sessions — or, in the test binary, two tests running as
/// threads of one process — would otherwise take turns evicting each other's
/// rust-analyzer. The metric that says "one start for this request" would then
/// be measuring the scheduler.
static LIVE: std::sync::Mutex<Vec<(PathBuf, Provider)>> = std::sync::Mutex::new(Vec::new());

/// How many trees may have a live rust-analyzer at once.
///
/// Each is roughly a hundred megabytes resident, so this is not unbounded. Four
/// is past what any session reaches and enough that concurrent tests do not
/// evict one another.
const MOST_LIVE: usize = 4;

/// Where a tree's build output goes: into the store, never into the tree.
///
/// rust-analyzer runs Cargo, and Cargo with no `CARGO_TARGET_DIR` writes into
/// the workspace. Inside `hacer` that means the snapshot contains a build tree,
/// the rollback destroys the build cache, and a run that changed two files
/// reports twenty-nine changes. Found by a test that asserted the count.
fn build_directory(store_root: &Path, tree: &Path) -> PathBuf {
    let key = tree
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    store_root.join("state").join("rust-target").join(key)
}

/// Do something with the provider for a tree, starting or reusing one.
///
/// Reused rather than rebuilt because the expensive half is the rust-analyzer
/// behind it — 25 seconds against 20 milliseconds on this workspace.
pub fn with_provider<T>(
    store_root: &Path,
    tree: &Path,
    work: impl FnOnce(&mut Provider) -> T,
) -> T {
    let mut live = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let found = live.iter().position(|(root, _)| root.as_path() == tree);
    match found {
        Some(index) => {
            let held = live.remove(index);
            live.push(held);
        }
        None => {
            if live.len() >= MOST_LIVE {
                live.remove(0);
            }
            live.push((
                tree.to_path_buf(),
                Provider::open(tree, knowledge(store_root, tree))
                    .building_into(&build_directory(store_root, tree)),
            ));
        }
    }
    let (_, provider) = live.last_mut().expect("just pushed");
    work(provider)
}

/// Let go of one tree's provider, killing its rust-analyzer.
///
/// One tree and not all of them, for the reason the list exists: a session
/// finishing with its workspace has no business ending another one's.
pub fn release(tree: &Path) {
    let mut live = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    live.retain(|(root, _)| root.as_path() != tree);
}

// ── the tree a session is asking about ───────────────────────────────────────

/// The tree these verbs are about: the workspace when there is one.
pub fn tree_of(here: &Where) -> PathBuf {
    here.confined_to()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| here.at().to_path_buf())
}

fn declined(face: Face, op: &str, word: &str, why: &str) {
    if face.is_machine() {
        face.say(thalyx_files::machine::declined(op, word, why));
    } else {
        println!("\n  {why}\n");
    }
}

// ── contexto ─────────────────────────────────────────────────────────────────

/// One entry of a repo map.
struct Entry {
    handle: String,
    name: String,
    kind: String,
    package: Option<String>,
    file: String,
    line: u32,
    through: u32,
    signature: Option<String>,
    uses: usize,
    source: &'static str,
}

impl Entry {
    fn value(&self) -> Value {
        let mut fields = serde_json::Map::new();
        fields.insert("handle".into(), json!(self.handle));
        fields.insert("name".into(), json!(self.name));
        fields.insert("kind".into(), json!(self.kind));
        if let Some(package) = &self.package {
            fields.insert("crate".into(), json!(package));
        }
        fields.insert("file".into(), json!(self.file));
        fields.insert("line".into(), json!(self.line));
        fields.insert(
            "lines".into(),
            json!(self.through.saturating_sub(self.line) + 1),
        );
        if let Some(signature) = &self.signature {
            fields.insert("signature".into(), json!(signature));
        }
        fields.insert("uses".into(), json!(self.uses));
        fields.insert("source".into(), json!(self.source));
        Value::Object(fields)
    }
}

/// `contexto <consulta> [presupuesto=N] [expandir=<handle>]`
pub fn context(store_root: &Path, here: &Where, rest: &str, face: Face) -> Fallible {
    let Some(given) = crate::words::asked(face, CONTEXT_OP, rest) else {
        return Ok(());
    };
    let mut query = String::new();
    let mut budget = BUDGET;
    let mut expand: Option<String> = None;
    for word in &given {
        let text = word.as_str();
        if let Some(value) = option(text, &["presupuesto", "budget"]) {
            match value.parse::<usize>() {
                Ok(asked) => budget = asked.min(MOST_BUDGET),
                Err(_) => {
                    declined(
                        face,
                        CONTEXT_OP,
                        "incomplete",
                        &format!("`{value}` is not a number of bytes"),
                    );
                    return Ok(());
                }
            }
        } else if let Some(value) = option(text, &["expandir", "expand"]) {
            expand = Some(value.to_string());
        } else if query.is_empty() {
            query = text.to_string();
        } else {
            query.push(' ');
            query.push_str(text);
        }
    }

    let tree = tree_of(here);
    if let Some(handle) = expand {
        return expanded(store_root, here, &tree, &handle, face);
    }
    if query.is_empty() {
        declined(
            face,
            CONTEXT_OP,
            "incomplete",
            "name a symbol, or a file, to be told about",
        );
        return Ok(());
    }

    // A query shaped like a path is one, and it is anchored before anything
    // opens it. Without this a `contexto ../../etc/passwd` would be joined onto
    // the tree, found, and read: the boundary is not what `Slot::Text` checks,
    // so the verb checks it where it knows the argument is a file.
    if query.ends_with(".rs") || query.contains('/') {
        let named = tree.join(&query);
        if named.exists()
            && let Err(error) = here.anchor(&named)
        {
            declined(face, CONTEXT_OP, "outside", &error.to_string());
            return Ok(());
        }
    }

    let asked = Asked {
        store_root: store_root.to_path_buf(),
        tree: tree.clone(),
        query: query.clone(),
    };
    let (entries, source, fresh, why) = gather(&asked);

    let (returned, used, omitted) = fit(&entries, budget);

    // What the model did *not* have to read. The number this whole file exists
    // to move: without it, "the answer is small" is a claim about a JSON blob
    // rather than a measurement against the alternative.
    let held: usize = entries
        .iter()
        .map(|entry| lines_of(&tree, &entry.file, entry.line, entry.through).len())
        .sum();

    if face.is_machine() {
        face.say(thalyx_files::machine::answer(
            CONTEXT_OP,
            vec![
                ("query", json!(query)),
                ("entries", json!(returned)),
                ("shown", json!(returned.len())),
                ("omitted_for_budget", json!(omitted)),
                ("budget_bytes", json!(budget)),
                ("returned_bytes", json!(used)),
                // Rule: no silent loss. Every byte not returned is reachable
                // through a handle, and the answer says how many there are.
                ("held_bytes", json!(held.saturating_sub(used))),
                ("source", json!(source)),
                ("fresh", json!(fresh)),
                ("detail", json!(why)),
            ],
        ));
    } else {
        println!();
        if returned.is_empty() {
            println!("  nothing named `{query}` — {why}");
        }
        for entry in &entries[..returned.len()] {
            println!(
                "  {} {} — {}:{}{}",
                entry.kind,
                entry.name,
                entry.file,
                entry.line,
                entry
                    .signature
                    .as_ref()
                    .map(|signature| format!("\n      {signature}"))
                    .unwrap_or_default()
            );
        }
        if omitted > 0 {
            println!("  … {omitted} more did not fit in {budget} bytes");
        }
        println!("  ({source}, {fresh})");
        println!();
    }
    Ok(())
}

/// As many entries as fit, and how many did not.
///
/// The budget is applied **after** the ranking and never inside it: an entry is
/// dropped for not fitting, never for being expensive to describe. And the
/// first one is always returned, whatever it costs — a budget so small that the
/// answer is empty is a budget that has turned a question into silence, and
/// silence is never an answer.
fn fit(entries: &[Entry], budget: usize) -> (Vec<Value>, usize, usize) {
    let mut returned: Vec<Value> = Vec::new();
    let mut used = 0usize;
    let mut omitted = 0usize;
    for entry in entries {
        let value = entry.value();
        let cost = value.to_string().len();
        if !returned.is_empty() && used + cost > budget {
            omitted += 1;
            continue;
        }
        used += cost;
        returned.push(value);
    }
    (returned, used, omitted)
}

struct Asked {
    store_root: PathBuf,
    tree: PathBuf,
    query: String,
}

/// The entries for a query, from the compiler when there is one and from the
/// index when there is not.
fn gather(asked: &Asked) -> (Vec<Entry>, &'static str, &'static str, String) {
    match from_analyzer(asked) {
        Ok(Some((entries, fresh))) => (entries, "rust-analyzer", fresh, String::new()),
        // Named rather than swallowed. A model told `source: index` knows the
        // answer was matched and not resolved; one told nothing would act on a
        // scan believing it had a compiler.
        Ok(None) | Err(_) => {
            let (entries, fresh, why) = from_index(asked);
            (entries, "index", fresh, why)
        }
    }
}

/// Entries and the standing of the answer they came from, or `None` when this
/// provider has nothing to say about the query at all.
type Resolved = Option<(Vec<Entry>, &'static str)>;

fn from_analyzer(asked: &Asked) -> Result<Resolved, Box<dyn std::error::Error>> {
    with_provider(&asked.store_root, &asked.tree, |provider| {
        // A file, not a name: the map of one file, which is what an agent
        // opening a module actually wants and is the cheapest thing to give it.
        if asked.query.ends_with(".rs") || asked.query.contains('/') {
            let file = asked.tree.join(&asked.query);
            if !file.is_file() {
                return Ok(None);
            }
            let outline = provider.outline(&file)?;
            let package = provider
                .workspace()
                .ok()
                .and_then(|workspace| workspace.package_of(&file))
                .map(|package| package.name.clone());
            let entries: Vec<Entry> = outline
                .iter()
                .map(|item| Entry {
                    handle: handle_for(&item.at.path, item.at.line, item.through),
                    name: item.name.clone(),
                    kind: item.kind.clone(),
                    package: package.clone(),
                    file: item.at.path.clone(),
                    line: item.at.line,
                    through: item.through,
                    signature: None,
                    uses: 0,
                    source: "rust-analyzer",
                })
                .collect();
            remember_spans(provider, &entries);
            return Ok(Some((entries, "current")));
        }

        // `Store::lock` — the last segment is the name and the first is the
        // filter, which is exactly how a person writes it.
        let name = asked
            .query
            .rsplit("::")
            .next()
            .unwrap_or(&asked.query)
            .to_string();
        let (known, standing, _) = provider.known(&name)?;
        let Some(known) = known else {
            return Ok(Some((Vec::new(), standing_word(&standing))));
        };
        let at = known.defined.first().cloned().unwrap_or(At {
            path: String::new(),
            line: 1,
            column: 1,
        });
        let through = provider
            .outline(&asked.tree.join(&at.path))
            .ok()
            .and_then(|outline| {
                outline
                    .iter()
                    .find(|item| item.name == known.name && item.at.line == at.line)
                    .map(|item| item.through)
            })
            .unwrap_or(at.line);
        let entries = vec![Entry {
            handle: handle_for(&at.path, at.line, through),
            name: known.name.clone(),
            kind: known.kind.clone(),
            package: known.package.clone(),
            file: at.path.clone(),
            line: at.line,
            through,
            signature: known.signature.clone(),
            uses: known.used.len(),
            source: "rust-analyzer",
        }];
        remember_spans(provider, &entries);
        Ok(Some((entries, standing_word(&standing))))
    })
}

fn standing_word(standing: &Standing) -> &'static str {
    match standing {
        Standing::Current => "current",
        Standing::Stale { .. } => "stale",
        Standing::Unknown => "unknown",
    }
}

/// The same question put to Thalyx's own index, for a tree that is not Rust or
/// a machine with no rust-analyzer.
fn from_index(asked: &Asked) -> (Vec<Entry>, &'static str, String) {
    let key = asked
        .tree
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let database = asked
        .store_root
        .join("state")
        .join("graph")
        .join(format!("{key}.db"));
    let Ok(mut index) = thalyx_graph::Index::open(&database, &asked.tree) else {
        return (
            Vec::new(),
            "unknown",
            "there is no rust-analyzer here and the index could not be opened".to_string(),
        );
    };
    let _ = index.refresh_if_stale();
    let fresh = match index.freshness() {
        Ok(freshness) if freshness.is_current() => "current",
        Ok(_) => "stale",
        Err(_) => "unknown",
    };
    let name = asked
        .query
        .rsplit("::")
        .next()
        .unwrap_or(&asked.query)
        .to_string();
    let Ok(found) = index.symbol(&name) else {
        return (Vec::new(), fresh, "the index could not be read".to_string());
    };
    let found = found.regardless_of_freshness();
    let uses = found.uses.len();
    let entries: Vec<Entry> = found
        .definitions
        .iter()
        .map(|definition| Entry {
            handle: handle_for(
                &definition.path,
                definition.line as u32,
                definition.line as u32,
            ),
            name: definition.name.clone(),
            kind: definition.kind.clone(),
            package: None,
            file: definition.path.clone(),
            line: definition.line as u32,
            through: definition.line as u32,
            signature: None,
            uses,
            source: "index",
        })
        .collect();
    (
        entries,
        fresh,
        "answered from the index: names are matched here, not resolved".to_string(),
    )
}

/// A stable name for a span, so the same question hands back the same handle.
///
/// Derived from what it points at rather than counted, which is what makes it
/// stable across calls and across processes — a counter would give a model a
/// handle that means something different tomorrow.
fn handle_for(path: &str, from: u32, through: u32) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{path}:{from}:{through}").as_bytes());
    format!("ctx-{}", hex::encode(&digest[..6]))
}

fn remember_spans(provider: &mut Provider, entries: &[Entry]) {
    let Ok(witness) = provider.source_witness() else {
        return;
    };
    for entry in entries {
        let value = json!({
            "path": entry.file, "from": entry.line, "through": entry.through
        })
        .to_string();
        let _ =
            provider
                .knowledge()
                .remember(KIND_SPAN, &entry.handle, &witness, entry.source, &value);
    }
}

/// `contexto expandir=<handle>` — the exact lines, and nothing around them.
fn expanded(store_root: &Path, here: &Where, tree: &Path, handle: &str, face: Face) -> Fallible {
    let store = knowledge(store_root, tree);
    let witness = with_provider(store_root, tree, |provider| provider.source_witness().ok());
    let Some(witness) = witness else {
        declined(
            face,
            CONTEXT_OP,
            "unreadable",
            "the tree could not be weighed, so no handle can be said to still point anywhere",
        );
        return Ok(());
    };
    let Ok(Some(held)) = store.recall(KIND_SPAN, handle, &witness) else {
        declined(
            face,
            CONTEXT_OP,
            "no_such_handle",
            &format!("`{handle}` is not a handle this machine has issued"),
        );
        return Ok(());
    };
    let span: Value = serde_json::from_str(&held.value).unwrap_or(Value::Null);
    let path = span.get("path").and_then(Value::as_str).unwrap_or_default();
    let from = span.get("from").and_then(Value::as_u64).unwrap_or(1) as u32;
    let through = span.get("through").and_then(Value::as_u64).unwrap_or(1) as u32;

    // The containment check, on a path that came back from the machine's own
    // memory. A handle is a string a caller sends, and a store somebody edited
    // is a store that names `/etc/shadow`.
    if here.anchor(&tree.join(path)).is_err() {
        declined(
            face,
            CONTEXT_OP,
            "outside",
            &format!("`{path}` is not inside this workspace"),
        );
        return Ok(());
    }

    let text = lines_of(tree, path, from, through);
    if face.is_machine() {
        face.say(thalyx_files::machine::answer(
            CONTEXT_OP,
            vec![
                ("handle", json!(handle)),
                ("file", json!(path)),
                ("from_line", json!(from)),
                (
                    "through_line",
                    json!(from + text.lines().count() as u32 - 1),
                ),
                ("text", json!(text)),
                ("returned_bytes", json!(text.len())),
                // The standing of the handle itself, not of the file: the span
                // was worked out against a tree, and this says whether that is
                // still the tree.
                ("fresh", json!(held.standing.word())),
                ("source", json!(held.source)),
            ],
        ));
    } else {
        println!("\n{text}\n");
    }
    Ok(())
}

/// The text of a line range, bounded, or empty when the file cannot be read.
fn lines_of(tree: &Path, path: &str, from: u32, through: u32) -> String {
    let Ok(text) = std::fs::read_to_string(tree.join(path)) else {
        return String::new();
    };
    let first = from.saturating_sub(1) as usize;
    text.lines()
        .skip(first)
        .take(((through.saturating_sub(from) + 1) as usize).min(MOST_LINES))
        .collect::<Vec<_>>()
        .join("\n")
}

fn option<'a>(word: &'a str, names: &[&str]) -> Option<&'a str> {
    let (name, value) = word.split_once('=')?;
    names.contains(&name).then_some(value)
}

// ── renombrar ────────────────────────────────────────────────────────────────

/// `renombrar <nombre|archivo:línea:columna> <nuevo>`
///
/// The verb that makes the whole arrangement worth having. A rename is the
/// canonical multi-file change, it is the thing an agent gets wrong most often,
/// and it is *entirely* mechanical once somebody knows what the name is — which
/// is exactly the part a scan cannot do and a compiler frontend can.
///
/// It writes through the session's own boundary, so inside `hacer` it happens
/// under the snapshot and comes back out if the checks say no.
pub fn rename(store_root: &Path, here: &Where, rest: &str, face: Face) -> Fallible {
    let Some(given) = crate::words::asked(face, RENAME_OP, rest) else {
        return Ok(());
    };
    if given.len() < 2 {
        declined(
            face,
            RENAME_OP,
            "incomplete",
            "name what to rename and what to call it: `renombrar Keystore KeyVault`, \
             or `renombrar src/keystore.rs:1:12 KeyVault`",
        );
        return Ok(());
    }
    let anchor = given[0].as_str().to_string();
    let to = given[1].as_str().to_string();
    if to.is_empty() || !to.chars().all(|c| c.is_alphanumeric() || c == '_') {
        declined(
            face,
            RENAME_OP,
            "not_a_name",
            &format!("`{to}` is not a Rust identifier"),
        );
        return Ok(());
    }

    let tree = tree_of(here);
    let resolved = with_provider(store_root, &tree, |provider| {
        let (file, line, column) = match place(provider, &tree, &anchor) {
            Ok(place) => place,
            Err(why) => return Err(why),
        };
        provider
            .rename_texts(&file, line, column, &to)
            .map(|texts| (texts, file, line, column))
            .map_err(|error| error.to_string())
    });

    let (texts, file, line, column) = match resolved {
        Ok(all) => all,
        Err(why) => {
            declined(face, RENAME_OP, "unresolved", &why);
            return Ok(());
        }
    };
    if texts.is_empty() {
        declined(
            face,
            RENAME_OP,
            "nothing_to_do",
            &format!("`{anchor}` resolves to nothing that can be renamed here"),
        );
        return Ok(());
    }

    // Every write goes through the session's boundary, one at a time, and the
    // first refusal stops the rest. A half-applied rename is worse than none —
    // it compiles nowhere and looks like it worked — so the paths are all
    // checked before any of them is opened.
    let mut anchored = Vec::with_capacity(texts.len());
    for (path, _) in &texts {
        match here.anchor(path) {
            Ok(held) => anchored.push(held),
            Err(error) => {
                declined(
                    face,
                    RENAME_OP,
                    "outside",
                    &format!("{}: {error}", path.display()),
                );
                return Ok(());
            }
        }
    }

    let mut written = Vec::with_capacity(texts.len());
    for (held, (path, text)) in anchored.iter().zip(texts.iter()) {
        if let Err(error) = std::fs::write(held.path(), text) {
            declined(
                face,
                RENAME_OP,
                "unwritable",
                &format!(
                    "{} could not be written: {error}. {} file(s) were already changed, and \
                     the boundary around this is what puts them back",
                    path.display(),
                    written.len()
                ),
            );
            return Ok(());
        }
        written.push(
            path.strip_prefix(&tree)
                .unwrap_or(path)
                .display()
                .to_string(),
        );
    }

    if face.is_machine() {
        face.say(thalyx_files::machine::answer(
            RENAME_OP,
            vec![
                ("from", json!(anchor)),
                ("to", json!(to)),
                (
                    "resolved_at",
                    json!(format!(
                        "{}:{line}:{column}",
                        file.strip_prefix(&tree).unwrap_or(&file).display()
                    )),
                ),
                ("files", json!(written)),
                ("files_changed", json!(written.len())),
                ("source", json!("rust-analyzer")),
            ],
        ));
    } else {
        println!(
            "\n  {} renamed to {} across {} file(s)\n",
            anchor,
            to,
            written.len()
        );
    }
    Ok(())
}

/// Where the thing to rename is: a position if the caller gave one, and
/// otherwise the declaration of the name.
fn place(
    provider: &mut Provider,
    tree: &Path,
    anchor: &str,
) -> Result<(PathBuf, u32, u32), String> {
    let parts: Vec<&str> = anchor.rsplitn(3, ':').collect();
    if parts.len() == 3
        && let (Ok(column), Ok(line)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>())
    {
        let file = tree.join(parts[2]);
        if file.is_file() {
            return Ok((file, line, column));
        }
        return Err(format!("{} is not a file of this workspace", parts[2]));
    }

    let (known, standing, _) = provider.known(anchor).map_err(|error| error.to_string())?;
    let known = known.ok_or_else(|| format!("nothing in this workspace declares `{anchor}`"))?;
    if matches!(standing, Standing::Stale { .. }) {
        // Cannot happen through `known`, which never returns a stale answer;
        // written so that a future path that could is refused rather than
        // renaming against a tree that has moved.
        return Err(format!("what is known about `{anchor}` is out of date"));
    }
    let at = known
        .defined
        .first()
        .ok_or_else(|| format!("`{anchor}` is known but has no declaration"))?;
    Ok((tree.join(&at.path), at.line, at.column))
}

// ── what the provider has cost, for the metrics of a run ─────────────────────

/// The live provider's counters, or zeroes when there is no provider.
///
/// Read before and after a run and subtracted, which is the only honest way to
/// attribute semantic work to one request: the provider outlives the request on
/// purpose, so its totals are not the request's.
pub fn tally(tree: &Path) -> thalyx_rust::Tally {
    let live = LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    live.iter()
        .find(|(root, _)| root.as_path() == tree)
        .map(|(_, provider)| provider.tally.clone())
        .unwrap_or_default()
}

// ── the Rust check, for `hacer` ──────────────────────────────────────────────

/// Which crates a set of changed paths selects, and the identity of everything
/// that check would read.
pub struct Selection {
    pub packages: Vec<String>,
    pub why: String,
    pub unattributed: Vec<String>,
    pub identity: Option<thalyx_know::Witness>,
}

/// Work out what has to be compiled, and what a cached answer about it covers.
///
/// `None` when this is not a Cargo workspace at all, which is a different fact
/// from "nothing was affected" and is reported as one — rule 10, where the
/// difference decides whether a caller goes looking for a bug in its own diff.
pub fn selection(
    store_root: &Path,
    tree: &Path,
    changed: &[String],
    asked: &[String],
) -> Option<Selection> {
    with_provider(store_root, tree, |provider| {
        let workspace = provider.workspace().ok()?.clone();
        // An explicit list is the escape hatch, and it skips the derivation
        // entirely: a caller that says which crates to check has made a
        // decision, and second-guessing it would make the escape hatch a
        // suggestion.
        let (packages, why, unattributed) = if asked.is_empty() {
            let reached = thalyx_rust::affected(&workspace, tree, changed);
            (reached.selected, reached.why, reached.unattributed)
        } else {
            (
                asked.to_vec(),
                format!("the program named {} package(s) itself", asked.len()),
                Vec::new(),
            )
        };
        let identity = (!packages.is_empty()).then(|| {
            thalyx_rust::affected::identity(&workspace, &packages, &thalyx_rust::toolchain())
        });
        Some(Selection {
            packages,
            why,
            unattributed,
            identity,
        })
    })
}

/// What was recorded about this exact check over this exact state, if anything.
pub fn recall_validation(
    store_root: &Path,
    tree: &Path,
    key: &str,
    identity: &thalyx_know::Witness,
) -> Option<String> {
    knowledge(store_root, tree)
        .recall_current(thalyx_rust::KIND_VALIDATION, key, identity)
        .ok()
        .flatten()
        .map(|held| held.value)
}

/// Record what a check found about a state.
///
/// Only a real verdict is kept. A check that **could not run** — no cargo, no
/// kernel to confine with — is not a result about the tree, and remembering it
/// would make a machine that once lacked a toolchain go on reporting
/// `not_proven` about bytes it never compiled.
pub fn remember_validation(
    store_root: &Path,
    tree: &Path,
    key: &str,
    identity: &thalyx_know::Witness,
    value: &str,
) {
    let _ = knowledge(store_root, tree).remember(
        thalyx_rust::KIND_VALIDATION,
        key,
        identity,
        "cargo",
        value,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, lines: u32) -> Entry {
        Entry {
            handle: handle_for("src/lib.rs", 1, lines),
            name: name.to_string(),
            kind: "function".to_string(),
            package: Some("a-crate".to_string()),
            file: "src/lib.rs".to_string(),
            line: 1,
            through: lines,
            signature: Some(format!("pub fn {name}() -> Result<()>")),
            uses: 17,
            source: "rust-analyzer",
        }
    }

    #[test]
    fn a_budget_that_is_reached_drops_entries_and_says_how_many() {
        let entries: Vec<Entry> = (0..20).map(|n| entry(&format!("name{n}"), 40)).collect();
        let whole = fit(&entries, 100_000);
        assert_eq!(whole.0.len(), 20);
        assert_eq!(whole.2, 0);

        let (returned, used, omitted) = fit(&entries, 400);
        assert!(used <= 400 || returned.len() == 1, "{used} bytes returned");
        assert_eq!(
            returned.len() + omitted,
            20,
            "an entry was neither returned nor counted as omitted, which is a \
             silent loss and the one thing this surface must never do"
        );
    }

    #[test]
    fn a_budget_of_nothing_still_answers_something() {
        let (returned, _, omitted) = fit(&[entry("only", 10)], 0);
        assert_eq!(returned.len(), 1);
        assert_eq!(omitted, 0);
    }

    #[test]
    fn a_handle_names_a_span_and_not_a_moment() {
        // Stable across calls and across processes, because a model that got a
        // different handle for the same declaration on every question could
        // never carry one from one turn to the next.
        assert_eq!(
            handle_for("src/lib.rs", 10, 40),
            handle_for("src/lib.rs", 10, 40)
        );
        assert_ne!(
            handle_for("src/lib.rs", 10, 40),
            handle_for("src/lib.rs", 10, 41)
        );
        assert!(handle_for("src/lib.rs", 1, 2).starts_with("ctx-"));
    }

    #[test]
    fn an_entry_carries_where_the_answer_came_from() {
        let value = entry("lock", 12).value();
        assert_eq!(value["source"], json!("rust-analyzer"));
        assert_eq!(value["crate"], json!("a-crate"));
        assert_eq!(value["uses"], json!(17));
        assert_eq!(value["lines"], json!(12));
        assert!(
            value["signature"]
                .as_str()
                .is_some_and(|s| s.contains("pub fn")),
            "the signature is the half that makes an entry worth reading"
        );
    }

    #[test]
    fn only_the_option_words_this_verb_knows_are_options() {
        assert_eq!(
            option("presupuesto=400", &["presupuesto", "budget"]),
            Some("400")
        );
        assert_eq!(
            option("budget=400", &["presupuesto", "budget"]),
            Some("400")
        );
        // A query that happens to contain `=` is a query.
        assert_eq!(option("a=b", &["presupuesto"]), None);
        assert_eq!(option("Store::lock", &["presupuesto"]), None);
    }
}
