//! Putting the one screen on the display, and filling it with what this machine
//! actually is.
//!
//! The decree is `vault/02-Arquitectura/La-Pantalla.md`. How the screen *looks*
//! is decided in `thalyx-screen`, which is pure and has no idea a display
//! exists; what is here is the other two halves: reading the machine's real
//! state into a [`Screen`], and getting a frame onto `/dev/fb0`.
//!
//! ## This is the face, not a way to reach one
//!
//! Cesar, 2026-08-28: *«no quiero un comando para activar ui, quiero ya la ui,
//! la que se ve al iniciar»*. So [`show`] is what `session::run` calls **before
//! it prints a prompt**, and the text session is the fallback rather than the
//! front door. A verb named `pantalla` still exists, and it is the way back
//! after Ctrl-C — not the way in.
//!
//! ## The verbs run here, and how
//!
//! The first delivery of this file drew the machine and did nothing with what
//! was typed, because `session::run` was one six-hundred-line loop that printed
//! as it went. What made that loop reusable was noticing that its arms touch
//! exactly four things — the store, where the person is standing, which face
//! answers, and how this process came to exist — and none of the rest. It is
//! [`crate::session::dispatch`] now, and both faces call it.
//!
//! What the arms print is caught at the **descriptor**, by `thalyx-capture`,
//! rather than by threading a writer through all of them. That is not a
//! shortcut: `correr` and `ejecutar` start other programs, and a module's
//! output is on descriptor 1 of a process this one does not control. Anything
//! narrower would draw an empty answer for the two verbs whose whole point is
//! running something.
//!
//! ## What still needs his hardware
//!
//! Everything about the glass. Whether `/dev/fb0` is there, whether that
//! firmware packs a pixel the way the code assumed, whether the console gives
//! the display up and takes it back, whether the keyboard still reaches us in
//! graphics mode, and whether the layout is right at his resolution. None of it
//! can be asked in a container with no display, and [`describe`] exists so the
//! first question can be asked without the console being taken to ask it.
//!
use crate::files::Face;
use std::io::Read;
use std::path::Path;
use thalyx_core::Store;
use thalyx_screen::{Bar, Guard, Panel, PixelFormat, Prompt, Row, Screen, Tone, Turn, Typography};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// Where the display is. Not searched for: `/dev/fb0` is what `CONFIG_FB_EFI`
/// plus `devtmpfs` produce, and looking for "any framebuffer" would make a
/// machine with two of them pick one silently.
const FRAMEBUFFER: &str = "/dev/fb0";
/// The console whose mode is taken. On the image the kernel command line ends
/// with `console=tty0`, so this is the one that is drawing.
const CONSOLE: &str = "/dev/console";

// ---------------------------------------------------------------------------
// Reading the machine.
// ---------------------------------------------------------------------------

fn bytes_in_words(bytes: u64) -> String {
    const UNITS: [(u64, &str); 4] = [
        (1 << 30, "GiB"),
        (1 << 20, "MiB"),
        (1 << 10, "KiB"),
        (1, "B"),
    ];
    for (size, name) in UNITS {
        if bytes >= size {
            return format!("{:.1} {name}", bytes as f64 / size as f64);
        }
    }
    "0 B".to_string()
}

fn guard_now() -> Guard {
    // Two questions, not one, and this is the distinction that was got wrong
    // once already: *loaded* and *denying* are different facts. A machine with
    // nothing attached is not observing — it is unguarded, and saying
    // "observando" there would be the comfortable answer to a question nobody
    // asked.
    let Some(object) = crate::init::embedded::OBJECT else {
        return Guard::Absent;
    };
    match thalyx_bpf::attachment(object) {
        Ok(state) if state.is_absent() => Guard::Absent,
        Ok(_) => {
            use thalyx_permd::PolicyStore;
            match thalyx_permd::KernelStore::default_map().enforcement() {
                thalyx_permd::Enforcement::Enforcing => Guard::Enforcing,
                thalyx_permd::Enforcement::Observing => Guard::Observing,
                // Rule 10: a mode that could not be read is not a mode.
                thalyx_permd::Enforcement::Unreadable(_) => Guard::Unknown,
            }
        }
        Err(_) => Guard::Unknown,
    }
}

