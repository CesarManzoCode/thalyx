//! The mechanical parser.
//!
//! Input: source text. Output: the raw dependency references it declares.
//! Nothing else — no I/O, no filesystem, no knowledge of the graph.
//!
//! That narrowness is the point. The rest of the system consumes the graph and
//! does not know how it was produced, so this parser can start as line-oriented
//! pattern matching and become tree-sitter later **without touching the graph
//! contract**. It is deterministic input to output, which also makes it the
//! easiest piece in the system to test.
//!
//! What it deliberately does *not* do is decide which file a reference points
//! at. `import foo.bar` is a reference; whether it resolves to a file in this
//! tree, to a third-party package, or to nothing at all is a question about the
//! tree, not about the text. Resolution belongs to the graph.
//!
//! See `vault/03-Primitivas/Parser-Mecanico.md`.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A dependency reference exactly as it was written in the source.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Reference {
    /// The target as written: `std::fs`, `./utils`, `numpy`, `"config.h"`.
    pub target: String,
    /// 1-indexed line it appeared on, for auditing and error messages.
    pub line: usize,
    /// How it was written, which is what resolution needs to interpret it.
    pub kind: ReferenceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    /// Explicitly relative: `./utils`, `../lib/x`, `#include "local.h"`.
    /// Resolvable against the importing file's directory.
    Relative,
    /// Rooted at the project or a package: `crate::x`, `from app.models`.
    Rooted,
    /// Neither: `numpy`, `#include <stdio.h>`. Usually external.
    Opaque,
}

/// Languages the parser recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    C,
    Go,
}

impl Language {
    /// Guess the language from a file extension.
    ///
    /// Returns `None` for anything unrecognised, and the caller indexes the
    /// file as a node with no outgoing edges rather than guessing.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "rs" => Some(Language::Rust),
            "py" | "pyi" => Some(Language::Python),
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => Some(Language::JavaScript),
            "c" | "h" | "cc" | "cpp" | "hpp" | "cxx" => Some(Language::C),
            "go" => Some(Language::Go),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::C => "c",
            Language::Go => "go",
        }
    }
}

/// A name this file declares, as opposed to one it mentions.
///
/// `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **C2**: *`grep`
/// contesta con renglones porque no sabe qué es un símbolo*. This is the part
/// that knows. A definition is a fact about the text — that this line is where
/// the name comes from — and it is what makes "where is `login` defined" a
/// one-row answer instead of two hundred lines of matches.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub kind: SymbolKind,
    /// 1-indexed, so the answer can send somebody straight to it.
    pub line: usize,
    /// Whether another file could refer to this name at all.
    ///
    /// Each language's own rule and not a heuristic: Rust wants `pub`,
    /// JavaScript wants `export`, Go wants a capital letter, C is external
    /// unless `static`, and Python is everything that does not start with `_`.
    ///
    /// It exists because of what happened when a name became a dependency edge.
    /// `thalyx-snapshot` declares `fn place` and `fn relative` — both private,
    /// both ordinary words — and every file in the repository that writes
    /// `let relative = …` was reported as depending on it. Thirty-three
    /// dependents where about eight are real. A private name **cannot** be
    /// referred to from another file, so that is not a guess about the code, it
    /// is the language saying the edge is impossible.
    pub exported: bool,
}

/// What kind of thing a name is, at the coarseness five languages share.
///
/// Three and not fifteen, deliberately. A Rust `trait`, a Go `interface` and a
/// TypeScript `interface` are the same idea for the purpose of finding one, and
/// a caller that had to learn each language's vocabulary before it could ask a
/// question would be paying the discovery cost this whole catalogue exists to
/// lower. The language is a separate field, so nothing is lost by the caller
/// that does care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// Something callable: `fn`, `def`, `function`, `func`, a C function body.
    Function,
    /// Something that names a shape: `struct`, `enum`, `trait`, `class`,
    /// `interface`, `type`, `union`.
    Type,
    /// A value with a name: `const`, `static`, `#define`, a JavaScript `const`.
    Constant,
}

impl SymbolKind {
    /// The word a program matches on. Stable, never translated.
    pub fn word(self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Type => "type",
            SymbolKind::Constant => "constant",
        }
    }
}

/// Source with everything that is not code taken out of it.
///
/// One scan for the three entry points below, because they were three scans
/// that disagreed. Each did its own comment handling — a line *starting* with
/// `//` was a comment and nothing else was — and every gap that left showed up
/// as a wrong answer somewhere: a `#include` inside `/* … */` became a
/// dependency edge no execution follows; the word `definitions` inside a C
/// block comment became a use of `thalyx_parser::definitions`; a name inside a
/// Rust string that ran over two lines became a use of whatever declares it.
/// All three were found by indexing this repository and reading the rows.
///
/// It is a scrubber and not a lexer. It knows four things — line comments,
/// block comments, double-quoted strings that may span lines, and quotes that
/// close on their own line — and that is enough for the questions above,
/// because every one of them is *what to ignore* rather than what to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scrubbed {
    /// 1-indexed, so an answer can send somebody straight to the line.
    number: usize,
    /// Comments blanked, string literals left whole — `#include "local.h"` and
    /// `import "fmt"` write the reference *inside* the quotes, so a reference
    /// scan that got the stripped line would find nothing at all.
    code: String,
    /// The same line with the contents of strings blanked too, which is what a
    /// scan for identifiers needs: a log line containing the word `login` is
    /// not a use of `login`.
    bare: String,
}

/// Which sequences start a comment in a language.
///
/// Python is the one that has to be asked separately in both directions: `#`
/// begins a comment there and nowhere else — in C it begins the preprocessor,
/// which is half of what the C parser is for — and `//` is floor division there
/// and a comment everywhere else.
fn comment_markers(language: Language) -> (&'static str, bool) {
    match language {
        Language::Python => ("#", false),
        _ => ("//", true),
    }
}

fn scrub(language: Language, source: &str) -> Vec<Scrubbed> {
    let (line_comment, has_block_comments) = comment_markers(language);

    let mut out = Vec::new();
    // The two states that outlive a line. A string opened with a double quote
    // does — that is what a Rust `"…\` continuation and a `r#"…"#` both are —
    // and a block comment does. Nothing else may: a Rust lifetime is written
    // `&'a str`, so a single quote left open at the end of a line is a lifetime
    // far more often than a string, and carrying it would blank the rest of the
    // file. That is the difference between a few rows too many and a file the
    // index cannot see, so the two are not treated alike.
    let mut in_string = false;
    let mut in_block = false;

    for (index, raw) in source.lines().enumerate() {
        let characters: Vec<char> = raw.chars().collect();
        let mut code = String::with_capacity(raw.len());
        let mut bare = String::with_capacity(raw.len());
        let mut in_char = false;
        let mut escaped = false;
        let mut at = 0;

        while at < characters.len() {
            let character = characters[at];
            let next = characters.get(at + 1).copied();

            if in_block {
                if character == '*' && next == Some('/') {
                    in_block = false;
                    at += 2;
                    continue;
                }
                at += 1;
                continue;
            }

            if in_string {
                code.push(character);
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    in_string = false;
                }
                at += 1;
                continue;
            }

            if in_char {
                code.push(character);
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '\'' {
                    in_char = false;
                }
                at += 1;
                continue;
            }

            // A comment marker, checked before anything else can consume it.
            if raw_starts_with(&characters, at, line_comment) {
                break;
            }
            if has_block_comments && character == '/' && next == Some('*') {
                in_block = true;
                at += 2;
                continue;
            }

            if character == '"' {
                in_string = true;
                code.push(character);
                bare.push(' ');
                at += 1;
                continue;
            }
            if character == '\'' {
                in_char = true;
                code.push(character);
                bare.push(' ');
                at += 1;
                continue;
            }

            code.push(character);
            bare.push(character);
            at += 1;
        }

        // A quote that never closed on its line was a lifetime, an apostrophe
        // in prose, or a typo — never a string that continues. See above.
        let _ = in_char;

        out.push(Scrubbed {
            number: index + 1,
            code,
            bare,
        });
    }

    out
}

