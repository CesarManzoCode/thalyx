//! What a programming agent outside the machine is allowed to be.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md`. During the adoption phase
//! the agent — Claude Code, today, on the host's Fedora — is not inside Thalyx.
//! It reaches in through one channel, and this module is what it reaches: a
//! session with a root it cannot leave, a set of verbs it cannot grow, and a
//! provenance that follows everything it does into the journal.
//!
//! ## The boundary is the reason this file exists
//!
//! Being connected to a port is not authority. A program on the other end of a
//! character device is exactly as trustworthy as whatever wrote to that device,
//! and on a developer's machine that is a language model with a prompt in it.
//! So the answer to "what may it do" is written here as a **list**, and the list
//! is short:
//!
//! - it stands in one directory and never moves;
//! - every path it names is resolved the way the verb will resolve it and then
//!   the way the *kernel* will, and both must land inside that directory;
//! - the verbs it may reach are named one by one, and every one of them is a
//!   verb the machine already had;
//! - nothing that changes the machine rather than the workspace is on the list:
//!   no `instalar-en`, no `apagar`, no `negar`, no `observar`, no `correr`, no
//!   `ejecutar`, no `matar`.
//!
//! ## And it is not a second sandbox
//!
//! `CLAUDE.md`: do not invent a parallel one. Nothing here confines a process —
//! the confinement of modules is `thalyx-sandbox` and the enforcement is the
//! LSM, and neither is touched. What this is, is the smaller and older thing:
//! **an argument check in front of the verbs**, of exactly the kind
//! `thalyx_core`'s module API does for a grant. The reason it can be that small
//! is that the external agent never runs a program here. It types.
//!
//! ## Why the arguments are checked against the catalogue and not against prose
//!
//! `catalogue.rs` already says what each verb takes, in order. So the table
//! below says, per verb, which of those argument slots is a path — and a test
//! holds the two lists to each other. A verb whose shape changed in the
//! catalogue and not here stops being exposed rather than being exposed with the
//! wrong slot guarded, which is rule 9: the cautious answer, never the fast one.

use crate::catalogue::{self, Verb};
use crate::files::{Face, Where};
use std::path::{Component, Path, PathBuf};
use thalyx_core::Store;

/// What a slot of a verb's arguments is, as far as the boundary cares.
///
/// Four kinds and not two, because "check whether this is inside the workspace"
/// is the wrong question for three of them: a symbol is a name, a search is
/// text, and a glob is neither a name nor a path. Asking the containment
/// question of a symbol would refuse `Store` for not being a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The whole argument names a file or directory. Guarded.
    Path,
    /// A window flag (`limite=`, `cursor=`, `desde=`) or `en=<path>`. The path
    /// half of `en=` is guarded; the rest are numbers and opaque cursors.
    Option,
    /// A glob matched against names anywhere below. Never a path: `encontrar`
    /// walks a tree, so a `/` in one is a pattern that can match nothing, and a
    /// caller that meant a path wanted a different verb.
    Pattern,
    /// A name, a label or a piece of text. Bounded in length and nothing else.
    Text,
    /// One of a fixed set of words the verb itself knows.
    Word(&'static [&'static str]),
}

/// One verb this session may reach, and the shape it may be asked in.
pub struct Exposed {
    /// The catalogue's id. Never a word a person types — those are
    /// translations, and this is an interface.
    pub verb: &'static str,
    /// One entry per argument slot, in order.
    pub slots: &'static [Slot],
    /// The slot every argument past `slots` must match, if any are allowed —
    /// decided per call, from the arguments themselves.
    ///
    /// A function for the same reason `verbatim_from` is one, and it is the same
    /// verb that needs it. `editar … sustituir-lote` carries several exact
    /// strings **and** several file names past the action, interleaved, and
    /// which is which is only knowable after reading the counts inside the
    /// line. So this table cannot type-check them by position: checking them all
    /// as paths would refuse a rename of the text `../mod.rs`, and checking them
    /// all as text would drop the path rule for the file names.
    ///
    /// It is `Text` for that one subverb, and the path rule moves into
    /// `edit::substitute_batch`, which is the one place that knows which word is
    /// a file — written there, beside the loop that does it, saying what it is
    /// standing in for. Every other entry answers the same slot it always did.
    pub repeating: fn(&[String]) -> Option<Slot>,
    /// The slot from which arguments are put on the line **unquoted**, joined by
    /// single spaces — decided per call, from the arguments themselves.
    ///
    /// One verb needs it and the reason is the verb's own, written above
    /// `edit::act`: only the file name is split as words, and everything after
    /// it is taken from the line byte for byte — because the text going into a
    /// file may begin with spaces and a configuration line means something
    /// different with them. So `editar` is composed the way `editar` is read,
    /// rather than being made an exception at the far end.
    ///
    /// It costs nothing in safety here: the quoted half is the only half that
    /// names a path, and an unquoted argument past it cannot become a verb —
    /// `editar` has already consumed the line by the time it looks.
    ///
    /// A function and not a number, because one verb's answer depends on which
    /// of its subverbs was asked for. `editar … sustituir <viejo> <nuevo>` sends
    /// **two exact strings**, either of which may hold a space, and joining
    /// those unquoted is how `viejo con espacios` arrives as three arguments and
    /// substitutes something nobody asked for. That subverb is therefore quoted
    /// all through — which is lossless where the unquoted join is not — and
    /// `edit::act` reads its subverb as a word so the quoting reaches it intact.
    pub verbatim_from: fn(&[String]) -> Option<usize>,
}

/// Every argument single-quoted, which is what all but one verb want.
///
/// A named constant rather than a closure per entry, so that the table below
/// still reads as a table: the one verb that is different is visibly the one
/// that is different.
const QUOTED: fn(&[String]) -> Option<usize> = |_| None;

