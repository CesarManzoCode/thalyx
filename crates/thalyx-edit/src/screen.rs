//! Where the cursor is, what part of the file is on screen, and what a key does.
//!
//! Everything here is pure: text and a key in, new state out. No terminal is
//! opened, nothing is printed, and the row of characters to draw is *returned*
//! rather than written. That is the same split `thalyx-term` made for the line
//! editor and it is made for the same reason — the parts worth testing are
//! **where the cursor lands** and **what ends up on screen**, and those answers
//! must not require a terminal to ask. Drawing lives in `thalyx-cli/src/edit.rs`.
//!
//! ## The arithmetic that is worth this much care
//!
//! A screen editor is four numbers that must agree: which line the cursor is on,
//! which column, which line is at the top of the screen, and which column is at
//! the left. Every visible bug in an editor is two of those four disagreeing —
//! the cursor drawn where the character is not, a scroll that jumps, a `End` on
//! a line shorter than the last one that lands past the text.
//!
//! ## Columns are characters, and that is a stated limit
//!
//! A column here is one `char`. For Spanish that is exactly right and it is the
//! bug `thalyx-term` documents: `ñ` is two bytes, and a cursor counted in bytes
//! lands inside it. For a CJK ideograph or an emoji, which occupy two terminal
//! cells, it is wrong — the text is preserved perfectly and the cursor is drawn
//! one cell to the left of where it belongs. Stated rather than discovered: the
//! fix is a width table, it is real work, and no file this machine edits today
//! needs it.

use crate::{Span, Text};
use thalyx_term::Key;

/// Where in the text the person is, counted from zero.
///
/// Zero-based, unlike [`Span`], and the difference is deliberate rather than
/// sloppy. A span is *typed by a person* and people count from one; a cursor is
/// an index into a `Vec` and every arithmetic on it is off by one if it is not.
/// The conversion happens once, in [`Cursor::line_number`], instead of at every
/// use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    pub line: usize,
    pub column: usize,
}

impl Cursor {
    /// The line as a person and every error message counts it.
    pub fn line_number(self) -> usize {
        self.line + 1
    }
}

/// What part of the text the screen is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub top: usize,
    pub left: usize,
    pub height: usize,
    pub width: usize,
}

impl Viewport {
    pub fn of(height: usize, width: usize) -> Self {
        Self {
            top: 0,
            left: 0,
            // A viewport of zero rows has no line the cursor can be on, and
            // every `%` and every subtraction below it divides by nothing. One
            // is the smallest screen that is still a screen.
            height: height.max(1),
            width: width.max(1),
        }
    }
}

/// One row of the screen, ready to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The line's number, or `None` for a row past the end of the file.
    ///
    /// `None` rather than a blank string, because "there is no line here" and
    /// "there is a line here and it is empty" are different facts and an editor
    /// that shows them the same way tells a person their file is longer than it
    /// is.
    pub number: Option<usize>,
    pub text: String,
}

/// The whole screen, and where the terminal's cursor must be put on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub rows: Vec<Row>,
    /// Which row of [`Self::rows`] the cursor sits on, counted from zero.
    pub cursor_row: usize,
    /// Which column of that row, counted from zero and already adjusted for
    /// horizontal scrolling.
    pub cursor_column: usize,
}

/// What the caller must do about a key, after the key has been applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    /// Redraw; nothing else.
    Moved,
    /// The text changed. Redraw, and the file is now unsaved.
    Changed,
    /// Ctrl-O: write it.
    Save,
    /// Ctrl-X: leave. Whether that needs a confirmation is the caller's
    /// question, because only the caller can ask one.
    Leave,
    /// A key with no meaning here. Returned rather than folded into `Moved` so
    /// the caller can skip a redraw it does not need.
    Nothing,
}

/// A person's position in a file, and the rules for moving it.
#[derive(Debug, Clone)]
pub struct Editing {
    pub cursor: Cursor,
    pub view: Viewport,
    /// The column the person *wants*, which is not always the one they are on.
    ///
    /// Moving down from the end of a long line onto a short one puts the cursor
    /// at the short line's end; moving down again onto a long one must go back
    /// out to where they were. Without this the cursor walks left down a
    /// staircase and never comes back, which is the single most noticeable way
    /// a hand-written editor feels wrong.
    wanted_column: usize,
}

