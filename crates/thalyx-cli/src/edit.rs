//! `editar` — the two faces of changing text in a file.
//!
//! The engine is `thalyx-edit`. Nothing here decides what an edit *is*; this file
//! decides only how the two faces ask for one and how each is answered.
//!
//! ## Why one verb and not two
//!
//! A screen and a line address are the same act asked for two ways, so they are
//! one verb with the shape every other subverb in this system already has —
//! `intento empezar`, `intento abandonar`. `editar notas.txt` with nothing after
//! it means *give me the screen*, which is what a person wants and the shortest
//! thing to type; `editar notas.txt cambiar 12 …` is the same file changed by
//! something that cannot see a screen.
//!
//! The alternative — a separate verb for the machine — was rejected because it
//! is exactly the arrangement `Principio-Doble-Ruta.md` forbids. Two verbs drift:
//! one grows a feature, and the other route quietly loses capability without
//! anybody deciding it should.
//!
//! ## What a person can lose here, and what stops it
//!
//! This is the first verb whose ordinary use destroys the previous contents of a
//! file. Three things hold, and all three are in the engine rather than here, so
//! neither face can be the one that forgets:
//!
//! - the save is a write-then-rename, so a machine that loses power mid-save has
//!   either the old file or the new one and never half of each;
//! - anything that is not text, or is over the ceiling, is refused rather than
//!   opened and written back mangled;
//! - taking back more than the undo stack holds is `intento`, and every
//!   structured answer says so in its `undo` field.
//!
//! ## The screen, and the keys it does not use
//!
//! Raw mode in `thalyx-syscall` deliberately leaves `ISIG` and `IXON` on: Ctrl-C
//! must keep working on a machine whose only terminal this is. The consequence
//! is that the line discipline eats Ctrl-C, Ctrl-Z, Ctrl-S and Ctrl-Q before
//! Thalyx sees a byte — **so the editor saves with Ctrl-O and not Ctrl-S**, and a
//! test in `thalyx-edit::screen` fails if anybody ever binds one of the eaten
//! keys. A key that does nothing is how a person concludes an editor cannot save.
//!
//! And there is no alternate screen. Drawing in place scribbles over what was on
//! the terminal, which is the smaller of the two costs: `ISIG` means Ctrl-C
//! terminates the session outright, and a program that had switched to an
//! alternate screen would leave the person looking at a blank one with no way
//! back. On the image there is no scrollback worth the name to protect anyway.

use crate::files::{Face, Where};
use std::io::Write;
use std::path::{Path, PathBuf};
use thalyx_edit::screen::{Editing, Reaction, Viewport};
use thalyx_edit::{EditError, Edited, Text, machine};

/// What `editar` leaves for the surface it was typed on to do.
///
/// **This exists because the editor is the one verb whose answer is a surface
/// rather than words.** Every other verb finishes by printing; this one finishes
/// by taking over the display until the person leaves it, and *which* display
/// that is is not something the verb can know. Cesar found it by running the
/// image: `crear prueba.txt` worked and `editar prueba.txt` answered «there is
/// no terminal here to draw an editor on» — on the screen the machine boots
/// into, which is nothing but display.
///
/// The reason is not a missing check, it is where the check was. The editor here
/// writes ANSI to descriptor 1 and reads keys from descriptor 0, and under the
/// screen descriptor 1 is `thalyx-capture`'s buffer and descriptor 0 is
/// `/dev/null`. Faking a terminal there would have drawn the escape sequences
/// into the conversation as text.
///
/// So the verb answers *open one*, and the surface — the text session or the
/// screen — opens the one it has. That is the same shape [`Flow::Emptied`] took
/// for `limpiar` and for the same reason: the meaning of the verb is a property
/// of the surface, so the surface is what finishes it.
///
/// [`Flow::Emptied`]: crate::session::Flow::Emptied
pub enum Opens {
    /// Nothing more. The verb said everything it had to say.
    Nothing,
    /// A screen editor on this file, which only a surface can put up.
    Editor(PathBuf),
}

/// The subverbs, in both spellings, and the order they are offered in.
pub const ACTIONS: &[&str] = &[
    "ver",
    "show",
    "poner",
    "insert",
    "cambiar",
    "replace",
    "borrar",
    "delete",
    // The one that is not addressed by line. It is last because it is the
    // newest and not because it is the least used — for a mechanical change it
    // is the one to reach for, and `describe` says so.
    "sustituir",
    "substitute",
    // Several exact substitutions, in order, in one call. Last because it is
    // the newest; it is the one a mechanical rename of a symbol actually wants,
    // and `describe` says so.
    "sustituir-lote",
    "substitute_batch",
];

/// The two spellings of the subverb that addresses text instead of a line.
///
/// Its own constant because three places have to agree about it — the parser
/// here, the external boundary's decision about how to put the arguments on the
/// line, and the tests — and three copies of two strings is how one of them
/// keeps the old spelling after a rename.
pub const SUBSTITUTE: &[&str] = &["sustituir", "substitute"];

/// The two spellings of the subverb that carries several substitutions at once.
///
/// Its own constant beside [`SUBSTITUTE`] and for the same reason: the parser
/// here, the external boundary's decision about how to check the arguments past
/// the action, and the tests all have to agree about these two strings.
///
/// The English spelling has an underscore where the Spanish has a hyphen, which
/// is not an oversight. `crear-carpeta` is how this session spells a two-word
/// name in Spanish, and `substitute_batch` is what `thalyx_edit`'s `action`
/// enumeration sends — one string, passed through, rather than a mapping
/// between two spellings that somebody has to keep in step.
pub const SUBSTITUTE_BATCH: &[&str] = &["sustituir-lote", "substitute_batch"];

/// How many lines one `ver` answers with when nobody said.
///
/// `Superficie-para-el-LLM.md`, punto **B1**: no answer may eat a context
/// window, and a file of four thousand lines returned whole is a caller that
/// forgot what it was doing. The count and `more` are in every answer, so asking
/// for the rest costs one more call and never a guess.
const PAGE: usize = 200;

pub fn run(here: &Where, rest: &str, face: Face) -> std::io::Result<Opens> {
    act(here, rest, face, false)
}

/// `ensayo editar <archivo> poner|cambiar|borrar …` — D1 for the last verb
/// that changes the machine and could not be rehearsed.
///
/// Cheap for one reason: [`change`] already applies to a `Text` in memory and
/// then saves it, so a rehearsal is that same path with the save left out. It
/// is the run's own arithmetic and not a second copy of it, which is the same
/// property `foresee_run` has and for the same reason.
pub fn foresee(here: &Where, rest: &str, face: Face) -> std::io::Result<()> {
    // A rehearsal never opens anything — `""` is `nothing_to_rehearse` below —
    // so there is nothing here for a surface to do.
    act(here, rest, face, true).map(|_| ())
}

