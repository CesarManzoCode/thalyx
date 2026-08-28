//! The line a person is typing, before they press Return.
//!
//! ## Why this exists
//!
//! Until now the session read with `read_line`, which hands over whatever the
//! kernel's own line editor collected. That editor is fine — it is what makes
//! backspace work at a shell — but it is **someone else's**, and on the image
//! there is nothing behind Thalyx to provide it. A person who typed a long path
//! and noticed a mistake in the middle had to delete back to it or start over,
//! and every command they had already run was gone the moment it scrolled away.
//!
//! `Principio-Doble-Ruta.md` promises the human loses no capability by not using
//! the agent. A terminal without arrows and without history is a loss of
//! capability so ordinary that nobody would think to write it down.
//!
//! ## What is here and what is not
//!
//! Everything in this crate is pure: bytes in, state out. No terminal is opened,
//! no mode is set, nothing is printed. That is deliberate — the parts worth
//! testing are *what a keystroke means* and *where the cursor lands*, and those
//! answers must not require a terminal to ask. Putting the terminal in raw mode
//! lives in `thalyx-syscall`, and drawing lives in the CLI.
//!
//! ## Characters, not bytes
//!
//! The line is a `Vec<char>` and not a `String`, and that is the bug this
//! prevents rather than a preference. `ñ` is two bytes; a cursor counted in
//! bytes lands between them, and the left arrow in `contraseña` produces
//! invalid UTF-8 out of a valid word. Spanish is the language this machine is
//! used in, so getting that wrong would be visible on the first day.

/// One thing the person did, already decoded from whatever bytes carried it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Up,
    Down,
    Tab,
    PageUp,
    PageDown,
    /// A control key this crate does not give a name of its own, carried as the
    /// letter it was typed with — `Ctrl('o')` for Ctrl-O.
    ///
    /// Named rather than dropped because the editor needs several of them and
    /// the line editor needs none: one decoder, two callers, and the caller that
    /// has no use for a key ignores it instead of the decoder deciding for both.
    ///
    /// **Which letters are reachable is decided by the kernel, not here.** Raw
    /// mode in `thalyx-syscall` deliberately leaves `ISIG` and `IXON` on, so the
    /// line discipline eats Ctrl-C, Ctrl-\, Ctrl-Z, Ctrl-S and Ctrl-Q before any
    /// byte arrives — a key bound to one of those does nothing, or worse, wedges
    /// the terminal. That is why the editor saves with Ctrl-O and not Ctrl-S.
    Ctrl(char),
    /// Ctrl-C. Abandon the line without running it.
    Interrupt,
    /// Ctrl-D on an empty line, which every terminal has meant "I am done" for
    /// fifty years and which the session reads as `exit`.
    EndOfInput,
    /// Understood as a key press and not as anything to do — an escape sequence
    /// this does not implement, or a control byte with no meaning here.
    ///
    /// A separate variant rather than dropping it silently: a caller that
    /// treated "not understood" as "nothing arrived" would spin, and one that
    /// treated it as a character would print `[A` when somebody pressed a key
    /// this version does not know.
    Ignored,
}