/// What the bar says the store is: the disk it is on, or that there is not one.
///
/// ## What this said before, and why it was worse than nothing
///
/// Cesar photographed a booted machine whose bar read
/// `rw,size=980392k,nr_inodes=245098`. Those are a **tmpfs's** super options,
/// and the machine panel two inches to the right of them said `btrfs` — so the
/// bar was not merely unreadable, it disagreed with the truth beside it.
///
/// Two mistakes, both worth naming because each survives the other being fixed.
///
/// The first is that the fallback beat the thing it was a fallback for. One
/// predicate asked for `/var/thalyx` **or** `/`, inside a `find`, so what came
/// back was whichever appeared first *in the file* — and `/` is always mounted
/// before anything under it. The store was never consulted on a machine that
/// had one.
///
/// The second is that the last field of a `mountinfo` line is not a label and
/// never was. `mountinfo` is `… - <fstype> <source> <super options>`, with a
/// variable number of optional fields before the `-`, so counting from either
/// end without finding that separator answers a different question each time.
///
/// So this reads the source device, past the `-`, from the mount the store is
/// actually on — and when that is not a device it says so, because "this
/// machine forgets everything at the next boot" is the one fact about a store
/// that a person must not have to infer.
fn store_words(root: &Path) -> String {
    match std::fs::read_to_string("/proc/self/mountinfo") {
        Ok(text) => store_from(&text, root),
        // Rule 10: a failure to read is not a failure to exist. A bar that said
        // `sin disco` here would be telling somebody their work is not being
        // kept, on no evidence at all.
        Err(_) => "store ?".to_string(),
    }
}

/// The store's disk according to `mountinfo`, or why there is not one.
///
/// Pure so it can be tested against a captured `/proc/self/mountinfo` rather
/// than against a hand-written one — rule 6. The mount chosen is the longest
/// mount point that is a prefix of `root`, which is the same rule
/// [`crate::session::gather`] uses for the filesystem reading; two rules would
/// be two answers to one question, which is exactly the disagreement this
/// function was found by.
fn store_from(mountinfo: &str, root: &Path) -> String {
    let mut best: Option<(usize, String)> = None;
    for line in mountinfo.lines() {
        // Everything before the `-` has a variable length: optional fields like
        // `shared:1` are there or not. Splitting on it is the only way to know
        // which field is which on either side.
        let (before, after) = match line.split_once(" - ") {
            Some(halves) => halves,
            None => continue,
        };
        let point = match before.split(' ').nth(4) {
            Some(point) => point,
            None => continue,
        };
        let source = match after.split(' ').nth(1) {
            Some(source) => source,
            None => continue,
        };
        if root.starts_with(point) && point.len() >= best.as_ref().map_or(0, |(n, _)| *n) {
            best = Some((point.len(), source.to_string()));
        }
    }

    match best {
        Some((_, source)) if source.starts_with("/dev/") => source,
        // A store that is not on a device is the initramfs, and the initramfs is
        // memory. Said in the bar rather than left to be worked out from a
        // reading elsewhere, because it changes what everything typed below it
        // is worth.
        Some(_) => "sin disco — no recuerda".to_string(),
        None => "store ?".to_string(),
    }
}

fn clock() -> String {
    // No timezone database on the image, so this is the machine's uptime rather
    // than a wall clock that would be wrong by hours and look right. Saying
    // what it is beats showing a confident lie.
    match std::fs::read_to_string("/proc/uptime") {
        Ok(text) => {
            let seconds: f64 = text
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            format!("encendida {:.0}m", seconds / 60.0)
        }
        Err(_) => String::new(),
    }
}

fn where_panel(here: &Path) -> Panel {
    let mut rows = vec![Row::fact(here.display().to_string())];
    match thalyx_files::list(here) {
        Ok(listing) => {
            rows.push(Row::pair("cosas", listing.entries.len().to_string()));
            if !listing.unreadable.is_empty() {
                // Rule 10 again, on the screen this time: entries that could not
                // be read are counted separately, because a short listing and an
                // honest one look identical otherwise.
                rows.push(Row::toned(
                    format!("{} ilegibles", listing.unreadable.len()),
                    Tone::Refused,
                ));
            }
        }
        Err(error) => rows.push(Row::toned(error.to_string(), Tone::Refused)),
    }
    Panel::new("dónde", rows)
}

fn files_panel(here: &Path) -> Panel {
    let rows = match thalyx_files::list(here) {
        Ok(listing) => {
            let mut rows: Vec<Row> = listing
                .entries
                .iter()
                .filter(|entry| !thalyx_files::is_hidden(&entry.name))
                .take(14)
                .map(|entry| {
                    let name = entry.name.to_string_lossy();
                    match &entry.kind {
                        thalyx_files::Kind::Directory => Row::fact(format!("{name}/")),
                        thalyx_files::Kind::File { bytes } => {
                            Row::pair(name.to_string(), bytes_in_words(*bytes))
                        }
                        thalyx_files::Kind::Link { broken: true, .. } => {
                            Row::toned(format!("{name} →"), Tone::Refused)
                        }
                        thalyx_files::Kind::Link { .. } => {
                            Row::toned(format!("{name} →"), Tone::Muted)
                        }
                        thalyx_files::Kind::Other(what) => {
                            Row::toned(format!("{name}  {what}"), Tone::Muted)
                        }
                    }
                })
                .collect();
            if rows.is_empty() {
                rows.push(Row::note("Aquí no hay nada."));
            }
            rows
        }
        Err(error) => vec![Row::toned(error.to_string(), Tone::Refused)],
    };
    Panel::new("archivos", rows)
}