fn act(here: &Where, rest: &str, face: Face, rehearsing: bool) -> std::io::Result<Opens> {
    // Only the name is split as words. Everything after it is taken from the
    // line byte for byte, because the third part is text going into a file and a
    // configuration line that starts with four spaces means something with them
    // and something else without. `words.rs` calls this the one carve-out.
    let named = match crate::words::first(rest) {
        Ok(Some(named)) => named,
        Ok(None) => return which_file(face).map(|()| Opens::Nothing),
        Err(why) => {
            if face.is_machine() {
                face.say(thalyx_files::machine::refused(
                    if rehearsing { "rehearse" } else { "edit" },
                    why.word(),
                    why.remedy(),
                    &why.to_string(),
                ));
            } else {
                println!("\n  {why}\n");
            }
            return Ok(Opens::Nothing);
        }
    };
    let (named, after) = named;
    if named.is_empty() {
        return which_file(face).map(|()| Opens::Nothing);
    }

    // The subverb is read as a *word* and the rest of the line is not.
    //
    // It used to be a raw split on the first space, which is nearly the same
    // thing and is wrong in exactly one place: the external boundary composes
    // this line, and for `sustituir` it has to quote what follows — two exact
    // strings, either of which may hold a space. Quoting them means the subverb
    // arrives quoted too, and a raw split would look for a subverb called
    // `'sustituir'`, which no machine has. Reading it with the same scanner
    // that read the file name costs nothing and makes both spellings work.
    //
    // What must **not** become words is everything after it: the third part is
    // content going into a file, and `words.rs` calls that the one carve-out.
    let taken = match crate::words::first(after) {
        Ok(taken) => taken,
        Err(why) => {
            if face.is_machine() {
                face.say(thalyx_files::machine::refused(
                    op_of(rehearsing),
                    why.word(),
                    why.remedy(),
                    &why.to_string(),
                ));
            } else {
                println!("\n  {why}\n");
            }
            return Ok(Opens::Nothing);
        }
    };
    let action = taken.as_ref().map(|(word, _)| word.as_str()).unwrap_or("");
    let argument = taken.as_ref().map(|(_, rest)| rest.trim()).unwrap_or("");

    // Answered before the file is opened, because this subverb has no *one*
    // file: the name before it is the first of a list, and opening it here
    // would mean opening it twice and refusing the second as a repeat.
    if SUBSTITUTE.contains(&action) {
        return substitute(here, named.as_str(), argument, face, rehearsing)
            .map(|()| Opens::Nothing);
    }
    if SUBSTITUTE_BATCH.contains(&action) {
        return substitute_batch(here, named.as_str(), argument, face, rehearsing)
            .map(|()| Opens::Nothing);
    }

    let path = thalyx_files::resolve(here.at(), named.as_str());

    // Two anchors and both are needed. The first proves the *file* resolves
    // inside the workspace — `RESOLVE_BENEATH` refuses a link that leaves, and
    // that is the check `Text::open`'s deliberate symlink-following would
    // otherwise walk straight past. The second is what gets opened: the file's
    // *parent*, pinned, with the name appended, because `Text::save` stages a
    // temporary beside the file and renames it — and a descriptor path with no
    // usable parent has nowhere to stage.
    //
    // For the person's session both are the path itself and this costs a clone.
    let opened = match here
        .anchor(&path)
        .and_then(|_| here.anchor_parent(&path))
        .map_err(|error| {
            thalyx_edit::EditError::Absent(
                error
                    .path()
                    .unwrap_or(std::path::Path::new(""))
                    .to_path_buf(),
            )
        }) {
        Ok(anchored) => anchored,
        Err(error) => return refuse(op_of(rehearsing), &error, face).map(|()| Opens::Nothing),
    };

    let mut text = match Text::open_anchored(opened.path(), &path) {
        Ok(text) => text,
        Err(error) => return refuse(op_of(rehearsing), &error, face).map(|()| Opens::Nothing),
    };

    // No subverb is the person's case, and it is the one that needs a surface.
    // Answered **before** the match and by handing the file back rather than by
    // opening anything, because which surface this was typed on is the one thing
    // this verb cannot see — see [`Opens`]. The file is opened first all the
    // same: a file that is not text or is over the ceiling is refused here,
    // where a refusal is words, rather than by a display that has already gone
    // blank to show it.
    if action.is_empty() && !rehearsing {
        return Ok(Opens::Editor(text.path().to_path_buf()));
    }

    match action {
        // Both of these change nothing, so a rehearsal of them has nothing to
        // answer — and answering anyway would be a second, worse `ver`.
        "" => nothing_to_rehearse("opening the screen", face),
        "ver" | "show" if rehearsing => nothing_to_rehearse("`ver`", face),
        "ver" | "show" => show(&text, argument, face),
        "poner" | "insert" => {
            let (at, body) = split_argument(argument);
            let body = unescape(body);
            match thalyx_edit::span(at) {
                Ok(span) => change(&mut text, |t| t.insert(span.from, &body), face, rehearsing),
                Err(error) => refuse(op_of(rehearsing), &error, face),
            }
        }
        "cambiar" | "replace" => {
            let (at, body) = split_argument(argument);
            let body = unescape(body);
            match thalyx_edit::span(at) {
                Ok(span) => change(&mut text, |t| t.replace(span, &body), face, rehearsing),
                Err(error) => refuse(op_of(rehearsing), &error, face),
            }
        }
        "borrar" | "delete" => match thalyx_edit::span(argument) {
            Ok(span) => change(&mut text, |t| t.delete(span), face, rehearsing),
            Err(error) => refuse(op_of(rehearsing), &error, face),
        },
        other => {
            if face.is_machine() {
                face.say(machine::unknown(other, ACTIONS));
            } else {
                println!("\n  `{other}` is not something `editar` does.");
                println!("  Those are: {}.\n", ACTIONS.join(", "));
            }
            Ok(())
        }
    }
    .map(|()| Opens::Nothing)
}

/// Split `12 texto` into the address and everything after it.
///
/// `splitn(2)` and not `split_whitespace`, because the text being put in may
/// begin with spaces and they are part of it — indentation is the whole reason
/// somebody edits a configuration file by line.
fn split_argument(argument: &str) -> (&str, &str) {
    match argument.split_once(char::is_whitespace) {
        Some((at, body)) => (at, body),
        None => (argument, ""),
    }
}