/// Turn a run of bytes into keys, saying how many bytes were consumed.
///
/// Returns `None` when the bytes so far are a **prefix** of something longer —
/// an escape that has not yet been followed by its letter, or the first byte of
/// a multi-byte character. That is the whole reason this reports a length: the
/// caller must know to read more rather than to guess, and guessing is how a
/// slow terminal turns one arrow key into three garbage characters.
pub fn decode(bytes: &[u8]) -> Option<(Key, usize)> {
    let first = *bytes.first()?;

    match first {
        b'\r' | b'\n' => Some((Key::Enter, 1)),
        b'\t' => Some((Key::Tab, 1)),
        0x03 => Some((Key::Interrupt, 1)),
        0x04 => Some((Key::EndOfInput, 1)),
        // Both, because terminals disagree about which one backspace sends and a
        // person whose backspace does nothing concludes the machine is broken.
        0x7f | 0x08 => Some((Key::Backspace, 1)),
        0x01 => Some((Key::Home, 1)),
        0x05 => Some((Key::End, 1)),
        0x1b => decode_escape(bytes),
        // Every other control byte arrives named. It is never a character: passed
        // through as one it would be written into the line and then into a
        // filename, which is the bug this arm has always existed to stop.
        _ if first < 0x20 => {
            // 0x01 is Ctrl-A, so the byte is its distance from `a`. Lowercase,
            // always, because a terminal sends the same byte for Ctrl-O and
            // Ctrl-Shift-O and a caller matching on `'O'` would never fire.
            let letter = (b'a' + first - 1) as char;
            Some((Key::Ctrl(letter), 1))
        }
        _ => decode_char(bytes),
    }
}

fn decode_escape(bytes: &[u8]) -> Option<(Key, usize)> {
    // A lone escape is a prefix until proven otherwise. Answering `Ignored` here
    // would eat the `[A` of an arrow key and print it as text.
    let second = *bytes.get(1)?;
    if second != b'[' && second != b'O' {
        return Some((Key::Ignored, 2));
    }

    match bytes.get(2)? {
        b'A' => Some((Key::Up, 3)),
        b'B' => Some((Key::Down, 3)),
        b'C' => Some((Key::Right, 3)),
        b'D' => Some((Key::Left, 3)),
        b'H' => Some((Key::Home, 3)),
        b'F' => Some((Key::End, 3)),
        // `ESC [ 3 ~` is Delete, and the `~` may not have arrived yet.
        b'3' => match bytes.get(3) {
            Some(b'~') => Some((Key::Delete, 4)),
            Some(_) => Some((Key::Ignored, 4)),
            None => None,
        },
        // `ESC [ 5 ~` and `ESC [ 6 ~`. A screenful at a time is the difference
        // between reading a file and scrolling through it one line at a time,
        // and on the image there is no mouse to do it another way.
        b'5' | b'6' => match bytes.get(3) {
            Some(b'~') => {
                let key = if bytes[2] == b'5' {
                    Key::PageUp
                } else {
                    Key::PageDown
                };
                Some((key, 4))
            }
            Some(_) => Some((Key::Ignored, 4)),
            None => None,
        },
        b'1' | b'4' | b'7' | b'8' => match bytes.get(3) {
            Some(b'~') => {
                let key = if matches!(bytes[2], b'1' | b'7') {
                    Key::Home
                } else {
                    Key::End
                };
                Some((key, 4))
            }
            Some(_) => Some((Key::Ignored, 4)),
            None => None,
        },
        _ => Some((Key::Ignored, 3)),
    }
}

fn decode_char(bytes: &[u8]) -> Option<(Key, usize)> {
    // How many bytes this character claims, read from its first one.
    let wide = match bytes[0] {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        // A continuation byte with nothing in front of it. Not a prefix of
        // anything — the stream is out of step, and saying so as `Ignored`
        // resynchronises instead of hanging waiting for bytes that will not fix
        // it.
        _ => return Some((Key::Ignored, 1)),
    };
    if bytes.len() < wide {
        return None;
    }
    match std::str::from_utf8(&bytes[..wide]) {
        Ok(text) => text.chars().next().map(|c| (Key::Char(c), wide)),
        Err(_) => Some((Key::Ignored, 1)),
    }
}

// ────────────────────────────────────────────────────────── the line being typed

/// What the person has typed so far, and where in it they are.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Line {
    text: Vec<char>,
    /// In characters, never in bytes — see the crate docs. Always in
    /// `0..=text.len()`; `len()` means "after the last character".
    cursor: usize,
}