fn raw_starts_with(characters: &[char], at: usize, needle: &str) -> bool {
    needle
        .chars()
        .enumerate()
        .all(|(offset, wanted)| characters.get(at + offset) == Some(&wanted))
}

/// Extract every dependency reference from a source file.
pub fn parse(language: Language, source: &str) -> Vec<Reference> {
    let mut references = Vec::new();

    for line in scrub(language, source) {
        // Commented-out imports are gone before this point rather than skipped
        // here. A dependency the code cannot reach would put an edge in the
        // graph that no execution follows.
        let text = line.code.trim();
        if text.is_empty() {
            continue;
        }
        let number = line.number;

        match language {
            Language::Rust => parse_rust(text, number, &mut references),
            Language::Python => parse_python(text, number, &mut references),
            Language::JavaScript => parse_javascript(text, number, &mut references),
            Language::C => parse_c(text, number, &mut references),
            Language::Go => parse_go(text, number, &mut references),
        }
    }

    references.sort();
    references.dedup();
    references
}

/// Every name this file declares.
///
/// Line-oriented like [`parse`], and for the same reason: it is deterministic,
/// it is testable against real files, and the contract does not change when this
/// becomes tree-sitter. What it costs is that a definition split across lines —
/// a Rust `fn` whose arguments start on the next line — is still found, because
/// the name is on the first line, while one written in a way this does not
/// recognise is **missing rather than wrong**. That asymmetry is deliberate:
/// `Estrategia-de-Pruebas` rule 9 says the cautious answer, and a name reported
/// in the wrong place would send somebody to edit the wrong line.
pub fn definitions(language: Language, source: &str) -> Vec<Definition> {
    let mut found = Vec::new();

    for line in scrub(language, source) {
        let text = line.code.trim();
        if text.is_empty() {
            continue;
        }
        let number = line.number;
        match language {
            Language::Rust => rust_definition(text, number, &mut found),
            Language::Python => python_definition(text, number, &mut found),
            Language::JavaScript => javascript_definition(text, number, &mut found),
            Language::C => c_definition(text, number, &mut found),
            Language::Go => go_definition(text, number, &mut found),
        }
    }

    found.sort();
    found.dedup();
    found
}

