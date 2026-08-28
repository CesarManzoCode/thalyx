//! What this machine can be asked to do, in a form a program can read.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **A1**. It is the
//! first item of that catalogue and the cheapest, and it is the one no other
//! operating system can offer: on Linux `--help` is prose, it is written once per
//! tool by whoever wrote that tool, it is not consistent between any two of them,
//! and often it is not installed. Thalyx has **one** surface and knows all of it.
//!
//! What it buys is the first of the five costs — discovery. An agent that arrives
//! here does not need to have been trained on Thalyx and does not need somebody
//! to paste a list of verbs into its prompt. It asks.
//!
//! ## Why this is a table and not a document
//!
//! Before this file the list of verbs was written in **three** places: the match
//! arms of the session, the banner, and the completion list. Three copies of one
//! fact is three chances to drift, and this project has already paid for one of
//! them — new verbs were built and left out of the banner, and on a machine with
//! no shell a verb that is not in the banner does not exist for the person at it.
//!
//! So the names live here once. The completion list is **generated** from this
//! table, and two tests bind the other two copies to it:
//!
//! - every name here is typed at a real prompt and must be understood, which is
//!   what keeps the table from advertising a verb the session does not have;
//! - every name here must appear in the banner, which is what keeps a verb from
//!   existing without anybody being told.
//!
//! Neither is a check that the strings match. Both run the thing.

/// One verb, described the way a caller needs it rather than the way it is
/// implemented.
#[derive(Debug, Clone, Copy)]
pub struct Verb {
    /// The stable machine name. Never translated, never reworded: this is what a
    /// program matches on, and it is deliberately not the same string as any of
    /// `names`, so that renaming what a person types cannot break a program.
    pub id: &'static str,

    /// Every spelling that reaches this verb. The standard name comes first
    /// because it is the one the banner teaches and the one a model has seen a
    /// billion times.
    pub names: &'static [&'static str],

    /// What may follow it, in the order it is expected. Empty means the verb
    /// takes nothing.
    pub takes: &'static [&'static str],

    /// Words that may appear instead of an argument, both spellings.
    pub flags: &'static [&'static str],

    /// The `op` its structured answer carries, or `None` when this verb has no
    /// structured face yet.
    ///
    /// Said rather than omitted, because "this verb answers only in prose" is a
    /// fact a caller needs before it tries to parse the answer. An absent field
    /// and an unbuilt face are the same shape and different facts.
    pub answers: Option<&'static str>,

    /// Whether it can change the machine. What `ensayo` applies to, and what a
    /// caller must treat as consequential.
    pub changes: bool,

    /// The stable error words it can produce. A caller can write its handling
    /// before it has ever seen one fail.
    pub errors: &'static [&'static str],

    /// One line, in English, of what it is for.
    pub summary: &'static str,
}

/// The words that ask for a window, on every verb whose answer can be long.
///
/// `Superficie-para-el-LLM.md`, punto **B1**, and they are one constant rather
/// than three copies for the reason A1 exists at all: a caller that has to learn
/// a different spelling of "give me the next page" per verb pays the discovery
/// cost once per verb instead of once.
const WINDOW_FLAGS: &[&str] = &["limite=N", "limit=N", "cursor=…", "desde=…"];

/// The errors every verb that touches a path can produce.
const PATH_ERRORS: &[&str] = &["absent", "unreadable", "incomplete"];
/// The same, plus the refusal to write over something.
const WRITE_ERRORS: &[&str] = &["absent", "unreadable", "exists", "incomplete"];

