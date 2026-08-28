//! Turning one typed line into the words a verb acts on.
//!
//! Point 9 of the usable terminal, decided by Cesar on 2026-08-23: **quoting
//! now, a whole shell language later, and nothing learned now may have to be
//! unlearned then.** The decree is `vault/02-Arquitectura/Palabras.md`.
//!
//! The hole it closes is not a matter of taste. Before this, a file whose name
//! held a space could be listed and nothing else:
//!
//! ```text
//! cp mi archivo.txt copia.txt   ->  Two names: what to take, and where it goes.
//! rm mi archivo.txt             ->  .../mi is not there
//! ```
//!
//! Nothing was ever destroyed by it — all three verbs refused — but there was no
//! way to name the file at all.
//!
//! ## What it is not
//!
//! There are no pipes, no redirection, no variables and no substitution. This
//! splits a line into words and nothing else. What it does have to be is the
//! *same* splitting a shell would do, so that the day those arrive they are
//! added rather than swapped in.
//!
//! So the rules here are POSIX's, as far as POSIX goes today:
//!
//! - `'…'` is literal all through. A single quote cannot appear inside one.
//! - `"…"` is literal except `\"` and `\\`, which stand for the character after
//!   the backslash. Any other backslash inside double quotes stays a backslash —
//!   which is what POSIX says, and it is what leaves room for `$` and `` ` `` to
//!   mean something later without changing what anything means today.
//! - Outside quotes, `\` takes the next character literally.
//! - A line that ends inside a quote, or on a backslash, is **refused**. A shell
//!   would ask for another line; a Thalyx session has one line and a person who
//!   is owed an answer, and guessing which quote they meant to close is how a
//!   `rm` acts on something nobody named.
//!
//! ## Expansion stays in the verb, and that is a decree
//!
//! A shell expands `*.log` before the command runs. Thalyx does not and will not:
//! `encontrar *.rs` searches a whole tree for that pattern, and a line that
//! expanded it first would hand `encontrar` the names in one directory instead —
//! a different question, quietly. So the words come out of here **unexpanded**,
//! and the verb that knows what a pattern means to it does the matching.
//!
//! That is why a [`Word`] remembers which of its characters were quoted. `rm
//! "a*b"` has to remove one oddly-named file and `rm a*b` has to remove several,
//! and by the time the verb sees the text the quotes are gone. Keeping the mask
//! is what lets both of those keep meaning what they mean in every other
//! terminal.

/// One word of a line, and which of its characters were spelled out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    text: String,
    /// One entry per character of `text`: whether it arrived inside quotes or
    /// behind a backslash. Kept rather than a single "was quoted" flag, because
    /// `"a"*` is a pattern and `a"*"` is a name, and one flag per word answers
    /// both of those the same way.
    quoted: Vec<bool>,
}

impl Word {
    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Whether the verbs that match patterns should treat this as one.
    ///
    /// False for a word whose every `*` and `?` was quoted, which is the whole
    /// point of quoting them.
    pub fn is_pattern(&self) -> bool {
        self.text
            .chars()
            .zip(&self.quoted)
            .any(|(character, quoted)| !quoted && (character == '*' || character == '?'))
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Unclosed {
    #[error("the line ends inside a {0} quote, so it is not clear where the name stops")]
    Quote(char),

    #[error("the line ends on a backslash, which is waiting for a character that never came")]
    Backslash,
}

impl Unclosed {
    /// The word a program matches on.
    pub fn word(&self) -> &'static str {
        match self {
            Unclosed::Quote(_) => "unclosed_quote",
            Unclosed::Backslash => "trailing_backslash",
        }
    }

    /// `Superficie-para-el-LLM.md`, A2: an error names the way out.
    pub fn remedy(&self) -> &'static str {
        match self {
            Unclosed::Quote(_) => "close_the_quote",
            Unclosed::Backslash => "finish_the_escape",
        }
    }
}

/// Split a line into words, honouring quotes and backslashes.
pub fn words(line: &str) -> Result<Vec<Word>, Unclosed> {
    Ok(scan(line, false)?
        .into_iter()
        .map(|(word, _)| word)
        .collect())
}

/// The first word, and the rest of the line after it, byte for byte.
///
/// For `editar`, whose third part is **content**: the text going into a file
/// keeps its leading spaces, because a configuration file line that begins with
/// four of them means something with them and something else without. So the
/// name gets quoting and the text is taken from the line rather than from the
/// words. It is the one carve-out, and the decree names it.
pub fn first(line: &str) -> Result<Option<(Word, &str)>, Unclosed> {
    let mut scanned = scan(line, true)?;
    if scanned.is_empty() {
        return Ok(None);
    }
    let (word, ended) = scanned.remove(0);
    Ok(Some((word, line[ended..].trim_start())))
}

