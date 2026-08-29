//! `encontrar` and `contenido` — point 6 of the usable terminal.
//!
//! Two questions, two verbs, and a third that already existed:
//!
//! | asked | verb | answered from |
//! |---|---|---|
//! | where is the file called this | `encontrar` | walking the tree |
//! | which files say this | `contenido` | reading the tree |
//! | where is this name declared and used | `buscar` | the semantic index |
//!
//! Cesar decided the split on 2026-08-23. One verb with a flag would have been
//! one thing to discover instead of two, and the reason it lost is the third
//! of the five costs in `Superficie-para-el-LLM.md`: a verb whose meaning
//! depends on a flag is a verb a caller can get wrong silently. Three questions
//! that differ in what they read cannot be told apart by looking at the answer.
//!
//! ## Where the flags may go, and why that is a rule
//!
//! Only at the front. `contenido en=src TODO` searches `src` for `TODO`;
//! `contenido TODO en=src` searches the whole tree for the literal text
//! `TODO en=src`.
//!
//! That looks like the awkward choice until the other one is written out. If a
//! flag were recognised anywhere, then searching for the text `en=produccion`
//! would quietly become a search for nothing under a folder called
//! `produccion` — an answer that looks right, arrives fast, and is about a
//! different question. The rule here fails the other way: the search finds
//! nothing, and the person sees their own words in the answer and moves the
//! flag. Rule 9 — the cautious answer, never the plausible one.
//!
//! It also means the subject is **everything after the flags**, joined with
//! single spaces — `contenido fn main` looks for `fn main` and needs no quotes.
//! Until 2026-08-23 it was the rest of the line untouched, because whether
//! Thalyx had quoting at all was point 9 and undecided; a search verb that
//! invented quotes would have been deciding it. Cesar decided it that day, so a
//! run of spaces now collapses the way it does in every terminal and
//! `contenido "fn  main"` is how the other thing is said. See
//! `vault/02-Arquitectura/Palabras.md`.

use crate::files::{Face, Where};
use thalyx_files::search::{Found, Hit, Named};
use thalyx_files::window::{Asked, Cursor, Page};
use thalyx_files::{FileError, Size};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// `encontrar <patrón>` — files whose name matches, anywhere below here.
pub fn by_name(here: &Where, rest: &str, face: Face) -> Fallible {
    let op = "find";
    let Some(asked) = parse(rest, face, op) else {
        return Ok(());
    };
    let root = asked.root(here);

    let found = match thalyx_files::search::by_name(&root, &asked.subject) {
        Ok(found) => found,
        Err(error) => return refused(face, op, &error),
    };

    let rows: Vec<&Named> = found.rows.iter().collect();
    let page =
        match thalyx_files::window::page(rows, |row| row.path.as_bytes().to_vec(), &asked.window) {
            Ok(page) => page,
            Err(why) => {
                declined(face, op, "unordered", &why.to_string());
                return Ok(());
            }
        };

    if face.is_machine() {
        face.say(thalyx_files::machine::found_by_name(
            &root,
            &asked.subject,
            &found,
            &page,
        ));
        return Ok(());
    }

    println!();
    if page.rows.is_empty() {
        // Two facts, not one. "Nothing matched" and "nothing was looked at" send
        // a person to opposite places, and a tree of eleven thousand files that
        // matched nothing means the pattern is wrong.
        println!(
            "  nothing under {} is called `{}` — {} looked at.",
            root.display(),
            asked.subject,
            found.looked_at
        );
    }
    for row in &page.rows {
        match &row.kind {
            thalyx_files::Kind::Directory => println!("  {}/", row.path),
            thalyx_files::Kind::File { bytes } => {
                println!("  {}  ({})", row.path, Size(*bytes))
            }
            thalyx_files::Kind::Link { to, .. } => {
                println!("  {} -> {}", row.path, to.display())
            }
            thalyx_files::Kind::Other(what) => println!("  {}  [{what}]", row.path),
        }
    }
    say_the_rest(&page, &found, "encontrar");
    Ok(())
}

