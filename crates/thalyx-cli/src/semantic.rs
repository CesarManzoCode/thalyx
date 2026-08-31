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
use thalyx_core::Store;
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

/// The most use sites one entry may carry.
pub const MOST_USES: usize = 200;

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
pub fn build_directory(store_root: &Path, tree: &Path) -> PathBuf {
    let key = tree
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    store_root.join("state").join("rust-target").join(key)
}

// ── the provider's own process, under Thalyx's authority ─────────────────────

/// Starts the semantic provider the way Thalyx starts a program nobody signed.
///
/// `vault/03-Primitivas/Semantica-Compilada.md`, revised 2026-08-30. The
/// provider used to be an ordinary host process, justified by "it is a
/// reader" — which is true of the LSP protocol and false of the process tree.
/// rust-analyzer runs `cargo metadata`, and answering anything about a
/// workspace with a proc-macro or a build script in it means **compiling and
/// running them**: arbitrary code from a registry, executing at analysis time,
/// with whatever reach the process that started it had.
///
/// So it goes through `thalyx_core::start_foreign` — the same establishment
/// `ejecutar` uses: an enforcement gate that refuses when nothing can deny, its
/// own user, its own cgroup with a policy in the kernel, its own root
/// filesystem holding the workspace and the toolchain and nothing else, its own
/// pid namespace so killing the one process Thalyx holds kills every compiler
/// under it, its own network namespace, and the seccomp filter.
///
/// What it is granted, and nothing else:
///
/// - the **workspace**, read and write. Write because rust-analyzer's first act
///   on a workspace with no `Cargo.lock` is to write one, and a provider that
///   could not would answer every question about a tree it had failed to
///   describe;
/// - the **toolchain and the registry**, read-only;
/// - a **build directory outside the workspace**, both ways — outside because
///   a `target/` inside the tree is inside the snapshot, and a rollback would
///   destroy the build cache that makes the next question cheap.
struct UnderThalyx {
    store: Store,
    request_id: String,
}