fn modules_panel(store: &Store) -> Panel {
    let rows = match store.installed() {
        Ok(list) if list.is_empty() => {
            vec![Row::note("Nada instalado en esta máquina todavía.")]
        }
        Ok(list) => list
            .iter()
            .take(10)
            .map(|(id, version)| Row::pair(id.to_string(), version.to_string()))
            .collect(),
        Err(error) => vec![Row::toned(error.to_string(), Tone::Refused)],
    };
    Panel::new("módulos", rows)
}

fn running_panel() -> Panel {
    let running = thalyx_proc::running();
    let mut rows: Vec<Row> = running
        .processes
        .iter()
        .filter(|process| !process.kernel_thread)
        .take(8)
        .map(|process| {
            Row::pair(
                format!("{}  {}", process.pid, process.name),
                bytes_in_words(process.resident),
            )
        })
        .collect();
    if rows.is_empty() {
        rows.push(Row::note("Nada corriendo fuera del kernel."));
    }
    for (what, why) in running.unreadable.iter().take(2) {
        rows.push(Row::toned(format!("{what}: {why}"), Tone::Refused));
    }
    Panel::new("corriendo", rows)
}

fn memory_panel() -> Panel {
    let rows = match thalyx_proc::memory() {
        Ok(memory) => vec![
            Row::pair("en uso", bytes_in_words(memory.in_use())),
            Row::pair("disponible", bytes_in_words(memory.available)),
            Row::pair("total", bytes_in_words(memory.total)),
        ],
        Err(error) => vec![Row::toned(error.to_string(), Tone::Refused)],
    };
    Panel::new("memoria", rows)
}

fn network_panel() -> Panel {
    let rows = match thalyx_net::every() {
        Ok(interfaces) if interfaces.is_empty() => {
            vec![Row::note("El kernel no reporta ninguna interfaz.")]
        }
        Ok(interfaces) => {
            let mut rows: Vec<Row> = interfaces
                .iter()
                .take(6)
                .map(|interface| {
                    let tone = match interface.carrier {
                        thalyx_net::Carrier::Up => Tone::Ok,
                        // `Unknown` is what the kernel answers about an
                        // interface it has taken down, and it is not `Down`.
                        // Drawing them the same would be rule 10 broken in
                        // colour.
                        thalyx_net::Carrier::Down | thalyx_net::Carrier::Unknown => Tone::Muted,
                    };
                    Row::toned(
                        format!("{}  {}", interface.name, interface.carrier.word()),
                        tone,
                    )
                })
                .collect();
            rows.push(Row::note("Thalyx ve la red y no la usa."));
            rows
        }
        Err(error) => vec![Row::toned(error.to_string(), Tone::Refused)],
    };
    Panel::new("red", rows)
}

/// What this machine is showing, right now, in panels.
///
/// Rebuilt after every verb rather than kept and patched. A panel that said
/// eleven files after `rm` deleted one would be a screen quietly lying about
/// the thing the person just did, and the cost of asking again is one listing.
fn refresh(screen: &mut Screen, session: &crate::session::Session<'_>) {
    let here = session.here.at().to_path_buf();
    screen.bar = Bar {
        machine: "thalyx".to_string(),
        store: store_words(session.store.root()),
        guard: guard_now(),
        clock: clock(),
    };
    screen.left = vec![
        where_panel(&here),
        files_panel(&here),
        modules_panel(session.store),
    ];
    screen.right = vec![
        machine_panel(session.store),
        running_panel(),
        memory_panel(),
        network_panel(),
    ];
}

/// What the session's own first screen says, as a panel.
///
/// This is the same reading `thalyx session` prints as its banner, and it is on
/// the screen for the reason the banner exists at all: the first thing a machine
/// shows is the easiest place in the system to put on a show, and nobody checks
/// a banner. Every line here is `ok`, `no` or `?`, and `?` is never drawn as
/// `no` — rule 10, in colour.
fn machine_panel(store: &Store) -> Panel {
    use crate::session::Outcome;

    let rows = crate::session::gather(store)
        .iter()
        .map(|reading| match &reading.outcome {
            Outcome::Found(text) => Row::pair(reading.subject, first_clause(text)),
            Outcome::Absent(text) => Row::toned(
                format!("{}  {}", reading.subject, first_clause(text)),
                Tone::Refused,
            ),
            // Muted and not `Refused`: unreadable is not absent, and a screen
            // that drew them the same would be telling somebody to go fix a
            // thing that may well be there.
            Outcome::Unreadable(text) => Row::toned(
                format!("{}  ?  {}", reading.subject, first_clause(text)),
                Tone::Muted,
            ),
        })
        .collect();
    Panel::new("máquina", rows)
}