/// The scanner both of those are made of, so there is one set of rules.
///
/// Returns each word with the byte offset just past it, which is what makes
/// "the rest of the line, untouched" answerable at all.
fn scan(line: &str, stop_after_first: bool) -> Result<Vec<(Word, usize)>, Unclosed> {
    let mut found = Vec::new();
    let mut text = String::new();
    let mut quoted = Vec::new();
    // Separate from `text.is_empty()`, because `""` is a word: an empty name,
    // which a verb should refuse by name rather than never hear about.
    let mut started = false;
    let mut characters = line.char_indices();

    macro_rules! keep {
        ($character:expr, $was_quoted:expr) => {{
            text.push($character);
            quoted.push($was_quoted);
        }};
    }

    while let Some((at, character)) = characters.next() {
        match character {
            ' ' | '\t' => {
                if started {
                    found.push((
                        Word {
                            text: std::mem::take(&mut text),
                            quoted: std::mem::take(&mut quoted),
                        },
                        at,
                    ));
                    started = false;
                    if stop_after_first {
                        return Ok(found);
                    }
                }
            }
            '\\' => {
                started = true;
                match characters.next() {
                    Some((_, escaped)) => keep!(escaped, true),
                    None => return Err(Unclosed::Backslash),
                }
            }
            '\'' => {
                started = true;
                loop {
                    match characters.next() {
                        None => return Err(Unclosed::Quote('\'')),
                        Some((_, '\'')) => break,
                        Some((_, inside)) => keep!(inside, true),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match characters.next() {
                        None => return Err(Unclosed::Quote('"')),
                        Some((_, '"')) => break,
                        Some((_, '\\')) => match characters.next() {
                            None => return Err(Unclosed::Quote('"')),
                            Some((_, escaped @ ('"' | '\\'))) => keep!(escaped, true),
                            // POSIX keeps the backslash in every other case, and
                            // that is the room `$` and `` ` `` will need later.
                            Some((_, other)) => {
                                keep!('\\', true);
                                keep!(other, true);
                            }
                        },
                        Some((_, inside)) => keep!(inside, true),
                    }
                }
            }
            plain => {
                started = true;
                keep!(plain, false);
            }
        }
    }

    if started {
        found.push((Word { text, quoted }, line.len()));
    }
    Ok(found)
}

/// The words back as one phrase, single-spaced.
///
/// For the verbs whose subject is a sentence rather than a list of names —
/// `contenido fn main` looks for `fn main`. Unquoted runs of spaces collapse,
/// which is what every shell does and what `contenido "fn  main"` is for.
pub fn phrase(words: &[Word]) -> String {
    words.iter().map(Word::as_str).collect::<Vec<_>>().join(" ")
}

/// The words of a line, or the refusal said in whichever face is listening.
///
/// One place, so that an unclosed quote reads the same whichever verb was being
/// typed. A refusal that is worded differently by each verb is a refusal a
/// person has to learn several times.
/// `None` when it was refused, which the caller answers by returning: the
/// refusal has already been said, in the right face, and a verb that said
/// something else after it would be saying it twice.
pub fn asked(face: crate::files::Face, op: &str, line: &str) -> Option<Vec<Word>> {
    match words(line) {
        Ok(found) => Some(found),
        Err(why) => {
            if face.is_machine() {
                face.say(thalyx_files::machine::refused(
                    op,
                    why.word(),
                    why.remedy(),
                    &why.to_string(),
                ));
            } else {
                println!("\n  {why}\n");
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(line: &str) -> Vec<String> {
        words(line)
            .unwrap()
            .into_iter()
            .map(|word| word.as_str().to_string())
            .collect()
    }

    #[test]
    fn a_name_with_a_space_in_it_is_one_name_when_it_is_quoted() {
        // The whole reason this module exists. Before it, `cp mi archivo.txt x`
        // was three words and the verb refused, and there was no way to say it.
        assert_eq!(
            split(r#"cp "mi archivo.txt" copia"#),
            ["cp", "mi archivo.txt", "copia"]
        );
        assert_eq!(
            split("cp 'mi archivo.txt' copia"),
            ["cp", "mi archivo.txt", "copia"]
        );
        assert_eq!(
            split(r"cp mi\ archivo.txt copia"),
            ["cp", "mi archivo.txt", "copia"]
        );
    }

    #[test]
    fn a_line_with_no_quotes_in_it_splits_exactly_as_it_always_did() {
        // The control. Every line anybody has typed at this prompt until today
        // has no quotes in it, and none of them may change meaning.
        assert_eq!(split("rm a.log b.log"), ["rm", "a.log", "b.log"]);
        assert_eq!(split("  ls   /home  "), ["ls", "/home"]);
        assert_eq!(split(""), Vec::<String>::new());
        assert_eq!(split("rm *.log"), ["rm", "*.log"]);
    }

    #[test]
    fn quoting_a_star_makes_it_a_name_and_leaving_it_alone_makes_it_a_pattern() {
        // `rm "a*b"` has one oddly-named file to remove and `rm a*b` has several.
        // The quotes are gone by the time the verb sees the text, so the word has
        // to carry the answer itself.
        let pattern = &words("a*b").unwrap()[0];
        assert!(pattern.is_pattern());
        let name = &words(r#""a*b""#).unwrap()[0];
        assert!(!name.is_pattern());
        assert_eq!(name.as_str(), "a*b");
        // Per character and not per word: this is a pattern whose literal half
        // happens to be quoted, which is what every other terminal makes of it.
        let mixed = &words(r#""a"*"#).unwrap()[0];
        assert!(mixed.is_pattern());
        assert_eq!(mixed.as_str(), "a*");
    }

    #[test]
    fn a_quote_that_is_never_closed_is_refused_rather_than_guessed_at() {
        // A shell asks for another line. A session has one line and a person
        // waiting, and guessing where the name ends is how `rm` acts on
        // something nobody named.
        assert_eq!(words(r#"rm "a b"#).unwrap_err(), Unclosed::Quote('"'));
        assert_eq!(words("rm 'a b").unwrap_err(), Unclosed::Quote('\''));
        assert_eq!(words(r"rm a\").unwrap_err(), Unclosed::Backslash);
        assert_eq!(words(r#"rm "a\"#).unwrap_err(), Unclosed::Quote('"'));
    }

    #[test]
    fn inside_double_quotes_a_backslash_only_speaks_for_a_quote_or_itself() {
        // POSIX, and the room `$` and a backtick will need later: a backslash
        // that is not in front of something it can escape stays a backslash, so
        // adding meanings to those characters later changes nothing typed today.
        assert_eq!(split(r#""a\"b""#), [r#"a"b"#]);
        assert_eq!(split(r#""a\\b""#), [r"a\b"]);
        assert_eq!(split(r#""a\nb""#), [r"a\nb"]);
        // Single quotes have no escapes at all, so this is two characters.
        assert_eq!(split(r"'a\b'"), [r"a\b"]);
    }

    #[test]
    fn an_empty_pair_of_quotes_is_a_word_and_not_nothing() {
        // `rm ""` must reach the verb and be refused by name. Dropped here, it
        // would arrive as `rm` with no argument — a different error about a
        // different mistake.
        assert_eq!(split(r#"rm """#), ["rm", ""]);
        assert_eq!(words(r#""""#).unwrap().len(), 1);
    }

    #[test]
    fn the_first_word_is_quoted_and_everything_after_it_is_left_alone() {
        // `editar` writes what comes after into a file. A configuration line
        // that starts with four spaces means something with them and something
        // else without, so the text is taken from the line and not from the
        // words — while the name in front of it still gets to have a space.
        let (name, rest) = first(r#""mi archivo.txt" poner 3     sangrado"#)
            .unwrap()
            .expect("a first word");
        assert_eq!(name.as_str(), "mi archivo.txt");
        assert_eq!(rest, "poner 3     sangrado");

        let (name, rest) = first("nota.txt").unwrap().expect("a first word");
        assert_eq!(name.as_str(), "nota.txt");
        assert_eq!(rest, "");

        assert_eq!(first("   ").unwrap(), None);
        // The refusal has to happen here too, or `editar "a b` would edit a file
        // called `"a` and nobody would be told why.
        assert_eq!(first(r#""a b"#).unwrap_err(), Unclosed::Quote('"'));
    }

    #[test]
    fn a_phrase_is_the_words_back_with_one_space_between_them() {
        // `contenido fn main` looks for `fn main`. Runs of spaces collapse the
        // way they do in every terminal, and `contenido "fn  main"` is how the
        // other thing is said.
        assert_eq!(phrase(&words("fn   main").unwrap()), "fn main");
        assert_eq!(phrase(&words(r#""fn  main""#).unwrap()), "fn  main");
        assert_eq!(phrase(&words("").unwrap()), "");
    }
}