/// Read `\n` in what was typed as a real line break.
///
/// ## Why this exists at all
///
/// The session's framing contract is **one typed line, exactly one object**, so
/// a real newline can never arrive in an argument — there is no line left by the
/// time the verb sees it. Without an escape the structured face could therefore
/// only ever put in *one* line per call, while the person at the screen presses
/// Return as often as they like.
///
/// That is not a missing convenience, it is the arrangement
/// `Principio-Doble-Ruta.md` calls non-negotiable being broken: one route with
/// less capability than the other. A program building a five-line block would
/// have to make five calls, leaving the file in four states nobody asked for on
/// the way, each of them saved.
///
/// ## Why exactly three escapes and not a language
///
/// `\\` must be here or a literal backslash becomes untypable, and `\t`
/// because a Makefile is made of tabs and no terminal will send one through this
/// argument. Anything past those is a small language that has to be learnt,
/// documented and got right, and nothing has asked for one — so an unknown
/// escape is left exactly as it was typed rather than swallowed. A `\d` that
/// silently became `d` would corrupt a regular expression in a config file, and
/// the person would never see where it happened.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            // Left as typed, both characters. See above.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            // A trailing backslash is a backslash.
            None => out.push('\\'),
        }
    }
    out
}

fn which_file(face: Face) -> std::io::Result<()> {
    if face.is_machine() {
        face.say(machine::unknown("", ACTIONS));
    } else {
        println!("\n  Which file? `editar <archivo>` opens it.");
        println!("  With a line: `editar <archivo> cambiar 12 el texto nuevo`.\n");
    }
    Ok(())
}

fn refuse(op: &str, error: &EditError, face: Face) -> std::io::Result<()> {
    if face.is_machine() {
        face.say(machine::problem(op, error));
    } else {
        println!("\n  {error}.\n");
    }
    Ok(())
}

/// Apply one change and save it, in one act.
///
/// The save is here and not optional, which is the transactional shape the
/// machine face is built on: there is no buffer left open between typed lines
/// for a caller to lose track of. A change that could not be saved is reported
/// as the save's failure, and the file on disk is still the one that was there.
fn change(
    text: &mut Text,
    apply: impl FnOnce(&mut Text) -> Result<Edited, EditError>,
    face: Face,
    rehearsing: bool,
) -> std::io::Result<()> {
    let edited = match apply(text) {
        Ok(edited) => edited,
        Err(error) => return refuse(op_of(rehearsing), &error, face),
    };

    // The save is the whole difference, and it is one line. `text` is a value
    // this call owns; dropping it unsaved leaves the file on disk exactly as it
    // was, which is the property a rehearsal is.
    if rehearsing {
        return foreseen(&edited, text, face);
    }

    if let Err(error) = text.save() {
        return refuse("edit", &error, face);
    }

    if face.is_machine() {
        face.say(machine::did(&edited, text));
    } else {
        let what = match edited.what {
            thalyx_edit::Change::Inserted => "put in",
            thalyx_edit::Change::Replaced => "changed",
            thalyx_edit::Change::Deleted => "took out",
            _ => "wrote",
        };
        let where_ = match edited.span {
            Some(span) => format!(" at {span}"),
            None => String::new(),
        };
        let moved = edited.lines_after as i64 - edited.lines_before as i64;
        println!(
            "\n  {what}{where_} in {} — {} lines now ({moved:+}), {} bytes.",
            text.path().display(),
            edited.lines_after,
            edited.bytes,
        );
        if text.through_link() {
            // Said to a person too, and not only to a program. Somebody editing
            // a link in `/etc` who is not told has changed a file they did not
            // name.
            println!(
                "  (that is a link; the file written is {})",
                text.target().display()
            );
        }
        println!("  To take this back, it had to have been inside an `intento`.\n");
    }
    Ok(())
}

