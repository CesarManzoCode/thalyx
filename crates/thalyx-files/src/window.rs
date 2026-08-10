//! Cutting a long answer to a size a context window survives.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **B1**: *nunca un
//! listado sin límite*. The failure it exists to prevent is the second of the
//! five costs and it is the quiet one — `ls` on a directory of forty thousand
//! files does not fail, does not warn, and does not look like a defect. It
//! produces a caller that spent its whole window on names it did not ask about
//! and then forgot what it was doing, which reads from the outside as a stupid
//! agent rather than as a system that handed one too much.
//!
//! ## Why a cursor and not an offset
//!
//! An offset is a promise about a collection that nobody is holding still. Ask
//! for rows 200–400 of a directory, have three files deleted in between, and an
//! offset silently skips three rows that were never sent — no error, no gap, and
//! the caller concludes those files do not exist. A key-based cursor cannot do
//! that: it names the last row that was sent, and the next page is whatever
//! sorts after it, so an insertion or a deletion changes *what* comes next and
//! never *whether* something is quietly stepped over.
//!
//! What a key cursor cannot hide, and must therefore say, is that the collection
//! moved underneath it — rows inserted before the cursor will not be seen at
//! all. That is [`Continuity::Changed`], and it travels in the same object as
//! the rows for the reason `FS-en-Grafo` decrees for the index: a caveat
//! delivered separately from the data is a caveat that gets dropped.
//!
//! ## What is refused rather than guessed
//!
//! Both of the ways this can be asked wrongly produce an error and never a
//! plausible page, which is rule 9 — a corrupt input gets the cautious answer,
//! not the fast one:
//!
//! - a cursor this module did not write, including one from a future version of
//!   the format, because reading it under today's rules is how a caller silently
//!   gets the wrong window;
//! - rows that are not in ascending key order, because a cursor into them would
//!   name a position that means something different on the next call.

use sha2::{Digest, Sha256};

/// How many rows go out when nobody said.
///
/// Two hundred, and the number is a judgement about context rather than about
/// directories: a listing of two hundred names is a few thousand tokens, which a
/// caller can afford to be wrong about. It is deliberately not "as many as fit",
/// because what fits is a property of the reader and this side cannot know it.
pub const DEFAULT_LIMIT: usize = 200;

/// What the caller asked for: how much, and where to resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub limit: usize,
    pub after: Option<Cursor>,
}

impl Default for Asked {
    fn default() -> Self {
        Self {
            limit: DEFAULT_LIMIT,
            after: None,
        }
    }
}

/// A place in a collection, as the caller hands it back.
///
/// Holds the key of the last row that was sent and a stamp of the whole
/// collection at that moment. The key is what resumes; the stamp is only ever
/// used to tell the caller whether anything moved, and never to refuse — a
/// cursor into a directory that changed is still the honest place to continue
/// from, and refusing it would leave a caller with no way to finish reading a
/// tree that somebody is working in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    stamp: String,
    key: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum Cut {
    /// Not a cursor this module wrote, or one written by a version that does not
    /// exist yet.
    #[error("that cursor was not written by this machine")]
    BadCursor,
    /// The rows are not ascending by key, so no cursor into them would mean the
    /// same thing twice. A caller bug, reported rather than papered over.
    #[error("rows arrived out of key order, so a cursor into them would not be stable")]
    Unordered,
}

impl Cursor {
    /// The text a caller hands back, and the only shape accepted.
    ///
    /// `w1` is the version and it is checked, not skipped. A token written by a
    /// later format read under today's rules would resume somewhere plausible
    /// and wrong, which is precisely the failure rule 9 is about.
    pub fn parse(text: &str) -> Result<Self, Cut> {
        let mut parts = text.split('.');
        match parts.next() {
            Some("w1") => {}
            _ => return Err(Cut::BadCursor),
        }
        let stamp = parts.next().ok_or(Cut::BadCursor)?;
        let key = parts.next().ok_or(Cut::BadCursor)?;
        if parts.next().is_some() || stamp.len() != STAMP_LENGTH {
            return Err(Cut::BadCursor);
        }
        Ok(Self {
            stamp: stamp.to_string(),
            key: from_hex(key)?,
        })
    }

