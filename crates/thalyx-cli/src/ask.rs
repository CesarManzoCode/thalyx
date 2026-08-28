//! Asking the person, on whichever face is in front of them.
//!
//! ## Why this file exists
//!
//! `vault/11-Seguridad/Camino-Confiable.md` says a machine may not change
//! itself without a human saying so, and the eight places in this program that
//! obey it all had the same five lines written out by hand: print the question,
//! check `is_terminal`, read a line, compare it, refuse. That was fine while
//! there was one face to ask on.
//!
//! **Then the screen became the face the machine boots into, and every one of
//! the eight stopped working there.** Under `thalyx-capture` descriptor 0 is
//! `/dev/null` — deliberately, because a question printed into a buffer nobody
//! can see is a machine that hangs with a picture on it — so `is_terminal`
//! answers no and every one of them takes its refusal path. The consequence is
//! the one Cesar found by booting the image: on the display he starts the
//! machine on, `instalar`, `ejecutar`, `observar` and `instalar-en` cannot be
//! *finished*. A face you can look at and not act from.
//!
//! ## One comparison, two faces
//!
//! `vault/01-Filosofia/Principio-Doble-Ruta.md` is why the answer is not "teach
//! the screen to ask" but "take the asking out of the eight". What counts as
//! *yes* is [`Accepts`] and it is used by both faces, so a machine that accepts
//! `sí` at a terminal cannot come to accept only `y` on the glass. Before this,
//! the eight had already drifted: `intento abandonar` took `si` and `sí`, and
//! the verb that takes the kernel guard off the whole machine took neither.
//! Nobody decided that; it is what five hand-written lines become after being
//! written five times.
//!
//! What is **not** shared is the refusal. Each site still says in its own words
//! what did not happen — *«the guard stays on»*, *«nothing was undone»*,
//! *«formatting was not confirmed»* — because that sentence is the one part
//! that is about the verb rather than about the asking.
//!
//! ## Where the context comes from
//!
//! A confirmation is only a confirmation if it shows what was read *from the
//! thing itself*. At a terminal that is free: the verb printed it a few lines
//! up and it is still on the screen. Under the display it is in the capture
//! buffer, unread, and drawing the question without it would put *«teclea la
//! ruta del disco»* on the glass with no disk named anywhere near it. That is
//! what `thalyx_capture::said_so_far` is for, and it is the reason the screen
//! can reuse the eight verbs instead of growing eight second copies of them.

use std::cell::RefCell;
use std::io::{IsTerminal, Write};
use std::os::fd::OwnedFd;
use thalyx_screen::{Confirmation, PixelFormat, Row, Screen, Typography};

/// What authorises the thing about to happen.
///
/// Two shapes and not one, because they protect against different mistakes.
#[derive(Debug, Clone)]
pub enum Accepts {
    /// A yes. For the questions where the danger is doing the thing at all, and
    /// the person has just read what it is.
    Yes,
    /// These exact words, and nothing near them. For the questions where the
    /// danger is *habit*: `y` is muscle memory, and typing out the path of the
    /// disk that is about to be erased is the last chance to read which disk it
    /// is. Used by everything that writes to a device.
    Exactly(String),
}

impl Accepts {
    /// Whether what was typed authorises it. **The single comparison** — both
    /// faces call this one, which is the property that keeps them from drifting.
    ///
    /// Rule 9, fail closed, in the two ways it can be broken here: an
    /// [`Accepts::Exactly`] is compared whole and case-sensitively, so `/dev/sda`
    /// never authorises `/dev/sdb1` and `SÍ` is not a path; and it is refused
    /// outright when the words asked for are empty, because `"" == ""` would
    /// authorise the thing before anybody touched a key.
    pub fn allows(&self, typed: &str) -> bool {
        let typed = typed.trim();
        match self {
            // Both languages, because the machine is used in Spanish and the
            // verbs are written in English. A person answering the question in
            // the language the question was asked in is not a person saying no.
            Accepts::Yes => matches!(
                typed.to_lowercase().as_str(),
                "y" | "yes" | "s" | "si" | "sí"
            ),
            Accepts::Exactly(words) => !words.is_empty() && typed == words,
        }
    }