/// The first clause of a reading, because a panel column is narrow.
///
/// The whole sentence is still one keystroke away in the text session, and a
/// panel that wrapped every reading over four lines would push the ones below it
/// off the display — which is how a reading that says something is missing stops
/// being seen.
fn first_clause(text: &str) -> String {
    let cut = text.find(" — ").unwrap_or(text.len());
    text[..cut].chars().take(46).collect()
}

/// The screen a machine has just come up on.
fn live(session: &crate::session::Session<'_>) -> Screen {
    let mut screen = Screen::new(Bar {
        machine: "thalyx".to_string(),
        store: store_words(session.store.root()),
        guard: guard_now(),
        clock: clock(),
    });
    refresh(&mut screen, session);
    screen.conversation = vec![
        Turn::machine(format!(
            "Thalyx. El store es {}, y esto es la máquina — no hay nada debajo.",
            store_words(session.store.root())
        )),
        Turn::agent(
            "Escribe abajo. Los verbos son los mismos que en la sesión de texto: \
             `ls`, `cat`, `cd`, `modulos`, `procesos`, `estado`, `describe` los \
             enumera todos. Tab completa, las flechas repiten lo anterior, \
             AvPág y RePág recorren lo que ya se dijo.",
        ),
        Turn::machine(
            "Ctrl-C con la línea vacía baja a la sesión de texto; `pantalla` vuelve aquí.",
        ),
    ];
    screen.prompt = Prompt::default();
    screen
}

// ---------------------------------------------------------------------------
// Putting it on the glass.
// ---------------------------------------------------------------------------

/// What the kernel said about the display, turned into what the canvas needs.
fn format_of(geometry: &thalyx_syscall::DisplayGeometry) -> PixelFormat {
    PixelFormat {
        bits_per_pixel: geometry.bits_per_pixel,
        red: thalyx_screen::Channel::at(geometry.red.0, geometry.red.1),
        green: thalyx_screen::Channel::at(geometry.green.0, geometry.green.1),
        blue: thalyx_screen::Channel::at(geometry.blue.0, geometry.blue.1),
    }
}

/// What this display is, and whether the screen would work on it — **without
/// touching the console**.
///
/// This exists because the interesting question on a new machine is not "does
/// the screen look right", it is "would it come up at all", and the honest way
/// to ask that is one that cannot leave the display black if the answer is no.
/// So it reads the geometry, composes a real frame at that size, and converts it
/// into a buffer of exactly the length the kernel reported — every step of the
/// path except the one that writes to the device and the one that takes the
/// console.
pub fn describe(face: Face) -> Fallible {
    use std::os::fd::AsFd;

    let display = std::fs::OpenOptions::new()
        .read(true)
        .open(FRAMEBUFFER)
        .map_err(|error| format!("{FRAMEBUFFER}: {error}"))?;
    let geometry = thalyx_syscall::display_geometry(display.as_fd())?;

    let mut typography = Typography::embedded();
    let canvas = thalyx_screen::compose(
        &thalyx_screen::sample::working(),
        &mut typography,
        geometry.width,
        geometry.height,
    );
    let mut buffer = vec![0u8; geometry.buffer_len];
    let fits = canvas.write_into(&mut buffer, geometry.line_length, format_of(&geometry));

    match face {
        Face::Machine => println!(
            "{{\"op\":\"screen_describe\",\"device\":\"{FRAMEBUFFER}\",\"width\":{},\
             \"height\":{},\"bits_per_pixel\":{},\"line_length\":{},\"buffer_len\":{},\
             \"red\":[{},{}],\"green\":[{},{}],\"blue\":[{},{}],\"would_draw\":{},\
             \"refused\":{}}}",
            geometry.width,
            geometry.height,
            geometry.bits_per_pixel,
            geometry.line_length,
            geometry.buffer_len,
            geometry.red.0,
            geometry.red.1,
            geometry.green.0,
            geometry.green.1,
            geometry.blue.0,
            geometry.blue.1,
            fits.is_ok(),
            match &fits {
                Ok(()) => "null".to_string(),
                Err(why) => format!("\"{why}\""),
            }
        ),
        Face::Human => {
            println!(
                "display  {FRAMEBUFFER}  {}x{}  {} bits per pixel",
                geometry.width, geometry.height, geometry.bits_per_pixel
            );
            println!(
                "row      {} bytes   buffer {} bytes",
                geometry.line_length, geometry.buffer_len
            );
            println!(
                "channels red {}+{}  green {}+{}  blue {}+{}",
                geometry.red.0,
                geometry.red.1,
                geometry.green.0,
                geometry.green.1,
                geometry.blue.0,
                geometry.blue.1
            );
            match fits {
                Ok(()) => println!(
                    "ok  a frame of {}x{} fits this display and its packing is understood",
                    geometry.width, geometry.height
                ),
                Err(why) => println!("no  this display would not be drawn on: {why}"),
            }
        }
    }
    Ok(())
}

