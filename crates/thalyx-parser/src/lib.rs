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
        Language::Python => line.starts_with('#'),
        _ => line.starts_with("//"),
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

    #[test]
    fn output_is_deterministic() {
        // The parser is replaceable, so its contract has to be stable: the same
        // input must always produce byte-identical output.
        let source = "use b::x;\nuse a::y;\nuse c::z;\n";
        assert_eq!(parse(Language::Rust, source), parse(Language::Rust, source));
    }
}