impl Line {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from(text: &str) -> Self {
        let text: Vec<char> = text.chars().collect();
        Self {
            cursor: text.len(),
            text,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn as_string(&self) -> String {
        self.text.iter().collect()
    }

    /// Everything from the start of the line to the cursor.
    ///
    /// What completion works on: a person pressing Tab means "finish what is
    /// under my hand", not "finish the end of the line".
    pub fn before_cursor(&self) -> String {
        self.text[..self.cursor].iter().collect()
    }

    pub fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, text: &str) {
        for c in text.chars() {
            self.insert(c);
        }
    }

    /// Delete the character to the left. Nothing at the start, which is not a
    /// failure — it is what backspace does at the start of a line everywhere.
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.text.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }
}

// ────────────────────────────────────────────────────────── what was typed before

/// The lines already run, walked with the up and down arrows.
///
/// Bounded on purpose. An unbounded history on a machine that stays up for weeks
/// is memory that only grows, and the oldest line is the one nobody wants.
#[derive(Debug, Default)]
pub struct History {
    lines: Vec<String>,
    /// Where the walk currently is. `None` means "not walking" — on the line
    /// being typed now, which is not in `lines` and must not be lost by pressing
    /// Up and then Down.
    at: Option<usize>,
    /// What was on screen when the walk started, so Down past the newest entry
    /// gives it back instead of an empty line.
    interrupted: String,
}

impl History {
    pub const KEPT: usize = 500;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Remember a line that was run.
    pub fn remember(&mut self, line: &str) {
        self.at = None;
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        // A line identical to the one before it is not worth a second slot: it
        // would make Up press twice to go back one command, which reads as the
        // arrow key having missed.
        if self.lines.last().map(String::as_str) == Some(line) {
            return;
        }
        self.lines.push(line.to_string());
        if self.lines.len() > Self::KEPT {
            self.lines.remove(0);
        }
    }

    /// The previous line, or `None` at the oldest one.
    ///
    /// `current` is what is on screen now; it is kept so that walking back and
    /// forward again returns it. Losing a half-typed line to a stray Up is a
    /// small thing that feels like the machine throwing work away.
    pub fn back(&mut self, current: &str) -> Option<String> {
        if self.lines.is_empty() {
            return None;
        }
        let next = match self.at {
            None => {
                self.interrupted = current.to_string();
                self.lines.len() - 1
            }
            Some(0) => return None,
            Some(at) => at - 1,
        };
        self.at = Some(next);
        Some(self.lines[next].clone())
    }

    /// The next line, or what was being typed when the walk started.
    pub fn forward(&mut self) -> Option<String> {
        let at = self.at?;
        if at + 1 >= self.lines.len() {
            self.at = None;
            return Some(std::mem::take(&mut self.interrupted));
        }
        self.at = Some(at + 1);
        Some(self.lines[at + 1].clone())
    }
}

// ─────────────────────────────────────────────────────────────────── finishing it

/// What Tab did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion {
    /// Nothing matched. The caller does nothing, loudly or quietly, but does not
    /// pretend something was completed.
    None,
    /// Exactly one match: this is what to append to what was already typed.
    One(String),
    /// Several. The shared beginning is appended — which is what makes Tab worth
    /// pressing on a directory of forty files — and the choices are shown.
    Many {
        shared: String,
        choices: Vec<String>,
    },
}

/// Finish a partly typed word against the things it could be.
///
/// `typed` is the fragment under the cursor and `candidates` is everything that
/// could follow it. Whoever calls this decides what the candidates are — verbs
/// at the start of a line, file names after a verb — because that is a decision
/// about the session and not about text.
pub fn complete(typed: &str, candidates: &[String]) -> Completion {
    let matched: Vec<&String> = candidates
        .iter()
        .filter(|candidate| candidate.starts_with(typed))
        .collect();

    match matched.len() {
        0 => Completion::None,
        1 => Completion::One(matched[0][typed.len()..].to_string()),
        _ => {
            let shared = shared_start(&matched);
            Completion::Many {
                shared: shared[typed.len()..].to_string(),
                choices: matched.into_iter().cloned().collect(),
            }
        }
    }
}