impl Editing {
    pub fn new(view: Viewport) -> Self {
        Self {
            cursor: Cursor::default(),
            view,
            wanted_column: 0,
        }
    }

    /// Put the cursor on a given line, as a person numbers them.
    pub fn go_to(&mut self, text: &Text, line: usize) {
        self.cursor.line = line.saturating_sub(1).min(text.count() - 1);
        self.cursor.column = 0;
        self.wanted_column = 0;
        self.follow(text);
    }

    /// Apply one key, and say what the caller has to do about it.
    pub fn press(&mut self, text: &mut Text, key: Key) -> Reaction {
        let reaction = self.apply(text, key);
        // One place, after every key, rather than at each of the fifteen points
        // that move the cursor. A scroll adjustment that is written per movement
        // is a scroll adjustment that will be missing from one of them.
        if !matches!(reaction, Reaction::Nothing) {
            self.follow(text);
        }
        reaction
    }

    fn apply(&mut self, text: &mut Text, key: Key) -> Reaction {
        match key {
            Key::Left => {
                if self.cursor.column > 0 {
                    self.cursor.column -= 1;
                } else if self.cursor.line > 0 {
                    // Off the front of a line is onto the end of the one above,
                    // which is where every editor goes and where a person
                    // holding the key expects to end up.
                    self.cursor.line -= 1;
                    self.cursor.column = self.width_of(text, self.cursor.line);
                }
                self.wanted_column = self.cursor.column;
                Reaction::Moved
            }
            Key::Right => {
                if self.cursor.column < self.width_of(text, self.cursor.line) {
                    self.cursor.column += 1;
                } else if self.cursor.line + 1 < text.count() {
                    self.cursor.line += 1;
                    self.cursor.column = 0;
                }
                self.wanted_column = self.cursor.column;
                Reaction::Moved
            }
            Key::Up => {
                if self.cursor.line > 0 {
                    self.cursor.line -= 1;
                    self.settle(text);
                }
                Reaction::Moved
            }
            Key::Down => {
                if self.cursor.line + 1 < text.count() {
                    self.cursor.line += 1;
                    self.settle(text);
                }
                Reaction::Moved
            }
            Key::Home => {
                self.cursor.column = 0;
                self.wanted_column = 0;
                Reaction::Moved
            }
            Key::End => {
                self.cursor.column = self.width_of(text, self.cursor.line);
                self.wanted_column = self.cursor.column;
                Reaction::Moved
            }
            Key::PageUp => {
                self.cursor.line = self.cursor.line.saturating_sub(self.view.height);
                self.settle(text);
                Reaction::Moved
            }
            Key::PageDown => {
                self.cursor.line = (self.cursor.line + self.view.height).min(text.count() - 1);
                self.settle(text);
                Reaction::Moved
            }
            Key::Char(c) => {
                self.put(text, c);
                Reaction::Changed
            }
            // Tab is a character in a file and not a completion request. In the
            // line editor it completes a filename; here, completing anything
            // would make it impossible to type the one character that a
            // Makefile's syntax is made of.
            Key::Tab => {
                self.put(text, '\t');
                Reaction::Changed
            }
            Key::Enter => {
                self.split(text);
                Reaction::Changed
            }
            Key::Backspace => self.rub_out(text),
            Key::Delete => self.rub_forward(text),
            Key::Ctrl('o') => Reaction::Save,
            Key::Ctrl('x') => Reaction::Leave,
            Key::Ctrl('u') => {
                if text.undo() {
                    // The text under the cursor changed shape, so the cursor may
                    // now be past the end of a line that got shorter.
                    self.clamp(text);
                    Reaction::Changed
                } else {
                    Reaction::Nothing
                }
            }
            Key::Ctrl('k') => self.cut_line(text),
            // Ctrl-C never arrives — `ISIG` is on, so the kernel turns it into a
            // signal. It is matched anyway rather than left to the catch-all,
            // because a reader looking for "what does Ctrl-C do here" must find
            // an answer, and the answer is that this program never sees it.
            Key::Interrupt | Key::EndOfInput | Key::Ctrl(_) | Key::Ignored => Reaction::Nothing,
        }
    }

    fn width_of(&self, text: &Text, line: usize) -> usize {
        text.lines()
            .get(line)
            .map(|l| l.chars().count())
            .unwrap_or(0)
    }