impl thalyx_rust::analyzer::Spawn for UnderThalyx {
    fn start(
        &self,
        asked: thalyx_rust::analyzer::Launching<'_>,
    ) -> thalyx_rust::Result<thalyx_rust::analyzer::Started> {
        use thalyx_manifest::{Permission, PermissionKind};

        let mut grants = Vec::new();
        let mut grant = |path: &Path, write: bool| {
            grants.push(Permission {
                resource: path.display().to_string(),
                action: "read".to_string(),
                kind: PermissionKind::Session,
            });
            if write {
                grants.push(Permission {
                    resource: path.display().to_string(),
                    action: "write".to_string(),
                    kind: PermissionKind::Session,
                });
            }
        };
        grant(asked.root, true);
        if let Some(target) = asked.build_into {
            // Made here rather than left to Cargo: a grant on a directory that
            // does not exist yet is a grant on nothing, and `RootFs` refuses a
            // granted path that is not there.
            let _ = std::fs::create_dir_all(target);
            grant(target, true);
        }
        for path in asked.readable {
            if path.is_dir() {
                grant(path, false);
            }
        }

        let mut environment: Vec<(String, String)> = asked
            .environment
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        if let Some(target) = asked.build_into {
            environment.push(("CARGO_TARGET_DIR".to_string(), target.display().to_string()));
        }

        let started = match thalyx_core::start_foreign(
            &self.store,
            &thalyx_permd::KernelStore::default_map(),
            &thalyx_core::ForeignRequest {
                program: asked.program,
                args: Vec::new(),
                grants,
                helper: std::env::current_exe()
                    .unwrap_or_else(|_| std::path::PathBuf::from("thalyx")),
                request_id: self.request_id.clone(),
                profile: thalyx_sandbox::profile::SEMANTIC_PROVIDER,
                environment,
            },
        ) {
            Ok(started) => started,
            // **The fallback, and it is not a soft spot left open for
            // convenience.**
            //
            // `start_foreign` refuses on a machine whose kernel is not denying.
            // That is `Programas-Ajenos.md`'s decree and it is right — but a
            // Thalyx that could therefore not resolve a symbol *at all* would
            // be a machine where the programming face does not exist, and this
            // container is such a machine, as is any Fedora that has not run
            // `make -C lsm load`.
            //
            // So it falls back to what this crate could always do, and every
            // answer that came through it says `analyzer_confined: false` with
            // the reason attached. `THALYX_REQUIRE_CONFINED_ANALYZER=1` turns
            // it into a refusal, which is rule 3's shape: one variable per
            // requirement, so a machine that can enforce can demand that it did.
            Err(why) => {
                if std::env::var("THALYX_REQUIRE_CONFINED_ANALYZER").as_deref() == Ok("1") {
                    return Err(thalyx_rust::RustError::NoAnalyzer(format!(
                        "THALYX_REQUIRE_CONFINED_ANALYZER=1 and the semantic provider could \
                         not be confined: {why}. Nothing was started"
                    )));
                }
                let mut fell_back =
                    thalyx_rust::analyzer::Spawn::start(&thalyx_rust::analyzer::OnTheHost, asked)?;
                fell_back.how = format!("host (not confined: {why})");
                fell_back.confined = false;
                return Ok(fell_back);
            }
        };

        let how = format!("confined: {}", started.isolation);
        let confined = started.isolated;
        let mut started = started;
        // The child moves out — `Analyzer` owns the conversation over its
        // pipes — and the confinement stays behind to be torn down. That split
        // is why `ForeignProcess` holds an `Option<Child>` and not a `Child`:
        // the first shape left a placeholder process behind, which made this
        // depend on a `/bin/true` existing. The image holds the Linux kernel
        // and one program.
        let child = started.take_child().ok_or_else(|| {
            thalyx_rust::RustError::NoAnalyzer(
                "the confinement started nothing to talk to".to_string(),
            )
        })?;

        Ok(thalyx_rust::analyzer::Started {
            child,
            release: Some(Box::new(move || {
                started.shutdown(&thalyx_permd::KernelStore::default_map());
            })),
            how,
            confined,
        })
    }
}