/// Whether the file's brackets close, and where the first one that does not is.
///
/// `None` means every `(`, `[` and `{` that is really a bracket has a partner
/// of the right kind in the right order, and every string and comment closes.
///
/// ## What this is, said exactly, because it would be easy to oversell
///
/// It is **not a compiler and not a parser.** It cannot tell you that a type is
/// wrong, that a name does not exist, or that a `match` is missing an arm. What
/// it can tell you is the failure a mechanical edit actually produces: a
/// substitution that ate a brace, a replacement pasted one line too high, a
/// deletion that took a closing paren with it. Those turn a file into something
/// no compiler will accept, and they are exactly what an agent rewriting text
/// by pattern does when a pattern is slightly wrong.
///
/// ## Why it has its own scan, when this file's whole argument is that it should not
///
/// [`scrub`] exists because three scans disagreed, and the rule since is one
/// scan. This is the documented exception, and the exception was found by
/// pointing this function at **this file** — rule 6, and it took one run.
///
/// The scrubber answers *what to ignore*, and for that it is allowed to be
/// generous: meeting a lone `'` it blanks the rest of the line, because in Rust
/// a lone `'` is a lifetime far more often than a string. That is right for
/// counting identifiers and fatal for counting brackets — `pub fn name(self) ->
/// &'static str {` arrives with its `{` blanked away, so the parser's own
/// source reported as unbalanced at the first method it declares.
///
/// Balance needs *what to keep*, which is a different question and needs a
/// lexer rather than a scrubber. So this one knows the constructs a bracket can
/// hide in: line and block comments (nested, as Rust's are), double-quoted
/// strings with escapes, raw strings, backtick strings, triple-quoted strings,
/// and — the one that forced this — the difference between a Rust character
/// literal and a lifetime.
///
/// What it does not model is interpolation inside a template literal: the
/// contents of a JavaScript backtick string are skipped whole, `${…}` included,
/// so a bracket that is unbalanced *inside* an interpolation is not found. Said
/// here rather than discovered.
pub fn unbalanced(language: Language, source: &str) -> Option<String> {
    let characters: Vec<char> = source.chars().collect();
    let (line_comment, has_block_comments) = comment_markers(language);
    let has_raw_strings = matches!(language, Language::Rust);
    let has_backticks = matches!(language, Language::Go | Language::JavaScript);
    let has_triple_quotes = matches!(language, Language::Python);
    let single_quotes_are_strings = matches!(language, Language::Python | Language::JavaScript);

    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut at = 0usize;
    let mut line = 1usize;

    while at < characters.len() {
        let character = characters[at];
        if character == '\n' {
            line += 1;
            at += 1;
            continue;
        }

        if marker_at(&characters, at, line_comment) {
            while at < characters.len() && characters[at] != '\n' {
                at += 1;
            }
            continue;
        }

        if has_block_comments && marker_at(&characters, at, "/*") {
            // Counted rather than scanned for the first `*/`, because Rust's
            // block comments nest and a scan for the first one would leave the
            // rest of an outer comment being read as code.
            let opened_at = line;
            let mut depth = 0usize;
            while at < characters.len() {
                if marker_at(&characters, at, "/*") {
                    depth += 1;
                    at += 2;
                    continue;
                }
                if marker_at(&characters, at, "*/") {
                    depth -= 1;
                    at += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                if characters[at] == '\n' {
                    line += 1;
                }
                at += 1;
            }
            if depth > 0 {
                return Some(format!("line {opened_at}: a block comment is never closed"));
            }
            continue;
        }

        // A raw string: `r"…"`, `r#"…"#`, `br##"…"##`. The hashes decide where
        // it ends, which is the whole reason a raw string exists.
        if has_raw_strings
            && (character == 'r' || character == 'b')
            && !at
                .checked_sub(1)
                .and_then(|before| characters.get(before))
                .is_some_and(|before| before.is_alphanumeric() || *before == '_')
            && let Some((after, hashes)) = raw_string_opens(&characters, at)
        {
            let opened_at = line;
            let closing: String = std::iter::once('"')
                .chain(std::iter::repeat_n('#', hashes))
                .collect();
            let mut scan = after;
            while scan < characters.len() && !marker_at(&characters, scan, &closing) {
                if characters[scan] == '\n' {
                    line += 1;
                }
                scan += 1;
            }
            if scan >= characters.len() {
                return Some(format!("line {opened_at}: a raw string is never closed"));
            }
            at = scan + closing.chars().count();
            continue;
        }

        if has_triple_quotes && let Some(triple) = triple_quote_at(&characters, at) {
            let opened_at = line;
            let mut scan = at + 3;
            while scan < characters.len() && !marker_at(&characters, scan, triple) {
                if characters[scan] == '\n' {
                    line += 1;
                }
                scan += 1;
            }
            if scan >= characters.len() {
                return Some(format!(
                    "line {opened_at}: a triple-quoted string is never closed"
                ));
            }
            at = scan + 3;
            continue;
        }

        if character == '"'
            || (has_backticks && character == '`')
            || (single_quotes_are_strings && character == '\'')
        {
            let opened_at = line;
            let mut scan = at + 1;
            loop {
                match characters.get(scan) {
                    None => return Some(format!("line {opened_at}: a string is never closed")),
                    Some('\\') => scan += 2,
                    Some(&found) if found == character => break,
                    Some('\n') => {
                        line += 1;
                        scan += 1;
                    }
                    Some(_) => scan += 1,
                }
            }
            at = scan + 1;
            continue;
        }

        // **The construct that forced this function to have its own scan.**
        // `'a'` is a character and `'a` is a lifetime, and telling them apart
        // is one lookahead: a literal closes on its own quote, and after an
        // escape it closes a few characters later. A lifetime is a name and a
        // name holds no brackets, so a quote that opens one is simply stepped
        // over — where blanking the rest of the line was the defect.
        if character == '\'' {
            at = match character_literal_ends(&characters, at) {
                Some(end) => end + 1,
                None => at + 1,
            };
            continue;
        }

        match character {
            '(' | '[' | '{' => stack.push((character, line)),
            ')' | ']' | '}' => {
                let wanted = match character {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                match stack.pop() {
                    Some((opened, _)) if opened == wanted => {}
                    Some((opened, opened_at)) => {
                        return Some(format!(
                            "line {line}: `{character}` closes a `{opened}` opened on line \
                             {opened_at}"
                        ));
                    }
                    None => {
                        return Some(format!(
                            "line {line}: `{character}` closes something that was never opened"
                        ));
                    }
                }
            }
            _ => {}
        }
        at += 1;
    }

    stack
        .pop()
        .map(|(opened, at)| format!("line {at}: `{opened}` is never closed"))
}

/// Whether the characters at `at` spell this marker.
fn marker_at(characters: &[char], at: usize, needle: &str) -> bool {
    needle
        .chars()
        .enumerate()
        .all(|(offset, wanted)| characters.get(at + offset) == Some(&wanted))
}

/// Where a raw string's body starts and how many hashes close it.
fn raw_string_opens(characters: &[char], at: usize) -> Option<(usize, usize)> {
    let mut scan = at + 1;
    if characters.get(at) == Some(&'b') {
        // `b"…"` is a byte string and not a raw one; only `br…` continues here.
        if characters.get(scan) != Some(&'r') {
            return None;
        }
        scan += 1;
    }
    let mut hashes = 0usize;
    while characters.get(scan) == Some(&'#') {
        hashes += 1;
        scan += 1;
    }
    (characters.get(scan) == Some(&'"')).then_some((scan + 1, hashes))
}

/// The three quotes that open a triple-quoted string, if they are there.
fn triple_quote_at(characters: &[char], at: usize) -> Option<&'static str> {
    ["\"\"\"", "'''"]
        .into_iter()
        .find(|triple| marker_at(characters, at, triple))
}

/// Where a character literal's closing quote is, or `None` for a lifetime.
///
/// The whole of the distinction, in one place so that both the reasoning and
/// the test that exercises it have somewhere to point.
fn character_literal_ends(characters: &[char], at: usize) -> Option<usize> {
    match characters.get(at + 1) {
        // `'\n'`, `'\\'`, `'\u{1F600}'` — the escape decides nothing about the
        // length, so the closing quote is looked for rather than counted to.
        Some('\\') => characters
            .iter()
            .skip(at + 2)
            .take(12)
            .position(|&found| found == '\'')
            .map(|offset| at + 2 + offset),
        // `'a'`. Anything else with a quote two along is a one-character
        // literal; anything else at all is a lifetime.
        Some(_) if characters.get(at + 2) == Some(&'\'') => Some(at + 2),
        _ => None,
    }
}

/// Every identifier the file mentions, outside comments and outside strings.
///
/// This is the other half of C2 — *«llamada desde estos tres sitios»* — and the
/// reason it is here rather than done with `grep` at the far end is the one the
/// decree names: `grep` cannot tell a call from the same word inside a comment,
/// and a caller that has to filter those itself is paying the ambiguity cost the
/// system was supposed to absorb.
///
/// String contents are dropped as well as comments. A log line that happens to
/// contain the word `login` is not a use of `login`, and an answer that counted
/// it would be confidently wrong in a way a caller cannot check without opening
/// the file — which is exactly the trip this saves.
pub fn identifiers(language: Language, source: &str) -> Vec<(String, usize)> {
    identifiers_from(&scrub(language, source))
}

fn identifiers_from(scrubbed: &[Scrubbed]) -> Vec<(String, usize)> {
    let mut found = Vec::new();

    for line in scrubbed {
        let mut word = String::new();
        for character in line.bare.chars() {
            if character.is_alphanumeric() || character == '_' {
                word.push(character);
                continue;
            }
            if !word.is_empty() {
                found.push((std::mem::take(&mut word), line.number));
            }
        }
        if !word.is_empty() {
            found.push((word, line.number));
        }
    }

    found
}

/// The names this file introduces itself: locals, parameters, fields.
///
/// Not definitions — those are [`definitions`] — but the other thing a name in a
/// file can be: a thing this file just made up. `let directory = …`,
/// `for entry in …`, `fn handle(server: &Server)`, `pub subvolume: [u8; 16]`.
///
/// ## Why the index needs this and `grep` never did
///
/// Because of what happens when a mention becomes a dependency edge. Asked what
/// depends on `thalyx-snapshot/src/lib.rs`, the index answered with forty-one
/// files. That crate has `pub fn directory(&self)` and `pub fn subvolume(&self)`
/// — both perfectly public, both methods on a type, and both ordinary English
/// words. Every file in the repository holding `for directory in …` or a struct
/// field called `subvolume` was reported as a dependent.
///
/// A file that binds a name is talking about its own binding. That is not a
/// guess about which one it meant: the binding is right there in the same file,
/// and it shadows anything outside. So the rule only ever *removes* an edge,
/// and what it costs is the file that binds `difference` and also calls the
/// `difference` from somewhere else — a row lost, which is the direction rule 9
/// says to fail in.
///
/// It is deliberately not scope-aware. Scope is a compiler; a set per file is a
/// scan, and the difference between them is a row here and there in exchange
/// for a mechanism that can be read in one sitting.
pub fn bound_names(language: Language, source: &str) -> std::collections::HashSet<String> {
    bound_from(language, &scrub(language, source))
}

/// Both halves of what the index needs from a file, over one scan of it.
///
/// The index asks for the mentions and the bindings together, always, and
/// asking twice means scrubbing every file twice — 533.7 ms against 480.4 ms on
/// this repository, best of seven. The two public
/// functions stay, because each is a separate question and each has its own
/// tests; this is the one call that has both questions at once.
pub fn identifiers_and_bindings(
    language: Language,
    source: &str,
) -> (Vec<(String, usize)>, std::collections::HashSet<String>) {
    let scrubbed = scrub(language, source);
    (identifiers_from(&scrubbed), bound_from(language, &scrubbed))
}

fn bound_from(language: Language, scrubbed: &[Scrubbed]) -> std::collections::HashSet<String> {
    let mut bound = std::collections::HashSet::new();

    for line in scrubbed {
        let characters: Vec<char> = line.bare.chars().collect();
        let words = words_with_positions(&characters);

        for (index, (word, start, end)) in words.iter().enumerate() {
            // `name:` — a parameter, a struct field, a struct-literal key, a
            // type annotation. Never `name::`, which is a path and the most
            // reference-like thing there is.
            if characters.get(*end) == Some(&':') && characters.get(end + 1) != Some(&':') {
                bound.insert(word.clone());
                continue;
            }

            // `x := y`, which is Go's whole binding syntax.
            if characters.get(*end) == Some(&' ')
                && characters.get(end + 1) == Some(&':')
                && characters.get(end + 2) == Some(&'=')
            {
                bound.insert(word.clone());
                continue;
            }

            let previous = index.checked_sub(1).map(|before| words[before].0.as_str());
            match previous {
                // Everything between `let`/`var`/`const` and the `=` is a
                // pattern: `let mut x`, `let (a, b)`, `let Thing { one, two }`.
                Some("let" | "var" | "mut" | "ref") => {
                    if before_the_equals(&characters, *start) {
                        bound.insert(word.clone());
                    }
                }
                // `for x in …`, in Rust, Python and JavaScript alike.
                Some("for") => {
                    bound.insert(word.clone());
                }
                // `mod restore;` puts the name `restore` in this file's own
                // namespace, and `use … as Keys` does the same for `Keys`.
                // Both are the file naming something for itself, which is what
                // every other case here is.
                Some("mod" | "as") => {
                    bound.insert(word.clone());
                }
                _ => {}
            }
        }

        // Python binds by assignment and has no keyword to look for. Only at
        // the start of a statement, so `self.total = x` binds nothing and
        // `total = x` binds `total`.
        if language == Language::Python
            && let Some((first, _, end)) = words.first()
            && let Some(rest) = line.bare.get(*end..)
            && rest.trim_start().starts_with('=')
            && !rest.trim_start().starts_with("==")
        {
            bound.insert(first.clone());
        }
    }

    bound
}

/// Every identifier on a line, with where it starts and ends.
fn words_with_positions(characters: &[char]) -> Vec<(String, usize, usize)> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut start = 0;
    for (at, character) in characters.iter().enumerate() {
        if character.is_alphanumeric() || *character == '_' {
            if word.is_empty() {
                start = at;
            }
            word.push(*character);
            continue;
        }
        if !word.is_empty() {
            words.push((std::mem::take(&mut word), start, at));
        }
    }
    if !word.is_empty() {
        words.push((word, start, characters.len()));
    }
    words
}