    /// Land on a new line at the column the person wanted, or that line's end.
    fn settle(&mut self, text: &Text) {
        self.cursor.column = self
            .wanted_column
            .min(self.width_of(text, self.cursor.line));
    }

    /// Pull the cursor back inside the text after the text changed under it.
    fn clamp(&mut self, text: &Text) {
        self.cursor.line = self.cursor.line.min(text.count() - 1);
        self.cursor.column = self
            .cursor
            .column
            .min(self.width_of(text, self.cursor.line));
        self.wanted_column = self.cursor.column;
    }

    fn line_text(&self, text: &Text, line: usize) -> String {
        text.lines().get(line).cloned().unwrap_or_default()
    }

    fn put(&mut self, text: &mut Text, c: char) {
        let mut chars: Vec<char> = self.line_text(text, self.cursor.line).chars().collect();
        let at = self.cursor.column.min(chars.len());
        chars.insert(at, c);
        let line = chars.into_iter().collect::<String>();
        let _ = text.replace(Span::one(self.cursor.line_number()), &line);
        self.cursor.column = at + 1;
        self.wanted_column = self.cursor.column;
    }

    fn split(&mut self, text: &mut Text) {
        let chars: Vec<char> = self.line_text(text, self.cursor.line).chars().collect();
        let at = self.cursor.column.min(chars.len());
        let left: String = chars[..at].iter().collect();
        let right: String = chars[at..].iter().collect();
        // One `replace` with a break in it rather than a replace and an insert.
        // Two operations would be two entries on the undo stack, so one press of
        // Return would take two presses of Ctrl-U to take back.
        let _ = text.replace(
            Span::one(self.cursor.line_number()),
            &format!("{left}\n{right}"),
        );
        self.cursor.line += 1;
        self.cursor.column = 0;
        self.wanted_column = 0;
    }

    fn rub_out(&mut self, text: &mut Text) -> Reaction {
        if self.cursor.column > 0 {
            let mut chars: Vec<char> = self.line_text(text, self.cursor.line).chars().collect();
            chars.remove(self.cursor.column - 1);
            let line: String = chars.into_iter().collect();
            let _ = text.replace(Span::one(self.cursor.line_number()), &line);
            self.cursor.column -= 1;
            self.wanted_column = self.cursor.column;
            return Reaction::Changed;
        }
        if self.cursor.line == 0 {
            // Backspace at the very start of the file. Nothing to join to, and
            // `Nothing` rather than `Changed` so the caller does not mark a file
            // unsaved that nobody changed.
            return Reaction::Nothing;
        }
        let above = self.line_text(text, self.cursor.line - 1);
        let here = self.line_text(text, self.cursor.line);
        let column = above.chars().count();
        let _ = text.replace(
            Span {
                from: self.cursor.line,
                to: self.cursor.line + 1,
            },
            &format!("{above}{here}"),
        );
        self.cursor.line -= 1;
        self.cursor.column = column;
        self.wanted_column = column;
        Reaction::Changed
    }

    fn rub_forward(&mut self, text: &mut Text) -> Reaction {
        let here = self.line_text(text, self.cursor.line);
        let width = here.chars().count();
        if self.cursor.column < width {
            let mut chars: Vec<char> = here.chars().collect();
            chars.remove(self.cursor.column);
            let line: String = chars.into_iter().collect();
            let _ = text.replace(Span::one(self.cursor.line_number()), &line);
            return Reaction::Changed;
        }
        if self.cursor.line + 1 >= text.count() {
            return Reaction::Nothing;
        }
        let below = self.line_text(text, self.cursor.line + 1);
        let _ = text.replace(
            Span {
                from: self.cursor.line_number(),
                to: self.cursor.line_number() + 1,
            },
            &format!("{here}{below}"),
        );
        Reaction::Changed
    }

    fn cut_line(&mut self, text: &mut Text) -> Reaction {
        if text.count() == 1 && self.line_text(text, 0).is_empty() {
            return Reaction::Nothing;
        }
        let _ = text.delete(Span::one(self.cursor.line_number()));
        self.clamp(text);
        Reaction::Changed
    }