/// `contenido <texto>` — lines that hold the text, literally.
pub fn in_contents(here: &Where, rest: &str, face: Face) -> Fallible {
    let op = "grep";
    let Some(asked) = parse(rest, face, op) else {
        return Ok(());
    };
    let root = asked.root(here);

    let found = match thalyx_files::search::in_contents(&root, &asked.subject) {
        Ok(found) => found,
        Err(error) => return refused(face, op, &error),
    };

    let rows: Vec<&Hit> = found.rows.iter().collect();
    let page = match thalyx_files::window::page(rows, hit_key, &asked.window) {
        Ok(page) => page,
        Err(why) => {
            declined(face, op, "unordered", &why.to_string());
            return Ok(());
        }
    };

    if face.is_machine() {
        face.say(thalyx_files::machine::found_in_contents(
            &root,
            &asked.subject,
            &found,
            &page,
        ));
        return Ok(());
    }

    println!();
    if page.rows.is_empty() {
        println!(
            "  nothing under {} says `{}` — {} file(s) read.",
            root.display(),
            asked.subject,
            found.looked_at - found.not_text
        );
    }
    for row in &page.rows {
        // `path:line:` and nothing else in front of it, because that is what
        // every tool in the world prints and what `editar <archivo> ver <línea>`
        // takes next.
        println!(
            "  {}:{}: {}{}",
            row.path,
            row.line,
            row.text.trim_end(),
            if row.cut { " …" } else { "" }
        );
    }
    if found.not_text > 0 {
        println!();
        println!(
            "  {} file(s) skipped — not text, or over {}.",
            found.not_text,
            Size(thalyx_files::search::WEIGHT)
        );
    }
    say_the_rest(&page, &found, "contenido");
    Ok(())
}

/// What a cursor into a list of hits names. See `search::hit_key`.
///
/// Takes a double reference because the rows being paged are borrowed from the
/// answer rather than moved out of it — the window is generic over the row and
/// a page of `&Hit` is what avoids copying every line twice.
fn hit_key(row: &&Hit) -> Vec<u8> {
    thalyx_files::search::hit_key(row)
}

/// The tail every human answer ends with: what was left out, and what could not
/// be read.
///
/// Both, always, and never only when there is something to say — the count of
/// what was looked at is how a person judges whether a search that found
/// nothing asked the right tree.
fn say_the_rest<T, R>(page: &Page<T>, found: &Found<R>, verb: &str) {
    if page.more {
        println!();
        println!(
            "  showing {} of {}. `{verb} cursor={} …` continues.",
            page.before + page.rows.len(),
            page.total,
            page.next.as_deref().unwrap_or("…")
        );
    }
    // Rule 10, and it is printed after the rows so it is the last thing on
    // screen. A folder that could not be opened might be the one holding what
    // was being looked for, and an answer that scrolled it off the top reads as
    // a complete answer.
    if !found.unreadable.is_empty() {
        println!();
        println!("  {} could not be read:", found.unreadable.len());
        for (path, why) in &found.unreadable {
            println!("    {path} — {why}");
        }
    }
    println!();
}

/// What one typed line asked for.
struct Asking {
    subject: String,
    folder: String,
    window: Asked,
}

impl Asking {
    fn root(&self, here: &Where) -> std::path::PathBuf {
        if self.folder.is_empty() {
            here.at().to_path_buf()
        } else {
            thalyx_files::resolve(here.at(), &self.folder)
        }
    }
}