/// Whether this position is still on the left of an `=`, which is what tells a
/// binding pattern from the expression it is bound to.
fn before_the_equals(characters: &[char], at: usize) -> bool {
    characters[..at]
        .iter()
        .rev()
        .take_while(|c| **c != ';')
        .all(|c| *c != '=')
}

/// The name that follows a keyword, up to whatever ends it.
fn name_after<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    let name: &str = rest
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|piece| !piece.is_empty())?;
    // A name has to start where the keyword ended, or `fn` would match `fnord`
    // and `type` would match `typedef`.
    if !rest.starts_with(name) {
        return None;
    }
    Some(name)
}

/// Rust visibility, stripped so `pub fn` and `fn` are one case.
fn without_visibility(line: &str) -> &str {
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix("pub") {
        let rest = rest.trim_start();
        // `pub(crate)`, `pub(super)`, `pub(in path)`.
        if let Some(rest) = rest.strip_prefix('(') {
            if let Some((_, after)) = rest.split_once(')') {
                return after.trim_start();
            }
            return line;
        }
        return rest;
    }
    line
}

fn rust_definition(line: &str, number: usize, out: &mut Vec<Definition>) {
    // Asked before the prefix is stripped, because stripping is how the
    // question stops being answerable.
    let exported = is_rust_public(line);
    let mut line = without_visibility(line);
    // `const fn` is a function and not a constant, and it is the one prefix
    // where the order matters: stripped in the wrong order it becomes a
    // constant named `fn`.
    if line.starts_with("const fn ") || line.starts_with("const unsafe fn ") {
        line = line.strip_prefix("const ").unwrap_or(line);
    }
    for prefix in ["default ", "async ", "unsafe ", "extern \"C\" ", "extern "] {
        line = line.strip_prefix(prefix).unwrap_or(line);
    }
    let line = line;

    let cases: [(&str, SymbolKind); 8] = [
        ("fn ", SymbolKind::Function),
        ("struct ", SymbolKind::Type),
        ("enum ", SymbolKind::Type),
        ("trait ", SymbolKind::Type),
        ("union ", SymbolKind::Type),
        ("type ", SymbolKind::Type),
        ("const ", SymbolKind::Constant),
        ("static ", SymbolKind::Constant),
    ];

    for (keyword, kind) in cases {
        if let Some(name) = name_after(line, keyword) {
            out.push(Definition {
                name: name.to_string(),
                kind,
                line: number,
                exported,
            });
            return;
        }
    }

    if let Some(name) = name_after(line, "macro_rules! ") {
        out.push(Definition {
            name: name.to_string(),
            kind: SymbolKind::Function,
            line: number,
            // A macro carries no `pub`, and `#[macro_export]` is on the line
            // above. Counted as reachable rather than as private, because the
            // opposite would make every macro in a codebase invisible to the
            // index — a much larger hole than the private macros it would shut.
            exported: true,
        });
    }
}

/// Whether a Rust item is visible outside the file it is written in.
///
/// `pub(self)` is spelled like the others and means private, which is the one
/// way this reads backwards if it only looks for the three letters.
fn is_rust_public(line: &str) -> bool {
    let line = line.trim_start();
    let Some(rest) = line.strip_prefix("pub") else {
        return false;
    };
    // `pub` has to end where it ends, or `public_thing` is a visibility.
    if rest
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
    {
        return false;
    }
    let rest = rest.trim_start();
    if let Some(scope) = rest.strip_prefix('(')
        && let Some((named, _)) = scope.split_once(')')
    {
        return named.trim() != "self";
    }
    true
}

fn python_definition(line: &str, number: usize, out: &mut Vec<Definition>) {
    let line = line.strip_prefix("async ").unwrap_or(line);
    let cases: [(&str, SymbolKind); 2] =
        [("def ", SymbolKind::Function), ("class ", SymbolKind::Type)];
    for (keyword, kind) in cases {
        if let Some(name) = name_after(line, keyword) {
            out.push(Definition {
                name: name.to_string(),
                kind,
                // Python has no visibility, so the language's rule is the
                // convention it enforces nowhere and everybody follows: a
                // leading underscore says do not import this.
                exported: !name.starts_with('_'),
                line: number,
            });
            return;
        }
    }
}