/// The longest beginning every candidate agrees on.
///
/// Counted in characters. A prefix cut at a byte boundary can split `ñ`, and the
/// half that reaches the line is not a character — the same bug the `Line` type
/// exists to avoid, in the one place it would arrive from a filename instead of
/// from a keystroke.
fn shared_start(candidates: &[&String]) -> String {
    let first: Vec<char> = candidates[0].chars().collect();
    let mut end = first.len();
    for candidate in &candidates[1..] {
        let mut agreed = 0;
        for (a, b) in first.iter().zip(candidate.chars()) {
            if *a != b {
                break;
            }
            agreed += 1;
        }
        end = end.min(agreed);
    }
    first[..end].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ────────────────────────────────────────────────────────── reading keys

    #[test]
    fn the_four_arrows_arrive_as_arrows_and_not_as_text() {
        // The failure this prevents is the visible one: without it, pressing Up
        // types `^[[A` into the line.
        assert_eq!(decode(b"\x1b[A"), Some((Key::Up, 3)));
        assert_eq!(decode(b"\x1b[B"), Some((Key::Down, 3)));
        assert_eq!(decode(b"\x1b[C"), Some((Key::Right, 3)));
        assert_eq!(decode(b"\x1b[D"), Some((Key::Left, 3)));
    }

    #[test]
    fn half_an_escape_sequence_asks_for_more_instead_of_guessing() {
        // A slow terminal delivers an arrow key in pieces. Guessing here is how
        // one keystroke becomes three garbage characters on screen.
        assert_eq!(decode(b"\x1b"), None);
        assert_eq!(decode(b"\x1b["), None);
        assert_eq!(decode(b"\x1b[3"), None);
    }

    #[test]
    fn both_bytes_that_terminals_send_for_backspace_are_backspace() {
        // Terminals disagree, and a person whose backspace does nothing decides
        // the machine is broken rather than that a byte differed.
        assert_eq!(decode(b"\x7f"), Some((Key::Backspace, 1)));
        assert_eq!(decode(b"\x08"), Some((Key::Backspace, 1)));
    }

    #[test]
    fn an_accented_letter_arrives_as_one_key_and_not_as_two() {
        // Two bytes, one character. This is the language the machine is used in.
        assert_eq!(decode("ñ".as_bytes()), Some((Key::Char('ñ'), 2)));
        assert_eq!(decode("é".as_bytes()), Some((Key::Char('é'), 2)));
    }

    #[test]
    fn the_first_byte_of_an_accented_letter_asks_for_the_second() {
        let bytes = "ñ".as_bytes();
        assert_eq!(decode(&bytes[..1]), None);
    }

    #[test]
    fn a_control_byte_arrives_named_and_never_as_a_character() {
        // The claim is not that it is discarded — the editor binds several of
        // these — it is that it can never reach the line, and from there a
        // filename. `Ctrl` is not something `Line::insert` is ever called with.
        assert_eq!(decode(b"\x0b"), Some((Key::Ctrl('k'), 1)));
        assert_eq!(decode(b"\x0f"), Some((Key::Ctrl('o'), 1)));
        assert_eq!(decode(b"\x18"), Some((Key::Ctrl('x'), 1)));
    }

    #[test]
    fn the_keys_the_line_discipline_eats_are_still_decoded_as_themselves() {
        // Ctrl-S and Ctrl-Q do arrive here when something else turned flow
        // control off, and this crate must not pretend otherwise. What it must
        // not do is let the editor *bind* them, which is a fact about raw mode
        // written where the binding is made.
        assert_eq!(decode(b"\x13"), Some((Key::Ctrl('s'), 1)));
        assert_eq!(decode(b"\x11"), Some((Key::Ctrl('q'), 1)));
    }

    #[test]
    fn a_screenful_at_a_time_arrives_as_its_own_key() {
        assert_eq!(decode(b"\x1b[5~"), Some((Key::PageUp, 4)));
        assert_eq!(decode(b"\x1b[6~"), Some((Key::PageDown, 4)));
        // And the `~` that has not arrived yet is a prefix, not a key.
        assert_eq!(decode(b"\x1b[5"), None);
    }

    #[test]
    fn ctrl_c_and_ctrl_d_are_not_characters() {
        assert_eq!(decode(b"\x03"), Some((Key::Interrupt, 1)));
        assert_eq!(decode(b"\x04"), Some((Key::EndOfInput, 1)));
    }

    // ───────────────────────────────────────────────────────── editing a line

    #[test]
    fn a_letter_can_be_fixed_in_the_middle_without_retyping_the_rest() {
        // The whole reason this crate exists: before it, the only way to fix a
        // character in the middle was to delete back to it.
        let mut line = Line::from("documentos");
        for _ in 0..4 {
            line.left();
        }
        line.backspace();
        line.insert('E');
        assert_eq!(line.as_string(), "documEntos");
    }

    #[test]
    fn the_cursor_counts_characters_and_never_lands_inside_one() {
        let mut line = Line::from("contraseña");
        line.end();
        line.backspace();
        assert_eq!(line.as_string(), "contraseñ");
        // The second one is the whole test: `ñ` is two bytes, so a cursor
        // counted in bytes would delete half of it and leave something that is
        // not text at all.
        line.backspace();
        assert_eq!(line.as_string(), "contrase");
    }

    #[test]
    fn backspace_at_the_start_does_nothing_rather_than_failing() {
        let mut line = Line::from("abc");
        line.home();
        line.backspace();
        assert_eq!(line.as_string(), "abc");
        assert_eq!(line.cursor(), 0);
    }

    #[test]
    fn the_cursor_cannot_be_walked_off_either_end() {
        let mut line = Line::from("ab");
        for _ in 0..10 {
            line.left();
        }
        assert_eq!(line.cursor(), 0);
        for _ in 0..10 {
            line.right();
        }
        assert_eq!(line.cursor(), 2);
    }

    #[test]
    fn home_and_end_reach_both_ends_of_an_accented_line() {
        let mut line = Line::from("añb");
        line.home();
        assert_eq!(line.cursor(), 0);
        line.end();
        // Three characters, five bytes. The distinction is the point.
        assert_eq!(line.cursor(), 3);
    }

    // ────────────────────────────────────────────────────────────── history

    #[test]
    fn the_last_command_comes_back_with_one_press_of_up() {
        let mut history = History::new();
        history.remember("ls -a");
        assert_eq!(history.back(""), Some("ls -a".to_string()));
    }

    #[test]
    fn walking_back_and_forward_again_returns_the_half_typed_line() {
        let mut history = History::new();
        history.remember("ls");

        assert_eq!(history.back("cd Doc"), Some("ls".to_string()));
        // Losing a half-typed line to a stray Up is small and feels like the
        // machine throwing work away.
        assert_eq!(history.forward(), Some("cd Doc".to_string()));
    }

    #[test]
    fn the_same_command_twice_takes_one_slot_and_not_two() {
        let mut history = History::new();
        history.remember("ls");
        history.remember("ls");
        // Two slots would make Up need two presses to go back one command, which
        // reads as the arrow key having missed.
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn blank_lines_are_not_remembered() {
        let mut history = History::new();
        history.remember("   ");
        history.remember("");
        assert!(history.is_empty());
    }

    #[test]
    fn up_at_the_oldest_line_stays_there_instead_of_wrapping_around() {
        let mut history = History::new();
        history.remember("uno");
        history.remember("dos");

        assert_eq!(history.back(""), Some("dos".to_string()));
        assert_eq!(history.back(""), Some("uno".to_string()));
        // Wrapping to the newest would look like the history reordering itself.
        assert_eq!(history.back(""), None);
    }

    #[test]
    fn history_stops_growing_and_drops_the_oldest_first() {
        let mut history = History::new();
        for n in 0..History::KEPT + 10 {
            history.remember(&format!("comando {n}"));
        }
        assert_eq!(history.len(), History::KEPT);
        assert_eq!(
            history.back(""),
            Some(format!("comando {}", History::KEPT + 9))
        );
    }

    #[test]
    fn down_without_having_gone_up_does_nothing() {
        let mut history = History::new();
        history.remember("ls");
        assert_eq!(history.forward(), None);
    }

    // ─────────────────────────────────────────────────────────── finishing it

    fn named(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn one_match_is_finished_outright() {
        let got = complete("Doc", &named(&["Documentos", "Descargas"]));
        assert_eq!(got, Completion::One("umentos".to_string()));
    }

    #[test]
    fn several_matches_fill_in_as_far_as_they_agree() {
        let got = complete("D", &named(&["Documentos", "Descargas", "Datos"]));
        match got {
            // They agree on nothing past `D`, so nothing is appended and the
            // choices are shown. Appending a guess here would put a name on the
            // line that the person did not choose.
            Completion::Many { shared, choices } => {
                assert_eq!(shared, "");
                assert_eq!(choices.len(), 3);
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn the_shared_beginning_is_what_makes_tab_worth_pressing() {
        let got = complete("doc", &named(&["documentos-2024", "documentos-2025"]));
        match got {
            Completion::Many { shared, .. } => assert_eq!(shared, "umentos-202"),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_shared_beginning_is_never_cut_through_the_middle_of_a_letter() {
        // Both share `añ`. Counted in bytes the shared prefix could stop between
        // the two bytes of `ñ`, and half a character would reach the line.
        let got = complete("a", &named(&["añejo", "añil"]));
        match got {
            Completion::Many { shared, .. } => assert_eq!(shared, "ñ"),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn nothing_matching_completes_to_nothing_rather_than_to_anything() {
        assert_eq!(complete("zz", &named(&["Documentos"])), Completion::None);
        // An empty candidate list is the ordinary case in an empty directory,
        // not an error.
        assert_eq!(complete("a", &[]), Completion::None);
    }

    #[test]
    fn completion_works_on_what_is_under_the_cursor_and_not_on_the_whole_line() {
        let mut line = Line::from("cd Documentos/notas");
        // Back over `notas`, the five characters after the slash.
        for _ in 0..5 {
            line.left();
        }
        // A person pressing Tab means "finish what is under my hand".
        assert_eq!(line.before_cursor(), "cd Documentos/");
    }
    /// Why the confirmation on the display says Ctrl-C and not Escape.
    ///
    /// The drawn hint said *«Escape cancela»* from the day the confirmation was
    /// designed until the day it was wired to a keyboard. It cannot work: a bare
    /// Escape is the prefix of every arrow key, so a decoder that guessed at it
    /// would turn one arrow key into a cancelled confirmation plus two stray
    /// characters. Waiting is correct — and it means a person who presses Escape
    /// on the trusted path sees nothing happen at all.
    ///
    /// Pinned here rather than left in a comment on the drawing, because the
    /// drawing is where the wrong answer was written and this is where the fact
    /// lives.
    #[test]
    fn a_bare_escape_is_not_a_key_yet_and_ctrl_c_is() {
        assert_eq!(decode(&[0x1b]), None, "a lone Escape was decoded as a key");
        // With what follows it, it is an arrow — which is the whole reason the
        // decoder may not answer on the Escape alone.
        assert_eq!(decode(b"\x1b[A"), Some((Key::Up, 3)));

        // The key the hint names instead. Raw mode on this machine is entered
        // without `ISIG`, so this byte arrives instead of killing the session.
        assert_eq!(decode(&[0x03]), Some((Key::Interrupt, 1)));
    }
}