/// Why the screen could not come up.
///
/// A type and not a string, because `describe` advertises exactly two error
/// words for this verb and a caller has been promised it can write its handling
/// before it ever sees one. Formatting the reason into prose and then matching
/// on the prose is the same mistake as rule 5's grep for a sentence the probe
/// had stopped printing.
#[derive(Debug)]
pub struct NoScreen {
    /// `not_a_terminal` or `no_display`.
    pub code: &'static str,
    /// The whole of it, for a person.
    pub why: String,
}

impl NoScreen {
    fn no_display(error: impl std::fmt::Display) -> Self {
        Self {
            code: "no_display",
            why: error.to_string(),
        }
    }
}

impl std::fmt::Display for NoScreen {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{}", self.why)
    }
}

impl std::error::Error for NoScreen {}

/// Why the screen stopped being on the display.
pub enum Left {
    /// The person asked for the text session, with Ctrl-C on an empty line.
    /// On the machine that is the only way out of the screen, and it is not a
    /// way out of Thalyx.
    ForTheTextSession,
    /// `salir` in a session that has somewhere to go back to. Never on the
    /// machine, where the verb refuses.
    Finished,
}

/// How many drawn lines one press of AvPág/RePág moves.
///
/// Not a screenful, because the screen does not know how many lines fit until
/// it has wrapped them, and a page that overshot would skip the line somebody
/// was reading. A fixed step that is smaller than any display is the safe one.
const SCROLL_STEP: usize = 8;

/// How much of one answer is kept. A `cat` of something large would otherwise
/// grow this process's memory by the size of the file, on a machine whose whole
/// point is that there is nothing else running to notice.
const MOST_LINES_OF_AN_ANSWER: usize = 500;

/// How many turns the conversation keeps. Roughly a long afternoon of use.
const MOST_TURNS: usize = 400;