/// Every verb the session has.
///
/// Ordered the way a person meets them: look around, move, change, then the
/// things that are about the machine rather than about files.
pub const VERBS: &[Verb] = &[
    Verb {
        id: "list",
        names: &["ls", "ver", "look"],
        takes: &["path"],
        flags: &[
            "-a",
            "-l",
            "todo",
            "detalles",
            "limite=N",
            "limit=N",
            "cursor=…",
            "desde=…",
        ],
        answers: Some("list"),
        changes: false,
        errors: &["absent", "unreadable", "incomplete", "bad_cursor"],
        summary: "What is in a directory, or about one thing that is not one.",
    },
    Verb {
        id: "read",
        names: &["cat", "leer", "read"],
        takes: &["path"],
        flags: &[],
        answers: Some("read"),
        changes: false,
        errors: &[
            "absent",
            "is_directory",
            "not_text",
            "unreadable",
            "incomplete",
        ],
        summary: "The contents of a file, refused rather than printed when it is not text.",
    },
    Verb {
        id: "go",
        names: &["cd", "ir", "go"],
        takes: &["path"],
        flags: &[],
        answers: Some("go"),
        changes: false,
        errors: PATH_ERRORS,
        summary: "Move somewhere. With nothing after it, home.",
    },
    Verb {
        id: "where",
        names: &["pwd", "donde", "dónde", "where"],
        takes: &[],
        flags: &[],
        answers: Some("where"),
        changes: false,
        errors: &[],
        summary: "Where this session is standing, exactly and never shortened.",
    },
    Verb {
        id: "make_directory",
        names: &["mkdir", "crear-carpeta", "nueva-carpeta"],
        takes: &["path..."],
        flags: &[],
        answers: Some("make_directory"),
        changes: true,
        errors: WRITE_ERRORS,
        summary: "Make a directory and every parent it needs.",
    },
    Verb {
        id: "make_file",
        names: &["touch", "crear"],
        takes: &["path..."],
        flags: &[],
        answers: Some("make_file"),
        changes: true,
        errors: WRITE_ERRORS,
        summary: "Make an empty file, refusing to flatten one that is there.",
    },
    Verb {
        id: "copy",
        names: &["cp", "copiar"],
        takes: &["from", "to"],
        flags: &[],
        answers: Some("copy"),
        changes: true,
        errors: WRITE_ERRORS,
        summary: "Copy a file or a whole directory. A link is copied as a link.",
    },
    Verb {
        id: "move",
        names: &["mv", "mover", "renombrar"],
        takes: &["from", "to"],
        flags: &[],
        answers: Some("move"),
        changes: true,
        errors: WRITE_ERRORS,
        summary: "Move or rename, falling back to copy-and-delete across filesystems.",
    },
    Verb {
        id: "remove",
        names: &["rm", "borrar", "eliminar"],
        takes: &["path..."],
        flags: &[],
        answers: Some("remove"),
        changes: true,
        errors: PATH_ERRORS,
        summary: "Delete. Inside /home this cannot be undone.",
    },
    Verb {
        id: "edit",
        names: &["editar", "edit"],
        // The file first, then what to do to it. A caller reading this table
        // learns that `editar <path>` alone is a legal line, which is the form
        // that opens a screen and the one a program must not use.
        takes: &["path", "ver|poner|cambiar|borrar", "line|line-line", "text"],
        flags: &[],
        answers: Some("edit"),
        changes: true,
        errors: &[
            "absent",
            "is_directory",
            "not_text",
            "too_large",
            "no_such_line",
            "backwards",
            "malformed_address",
            "unreadable",
            "unwritable",
            // The one a program is most likely to meet and the one it can do
            // something about: it asked for the screen, and there is none.
            "no_screen",
            "unknown_action",
        ],
        summary: "Change the text in a file, by line for a program or on a screen for a person.",
    },
    Verb {
        id: "structured",
        names: &["structured", "estructurado"],
        takes: &["on|off"],
        flags: &[],
        answers: Some("structured"),
        changes: false,
        errors: &["incomplete"],
        summary: "Answer in JSON objects a program can parse, or back in sentences.",
    },
    Verb {
        id: "rehearse",
        names: &["ensayo", "rehearse"],
        takes: &["verb", "arguments..."],
        flags: &[],
        answers: Some("rehearse"),
        changes: false,
        errors: &["incomplete", "unknown_verb"],
        summary: "What a verb would do, worked out without doing any of it.",
    },
    Verb {
        id: "describe",
        names: &["describe", "describir"],
        takes: &["verb"],
        flags: &[],
        answers: Some("describe"),
        changes: false,
        errors: &["unknown_verb"],
        summary: "Every verb this machine has, its arguments and the errors it can give.",
    },
    Verb {
        id: "index_build",
        names: &["indexar", "index"],
        takes: &["path"],
        flags: &[],
        answers: Some("index_build"),
        changes: false,
        errors: &["unreadable", "tree_too_large"],
        summary: "Read a tree and record what refers to what. Defaults to where you are. \
                  Hidden directories and build outputs are not read, and a tree too big to \
                  wait for is refused rather than started.",
    },
    Verb {
        id: "depends_on",
        names: &["depende", "depends"],
        takes: &["path"],
        flags: WINDOW_FLAGS,
        answers: Some("depends_on"),
        changes: false,
        errors: &["unreadable", "incomplete", "bad_cursor"],
        summary: "What this file refers to, from the index rather than by reading it.",
    },
    Verb {
        id: "depended_on_by",
        names: &["usan", "dependents"],
        takes: &["path"],
        flags: WINDOW_FLAGS,
        answers: Some("depended_on_by"),
        changes: false,
        errors: &["unreadable", "incomplete", "bad_cursor"],
        summary: "What refers to this file. No directory walk can answer this one.",
    },
    Verb {
        id: "symbol",
        names: &["buscar", "symbol"],
        takes: &["name"],
        flags: WINDOW_FLAGS,
        answers: Some("symbol"),
        changes: false,
        errors: &["unreadable", "incomplete", "bad_cursor"],
        summary: "Where a name is defined and every place it is used. Exact, and never a comment.",
    },
    // Point 6 of the usable terminal, and they sit here rather than with the
    // file verbs because the question above them is the one a caller has to get
    // right: `buscar` reads the index and knows what a symbol is, these two read
    // the tree and know what a byte is. A caller that picks the wrong one gets a
    // correct answer to a question it did not ask.
    Verb {
        id: "find",
        names: &["encontrar", "find"],
        takes: &["name-pattern", "en=folder"],
        flags: WINDOW_FLAGS,
        answers: Some("find"),
        changes: false,
        errors: &[
            "absent",
            "unreadable",
            "not_a_directory",
            "nothing_asked",
            "tree_too_large",
            "bad_cursor",
        ],
        summary: "Files whose name matches, anywhere below. `*` and `?`, the same as `rm`.",
    },
    Verb {
        id: "grep",
        names: &["contenido", "grep"],
        takes: &["text", "en=folder"],
        flags: WINDOW_FLAGS,
        answers: Some("grep"),
        changes: false,
        errors: &[
            "absent",
            "unreadable",
            "not_a_directory",
            "nothing_asked",
            "tree_too_large",
            "bad_cursor",
        ],
        summary: "Lines holding this text, literally. Flags go first; the rest of the line is the text.",
    },
    // Point 7. Three verbs over /proc, and `matar` is the second verb in this
    // machine whose ordinary use destroys something — the first was `editar`,
    // and unlike a file there is nothing to write back afterwards.
    Verb {
        id: "processes",
        names: &["procesos", "ps"],
        takes: &["name-pattern"],
        flags: WINDOW_FLAGS,
        answers: Some("processes"),
        changes: false,
        errors: &["unreadable", "bad_cursor"],
        summary: "What is running, with its number, its state and what it occupies.",
    },
    Verb {
        id: "memory",
        // `free` and not `memory`: `recuerdos` already answers to that word,
        // and it is the agent's memory rather than the machine's. `free` is
        // also what a person coming from Linux would type, which is the naming
        // rule Cesar set on 2026-08-09.
        names: &["memoria", "free"],
        takes: &[],
        flags: &[],
        answers: Some("memory"),
        changes: false,
        errors: &["unreadable"],
        summary: "How much memory there is, and how much something new could get.",
    },
    Verb {
        id: "stop",
        names: &["matar", "stop", "kill"],
        takes: &["pid", "forzar"],
        flags: &[],
        answers: Some("stop"),
        changes: true,
        errors: &[
            "no_such_process",
            "is_init",
            "is_self",
            "not_allowed",
            "not_a_number",
            "nothing_asked",
            "one_at_a_time",
            "unreadable",
        ],
        summary: "Ask one process to stop, or with `forzar` make it. Cannot be undone.",
    },
    Verb {
        id: "history",
        names: &["historia", "history"],
        takes: &[],
        flags: WINDOW_FLAGS,
        answers: Some("history"),
        changes: false,
        errors: &["unreadable", "bad_cursor", "unknown_argument"],
        summary: "What this machine did and what came of it. Not everything that happened to it.",
    },
    Verb {
        id: "attempt",
        names: &["intento", "attempt"],
        takes: &["empezar <label> | confirmar | abandonar [si]"],
        flags: &["si", "yes"],
        answers: Some("attempt"),
        // The one verb whose whole purpose is that what it wraps can be
        // undone — and it changes the machine itself: a snapshot is taken,
        // and abandoning replaces a whole subvolume.
        changes: true,
        errors: &[
            "not_a_subvolume",
            "the_whole_system",
            "already_open",
            "none_open",
            "snapshot_gone",
            "unreadable",
            "unknown_argument",
        ],
        summary: "Open something that can be taken back whole, then keep it or undo all of it.",
    },
    Verb {
        id: "changes",
        names: &["cambios", "changes"],
        takes: &[],
        flags: WINDOW_FLAGS,
        answers: Some("changes"),
        // It empties the kernel's queue, which nothing can put back. It changes
        // no file, and a caller deciding whether to be careful must still be
        // told: asking twice is not the same as asking once.
        changes: false,
        errors: &["not_loaded", "unreadable", "bad_cursor", "unknown_argument"],
        summary: "What the kernel saw change and who did it. Reading empties the queue; never paths.",
    },
    Verb {
        id: "clear",
        names: &["clear", "limpiar", "cls"],
        takes: &[],
        flags: &[],
        answers: Some("clear"),
        changes: false,
        errors: &[],
        summary: "Wipe the screen. Nothing on the machine changes.",
    },
    Verb {
        id: "screen",
        names: &["pantalla", "screen"],
        takes: &[],
        flags: &[],
        answers: Some("screen"),
        // It changes nothing about the machine — it changes which face is in
        // front of the person. Said carefully because `changes` is what `ensayo`
        // and every caller treat as consequential, and a verb marked consequential
        // for taking over a console would make that word mean less everywhere else.
        changes: false,
        errors: &["no_display", "not_a_terminal"],
        summary: "Put the one screen on this machine's display. Ctrl-C comes back here.",
    },
    Verb {
        id: "available",
        names: &["disponibles", "available", "repo"],
        takes: &[],
        flags: &["limite=", "cursor="],
        answers: Some("available"),
        changes: false,
        errors: &[],
        summary: "What is in this machine's repository and could be installed.",
    },
    Verb {
        id: "install",
        names: &["instalar", "install"],
        takes: &["module-id"],
        flags: &[],
        answers: Some("install"),
        changes: true,
        errors: &[],
        summary: "Install a signed module, showing what it asks for first.",
    },
    Verb {
        id: "modules",
        names: &["modulos", "módulos", "modules"],
        takes: &[],
        flags: &["limite=", "cursor="],
        answers: Some("modules"),
        changes: false,
        errors: &[],
        summary: "What is installed on this machine.",
    },
    Verb {
        id: "execute",
        names: &["ejecutar", "execute"],
        takes: &["path", "arguments"],
        flags: &["leyendo", "reading", "escribiendo", "writing"],
        answers: Some("execute"),
        changes: true,
        errors: &[
            "nothing_asked",
            "grant_without_path",
            "needs_a_human",
            "unclosed_quote",
            "trailing_backslash",
        ],
        summary: "Run a program nobody signed, confined, after a human says yes.",
    },
    Verb {
        id: "run",
        names: &["correr", "run"],
        takes: &["module-id"],
        flags: &["sin-confinar"],
        answers: Some("run"),
        changes: true,
        errors: &[],
        summary: "Run an installed module, confined to what it was granted.",
    },
    Verb {
        id: "permissions",
        names: &["permisos", "permissions"],
        takes: &[],
        flags: &[],
        answers: Some("permissions"),
        changes: false,
        errors: &[],
        summary: "What is granted right now, and to whom.",
    },
    // The two directions of the kernel guard, and two verbs rather than one
    // with an argument. `crate::guard` carries the reason; the short version is
    // that a typo in an argument must not be able to disarm the machine.
    Verb {
        id: "deny",
        names: &["negar", "deny"],
        takes: &[],
        flags: &[],
        answers: Some("deny"),
        changes: true,
        errors: &[
            "unreadable",
            "not_loaded",
            "did_not_take",
            "no_mode_flag",
            "kernel_refused",
        ],
        summary: "Make the kernel guard binding, so written policy is really enforced.",
    },
    Verb {
        id: "observe",
        names: &["observar", "observe"],
        takes: &[],
        flags: &[],
        answers: Some("observe"),
        changes: true,
        errors: &[
            "needs_a_human",
            "unreadable",
            "not_loaded",
            "did_not_take",
            "no_mode_flag",
            "kernel_refused",
        ],
        summary: "Stop the kernel guard from denying. Asks a human, and only a human.",
    },
    Verb {
        id: "rollback",
        names: &["revertir", "rollback"],
        takes: &[],
        flags: &[],
        answers: Some("rollback"),
        changes: true,
        errors: &[],
        summary: "Undo the last install, refusing when the disk no longer matches.",
    },
    Verb {
        id: "memory",
        names: &["recuerdos", "recordar", "memory", "recall"],
        takes: &[],
        flags: &[],
        answers: Some("memory"),
        changes: false,
        errors: &[],
        summary: "What this machine will still know after a restart.",
    },
    Verb {
        id: "state",
        names: &["estado", "status"],
        takes: &[],
        flags: &[],
        answers: Some("state"),
        changes: false,
        errors: &[],
        summary: "The machine re-read: store, enforcement, index, model.",
    },
    Verb {
        id: "kernel",
        names: &["nucleo", "núcleo", "kernel", "dmesg"],
        takes: &["todo|lento"],
        flags: &[],
        answers: Some("kernel"),
        changes: false,
        errors: &[],
        summary: "What the kernel has been saying. There is no dmesg in here.",
    },
    Verb {
        id: "disks",
        names: &["discos", "disks"],
        takes: &[],
        flags: &[],
        answers: Some("disks"),
        changes: false,
        errors: &[],
        summary: "The disks this machine can see, and which one it booted from.",
    },
    Verb {
        id: "keyboard",
        names: &["teclado", "keyboard"],
        takes: &["layout"],
        flags: &[],
        answers: Some("keyboard"),
        // It changes the machine — and unlike every other verb that does, what
        // it changes is how the machine can be told to change it back. That is
        // why `teclado ingles` puts back the kernel's own table rather than
        // something close to it.
        changes: true,
        errors: &["no_console", "no_such_layout", "not_loaded", "left_alone"],
        summary: "Which keyboard layout the kernel holds, and which one to put on it.",
    },
    Verb {
        id: "network",
        names: &["red", "network"],
        takes: &[],
        flags: &[],
        answers: Some("network"),
        changes: false,
        errors: &[],
        summary: "The network hardware this machine has. Thalyx cannot use it yet.",
    },
    Verb {
        id: "install_onto",
        names: &["instalar-en", "install-onto"],
        takes: &["disk"],
        flags: &[],
        answers: Some("install_onto"),
        changes: true,
        errors: &[],
        summary: "Put this machine onto a disk. Everything on that disk is lost.",
    },
    Verb {
        id: "leave",
        names: &["salir", "exit", "quit"],
        takes: &[],
        flags: &[],
        answers: Some("leave"),
        changes: false,
        errors: &[],
        summary: "Leave the session. On the machine itself there is nowhere to go.",
    },
    Verb {
        id: "power_off",
        names: &["apagar", "poweroff"],
        takes: &[],
        flags: &[],
        answers: Some("power_off"),
        changes: true,
        errors: &[],
        summary: "Turn the machine off.",
    },
];