    /// What the display tells the person to type.
    ///
    /// `sí` and not `y` on the glass: the screen is in Spanish, and [`allows`]
    /// takes both, so the two faces disagree about nothing.
    ///
    /// [`allows`]: Accepts::allows
    fn shown(&self) -> String {
        match self {
            Accepts::Yes => "sí".to_string(),
            Accepts::Exactly(words) => words.clone(),
        }
    }
}

/// What came back. Four and not two, because rule 10 is exactly the difference
/// between the last two: *nobody to ask* and *asked and could not read it* send
/// a person to look in different places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answered {
    /// Authorised.
    Yes,
    /// Something else was typed, or the question was cancelled. A person said no.
    No,
    /// There is no face to ask on: a pipe, a script, a machine with no console.
    /// Silence is not consent.
    NoOneToAsk,
    /// There is a face, and nothing could be read from it.
    Unreadable,
}

/// Ask, and come back with one of the four.
///
/// `question` is the line the person reads — printed at a terminal, drawn as the
/// headline on the display. It carries no trailing newline for the same reason
/// it never did: at a terminal the answer is typed on the same line.
pub fn confirm(question: &str, accepts: &Accepts) -> Answered {
    if let Some(painter) = borrow_painter() {
        return on_the_display(painter, question, accepts);
    }

    // Silence is not consent. This is the check every one of the eight sites
    // used to make for itself.
    if !std::io::stdin().is_terminal() {
        return Answered::NoOneToAsk;
    }

    print!("{question}");
    let _ = std::io::stdout().flush();
    match crate::term::read_answer() {
        Ok(Some(typed)) if accepts.allows(&typed) => Answered::Yes,
        Ok(Some(_)) => Answered::No,
        // End of input with nothing in it is not a yes and is not a no: nobody
        // answered. Reported apart from a read that failed, which is a machine
        // problem rather than a person's silence.
        Ok(None) => Answered::NoOneToAsk,
        Err(_) => Answered::Unreadable,
    }
}

// ---------------------------------------------------------------------------
// The display's face.
// ---------------------------------------------------------------------------

/// Everything needed to draw a question on the glass and read the answer.
///
/// Owned rather than borrowed, and **moved** into the slot below rather than
/// lent to it, because a borrow that outlives the frame it came from is the one
/// thing a thread-local cannot be told about. The screen loop gives this up for
/// exactly as long as one verb runs and takes it back after.
pub struct Painter {
    pub display: thalyx_syscall::Mapped,
    pub geometry: thalyx_syscall::DisplayGeometry,
    pub format: PixelFormat,
    pub typography: Typography,
    /// The machine as it was drawn a moment ago. The question is drawn over
    /// this, so a cancelled confirmation puts back exactly the screen the
    /// person was looking at.
    pub screen: Screen,
    /// The real console, duplicated **before** the capture put `/dev/null` on
    /// descriptor 0. Without this there is a display to draw a question on and
    /// no keyboard to answer it with.
    pub keyboard: OwnedFd,
    /// Bytes the screen loop had read and not yet decoded. They are a person's
    /// keystrokes and they move with whoever is reading — the same rule that
    /// `term::take_pending` exists for, one layer in.
    pub pending: Vec<u8>,
}

thread_local! {
    /// The display a question raised on this thread should be drawn on.
    ///
    /// Thread-local for the reason `thalyx-capture` gives at length: descriptors
    /// and process-wide switches with no owner are rule 11, and `cargo test`
    /// runs a binary's tests as threads in one process. A verb runs on the
    /// thread that installed this — `screen::run_one` calls `session::act_on`
    /// directly — so the thread is the right scope, and a thread with no display
    /// asks the terminal, which is what every test and every piped session does.
    static PAINTER: RefCell<Option<Painter>> = const { RefCell::new(None) };
}

/// Lend the display to whatever runs inside `body`, and take it back.
///
/// Returns the painter along with the body's value rather than leaving it in the
/// slot: the screen loop needs the mapping and the leftover keystrokes back to
/// draw its next frame, and a painter that stayed installed would be a display
/// borrowed by a thread that is no longer drawing on it.
pub fn while_the_display_can_ask<T>(painter: Painter, body: impl FnOnce() -> T) -> (Painter, T) {
    let previous = PAINTER.with(|slot| slot.borrow_mut().replace(painter));
    let returned = Returned { previous };

    let value = body();

    let painter = PAINTER
        .with(|slot| slot.borrow_mut().take())
        .expect("the display this call lent out");
    drop(returned);
    (painter, value)
}