/// The four answers a verb gives about its arguments past the last named slot,
/// as named constants so the table below still reads as a table.
const NOTHING_MORE: fn(&[String]) -> Option<Slot> = |_| None;
const MORE_PATHS: fn(&[String]) -> Option<Slot> = |_| Some(Slot::Path);
const MORE_OPTIONS: fn(&[String]) -> Option<Slot> = |_| Some(Slot::Option);
const MORE_TEXT: fn(&[String]) -> Option<Slot> = |_| Some(Slot::Text);

/// The whole of what an external agent may ask for.
///
/// Read it as the claim it is: **these and only these**. Adding a line is a
/// decision about what a program on somebody's host may do to a machine, and it
/// belongs in the vault before it belongs here.
pub const EXPOSED: &[Exposed] = &[
    // ── looking, which is most of what an agent does ──────────────────────
    Exposed {
        verb: "state",
        slots: &[],
        repeating: NOTHING_MORE,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "describe",
        slots: &[Slot::Text],
        repeating: NOTHING_MORE,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "where",
        slots: &[],
        repeating: NOTHING_MORE,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "list",
        slots: &[Slot::Path],
        repeating: MORE_OPTIONS,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "read",
        slots: &[Slot::Path],
        repeating: NOTHING_MORE,
        verbatim_from: QUOTED,
    },
    // ── the index, which is the reason any of this is worth doing ─────────
    Exposed {
        verb: "index_build",
        slots: &[Slot::Path],
        repeating: NOTHING_MORE,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "symbol",
        slots: &[Slot::Text],
        repeating: MORE_OPTIONS,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "depends_on",
        slots: &[Slot::Path],
        repeating: MORE_OPTIONS,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "depended_on_by",
        slots: &[Slot::Path],
        repeating: MORE_OPTIONS,
        verbatim_from: QUOTED,
    },
    // ── searching the bytes, for the questions the index cannot answer ────
    Exposed {
        verb: "find",
        slots: &[Slot::Pattern],
        repeating: MORE_OPTIONS,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "grep",
        // The text is last and takes the rest of the line, so the options come
        // first — which is what `search::parse` does and why the slots are in
        // this order and not the catalogue's reading order.
        slots: &[],
        repeating: MORE_TEXT,
        verbatim_from: QUOTED,
    },
    // ── changing the workspace ────────────────────────────────────────────
    Exposed {
        verb: "edit",
        // Four: the file, what to do to it, which line or lines, and the text.
        // The action list is `edit::ACTIONS` itself rather than a copy — a
        // second list of the same words is a second thing to keep in step, and
        // this one would go wrong silently: an action spelled right in one place
        // and missing from the other is refused by the boundary with a message
        // naming the verb's own list, which is the most confusing refusal there
        // is.
        slots: &[
            Slot::Path,
            Slot::Word(crate::edit::ACTIONS),
            Slot::Text,
            Slot::Text,
        ],
        // Only `sustituir` uses it, and what it repeats is more files to make
        // the same substitution in. Guarded as paths, which is the strictest
        // slot there is, so naming a seventh file costs the same check the
        // first one got. For the line-addressed subverbs there is never a fifth
        // argument: `editar` takes the rest of the line as one text.
        // For `sustituir` this is more files to make the same substitution in,
        // guarded as paths — the strictest slot there is, so naming a seventh
        // file costs the same check the first one got. For `sustituir-lote` it
        // cannot be: see `Exposed::repeating`. For the line-addressed subverbs
        // there is never a fifth argument at all, because `editar` takes the
        // rest of the line as one text.
        repeating: |arguments| match arguments.get(1) {
            Some(action) if crate::edit::SUBSTITUTE_BATCH.contains(&action.as_str()) => {
                Some(Slot::Text)
            }
            _ => Some(Slot::Path),
        },
        verbatim_from: |arguments| match arguments.get(1) {
            // Two exact strings and a list of names. Every one of them is
            // lossless inside single quotes and none of them is content with
            // meaningful leading spaces, so there is nothing here for the
            // carve-out to protect.
            Some(action)
                if crate::edit::SUBSTITUTE.contains(&action.as_str())
                    || crate::edit::SUBSTITUTE_BATCH.contains(&action.as_str()) =>
            {
                None
            }
            _ => Some(1),
        },
    },
    Exposed {
        verb: "make_file",
        slots: &[],
        repeating: MORE_PATHS,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "make_directory",
        slots: &[],
        repeating: MORE_PATHS,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "copy",
        slots: &[Slot::Path, Slot::Path],
        repeating: NOTHING_MORE,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "move",
        slots: &[Slot::Path, Slot::Path],
        repeating: NOTHING_MORE,
        verbatim_from: QUOTED,
    },
    Exposed {
        verb: "remove",
        slots: &[],
        repeating: MORE_PATHS,
        verbatim_from: QUOTED,
    },
    // ── the boundary around a change, which is the whole pitch ────────────
    Exposed {
        verb: "attempt",
        slots: &[
            Slot::Word(&[
                "empezar",
                "start",
                "iniciar",
                "confirmar",
                "keep",
                "guardar",
                "abandonar",
                "abandon",
                "deshacer",
            ]),
            Slot::Text,
        ],
        // `MORE_TEXT` and not `NOTHING_MORE`, because abandoning in one call
        // says three things — which attempt, and the two halves of what it
        // costs — and the old shape had room for one. None of them is a path,
        // and the verb itself refuses every word it does not know, so what is
        // widened here is length and not authority.
        repeating: MORE_TEXT,
        verbatim_from: QUOTED,
    },
    // ── what a verb would do, without doing any of it ─────────────────────
    //
    // `ensayo` takes a verb and that verb's arguments, so its guarding is the
    // guarding of whatever it wraps. See `check` — it recurses, which is the
    // one place this table is not read positionally.
    Exposed {
        verb: "rehearse",
        slots: &[],
        repeating: MORE_TEXT,
        verbatim_from: QUOTED,
    },
];

/// The longest an argument may be.
///
/// A file this machine will print is 64 kB; an argument is a path, a name or a
/// line of replacement text, and something claiming to be one of those at
/// megabyte length is a caller that has lost track of what it is sending.
const LONGEST_ARGUMENT: usize = 64 * 1024;