/// Draw the machine on the display, and run what is typed into it.
///
/// This is what boot lands on. `session::run` calls it before it prints a single
/// prompt, and the text session underneath is what it falls back to — so a
/// display that cannot be drawn on is an `Err` here and a working machine there,
/// never a machine that stops.
pub fn show(session: &mut crate::session::Session<'_>) -> Result<Left, NoScreen> {
    use std::io::IsTerminal;
    use std::os::fd::AsFd;

    // Before the device, and this one is not about the display at all. A screen
    // is a keyboard as well as a picture, and a session being fed from a pipe
    // has none: taking the console there would black out a machine to draw a
    // frame nobody can type into, and — because `catalogue_is_true` types every
    // advertised verb into a piped session — it would do it on whatever machine
    // was running the tests. Rule 11.
    if !std::io::stdin().is_terminal() {
        return Err(NoScreen {
            code: "not_a_terminal",
            why: "the screen needs a keyboard, and this input is not a terminal".to_string(),
        });
    }

    let display = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(FRAMEBUFFER)
        .map_err(|error| NoScreen {
            code: "no_display",
            why: format!(
                "{FRAMEBUFFER} could not be opened: {error}. \
                 On this machine that means either the kernel has no framebuffer \
                 (`FB_EFI` and `FB` in thalyx.config) or this is not a Thalyx \
                 machine and there is a display server holding it."
            ),
        })?;

    let geometry =
        thalyx_syscall::display_geometry(display.as_fd()).map_err(NoScreen::no_display)?;
    let mut mapped = thalyx_syscall::map_shared(display.as_fd(), 0, geometry.buffer_len, true)
        .map_err(NoScreen::no_display)?;

    // The console goes into graphics mode only once the mapping exists, so a
    // machine that fails to map still has a readable console to say so on.
    let console = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(CONSOLE)
        .map_err(NoScreen::no_display)?;
    let _graphics =
        thalyx_syscall::GraphicsMode::enter(console.as_fd()).map_err(NoScreen::no_display)?;

    let stdin = std::io::stdin();
    // Without signals, and the reason is in `RawMode::enter_without_signals`:
    // with the console in graphics mode, a Ctrl-C that kills the process is the
    // one keystroke that can leave this machine with a black screen.
    let _raw = thalyx_syscall::RawMode::enter_without_signals(stdin.as_fd());

    let mut typography = Typography::embedded();
    let mut screen = live(session);
    let mut line = thalyx_term::Line::new();
    // What has been typed before, newest last. Its own list rather than the text
    // session's, which lives inside `term::Terminal` and belongs to a reader
    // this one does not use.
    let mut history: Vec<String> = Vec::new();
    let mut recalled: Option<usize> = None;

    let mut input = stdin.lock();
    let mut pending: Vec<u8> = crate::term::take_pending();
    let mut chunk = [0u8; 256];
    let mut leaving: Option<Left> = None;

    loop {
        screen.prompt = Prompt {
            line: line.as_string(),
            caret: line.cursor(),
            suggestion: None,
        };
        let canvas =
            thalyx_screen::compose(&screen, &mut typography, geometry.width, geometry.height);
        canvas
            .write_into(
                mapped.bytes_mut(),
                geometry.line_length,
                format_of(&geometry),
            )
            .map_err(NoScreen::no_display)?;

        if let Some(left) = leaving {
            crate::term::give_pending(&pending);
            return Ok(left);
        }

        if pending.is_empty() {
            let read = input.read(&mut chunk).map_err(NoScreen::no_display)?;
            if read == 0 {
                return Ok(Left::ForTheTextSession);
            }
            pending.extend_from_slice(&chunk[..read]);
        }

        // `decode` answers `None` when what is buffered is the *prefix* of
        // something longer — half an arrow key across two reads. Stopping there
        // and waiting for more is the whole reason it reports a length; guessing
        // is how one arrow key becomes three characters in the line.
        while let Some((key, used)) = thalyx_term::decode(&pending) {
            pending.drain(..used);
            match key {
                // Ctrl-C on a line with something in it throws the line away,
                // the same as in the text session. On an empty line it is the
                // way down to that session — deliberately two presses from the
                // middle of typing, because leaving the screen by accident on a
                // machine with no other face is worse than typing Ctrl-C twice.
                thalyx_term::Key::Interrupt => {
                    if line.is_empty() {
                        leaving = Some(Left::ForTheTextSession);
                    } else {
                        line.clear();
                    }
                }
                thalyx_term::Key::EndOfInput => leaving = Some(Left::ForTheTextSession),
                thalyx_term::Key::Char(c) => {
                    line.insert(c);
                    recalled = None;
                }
                thalyx_term::Key::Backspace => line.backspace(),
                thalyx_term::Key::Delete => line.delete(),
                thalyx_term::Key::Left => line.left(),
                thalyx_term::Key::Right => line.right(),
                thalyx_term::Key::Home => line.home(),
                thalyx_term::Key::End => line.end(),
                thalyx_term::Key::PageUp => screen.scrollback += SCROLL_STEP,
                thalyx_term::Key::PageDown => {
                    screen.scrollback = screen.scrollback.saturating_sub(SCROLL_STEP)
                }
                thalyx_term::Key::Up => recall(&history, &mut recalled, &mut line, true),
                thalyx_term::Key::Down => recall(&history, &mut recalled, &mut line, false),
                thalyx_term::Key::Tab => {
                    if let Some(said) = complete(session, &mut line) {
                        push(&mut screen, Turn::machine(said));
                    }
                }
                thalyx_term::Key::Enter => {
                    let typed = line.as_string().trim().to_string();
                    line.clear();
                    recalled = None;
                    // An answer brings the view back to the bottom. Anything
                    // else means the machine replies somewhere the person is not
                    // looking.
                    screen.scrollback = 0;
                    if typed.is_empty() {
                        continue;
                    }
                    if history.last().map(String::as_str) != Some(typed.as_str()) {
                        history.push(typed.clone());
                    }
                    push(&mut screen, Turn::person(&typed));
                    leaving = run_one(session, &mut screen, &typed);
                    refresh(&mut screen, session);
                }
                _ => {}
            }
            if leaving.is_some() {
                break;
            }
        }
    }
}

/// Run one typed line with its output caught, and put the answer on the screen.
///
/// Returns `Some` when the line asked to leave. Every other outcome — including
/// a verb that failed — is a turn of the conversation, because on this machine
/// a failure is something to read, not something to exit for.
fn run_one(
    session: &mut crate::session::Session<'_>,
    screen: &mut Screen,
    typed: &str,
) -> Option<Left> {
    let caught = thalyx_capture::what_it_says(|| session.act_on(typed));

    let (outcome, said) = match caught {
        Ok(both) => both,
        // The redirection itself failed. Running the verb anyway would print
        // onto a console in graphics mode and could stop on a question nobody
        // can answer — see `capture`. Saying so is the cautious answer.
        Err(error) => {
            push(
                screen,
                Turn::machine(format!(
                    "No pude ejecutar eso sin perder lo que dijera: {error}"
                )),
            );
            return None;
        }
    };

    for turn in answer(&said) {
        push(screen, turn);
    }

    match outcome {
        Ok(crate::session::Flow::Stay) => None,
        Ok(crate::session::Flow::Leave) => Some(Left::Finished),
        // What `clear` means here. The verb printed nothing — there is no
        // console under the screen to print an escape to — so the conversation
        // is dropped, which is the same thing the escape does to a console.
        // The scrollback goes with it: `limpiar` that left the previous
        // hundred lines one PageUp away would not have cleared anything.
        Ok(crate::session::Flow::Emptied) => {
            screen.conversation.clear();
            screen.scrollback = 0;
            None
        }
        Ok(crate::session::Flow::ToTheScreen) => {
            push(
                screen,
                Turn::machine("Ya estás en la pantalla. Ctrl-C con la línea vacía baja al texto."),
            );
            None
        }
        // A verb that came back with an error has already printed most of what
        // it wanted to say; this is the part `main` would have put on stderr.
        Err(error) => {
            push(screen, Turn::machine(error.to_string()));
            None
        }
    }
}