/// Puts back whatever was installed before, however the body leaves.
///
/// A guard for the reason `thalyx-capture`'s is one: a verb can panic, and a
/// slot left holding a mapping whose frame has gone is worse than one left
/// empty.
struct Returned {
    previous: Option<Painter>,
}

impl Drop for Returned {
    fn drop(&mut self) {
        PAINTER.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}

/// Take the display out of the slot for the duration of one question.
///
/// **Taken out and not borrowed through**, which is what makes a confirmation
/// raised from inside a confirmation impossible rather than merely unlikely: the
/// inner one finds the slot empty and asks the terminal, and the outer one still
/// owns the pixels it is in the middle of drawing.
fn borrow_painter() -> Option<Painter> {
    PAINTER.with(|slot| slot.borrow_mut().take())
}

fn give_painter_back(painter: Painter) {
    PAINTER.with(|slot| *slot.borrow_mut() = Some(painter));
}

/// How many lines of what the verb already printed are drawn as context.
///
/// A cap and not the whole buffer: `instalar-en` prints a page of disk layout,
/// and a confirmation whose context runs off the bottom of the display is a
/// confirmation whose most important line — the one naming the device — may be
/// the one that did not fit. The **last** lines are kept, because that is where
/// a verb puts what it is about to do.
const MOST_CONTEXT_LINES: usize = 12;

fn on_the_display(mut painter: Painter, question: &str, accepts: &Accepts) -> Answered {
    let mut confirmation = Confirmation {
        what: question.trim().trim_end_matches(':').trim().to_string(),
        found: context_rows(question),
        type_this: accepts.shown(),
        typed: String::new(),
    };

    let answer = ask_on_the_glass(&mut painter, &mut confirmation, accepts);

    // The question comes off the display before the verb carries on, so whatever
    // it prints next is drawn on the machine's own screen and not underneath a
    // dialogue that has already been answered.
    painter.screen.confirmation = None;
    give_painter_back(painter);
    answer
}

/// The lines the verb printed before it stopped to ask, as rows of the
/// confirmation.
///
/// The question itself is dropped from them: it is already the headline, and a
/// confirmation that says the same sentence twice reads as two questions.
fn context_rows(question: &str) -> Vec<Row> {
    let Some(said) = thalyx_capture::said_so_far() else {
        // No capture is running, so there is nothing the verb printed that this
        // could show. Rule 10: no context is drawn, rather than an invented one.
        return Vec::new();
    };

    let asked = question.trim();
    let mut lines: Vec<&str> = said
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.last().map(|line| line.trim()) == Some(asked) {
        lines.pop();
    }

    let over = lines.len().saturating_sub(MOST_CONTEXT_LINES);
    let mut rows: Vec<Row> = Vec::new();
    if over > 0 {
        // Said rather than silently cut. A context that was trimmed and does not
        // say so is a context a person reads as complete.
        rows.push(Row::Note(format!("… {over} línea(s) más arriba")));
    }
    for line in lines.into_iter().skip(over) {
        rows.push(Row::fact(line.trim_start()));
    }
    rows
}

/// Draw, read a key, draw again. Returns when the person has answered.
fn ask_on_the_glass(
    painter: &mut Painter,
    confirmation: &mut Confirmation,
    accepts: &Accepts,
) -> Answered {
    use std::io::Read;

    let mut keyboard = std::fs::File::from(
        match thalyx_syscall::duplicate(std::os::fd::AsFd::as_fd(&painter.keyboard)) {
            Ok(copy) => copy,
            // Nothing to read the answer from. Not a no — a machine that cannot
            // be asked, which is what the caller's own refusal is written for.
            Err(_) => return Answered::Unreadable,
        },
    );

    let mut chunk = [0u8; 256];
    loop {
        painter.screen.confirmation = Some(confirmation.clone());
        if draw(painter).is_err() {
            // The question could not be put on the glass. Answering it anyway
            // from a keyboard would be authorising something nobody was shown.
            return Answered::Unreadable;
        }

        if painter.pending.is_empty() {
            match keyboard.read(&mut chunk) {
                Ok(0) => return Answered::NoOneToAsk,
                Ok(read) => painter.pending.extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Answered::Unreadable,
            }
        }

        while let Some((key, used)) = thalyx_term::decode(&painter.pending) {
            painter.pending.drain(..used);
            match key {
                thalyx_term::Key::Enter => {
                    return if accepts.allows(&confirmation.typed) {
                        Answered::Yes
                    } else {
                        Answered::No
                    };
                }
                // Ctrl-C cancels, and the drawn hint says Ctrl-C rather than
                // Escape. A bare Escape is the *prefix* of every arrow key, so
                // `decode` correctly waits for the byte after it — a hint naming
                // it would have sent a person to a key that appears to do
                // nothing until they press another one.
                thalyx_term::Key::Interrupt | thalyx_term::Key::EndOfInput => {
                    return Answered::No;
                }
                thalyx_term::Key::Char(c) => confirmation.typed.push(c),
                thalyx_term::Key::Backspace | thalyx_term::Key::Delete => {
                    confirmation.typed.pop();
                }
                // Everything else is ignored on purpose. There is no line editor
                // here and there should not be: arrow keys, tab completion and
                // history are conveniences, and a confirmation whose text can be
                // recalled from history is one a person can authorise without
                // reading.
                _ => {}
            }
        }
    }
}

pub fn draw(painter: &mut Painter) -> std::io::Result<()> {
    let canvas = thalyx_screen::compose(
        &painter.screen,
        &mut painter.typography,
        painter.geometry.width,
        painter.geometry.height,
    );
    let line_length = painter.geometry.line_length;
    let format = painter.format;
    canvas
        .write_into(painter.display.bytes_mut(), line_length, format)
        .map_err(|why| std::io::Error::other(why.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole file exists for: one comparison, both faces.
    #[test]
    fn what_counts_as_yes_does_not_depend_on_which_face_asked() {
        for said in ["y", "Y", "yes", "YES", "s", "si", "sí", "Sí", "  sí  "] {
            assert!(Accepts::Yes.allows(said), "{said:?} was not read as a yes");
        }
        for said in ["", "n", "no", "nope", "sim", "yy", "ssí"] {
            assert!(!Accepts::Yes.allows(said), "{said:?} was read as a yes");
        }
    }

    #[test]
    fn the_exact_words_are_exact_and_nothing_near_them_authorises() {
        let asked = Accepts::Exactly("/dev/sdb".to_string());
        assert!(asked.allows("/dev/sdb"));
        // Trimmed at the ends, because a terminal's newline is not a person's
        // answer — and nowhere else.
        assert!(asked.allows("  /dev/sdb\t"));
        for near in [
            "/dev/sd",
            "/dev/sdb1",
            "/DEV/SDB",
            "/dev/sda",
            "sí",
            "y",
            "",
        ] {
            assert!(!asked.allows(near), "{near:?} was accepted");
        }
    }

    #[test]
    fn a_confirmation_that_asks_for_no_words_authorises_nothing() {
        // `"" == ""` is true, so the empty case has to be refused by name or an
        // `Exactly` built from a value nobody filled in is authorised before
        // anybody touches a key.
        assert!(!Accepts::Exactly(String::new()).allows(""));
        assert!(!Accepts::Exactly(String::new()).allows("sí"));
    }

    #[test]
    fn the_display_is_told_to_type_the_language_the_question_is_in() {
        assert_eq!(Accepts::Yes.shown(), "sí");
        assert!(Accepts::Yes.allows(&Accepts::Yes.shown()));
        let path = Accepts::Exactly("/dev/sdb".to_string());
        assert_eq!(path.shown(), "/dev/sdb");
        assert!(path.allows(&path.shown()));
    }

    /// Rule 10, and the reason there are four outcomes rather than two.
    #[test]
    fn nobody_to_ask_and_could_not_read_are_not_the_same_answer() {
        assert_ne!(Answered::NoOneToAsk, Answered::Unreadable);
        // And neither of them is a no that a person gave.
        assert_ne!(Answered::NoOneToAsk, Answered::No);
        assert_ne!(Answered::Unreadable, Answered::No);
    }

    /// With no display installed, this thread asks the terminal — which is what
    /// every test in this crate and every piped session is doing.
    #[test]
    fn a_thread_with_no_display_installed_does_not_think_it_has_one() {
        assert!(borrow_painter().is_none());
    }

    #[test]
    fn the_context_of_a_question_is_empty_when_nothing_is_being_captured() {
        // Not a fabricated context and not a panic: there is no verb printing on
        // this thread, so there is nothing it printed. Rule 10 again.
        assert!(context_rows("  Run it? [y/N] ").is_empty());
    }
}