/// Why a request never reached a verb.
///
/// Three fields and not one, punto **A2** of `Superficie-para-el-LLM.md`: an
/// error that only says what went wrong costs the caller a whole cycle of
/// guessing, and one that names the way out is documentation delivered at the
/// moment it is useful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub word: &'static str,
    pub remedy: &'static str,
    pub message: String,
}

impl Refusal {
    fn new(word: &'static str, remedy: &'static str, message: impl Into<String>) -> Self {
        Self {
            word,
            remedy,
            message: message.into(),
        }
    }
}

/// Look up what a verb id may be asked in, and the catalogue entry behind it.
pub fn exposed(verb: &str) -> Option<(&'static Exposed, &'static Verb)> {
    let shape = EXPOSED.iter().find(|exposed| exposed.verb == verb)?;
    // Never `expect`: a table entry naming a verb the catalogue lost is a
    // verb that is not exposed, which is the cautious answer. The test below
    // is what makes that never happen quietly.
    let verb = catalogue::VERBS.iter().find(|entry| entry.id == verb)?;
    Some((shape, verb))
}

/// A session held by something that is not on this machine.
///
/// It owns its own `here` rather than sharing the person's, which is not
/// tidiness: the human's session and the agent's run in one process, and a
/// shared location would mean an agent's `ls` moved the person's prompt.
pub struct ExternalAgentSession {
    workspace: PathBuf,
    /// The same path, with every symlink resolved. Kept rather than recomputed
    /// per request, because it is the thing every containment check is against
    /// and re-resolving it per call is a check that can change answer mid-session.
    real_workspace: PathBuf,
    here: Where,
}

