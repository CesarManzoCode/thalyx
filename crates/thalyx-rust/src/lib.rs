//! The Rust programming face: what Cargo and rust-analyzer already know, under
//! Thalyx's rules about freshness, budget and authority.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md` says programming is the
//! proving ground and not Thalyx's identity, so everything Rust-shaped is
//! here and nothing Rust-shaped is in the core. What the core keeps is
//! general: persistent state, a witness, a budget, a transaction, an authority
//! that decides. This crate is the first provider plugged into all of that, and
//! it is deliberately the *only* one — a plugin architecture written before its
//! second plugin is a guess about the second plugin.
//!
//! ## The division of labour, in one line each
//!
//! - **Cargo** says what the workspace is and which crate depends on which.
//! - **rust-analyzer** says what a name *is* — the question a scan cannot
//!   answer and has never been able to.
//! - **`thalyx-know`** says whether either answer still holds.
//! - **Thalyx** decides what is done about it, and is the only thing that
//!   writes.
//!
//! ## Nothing here is believed without its standing
//!
//! Every answer this crate returns carries where it came from and whether it is
//! current. A frontier model told "`Store::lock` is at store.rs:212" is going
//! to act on it; the same model told the index was built before four files
//! changed can decide to spend a call refreshing. The failure mode this closes
//! is not a wrong answer, it is a **confident** one.

pub mod affected;
pub mod analyzer;
pub mod edits;
pub mod metadata;
pub mod toolchain;

pub use affected::{Affected, affected};
pub use analyzer::{Analyzer, FileEdit, Ready, Spot, Symbol};
pub use metadata::{Package, Workspace};

/// One file a rename would rewrite, and how much of it.
///
/// The count is the whole reason this is a struct rather than the pair it was.
/// rust-analyzer answers a rename with a `WorkspaceEdit` — every file, and
/// inside each one every range it decided really refers to the symbol — and
/// applying it collapsed all of that into one new string per file. So a caller
/// was told six files changed and had no way to learn that one of them changed
/// in three places except by searching the tree again, textually, for the
/// answer the compiler frontend had already given exactly.
pub struct Renamed {
    pub path: PathBuf,
    /// What the file should say afterwards.
    pub text: String,
    /// How many separate places in this file the rename rewrote.
    pub edits: usize,
}

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thalyx_know::{Knowledge, Standing, Witness};

#[derive(Debug, thiserror::Error)]
pub enum RustError {
    #[error("{0} is not a Cargo workspace: there is no Cargo.toml in it")]
    NotACargoWorkspace(String),
    #[error("there is no `cargo` on this machine: {0}")]
    NoCargo(String),
    #[error("cargo could not describe this workspace: {0}")]
    Cargo(String),
    #[error("there is no rust-analyzer on this machine: {0}")]
    NoAnalyzer(String),
    #[error("rust-analyzer did not answer: {0}")]
    Silent(String),
    #[error("rust-analyzer refused: {0}")]
    Refused(String),
    /// The tree changed underneath the question. Its own variant rather than a
    /// refusal, because the protocol's answer to it is *ask again* and a caller
    /// that could not tell them apart would give up on a retryable question.
    #[error("the file changed while rust-analyzer was answering: {0}")]
    Moved(String),
    #[error("{0} is outside the workspace, and nothing here may touch it")]
    Outside(String),
    #[error("{0}")]
    Unreadable(String),
    #[error("the machine's memory could not be read: {0}")]
    Know(#[from] thalyx_know::KnowError),
}

pub type Result<T> = std::result::Result<T, RustError>;

/// The kinds of fact this provider remembers. Constants because the string is
/// the key: a kind spelled two ways is two caches, and the second one is always
/// the empty one somebody is confused by.
pub const KIND_WORKSPACE: &str = "rust.workspace";
/// What is remembered about one name.
///
/// **`.2` and not `.1`.** What is stored under this key changed shape on
/// 2026-08-30 — from "the one declaration, or nothing" to "nothing, one, or
/// several" — and a store written by the older shape would deserialise as a
/// miss on every question, forever, in silence. A new key throws the old
/// entries away once instead of tripping over them every time. Rule 9: the
/// cautious answer.
pub const KIND_SYMBOL: &str = "rust.symbol.2";
pub const KIND_IDENTITY: &str = "rust.identity";
pub const KIND_OUTLINE: &str = "rust.outline";
pub const KIND_VALIDATION: &str = "rust.validation";

/// Where something is, in the form an answer carries it.
///
/// Paths relative to the workspace root and lines one-based — the surface's
/// coordinates, converted exactly once, here. LSP's zero-based lines never
/// leave [`analyzer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct At {
    pub path: String,
    pub line: u32,
    pub column: u32,
}