/// `editar <archivo> sustituir <viejo> <nuevo> [más archivos…]` — one exact
/// substitution, everywhere it occurs, across every file named.
///
/// ## Why this exists, and what it is not
///
/// It comes out of a measurement rather than a wish.
/// `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md` records what the same
/// model did to the same mechanical rename on Linux and on Thalyx: on Linux one
/// call per file, because its editor can replace every occurrence in a file at
/// once; here sixteen calls, one per place, each carrying the whole new text of
/// a line. Every property Thalyx was being measured for held — the workspace
/// boundary, the reversibility, the structured answers — and the *granularity*
/// of the write surface cost a third of the wall clock and half again as many
/// tokens out of the model.
///
/// So this is the shape that was missing and nothing more. It is **not** a
/// rename: nothing here knows what a symbol is, and the same characters in a
/// comment, a string or a longer identifier are matched exactly as the
/// definition is. Calling it `renombrar` would be a promise this machine's index
/// cannot keep today — see `thalyx_edit::Text::substitute`.
///
/// ## Preflight, then write, and never the other order
///
/// Every file is opened, counted and checked **before** any of them is written.
/// A file that is not there, is not text, is over the ceiling, was named twice,
/// or does not contain the text at all stops the whole call with nothing
/// changed. That is the answer that costs a caller one corrected call; the
/// alternative — change four files and refuse the fifth — costs it a
/// reconstruction, and rule 9 says which of those to pick.
///
/// What preflight cannot promise is the write itself: a disk that fills between
/// the third save and the fourth leaves three files changed. Each save is
/// atomic on its own, so nothing is half-written, and the answer then says
/// exactly which files carry the new text and which do not. Nothing here builds
/// a transaction to avoid that — `intento` is the transaction, it is proven on
/// real hardware, and a second weaker one beside it is the thing
/// `Principio-Doble-Ruta` calls drift.
fn substitute(
    here: &Where,
    first_named: &str,
    rest: &str,
    face: Face,
    rehearsing: bool,
) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;

    let op = op_of(rehearsing);
    let Some(words) = crate::words::asked(face, op, rest) else {
        return Ok(());
    };
    if words.len() < 2 {
        return refuse_substitution(
            op,
            &EditError::Incomplete {
                needs: "`sustituir` needs the text to replace and the text to put in its \
                        place: editar <archivo> sustituir <viejo> <nuevo> [más archivos…]",
            },
            face,
        );
    }
    let old = words[0].as_str().to_string();
    let new = words[1].as_str().to_string();

    // Asked before a single file is opened. A caller that sent the same string
    // twice, or one with a line break in it, has asked for something no file
    // can answer, and opening sixty-four of them to find that out is sixty-four
    // opens spent on a refusal that was decidable from the arguments.
    if let Err(error) = thalyx_edit::substitutable(&old, &new) {
        return refuse_substitution(op, &error, face);
    }

    let mut named: Vec<String> = Vec::with_capacity(words.len() - 1);
    named.push(first_named.to_string());
    named.extend(words[2..].iter().map(|word| word.as_str().to_string()));
    if named.len() > thalyx_edit::MOST_FILES {
        return refuse_substitution(
            op,
            &EditError::TooMuch {
                what: "files named in one substitution",
                given: named.len(),
                most: thalyx_edit::MOST_FILES,
            },
            face,
        );
    }

    // Held open for the whole call, and that is load-bearing rather than tidy:
    // for a confined session the anchor *is* the descriptor the kernel resolved
    // inside the workspace, `Text::save` stages beside it, and dropping it after
    // the preflight would mean saving through a path that has to be resolved a
    // second time — which is the precise shape of the check-then-reopen this
    // boundary was hardened to stop.
    let mut held: Vec<(crate::confine::Anchored, Text)> = Vec::with_capacity(named.len());
    let mut seen: std::collections::BTreeSet<(u64, u64)> = std::collections::BTreeSet::new();
    let mut total = 0usize;

    for name in &named {
        let path = thalyx_files::resolve(here.at(), name);
        let anchored = match here
            .anchor(&path)
            .and_then(|_| here.anchor_parent(&path))
            .map_err(|error| {
                EditError::Absent(
                    error
                        .path()
                        .unwrap_or(std::path::Path::new(""))
                        .to_path_buf(),
                )
            }) {
            Ok(anchored) => anchored,
            Err(error) => return refuse_substitution(op, &error, face),
        };

        let text = match Text::open_anchored(anchored.path(), &path) {
            Ok(text) => text,
            Err(error) => return refuse_substitution(op, &error, face),
        };

        // Identity and not the name. Two names for one file — a symlink, a hard
        // link, `./src/x.rs` beside `src/x.rs` — would each be substituted
        // against the text as it was before the call, and the second save would
        // silently throw the first away.
        match std::fs::metadata(anchored.path()) {
            Ok(meta) => {
                if !seen.insert((meta.dev(), meta.ino())) {
                    return refuse_substitution(op, &EditError::RepeatedPath { path }, face);
                }
            }
            Err(error) => {
                return refuse_substitution(
                    op,
                    &EditError::Unreadable {
                        path,
                        detail: error.to_string(),
                    },
                    face,
                );
            }
        }

        let found = text.occurrences(&old);
        if found == 0 {
            return refuse_substitution(op, &EditError::NoOccurrences { path, old }, face);
        }
        total += found;
        held.push((anchored, text));
    }

    if total > thalyx_edit::MOST_REPLACEMENTS {
        return refuse_substitution(
            op,
            &EditError::TooMuch {
                what: "places to change in one substitution",
                given: total,
                most: thalyx_edit::MOST_REPLACEMENTS,
            },
            face,
        );
    }

    let mut done: Vec<thalyx_edit::Substituted> = Vec::with_capacity(held.len());
    for index in 0..held.len() {
        let outcome = {
            let (_, text) = &mut held[index];
            match text.substitute(&old, &new) {
                // The save is what makes this the same transaction shape every
                // other structured edit has: nothing is left open between one
                // typed line and the next.
                Ok(one) if !rehearsing => text.save().map(|_| one),
                other => other,
            }
        };
        match outcome {
            Ok(one) => done.push(one),
            Err(error) => {
                let left: Vec<std::path::PathBuf> = held[index..]
                    .iter()
                    .map(|(_, text)| text.path().to_path_buf())
                    .collect();
                return half_done(op, &old, &new, &done, &left, &error, face);
            }
        }
    }

    if face.is_machine() {
        face.say(if rehearsing {
            thalyx_edit::machine::would_substitute(&old, &new, &done)
        } else {
            thalyx_edit::machine::substituted(&old, &new, &done)
        });
        return Ok(());
    }

    let places: usize = done.iter().map(|one| one.replacements).sum();
    let what = if rehearsing {
        "would change"
    } else {
        "changed"
    };
    println!(
        "\n  {what} {places} place(s) in {} file(s), `{old}` -> `{new}`:",
        done.len()
    );
    for one in &done {
        println!(
            "    {} — {} place(s) on {} line(s), from line {}",
            one.path.display(),
            one.replacements,
            one.lines,
            one.first_line
        );
    }
    if rehearsing {
        println!("  Nothing was written.\n");
    } else {
        println!("  To take this back, it had to have been inside an `intento`.\n");
    }
    Ok(())
}

/// One substitution of a batch, as the line spelled it.
struct Operation {
    old: String,
    new: String,
    paths: Vec<String>,
}