/// The verb a typed line names, if any.
///
/// Matches the first word only. Everything after it is arguments, which is the
/// rule the completion list is built on too — offering a file name where a verb
/// belongs would put a path at the start of a line, where nothing can run it.
pub fn verb_named(word: &str) -> Option<&'static Verb> {
    VERBS.iter().find(|verb| verb.names.contains(&word.trim()))
}

/// The verb a stable machine name identifies.
///
/// The other direction from [`verb_named`], and it exists because the agent
/// speaks in ids: `thalyx_agent::ProposedOperation::name` is `make_directory`,
/// and what the session's dispatch matches is `mkdir`. One table answers both,
/// so a verb the model can propose and a verb a person can type cannot come
/// apart.
pub fn verb_with_id(id: &str) -> Option<&'static Verb> {
    VERBS.iter().find(|verb| verb.id == id)
}

/// Every spelling of every verb, for tab completion.
///
/// Generated rather than listed, which removes the second of the three copies.
/// The third — the match arms of the session — is bound by a test that types
/// each of these at a real prompt.
pub fn every_name() -> Vec<&'static str> {
    VERBS
        .iter()
        .flat_map(|verb| verb.names.iter().copied())
        .collect()
}

// ─────────────────────────────────────────────────── answering `describe`

use crate::files::Face;
use serde_json::json;