/// Split a line into its leading flags and the subject that follows them.
///
/// Returns `None` when the line was refused, having already said so in whichever
/// face asked — so the caller's `let … else` reads as "it was answered".
fn parse(rest: &str, face: Face, op: &str) -> Option<Asking> {
    let mut window = Asked::default();
    let mut folder = String::new();
    let given = crate::words::asked(face, op, rest)?;
    let mut remainder = given.as_slice();

    while let Some(head) = remainder.first() {
        // The last word of the line is still a candidate flag: `contenido
        // en=src` with nothing after it is a folder and an empty subject, which
        // is refused below by name rather than by searching for "".
        let word = head.as_str();
        match word.split_once('=') {
            Some(("limite" | "limit", count)) => match count.parse::<usize>() {
                Ok(limit) => window.limit = limit,
                // A mis-typed number is refused rather than falling through to
                // being part of the text. `limite=dos` as a search term would
                // find nothing and look like an answer.
                Err(_) => {
                    declined(face, op, "incomplete", &format!("`{word}` is not a number"));
                    return None;
                }
            },
            Some(("cursor" | "desde", token)) => match Cursor::parse(token) {
                Ok(cursor) => window.after = Some(cursor),
                Err(why) => {
                    declined(face, op, "bad_cursor", &why.to_string());
                    return None;
                }
            },
            Some(("en" | "in", named)) => folder = named.to_string(),
            _ => break,
        }
        remainder = &remainder[1..];
    }

    // Joined with single spaces, which is what several words mean as one
    // subject. `contenido "fn  main"` is how two spaces are asked for, the same
    // way they are asked for anywhere else.
    let subject = crate::words::phrase(remainder);
    if subject.is_empty() {
        declined(
            face,
            op,
            "nothing_asked",
            if op == "find" {
                "which name — `encontrar *.rs`"
            } else {
                "which text — `contenido fn main`"
            },
        );
        return None;
    }
    Some(Asking {
        subject,
        folder,
        window,
    })
}

/// An error the engine produced, in both faces and with its own word.
///
/// `machine::failure` and not `machine::declined`, and the difference is a
/// field: a `FileError` knows its own remedy and `declined` has nowhere to put
/// one. A caller told `not_a_directory` with no `remedy` has to work out what
/// to do next, which is exactly the discovery cost punto **A2** exists to pay
/// once. This was found by a test asking for the remedy and getting `null`.
fn refused(face: Face, op: &str, error: &FileError) -> Fallible {
    if face.is_machine() {
        face.say(thalyx_files::machine::failure(op, error));
    } else {
        println!("\n  {error}\n");
    }
    Ok(())
}

fn declined(face: Face, op: &str, word: &str, why: &str) {
    if face.is_machine() {
        face.say(thalyx_files::machine::declined(op, word, why));
    } else {
        println!("\n  {why}\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asking(line: &str) -> Option<Asking> {
        parse(line, Face::Human, "find")
    }

    #[test]
    fn a_flag_after_the_text_is_part_of_the_text_and_not_a_flag() {
        // The whole argument of this module's header, as an assertion. If this
        // ever passes the other way, searching for `en=produccion` silently
        // becomes a search of a folder.
        let asked = asking("TODO en=src").expect("a subject");
        assert_eq!(asked.subject, "TODO en=src");
        assert!(asked.folder.is_empty());
    }

    #[test]
    fn a_flag_before_the_text_is_a_flag_and_never_part_of_it() {
        let asked = asking("en=src limite=5 TODO en la casa").expect("a subject");
        assert_eq!(asked.subject, "TODO en la casa");
        assert_eq!(asked.folder, "src");
        assert_eq!(asked.window.limit, 5);
    }

    #[test]
    fn a_subject_of_several_words_stays_one_subject() {
        // The thing this has to keep doing: `contenido fn main` searches for
        // `fn main`. A parser that split on whitespace and took the first word
        // would search for "fn" and drop "main" without saying so.
        assert_eq!(asking("fn main").expect("a subject").subject, "fn main");
    }

    #[test]
    fn a_run_of_spaces_collapses_unless_it_is_quoted() {
        // Changed on 2026-08-23, when Cesar decided the line gets quoting: this
        // used to keep the run because the subject was the rest of the line
        // untouched, and there was no way to ask for it back. Now it collapses
        // the way it does in every terminal, and the quotes are how the other
        // thing is said — which is the same rule a whole shell language would
        // bring later, so nobody has to unlearn this one.
        assert_eq!(asking("fn  main").expect("a subject").subject, "fn main");
        assert_eq!(
            asking(r#""fn  main""#).expect("a subject").subject,
            "fn  main"
        );
    }

    #[test]
    fn a_line_with_only_flags_is_refused_rather_than_matching_everything() {
        assert!(asking("en=src").is_none());
        assert!(asking("").is_none());
    }

    #[test]
    fn a_limit_that_is_not_a_number_is_refused_rather_than_searched_for() {
        assert!(asking("limite=dos algo").is_none());
    }
}
