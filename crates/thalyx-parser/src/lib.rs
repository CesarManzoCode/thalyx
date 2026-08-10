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

/// Extract every dependency reference from a source file.
pub fn parse(language: Language, source: &str) -> Vec<Reference> {
    let mut references = Vec::new();

    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        let number = index + 1;

        // Line comments are skipped rather than parsed. A commented-out import
        // is not a dependency, and treating it as one would put edges in the
        // graph that no execution can ever follow.
        if is_comment(language, line) {
            continue;
        }

        match language {
            Language::Rust => parse_rust(line, number, &mut references),
            Language::Python => parse_python(line, number, &mut references),
            Language::JavaScript => parse_javascript(line, number, &mut references),
            Language::C => parse_c(line, number, &mut references),
            Language::Go => parse_go(line, number, &mut references),
        }
    }

    references.sort();
    references.dedup();
    references
}

fn is_comment(language: Language, line: &str) -> bool {
    match language {
        // `#include` and `#define` are not comments in C, and treating every
        // `#` line as one there would throw away half of what the C half of
        // this parser is for.
        Language::Python => line.starts_with('#'),
        _ => line.starts_with("//"),
    }
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

    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        let number = index + 1;
        if is_comment(language, line) {
            continue;
        }
        match language {
            Language::Rust => rust_definition(line, number, &mut found),
            Language::Python => python_definition(line, number, &mut found),
            Language::JavaScript => javascript_definition(line, number, &mut found),
            Language::C => c_definition(line, number, &mut found),
            Language::Go => go_definition(line, number, &mut found),
        }
    }

    found.sort();
    found.dedup();
    found
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
    let mut found = Vec::new();

    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if is_comment(language, line) {
            continue;
        }
        let line = without_strings(line);

        let mut word = String::new();
        for character in line.chars() {
            if character.is_alphanumeric() || character == '_' {
                word.push(character);
                continue;
            }
            if !word.is_empty() {
                found.push((std::mem::take(&mut word), index + 1));
            }
        }
        if !word.is_empty() {
            found.push((word, index + 1));
        }
    }

    found
}

/// The line with everything between quotes removed.
///
/// One line at a time, so a string that spans lines is only half removed. That
/// is the honest limit of a line-oriented parser and it fails in the safe
/// direction: the extra identifiers it lets through are reported as mentions,
/// which is a row too many, never a definition in the wrong place.
fn without_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut inside: Option<char> = None;
    let mut escaped = false;

    for character in line.chars() {
        match inside {
            Some(quote) => {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == quote {
                    inside = None;
                    out.push(' ');
                }
            }
            None => {
                if character == '"' || character == '\'' {
                    inside = Some(character);
                    out.push(' ');
                } else {
                    out.push(character);
                }
            }
        }
    }

    out
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
            });
            return;
        }
    }

    if let Some(name) = name_after(line, "macro_rules! ") {
        out.push(Definition {
            name: name.to_string(),
            kind: SymbolKind::Function,
            line: number,
        });
    }
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
                line: number,
            });
            return;
        }
    }
}

fn javascript_definition(line: &str, number: usize, out: &mut Vec<Definition>) {
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
    if let Some(name) = name_after(line, "#define ") {
        out.push(Definition {
            name: name.to_string(),
            kind: SymbolKind::Constant,
            line: number,
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
                name: name.to_string(),
                kind,
                line: number,
            });
            return;
        }
    }
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
            "without_strings",    // a private fn
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