/// The provider for a tree, confined where this machine can confine anything.
///
/// **Falls back to a host process, says so, and can be made to refuse.**
///
/// The fallback is not a soft spot left open for convenience: `start_foreign`
/// refuses on a machine whose kernel is not denying, which is
/// `Programas-Ajenos.md`'s decree and is right — and a Thalyx that therefore
/// could not resolve a symbol at all would be a machine where the programming
/// face does not exist. This container is such a machine, and so is any Fedora
/// that has not run `make -C lsm load`.
///
/// So it is reported rather than hidden: every answer carries
/// `analyzer_confined`, and `THALYX_REQUIRE_CONFINED_ANALYZER=1` turns the
/// fallback into a refusal. Rule 3's shape — one variable per requirement — so
/// a machine that can enforce can demand that it did.
fn provider_for(store_root: &Path, tree: &Path) -> Provider {
    let provider = Provider::open(tree, knowledge(store_root, tree))
        .building_into(&build_directory(store_root, tree))
        .reaching(
            thalyx_rust::toolchain::readable(),
            thalyx_rust::toolchain::environment()
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
        );
    match Store::open(store_root) {
        Ok(store) => provider.spawning(std::sync::Arc::new(UnderThalyx {
            store,
            request_id: crate::new_request_id(),
        })),
        // No store is no journal and no uid registry, which are two of the
        // things confining a program needs. Said as what it is rather than
        // becoming a silent host process: the answer's `analyzer_confined`
        // will be `false` and `analyzer_how` will be `host`.
        Err(_) => provider,
    }
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
            live.push((tree.to_path_buf(), provider_for(store_root, tree)));
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
    /// One-based, as every other coordinate on this surface is.
    ///
    /// Carried so the entry can name *itself* precisely: `file:line:column` is
    /// what `renombrar` takes and what resolves an ambiguity, and an answer
    /// that described three candidates without saying how to ask about one of
    /// them would be an answer a caller cannot act on.
    column: u32,
    through: u32,
    signature: Option<String>,
    uses: usize,
    /// Where it is used, when the caller asked for them. Held back by default
    /// because a symbol with two hundred uses would be the whole budget, and
    /// the count alone answers most of the questions the list is asked for.
    used_at: Vec<String>,
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
            "at".into(),
            json!(format!("{}:{}:{}", self.file, self.line, self.column)),
        );
        fields.insert(
            "lines".into(),
            json!(self.through.saturating_sub(self.line) + 1),
        );
        if let Some(signature) = &self.signature {
            fields.insert("signature".into(), json!(signature));
        }
        fields.insert("uses".into(), json!(self.uses));
        if !self.used_at.is_empty() {
            fields.insert("used_at".into(), json!(self.used_at));
        }
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
    let mut uses = 0usize;
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
        } else if let Some(value) = option(text, &["usos", "uses"]) {
            // The list the index verb gives whole. Here it is asked for,
            // because on a name like `Store` it *is* the budget — and because
            // the count answers "is this used anywhere" and "is this used a
            // lot", which is most of what the list gets asked.
            match value.parse::<usize>() {
                Ok(asked) => uses = asked.min(MOST_USES),
                Err(_) => {
                    declined(
                        face,
                        CONTEXT_OP,
                        "incomplete",
                        &format!("`{value}` is not a number of use sites"),
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
        uses,
    };
    let Answered {
        entries,
        source,
        fresh,
        resolution,
        why,
    } = gather(&asked);

    let confinement = with_provider(store_root, &tree, |provider| {
        (
            provider.analyzer_confined(),
            provider.analyzer_how().map(str::to_string),
        )
    });

    let (returned, used, omitted) = fit(&entries, budget);

    // What the model did *not* have to read: the whole of every file these
    // entries live in, which is what an agent without this verb would have
    // opened. The number this file exists to move, and without it "the answer
    // is small" is a claim about a JSON blob rather than a measurement against
    // the alternative.
    //
    // Deduplicated, because twenty entries out of one file are one file.
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let held: u64 = entries
        .iter()
        .filter(|entry| seen.insert(entry.file.as_str()))
        .filter_map(|entry| std::fs::metadata(tree.join(&entry.file)).ok())
        .map(|about| about.len())
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
                // Rule: no silent loss. What was not returned is reachable —
                // by handle for a declaration, by `read` for a whole file —
                // and the answer says how much of it there is.
                ("held_bytes", json!(held)),
                ("source", json!(source)),
                ("fresh", json!(fresh)),
                // The field a program branches on. Always present, on every
                // answer, whichever provider gave it — a field that only turns
                // up on the ambiguous day is a field nobody handles on the
                // ambiguous day.
                ("resolution", json!(resolution)),
                // What stood behind the process that answered. rust-analyzer
                // runs Cargo, which compiles and runs build scripts, so this
                // is a fact about arbitrary code having executed — reported
                // rather than assumed, on every answer, including the ones the
                // index gave where it is `null`.
                ("analyzer_confined", json!(confinement.0)),
                ("analyzer_how", json!(confinement.1)),
                ("detail", json!(why)),
            ],
        ));
    } else {
        println!();
        if resolution == "ambiguous" {
            println!("  {why}");
            println!();
        }
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
    /// How many use sites to return with the entry. Zero — the default —
    /// returns the count and nothing else.
    uses: usize,
}

/// What a query came back as.
///
/// `resolution` is the field this whole file turns on: `one` means a compiler
/// frontend resolved the name to exactly one declaration, and **`ambiguous`
/// means it resolved to several and this machine refused to pick**. A surface
/// that answered a three-`Config` workspace with one `Config` and no word
/// about the other two would be handing a model a confident wrong answer, and
/// the thing that acts on it is a rename.
struct Answered {
    entries: Vec<Entry>,
    source: &'static str,
    fresh: &'static str,
    resolution: &'static str,
    why: String,
}