/// `editar <archivo> sustituir-lote <n> <viejo> <nuevo> [n-1 archivos] <n> …`
///
/// ## Why the counts, and why the first file is where it is
///
/// The line has to say where one operation stops and the next begins, and both
/// of the obvious ways of saying it are wrong. A separator word — `--`, `+` —
/// is a word a file can be called and a word an exact string can be; the day it
/// collides, a rename silently becomes two. Guessing from the shape is worse:
/// `old`, `new` and a path are all just words.
///
/// A count cannot collide with anything, because after it is read the arity of
/// everything that follows is known. It is the total number of files that
/// operation names, and the first operation takes its first file from the name
/// before the subverb — which is where `editar` puts a file name, and where a
/// caller reading `editar src/a.rs sustituir …` already expects one.
///
/// **No caller writes this by hand.** `thalyx_edit`'s `substitute_batch` takes
/// `operations: [{old, new, paths}]` and composes the line, which is the whole
/// point of an adapter; a person with two substitutions to make types
/// `sustituir` twice and is right to.
///
/// ## What the batch means, said before it is implemented
///
/// **The operations apply in the order they were given, each to what the one
/// before it left.** That is exactly what the same calls made one after another
/// mean, which is the property that matters: a batch is a way to spend one
/// round trip instead of five, never a different semantics arriving under a new
/// name. It is also what makes the natural pair
///
///     uids::Thing::load  ->  uids::ThingRenamed::load
///     Thing::load        ->  ThingRenamed::load
///
/// mean what whoever wrote it meant — the qualified spellings first, then
/// whatever bare ones are left. Those two *overlap* and the order settles them,
/// which is defined rather than accidental.
///
/// What is refused is the composition that order cannot settle honestly:
/// `A -> B` and then `B -> C`, where the second operation eats what the first
/// wrote and every `A` silently becomes a `C`. Nothing in the call shows that,
/// so it is refused with both strings named — [`EditError::Chained`].
///
/// ## Preflight is complete here, and that is not a slogan
///
/// Every file is opened **once**, every operation is applied to it **in
/// memory**, and only then is anything saved. So a batch whose third pattern is
/// not in one of its files writes nothing at all, and a file five patterns touch
/// is written once with all five in it rather than five times against a state
/// each save is racing. What remains uncoverable is the save itself, and the
/// answer for that is the same one `sustituir` gives: exactly which files carry
/// the new text and which do not, and `intento` to put all of them back.
fn substitute_batch(
    here: &Where,
    first_named: &str,
    rest: &str,
    face: Face,
    rehearsing: bool,
) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;

    let op = op_of(rehearsing);
    let Some(words) = crate::words::asked(face, op, rest) else {
        return Ok(());
    };
    let words: Vec<String> = words.iter().map(|word| word.as_str().to_string()).collect();

    let operations = match read_operations(first_named, &words) {
        Ok(operations) => operations,
        Err(error) => return refuse_substitution(op, &error, face),
    };

    // ── everything decidable from the arguments, before a file is opened ──
    //
    // A batch that opens sixty-four files to discover that two of its patterns
    // look for the same text has spent sixty-four opens on a refusal that was
    // in the line all along.
    for operation in &operations {
        if let Err(error) = thalyx_edit::substitutable(&operation.old, &operation.new) {
            return refuse_substitution(op, &error, face);
        }
    }
    for (index, operation) in operations.iter().enumerate() {
        for other in &operations[index + 1..] {
            if other.old == operation.old {
                return refuse_substitution(
                    op,
                    &EditError::BadBatch {
                        why: format!(
                            "two operations of this batch both look for `{}`. The second \
                             would run against what the first left, so what it means \
                             depends on the first — name the text once",
                            operation.old
                        ),
                    },
                    face,
                );
            }
            // The chain, checked between the strings themselves rather than
            // against the files. `A -> B` then `B -> C` is ambiguous whether or
            // not the two happen to share a file today, and a check that let it
            // through on a batch whose paths did not overlap would be a check
            // that starts refusing when somebody adds a file.
            if operation.new.contains(other.old.as_str()) {
                return refuse_substitution(
                    op,
                    &EditError::Chained {
                        earlier_new: operation.new.clone(),
                        later_old: other.old.clone(),
                    },
                    face,
                );
            }
        }
    }

    // The `..` rule, applied here rather than at the external boundary. That
    // boundary checks argument slots by position, and a batch's positions mean
    // nothing until its counts have been read — so it cannot tell a path from
    // an exact string, and checking every word as a path would refuse a rename
    // of the text `../mod.rs`. This is the one place that knows which words are
    // files. `here.anchor` below is still the real check, in the kernel; this
    // is the cheap early one it used to have.
    for operation in &operations {
        for name in &operation.paths {
            if std::path::Path::new(name)
                .components()
                .any(|part| part == std::path::Component::ParentDir)
            {
                return refuse_substitution(
                    op,
                    &EditError::BadBatch {
                        why: format!(
                            "`{name}` has a `..` in it, and every file a batch names is \
                             below the workspace root"
                        ),
                    },
                    face,
                );
            }
        }
    }

    // ── open every distinct file once, and remember which operation wants it ──
    //
    // Once, by inode, across the **whole** batch. Two operations naming the same
    // file is the ordinary case — it is what a five-pattern rename of one symbol
    // looks like — and opening it twice would give each operation a copy of the
    // text as it was before the call, so the second save would throw the first
    // away. That is the same mistake `RepeatedPath` refuses inside one
    // operation, and across operations the answer is not to refuse but to share
    // the open file.
    let mut held: Vec<(crate::confine::Anchored, Text)> = Vec::new();
    let mut by_inode: std::collections::BTreeMap<(u64, u64), usize> = Default::default();
    // Per operation, which entries of `held` it applies to, in the order it
    // named them.
    let mut wants: Vec<Vec<usize>> = Vec::with_capacity(operations.len());

    for operation in &operations {
        let mut mine = Vec::with_capacity(operation.paths.len());
        let mut seen: std::collections::BTreeSet<(u64, u64)> = Default::default();
        for name in &operation.paths {
            let path = thalyx_files::resolve(here.at(), name);
            let anchored = match here
                .anchor(&path)
                .and_then(|_| here.anchor_parent(&path))
                .map_err(|error| {
                    EditError::Absent(
                        error
                            .path()
                            .unwrap_or(std::path::Path::new(""))
                            .to_path_buf(),
                    )
                }) {
                Ok(anchored) => anchored,
                Err(error) => return refuse_substitution(op, &error, face),
            };
            let identity = match std::fs::metadata(anchored.path()) {
                Ok(meta) => (meta.dev(), meta.ino()),
                Err(error) => {
                    return refuse_substitution(
                        op,
                        &EditError::Unreadable {
                            path,
                            detail: error.to_string(),
                        },
                        face,
                    );
                }
            };
            if !seen.insert(identity) {
                return refuse_substitution(op, &EditError::RepeatedPath { path }, face);
            }
            let at = match by_inode.get(&identity) {
                Some(at) => *at,
                None => {
                    let text = match Text::open_anchored(anchored.path(), &path) {
                        Ok(text) => text,
                        Err(error) => return refuse_substitution(op, &error, face),
                    };
                    held.push((anchored, text));
                    by_inode.insert(identity, held.len() - 1);
                    held.len() - 1
                }
            };
            mine.push(at);
        }
        wants.push(mine);
    }

    if held.len() > thalyx_edit::MOST_FILES {
        return refuse_substitution(
            op,
            &EditError::TooMuch {
                what: "files named in one batch",
                given: held.len(),
                most: thalyx_edit::MOST_FILES,
            },
            face,
        );
    }

    // ── apply the whole batch in memory, and only then write ──
    //
    // In order, each operation against what the one before it left, which is
    // the semantics written at the top of this function and the only one that
    // makes a batch equal to the same calls made in sequence.
    let mut done: Vec<Vec<thalyx_edit::Substituted>> = Vec::with_capacity(operations.len());
    let mut total = 0usize;
    for (operation, mine) in operations.iter().zip(&wants) {
        let mut theirs = Vec::with_capacity(mine.len());
        for at in mine {
            let (_, text) = &mut held[*at];
            match text.substitute(&operation.old, &operation.new) {
                Ok(one) => {
                    total += one.replacements;
                    theirs.push(one);
                }
                // Nothing has been saved, so this is an ordinary refusal and
                // the workspace is untouched — which is the whole reason the
                // applying happens before any of the writing.
                Err(error) => return refuse_substitution(op, &error, face),
            }
        }
        done.push(theirs);
    }

    if total > thalyx_edit::MOST_REPLACEMENTS {
        return refuse_substitution(
            op,
            &EditError::TooMuch {
                what: "places to change in one batch",
                given: total,
                most: thalyx_edit::MOST_REPLACEMENTS,
            },
            face,
        );
    }

    let batch: Vec<thalyx_edit::machine::Batched<'_>> = operations
        .iter()
        .zip(&done)
        .map(|(operation, theirs)| thalyx_edit::machine::Batched {
            old: &operation.old,
            new: &operation.new,
            done: theirs,
        })
        .collect();

    if rehearsing {
        if face.is_machine() {
            face.say(thalyx_edit::machine::would_substitute_batch(&batch));
        } else {
            tell_a_person(&operations, &batch, true);
        }
        return Ok(());
    }

    // One save per file, and only files something actually changed. A file
    // every operation skipped cannot exist here — an operation whose text is
    // not in a file it named refused the whole batch above — but saving by
    // `modified` rather than by "we opened it" is the honest condition.
    let mut written: Vec<std::path::PathBuf> = Vec::new();
    for index in 0..held.len() {
        let (_, text) = &mut held[index];
        if !text.is_modified() {
            continue;
        }
        if let Err(error) = text.save() {
            let left: Vec<std::path::PathBuf> = held[index..]
                .iter()
                .filter(|(_, text)| text.is_modified())
                .map(|(_, text)| text.path().to_path_buf())
                .collect();
            if written.is_empty() {
                return refuse_substitution(op, &error, face);
            }
            if face.is_machine() {
                face.say(thalyx_edit::machine::half_substituted_batch(
                    &batch, &written, &left, &error,
                ));
            } else {
                println!("\n  {error}.");
                for path in &written {
                    println!("    changed: {}", path.display());
                }
                for path in &left {
                    println!("    left alone: {}", path.display());
                }
                println!("  Only an `intento` puts all of them back.\n");
            }
            return Ok(());
        }
        written.push(text.path().to_path_buf());
    }

    if face.is_machine() {
        face.say(thalyx_edit::machine::substituted_batch(&batch));
        return Ok(());
    }
    tell_a_person(&operations, &batch, false);
    Ok(())
}

