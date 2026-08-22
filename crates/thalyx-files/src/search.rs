//! Finding a file by its name, and finding text inside files.
//!
//! Point 6 of the usable terminal, in `vault/06-Pendientes/Tareas-Pendientes.md`.
//! Two questions that a person asks constantly and that Thalyx could not answer
//! at all: *where is the file called this*, and *which files say this*.
//!
//! ## Why these are not `buscar`
//!
//! `buscar` already exists and answers a third question — where a **symbol** is
//! declared and every place it is used, out of the semantic index
//! (`Superficie-para-el-LLM.md`, punto **C2**). It is the better answer whenever
//! it applies, because it costs a fraction of the tokens and has no false
//! positives from comments or strings.
//!
//! It applies to five languages and to files the index has been built over.
//! Everything else — a log, a `Makefile`, a TOML, a language nobody wrote a
//! parser for, a tree nobody indexed — is what these two verbs are for. Cesar
//! decided on 2026-08-23 that they get their own names rather than crowding into
//! `buscar`, so that no caller has to work out which of three questions a single
//! verb answered. Three questions, three verbs.
//!
//! ## Literal text, and no pattern language
//!
//! Content is matched **literally**: what was typed is what is looked for, and a
//! dot is a dot. Cesar's decision, on 2026-08-23, and there are two reasons
//! behind the recommendation it followed.
//!
//! The first is the image. It carries the kernel and one program, so every
//! dependency is weight at boot, and a regular-expression engine is more of it
//! than the question is worth.
//!
//! The second is that a pattern language is not decided yet. Point 9 of the same
//! list — whether Thalyx has a shell language at all, with its quoting and its
//! wildcards — is explicitly *decree before code*. Inventing a regex dialect
//! here would be deciding a piece of it in a crate nobody would think to look
//! in.
//!
//! Names, on the other hand, are matched with the `*` and `?` of [`matches`],
//! because that is the vocabulary `rm`, `cp` and `mv` already use in this
//! session, and a second spelling of the same idea is a discovery cost paid
//! twice.
//!
//! ## What is refused rather than answered slowly
//!
//! Both walks stop at [`CEILING`] files and refuse, the same as the index does,
//! and they stop **while walking** rather than after. A tree of a million files
//! costs a moment and not a minute, and the answer names the ceiling so the
//! caller can do the one useful thing: name something smaller.
//!
//! ## Rule 10, in the shape it takes here
//!
//! A file that could not be read is not a file with no matches. It travels in
//! `unreadable`, with its reason, in the same value as the hits — never dropped,
//! and never counted as a miss. A `contenido` that quietly skipped the file it
//! could not open would answer "not here" about the one place it is.

use crate::{CEILING, FileError, Kind, matches, walk};
use std::path::Path;

/// A file whose name matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    /// Relative to the root the search started from, so the row is short and
    /// says where it is relative to what was asked.
    pub path: String,
    pub kind: Kind,
}

/// One line that held the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    /// 1-based, the way every editor and every error message counts lines —
    /// including `editar`, which is what a caller does with this next.
    pub line: usize,
    /// The line itself, cut to [`LINE`] characters when it is longer.
    pub text: String,
    /// Whether `text` is the whole line. A caller that pasted a cut line back
    /// into a file would be writing something the file never said.
    pub cut: bool,
}

/// What a search found, and everything it could not establish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found<T> {
    /// In ascending path order — and for [`Hit`], by path then line. The window
    /// pages by that order, so producing it out of order is a bug that would
    /// surface as a refused answer rather than as a wrong one.
    pub rows: Vec<T>,
    /// How many files were looked at. Not how many matched: a search of eleven
    /// thousand files that found nothing and a search of four that found
    /// nothing are different answers to the same question.
    pub looked_at: usize,
    /// Rule 10. What could not be read, and why, kept apart from what did not
    /// match.
    pub unreadable: Vec<(String, String)>,
    /// Files skipped because they are not text. Only ever non-zero for
    /// [`in_contents`], and its own number rather than folded into
    /// `unreadable`, because a binary is a file that was read perfectly well
    /// and has nothing a line search can say about it.
    pub not_text: usize,
}