/// `describe [verbo]` — the machine reading itself out loud.
pub fn describe(face: Face, rest: &str) {
    let asked = rest.trim();

    let chosen: Vec<&Verb> = if asked.is_empty() {
        VERBS.iter().collect()
    } else {
        match verb_named(asked) {
            Some(verb) => vec![verb],
            None => {
                let why = format!("`{asked}` is not a verb of this machine");
                if face == Face::Machine {
                    println!(
                        "{}",
                        thalyx_files::machine::declined("describe", "unknown_verb", &why)
                    );
                } else {
                    println!("\n  {why}. `describe` alone lists them all.\n");
                }
                return;
            }
        }
    };

    if face == Face::Machine {
        let verbs: Vec<serde_json::Value> = chosen.iter().map(|verb| as_object(verb)).collect();
        println!(
            "{}",
            thalyx_files::machine::answer(
                "describe",
                vec![("count", json!(verbs.len())), ("verbs", json!(verbs))],
            )
        );
        return;
    }

    println!();
    for verb in chosen {
        // The names on one line and the summary under it, because a person
        // scanning this is looking for a word and then for what it does.
        println!("  {}", verb.names.join(", "));
        println!("      {}", verb.summary);
    }
    println!();
}

fn as_object(verb: &Verb) -> serde_json::Value {
    json!({
        "id": verb.id,
        "names": verb.names,
        "takes": verb.takes,
        "flags": verb.flags,
        // `null` rather than absent, because "this verb only speaks prose" is
        // something a caller has to know *before* it tries to parse the answer.
        "answers": verb.answers,
        "changes": verb.changes,
        "errors": verb.errors,
        "summary": verb.summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The two vocabularies are one vocabulary, checked from the only crate
    /// that can see both.
    ///
    /// `thalyx-agent` declares what a model may propose and `thalyx-cli`
    /// declares what the session can be asked; `thalyx-cli` depends on
    /// `thalyx-agent` and not the other way round, so the agent cannot read
    /// this table and the binding has to live here.
    ///
    /// Both directions, and each is a different failure. An op with no
    /// operation is a verb the model was given no way to ask for — the same
    /// silence `answers: None` used to produce, one layer up. An operation
    /// with no op is a word the grammar spends tokens on that reaches no verb,
    /// so every inference that picks it is refused for no visible reason and
    /// the tier gets blamed.
    #[test]
    fn the_model_can_propose_exactly_the_verbs_the_session_has() {
        use thalyx_agent::ProposedOperation;

        let ops: BTreeSet<&str> = VERBS.iter().filter_map(|verb| verb.answers).collect();
        let proposable: BTreeSet<&str> = ProposedOperation::ALL
            .iter()
            // Abstention is not a verb and never will be: it is the model
            // saying it found nothing, which is an answer about the request
            // rather than a thing to do.
            .filter(|op| **op != ProposedOperation::Nothing)
            .map(|op| op.name())
            .collect();

        let unaskable: Vec<&&str> = ops.difference(&proposable).collect();
        assert_eq!(
            unaskable,
            Vec::<&&str>::new(),
            "the session has verbs the model has no way to ask for"
        );

        let unreachable: Vec<&&str> = proposable.difference(&ops).collect();
        assert_eq!(
            unreachable,
            Vec::<&&str>::new(),
            "the grammar can emit words that reach no verb"
        );
    }

    /// Which operations get the tight target rule, checked against the
    /// catalogue's own account of what each verb is given.
    #[test]
    fn the_grammar_asks_for_a_module_id_exactly_where_a_verb_wants_one() {
        use thalyx_agent::ProposedOperation;

        for operation in ProposedOperation::ALL {
            if operation == ProposedOperation::Nothing {
                continue;
            }
            let verb = VERBS
                .iter()
                .find(|verb| verb.answers == Some(operation.name()))
                .unwrap_or_else(|| panic!("{} reaches no verb", operation.name()));

            assert_eq!(
                operation.takes_module_id(),
                verb.takes.contains(&"module-id"),
                "the grammar and the catalogue disagree about whether `{}` is \
                 given a module id, so the model is either handed a rule that \
                 refuses what the verb wants or one that spends tokens on \
                 anything",
                verb.id
            );
        }
    }

    #[test]
    fn no_two_verbs_answer_to_the_same_word() {
        let mut seen = BTreeSet::new();
        for verb in VERBS {
            for name in verb.names {
                assert!(
                    seen.insert(*name),
                    "`{name}` reaches two verbs, so the catalogue cannot say which"
                );
            }
        }
    }

    #[test]
    fn every_verb_has_an_id_nobody_types() {
        for verb in VERBS {
            // The id is what a program matches on and the names are what a
            // person types. Letting them be the same string means renaming the
            // human word silently renames the machine one.
            assert!(!verb.id.is_empty());
            assert!(
                !verb.id.contains(' '),
                "`{}` is a sentence, not an identifier",
                verb.id
            );
        }
    }

    #[test]
    fn a_verb_that_can_change_the_machine_says_so() {
        // Read as a claim: these and only these are consequential. A caller
        // reads `changes` to decide whether to rehearse first, so a verb that
        // lied here would be rehearsed by nobody.
        let changing: BTreeSet<&str> = VERBS
            .iter()
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
                "deny",
                "observe",
                // The one whose change cannot be taken back at all. Every
                // other entry on this list either has a rollback or writes
                // something that can be written again.
                "stop",
                "install",
                "run",
                // The only entry here whose change is somebody else's code
                // running. It is on this list for the same reason `stop` is:
                // what it does cannot be taken back once it has happened.
                "execute",
                "rollback",
                "install_onto",
                // The one whose change is to the instrument a person would use
                // to change it back: a layout loaded wrong is a machine that
                // looks healthy and types the wrong letters, with no second
                // terminal on the image to fix it from.
                "keyboard",
                "power_off",
                // It changes the machine even though its purpose is that what
                // it wraps can be undone: opening one takes a snapshot, and
                // abandoning one replaces a whole subvolume.
                "attempt",
            ])
        );
    }

    #[test]
    fn a_verb_with_a_structured_face_names_the_op_it_answers_with() {
        for verb in VERBS.iter().filter(|verb| verb.answers.is_some()) {
            let op = verb.answers.unwrap();
            assert!(!op.is_empty(), "{} answers with an empty op", verb.id);
        }
    }

    #[test]
    fn a_verb_that_has_no_structured_face_says_none_rather_than_pretending() {
        // The point of `answers: None` is that "this one only speaks prose" is a
        // fact a caller needs *before* it tries to parse. Rule 10 on the wire: a
        // failure to have a face is not a face that failed.
        //
        // The whole set is pinned rather than one example, because the way this
        // goes wrong is silent and it already did: `red` was built with both
        // faces on 2026-08-23 and left declared `None` here, so `describe` told
        // every program that the only listing of network hardware spoke prose —
        // and a program that believes that never calls the verb at all. A single
        // `contains` could not see it. Growing a face means editing this list,
        // and that edit is the moment to check the claim is now true.
        //
        // **It is empty, and that is the claim now.** Every verb this machine has
        // answers by structure, including the three that used to have nothing to
        // say — `limpiar`, `salir` and `apagar` — because silence is never an
        // answer and those were the three places it was still being given. A verb
        // added without a face has to add itself here, in a test that says so.
        let prose_only: Vec<&str> = VERBS
            .iter()
            .filter(|verb| verb.answers.is_none())
            .map(|verb| verb.id)
            .collect();
        assert_eq!(
            prose_only,
            Vec::<&str>::new(),
            "a verb was added with no structured face; the decree is that it is born with both"
        );
    }

    #[test]
    fn a_word_finds_its_verb_by_any_of_its_spellings() {
        assert_eq!(verb_named("ls").unwrap().id, "list");
        assert_eq!(verb_named("ver").unwrap().id, "list");
        assert_eq!(verb_named("look").unwrap().id, "list");
        assert!(verb_named("no-such-verb").is_none());
    }

    #[test]
    fn every_name_is_offered_for_completion() {
        let offered = every_name();
        for verb in VERBS {
            for name in verb.names {
                assert!(
                    offered.contains(name),
                    "`{name}` exists and tab would never find it"
                );
            }
        }
    }
}