impl ExternalAgentSession {
    /// Open a session confined to `workspace`.
    ///
    /// Refuses a workspace that is not there, and refuses `/` — the same refusal
    /// `intento` makes and for the same reason: a boundary that is the whole
    /// filesystem is not a boundary.
    pub fn open(workspace: &Path) -> Result<Self, Refusal> {
        let real = std::fs::canonicalize(workspace).map_err(|error| {
            Refusal::new(
                "absent",
                "import_a_project",
                format!("{} is not there: {error}", workspace.display()),
            )
        })?;
        if real.parent().is_none() {
            return Err(Refusal::new(
                "the_whole_system",
                "name_a_folder",
                "a workspace of `/` is not a workspace; name a directory under /home",
            ));
        }
        if !real.is_dir() {
            return Err(Refusal::new(
                "not_a_directory",
                "name_a_folder",
                format!("{} is not a directory", real.display()),
            ));
        }
        // The kernel's copy of the boundary, opened once and held for the life
        // of the session. Everything below is still checked as a name — that is
        // what produces a legible refusal — but the *opens* go through this,
        // which is what makes the boundary a boundary rather than a comparison
        // somebody can invalidate. See `crate::confine`.
        let confinement = crate::confine::Confinement::of(&real).map_err(|error| {
            Refusal::new(
                "unreadable",
                "import_a_project",
                format!("{} could not be held open: {error}", real.display()),
            )
        })?;

        let mut here = Where::start();
        here.confine(confinement);
        // Not `Where::start()`'s /home: the agent stands in its workspace, and
        // it is `here` that decides which subvolume `intento` is about and which
        // tree `encontrar` walks when nobody named one.
        here.go(&real.to_string_lossy()).map_err(|error| {
            Refusal::new(
                "unreadable",
                "import_a_project",
                format!("{} could not be entered: {error}", real.display()),
            )
        })?;
        Ok(Self {
            workspace: workspace.to_path_buf(),
            real_workspace: real,
            here,
        })
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// The verb ids this session accepts.
    ///
    /// Filtered through [`exposed`] rather than read straight off the table, so
    /// that a verb the catalogue no longer has is never advertised to a host
    /// that would then build a tool for it.
    pub fn verbs() -> Vec<String> {
        EXPOSED
            .iter()
            .filter(|shape| exposed(shape.verb).is_some())
            .map(|shape| shape.verb.to_string())
            .collect()
    }

    /// Run one request and give back the structured answer, verbatim.
    ///
    /// The answer is not composed here and never reworded: it is the object the
    /// verb itself produced, caught off this thread's structured face. That is
    /// the whole of `CLAUDE.md`'s "do not build a parallel API" — the external
    /// route and the person's route are the same code, and the only difference
    /// is where the line came from and what was allowed to be in it.
    pub fn answer(
        &mut self,
        store: &Store,
        verb: &str,
        arguments: &[String],
    ) -> Result<serde_json::Value, Refusal> {
        let (shape, entry) = exposed(verb).ok_or_else(|| {
            Refusal::new(
                "not_exposed",
                "ask_describe",
                format!(
                    "`{verb}` is not one of the verbs this session may reach. \
                     The list is in the hello message and in `describe`"
                ),
            )
        })?;

        check(shape, arguments, &self.here, &self.real_workspace)?;
        let line = compose(shape, entry, arguments);

        // Machine face, always. A structured caller asking for prose would be
        // asking for a second version of events, which is the thing the
        // two-faces decree exists to prevent.
        let mut face = Face::Machine;
        let store_here = &mut self.here;
        let (outcome, said) = crate::files::caught(|| {
            crate::session::dispatch_external(store, store_here, &mut face, &line)
        });

        if let Err(error) = outcome {
            return Err(Refusal::new(
                "failed",
                "read_the_message",
                error.to_string(),
            ));
        }

        // Exactly one object. The framing contract of `thalyx_files::machine`
        // is one line in, one object out, and a verb that broke it would hand a
        // caller half an answer with no way to tell.
        match said.len() {
            1 => serde_json::from_str(&said[0]).map_err(|error| {
                Refusal::new(
                    "unintelligible",
                    "cannot",
                    format!("`{verb}` answered with something that is not an object: {error}"),
                )
            }),
            0 => Err(Refusal::new(
                "silent",
                "cannot",
                format!(
                    "`{verb}` answered nothing, and silence is never an answer. \
                     This is a defect in Thalyx and not in the request"
                ),
            )),
            many => Err(Refusal::new(
                "several_answers",
                "cannot",
                format!(
                    "`{verb}` answered with {many} objects where the contract is one. \
                     This is a defect in Thalyx and not in the request"
                ),
            )),
        }
    }

    /// Whether the session is still standing where it was confined.
    ///
    /// Belt over braces, and it has a reason: the check above guards arguments,
    /// and this checks the *outcome*. A verb that moved the session out of the
    /// workspace by some route nobody thought of would be caught here on the
    /// next request rather than never.
    pub fn still_confined(&self) -> bool {
        std::fs::canonicalize(self.here.at())
            .map(|real| real.starts_with(&self.real_workspace))
            .unwrap_or(false)
    }
}

/// Turn a verb and its arguments into a line of the session's own vocabulary.
///
/// **Every argument is single-quoted.** POSIX single quotes are literal all
/// through, which is what makes this closed by construction rather than by
/// escaping: an argument cannot end the quote, cannot become a second word, and
/// cannot become a second verb. `words.rs` implements the same rule the shell
/// does, so what is composed here is split back into exactly the arguments that
/// went in.
///
/// The one thing quoting costs is globbing — `rm 'a*b'` names one oddly-named
/// file rather than several. That is the right default for a program naming a
/// file, and `encontrar` is unaffected: its pattern is rebuilt from the words'
/// text, which drops the quoting mask.
fn compose(shape: &Exposed, verb: &Verb, arguments: &[String]) -> String {
    format!("{}{}", verb.names[0], tail(shape, arguments))
}

/// The arguments of a line, quoted the way the verb they belong to reads them.
fn tail(shape: &Exposed, arguments: &[String]) -> String {
    // `ensayo` composes what it wraps exactly as that verb would have been
    // composed on its own — the same recursion `check` makes, and it has to be
    // the same or the two would disagree about which argument is which. What
    // must *not* be quoted is the wrapped verb's own name: `ensayo` splits its
    // first word off the raw line, so `ensayo 'rm' …` asks to rehearse a verb
    // called `'rm'`, which no machine has.
    if shape.verb == "rehearse"
        && let Some((named, rest)) = arguments.split_first()
        && let Some(entry) = catalogue::verb_named(named)
        && let Some((wrapped, wrapped_verb)) = exposed(entry.id)
    {
        return format!(" {}{}", wrapped_verb.names[0], tail(wrapped, rest));
    }

    // Asked once for the whole line rather than once per argument: the answer
    // is a property of the call, and a function consulted per argument could in
    // principle answer differently halfway through.
    let verbatim = (shape.verbatim_from)(arguments);
    let mut out = String::new();
    for (position, argument) in arguments.iter().enumerate() {
        out.push(' ');
        if verbatim.is_some_and(|from| position >= from) {
            out.push_str(argument);
            continue;
        }
        out.push('\'');
        // `'` cannot appear inside single quotes anywhere, so it is closed,
        // escaped and reopened — the same three characters every shell uses.
        out.push_str(&argument.replace('\'', r"'\''"));
        out.push('\'');
    }
    out
}

/// Check every argument against the slot it landed in.
fn check(
    shape: &Exposed,
    arguments: &[String],
    here: &Where,
    workspace: &Path,
) -> Result<(), Refusal> {
    if (shape.repeating)(arguments).is_none() && arguments.len() > shape.slots.len() {
        return Err(Refusal::new(
            "too_many_arguments",
            "ask_describe",
            format!(
                "`{}` takes at most {} argument(s) and was given {}",
                shape.verb,
                shape.slots.len(),
                arguments.len()
            ),
        ));
    }

    // `ensayo <verb> <arguments…>` is the one entry whose arguments are another
    // entry's. Guarding it as free text would be a hole with a name on it: the
    // wrapped verb is checked exactly as if it had been asked for directly, and
    // a wrapped verb that is not exposed is refused even though rehearsing it
    // would change nothing — a caller must not be able to learn what is outside
    // the workspace by rehearsing a read of it.
    if shape.verb == "rehearse" {
        let Some(inner) = arguments.first() else {
            return Err(Refusal::new(
                "nothing_asked",
                "name_a_verb",
                "`rehearse` needs the verb it should work out, and its arguments",
            ));
        };
        let Some(entry) = catalogue::verb_named(inner) else {
            return Err(Refusal::new(
                "unknown_verb",
                "ask_describe",
                format!("`{inner}` is not a verb of this machine"),
            ));
        };
        let Some((wrapped, _)) = exposed(entry.id) else {
            return Err(Refusal::new(
                "not_exposed",
                "ask_describe",
                format!("`{}` is not a verb this session may reach", entry.id),
            ));
        };
        return check(wrapped, &arguments[1..], here, workspace);
    }

    for (position, argument) in arguments.iter().enumerate() {
        if argument.len() > LONGEST_ARGUMENT {
            return Err(Refusal::new(
                "too_long",
                "send_less",
                format!(
                    "argument {position} is {} bytes and the limit is {LONGEST_ARGUMENT}",
                    argument.len()
                ),
            ));
        }
        if argument.contains('\0') {
            // A path is bytes to the kernel and a C string to almost everything
            // else, so a NUL in the middle is a name that means one thing here
            // and another there. Refused rather than trimmed.
            return Err(Refusal::new(
                "not_a_name",
                "cannot",
                format!("argument {position} holds a NUL byte"),
            ));
        }

        let slot = shape
            .slots
            .get(position)
            .copied()
            .or_else(|| (shape.repeating)(arguments))
            .ok_or_else(|| {
                Refusal::new(
                    "too_many_arguments",
                    "ask_describe",
                    format!(
                        "`{}` has nothing to do with argument {position}",
                        shape.verb
                    ),
                )
            })?;

        match slot {
            Slot::Path => {
                inside(here, workspace, argument)?;
            }
            Slot::Option => {
                let Some((name, value)) = argument.split_once('=') else {
                    return Err(Refusal::new(
                        "unknown_argument",
                        "ask_describe",
                        format!("`{argument}` is not one of `limite=`, `cursor=`, `desde=`, `en=`"),
                    ));
                };
                match name {
                    "en" | "in" => {
                        inside(here, workspace, value)?;
                    }
                    "limite" | "limit" => {
                        if value.parse::<usize>().is_err() {
                            return Err(Refusal::new(
                                "incomplete",
                                "give_a_number",
                                format!("`{argument}` is not a number"),
                            ));
                        }
                    }
                    // Opaque by design — a cursor is a token this machine
                    // produced, and the verb that made it is the thing that
                    // refuses a bad one, with `bad_cursor`.
                    "cursor" | "desde" => {}
                    _ => {
                        return Err(Refusal::new(
                            "unknown_argument",
                            "ask_describe",
                            format!("`{name}=` is not something this verb takes"),
                        ));
                    }
                }
            }
            Slot::Pattern => {
                if argument.contains('/') {
                    return Err(Refusal::new(
                        "not_a_pattern",
                        "name_a_name",
                        format!(
                            "`{argument}` has a `/` in it. This matches names anywhere \
                             below, so a pattern with a path in it matches nothing"
                        ),
                    ));
                }
            }
            Slot::Text => {}
            Slot::Word(allowed) => {
                if !allowed.contains(&argument.as_str()) {
                    return Err(Refusal::new(
                        "unknown_argument",
                        "ask_describe",
                        format!("`{argument}` is not one of {}", allowed.join(", ")),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Refuse anything that does not land inside the workspace.
///
/// Three questions, in this order, and all three have to be answered yes:
///
/// 1. **Is there a `..` in it?** Refused outright. The session stands at the
///    root of its workspace and every path it can want is below that, so `..` is
///    never needed — and the two ways of folding it disagree. `thalyx_files::resolve`
///    folds it lexically, before symlinks; the kernel folds it after. On a tree
///    with a symlink in it those give different files, and a check that used one
///    while the verb used the other would be a check that passes on the escape.
/// 2. **Where would the verb look?** `thalyx_files::resolve`, exactly the call
///    the verb makes, so what is checked is what will be opened.
/// 3. **What is actually there?** The deepest part of that path that exists is
///    canonicalised, which resolves every symlink the way the kernel will, and
///    the result must be under the workspace. A symlink out is caught here and
///    nowhere else.
///
/// The third is asked of the deepest *existing* prefix, because a caller
/// creating a file names something that is not there yet, and canonicalising a
/// path that does not exist answers `ENOENT` for every one of them.
fn inside(here: &Where, workspace: &Path, named: &str) -> Result<PathBuf, Refusal> {
    if named.is_empty() {
        return Err(Refusal::new(
            "nothing_asked",
            "name_a_path",
            "an empty path names nothing",
        ));
    }
    if Path::new(named)
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(Refusal::new(
            "outside_workspace",
            "name_a_path_inside",
            format!(
                "`{named}` has a `..` in it. This session stands at the root of its \
                 workspace and everything it may reach is below that, so `..` is refused \
                 rather than resolved"
            ),
        ));
    }

    let asked = thalyx_files::resolve(here.at(), named);

    // The cheap half first, so that an obviously-outside path is refused without
    // touching the disk. It is not the check — the next one is.
    if !asked.starts_with(workspace) {
        return Err(Refusal::new(
            "outside_workspace",
            "name_a_path_inside",
            format!(
                "{} is outside {}, which is this session's whole world",
                asked.display(),
                workspace.display()
            ),
        ));
    }

    // Now the real one, and it is the kernel's rather than this program's.
    //
    // Until 2026-08-28 this walked up to the deepest existing component,
    // `canonicalize`d it, compared the answer against the workspace and let the
    // verb open the original name all over again. Every step was right and the
    // sequence was still wrong, because between the comparison and the open
    // there is a moment: a test that swapped `src` for a link to another tree
    // while an agent read `src/main.rs` got 57 files from outside the workspace
    // in 4000 reads.
    //
    // So the answer comes from `openat2` with `RESOLVE_BENEATH` against a
    // descriptor for the workspace, held open since the session opened, and the
    // same anchor is what the verb opens — `crate::confine`. What this call is
    // for now is the *refusal*: the anchor knows a path is outside and does not
    // know how to say so with a remedy, and an agent handed "is not there"
    // about a file that is there would go looking for the wrong thing.
    // A path being created names something that is not there — often several
    // levels of it, `crear src/new/deep/file.rs` — so the question is asked of
    // the deepest **existing** prefix, exactly as the walk it replaces did. What
    // changed is who answers: `openat2` and not `canonicalize`.
    // `locate` and not `anchor`, because this is the one place that has to tell
    // *outside* from *not there*. A walk that could not would climb straight
    // past a symlink pointing out — `out` is refused, so ask about the
    // workspace root, which is fine — and answer that `out/passwd` is inside.
    let mut probe = asked.clone();
    loop {
        match here.locate(&probe) {
            Ok(()) => return Ok(asked),
            // Not there. Ask about its parent, which is what a path being made
            // requires to be inside and all it requires.
            Err(crate::confine::NotAnchored::Absent) => {}
            Err(crate::confine::NotAnchored::Outside) => break,
            Err(crate::confine::NotAnchored::Unreadable(error)) => {
                return Err(Refusal::new("unreadable", "cannot", error.to_string()));
            }
        }
        // The workspace root itself is asked about like any other prefix — it
        // is what `crear made.txt` needs to be inside, and stopping one step
        // short of it refused every file made at the top of a workspace.
        if !probe.pop() || !probe.starts_with(workspace) {
            break;
        }
    }

    Err(Refusal::new(
        "outside_workspace",
        "name_a_path_inside",
        format!(
            "{asked_display} does not resolve inside {workspace_display}. \
             A symlink pointing out of the workspace — and an absolute symlink of \
             any kind, including one that would have landed inside — is refused by \
             the kernel during resolution rather than followed and checked \
             afterwards, because a check that is not the open is not a check",
            asked_display = asked.display(),
            workspace_display = workspace.display()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// A workspace with a couple of things in it, on a real filesystem.
    fn workspace() -> (tempfile::TempDir, ExternalAgentSession) {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("src")).expect("src");
        std::fs::write(root.path().join("src/main.rs"), "fn main() {}\n").expect("write");
        let session = ExternalAgentSession::open(root.path()).expect("open");
        (root, session)
    }

    fn refuse(session: &ExternalAgentSession, named: &str) -> Refusal {
        inside(&session.here, &session.real_workspace, named)
            .expect_err(&format!("`{named}` should not be reachable"))
    }

    /// A component of the path becomes a link somewhere else, mid-request.
    ///
    /// This is the test that found the defect, and it found it because it is
    /// the only shape that can: every static check in this file passed on every
    /// one of those requests. `src` is a real directory when the boundary looks
    /// and a symlink to another tree when the verb opens, and before the
    /// session was anchored **57 of 4000 reads came back with a file from
    /// outside the workspace**.
    ///
    /// One-sided on purpose, which is rule 7. A run where the swapper never won
    /// the race proves nothing, so the assertion is on the direction ambient
    /// noise cannot reach: **zero** escapes, ever. The refusal count beside it
    /// is the control — a run where the swapper did nothing would also score
    /// zero escapes, and would score zero refusals with it.
    fn a_swapped_component_never_reaches_outside(verb: &str, arguments: &[&str]) -> (usize, usize) {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir_all(outside.path().join("src")).unwrap();
        std::fs::write(outside.path().join("src/main.rs"), "SECRET FROM OUTSIDE\n").unwrap();
        // The same words in a *name*, not only in a file's contents. Without
        // this the detector below is blind to `list`, whose answer carries
        // names and never bytes — and a leak that only `list` can produce would
        // pass a test that only looks for contents.
        std::fs::write(outside.path().join("src/SECRET FROM OUTSIDE"), "x\n").unwrap();
        let sentinel = outside.path().join("src/main.rs");

        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        let mut session = ExternalAgentSession::open(root.path()).expect("open");
        let store = Store::open(root.path().join(".store")).expect("store");

        let real_src = root.path().join("src");
        let away = outside.path().join("src");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let swapper = std::thread::spawn(move || {
            let spare = real_src.with_file_name("src.real");
            while !flag.load(Ordering::Relaxed) {
                let _ = std::fs::rename(&real_src, &spare);
                let _ = std::os::unix::fs::symlink(&away, &real_src);
                let _ = std::fs::remove_file(&real_src);
                let _ = std::fs::rename(&spare, &real_src);
            }
        });

        let arguments: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
        let mut refusals = 0;
        let mut answers = 0;
        let mut read_from_outside = 0;
        for _ in 0..4000 {
            match session.answer(&store, verb, &arguments) {
                Ok(value) => {
                    answers += 1;
                    if serde_json::to_string(&value)
                        .unwrap()
                        .contains("SECRET FROM OUTSIDE")
                    {
                        read_from_outside += 1;
                    }
                }
                Err(_) => refusals += 1,
            }
        }
        stop.store(true, Ordering::Relaxed);
        swapper.join().unwrap();

        assert_eq!(
            read_from_outside, 0,
            "`{verb}` read {read_from_outside} files from outside the workspace"
        );
        // The other half, and the one a read-only check would miss: whatever
        // the verb did, it did not do it to the file outside.
        assert_eq!(
            std::fs::read_to_string(&sentinel).unwrap(),
            "SECRET FROM OUTSIDE\n",
            "`{verb}` changed a file outside the workspace"
        );
        assert!(
            sentinel.exists(),
            "`{verb}` deleted a file outside the workspace"
        );
        (answers, refusals)
    }

    #[test]
    fn reading_through_a_component_being_swapped_never_leaves_the_workspace() {
        let (answers, refusals) =
            a_swapped_component_never_reaches_outside("read", &["src/main.rs"]);

        // ── the control, and why it is a skip and not a failure ──
        //
        // The claim this test makes — **zero** escapes — is one-sided and
        // holds however the threads were scheduled. The control is not: it says
        // the swapper won at least one race and lost at least one, which is the
        // only way to tell this apart from a run where the swapper thread never
        // got scheduled at all and the boundary was never asked anything hard.
        //
        // Run alone on this container the margin is enormous — around 2 600
        // answers to 1 400 refusals out of 4 000 — so a zero is not a thin race
        // lost, it is a thread that never ran. That happens: on 2026-08-29 this
        // failed once during a whole-workspace run that was sharing four cores
        // with a `git push`, and passed on ten runs before and after it.
        //
        // A control that fails when the machine is busy reports "the boundary
        // leaked" for a fact about the load average, which is the most
        // misleading message this file could produce. Rule 3: a test that could
        // not make its measurement **says so and skips**, and there is one
        // environment variable for this one requirement that turns the skip into
        // a failure. `dev/verify.sh` sets it, because his machine is the quiet
        // one and is where this claim has to hold.
        if refusals == 0 || answers == 0 {
            assert!(
                std::env::var("THALYX_REQUIRE_RACE_TESTS").is_err(),
                "THALYX_REQUIRE_RACE_TESTS is set and the swap never raced: \
                 {answers} answered, {refusals} refused"
            );
            eprintln!(
                "NOT PROVEN: the component swap never raced this run ({answers} answered, \
                 {refusals} refused), so nothing here was measured. The escape check above \
                 still held. Set THALYX_REQUIRE_RACE_TESTS=1 to make this a failure."
            );
        }
    }

    #[test]
    fn editing_through_a_component_being_swapped_never_leaves_the_workspace() {
        // Worse than a read if it leaks: this one writes. The edit itself is a
        // no-op replacement of line 1, so what is being measured is where the
        // write lands and not what it says.
        a_swapped_component_never_reaches_outside(
            "edit",
            &["src/main.rs", "replace", "1", "fn main() {}"],
        );
    }

    #[test]
    fn removing_through_a_component_being_swapped_never_leaves_the_workspace() {
        a_swapped_component_never_reaches_outside("remove", &["src/main.rs"]);
    }

    #[test]
    fn listing_through_a_component_being_swapped_never_leaves_the_workspace() {
        a_swapped_component_never_reaches_outside("list", &["src"]);
    }

    #[test]
    fn every_exposed_verb_is_a_verb_the_catalogue_has() {
        // The anti-drift binding. A verb renamed in the catalogue and not here
        // must not silently stop being exposed *and* must not be exposed under a
        // name nothing answers to — this is the test that turns either into a
        // failure at build time.
        for shape in EXPOSED {
            assert!(
                catalogue::VERBS.iter().any(|verb| verb.id == shape.verb),
                "`{}` is exposed to external agents and is not a verb of this machine",
                shape.verb
            );
        }
    }

    #[test]
    fn nothing_that_changes_the_machine_rather_than_the_workspace_is_exposed() {
        // Read as the claim it is. The catalogue already says which verbs change
        // something; this says which of those an agent on somebody's host may
        // reach, and pins the set so that adding one is a deliberate edit here
        // rather than a side effect of adding a line to the table above.
        let changing: BTreeSet<&str> = EXPOSED
            .iter()
            .filter_map(|shape| catalogue::VERBS.iter().find(|verb| verb.id == shape.verb))
            .filter(|verb| verb.changes)
            .map(|verb| verb.id)
            .collect();
        assert_eq!(
            changing,
            BTreeSet::from([
                "make_directory",
                "make_file",
                "copy",
                "move",
                "remove",
                "edit",
                // The one whose whole purpose is that the others can be undone.
                "attempt",
            ]),
            "an external agent may now change something new about this machine"
        );
        for forbidden in [
            "install",
            "install_onto",
            "power_off",
            "deny",
            "observe",
            "run",
            "execute",
            "stop",
            "rollback",
            "keyboard",
        ] {
            assert!(
                !EXPOSED.iter().any(|shape| shape.verb == forbidden),
                "`{forbidden}` is reachable from outside the machine"
            );
        }
    }

    #[test]
    fn an_absolute_path_outside_the_workspace_is_refused() {
        let (_root, session) = workspace();
        assert_eq!(refuse(&session, "/etc/passwd").word, "outside_workspace");
    }

    #[test]
    fn a_path_that_climbs_out_with_dot_dot_is_refused() {
        let (_root, session) = workspace();
        assert_eq!(
            refuse(&session, "../../etc/passwd").word,
            "outside_workspace"
        );
    }

    #[test]
    fn a_dot_dot_that_would_have_landed_inside_is_still_refused() {
        // Not an oversight. Folding `..` lexically and folding it through the
        // kernel give different files the moment a symlink is involved, and the
        // verb does the first while the kernel does the second. Refusing the
        // character is the only way both can be right.
        let (_root, session) = workspace();
        assert_eq!(
            refuse(&session, "src/../src/main.rs").word,
            "outside_workspace"
        );
    }

    #[test]
    fn a_symlink_pointing_out_of_the_workspace_is_refused() {
        // The check the lexical one cannot make, and the one an attacker would
        // reach for: nothing in the argument says `..` or `/etc`, and the path
        // is inside the workspace by every string comparison there is.
        let (root, session) = workspace();
        std::os::unix::fs::symlink("/etc", root.path().join("out")).expect("symlink");
        let refusal = refuse(&session, "out/passwd");
        assert_eq!(refusal.word, "outside_workspace");
        // The message names what the agent asked for and the workspace, and
        // deliberately not what the link pointed at. Since the boundary became
        // `openat2` there is nothing to name: the kernel refused during
        // resolution and never told anybody where it would have gone. Telling
        // an agent that its link led to `/etc` would be handing it a fact about
        // a filesystem it may not see, in the refusal for trying to see it.
        assert!(
            refusal.message.contains("out/passwd"),
            "{}",
            refusal.message
        );
        assert!(
            !refusal.message.contains("/etc/passwd"),
            "the refusal told the agent where its link pointed: {}",
            refusal.message
        );
    }

    #[test]
    fn a_symlink_that_stays_inside_the_workspace_is_allowed() {
        // The control. Without it a guard that refused everything would look
        // exactly like one that works — rule 4 of `CLAUDE.md`.
        let (root, session) = workspace();
        std::os::unix::fs::symlink("src", root.path().join("code")).expect("symlink");
        inside(&session.here, &session.real_workspace, "code/main.rs")
            .expect("a link that stays inside is inside");

        // And the narrowing that came with the kernel doing the resolving: the
        // same link, spelled absolutely, is refused. `RESOLVE_BENEATH` refuses
        // every absolute symlink, because deciding whether one lands inside
        // means resolving it in userspace first — which is the two-step check
        // this boundary exists to stop making. Asserted so that the loss is a
        // decision on the record rather than a surprise in somebody's project.
        std::os::unix::fs::symlink(root.path().join("src"), root.path().join("spelled_out"))
            .expect("symlink");
        assert_eq!(
            refuse(&session, "spelled_out/main.rs").word,
            "outside_workspace"
        );
    }

    #[test]
    fn a_file_that_does_not_exist_yet_is_reachable_inside_the_workspace() {
        // Creating a file names something that is not there, so a containment
        // check that canonicalised the whole path would refuse every `touch`.
        let (_root, session) = workspace();
        inside(
            &session.here,
            &session.real_workspace,
            "src/new/deep/file.rs",
        )
        .expect("a path that does not exist yet is still inside");
    }

    #[test]
    fn an_argument_cannot_smuggle_a_second_verb_through_the_line() {
        // The shape a shell would be vulnerable to. What comes back must be one
        // verb and one argument, whatever the argument looks like.
        let (shape, verb) = exposed("read").unwrap();
        let line = compose(shape, verb, &["a.txt' ; apagar ; '".to_string()]);
        let words = crate::words::words(&line).expect("the line splits");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].as_str(), "cat");
        // Byte for byte what went in, trailing quote included: the argument
        // came back as an argument.
        assert_eq!(words[1].as_str(), "a.txt' ; apagar ; '");
    }

    #[test]
    fn rehearsing_a_verb_composes_that_verbs_line_and_not_a_quoted_word() {
        // Found by running it. `ensayo` splits its first word off the raw line,
        // so a quoted verb name asks to rehearse a verb called `'rm'` — which
        // the machine correctly says it does not have, in an answer that reads
        // like the agent's mistake and is Thalyx's.
        let (shape, verb) = exposed("rehearse").unwrap();
        let line = compose(shape, verb, &["rm".into(), "a file.txt".into()]);
        assert_eq!(line, "ensayo rm 'a file.txt'");
    }

    #[test]
    fn rehearsing_an_edit_keeps_that_verbs_verbatim_tail() {
        // The two carve-outs meeting: `ensayo` does not quote the verb, `editar`
        // does not quote anything past the file, and a composition that forgot
        // either would put quotes into somebody's source file.
        let (shape, verb) = exposed("rehearse").unwrap();
        let line = compose(
            shape,
            verb,
            &[
                "editar".into(),
                "src/a.rs".into(),
                "cambiar".into(),
                "3".into(),
                "    let x = 1;".into(),
            ],
        );
        assert_eq!(line, "ensayo editar 'src/a.rs' cambiar 3     let x = 1;");
    }

    #[test]
    fn a_name_holding_a_single_quote_survives_being_composed() {
        let (shape, verb) = exposed("read").unwrap();
        let line = compose(shape, verb, &["it's here.txt".to_string()]);
        let words = crate::words::words(&line).expect("the line splits");
        assert_eq!(words[1].as_str(), "it's here.txt");
    }

    #[test]
    fn a_verb_that_is_not_on_the_list_is_refused_before_anything_runs() {
        let (_root, mut session) = workspace();
        let store = crate::files::Face::Machine;
        let _ = store;
        let root = tempfile::tempdir().expect("store");
        let store = Store::open(root.path()).expect("store");
        let refusal = session
            .answer(&store, "power_off", &[])
            .expect_err("`apagar` is not reachable from outside");
        assert_eq!(refusal.word, "not_exposed");
    }

    #[test]
    fn abandoning_in_one_call_reaches_the_verb_through_the_boundary() {
        // The plumbing, and the reason this test exists rather than being
        // assumed: the shape of `attempt` used to have room for one argument
        // after the subverb, so the three words that make an abandon one call
        // would have been refused here as `too_many_arguments` — by the
        // boundary, in the machine, with the tool and the verb both correct and
        // nothing in either of them able to say why.
        let (_root, session) = workspace();
        let shape = EXPOSED.iter().find(|e| e.verb == "attempt").unwrap();
        check(
            shape,
            &[
                "abandonar".into(),
                "snapshot=2026-08-29T11-04-02Z-rename".into(),
                "delete=0".into(),
                "revert=3".into(),
            ],
            &session.here,
            &session.real_workspace,
        )
        .expect("the one-call abandon is refused before the verb sees it");

        // And the control, which is what makes the line above mean something: a
        // widened shape must not have turned this verb into one that takes
        // paths. `attempt` names no file, ever.
        assert_eq!(refuse(&session, "/etc/passwd").word, "outside_workspace");
    }

    #[test]
    fn rehearsing_a_verb_that_is_not_exposed_is_refused_too() {
        // Rehearsing changes nothing, which is exactly why it would be the way
        // in: `ensayo rm /etc/passwd` answers what removing it would cost, and
        // that answer is a fact about a filesystem this session may not see.
        let (_root, session) = workspace();
        let shape = EXPOSED.iter().find(|e| e.verb == "rehearse").unwrap();
        let refusal = check(
            shape,
            &["instalar-en".into(), "/dev/sda".into()],
            &session.here,
            &session.real_workspace,
        )
        .expect_err("rehearsing an unexposed verb is not allowed");
        assert_eq!(refusal.word, "not_exposed");
    }

    #[test]
    fn rehearsing_an_exposed_verb_guards_that_verbs_paths() {
        let (_root, session) = workspace();
        let shape = EXPOSED.iter().find(|e| e.verb == "rehearse").unwrap();
        let refusal = check(
            shape,
            &["rm".into(), "/etc/passwd".into()],
            &session.here,
            &session.real_workspace,
        )
        .expect_err("a rehearsed path is still a path");
        assert_eq!(refusal.word, "outside_workspace");

        check(
            shape,
            &["rm".into(), "src/main.rs".into()],
            &session.here,
            &session.real_workspace,
        )
        .expect("rehearsing a removal inside the workspace is allowed");
    }

    #[test]
    fn a_window_flag_that_is_not_a_number_is_refused_before_the_verb_sees_it() {
        let (_root, session) = workspace();
        let shape = EXPOSED.iter().find(|e| e.verb == "list").unwrap();
        let refusal = check(
            shape,
            &[".".into(), "limite=dos".into()],
            &session.here,
            &session.real_workspace,
        )
        .expect_err("a limit that is not a number is not a limit");
        assert_eq!(refusal.word, "incomplete");
    }

    #[test]
    fn the_folder_half_of_an_en_flag_is_guarded_like_any_other_path() {
        // The hole that a positional check alone would leave: `encontrar` takes
        // a pattern, and the tree it walks arrives as part of a flag.
        let (_root, session) = workspace();
        let shape = EXPOSED.iter().find(|e| e.verb == "find").unwrap();
        let refusal = check(
            shape,
            &["*.rs".into(), "en=/etc".into()],
            &session.here,
            &session.real_workspace,
        )
        .expect_err("a search rooted outside the workspace is outside the workspace");
        assert_eq!(refusal.word, "outside_workspace");
    }

    #[test]
    fn an_argument_holding_a_nul_byte_is_refused() {
        let (_root, session) = workspace();
        let shape = EXPOSED.iter().find(|e| e.verb == "read").unwrap();
        let refusal = check(
            shape,
            &["src/main.rs\0/etc/passwd".into()],
            &session.here,
            &session.real_workspace,
        )
        .expect_err("a NUL in a name is not a name");
        assert_eq!(refusal.word, "not_a_name");
    }
}