/// What a verb printed, as turns of the conversation.
///
/// One turn and not one per line: the rule between two lines of the same answer
/// is not a boundary between two things said, and drawing it as one would put a
/// coloured bar down the middle of every listing.
fn answer(said: &str) -> Vec<Turn> {
    let text = said.trim_end_matches('\n');
    if text.trim().is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= MOST_LINES_OF_AN_ANSWER {
        return vec![Turn::machine(text)];
    }
    // The tail, because that is what a terminal would have left on the screen,
    // and said rather than silently cut: an answer that was trimmed and does not
    // say so is an answer nobody can tell from a complete one.
    let kept = lines[lines.len() - MOST_LINES_OF_AN_ANSWER..].join("\n");
    vec![
        Turn::machine(format!(
            "… {} líneas antes de esto no se guardaron.",
            lines.len() - MOST_LINES_OF_AN_ANSWER
        )),
        Turn::machine(kept),
    ]
}

/// Add a turn, and forget the oldest once there are too many.
fn push(screen: &mut Screen, turn: Turn) {
    screen.conversation.push(turn);
    if screen.conversation.len() > MOST_TURNS {
        let excess = screen.conversation.len() - MOST_TURNS;
        screen.conversation.drain(..excess);
    }
}

/// Walk back and forth through what has been typed.
fn recall(
    history: &[String],
    recalled: &mut Option<usize>,
    line: &mut thalyx_term::Line,
    backwards: bool,
) {
    if history.is_empty() {
        return;
    }
    let next = match (*recalled, backwards) {
        (None, true) => Some(history.len() - 1),
        (None, false) => None,
        (Some(0), true) => Some(0),
        (Some(index), true) => Some(index - 1),
        (Some(index), false) if index + 1 < history.len() => Some(index + 1),
        // Past the newest is the empty line the person was typing, which is
        // where Down has to be able to get back to.
        (Some(_), false) => None,
    };
    *recalled = next;
    line.clear();
    if let Some(index) = next {
        for c in history[index].chars() {
            line.insert(c);
        }
    }
}

/// Complete what is being typed, and say what the choices were when there are
/// several.
///
/// The same list the text session's Tab uses — `session::completions` — so a
/// verb that completes in one face completes in the other.
fn complete(session: &crate::session::Session<'_>, line: &mut thalyx_term::Line) -> Option<String> {
    let typed = line.as_string();
    let before: String = typed.chars().take(line.cursor()).collect();
    let candidates = crate::session::completions(session.here.at(), &before);

    let fragment = before.rsplit(' ').next().unwrap_or("").to_string();
    let matching: Vec<&String> = candidates
        .iter()
        .filter(|candidate| candidate.starts_with(&fragment))
        .collect();

    let shared = match matching.split_first() {
        None => return None,
        Some((first, rest)) => rest.iter().fold((*first).clone(), |shared, candidate| {
            let keep = shared
                .chars()
                .zip(candidate.chars())
                .take_while(|(a, b)| a == b)
                .count();
            shared.chars().take(keep).collect()
        }),
    };

    for c in shared.chars().skip(fragment.chars().count()) {
        line.insert(c);
    }

    if matching.len() > 1 && shared.chars().count() == fragment.chars().count() {
        // Only when Tab could not add anything: a list printed on every press
        // would bury the conversation under the same twelve names.
        Some(
            matching
                .iter()
                .take(40)
                .map(|candidate| candidate.as_str())
                .collect::<Vec<_>>()
                .join("  "),
        )
    } else {
        None
    }
}