    fn write(stamp: &str, key: &[u8]) -> String {
        let mut out = String::from("w1.");
        out.push_str(stamp);
        out.push('.');
        for byte in key {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }
}

/// Whether the collection is the one the cursor was taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Continuity {
    /// No cursor was given: this is the beginning.
    FirstPage,
    /// A cursor was given and the collection is byte-for-byte the one it came
    /// from.
    Unchanged,
    /// A cursor was given and something was added, removed or renamed since.
    /// The page is still correct — it resumes after the same key — but rows that
    /// appeared *before* that key will not be in it.
    Changed,
}

impl Continuity {
    /// The word a program matches on. Stable, never translated, and never a
    /// sentence: the prose around it will be reworded and anything parsing it
    /// would break the first time somebody improved it.
    pub fn word(self) -> &'static str {
        match self {
            Continuity::FirstPage => "first_page",
            Continuity::Unchanged => "unchanged",
            Continuity::Changed => "changed",
        }
    }
}

/// One page, and everything needed to know it is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub rows: Vec<T>,
    /// How many there are in the whole collection, not how many were sent.
    /// Without it a caller cannot tell a directory of nine from the first nine
    /// of nine thousand, and there is no cheaper way for it to find out.
    pub total: usize,
    /// How many rows were skipped to reach this page, so `before + rows.len()`
    /// says where the caller is inside `total`.
    pub before: usize,
    /// Whether anything was left behind.
    ///
    /// Its own field and not `next.is_some()`, because those two are different
    /// questions and there is a case where they disagree: a caller asking for
    /// none of it to find out how many there are gets `more` and no cursor,
    /// since nothing was sent and so no row can name the place to resume from.
    /// Folding them together would answer "there is no more" to a caller that
    /// had just been told there are forty thousand.
    pub more: bool,
    /// The token for the next page, or `None` when there is no *new* position to
    /// hand over. `None` is an answer and is written out as `null`: a key that
    /// only appears when there is more is a key nobody wrote the branch for.
    pub next: Option<String>,
    pub continuity: Continuity,
}

/// Cut `rows` down to what was asked for.
///
/// `key` must be ascending across `rows` and is what the cursor names. For a
/// directory that is the sort the listing already has; for anything read out of
/// the index it is a sort the caller has to do, which is why handing them over
/// unordered is an error rather than a shrug.
pub fn page<T>(rows: Vec<T>, key: impl Fn(&T) -> Vec<u8>, asked: &Asked) -> Result<Page<T>, Cut> {
    let keys: Vec<Vec<u8>> = rows.iter().map(&key).collect();
    if keys.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(Cut::Unordered);
    }

    let stamp = stamp_of(&keys);
    let total = rows.len();

    let (before, continuity) = match &asked.after {
        None => (0, Continuity::FirstPage),
        Some(cursor) => {
            // Everything up to and including the key the cursor names, whether
            // or not that row is still there. Counting instead of searching for
            // the row itself is what makes a deleted anchor a non-event: the
            // position is defined by the ordering, not by the row surviving.
            let skipped = keys.partition_point(|candidate| candidate <= &cursor.key);
            (
                skipped,
                if cursor.stamp == stamp {
                    Continuity::Unchanged
                } else {
                    Continuity::Changed
                },
            )
        }
    };

    let mut rows: Vec<T> = rows.into_iter().skip(before).take(asked.limit).collect();
    let sent = rows.len();
    let more = before + sent < total;
    let next = if more && sent > 0 {
        // The key of the last row actually sent. Taken from `keys` rather than
        // recomputed, so that a `key` function that is not pure cannot produce a
        // cursor pointing at a row nobody was given.
        Some(Cursor::write(&stamp, &keys[before + sent - 1]))
    } else {
        None
    };
    rows.shrink_to_fit();

    Ok(Page {
        rows,
        total,
        before,
        more,
        next,
        continuity,
    })
}

/// How many hex characters of the collection's digest travel in a cursor.
///
/// Sixteen, which is not a security boundary: the stamp only decides whether the
/// caller is told the collection moved, and the failure a collision produces is
/// being told `unchanged` about a directory that changed. Keeping the whole
/// digest would put sixty-four characters in every cursor for that.
const STAMP_LENGTH: usize = 16;