/// The entries for a query, from the compiler when there is one and from the
/// index when there is not.
fn gather(asked: &Asked) -> Answered {
    match from_analyzer(asked) {
        Ok(Some(answered)) => answered,
        // Named rather than swallowed. A model told `source: index` knows the
        // answer was matched and not resolved; one told nothing would act on a
        // scan believing it had a compiler.
        Ok(None) | Err(_) => {
            let (entries, fresh, why) = from_index(asked);
            Answered {
                entries,
                source: "index",
                fresh,
                // Never `one` and never `ambiguous`. The index matches text; it
                // cannot say a name resolves to one thing, so it must not be
                // able to say a name resolves to several either — an ambiguity
                // is a claim, and only the thing that can resolve names is
                // entitled to make it.
                resolution: "matched",
                why,
            }
        }
    }
}

/// An answer, or `None` when this provider has nothing to say about the query
/// at all.
type Resolved = Option<Answered>;

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
                    column: item.at.column,
                    through: item.through,
                    signature: None,
                    uses: 0,
                    used_at: Vec::new(),
                    source: "rust-analyzer",
                })
                .collect();
            remember_spans(provider, &entries);
            return Ok(Some(Answered {
                entries,
                source: "rust-analyzer",
                fresh: "current",
                // A file's map is a list of everything in it and never a claim
                // about which one a name means. There is no question here to
                // be ambiguous about.
                resolution: "file",
                why: String::new(),
            }));
        }

        // `Store::lock` — the last segment is the name and the first is the
        // filter, which is exactly how a person writes it.
        let name = asked
            .query
            .rsplit("::")
            .next()
            .unwrap_or(&asked.query)
            .to_string();
        let (resolution, standing, _) = provider.known(&name)?;
        let fresh = standing_word(&standing);

        if resolution.is_ambiguous() {
            // **The refusal, as an answer rather than an error.** Every
            // candidate comes back described and handled, so the caller —
            // model or program — can choose one and ask again with
            // `file:line:column`, which names exactly one declaration.
            //
            // Nothing here is ranked. A "most likely" candidate at the top of
            // this list would be the heuristic guess this whole shape exists
            // to remove, wearing a disclaimer.
            let entries: Vec<Entry> = resolution
                .candidates()
                .iter()
                .map(|candidate| Entry {
                    handle: handle_for(&candidate.at.path, candidate.at.line, candidate.at.line),
                    name: candidate.name.clone(),
                    kind: candidate.kind.clone(),
                    package: candidate.package.clone(),
                    file: candidate.at.path.clone(),
                    line: candidate.at.line,
                    column: candidate.at.column,
                    through: candidate.at.line,
                    signature: candidate.signature.clone(),
                    uses: 0,
                    used_at: Vec::new(),
                    source: "rust-analyzer",
                })
                .collect();
            remember_spans(provider, &entries);
            return Ok(Some(Answered {
                why: resolution.ambiguity(&name),
                entries,
                source: "rust-analyzer",
                fresh,
                resolution: "ambiguous",
            }));
        }

        let Some(known) = resolution.only() else {
            return Ok(Some(Answered {
                entries: Vec::new(),
                source: "rust-analyzer",
                fresh,
                resolution: "nothing",
                why: format!("nothing in this workspace declares `{name}`"),
            }));
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
            column: at.column,
            through,
            signature: known.signature.clone(),
            uses: known.used.len(),
            used_at: known
                .used
                .iter()
                .take(asked.uses)
                .map(|at| format!("{}:{}", at.path, at.line))
                .collect(),
            source: "rust-analyzer",
        }];
        remember_spans(provider, &entries);
        Ok(Some(Answered {
            entries,
            source: "rust-analyzer",
            fresh,
            resolution: "one",
            why: String::new(),
        }))
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
            // The index knows which line and not which column: it matched a
            // name in a file. Said as 1 rather than left out, because an
            // absent column would make `at` two different shapes depending on
            // which provider answered.
            column: 1,
            through: definition.line as u32,
            signature: None,
            uses,
            used_at: found
                .uses
                .iter()
                .take(asked.uses)
                .map(|at| format!("{}:{}", at.path, at.line))
                .collect(),
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
        let at = place(provider, &tree, &anchor)?;
        provider
            .rename_texts(&at.file, at.line, at.column, &to)
            .map(|texts| (texts, at))
            .map_err(|error| Unplaced::of("unresolved", error.to_string()))
    });

    let (texts, at) = match resolved {
        Ok(all) => all,
        Err(why) => {
            // **Nothing has been written at this point, and that is the whole
            // claim.** `place` runs before `rename_texts`, `rename_texts`
            // writes nothing anywhere, and the loop that opens files is below
            // both. A rename that met three candidates leaves the workspace
            // byte for byte what it was, and says which three.
            if face.is_machine() {
                face.say(thalyx_files::machine::refused_with(
                    RENAME_OP,
                    why.word,
                    if why.word == "ambiguous" {
                        "name_one_candidate"
                    } else {
                        "ask_context"
                    },
                    &why.message,
                    vec![
                        ("from", json!(anchor)),
                        ("to", json!(to)),
                        ("candidates", json!(why.candidates)),
                        ("files_changed", json!(0)),
                    ],
                ));
            } else {
                println!("\n  {}\n", why.message);
            }
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
    for change in &texts {
        let path = &change.path;
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

    // Built while the edits are applied, from the plan rust-analyzer already
    // handed over. Never by scanning the tree afterwards: a second pass would be
    // a textual count of a string, which is the answer this verb exists to be
    // better than, and it would be wrong wherever the new name was already
    // there for some other reason.
    let mut written = Vec::with_capacity(texts.len());
    let mut edits_by_file = Vec::with_capacity(texts.len());
    for (held, change) in anchored.iter().zip(texts.iter()) {
        let (path, text) = (&change.path, &change.text);
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
        let named = path
            .strip_prefix(&tree)
            .unwrap_or(path)
            .display()
            .to_string();
        edits_by_file.push(json!({"path": named, "edits": change.edits}));
        written.push(named);
    }

    let where_it_started = format!(
        "{}:{}:{}",
        at.file.strip_prefix(&tree).unwrap_or(&at.file).display(),
        at.line,
        at.column
    );
    let total: usize = edits_by_file
        .iter()
        .filter_map(|entry| entry["edits"].as_u64())
        .map(|count| count as usize)
        .sum();

    if face.is_machine() {
        let mut carried = vec![
            ("from", json!(anchor)),
            ("to", json!(to)),
            ("resolved_at", json!(where_it_started)),
            ("files", json!(written)),
            ("files_changed", json!(written.len())),
            // Per file, in the order the files were written, which is the order
            // rust-analyzer listed them. A caller that wants to know what a
            // rename did no longer needs a second question to find out.
            ("edits_by_file", json!(edits_by_file)),
            ("edits", json!(total)),
            ("source", json!("rust-analyzer")),
        ];
        // **Only when it is really known.** Given a name, this verb reaches the
        // place through the symbol's declaration and the answer can say so.
        // Given `file:line:column`, the caller pointed somewhere and this has no
        // idea whether that is the declaration or one of its uses — so the field
        // is absent rather than a guess, and a caller can tell the difference
        // between "it is here" and "nobody asked".
        if at.is_the_declaration {
            carried.push(("definition", json!(where_it_started)));
        }
        face.say(thalyx_files::machine::answer(RENAME_OP, carried));
    } else {
        println!(
            "\n  {} renamed to {} — {} edit(s) across {} file(s)\n",
            anchor,
            to,
            total,
            written.len()
        );
    }
    Ok(())
}

/// Why a name could not be turned into a place.
///
/// A word and not only a sentence, because the caller that matters most here
/// is a **program**: `renombrar` inside `hacer` is a step whose answer another
/// step branches on, and "ambiguous, here are three handles" and "there is no
/// such name" call for opposite next moves. A program handed one string for
/// both would have to match on prose.
pub struct Unplaced {
    pub word: &'static str,
    pub message: String,
    /// The candidates, when the reason was that there were several.
    pub candidates: Vec<Value>,
}

impl Unplaced {
    fn of(word: &'static str, message: String) -> Self {
        Self {
            word,
            message,
            candidates: Vec::new(),
        }
    }
}

/// Where the thing to rename is: a position if the caller gave one, and
/// otherwise the declaration of the name.
/// A place in the tree, and whether it is known to be where the name is
/// *declared*.
///
/// The two cases really are different and were reported as one. Given a name,
/// the resolution comes back with `defined`, and the place taken is the
/// declaration — that is a fact about the symbol. Given `file:line:column`, the
/// caller pointed at somewhere, and that somewhere is very often a use site.
/// Answering `definition` for both would be inventing the half that is not
/// known, which is the one thing a caller of a semantic surface must be able to
/// rule out.
struct Placed {
    file: PathBuf,
    line: u32,
    column: u32,
    /// True only when this place was reached *through* the name's declaration.
    is_the_declaration: bool,
}

fn place(provider: &mut Provider, tree: &Path, anchor: &str) -> Result<Placed, Unplaced> {
    let parts: Vec<&str> = anchor.rsplitn(3, ':').collect();
    if parts.len() == 3
        && let (Ok(column), Ok(line)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>())
    {
        let file = tree.join(parts[2]);
        if file.is_file() {
            // The caller pointed here. Nothing on this path knows whether it is
            // a declaration or a use, and nothing here is going to guess.
            return Ok(Placed {
                file,
                line,
                column,
                is_the_declaration: false,
            });
        }
        return Err(Unplaced::of(
            "absent",
            format!("{} is not a file of this workspace", parts[2]),
        ));
    }

    let (resolution, standing, _) = provider
        .known(anchor)
        .map_err(|error| Unplaced::of("unresolved", error.to_string()))?;

    // **Refused before anything is written, and refused by name.**
    //
    // The alternative was already in this file: `ask_about` took the first
    // exact match rust-analyzer listed. A workspace with `crate_a::Config`,
    // `crate_b::Config` and `crate_c::Config` in it would have had one of the
    // three renamed across every file that uses it, chosen by index order, and
    // the answer would have said `source: rust-analyzer` — which is true and
    // is the reason it would have been believed.
    //
    // There is no heuristic here on purpose. A mutation is exactly the place
    // where "probably this one" is worth less than nothing: a wrong guess
    // costs a rollback and a lost round trip, and a *right* guess teaches the
    // caller that the guessing is reliable.
    if resolution.is_ambiguous() {
        return Err(Unplaced {
            word: "ambiguous",
            message: resolution.ambiguity(anchor),
            candidates: resolution
                .candidates()
                .iter()
                .map(|candidate| {
                    json!({
                        "name": candidate.name,
                        "kind": candidate.kind,
                        "crate": candidate.package,
                        "container": candidate.container,
                        "at": candidate.handle,
                        "file": candidate.at.path,
                        "line": candidate.at.line,
                        "signature": candidate.signature,
                    })
                })
                .collect(),
        });
    }

    let known = resolution.only().ok_or_else(|| {
        Unplaced::of(
            "unresolved",
            format!("nothing in this workspace declares `{anchor}`"),
        )
    })?;
    if matches!(standing, Standing::Stale { .. }) {
        // Cannot happen through `known`, which never returns a stale answer;
        // written so that a future path that could is refused rather than
        // renaming against a tree that has moved.
        return Err(Unplaced::of(
            "stale",
            format!("what is known about `{anchor}` is out of date"),
        ));
    }
    let at = known.defined.first().ok_or_else(|| {
        Unplaced::of(
            "unresolved",
            format!("`{anchor}` is known but has no declaration"),
        )
    })?;
    Ok(Placed {
        file: tree.join(&at.path),
        line: at.line,
        column: at.column,
        // Reached through `known.defined`, which is the declaration and nothing
        // else.
        is_the_declaration: true,
    })
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
            column: 8,
            through: lines,
            signature: Some(format!("pub fn {name}() -> Result<()>")),
            uses: 17,
            used_at: Vec::new(),
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