impl At {
    fn of(spot: &Spot, root: &Path) -> Self {
        At {
            path: spot
                .path
                .strip_prefix(root)
                .unwrap_or(&spot.path)
                .display()
                .to_string(),
            line: spot.line + 1,
            column: spot.character + 1,
        }
    }
}

/// One declaration of a file, and how far it reaches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outlined {
    pub name: String,
    pub kind: String,
    pub container: Option<String>,
    /// Where it starts, one-based.
    pub at: At,
    /// The last line it occupies, one-based and inclusive.
    pub through: u32,
}

/// What is known about one name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Known {
    pub name: String,
    pub kind: String,
    /// The crate it is declared in, when it is inside one.
    pub package: Option<String>,
    pub defined: Vec<At>,
    pub signature: Option<String>,
    /// Every use, the declaration included. Kept whole in the machine; the
    /// surface returns a count and a window of it.
    pub used: Vec<At>,
}

/// One of the declarations a name could mean, when there is more than one.
///
/// Enough to choose between them without opening a file — which package, which
/// module, what kind of thing, what it looks like — and a handle that names
/// exactly this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub name: String,
    pub kind: String,
    /// The crate it is declared in, when it is inside one.
    pub package: Option<String>,
    /// The enclosing name the server gave, when it gave one.
    pub container: Option<String>,
    pub at: At,
    pub signature: Option<String>,
    /// The identity that resolves this one and no other: `path:line:column`.
    ///
    /// **Deliberately the shape `renombrar` and `contexto` already take.** A
    /// new kind of handle would be a second identity for a place, and the
    /// caller resolving an ambiguity would have to learn one thing to ask the
    /// question and another to answer it.
    pub handle: String,
}

/// What a name turns out to be.
///
/// Three answers and not two. Until 2026-08-30 this was `Option<Known>`, and
/// `ask_about` produced it with `candidates.into_iter().find(|s| s.name ==
/// name)` — so a workspace with `crate_a::Config`, `crate_b::Config` and
/// `crate_c::Config` in it got whichever one rust-analyzer happened to list
/// first, described as *the* `Config`, with nothing anywhere saying a choice
/// had been made.
///
/// That is the worst shape a wrong answer can have: it is confident, it is
/// well formed, and the caller that acts on it is a rename. So the several
/// case is a value the caller has to handle, and a mutation against it refuses
/// before it writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "resolution", rename_all = "snake_case")]
pub enum Resolution {
    /// Nothing in this workspace declares it.
    Nothing,
    /// Exactly one declaration, and everything known about it.
    One { known: Box<Known> },
    /// Several, and choosing between them is not this machine's to do.
    Several { candidates: Vec<Candidate> },
}

impl Resolution {
    /// The one declaration, when there is exactly one.
    ///
    /// Named `only` rather than `known` so that a caller writing it is
    /// visibly asserting the thing that might not be true.
    pub fn only(&self) -> Option<&Known> {
        match self {
            Resolution::One { known } => Some(known),
            _ => None,
        }
    }

    /// The candidates, when there is more than one.
    pub fn candidates(&self) -> &[Candidate] {
        match self {
            Resolution::Several { candidates } => candidates,
            _ => &[],
        }
    }

    pub fn is_ambiguous(&self) -> bool {
        matches!(self, Resolution::Several { .. })
    }

    /// One sentence naming what could not be chosen between.
    pub fn ambiguity(&self, name: &str) -> String {
        let listed: Vec<String> = self
            .candidates()
            .iter()
            .map(|candidate| {
                let package = candidate.package.as_deref().unwrap_or("this workspace");
                format!(
                    "{} {} in {package} ({})",
                    candidate.kind, candidate.name, candidate.handle
                )
            })
            .collect();
        format!(
            "`{name}` names {} declarations here and this machine will not choose \
             between them: {}. Ask again with one of the handles — \
             `path:line:column` — which names exactly one",
            listed.len(),
            listed.join("; ")
        )
    }
}