/// Write a frame to a PNG instead of to a display.
///
/// This is how the screen is looked at on a machine that has none — which is
/// every machine that builds Thalyx. It is the same composition path the
/// display uses, so what comes out is what gets drawn.
pub fn to_png(out: &Path, width: u32, height: u32, which: &str, face: Face) -> Fallible {
    let screen = match which {
        "confirmando" | "confirming" => thalyx_screen::sample::confirming(),
        "trabajando" | "working" => thalyx_screen::sample::working(),
        other => {
            return Err(
                format!("`{other}` is not a sample: try `trabajando` or `confirmando`").into(),
            );
        }
    };
    let mut typography = Typography::embedded();
    let canvas = thalyx_screen::compose(&screen, &mut typography, width, height);
    let bytes = thalyx_screen::png::encode(&canvas)?;
    std::fs::write(out, &bytes)?;
    match face {
        Face::Machine => println!(
            "{{\"wrote\":\"{}\",\"width\":{width},\"height\":{height},\"bytes\":{}}}",
            out.display(),
            bytes.len()
        ),
        Face::Human => println!(
            "wrote  {}  ▪ {width}x{height}, {} bytes",
            out.display(),
            bytes.len()
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::store_from;
    use std::path::Path;

    /// `/proc/self/mountinfo`, captured verbatim from the container this was
    /// written in.
    ///
    /// Rule 6: a parser for another kernel interface's output needs one real
    /// sample. The thing a hand-written one would have got wrong here is the
    /// exact thing that broke — the optional fields before the `-`, which are
    /// present on some lines and absent on others, so no invented fixture would
    /// have both shapes in it unless its author already knew.
    const CAPTURED: &str = "\
22 27 0:21 / /proc rw,relatime - proc proc rw
23 27 0:22 / /sys rw,relatime - sysfs sysfs rw
24 27 0:6 / /dev rw,relatime - devtmpfs devtmpfs rw,size=8225260k,nr_inodes=2056315,mode=755
25 24 0:23 / /dev/shm rw,relatime - tmpfs tmpfs rw,size=16461068k
26 24 0:24 / /dev/pts rw,relatime - devpts devpts rw,mode=600,ptmxmode=000
27 1 254:0 / / rw,relatime - ext4 /dev/vda rw,resv_strict,resuid=65534,resgid=65534
";

    /// The same file as a booted Thalyx has it: an initramfs root with the store
    /// disk mounted underneath, and `/` written down first.
    ///
    /// The shared-mount optional field is on the store's line and not on the
    /// root's, on purpose. That asymmetry is the format, and counting fields
    /// from the end is what it defeats.
    const A_BOOTED_MACHINE: &str = "\
1 1 0:2 / / rw,relatime - rootfs rootfs rw,size=980392k,nr_inodes=245098
2 1 0:5 / /proc rw,relatime - proc proc rw
9 1 254:0 /system /var/thalyx rw,relatime shared:1 - btrfs /dev/vda rw,ssd,subvol=/system
";

    #[test]
    fn the_bar_names_the_disk_the_store_is_on_and_not_the_root_above_it() {
        // The defect, exactly as it was photographed. The old predicate asked
        // for `/var/thalyx` *or* `/` inside a `find`, so it answered with
        // whichever came first in the file — and `/` is always first. Two
        // inches away the machine panel read `btrfs`, off the store's own
        // mount, so the screen disagreed with itself.
        assert_eq!(
            store_from(A_BOOTED_MACHINE, Path::new("/var/thalyx/modules")),
            "/dev/vda"
        );
    }

    #[test]
    fn a_store_that_is_not_on_a_disk_says_it_forgets() {
        // A machine booted with no store attached keeps everything in the
        // initramfs, which is memory. This is the one reading whose absence
        // costs somebody work they have already done, so it is not left to be
        // inferred from a panel further down.
        assert_eq!(
            store_from(A_BOOTED_MACHINE, Path::new("/tmp/whatever")),
            "sin disco — no recuerda"
        );
    }

    #[test]
    fn the_captured_sample_parses_to_the_disk_this_container_is_on() {
        // The control for both tests above, against the real file rather than
        // either machine written down here. A parser that returned the last
        // field, or the first matching line, gets a different answer for this
        // input — the old one returned `rw,resv_strict,resuid=65534,resgid=65534`.
        assert_eq!(store_from(CAPTURED, Path::new("/var/thalyx")), "/dev/vda");
        assert_eq!(
            store_from(CAPTURED, Path::new("/dev/shm/x")),
            "sin disco — no recuerda"
        );
    }

    #[test]
    fn a_mountinfo_that_says_nothing_does_not_become_a_claim_that_it_forgets() {
        // Rule 10, which is the reason there are three answers and not two: a
        // file with no line covering the store is a failure to read, and
        // printing `sin disco` for it would tell somebody their work is being
        // thrown away on no evidence.
        assert_eq!(store_from("", Path::new("/var/thalyx")), "store ?");
        assert_eq!(
            store_from("nonsense\n", Path::new("/var/thalyx")),
            "store ?"
        );
    }
}