/// Read the operations off the line, or say exactly which word broke it.
///
/// Its own function because it takes no filesystem and no session: the whole of
/// what a batch's shape is can then be tested against a list of strings, which
/// is what a grammar deserves.
fn read_operations(first_named: &str, words: &[String]) -> Result<Vec<Operation>, EditError> {
    const SHAPE: &str = "editar <archivo> sustituir-lote <cuántos-archivos> <viejo> <nuevo> \
                         [más archivos…] [<cuántos-archivos> <viejo> <nuevo> <archivos…>…]";

    let mut operations: Vec<Operation> = Vec::new();
    let mut at = 0usize;
    while at < words.len() {
        let counted: usize = words[at].parse().map_err(|_| EditError::BadBatch {
            why: format!(
                "`{}` is where this batch says how many files its operation {} names, and \
                 it is not a number: {SHAPE}",
                words[at],
                operations.len() + 1
            ),
        })?;
        if counted == 0 {
            return Err(EditError::BadBatch {
                why: format!(
                    "operation {} of this batch names no file, and a substitution across \
                     nothing is not a substitution",
                    operations.len() + 1
                ),
            });
        }
        // The first operation's first file is the name before the subverb, so
        // that is one fewer word to read off the line for it and none for the
        // rest.
        let borrowed = usize::from(operations.is_empty());
        let here = counted - borrowed;
        let needs = 3 + here;
        if at + needs > words.len() {
            return Err(EditError::BadBatch {
                why: format!(
                    "operation {} of this batch says it names {counted} file(s) and the \
                     line ends before they are all there: {SHAPE}",
                    operations.len() + 1
                ),
            });
        }
        let mut paths: Vec<String> = Vec::with_capacity(counted);
        if operations.is_empty() {
            paths.push(first_named.to_string());
        }
        paths.extend(words[at + 3..at + 3 + here].iter().cloned());
        operations.push(Operation {
            old: words[at + 1].clone(),
            new: words[at + 2].clone(),
            paths,
        });
        at += needs;
    }

    if operations.is_empty() {
        return Err(EditError::BadBatch {
            why: format!("this batch carries no operation at all: {SHAPE}"),
        });
    }
    if operations.len() > thalyx_edit::MOST_OPERATIONS {
        return Err(EditError::TooMuch {
            what: "substitutions in one batch",
            given: operations.len(),
            most: thalyx_edit::MOST_OPERATIONS,
        });
    }
    Ok(operations)
}

/// The batch, for somebody reading it on a screen.
fn tell_a_person(
    operations: &[Operation],
    batch: &[thalyx_edit::machine::Batched<'_>],
    rehearsing: bool,
) {
    let places: usize = batch
        .iter()
        .flat_map(|one| one.done.iter())
        .map(|one| one.replacements)
        .sum();
    let what = if rehearsing {
        "would change"
    } else {
        "changed"
    };
    println!(
        "\n  {what} {places} place(s) with {} substitution(s):",
        operations.len()
    );
    for one in batch {
        println!("    `{}` -> `{}`", one.old, one.new);
        for row in one.done {
            println!(
                "      {} — {} place(s) on {} line(s), from line {}",
                row.path.display(),
                row.replacements,
                row.lines,
                row.first_line
            );
        }
    }
    if rehearsing {
        println!("  Nothing was written.\n");
    } else {
        println!("  To take this back, it had to have been inside an `intento`.\n");
    }
}

/// A substitution that refused before it wrote anything.
///
/// Its own function so that `wrote: false` cannot be forgotten on one of the
/// eight paths that reach it. That field is the whole difference between a
/// workspace nobody touched and one halfway through a rename, and a caller
/// cannot get it from `ok`.
fn refuse_substitution(op: &str, error: &EditError, face: Face) -> std::io::Result<()> {
    if face.is_machine() {
        face.say(thalyx_edit::machine::not_substituted(op, error));
    } else {
        println!("\n  {error}.");
        println!("  Nothing was written.\n");
    }
    Ok(())
}

/// A substitution that passed its preflight and then could not finish.
fn half_done(
    op: &str,
    old: &str,
    new: &str,
    done: &[thalyx_edit::Substituted],
    left: &[std::path::PathBuf],
    error: &EditError,
    face: Face,
) -> std::io::Result<()> {
    // Nothing written yet means this is an ordinary refusal, and saying
    // otherwise would send a caller to abandon an attempt it never needed.
    if done.is_empty() {
        return refuse_substitution(op, error, face);
    }
    if face.is_machine() {
        face.say(thalyx_edit::machine::half_substituted(
            old, new, done, left, error,
        ));
    } else {
        println!("\n  {error}.");
        println!(
            "  {} file(s) were already changed and {} were not:",
            done.len(),
            left.len()
        );
        for one in done {
            println!("    changed: {}", one.path.display());
        }
        for path in left {
            println!("    left alone: {}", path.display());
        }
        println!("  Only an `intento` puts all of them back.\n");
    }
    Ok(())
}

/// The `op` a refusal carries, which has to follow the verb it stood in for.
///
/// `describe` promises `rehearse` for `ensayo`, and a refusal that came back
/// under `edit` would be read as the file having been touched and failed.
fn op_of(rehearsing: bool) -> &'static str {
    if rehearsing { "rehearse" } else { "edit" }
}