/// What a query cost and where the answer came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    /// Questions asked of this provider.
    pub queries: usize,
    /// Answered from what the machine already knew, with the state unchanged.
    pub hits: usize,
    /// Answered by asking rust-analyzer.
    pub misses: usize,
    /// Times a rust-analyzer was started. The expensive number: about 25
    /// seconds on this workspace, and the reason the cache exists.
    pub analyzer_starts: usize,
    /// Whether the running analyzer is under Thalyx's confinement.
    ///
    /// `None` when none is running. **Never assumed**: a machine whose kernel
    /// cannot deny runs the provider as a host process, and an answer that did
    /// not say so would let a reader believe a whole compiler tree had been
    /// confined when it had not. See `analyzer::Spawn`.
    pub analyzer_confined: Option<bool>,
    /// One phrase saying what started it: `confined: <profile>`, or `host`.
    pub analyzer_how: Option<String>,
    /// Times Cargo was asked to describe the workspace.
    pub cargo_calls: usize,
}

/// One tree, everything this machine knows about it, and the two tools that
/// can learn more.
pub struct Provider {
    root: PathBuf,
    /// Where the tools this provider runs are told to put build output.
    ///
    /// Outside the workspace, always, when the caller can name a place: see
    /// [`Analyzer::start`]. `None` means Cargo's default, which is inside the
    /// tree — right for a test with nowhere else to put it, wrong for a
    /// workspace anybody snapshots.
    build_into: Option<PathBuf>,
    knowledge: Knowledge,
    workspace: Option<Workspace>,
    analyzer: Option<Analyzer>,
    /// Why there is no analyzer, once we have tried and failed. Kept so the
    /// second question does not pay the 25 seconds again to be told the same
    /// thing — and so the answer can say *which* failure it was.
    analyzer_refused: Option<String>,
    /// Who starts the analyzer's process, and under whose authority.
    ///
    /// Defaults to [`analyzer::OnTheHost`], which is what a crate that knows
    /// nothing about cgroups can do by itself and which says `confined: false`
    /// on every answer. The authority that can do better hands one in with
    /// [`Provider::spawning`].
    spawner: std::sync::Arc<dyn analyzer::Spawn>,
    /// Where the analyzer must be able to read outside the workspace.
    readable: Vec<PathBuf>,
    /// Where its toolchain is, for a process that is not the user who
    /// installed it.
    environment: Vec<(String, String)>,
    pub tally: Tally,
}

impl Provider {
    pub fn open(root: &Path, knowledge: Knowledge) -> Self {
        Self {
            root: root.to_path_buf(),
            build_into: None,
            knowledge,
            workspace: None,
            analyzer: None,
            analyzer_refused: None,
            spawner: std::sync::Arc::new(analyzer::OnTheHost),
            readable: Vec::new(),
            environment: Vec::new(),
            tally: Tally::default(),
        }
    }

    /// The same, with somebody else starting the analyzer's process.
    ///
    /// The authority above this crate hands in a spawner that puts the server
    /// — and every `cargo`, `rustc` and build script under it — in a cgroup
    /// with a policy, a private root filesystem holding only the workspace and
    /// the toolchain, no network, its own user and the seccomp filter. See
    /// [`analyzer::Spawn`].
    pub fn spawning(mut self, spawner: std::sync::Arc<dyn analyzer::Spawn>) -> Self {
        self.spawner = spawner;
        self
    }

    /// What the analyzer must be able to read outside the workspace, and where
    /// its toolchain is.
    ///
    /// Both, together, because they are the same fact twice: a grant on
    /// `~/.cargo` and a `CARGO_HOME` that names it are the permission and the
    /// address, and one without the other is a process that may read a
    /// directory it will never look in.
    pub fn reaching(mut self, readable: Vec<PathBuf>, environment: Vec<(String, String)>) -> Self {
        self.readable = readable;
        self.environment = environment;
        self
    }

    /// Whether the running analyzer is under Thalyx's confinement.
    ///
    /// `None` when none is running. Reported rather than assumed: a machine
    /// that cannot enforce runs the provider on the host, and an answer that
    /// did not say so would let a reader believe a compiler tree had been
    /// confined when it had not.
    pub fn analyzer_confined(&self) -> Option<bool> {
        self.analyzer.as_ref().map(Analyzer::confined)
    }