/// How many characters of a matching line travel back.
///
/// A minified bundle is one line of two hundred thousand characters, and a
/// single hit in it would otherwise fill the answer that was supposed to be
/// bounded. Cut lines say they were cut.
pub const LINE: usize = 400;

/// Refusing to read a file larger than this while searching contents.
///
/// Four megabytes, the same ceiling `editar` uses, and for a related reason: a
/// line-by-line search of a two-gigabyte file is an answer nobody waits for.
/// It is counted as `not_text` rather than as unreadable, because nothing went
/// wrong — the file is simply not what this verb is for.
pub const WEIGHT: u64 = 4 * 1024 * 1024;

/// Files whose name matches `pattern`, anywhere under `root`.
///
/// The pattern is matched against the **file name**, not the path. `*.rs` finds
/// every Rust file in the tree, which is what a person means by it; matching the
/// path would make `*.rs` mean only the ones sitting directly in `root`, and the
/// person would have to know to type `*/*.rs` and `*/*/*.rs`.
pub fn by_name(root: &Path, pattern: &str) -> Result<Found<Named>, FileError> {
    if pattern.is_empty() {
        return Err(FileError::NothingAsked);
    }
    ensure_directory(root)?;

    let mut found = Found {
        rows: Vec::new(),
        looked_at: 0,
        unreadable: Vec::new(),
        not_text: 0,
    };

    for entry in walk(root) {
        let entry = match entry {
            Ok(entry) => entry,
            // The walk failing on one directory is not the walk failing. Rule
            // 10: it is recorded and the rest of the tree is still searched,
            // because a search that stopped at the first unreadable folder
            // would answer "not found" about a tree it never finished.
            Err(error) => {
                found
                    .unreadable
                    .push((path_in(&error), stripped(error.to_string())));
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        found.looked_at += 1;
        if found.looked_at > CEILING {
            return Err(FileError::TreeTooLarge {
                root: root.to_path_buf(),
                ceiling: CEILING,
            });
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        if !matches(pattern, &name) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let kind = match entry.path().symlink_metadata() {
            Ok(meta) => crate::kind_of(entry.path(), &meta),
            Err(error) => {
                found
                    .unreadable
                    .push((relative.to_string_lossy().into_owned(), error.to_string()));
                continue;
            }
        };
        found.rows.push(Named {
            path: relative.to_string_lossy().into_owned(),
            kind,
        });
    }

    found.rows.sort_by(|a, b| a.path.cmp(&b.path));
    found.unreadable.sort();
    Ok(found)
}

/// Lines holding `text`, in every text file under `root`.
///
/// `text` is literal. Case matters, because a search that quietly ignored case
/// would answer about `Error` when asked about `error`, and there is no way for
/// the caller to ask for the strict one back.
pub fn in_contents(root: &Path, text: &str) -> Result<Found<Hit>, FileError> {
    if text.is_empty() {
        return Err(FileError::NothingAsked);
    }
    ensure_directory(root)?;

    let mut found = Found {
        rows: Vec::new(),
        looked_at: 0,
        unreadable: Vec::new(),
        not_text: 0,
    };

    for entry in walk(root) {
        let entry = match entry {
            Ok(error_free) => error_free,
            Err(error) => {
                found
                    .unreadable
                    .push((path_in(&error), stripped(error.to_string())));
                continue;
            }
        };
        if entry.depth() == 0 || !entry.file_type().is_file() {
            continue;
        }
        found.looked_at += 1;
        if found.looked_at > CEILING {
            return Err(FileError::TreeTooLarge {
                root: root.to_path_buf(),
                ceiling: CEILING,
            });
        }

        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().into_owned();

        match entry.metadata() {
            Ok(meta) if meta.len() > WEIGHT => {
                found.not_text += 1;
                continue;
            }
            Ok(_) => {}
            Err(error) => {
                found.unreadable.push((relative, error.to_string()));
                continue;
            }
        }

        let bytes = match std::fs::read(entry.path()) {
            Ok(bytes) => bytes,
            Err(error) => {
                found.unreadable.push((relative, error.to_string()));
                continue;
            }
        };
        // The same sniff `leer` refuses on, and for a related reason: a search
        // of an ELF binary finds the text inside it and hands back a "line" of
        // machine code. That is not a false positive a caller can filter — it
        // is a screenful of bytes with a path in front of it.
        if !bytes.is_empty() && crate::not_text(&bytes).is_some() {
            found.not_text += 1;
            continue;
        }
        let Ok(whole) = std::str::from_utf8(&bytes) else {
            // Valid at the sniff and invalid further in. Not text, and not a
            // failure to read: the bytes arrived.
            found.not_text += 1;
            continue;
        };

        for (at, line) in whole.lines().enumerate() {
            if !line.contains(text) {
                continue;
            }
            let (shown, cut) = clip(line);
            found.rows.push(Hit {
                path: relative.clone(),
                line: at + 1,
                text: shown,
                cut,
            });
        }
    }

    // By path, then by line. `sort_by_key` on the pair would clone every path;
    // the comparison does not.
    found
        .rows
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    found.unreadable.sort();
    Ok(found)
}

/// The key a cursor into a name search names.
///
/// The path alone, because two rows never share one: a tree cannot hold two
/// files at the same relative path.
pub fn name_key(row: &Named) -> Vec<u8> {
    row.path.as_bytes().to_vec()
}

/// The key a cursor into a content search names.
///
/// Path and line together, and the line big-endian and fixed-width so that byte
/// order and numeric order are the same thing. As decimal text, line `10` sorts
/// before line `9` and the window would refuse the whole answer as unordered —
/// which is the failure this shape exists to prevent, found the same way in
/// `index.rs`.
///
/// The zero byte between them is what stops `("a/b", 1)` and `("a", 0x2f62…)`
/// from producing the same key: a path cannot contain a zero byte, so the
/// separator can never appear inside the first field.
pub fn hit_key(row: &Hit) -> Vec<u8> {
    let mut key = row.path.as_bytes().to_vec();
    key.push(0);
    key.extend_from_slice(&(row.line as u64).to_be_bytes());
    key
}

/// Cut a long line on a character boundary, and say whether it was cut.
///
/// By characters and not by bytes: slicing UTF-8 mid-character would hand back
/// a string that is not text, which is exactly what this verb refuses whole
/// files for.
fn clip(line: &str) -> (String, bool) {
    if line.chars().count() <= LINE {
        return (line.to_string(), false);
    }
    (line.chars().take(LINE).collect(), true)
}

/// A search is asked of a directory, and a file is a different question.
///
/// Refused rather than treated as a one-file tree, because `contenido algo
/// notas.txt` reads as *look for `algo` in `notas.txt`* and that is not what
/// this walks. Answering it as a tree of one would make the wrong reading work
/// most of the time and fail silently the rest.
fn ensure_directory(root: &Path) -> Result<(), FileError> {
    let meta = root
        .metadata()
        .map_err(|error| crate::classify(root, error))?;
    if meta.is_dir() {
        Ok(())
    } else {
        Err(FileError::NotADirectory(root.to_path_buf()))
    }
}

/// The path a walk error is about, or the walk's own root when it names none.
fn path_in(error: &walkdir::Error) -> String {
    error
        .path()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("…"))
}

/// `walkdir`'s message with the path taken back out.
///
/// It formats as `IO error for operation on /a/b: …`, and the path is already
/// the other half of the pair. Repeating it puts the same absolute path twice
/// in a row that is supposed to be relative.
fn stripped(message: String) -> String {
    match message.split_once(": ") {
        Some((_, detail)) => detail.to_string(),
        None => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{Asked, page};

    /// A real tree on a real filesystem, because rule 1 of
    /// `Estrategia-de-Pruebas.md` says every defect this project has found came
    /// from running something rather than from reading it. A fake directory
    /// would model my idea of a walk, and the walk is what is under test.
    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, contents) in files {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, contents).unwrap();
        }
        dir
    }

    fn names(found: &Found<Named>) -> Vec<&str> {
        found.rows.iter().map(|row| row.path.as_str()).collect()
    }

    #[test]
    fn a_pattern_matches_the_name_and_finds_it_however_deep_it_is() {
        // Matching the whole relative path instead would make `*.rs` mean only
        // the files sitting directly in the root, and a person would have to
        // learn to type `*/*.rs` and `*/*/*.rs` to search their own project.
        let dir = tree(&[
            ("top.rs", ""),
            ("src/one.rs", ""),
            ("src/deep/two.rs", ""),
            ("src/notes.txt", ""),
        ]);
        let found = by_name(dir.path(), "*.rs").unwrap();
        assert_eq!(
            names(&found),
            vec!["src/deep/two.rs", "src/one.rs", "top.rs"]
        );
    }

    #[test]
    fn a_hidden_directory_is_not_walked_into_and_the_named_root_always_is() {
        let dir = tree(&[(".git/objects/thing.rs", ""), ("kept.rs", "")]);
        assert_eq!(
            names(&by_name(dir.path(), "*.rs").unwrap()),
            vec!["kept.rs"]
        );

        // And the other half of the same rule, which is what keeps the first
        // one from being a trap: a person who names a hidden folder has named
        // it on purpose, and a filter that refused it would answer "nothing
        // here" about a directory full of files.
        let inside = dir.path().join(".git");
        let found = by_name(&inside, "*.rs").unwrap();
        assert_eq!(names(&found), vec!["objects/thing.rs"]);
    }

    #[test]
    fn a_search_that_matched_nothing_still_says_how_much_it_looked_at() {
        // The two ways of finding nothing. A pattern that is wrong and a tree
        // that is empty produce the same rows and different answers, and only
        // the count tells them apart — without it a caller re-runs the search
        // somewhere else to find out which it was.
        let dir = tree(&[("a.txt", ""), ("b/c.txt", "")]);
        let found = by_name(dir.path(), "*.rs").unwrap();
        assert!(found.rows.is_empty());
        // Three: two files and the directory `b`, which is a thing a name
        // search can match and therefore a thing it looked at.
        assert_eq!(found.looked_at, 3);
    }

    #[test]
    fn a_directory_matches_a_name_search_and_says_it_is_one() {
        let dir = tree(&[("build_output/x", "")]);
        let found = by_name(dir.path(), "build_*").unwrap();
        assert_eq!(names(&found), vec!["build_output"]);
        assert!(matches!(found.rows[0].kind, Kind::Directory));
    }

    #[test]
    fn a_line_comes_back_with_the_number_an_editor_would_use() {
        // 1-based, because the next thing anybody does with this answer is
        // `editar <archivo> ver <línea>`, and an off-by-one between the verb
        // that finds a line and the verb that shows it is the kind of defect
        // that survives for months by looking like a typo.
        let dir = tree(&[("a.txt", "one\ntwo\nthree\n")]);
        let found = in_contents(dir.path(), "two").unwrap();
        assert_eq!(found.rows.len(), 1);
        assert_eq!(found.rows[0].line, 2);
        assert_eq!(found.rows[0].text, "two");
        assert!(!found.rows[0].cut);
    }

    #[test]
    fn the_text_is_literal_so_a_dot_is_a_dot_and_a_star_is_a_star() {
        // The decision Cesar took on 2026-08-23, as an assertion. If a regular
        // expression engine is ever put behind this verb, this test is what
        // says so out loud instead of a person discovering it when `a.c`
        // matched `abc`.
        let dir = tree(&[("a.txt", "abc\na.c\n"), ("b.txt", "x*y\nxy\n")]);
        let dotted = in_contents(dir.path(), "a.c").unwrap();
        assert_eq!(dotted.rows.len(), 1);
        assert_eq!(dotted.rows[0].text, "a.c");

        let starred = in_contents(dir.path(), "x*y").unwrap();
        assert_eq!(starred.rows.len(), 1);
        assert_eq!(starred.rows[0].text, "x*y");
    }

    #[test]
    fn case_matters_because_there_is_no_way_to_ask_for_it_back() {
        // A search that quietly ignored case would answer about `Error` when
        // asked about `error`, and nothing in the answer would say so. The
        // reverse mistake — being strict when somebody wanted loose — is
        // visible immediately and costs one more search.
        let dir = tree(&[("a.txt", "Error here\nerror there\n")]);
        let found = in_contents(dir.path(), "Error").unwrap();
        assert_eq!(found.rows.len(), 1);
        assert_eq!(found.rows[0].line, 1);
    }

    #[test]
    fn a_binary_is_skipped_and_counted_rather_than_answered_about() {
        // `cat` on a binary is a rite of passage that leaves a terminal
        // unusable, and on the image there is no second terminal. A grep hit
        // inside an ELF is the same bytes with a path in front of them.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
        std::fs::write(dir.path().join("thing.bin"), b"hello\0\0\0hello").unwrap();

        let found = in_contents(dir.path(), "hello").unwrap();
        assert_eq!(found.rows.len(), 1);
        assert_eq!(found.rows[0].path, "a.txt");
        // Counted, not silent. A caller told nothing would conclude the binary
        // simply had no match in it.
        assert_eq!(found.not_text, 1);
        assert!(found.unreadable.is_empty(), "a binary read perfectly well");
    }

    #[test]
    fn a_file_too_heavy_to_read_is_counted_as_not_text_and_never_read() {
        let dir = tempfile::tempdir().unwrap();
        let heavy = dir.path().join("heavy.log");
        std::fs::write(&heavy, vec![b'x'; (WEIGHT + 1) as usize]).unwrap();
        std::fs::write(dir.path().join("light.log"), "xxx\n").unwrap();

        let found = in_contents(dir.path(), "xxx").unwrap();
        assert_eq!(found.rows.len(), 1);
        assert_eq!(found.rows[0].path, "light.log");
        assert_eq!(found.not_text, 1);
    }

    #[test]
    fn a_line_longer_than_the_ceiling_is_cut_and_says_that_it_was() {
        // A minified bundle is one line of two hundred thousand characters. A
        // caller that pasted a cut line back into a file would be writing
        // something the file never said, and only `cut` tells it not to.
        let dir = tree(&[(
            "one.js",
            &format!("{}needle{}\n", "a".repeat(10), "b".repeat(LINE)),
        )]);
        let found = in_contents(dir.path(), "needle").unwrap();
        assert_eq!(found.rows.len(), 1);
        assert!(found.rows[0].cut);
        assert_eq!(found.rows[0].text.chars().count(), LINE);
    }

    #[test]
    fn a_cut_line_is_still_valid_text_and_never_half_a_character() {
        // Slicing UTF-8 mid-character would hand back a string that is not
        // text — which is the exact thing this verb refuses whole files for.
        let dir = tree(&[("u.txt", &format!("{}needle\n", "ñ".repeat(LINE)))]);
        let found = in_contents(dir.path(), "needle").unwrap();
        assert_eq!(found.rows[0].text.chars().count(), LINE);
        assert!(found.rows[0].text.chars().all(|c| c == 'ñ'));
    }

    #[test]
    fn a_symlink_is_not_followed_and_a_tree_cannot_search_itself_forever() {
        let dir = tree(&[("real/a.txt", "needle\n")]);
        std::os::unix::fs::symlink(dir.path(), dir.path().join("real/loop")).unwrap();

        // It terminates, which is the claim. A walk that followed links would
        // recurse into itself until the path length gave out.
        let found = in_contents(dir.path(), "needle").unwrap();
        assert_eq!(found.rows.len(), 1);
        assert_eq!(found.rows[0].path, "real/a.txt");
    }

    #[test]
    fn nothing_to_look_for_is_refused_rather_than_matching_every_line() {
        let dir = tree(&[("a.txt", "anything\n")]);
        assert!(matches!(
            in_contents(dir.path(), "").unwrap_err(),
            FileError::NothingAsked
        ));
        assert!(matches!(
            by_name(dir.path(), "").unwrap_err(),
            FileError::NothingAsked
        ));
    }

    #[test]
    fn a_file_named_where_a_tree_goes_is_refused_and_not_searched_as_one() {
        let dir = tree(&[("a.txt", "needle\n")]);
        let error = in_contents(&dir.path().join("a.txt"), "needle").unwrap_err();
        assert!(matches!(error, FileError::NotADirectory(_)));
        // And the shape of the other refusal stays its own thing.
        let absent = in_contents(&dir.path().join("nowhere"), "needle").unwrap_err();
        assert!(matches!(absent, FileError::Absent(_)));
    }

    #[test]
    fn a_tree_over_the_ceiling_is_refused_before_it_is_read_rather_than_after() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..=CEILING {
            std::fs::write(dir.path().join(format!("f{n:06}")), "needle\n").unwrap();
        }
        let refused = in_contents(dir.path(), "needle").unwrap_err();
        assert!(matches!(refused, FileError::TreeTooLarge { .. }));
        assert!(matches!(
            by_name(dir.path(), "*").unwrap_err(),
            FileError::TreeTooLarge { .. }
        ));
    }

    #[test]
    fn the_rows_are_in_an_order_the_window_will_page_and_not_refuse() {
        // The window refuses rows that are not ascending by key, because a
        // cursor into them would name a different position on the next call.
        // Producing them unordered here would surface as a whole answer being
        // refused, which reads as the paging being broken rather than the
        // search — so it is asserted where it is caused.
        let dir = tree(&[
            ("b/z.txt", "needle\nneedle\n"),
            ("a.txt", "x\nneedle\n"),
            ("b/a.txt", "needle\n"),
        ]);
        let found = in_contents(dir.path(), "needle").unwrap();
        let rows: Vec<&Hit> = found.rows.iter().collect();
        let paged = page(rows, |row| hit_key(row), &Asked::default())
            .expect("the search produced them in key order");
        assert_eq!(paged.total, 4);
        let seen: Vec<(&str, usize)> = paged
            .rows
            .iter()
            .map(|row| (row.path.as_str(), row.line))
            .collect();
        assert_eq!(
            seen,
            vec![("a.txt", 2), ("b/a.txt", 1), ("b/z.txt", 1), ("b/z.txt", 2)]
        );
    }

    #[test]
    fn ten_lines_of_the_same_file_page_without_a_line_number_sorting_as_text() {
        // The failure this is about: as decimal text, `10` sorts before `9`,
        // the keys stop ascending, and the window refuses the entire answer.
        // It was found this way once already, in `index.rs`.
        let dir = tree(&[("a.txt", &"needle\n".repeat(12))]);
        let found = in_contents(dir.path(), "needle").unwrap();
        let rows: Vec<&Hit> = found.rows.iter().collect();
        let paged = page(rows, |row| hit_key(row), &Asked::default()).expect("in key order");
        assert_eq!(paged.total, 12);
        assert_eq!(paged.rows[9].line, 10);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_reported_and_never_counted_as_a_miss() {
        // Rule 10: a failure to read is not a failure to exist. A `contenido`
        // that silently skipped the file it could not open would answer "not
        // here" about the one place the text is.
        let dir = tree(&[("open.txt", "needle\n"), ("shut.txt", "needle\n")]);
        let shut = dir.path().join("shut.txt");
        std::fs::set_permissions(&shut, std::os::unix::fs::PermissionsExt::from_mode(0o000))
            .unwrap();

        if std::fs::read(&shut).is_ok() {
            // Running as root, where a mode of 000 is not a refusal. Said out
            // loud rather than passing quietly, and
            // `THALYX_REQUIRE_UNREADABLE_TESTS=1` turns the skip into a failure
            // — one variable for this one requirement, so that demanding it on
            // a machine that has an ordinary user does not also demand four
            // other things.
            assert!(
                std::env::var_os("THALYX_REQUIRE_UNREADABLE_TESTS").is_none(),
                "THALYX_REQUIRE_UNREADABLE_TESTS is set and this is running as a user \
                 that a mode of 000 does not stop. Run the suite as an ordinary user."
            );
            eprintln!(
                "NOT PROVEN: a mode of 000 does not stop this user, so no read could be \
                 made to fail here."
            );
            return;
        }

        let found = in_contents(dir.path(), "needle").unwrap();
        assert_eq!(found.rows.len(), 1);
        assert_eq!(found.rows[0].path, "open.txt");
        // The control that makes the line above mean something: it looked at
        // both files. Without it, a walk that never reached `shut.txt` at all
        // would produce exactly this answer.
        assert_eq!(found.looked_at, 2);
        assert_eq!(found.unreadable.len(), 1);
        assert_eq!(found.unreadable[0].0, "shut.txt");
        assert_eq!(
            found.not_text, 0,
            "it was not read, so it is not 'not text'"
        );
    }
}
