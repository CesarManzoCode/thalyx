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
use thalyx_edit::screen::{Editing, Reaction, Viewport};
use thalyx_edit::{EditError, Edited, Text, machine};

/// The subverbs, in both spellings, and the order they are offered in.
const ACTIONS: &[&str] = &[
    "ver", "show", "poner", "insert", "cambiar", "replace", "borrar", "delete",
];

/// How many lines one `ver` answers with when nobody said.
///
/// `Superficie-para-el-LLM.md`, punto **B1**: no answer may eat a context
/// window, and a file of four thousand lines returned whole is a caller that
/// forgot what it was doing. The count and `more` are in every answer, so asking
/// for the rest costs one more call and never a guess.
const PAGE: usize = 200;

pub fn run(here: &Where, rest: &str, face: Face) -> std::io::Result<()> {
    // Only the name is split as words. Everything after it is taken from the
    // line byte for byte, because the third part is text going into a file and a
    // configuration line that starts with four spaces means something with them
    // and something else without. `words.rs` calls this the one carve-out.
    let named = match crate::words::first(rest) {
        Ok(Some(named)) => named,
        Ok(None) => return which_file(face),
        Err(why) => {
            if face.is_machine() {
                face.say(thalyx_files::machine::refused(
                    "edit",
                    why.word(),
                    why.remedy(),
                    &why.to_string(),
                ));
            } else {
                println!("\n  {why}\n");
            }
            return Ok(());
        }
    };
    let (named, after) = named;
    if named.is_empty() {
        return which_file(face);
    }

    let path = thalyx_files::resolve(here.at(), named.as_str());
    let mut words = after.splitn(2, char::is_whitespace);
    let action = words.next().unwrap_or("").trim();
    let argument = words.next().unwrap_or("").trim();

    let mut text = match Text::open(&path) {
        Ok(text) => text,
        Err(error) => return refuse("edit", &error, face),
    };

    match action {
        // No subverb is the person's case, and it is the one that needs a
        // terminal. Refused with its own word rather than left to hang, because
        // a program down a pipe waiting for a screen waits forever.
        "" => screen(&mut text, face),
        "ver" | "show" => show(&text, argument, face),
        "poner" | "insert" => {
            let (at, body) = split_argument(argument);
            let body = unescape(body);
            match thalyx_edit::span(at) {
                Ok(span) => change(&mut text, |t| t.insert(span.from, &body), face),
                Err(error) => refuse("edit", &error, face),
            }
        }
        "cambiar" | "replace" => {
            let (at, body) = split_argument(argument);
            let body = unescape(body);
            match thalyx_edit::span(at) {
                Ok(span) => change(&mut text, |t| t.replace(span, &body), face),
                Err(error) => refuse("edit", &error, face),
            }
        }
        "borrar" | "delete" => match thalyx_edit::span(argument) {
            Ok(span) => change(&mut text, |t| t.delete(span), face),
            Err(error) => refuse("edit", &error, face),
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
) -> std::io::Result<()> {
    let edited = match apply(text) {
        Ok(edited) => edited,
        Err(error) => return refuse("edit", &error, face),
    };
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

/// Open the full-screen editor, or say why there is none.
fn screen(text: &mut Text, face: Face) -> std::io::Result<()> {
    use std::os::fd::AsFd;

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

    #[test]
    fn a_line_is_cut_to_the_screen_by_characters_and_never_through_a_letter() {
        assert_eq!(clip("contraseña", 8), "contrase");
        assert_eq!(clip("contraseña", 9), "contraseñ");
        assert_eq!(clip("corto", 40), "corto");
    }
}