    /// One phrase saying what started the running analyzer.
    pub fn analyzer_how(&self) -> Option<&str> {
        self.analyzer.as_ref().map(Analyzer::how)
    }

    /// The same, told where to put anything it builds.
    pub fn building_into(mut self, target: &Path) -> Self {
        self.build_into = Some(target.to_path_buf());
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn knowledge(&self) -> &Knowledge {
        &self.knowledge
    }

    /// What Cargo says this workspace is, from memory when the manifests have
    /// not moved.
    ///
    /// The witness is the manifests alone: `cargo metadata --no-deps` reads
    /// nothing else, so a change to a `.rs` file cannot change its answer and
    /// invalidating on one would be a false miss on every edit.
    pub fn workspace(&mut self) -> Result<&Workspace> {
        if self.workspace.is_none() {
            let key = self.root.display().to_string();
            let manifests = self.manifest_witness();
            let remembered = self
                .knowledge
                .recall_current(KIND_WORKSPACE, &key, &manifests)?
                .and_then(|held| Workspace::parse(&held.value).ok());
            let workspace = match remembered {
                Some(workspace) => {
                    self.tally.hits += 1;
                    workspace
                }
                None => {
                    let root = self.root.clone();
                    let build_into = self.build_into.clone();
                    let (text, identity) = self.steady(
                        |provider| Ok(provider.manifest_witness()),
                        |provider| {
                            provider.tally.cargo_calls += 1;
                            provider.tally.misses += 1;
                            raw_metadata(&root, build_into.as_deref())
                        },
                    )?;
                    let workspace = Workspace::parse(&text)?;
                    if let Some(identity) = identity {
                        self.knowledge
                            .remember(KIND_WORKSPACE, &key, &identity, "cargo", &text)?;
                    }
                    workspace
                }
            };
            self.workspace = Some(workspace);
        }
        Ok(self.workspace.as_ref().expect("just set"))
    }

    /// The identity of the files `cargo metadata --no-deps` reads.
    ///
    /// The manifests alone: a change to a `.rs` file cannot change what Cargo
    /// says the workspace is, and invalidating on one would be a false miss on
    /// every edit anybody ever makes.
    fn manifest_witness(&self) -> Witness {
        thalyx_know::witness(&thalyx_know::Over {
            roots: std::slice::from_ref(&self.root),
            suffixes: &["Cargo.toml", "Cargo.lock"],
            skip: affected::NOT_SOURCE,
        })
    }

    /// Do something that reads the tree, and say which tree it actually read.
    ///
    /// The identity is taken before **and** after, and comes back only when
    /// they agree. Without that this cache stores answers about a tree that no
    /// longer exists and calls them current — which is not hypothetical: the
    /// first thing rust-analyzer does on a workspace with no `Cargo.lock` is
    /// write one, so the very first question of a session changes the tree it
    /// is a question about. That made the second session miss every time, and
    /// the only reason it was caught is that the test counted the starts.
    ///
    /// One retry, because the settling is a one-off. Still moving after that
    /// means something else is writing, and then the answer is handed back with
    /// **no identity at all** rather than with a wrong one. `false miss =
    /// slower, false hit = wrong`.
    fn steady<T>(
        &mut self,
        mut identity: impl FnMut(&mut Self) -> Result<Witness>,
        mut ask: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<(T, Option<Witness>)> {
        let mut answer = None;
        for last in [false, true] {
            let before = identity(self)?;
            let got = ask(self)?;
            let after = identity(self)?;
            if before.id == after.id {
                return Ok((got, Some(after)));
            }
            answer = Some(got);
            if last {
                break;
            }
        }
        Ok((answer.expect("the loop ran"), None))
    }

    /// The identity of everything a semantic answer about this tree depends on.
    pub fn source_witness(&mut self) -> Result<Witness> {
        let workspace = self.workspace()?;
        Ok(affected::source_identity(workspace))
    }

    /// Everything known about one name — from memory when the sources have not
    /// changed, and from rust-analyzer when they have.
    ///
    /// Returns the answer **and its standing**, so a caller can never end up
    /// relaying a location from before four files moved without saying so.
    pub fn known(&mut self, name: &str) -> Result<(Resolution, Standing, String)> {
        self.tally.queries += 1;
        let witness = self.source_witness()?;
        if let Some(held) = self.knowledge.recall_current(KIND_SYMBOL, name, &witness)?
            && let Ok(resolution) = serde_json::from_str::<Resolution>(&held.value)
        {
            self.tally.hits += 1;
            return Ok((resolution, Standing::Current, held.source));
        }

        self.tally.misses += 1;
        let name = name.to_string();
        let (resolution, identity) = self.steady(
            |provider| provider.source_witness(),
            |provider| provider.ask_about(&name),
        )?;

        let value = serde_json::to_string(&resolution)
            .unwrap_or_else(|_| r#"{"resolution":"nothing"}"#.into());
        match identity {
            Some(identity) => {
                self.knowledge
                    .remember(KIND_SYMBOL, &name, &identity, "rust-analyzer", &value)?;
                Ok((resolution, Standing::Current, "rust-analyzer".to_string()))
            }
            // Nothing is remembered, and the caller is told the answer has no
            // standing rather than being handed one it can rely on.
            None => Ok((resolution, Standing::Unknown, "rust-analyzer".to_string())),
        }
    }

    /// Ask rust-analyzer everything a repo map wants about one name.
    ///
    /// One place, so that the definition, the signature and the uses are always
    /// about the same declaration. Three call sites picking their own would be
    /// three chances to describe one symbol and cite another.
    fn ask_about(&mut self, name: &str) -> Result<Resolution> {
        let root = self.root.clone();
        let analyzer = self.analyzer()?;
        let mut exact: Vec<Symbol> = analyzer
            .symbols_named(name)?
            .into_iter()
            .filter(|symbol| symbol.name == name)
            .collect();

        // Two entries at one place are one declaration. rust-analyzer answers a
        // workspace-symbol query out of several indexes and the same item can
        // come back twice; counting that as an ambiguity would refuse a rename
        // over a workspace that has exactly one `Config` in it, which is a
        // false alarm nobody can act on. Sorted first so the order a caller
        // sees is the same on every run.
        exact.sort_by(|left, right| {
            (&left.at.path, left.at.line, left.at.character).cmp(&(
                &right.at.path,
                right.at.line,
                right.at.character,
            ))
        });
        exact.dedup_by(|left, right| {
            left.at.path == right.at.path
                && left.at.line == right.at.line
                && left.at.character == right.at.character
        });

        match exact.len() {
            0 => Ok(Resolution::Nothing),
            1 => {
                let declaration = exact.remove(0);
                let at = declaration.at.clone();
                let signature = analyzer.signature(&at.path, at.line, at.character)?;
                let used = analyzer.references(&at.path, at.line, at.character)?;
                let package = self
                    .workspace()?
                    .package_of(&at.path)
                    .map(|package| package.name.clone());
                Ok(Resolution::One {
                    known: Box::new(Known {
                        name: name.to_string(),
                        kind: declaration.kind.to_string(),
                        package,
                        defined: vec![At::of(&at, &root)],
                        signature,
                        used: used.iter().map(|spot| At::of(spot, &root)).collect(),
                    }),
                })
            }
            // Several. The signature of each is asked for and the *references*
            // of none are: a caller looking at this has not decided which
            // symbol it means yet, and asking rust-analyzer for the use sites
            // of three declarations to answer a question about which one is
            // meant is paying three times for an answer that is thrown away.
            _ => {
                // Everything the server has to be asked, asked while the
                // server is borrowed; the workspace lookup happens after,
                // because it borrows `self` too and a loop that alternated
                // between them would not compile without releasing one of them
                // every time round.
                let mut described = Vec::with_capacity(exact.len());
                for declaration in exact {
                    let at = declaration.at.clone();
                    let signature = analyzer.signature(&at.path, at.line, at.character)?;
                    described.push((declaration, signature));
                }

                let mut candidates = Vec::with_capacity(described.len());
                for (declaration, signature) in described {
                    let package = self
                        .workspace()?
                        .package_of(&declaration.at.path)
                        .map(|package| package.name.clone());
                    let placed = At::of(&declaration.at, &root);
                    candidates.push(Candidate {
                        name: declaration.name.clone(),
                        kind: declaration.kind.to_string(),
                        package,
                        container: declaration.container.clone(),
                        handle: format!("{}:{}:{}", placed.path, placed.line, placed.column),
                        at: placed,
                        signature,
                    });
                }
                Ok(Resolution::Several { candidates })
            }
        }
    }

    /// What the name written at this exact place *is*.
    ///
    /// **The question the index has never been able to answer.** `Keys` in
    /// `fn boot() -> Keys` is `Keystore`, because three files up somebody wrote
    /// `use crate::keystore::Keystore as Keys`, and no amount of scanning turns
    /// one name into the other. rust-analyzer resolves the binding, which is
    /// what a compiler is for.
    pub fn identity_at(&mut self, file: &Path, line: u32, column: u32) -> Result<Vec<At>> {
        self.tally.queries += 1;
        let witness = self.source_witness()?;
        let relative = file
            .strip_prefix(&self.root)
            .unwrap_or(file)
            .display()
            .to_string();
        let key = format!("{relative}:{line}:{column}");
        if let Some(held) = self
            .knowledge
            .recall_current(KIND_IDENTITY, &key, &witness)?
            && let Ok(found) = serde_json::from_str::<Vec<At>>(&held.value)
        {
            self.tally.hits += 1;
            return Ok(found);
        }
        self.tally.misses += 1;
        let root = self.root.clone();
        let file = file.to_path_buf();
        let (found, identity) = self.steady(
            |provider| provider.source_witness(),
            |provider| {
                let analyzer = provider.analyzer()?;
                // One-based at the surface, zero-based on the wire, converted
                // here and nowhere else.
                let spots =
                    analyzer.definition(&file, line.saturating_sub(1), column.saturating_sub(1))?;
                Ok(spots
                    .iter()
                    .map(|spot| At::of(spot, &root))
                    .collect::<Vec<At>>())
            },
        )?;
        if let Some(identity) = identity {
            let value = serde_json::to_string(&found).unwrap_or_else(|_| "[]".into());
            self.knowledge
                .remember(KIND_IDENTITY, &key, &identity, "rust-analyzer", &value)?;
        }
        Ok(found)
    }

    /// Everything one file declares, with the whole extent of each declaration.
    ///
    /// The extent is what makes progressive disclosure possible: a repo map
    /// entry is a name and a signature, and expanding it means handing back
    /// *exactly* the lines the declaration occupies rather than the file it is
    /// in. Without a real end line an expansion is a guess with a window
    /// around it.
    pub fn outline(&mut self, file: &Path) -> Result<Vec<Outlined>> {
        self.tally.queries += 1;
        let witness = self.source_witness()?;
        let relative = file
            .strip_prefix(&self.root)
            .unwrap_or(file)
            .display()
            .to_string();
        if let Some(held) = self
            .knowledge
            .recall_current(KIND_OUTLINE, &relative, &witness)?
            && let Ok(found) = serde_json::from_str::<Vec<Outlined>>(&held.value)
        {
            self.tally.hits += 1;
            return Ok(found);
        }
        self.tally.misses += 1;
        let root = self.root.clone();
        let file = file.to_path_buf();
        let (found, identity) = self.steady(
            |provider| provider.source_witness(),
            |provider| {
                let analyzer = provider.analyzer()?;
                Ok(analyzer
                    .symbols_in(&file)?
                    .iter()
                    .map(|symbol| Outlined {
                        name: symbol.name.clone(),
                        kind: symbol.kind.to_string(),
                        container: symbol.container.clone(),
                        at: At::of(&symbol.at, &root),
                        through: symbol.at.end_line + 1,
                    })
                    .collect::<Vec<Outlined>>())
            },
        )?;
        if let Some(identity) = identity {
            let value = serde_json::to_string(&found).unwrap_or_else(|_| "[]".into());
            self.knowledge
                .remember(KIND_OUTLINE, &relative, &identity, "rust-analyzer", &value)?;
        }
        Ok(found)
    }

    /// What renaming the symbol at this place would change, described.
    ///
    /// Never cached: it is asked once, immediately before it is applied inside
    /// a transaction, and a remembered plan is a plan against a tree that may
    /// have moved between remembering and applying. The one place where paying
    /// again is cheaper than being wrong.
    pub fn rename_plan(
        &mut self,
        file: &Path,
        line: u32,
        column: u32,
        to: &str,
    ) -> Result<Vec<FileEdit>> {
        self.tally.queries += 1;
        self.tally.misses += 1;
        let analyzer = self.analyzer()?;
        analyzer.rename(file, line.saturating_sub(1), column.saturating_sub(1), to)
    }

    /// The text every file would hold after a rename, without writing any of
    /// it.
    ///
    /// The split is the point: this crate produces *what the files should say*,
    /// and the authority above it decides whether that gets written, inside
    /// which transaction, and past which boundary. rust-analyzer never touches
    /// the tree.
    pub fn rename_texts(
        &mut self,
        file: &Path,
        line: u32,
        column: u32,
        to: &str,
    ) -> Result<Vec<Renamed>> {
        let plan = self.rename_plan(file, line, column, to)?;
        let utf8 = self
            .analyzer
            .as_ref()
            .map(Analyzer::utf8_columns)
            .unwrap_or(false);
        let mut written = Vec::new();
        for change in plan {
            let text = std::fs::read_to_string(&change.path).map_err(|error| {
                RustError::Unreadable(format!("{}: {error}", change.path.display()))
            })?;
            let after = edits::applied(&text, &change.edits, utf8)
                .map_err(|why| RustError::Refused(format!("{}: {why}", change.path.display())))?;
            written.push(Renamed {
                path: change.path,
                text: after,
                // Counted here, from the plan rust-analyzer handed over, and
                // never by looking at the tree afterwards. A second pass would
                // be a different question — it would count occurrences of a
                // string, which is exactly the textual answer this whole path
                // exists to avoid, and it would be wrong wherever the new name
                // already appeared for some other reason.
                edits: change.edits.len(),
            });
        }
        Ok(written)
    }

    /// Whether a rust-analyzer is running, without starting one.
    pub fn analyzer_running(&self) -> bool {
        self.analyzer.is_some()
    }

    /// The running rust-analyzer, started if this is the first question.
    ///
    /// Started once per process and kept: the cost is the start, not the
    /// query — 25 seconds against 20 milliseconds on this workspace — so a
    /// design that started one per question would be a design where asking two
    /// questions costs twice as much as asking one, which is the opposite of
    /// what `hacer` is for.
    fn analyzer(&mut self) -> Result<&mut Analyzer> {
        if let Some(why) = &self.analyzer_refused {
            return Err(RustError::NoAnalyzer(why.clone()));
        }
        if self.analyzer.is_none() {
            let Some(binary) = analyzer::find() else {
                // Naming every place that was looked at, and not just the
                // absence. On 2026-08-29 this said "no rust-analyzer" on a
                // machine where `rustup component add rust-analyzer` had just
                // succeeded — because `sudo` had made `$HOME` be `/root` and
                // the sentence gave the person nothing to notice that with.
                let why = analyzer::why_no_analyzer();
                self.analyzer_refused = Some(why.clone());
                return Err(RustError::NoAnalyzer(why));
            };
            self.tally.analyzer_starts += 1;
            match Analyzer::start(
                &self.root,
                &binary,
                self.build_into.as_deref(),
                &self.readable,
                &self.environment,
                self.spawner.as_ref(),
            ) {
                Ok(analyzer) => {
                    // Recorded at the moment of the start, so a caller that
                    // subtracts two tallies around a request still learns what
                    // stood behind the process that answered it.
                    self.tally.analyzer_confined = Some(analyzer.confined());
                    self.tally.analyzer_how = Some(analyzer.how().to_string());
                    self.analyzer = Some(analyzer);
                }
                Err(error) => {
                    self.analyzer_refused = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        Ok(self.analyzer.as_mut().expect("just set"))
    }
}

fn raw_metadata(root: &Path, build_into: Option<&Path>) -> Result<String> {
    let mut command = std::process::Command::new(metadata::cargo());
    if let Some(target) = build_into {
        command.env("CARGO_TARGET_DIR", target);
    }
    let output = command
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .output()
        .map_err(|error| RustError::NoCargo(error.to_string()))?;
    if !output.status.success() {
        return Err(RustError::Cargo(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What compiles here, as a string a cache identity can be made of.
///
/// Rule 12, as a value: the same bytes compiled by a different toolchain is a
/// different answer, and a build with a different configuration is a different
/// system. Asked once per process — it cannot change under a running one.
pub fn toolchain() -> String {
    static ASKED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ASKED
        .get_or_init(|| {
            std::process::Command::new("rustc")
                .arg("--version")
                .arg("--verbose")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                // Not a default that pretends: a toolchain nobody could
                // identify makes every identity different from every other, so
                // nothing is ever reused. Rule 9's cautious answer.
                .unwrap_or_else(|| format!("unknown-toolchain-{}", std::process::id()))
        })
        .clone()
}