    /// Move the viewport the least amount that puts the cursor back on it.
    ///
    /// The least amount, deliberately. Recentring on every scroll is what makes
    /// a screen jump half a page when somebody presses Down once at the bottom,
    /// and a person reading code loses their place every time it happens.
    fn follow(&mut self, text: &Text) {
        if self.cursor.line < self.view.top {
            self.view.top = self.cursor.line;
        } else if self.cursor.line >= self.view.top + self.view.height {
            self.view.top = self.cursor.line + 1 - self.view.height;
        }
        if self.cursor.column < self.view.left {
            self.view.left = self.cursor.column;
        } else if self.cursor.column >= self.view.left + self.view.width {
            self.view.left = self.cursor.column + 1 - self.view.width;
        }
        let _ = text;
    }

    /// What the screen should show right now.
    pub fn frame(&self, text: &Text) -> Frame {
        let mut rows = Vec::with_capacity(self.view.height);
        for offset in 0..self.view.height {
            let line = self.view.top + offset;
            match text.lines().get(line) {
                Some(body) => rows.push(Row {
                    number: Some(line + 1),
                    text: body
                        .chars()
                        .skip(self.view.left)
                        .take(self.view.width)
                        .collect(),
                }),
                None => rows.push(Row {
                    number: None,
                    text: String::new(),
                }),
            }
        }
        Frame {
            rows,
            cursor_row: self.cursor.line.saturating_sub(self.view.top),
            cursor_column: self.cursor.column.saturating_sub(self.view.left),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn text(body: &str) -> Text {
        Text::from_str(Path::new("/tmp/notes.txt"), body, None)
    }

    fn editing() -> Editing {
        Editing::new(Viewport::of(4, 20))
    }

    #[test]
    fn walking_down_a_short_line_and_out_the_other_side_returns_to_the_column_wanted() {
        // The staircase bug, which is what `wanted_column` exists to prevent.
        let mut file = text("aaaaaaaaaa\nbb\ncccccccccc\n");
        let mut edit = editing();
        edit.press(&mut file, Key::End);
        assert_eq!(edit.cursor.column, 10);

        edit.press(&mut file, Key::Down);
        assert_eq!(
            edit.cursor.column, 2,
            "the short line has nowhere else to be"
        );

        edit.press(&mut file, Key::Down);
        assert_eq!(
            edit.cursor.column, 10,
            "and the long one gets the column back"
        );
    }

    #[test]
    fn the_left_arrow_at_the_start_of_a_line_lands_at_the_end_of_the_one_above() {
        let mut file = text("uno\ndos\n");
        let mut edit = editing();
        edit.press(&mut file, Key::Down);
        assert_eq!(edit.cursor, Cursor { line: 1, column: 0 });

        edit.press(&mut file, Key::Left);
        assert_eq!(edit.cursor, Cursor { line: 0, column: 3 });
    }

    #[test]
    fn return_in_the_middle_of_a_line_splits_it_and_takes_one_undo_to_put_back() {
        let mut file = text("unodos\n");
        let mut edit = editing();
        for _ in 0..3 {
            edit.press(&mut file, Key::Right);
        }
        edit.press(&mut file, Key::Enter);
        assert_eq!(file.lines(), &["uno", "dos"]);
        assert_eq!(edit.cursor, Cursor { line: 1, column: 0 });

        // One press of Return is one press of Ctrl-U, which is only true because
        // the split is a single operation on the text.
        edit.press(&mut file, Key::Ctrl('u'));
        assert_eq!(file.lines(), &["unodos"]);
    }

    #[test]
    fn backspace_at_the_start_of_a_line_joins_it_to_the_one_above() {
        let mut file = text("uno\ndos\n");
        let mut edit = editing();
        edit.press(&mut file, Key::Down);
        edit.press(&mut file, Key::Backspace);

        assert_eq!(file.lines(), &["unodos"]);
        // And the cursor is at the seam, not at the start — that is the position
        // that lets a person keep typing where they were.
        assert_eq!(edit.cursor, Cursor { line: 0, column: 3 });
    }

    #[test]
    fn backspace_at_the_very_start_of_the_file_changes_nothing_and_says_so() {
        let mut file = text("uno\n");
        let mut edit = editing();
        // `Nothing` and not `Changed`: a file marked unsaved by a keystroke that
        // did nothing is a file a person is asked about on the way out for no
        // reason.
        assert_eq!(edit.press(&mut file, Key::Backspace), Reaction::Nothing);
        assert!(!file.is_modified());
    }

    #[test]
    fn delete_at_the_end_of_a_line_pulls_the_next_one_up() {
        let mut file = text("uno\ndos\n");
        let mut edit = editing();
        edit.press(&mut file, Key::End);
        edit.press(&mut file, Key::Delete);
        assert_eq!(file.lines(), &["unodos"]);
    }

    #[test]
    fn typing_an_accented_letter_puts_it_where_the_cursor_is_and_not_inside_a_byte() {
        let mut file = text("contrasea\n");
        let mut edit = editing();
        for _ in 0..8 {
            edit.press(&mut file, Key::Right);
        }
        edit.press(&mut file, Key::Char('ñ'));
        assert_eq!(file.lines(), &["contraseña"]);
        assert_eq!(edit.cursor.column, 9);

        // And the left arrow steps over the whole letter, not half of it.
        edit.press(&mut file, Key::Left);
        assert_eq!(edit.cursor.column, 8);
    }

    #[test]
    fn the_screen_scrolls_by_one_line_rather_than_jumping_half_a_page() {
        let mut file = text("1\n2\n3\n4\n5\n6\n7\n8\n");
        let mut edit = editing();
        for _ in 0..4 {
            edit.press(&mut file, Key::Down);
        }
        // Four rows of screen, cursor on line 5 of the file: the top moved by
        // exactly one, so the person still sees the three lines they were
        // reading.
        assert_eq!(edit.view.top, 1);
        let frame = edit.frame(&file);
        assert_eq!(frame.rows[0].number, Some(2));
        assert_eq!(frame.cursor_row, 3);
    }

    #[test]
    fn a_row_past_the_end_of_the_file_is_not_an_empty_line() {
        let file = text("uno\n");
        let edit = editing();
        let frame = edit.frame(&file);
        assert_eq!(frame.rows[0].number, Some(1));
        // Three rows of screen left over, and none of them claims to be a line.
        assert_eq!(frame.rows[1].number, None);
        assert_eq!(frame.rows[3].number, None);
    }

    #[test]
    fn a_line_wider_than_the_screen_scrolls_sideways_and_the_cursor_stays_visible() {
        let wide = "x".repeat(50);
        let mut file = text(&format!("{wide}\n"));
        let mut edit = editing();
        edit.press(&mut file, Key::End);

        let frame = edit.frame(&file);
        assert_eq!(edit.view.left, 31, "50 - 20 + 1");
        assert_eq!(frame.rows[0].text.chars().count(), 19);
        assert!(frame.cursor_column < edit.view.width);
    }

    #[test]
    fn cutting_the_only_line_of_an_already_empty_file_does_nothing() {
        let mut file = text("");
        let mut edit = editing();
        assert_eq!(edit.press(&mut file, Key::Ctrl('k')), Reaction::Nothing);
        assert!(!file.is_modified());
    }

    #[test]
    fn cutting_the_last_line_leaves_the_cursor_somewhere_that_exists() {
        let mut file = text("uno\ndos\n");
        let mut edit = editing();
        edit.press(&mut file, Key::Down);
        edit.press(&mut file, Key::Ctrl('k'));
        assert_eq!(file.lines(), &["uno"]);
        // The cursor was on line 2 and there is no line 2 any more.
        assert_eq!(edit.cursor.line, 0);
    }

    #[test]
    fn tab_is_a_character_here_and_not_a_completion_request() {
        let mut file = text("uno\n");
        let mut edit = editing();
        edit.press(&mut file, Key::Home);
        edit.press(&mut file, Key::Tab);
        assert_eq!(file.lines(), &["\tuno"]);
    }

    #[test]
    fn the_keys_the_kernel_eats_are_not_bound_to_anything_here() {
        // If a later version binds Ctrl-S to save, this test fails and the
        // comment above it is why: raw mode leaves `IXON` on, so the byte never
        // arrives and the person concludes the editor cannot save.
        let mut file = text("uno\n");
        let mut edit = editing();
        for eaten in ['s', 'q', 'z', 'c'] {
            assert_eq!(
                edit.press(&mut file, Key::Ctrl(eaten)),
                Reaction::Nothing,
                "Ctrl-{eaten} is taken by the line discipline before Thalyx sees it"
            );
        }
    }
}
