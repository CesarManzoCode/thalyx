//! Reading a line the way a terminal is supposed to: arrows, history, Tab.
//!
//! `thalyx-term` decides what a keystroke means and where the cursor lands.
//! `thalyx-syscall` turns the terminal's own line editor off. This is the only
//! place the three meet, and the only place anything is drawn.
//!
//! ## The failure this file is built around
//!
//! Raw mode is a change to the terminal, not to this program, and it outlives
//! the program that made it. **A session that exits without putting it back
//! leaves the machine unusable** — no echo, no line editing — and on the image
//! there is no second terminal to recover from, because the session *is* the
//! machine. So the restore is a `Drop` guard held for the whole session rather
//! than something switched per line: fewer transitions, and every exit path
//! short of `SIGKILL` goes through it.
//!
//! ## Not a terminal is not a failure
//!
//! Piped input has no line discipline to turn off and no cursor to move. That is
//! ordinary — it is how every test in this repository drives the session — so it
//! falls back to reading whole lines. The fallback is chosen once, by asking,
//! and never inferred from something going wrong.

use std::io::{Read, Write};
use std::sync::Mutex;
use thalyx_term::{Completion, History, Key, Line};

/// Bytes read from the terminal and not yet used, shared by everything that
/// reads input.
///
/// Process-wide and not a field, and that is the bug it exists to fix rather
/// than a convenience. One `read` returns everything that has arrived, which is
/// routinely more than one line — somebody typing ahead, a pasted block, a test
/// writing every line at once. The moment a second place also reads `stdin`
/// directly, those two disagree about what is left: the buffered bytes are
/// already out of the kernel, so the other reader waits forever for input that
/// has been read and is sitting in memory.
///
/// That is not hypothetical. It hung the exit-criterion suite the first time
/// this file buffered anything, because `instalar` asks for a confirmation and
/// the `y` answering it had already been swallowed.
static PENDING: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Read one line, taking what is already buffered before asking the kernel.
///
/// This is what every confirmation prompt uses. Reading `stdin` directly is what
/// this exists to stop: a prompt that does so is answered by input that was
/// consumed before it asked.
pub fn read_answer() -> std::io::Result<Option<String>> {
    loop {
        {
            let mut pending = PENDING.lock().expect("the input buffer");
            if let Some(at) = pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = pending.drain(..=at).collect();
                return Ok(Some(
                    String::from_utf8_lossy(&line)
                        .trim_end_matches(['\n', '\r'])
                        .to_string(),
                ));
            }
        }

        let mut buffer = [0u8; 1024];
        let read = std::io::stdin().read(&mut buffer)?;
        if read == 0 {
            // Whatever is left with no newline behind it is still an answer —
            // the last line of a file that does not end in one.
            let mut pending = PENDING.lock().expect("the input buffer");
            if pending.is_empty() {
                return Ok(None);
            }
            let line: Vec<u8> = pending.drain(..).collect();
            return Ok(Some(String::from_utf8_lossy(&line).trim().to_string()));
        }
        PENDING
            .lock()
            .expect("the input buffer")
            .extend_from_slice(&buffer[..read]);
    }
}

/// How the line ended.
pub enum Ended {
    /// Run this.
    Line(String),
    /// Ctrl-C: forget it and give a fresh prompt. Not an exit — a person whose
    /// machine is one terminal needs a way to abandon a line that is not "leave".
    Abandoned,
    /// Ctrl-D on an empty line, or the input ran out.
    Closed,
}

/// The terminal for as long as the session lasts.
pub struct Terminal {
    /// `None` when the input is not a terminal. Held rather than re-entered per
    /// line so there is exactly one transition in and one out.
    raw: Option<thalyx_syscall::RawMode>,
    history: History,
}

impl Terminal {
    pub fn open() -> Self {
        let raw = thalyx_syscall::RawMode::enter(std::os::fd::AsFd::as_fd(&std::io::stdin()));
        Self {
            raw,
            history: History::new(),
        }
    }

    /// Whether there is a person's terminal on the other end of the input.
    ///
    /// Asked once, when the session opened, and answered from the same fact that
    /// decides whether raw mode was entered — never re-derived, because two
    /// places asking this question are two places that can disagree about which
    /// kind of stream they are writing to.
    pub fn on_a_terminal(&self) -> bool {
        self.raw.is_some()
    }