fn nothing_to_rehearse(what: &str, face: Face) -> std::io::Result<()> {
    let why = format!("{what} changes nothing, so there is nothing to rehearse");
    if face.is_machine() {
        face.say(thalyx_files::machine::declined(
            "rehearse", "harmless", &why,
        ));
    } else {
        println!("\n  {why}.\n");
    }
    Ok(())
}

fn foreseen(edited: &Edited, text: &Text, face: Face) -> std::io::Result<()> {
    if face.is_machine() {
        face.say(machine::would(edited, text));
        return Ok(());
    }

    let what = match edited.what {
        thalyx_edit::Change::Inserted => "would put in",
        thalyx_edit::Change::Replaced => "would change",
        thalyx_edit::Change::Deleted => "would take out",
        _ => "would write",
    };
    let where_ = match edited.span {
        Some(span) => format!(" at {span}"),
        None => String::new(),
    };
    let moved = edited.lines_after as i64 - edited.lines_before as i64;
    println!(
        "\n  {what}{where_} in {} — {} lines after ({moved:+}), {} bytes.",
        text.path().display(),
        edited.lines_after,
        edited.bytes,
    );
    if text.through_link() {
        println!(
            "  (that is a link; the file that would be written is {})",
            text.target().display()
        );
    }
    println!("  Nothing was written.\n");
    Ok(())
}