/// A digest of the keys, in order.
///
/// Of the keys and not of the count, because a rename changes neither the number
/// of entries nor the total and is exactly the change a caller most needs to
/// hear about — a page resumed across one silently describes two different
/// directories.
fn stamp_of(keys: &[Vec<u8>]) -> String {
    let mut hasher = Sha256::new();
    for key in keys {
        // The length prefix is what stops `["ab","c"]` and `["a","bc"]` from
        // hashing the same, which would report a rename as no change at all.
        hasher.update((key.len() as u64).to_le_bytes());
        hasher.update(key);
    }
    hasher
        .finalize()
        .iter()
        .take(STAMP_LENGTH / 2)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn from_hex(text: &str) -> Result<Vec<u8>, Cut> {
    if !text.len().is_multiple_of(2) {
        return Err(Cut::BadCursor);
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).map_err(|_| Cut::BadCursor))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows whose key is the row itself, which is the simplest ascending set.
    fn letters(count: usize) -> Vec<String> {
        (0..count).map(|n| format!("{n:06}")).collect()
    }

    fn by_bytes(row: &String) -> Vec<u8> {
        row.as_bytes().to_vec()
    }

    fn asking(limit: usize, after: Option<&str>) -> Asked {
        Asked {
            limit,
            after: after.map(|text| Cursor::parse(text).expect("a cursor we wrote")),
        }
    }

    #[test]
    fn everything_fitting_reports_no_next_page() {
        let page = page(letters(5), by_bytes, &asking(10, None)).unwrap();
        assert_eq!(page.rows.len(), 5);
        assert_eq!(page.total, 5);
        // `null` and not a token, because a caller that follows a cursor it was
        // handed at the end of a collection loops forever asking for nothing.
        assert!(page.next.is_none());
        assert!(!page.more);
    }

    #[test]
    fn a_long_collection_arrives_cut_with_the_whole_count_said() {
        let page = page(letters(40_000), by_bytes, &asking(200, None)).unwrap();
        assert_eq!(page.rows.len(), 200);
        // The number that makes the cut honest. Without it this answer and a
        // directory of exactly two hundred files are the same answer, and the
        // caller has no cheap way to find out which one it got.
        assert_eq!(page.total, 40_000);
        assert!(page.more);
    }

    #[test]
    fn the_pages_together_are_the_whole_collection_and_nothing_twice() {
        let all = letters(450);
        let mut seen: Vec<String> = Vec::new();
        let mut after: Option<String> = None;

        loop {
            let asked = Asked {
                limit: 100,
                after: after.as_deref().map(|t| Cursor::parse(t).unwrap()),
            };
            let page = page(all.clone(), by_bytes, &asked).unwrap();
            seen.extend(page.rows.iter().cloned());
            match page.next {
                Some(token) => after = Some(token),
                None => break,
            }
        }

        assert_eq!(seen, all, "paging lost or repeated rows");
    }

    #[test]
    fn a_row_deleted_behind_the_cursor_does_not_make_the_next_page_skip_one() {
        // The whole reason this is not an offset. With `skip(200)` the deletion
        // of one earlier row shifts everything up by one, and the row that was
        // at 200 is never sent to anybody — no error, no gap, and a caller that
        // concludes the file is not there.
        let all = letters(10);
        let first = page(all.clone(), by_bytes, &asking(5, None)).unwrap();
        let token = first.next.clone().unwrap();

        let mut shortened = all.clone();
        shortened.remove(0);

        let second = page(shortened, by_bytes, &asking(5, Some(&token))).unwrap();
        assert_eq!(second.rows, all[5..].to_vec());
    }

    #[test]
    fn a_cursor_whose_own_row_is_gone_still_resumes_after_it() {
        let all = letters(10);
        let first = page(all.clone(), by_bytes, &asking(5, None)).unwrap();
        let token = first.next.clone().unwrap();

        // The row the cursor names is the one that disappears. Searching for it
        // would find nothing and leave the caller with a choice between starting
        // over and stopping; counting past it has neither problem.
        let mut without = all.clone();
        without.remove(4);

        let second = page(without, by_bytes, &asking(5, Some(&token))).unwrap();
        assert_eq!(second.rows, all[5..].to_vec());
    }

    #[test]
    fn a_collection_that_moved_says_so_in_the_same_answer_as_the_rows() {
        let all = letters(10);
        let token = page(all.clone(), by_bytes, &asking(5, None))
            .unwrap()
            .next
            .unwrap();

        let mut changed = all.clone();
        changed.remove(0);
        let resumed = page(changed, by_bytes, &asking(5, Some(&token))).unwrap();
        assert_eq!(resumed.continuity, Continuity::Changed);

        // And the control, without which "changed" would be indistinguishable
        // from a stamp that always disagrees with itself.
        let untouched = page(all, by_bytes, &asking(5, Some(&token))).unwrap();
        assert_eq!(untouched.continuity, Continuity::Unchanged);
    }

    #[test]
    fn a_rename_that_keeps_the_count_is_still_a_change() {
        let all = letters(10);
        let token = page(all.clone(), by_bytes, &asking(5, None))
            .unwrap()
            .next
            .unwrap();

        let mut renamed = all.clone();
        renamed[9] = "9999".to_string();

        // A stamp over the count would call this unchanged, and a caller paging
        // through would describe two different directories as one.
        let page = page(renamed, by_bytes, &asking(5, Some(&token))).unwrap();
        assert_eq!(page.continuity, Continuity::Changed);
    }

    #[test]
    fn the_first_page_says_it_is_the_first_and_not_that_nothing_changed() {
        // Three states, and none of them inferred from the absence of another.
        // "No cursor was given" is not evidence about the collection.
        let page = page(letters(3), by_bytes, &asking(2, None)).unwrap();
        assert_eq!(page.continuity, Continuity::FirstPage);
    }

    #[test]
    fn a_cursor_from_a_format_that_does_not_exist_yet_is_refused() {
        // Rule 9. Read under today's rules it would resume somewhere plausible
        // and wrong, which is worse than saying no.
        assert!(matches!(
            Cursor::parse("w2.0011223344556677.61"),
            Err(Cut::BadCursor)
        ));
        assert!(matches!(Cursor::parse("nonsense"), Err(Cut::BadCursor)));
        assert!(matches!(Cursor::parse("w1.short.61"), Err(Cut::BadCursor)));
        assert!(matches!(
            Cursor::parse("w1.0011223344556677.6"),
            Err(Cut::BadCursor)
        ));
    }

    #[test]
    fn rows_out_of_order_are_refused_rather_than_paged_wrongly() {
        let jumbled = vec!["b".to_string(), "a".to_string()];
        assert!(matches!(
            page(jumbled, by_bytes, &asking(1, None)),
            Err(Cut::Unordered)
        ));
    }

    #[test]
    fn asking_for_none_of_it_answers_the_count_and_sends_nothing() {
        // A caller that only wants to know how big something is should not have
        // to receive any of it to find out.
        let page = page(letters(1000), by_bytes, &asking(0, None)).unwrap();
        assert!(page.rows.is_empty());
        assert_eq!(page.total, 1000);
        // And there is still a way in, or "how many" would be a dead end.
        assert!(page.more);
    }

    #[test]
    fn a_key_with_bytes_that_are_not_text_survives_the_round_trip() {
        // Names on Linux are bytes. A cursor that could only carry UTF-8 would
        // stop paging at the first file somebody made with a broken name, and
        // the caller would have no way past it.
        let rows: Vec<Vec<u8>> = vec![vec![0x61], vec![0xff, 0xfe], vec![0xff, 0xff]];
        let first = page(
            rows.clone(),
            |row| row.clone(),
            &Asked {
                limit: 2,
                after: None,
            },
        )
        .unwrap();
        let token = first.next.unwrap();

        let second = page(
            rows.clone(),
            |row| row.clone(),
            &Asked {
                limit: 2,
                after: Some(Cursor::parse(&token).unwrap()),
            },
        )
        .unwrap();
        assert_eq!(second.rows, vec![vec![0xff, 0xff]]);
    }
}