    /// Read one line, drawing it as it is typed.
    ///
    /// `complete_with` is asked for the candidates when Tab is pressed. A
    /// closure, because what may follow depends on the session — verbs at the
    /// start of a line, file names after one — and that is not a decision about
    /// text.
    pub fn read_line(
        &mut self,
        prompt: &str,
        complete_with: impl Fn(&str) -> Vec<String>,
    ) -> std::io::Result<Ended> {
        if self.raw.is_none() {
            return self.read_line_plainly(prompt);
        }

        print!("{prompt}");
        std::io::stdout().flush()?;

        let mut line = Line::new();
        let mut buffer = [0u8; 1024];

        loop {
            // Only when nothing is left over. Reading first would block with a
            // whole line already in hand, which is the same lost-input bug from
            // the other side.
            if PENDING.lock().expect("the input buffer").is_empty() {
                let read = std::io::stdin().read(&mut buffer)?;
                if read == 0 {
                    println!();
                    return Ok(Ended::Closed);
                }
                PENDING
                    .lock()
                    .expect("the input buffer")
                    .extend_from_slice(&buffer[..read]);
            }

            while let Some((key, used)) = {
                let pending = PENDING.lock().expect("the input buffer");
                thalyx_term::decode(&pending)
            } {
                PENDING.lock().expect("the input buffer").drain(..used);

                match key {
                    Key::Enter => {
                        println!();
                        let text = line.as_string();
                        self.history.remember(&text);
                        return Ok(Ended::Line(text));
                    }
                    Key::Interrupt => {
                        println!();
                        return Ok(Ended::Abandoned);
                    }
                    Key::EndOfInput if line.is_empty() => {
                        println!();
                        return Ok(Ended::Closed);
                    }
                    // Ctrl-D with something typed is not "I am done". Treating it
                    // as one would throw the line away and leave the session,
                    // which is two surprises for one keystroke.
                    Key::EndOfInput => {}
                    Key::Char(c) => line.insert(c),
                    Key::Backspace => line.backspace(),
                    Key::Delete => line.delete(),
                    Key::Left => line.left(),
                    Key::Right => line.right(),
                    Key::Home => line.home(),
                    Key::End => line.end(),
                    Key::Up => {
                        if let Some(text) = self.history.back(&line.as_string()) {
                            line = Line::from(&text);
                        }
                    }
                    Key::Down => {
                        if let Some(text) = self.history.forward() {
                            line = Line::from(&text);
                        }
                    }
                    Key::Tab => self.finish(&mut line, prompt, &complete_with)?,
                    Key::Ignored => {}
                }

                redraw(prompt, &line)?;
            }
        }
    }

    /// Tab: fill in what can be filled, and show the choices when it cannot.
    fn finish(
        &mut self,
        line: &mut Line,
        prompt: &str,
        complete_with: &impl Fn(&str) -> Vec<String>,
    ) -> std::io::Result<()> {
        let before = line.before_cursor();
        // The fragment is the last word, because that is what a person's hand is
        // on. Completing the whole line would replace things they already typed.
        let fragment = before.rsplit(' ').next().unwrap_or("").to_string();
        let candidates = complete_with(&before);

        match thalyx_term::complete(&fragment, &candidates) {
            Completion::None => {}
            Completion::One(rest) => {
                line.insert_str(&rest);
                // A trailing space after a finished word, unless it is a folder —
                // there the slash is already the separator and a space would stop
                // the next Tab from descending into it.
                if !rest.ends_with('/') {
                    line.insert(' ');
                }
            }
            Completion::Many { shared, choices } => {
                line.insert_str(&shared);
                // Printed above a redrawn prompt rather than in place of it: the
                // person is mid-line, and taking the line off the screen to show
                // a list is how they lose their place.
                println!();
                let width = crate::files::screen_width();
                for row in thalyx_files::in_columns(&choices, width, 4) {
                    println!("    {row}");
                }
                print!("{prompt}");
            }
        }
        Ok(())
    }

    /// Whole lines, for input that is not a terminal.
    fn read_line_plainly(&mut self, prompt: &str) -> std::io::Result<Ended> {
        print!("{prompt}");
        std::io::stdout().flush()?;

        let Some(text) = read_answer()? else {
            println!();
            return Ok(Ended::Closed);
        };
        self.history.remember(&text);
        Ok(Ended::Line(text))
    }
}

/// Put the line back on screen with the cursor where it belongs.
///
/// `\r` then erase-to-end-of-line, rather than backspacing over what changed.
/// Redrawing the whole line is the only version that is right for every edit —
/// inserting in the middle shifts everything after it, and a partial redraw
/// leaves the tail of the old line on screen.
fn redraw(prompt: &str, line: &Line) -> std::io::Result<()> {
    let mut out = std::io::stdout();
    write!(out, "\r\x1b[K{prompt}{}", line.as_string())?;

    // Walk back to where the cursor actually is. Counted in characters, because
    // that is what the terminal moves by — a byte count would leave the cursor
    // in the wrong column on any line with an accent in it.
    let after = line.as_string().chars().count() - line.cursor();
    if after > 0 {
        write!(out, "\x1b[{after}D")?;
    }
    out.flush()
}