fn javascript_definition(line: &str, number: usize, out: &mut Vec<Definition>) {
    // Asked before the word is stripped, for the same reason as Rust's `pub`.
    let exported = line.starts_with("export ") || line.starts_with("export default ");
    let line = line.strip_prefix("export default ").unwrap_or(line);
    let line = line.strip_prefix("export ").unwrap_or(line);
    let line = line.strip_prefix("async ").unwrap_or(line);

    let cases: [(&str, SymbolKind); 6] = [
        ("function ", SymbolKind::Function),
        ("class ", SymbolKind::Type),
        ("interface ", SymbolKind::Type),
        ("type ", SymbolKind::Type),
        ("const ", SymbolKind::Constant),
        ("let ", SymbolKind::Constant),
    ];

    for (keyword, kind) in cases {
        if let Some(name) = name_after(line, keyword) {
            // A `const` that is not assigned is a declaration of nothing; a
            // `const` inside a call argument list is not a top-level name. The
            // `=` is what separates the two, cheaply.
            if kind == SymbolKind::Constant && !line.contains('=') {
                return;
            }
            out.push(Definition {
                name: name.to_string(),
                kind,
                line: number,
                exported,
            });
            return;
        }
    }
}

/// Words that begin a C statement and are never a function being defined.
///
/// Without this, `if (ready) {` is read as a function called `if`, and the index
/// fills with control flow — which does not merely add rows, it makes the answer
/// to "where is this defined" useless by burying it.
const C_NOT_A_DEFINITION: &[&str] = &[
    "if", "for", "while", "switch", "return", "else", "do", "case", "sizeof",
];

fn c_definition(line: &str, number: usize, out: &mut Vec<Definition>) {
    // C's own rule, and the only one it has: `static` at file scope means this
    // name does not leave the translation unit. Everything else does.
    let exported = !line.starts_with("static ");
    if let Some(name) = name_after(line, "#define ") {
        out.push(Definition {
            name: name.to_string(),
            kind: SymbolKind::Constant,
            line: number,
            // A `#define` in a header is exactly what another file includes.
            exported: true,
        });
        return;
    }

    for keyword in ["struct ", "union ", "enum "] {
        if let Some(name) = name_after(line, keyword) {
            // `struct foo bar;` is a variable, not a definition of `foo`. The
            // brace is what says the shape is being given here.
            if line.ends_with('{') {
                out.push(Definition {
                    name: name.to_string(),
                    kind: SymbolKind::Type,
                    line: number,
                    exported,
                });
            }
            return;
        }
    }

    // A function body: `something name(args) {`. Only with the brace, so a
    // prototype in a header is not reported as the place the code lives.
    if !line.ends_with('{') {
        return;
    }
    let Some(open) = line.find('(') else { return };
    let before = line[..open].trim_end();
    let name: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();

    if name.is_empty()
        || name.chars().next().is_some_and(|c| c.is_ascii_digit())
        || C_NOT_A_DEFINITION.contains(&name.as_str())
        // Something has to precede the name, or this is a call statement and
        // not a definition — `setup() {` is not C, `void setup() {` is.
        || before.len() == name.len()
    {
        return;
    }

    out.push(Definition {
        name,
        kind: SymbolKind::Function,
        line: number,
        exported,
    });
}

fn go_definition(line: &str, number: usize, out: &mut Vec<Definition>) {
    if let Some(rest) = line.strip_prefix("func ") {
        // A method: `func (r *Thing) Name(`. The receiver is not the name.
        let rest = match rest.strip_prefix('(') {
            Some(after) => match after.split_once(')') {
                Some((_, tail)) => tail.trim_start(),
                None => return,
            },
            None => rest,
        };
        if let Some(name) = name_after(rest, "") {
            out.push(Definition {
                // Go is the one language where visibility is spelled in the
                // name itself, so there is nothing to guess at all.
                exported: is_go_exported(name),
                name: name.to_string(),
                kind: SymbolKind::Function,
                line: number,
            });
        }
        return;
    }

    let cases: [(&str, SymbolKind); 3] = [
        ("type ", SymbolKind::Type),
        ("const ", SymbolKind::Constant),
        ("var ", SymbolKind::Constant),
    ];
    for (keyword, kind) in cases {
        if let Some(name) = name_after(line, keyword) {
            out.push(Definition {
                exported: is_go_exported(name),
                name: name.to_string(),
                kind,
                line: number,
            });
            return;
        }
    }
}