fn show(text: &Text, argument: &str, face: Face) -> std::io::Result<()> {
    let (from, to) = if argument.is_empty() {
        (1, text.count().min(PAGE))
    } else {
        match thalyx_edit::span(argument) {
            Ok(span) => (span.from, span.to.min(text.count())),
            Err(error) => return refuse("edit_show", &error, face),
        }
    };
    if from > text.count() {
        return refuse(
            "edit_show",
            &EditError::NoSuchLine {
                path: text.path().to_path_buf(),
                asked: from,
                has: text.count(),
            },
            face,
        );
    }

    let rows: Vec<(usize, &str)> = (from..=to)
        .filter_map(|n| text.lines().get(n - 1).map(|body| (n, body.as_str())))
        .collect();
    let more = to < text.count();

    if face.is_machine() {
        face.say(machine::shown(text, from, &rows, more));
    } else {
        println!();
        // Right-aligned on the widest number actually shown, so the text starts
        // in one column instead of stepping right at line 100.
        let width = to.to_string().len();
        for (number, body) in &rows {
            println!("  {number:>width$}  {body}");
        }
        if more {
            println!(
                "\n  {} of {} lines. The rest: `editar … ver {}-{}`.",
                rows.len(),
                text.count(),
                to + 1,
                text.count()
            );
        }
        println!();
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────── the screen

/// Open the full-screen editor **on this terminal**, or say why there is none.
///
/// Called by the text session, which is the surface that has a terminal. The
/// screen has its own — same engine, different pixels — and the reason there are
/// two is in [`Opens`]: what an editor draws on is a property of the surface,
/// and the escape sequences below are meaningless anywhere but here.
pub fn on_this_terminal(path: &Path, face: Face) -> std::io::Result<()> {
    use std::os::fd::AsFd;

    let mut text = match Text::open(path) {
        Ok(text) => text,
        Err(error) => return refuse("edit", &error, face),
    };
    let text = &mut text;

    // Asked of the terminal rather than inferred from the face. A person can
    // turn the structured face on and still be at a keyboard, and refusing them
    // a screen because of a display setting would be taking capability away for
    // no reason.
    let stdin = std::io::stdin();
    let Some((rows, columns)) = thalyx_syscall::terminal_size(stdin.as_fd()) else {
        return refuse("edit", &EditError::NoScreen, face);
    };

    // Two rows are not the file: the header and the key legend. Subtracted here
    // rather than inside the viewport, because the viewport's business is the
    // text and a viewport that quietly knows about a status bar is one that will
    // be wrong the day there are two.
    let height = (rows as usize).saturating_sub(2).max(1);
    let mut edit = Editing::new(Viewport::of(height, columns as usize));
    let mut note = String::new();
    // Whether the last key was a Ctrl-X on a file with unsaved changes.
    //
    // A flag and deliberately **not** a second `read_key` nested inside the
    // Ctrl-X arm. The nested version was written first and it swallowed the key
    // that answered it: Ctrl-X then Ctrl-O — which is what a person does when
    // asked whether to save — consumed the Ctrl-O as a bare "not Ctrl-X" and
    // saved nothing. Every key stays in the one loop that knows what keys mean.
    let mut asked_to_leave = false;

    loop {
        draw(text, &edit, &note, rows as usize, columns as usize)?;
        note.clear();

        let Some(key) = crate::term::read_key()? else {
            // The input ended with the editor open. Leaving without saving is
            // the cautious half: the file on disk is untouched, and a session
            // that guessed and saved would have written whatever was on screen
            // when a pipe closed.
            break;
        };
        let was_asked = asked_to_leave;
        asked_to_leave = false;

        match edit.press(text, key) {
            Reaction::Save => match text.save() {
                Ok(done) => note = format!("saved — {} bytes", done.bytes),
                Err(error) => note = format!("{error}"),
            },
            Reaction::Leave => {
                if !text.is_modified() || was_asked {
                    break;
                }
                // Asked, and the default is to stay. A keystroke away from
                // losing an afternoon's work needs an answer that is not the one
                // a person gives by pressing Return out of habit.
                asked_to_leave = true;
                note = "unsaved — Ctrl-O to write, Ctrl-X again to leave anyway".into();
            }
            Reaction::Changed | Reaction::Moved | Reaction::Nothing => {}
        }
    }

    // Put the screen back the way a session expects to find it. Without this the
    // prompt lands wherever the cursor happened to be, on top of the file.
    print!("\x1b[2J\x1b[H");
    std::io::stdout().flush()?;

    if face.is_machine() {
        // Even here. A program that turned the structured face on and then found
        // itself at a keyboard must still get an object rather than silence —
        // silence is the one thing a parser cannot tell from a hang.
        let what = if text.is_modified() {
            thalyx_edit::Change::Unchanged
        } else {
            thalyx_edit::Change::Saved
        };
        face.say(machine::did(
            &Edited {
                what,
                path: text.path().to_path_buf(),
                span: None,
                lines_before: text.count(),
                lines_after: text.count(),
                bytes: text.weight(),
            },
            text,
        ));
    }
    Ok(())
}

/// Put one whole screen out in a single write.
///
/// One write and not one per row, and that is not an optimisation. A serial
/// console at any speed draws a screen written line by line visibly from the top
/// down, and the flicker reads as a machine struggling. The image's console is a
/// serial port on real hardware — the same one that cost 35 of 38 seconds of
/// boot on 2026-08-07.
fn draw(
    text: &Text,
    edit: &Editing,
    note: &str,
    rows: usize,
    columns: usize,
) -> std::io::Result<()> {
    let frame = edit.frame(text);
    let mut out = String::with_capacity(rows * columns);
    // Home, then clear from the cursor down. Not `\x1b[2J`: erasing the whole
    // screen before redrawing it is what produces a black flash between frames
    // on a slow console.
    out.push_str("\x1b[H\x1b[J");

    let name = text.path().display().to_string();
    let state = if text.is_modified() { "*" } else { " " };
    let where_ = format!("{}:{}", edit.cursor.line_number(), edit.cursor.column + 1);
    let header = format!("{state}{name}  {} lines  {where_}", text.count());
    out.push_str(&clip(&header, columns));
    out.push_str("\r\n");

    for row in &frame.rows {
        match row.number {
            Some(_) => out.push_str(&clip(&row.text, columns)),
            // A row past the end of the file. `~` is what every editor has put
            // there for fifty years, and it is the one character a person will
            // not mistake for a line of their file.
            None => out.push('~'),
        }
        out.push_str("\r\n");
    }

    let legend = if note.is_empty() {
        "Ctrl-O save   Ctrl-X leave   Ctrl-U undo   Ctrl-K cut line".to_string()
    } else {
        note.to_string()
    };
    out.push_str(&clip(&legend, columns));

    // The cursor last, so it lands after everything drawn. `+2` on the row: one
    // for the header, one because terminals count from 1.
    out.push_str(&format!(
        "\x1b[{};{}H",
        frame.cursor_row + 2,
        frame.cursor_column + 1
    ));

    let mut stdout = std::io::stdout();
    stdout.write_all(out.as_bytes())?;
    stdout.flush()
}

/// Cut a line to the width of the screen, counting characters.
///
/// Characters and not bytes: cutting `contraseña` by bytes can end the line in
/// half a letter, and a terminal handed half a UTF-8 sequence prints a
/// replacement character that stays on screen until the next full redraw.
fn clip(text: &str, columns: usize) -> String {
    text.chars().take(columns).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_and_the_text_are_split_so_indentation_survives() {
        // The case this exists for: a configuration file line that begins with
        // spaces. `split_whitespace` would eat them and silently un-indent it.
        assert_eq!(split_argument("12     indented"), ("12", "    indented"));
        assert_eq!(split_argument("12 plain"), ("12", "plain"));
        assert_eq!(split_argument("12"), ("12", ""));
    }

    #[test]
    fn a_break_can_be_typed_because_a_typed_line_cannot_contain_one() {
        assert_eq!(unescape("a\\nb\\nc"), "a\nb\nc");
        assert_eq!(unescape("\\tsangría"), "\tsangría");
        // A literal backslash stays typable, which is what stops this from
        // being a one-way door for anybody with a Windows path in a config file.
        assert_eq!(unescape("C:\\\\ruta"), "C:\\ruta");
    }

    #[test]
    fn an_escape_this_does_not_know_is_left_exactly_as_it_was_typed() {
        // `\d` in a regular expression, which is the case that would be
        // silently corrupted by a version of this that swallowed the backslash
        // — and the person would never find out where it happened.
        assert_eq!(unescape("^\\d+$"), "^\\d+$");
        assert_eq!(unescape("trailing\\"), "trailing\\");
    }

    /// The defect, at the line it happened on.
    ///
    /// On the image `crear prueba.txt` worked and `editar prueba.txt` answered
    /// *«there is no terminal here to draw an editor on»* — on the display the
    /// machine boots into. What was wrong is that this verb answered the
    /// question at all: it looked at descriptor 0, which under the screen is
    /// `/dev/null`, and concluded there was nowhere to draw.
    ///
    /// So the claim here is that it no longer answers it. **No terminal is
    /// faked and no check is deleted** — the check moved to the surface that
    /// owns a terminal, and what comes back is the file for whichever surface
    /// asked. This test runs under `cargo test`, whose descriptor 0 is not a
    /// terminal either: before the change it would have been a refusal.
    #[test]
    fn a_file_asked_for_with_no_subverb_is_handed_back_for_a_surface_to_open() {
        let tmp = tempfile::tempdir().expect("a temporary directory");
        let file = tmp.path().join("prueba.txt");
        std::fs::write(&file, "uno\ndos\n").expect("the file");

        match run(&Where::start(), &file.display().to_string(), Face::Human)
            .expect("`editar <archivo>`")
        {
            // And the right file: a transition that named the wrong path would
            // open an editor on somebody else's work, which is worse than the
            // refusal it replaced.
            Opens::Editor(opened) => assert_eq!(opened, file),
            Opens::Nothing => {
                panic!("`editar <archivo>` refused instead of asking for a surface")
            }
        }
    }

    /// The other route, unchanged: a subverb answers here and asks for nothing.
    ///
    /// Beside the one above rather than in another file, because the pair is the
    /// claim — one verb, two shapes — and a change that made *every* `editar`
    /// ask for a surface would leave a program down a pipe waiting for a screen.
    #[test]
    fn a_subverb_is_still_finished_by_the_verb_and_asks_for_no_surface() {
        let tmp = tempfile::tempdir().expect("a temporary directory");
        let file = tmp.path().join("prueba.txt");
        std::fs::write(&file, "uno\ndos\n").expect("the file");

        for typed in ["ver", "cambiar 1 UNO", "borrar 2"] {
            let asked = run(
                &Where::start(),
                &format!("{} {typed}", file.display()),
                Face::Human,
            )
            .expect("`editar <archivo> <subverbo>`");
            assert!(
                matches!(asked, Opens::Nothing),
                "`editar … {typed}` asked for a surface"
            );
        }
        // And it really did the work, rather than reporting nothing because it
        // took an early way out.
        assert_eq!(std::fs::read_to_string(&file).expect("the file"), "UNO\n");
    }

    /// A file that cannot be opened is refused **here**, in words, and never by
    /// a display that has already gone blank to show it.
    #[test]
    fn a_file_that_is_not_text_is_refused_before_any_surface_is_asked_for() {
        let tmp = tempfile::tempdir().expect("a temporary directory");
        let file = tmp.path().join("binario");
        std::fs::write(&file, [0u8, 159, 146, 150]).expect("the file");

        let asked = run(&Where::start(), &file.display().to_string(), Face::Human)
            .expect("`editar <binario>`");
        assert!(
            matches!(asked, Opens::Nothing),
            "a binary file was handed to a surface to open"
        );
    }

    #[test]
    fn a_line_is_cut_to_the_screen_by_characters_and_never_through_a_letter() {
        assert_eq!(clip("contraseña", 8), "contrase");
        assert_eq!(clip("contraseña", 9), "contraseñ");
        assert_eq!(clip("corto", 40), "corto");
    }
}