/// Go's whole visibility system: a name that starts with a capital leaves the
/// package, and one that does not, does not.
fn is_go_exported(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

fn parse_rust(line: &str, number: usize, out: &mut Vec<Reference>) {
    let rest = line
        .strip_prefix("use ")
        .or_else(|| line.strip_prefix("pub use "))
        .or_else(|| line.strip_prefix("mod "))
        .or_else(|| line.strip_prefix("pub mod "));

    let Some(rest) = rest else { return };

    // Take the path up to the first delimiter: `use a::b::{c, d};` yields `a::b`.
    let path: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
        .collect();
    let path = path.trim_end_matches(':').to_string();

    if path.is_empty() {
        return;
    }

    let kind = match path.split("::").next() {
        Some("crate") | Some("self") | Some("super") => ReferenceKind::Relative,
        _ => ReferenceKind::Rooted,
    };

    out.push(Reference {
        target: path,
        line: number,
        kind,
    });
}

fn parse_python(line: &str, number: usize, out: &mut Vec<Reference>) {
    if let Some(rest) = line.strip_prefix("from ") {
        let target: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
        if !target.is_empty() {
            out.push(Reference {
                kind: if target.starts_with('.') {
                    ReferenceKind::Relative
                } else {
                    ReferenceKind::Rooted
                },
                target,
                line: number,
            });
        }
        return;
    }

    if let Some(rest) = line.strip_prefix("import ") {
        // `import a, b` declares two dependencies, not one named "a, b".
        for part in rest.split(',') {
            let target: String = part
                .trim()
                .chars()
                .take_while(|c| !c.is_whitespace())
                .collect();
            if !target.is_empty() {
                out.push(Reference {
                    kind: if target.starts_with('.') {
                        ReferenceKind::Relative
                    } else {
                        ReferenceKind::Rooted
                    },
                    target,
                    line: number,
                });
            }
        }
    }
}

fn parse_javascript(line: &str, number: usize, out: &mut Vec<Reference>) {
    // Covers `import x from "y"`, `import "y"`, `export … from "y"` and
    // `require("y")` by looking for the quoted specifier after a keyword.
    let has_keyword =
        line.starts_with("import ") || line.starts_with("export ") || line.contains("require(");
    if !has_keyword {
        return;
    }

    for quote in ['"', '\''] {
        if let Some(start) = line.find(quote)
            && let Some(length) = line[start + 1..].find(quote)
        {
            let target = line[start + 1..start + 1 + length].to_string();
            if target.is_empty() {
                continue;
            }
            out.push(Reference {
                kind: if target.starts_with('.') {
                    ReferenceKind::Relative
                } else {
                    ReferenceKind::Opaque
                },
                target,
                line: number,
            });
            return;
        }
    }
}

fn parse_c(line: &str, number: usize, out: &mut Vec<Reference>) {
    let Some(rest) = line.strip_prefix("#include") else {
        return;
    };
    let rest = rest.trim();

    // `"local.h"` is relative to the including file; `<system.h>` is not.
    let (open, close, kind) = if rest.starts_with('"') {
        ('"', '"', ReferenceKind::Relative)
    } else if rest.starts_with('<') {
        ('<', '>', ReferenceKind::Opaque)
    } else {
        return;
    };

    let Some(start) = rest.find(open) else { return };
    let Some(length) = rest[start + 1..].find(close) else {
        return;
    };
    let target = rest[start + 1..start + 1 + length].to_string();

    if !target.is_empty() {
        out.push(Reference {
            target,
            line: number,
            kind,
        });
    }
}

fn parse_go(line: &str, number: usize, out: &mut Vec<Reference>) {
    // Both `import "fmt"` and the entries inside an import block.
    let candidate = line.strip_prefix("import ").unwrap_or(line);
    let candidate = candidate.trim();

    if !candidate.starts_with('"') && !candidate.contains(" \"") {
        return;
    }

    let Some(start) = candidate.find('"') else {
        return;
    };
    let Some(length) = candidate[start + 1..].find('"') else {
        return;
    };
    let target = candidate[start + 1..start + 1 + length].to_string();

    if !target.is_empty() {
        out.push(Reference {
            kind: if target.starts_with('.') {
                ReferenceKind::Relative
            } else {
                ReferenceKind::Opaque
            },
            target,
            line: number,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(language: Language, source: &str) -> Vec<String> {
        parse(language, source)
            .into_iter()
            .map(|r| r.target)
            .collect()
    }

    #[test]
    fn parses_rust_imports() {
        let source = r#"
use std::fs;
use crate::store::Store;
pub use serde::Serialize;
use thalyx_core::{install, remove};
mod harness;
"#;
        let found = targets(Language::Rust, source);
        assert!(found.contains(&"std::fs".to_string()));
        assert!(found.contains(&"crate::store::Store".to_string()));
        assert!(found.contains(&"serde::Serialize".to_string()));
        assert!(found.contains(&"thalyx_core".to_string()));
        assert!(found.contains(&"harness".to_string()));
    }

    #[test]
    fn marks_rust_crate_relative_paths_as_relative() {
        let parsed = parse(Language::Rust, "use crate::store::Store;");
        assert_eq!(parsed[0].kind, ReferenceKind::Relative);

        let parsed = parse(Language::Rust, "use serde::Serialize;");
        assert_eq!(parsed[0].kind, ReferenceKind::Rooted);
    }

    #[test]
    fn parses_python_imports_including_multiple_on_one_line() {
        let found = targets(
            Language::Python,
            "import os, sys\nfrom .models import User\nfrom app.db import session\n",
        );
        assert!(found.contains(&"os".to_string()));
        assert!(found.contains(&"sys".to_string()));
        assert!(found.contains(&".models".to_string()));
        assert!(found.contains(&"app.db".to_string()));
    }

    #[test]
    fn parses_javascript_imports_and_requires() {
        let found = targets(
            Language::JavaScript,
            "import React from 'react';\nconst x = require(\"./utils\");\nexport { a } from './a';\n",
        );
        assert!(found.contains(&"react".to_string()));
        assert!(found.contains(&"./utils".to_string()));
        assert!(found.contains(&"./a".to_string()));
    }

    #[test]
    fn distinguishes_c_local_includes_from_system_ones() {
        let parsed = parse(Language::C, "#include \"local.h\"\n#include <stdio.h>\n");
        let local = parsed.iter().find(|r| r.target == "local.h").unwrap();
        let system = parsed.iter().find(|r| r.target == "stdio.h").unwrap();
        assert_eq!(local.kind, ReferenceKind::Relative);
        assert_eq!(system.kind, ReferenceKind::Opaque);
    }

    #[test]
    fn parses_go_imports() {
        let found = targets(
            Language::Go,
            "import \"fmt\"\n\nimport (\n\t\"os\"\n\t\"github.com/x/y\"\n)\n",
        );
        assert!(found.contains(&"fmt".to_string()));
        assert!(found.contains(&"os".to_string()));
        assert!(found.contains(&"github.com/x/y".to_string()));
    }

    #[test]
    fn commented_out_imports_are_not_dependencies() {
        // An edge the graph can never follow is worse than a missing one: it
        // makes the graph confidently wrong rather than incomplete.
        assert!(targets(Language::Rust, "// use std::fs;").is_empty());
        assert!(targets(Language::Python, "# import os").is_empty());
        assert!(targets(Language::JavaScript, "// import x from 'y';").is_empty());
    }

    #[test]
    fn records_line_numbers() {
        let parsed = parse(Language::Rust, "\n\nuse std::fs;\n");
        assert_eq!(parsed[0].line, 3);
    }

    #[test]
    fn the_same_target_on_two_lines_is_two_references() {
        // The parser reports occurrences, not conclusions. Two mentions of the
        // same module on different lines are two facts about the text, and the
        // line numbers are what make an edge auditable back to its source.
        // Collapsing them into one dependency is the graph's job.
        let parsed = parse(Language::Python, "import os\nimport os\n");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].line, 1);
        assert_eq!(parsed[1].line, 2);
    }

    #[test]
    fn an_identical_reference_on_the_same_line_appears_once() {
        let parsed = parse(Language::Python, "import os, os\n");
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn unknown_extensions_have_no_language() {
        assert!(Language::from_path(Path::new("notes.md")).is_none());
        assert!(Language::from_path(Path::new("Makefile")).is_none());
        assert_eq!(
            Language::from_path(Path::new("src/lib.rs")),
            Some(Language::Rust)
        );
    }

    // ─────────────────────────────────────────────── the names a file declares

    fn names(language: Language, source: &str) -> Vec<(String, &'static str)> {
        definitions(language, source)
            .into_iter()
            .map(|found| (found.name, found.kind.word()))
            .collect()
    }

    /// The one sample that was not invented here.
    ///
    /// Rule 6 of `Estrategia-de-Pruebas.md`: a parser for somebody else's format
    /// needs one captured real sample, verbatim, because a hand-written fixture
    /// proves the parser matches its author's model of the format and nothing
    /// else. That rule was written after a parser was tested only against
    /// fixtures its author invented — twice, and the second time it accused
    /// llama.cpp of ignoring a grammar it had just obeyed.
    ///
    /// This file is the sample: real Rust, written for another purpose, never
    /// adjusted to make a test pass. The C half has one too, further down. The
    /// Python, JavaScript and Go halves have **none**, and that is stated here
    /// rather than left to be discovered: what those three have below are
    /// fixtures, and what a fixture proves is smaller than it looks.
    #[test]
    fn the_names_in_this_very_file_are_found_in_it() {
        let source = include_str!("lib.rs");
        let found: Vec<String> = definitions(Language::Rust, source)
            .into_iter()
            .map(|d| d.name)
            .collect();

        for expected in [
            "parse",              // pub fn
            "definitions",        // pub fn
            "identifiers",        // pub fn
            "scrub",              // a private fn
            "Scrubbed",           // a private struct
            "rust_definition",    // a private fn
            "Reference",          // pub struct
            "ReferenceKind",      // pub enum
            "Language",           // pub enum
            "SymbolKind",         // pub enum
            "C_NOT_A_DEFINITION", // a const
        ] {
            assert!(
                found.contains(&expected.to_string()),
                "`{expected}` is defined in this file and the parser did not find it"
            );
        }

        // And nothing that is plainly not a definition. `use` lines and match
        // arms are the two shapes most likely to be read as one.
        for wrong in ["use", "match", "if", "for", "std", "Some"] {
            assert!(
                !found.contains(&wrong.to_string()),
                "`{wrong}` is not a definition and the parser reported one"
            );
        }
    }

    #[test]
    fn a_const_fn_is_a_function_and_not_a_constant() {
        // The one prefix where stripping in the wrong order silently produces a
        // constant named `fn`.
        assert_eq!(
            names(Language::Rust, "pub const fn width() -> usize { 4 }"),
            vec![("width".to_string(), "function")]
        );
    }

    #[test]
    fn visibility_does_not_change_what_a_name_is() {
        for line in [
            "fn one() {}",
            "pub fn one() {}",
            "pub(crate) fn one() {}",
            "pub(super) fn one() {}",
        ] {
            assert_eq!(
                names(Language::Rust, line),
                vec![("one".to_string(), "function")],
                "{line:?}"
            );
        }
    }

    #[test]
    fn a_keyword_that_only_starts_a_name_is_not_that_keyword() {
        // `fnord` is not `fn ord`, and `typedef` is not `type def`. Without the
        // check that the name begins where the keyword ends, both become
        // definitions of things that do not exist.
        assert!(names(Language::Rust, "let fnord = 3;").is_empty());
        assert!(names(Language::Rust, "structural();").is_empty());
    }

    #[test]
    fn a_commented_out_definition_is_not_a_definition() {
        // The same rule the references half already has, and for the same
        // reason: a symbol the code cannot reach is worse than a missing one,
        // because it sends somebody to a line that does nothing.
        assert!(names(Language::Rust, "// fn ghost() {}").is_empty());
        assert!(names(Language::Python, "# def ghost(): pass").is_empty());
    }

    #[test]
    fn python_definitions_are_found_at_any_indentation() {
        // Hand-written, and what that proves is that the parser matches its
        // author's model of Python. There is no real Python in this repository
        // to capture, and rule 6 says to say so rather than imply otherwise.
        let found = names(
            Language::Python,
            "class Session:\n    def login(self):\n        pass\n    async def logout(self):\n        pass\n",
        );
        assert_eq!(
            found,
            vec![
                ("Session".to_string(), "type"),
                ("login".to_string(), "function"),
                ("logout".to_string(), "function"),
            ]
        );
    }

    #[test]
    fn javascript_declares_names_in_four_shapes() {
        let found = names(
            Language::JavaScript,
            "export function login(u) {}\nclass Session {}\nconst LIMIT = 5;\nexport const parse = (x) => x;\n",
        );
        assert!(found.contains(&("login".to_string(), "function")));
        assert!(found.contains(&("Session".to_string(), "type")));
        assert!(found.contains(&("LIMIT".to_string(), "constant")));
        assert!(found.contains(&("parse".to_string(), "constant")));
    }

    #[test]
    fn go_methods_are_named_by_the_method_and_not_by_the_receiver() {
        let found = names(
            Language::Go,
            "func Login(u string) {}\nfunc (s *Session) Close() {}\ntype Session struct {}\n",
        );
        assert!(found.contains(&("Login".to_string(), "function")));
        // The failure this prevents: every method in the file indexed under the
        // name of its receiver, so `Close` cannot be found at all and `Session`
        // has forty definitions.
        assert!(found.contains(&("Close".to_string(), "function")));
        assert!(found.contains(&("Session".to_string(), "type")));
    }

    /// The second captured sample, and the only real C in this repository.
    #[test]
    fn the_real_bpf_watcher_yields_its_functions_and_not_its_control_flow() {
        let source = include_str!("../../../lsm/thalyx_watch.bpf.c");
        let found = definitions(Language::C, source);
        let named: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();

        assert!(
            !named.is_empty(),
            "real C yielded no definitions at all: the C half is not working"
        );
        // Control flow read as functions is the failure that makes this useless
        // rather than merely incomplete: it does not add a few wrong rows, it
        // buries the right one.
        for wrong in C_NOT_A_DEFINITION {
            assert!(
                !named.contains(wrong),
                "`{wrong}` was read as a C function definition"
            );
        }
    }

    // ──────────────────────────────────────────── the names a file merely uses

    #[test]
    fn a_name_inside_a_string_is_not_a_use_of_it() {
        // The failure this prevents is the one that makes `grep` expensive to
        // read: a log line mentioning `login` counted as a call site, which a
        // caller cannot tell apart from a real one without opening the file —
        // exactly the trip this is supposed to save.
        let mentions: Vec<String> = identifiers(Language::Rust, "println!(\"login failed\");")
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(!mentions.contains(&"login".to_string()), "{mentions:?}");
        assert!(mentions.contains(&"println".to_string()));
    }

    #[test]
    fn a_name_inside_a_comment_is_not_a_use_of_it() {
        assert!(identifiers(Language::Rust, "// login happens here").is_empty());
        assert!(identifiers(Language::Python, "# login happens here").is_empty());
    }

    #[test]
    fn a_name_in_a_block_comment_is_not_a_use_of_it() {
        // Found by indexing this repository: `uapi_btrfs.h` was reported as a
        // dependent of the parser because the word `definitions` appears in a
        // `/* … */` header comment. Line-at-a-time comment handling can never
        // see this, and once a use becomes a dependency edge a wrong one is no
        // longer a row too many — it is a file somebody goes and reads.
        let source = "/*\n * login is described here\n */\nfn other() {}\n";
        let mentions: Vec<String> = identifiers(Language::Rust, source)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(!mentions.contains(&"login".to_string()), "{mentions:?}");
        assert!(mentions.contains(&"other".to_string()), "{mentions:?}");
    }

    #[test]
    fn a_name_in_a_string_that_runs_over_two_lines_is_not_a_use_of_it() {
        // The other half of the same repository finding: `thalyx-permd` was a
        // dependent of the parser because a panic message continued onto a
        // second line with a backslash, and the second line was scanned as code.
        let source = "let message = \"a message that runs \\\n     over two lines about login\";\n";
        let mentions: Vec<String> = identifiers(Language::Rust, source)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(!mentions.contains(&"login".to_string()), "{mentions:?}");
        assert!(mentions.contains(&"message".to_string()), "{mentions:?}");
    }

    #[test]
    fn a_comment_at_the_end_of_a_line_is_not_part_of_the_line() {
        let mentions: Vec<String> = identifiers(Language::Rust, "run(); // see login for why")
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(mentions, vec!["run".to_string()], "{mentions:?}");
    }

    #[test]
    fn an_include_inside_a_block_comment_is_not_a_dependency() {
        // The same hole in the reference half, where it is worse: an edge no
        // execution can follow. A commented-out `#include` was becoming one.
        let source = "/*\n#include \"old.h\"\n*/\n#include \"real.h\"\n";
        let found = targets(Language::C, source);
        assert_eq!(found, vec!["real.h".to_string()], "{found:?}");
    }

    #[test]
    fn a_lifetime_does_not_swallow_the_rest_of_the_file() {
        // The one thing the scrubber must not do. A `'` is a lifetime in Rust
        // far more often than the start of anything, so it may never carry over
        // to the next line — a state that did would blank whole files, which is
        // a recall loss nobody would notice and everybody would be hurt by.
        let source = "fn one<'a>(x: &'a str) {}\nfn two() { login(); }\n";
        let mentions: Vec<String> = identifiers(Language::Rust, source)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(mentions.contains(&"login".to_string()), "{mentions:?}");
    }

    #[test]
    fn a_slash_star_inside_a_string_does_not_open_a_comment() {
        // The mirror of the rule above, and the reason the scrubber tracks
        // strings and comments together instead of one after the other.
        let source = "let pattern = \"/*\";\nfn two() { login(); }\n";
        let mentions: Vec<String> = identifiers(Language::Rust, source)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(mentions.contains(&"login".to_string()), "{mentions:?}");
    }

    #[test]
    fn python_keeps_its_floor_division_and_c_keeps_its_preprocessor() {
        // `//` is a comment in four of the five languages and an operator in
        // Python; `#` is a comment in Python and the preprocessor in C. Getting
        // either backwards silently deletes half of what a language's parser is
        // for, and neither shows up as an error.
        let mentions: Vec<String> = identifiers(Language::Python, "half = total // divisor")
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(mentions.contains(&"divisor".to_string()), "{mentions:?}");
        assert_eq!(targets(Language::C, "#include <stdio.h>"), vec!["stdio.h"]);
    }

    // ─────────────────────────────── what another file could reach at all

    #[test]
    fn each_language_is_asked_its_own_visibility_rule_and_not_a_guess() {
        let exported = |language, line: &str| definitions(language, line)[0].exported;

        // Rust: `pub`, and `pub(self)` is the one that reads backwards.
        assert!(exported(Language::Rust, "pub fn open() {}"));
        assert!(exported(Language::Rust, "pub(crate) struct Thing;"));
        assert!(!exported(Language::Rust, "fn helper() {}"));
        assert!(!exported(Language::Rust, "pub(self) fn helper() {}"));

        // JavaScript: the word `export`.
        assert!(exported(Language::JavaScript, "export function go() {}"));
        assert!(!exported(Language::JavaScript, "function go() {}"));

        // Go: the capital letter, which is the whole system.
        assert!(exported(Language::Go, "func Close() {}"));
        assert!(!exported(Language::Go, "func close() {}"));

        // C: external unless `static`.
        assert!(exported(Language::C, "int setup(void) {"));
        assert!(!exported(Language::C, "static int setup(void) {"));

        // Python: the convention it enforces nowhere and everyone follows.
        assert!(exported(Language::Python, "def go():"));
        assert!(!exported(Language::Python, "def _go():"));
    }

    #[test]
    fn a_name_the_file_binds_is_the_files_own_and_not_somebody_elses() {
        // The defect: `thalyx-snapshot` declares `pub fn directory(&self)`, and
        // every file in the repository holding `for directory in …` was
        // reported as depending on it. A binding shadows anything outside, so
        // the file is talking about itself.
        let bound = bound_names(
            Language::Rust,
            "fn walk(root: &Path) {\n    for directory in root {\n        let place = 1;\n    }\n}\n             struct Held { subvolume: u32 }\nmod restore;\nuse a::b as Keys;\n",
        );
        for name in ["root", "directory", "place", "subvolume", "restore", "Keys"] {
            assert!(bound.contains(name), "`{name}` is bound here: {bound:?}");
        }
        // And what is plainly a reference is not swept up with them.
        for name in ["walk", "Path", "Held"] {
            assert!(
                !bound.contains(name),
                "`{name}` is not a binding: {bound:?}"
            );
        }
    }

    #[test]
    fn a_path_is_not_a_binding_however_many_colons_it_has() {
        // `name:` is a binding and `name::` is the most reference-like thing
        // there is. One character apart, and reading them alike would delete
        // every qualified call in the tree.
        let bound = bound_names(Language::Rust, "let answer = crate::store::save();\n");
        assert!(bound.contains("answer"), "{bound:?}");
        assert!(!bound.contains("store"), "{bound:?}");
        assert!(!bound.contains("save"), "{bound:?}");
    }

    #[test]
    fn what_is_on_the_right_of_the_equals_is_not_being_bound() {
        let bound = bound_names(Language::Rust, "let held = existing;\n");
        assert!(bound.contains("held"), "{bound:?}");
        assert!(!bound.contains("existing"), "{bound:?}");
    }

    #[test]
    fn a_mention_carries_the_line_it_was_on() {
        let mentions = identifiers(Language::Rust, "\n\nlogin();\n");
        assert!(mentions.contains(&("login".to_string(), 3)));
    }

    #[test]
    fn output_is_deterministic() {
        // The parser is replaceable, so its contract has to be stable: the same
        // input must always produce byte-identical output.
        let source = "use b::x;\nuse a::y;\nuse c::z;\n";
        assert_eq!(parse(Language::Rust, source), parse(Language::Rust, source));
    }
}

#[cfg(test)]
mod balance {
    use super::*;

    #[test]
    fn ordinary_rust_is_balanced() {
        // The control, and it is the important half: a check that reported
        // every file as broken would pass every "it noticed" test below and be
        // worse than having no check at all.
        assert_eq!(
            unbalanced(
                Language::Rust,
                "fn main() {\n    let a = [1, 2, 3];\n    println!(\"{a:?}\");\n}\n"
            ),
            None
        );
    }

    #[test]
    fn a_brace_a_substitution_ate_is_found_and_located() {
        // What a mechanical edit actually breaks.
        let why = unbalanced(Language::Rust, "fn main() {\n    let a = 1;\n").expect("a report");
        assert!(why.contains("line 1"), "{why}");
        assert!(why.contains("never closed"), "{why}");
    }

    #[test]
    fn a_bracket_closed_by_the_wrong_kind_is_found() {
        let why = unbalanced(Language::Rust, "fn f() { let a = (1, 2]; }\n").expect("a report");
        assert!(why.contains('('), "{why}");
    }

    #[test]
    fn brackets_inside_strings_and_comments_are_not_brackets() {
        // The reason this is built on the scrubber. Every one of these lines is
        // ordinary code that a naive counter calls broken, and a check that is
        // wrong about ordinary code is one nobody leaves switched on.
        for source in [
            "fn main() { println!(\"(\"); }\n",
            "fn main() { /* } */ }\n",
            "fn main() { // }\n}\n",
            "fn main() { let s = \"unclosed { brace\"; }\n",
        ] {
            assert_eq!(unbalanced(Language::Rust, source), None, "{source}");
        }
    }

    #[test]
    fn every_rust_file_in_this_repository_is_balanced() {
        // **Rule 6, and it earned its place on the first run.** A fixture only
        // proves the checker agrees with whoever wrote the fixture; this is
        // ninety thousand lines nobody wrote for it. The first version of
        // `unbalanced` was built on the scrubber and passed every hand-written
        // case above — and this test found, immediately, that `-> &'static str
        // {` arrives from the scrubber with its brace blanked away, because a
        // lone `'` makes it drop the rest of the line. That is what the lexer
        // below the fixtures exists for.
        //
        // A false positive here is the failure that matters: a check that
        // reports ordinary code as broken is one that gets switched off, and
        // then it protects nothing.
        let mut looked_at = 0usize;
        let mut trouble = Vec::new();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the workspace root")
            .to_path_buf();
        let mut waiting = vec![root.clone()];
        while let Some(directory) = waiting.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                // `target` holds generated sources from every dependency, which
                // is neither this project's code nor a fair sample of it.
                if path.is_dir() {
                    if !matches!(entry.file_name().to_str(), Some("target" | ".git")) {
                        waiting.push(path);
                    }
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                looked_at += 1;
                if let Some(why) = unbalanced(Language::Rust, &source) {
                    trouble.push(format!("{}: {why}", path.display()));
                }
            }
        }
        assert!(
            looked_at > 50,
            "only {looked_at} files were read, so this proved nothing"
        );
        assert!(
            trouble.is_empty(),
            "{} of {looked_at} real files were called unbalanced:\n{}",
            trouble.len(),
            trouble.join("\n")
        );
    }
}
