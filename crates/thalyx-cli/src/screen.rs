//! Putting the one screen on the display, and filling it with what this machine
//! actually is.
//!
//! The decree is `vault/02-Arquitectura/La-Pantalla.md`. How the screen *looks*
//! is decided in `thalyx-screen`, which is pure and has no idea a display
//! exists; what is here is the other two halves: reading the machine's real
//! state into a [`Screen`], and getting a frame onto `/dev/fb0`.
//!
//! ## What this delivery does and does not do
//!
//! It draws the machine, and the prompt takes typing. **It does not yet run the
//! verbs.** That is deliberate rather than unfinished: `session::run` is one
//! six-hundred-line loop that prints as it goes, and turning it into something
//! that hands an answer back would be a large edit to the most exercised code in
//! the project. Doing that in the same delivery as the first pixels anybody has
//! ever seen would mean that if Cesar's machine comes up black, there is no way
//! to tell which of the two changes did it — which is the rule in `CLAUDE.md`
//! about not stacking a second unverified change on the first.
//!
//! So this one answers the questions only his hardware can answer: whether
//! `/dev/fb0` is there, whether that firmware packs a pixel the way the code
//! assumed, whether the console gives the display up and takes it back, whether
//! the keyboard still reaches us in graphics mode, and whether the layout is
//! right at his resolution.

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

fn store_words() -> String {
    // The label the store was found by, which is what a person checks when they
    // want to know they are on the machine they think they are on.
    match std::fs::read_to_string("/proc/self/mountinfo") {
        Ok(text) => text
            .lines()
            .find(|line| line.contains(" /var/thalyx ") || line.contains(" / "))
            .and_then(|line| line.split(' ').next_back().map(str::to_string))
            .unwrap_or_else(|| "store".to_string()),
        Err(_) => "store".to_string(),
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

/// The screen this machine is, right now.
pub fn live(store: &Store, here: &Path) -> Screen {
    let mut screen = Screen::new(Bar {
        machine: "thalyx".to_string(),
        store: store_words(),
        guard: guard_now(),
        clock: clock(),
    });
    screen.left = vec![where_panel(here), files_panel(here), modules_panel(store)];
    screen.right = vec![running_panel(), memory_panel(), network_panel()];
    screen.conversation = vec![
        Turn::machine(format!(
            "Thalyx está en la pantalla. El store es {}.",
            store_words()
        )),
        Turn::agent(
            "Esta es la pantalla de Thalyx, dibujada por Thalyx sobre el framebuffer \
             que el firmware dejó configurado: sin X, sin Wayland, sin compositor. \
             Los verbos todavía se teclean en la sesión de texto; aquí se está \
             comprobando el dibujo.",
        ),
        Turn::machine("Escape o Ctrl-C devuelve la consola de texto."),
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

/// Draw the machine on the display until somebody presses Ctrl-C.
pub fn show(store: &Store) -> Fallible {
    use std::os::fd::AsFd;

    let display = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(FRAMEBUFFER)
        .map_err(|error| {
            format!(
                "{FRAMEBUFFER} could not be opened: {error}. \
                 On this machine that means either the kernel has no framebuffer \
                 (`FB_EFI` and `FB` in thalyx.config) or this is not a Thalyx \
                 machine and there is a display server holding it."
            )
        })?;

    let geometry = thalyx_syscall::display_geometry(display.as_fd())?;
    println!(
        "display  {}x{}  {} bits  row {} bytes  buffer {} bytes",
        geometry.width,
        geometry.height,
        geometry.bits_per_pixel,
        geometry.line_length,
        geometry.buffer_len
    );

    let mut mapped = thalyx_syscall::map_shared(display.as_fd(), 0, geometry.buffer_len, true)?;

    // The console goes into graphics mode only once the mapping exists, so a
    // machine that fails to map still has a readable console to say so on.
    let console = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(CONSOLE)?;
    let _graphics = thalyx_syscall::GraphicsMode::enter(console.as_fd())?;

    let stdin = std::io::stdin();
    // Without signals, and the reason is in `RawMode::enter_without_signals`:
    // with the console in graphics mode, a Ctrl-C that kills the process is the
    // one keystroke that can leave this machine with a black screen.
    let _raw = thalyx_syscall::RawMode::enter_without_signals(stdin.as_fd());

    let mut typography = Typography::embedded();
    let here = std::env::current_dir().unwrap_or_else(|_| Path::new("/").to_path_buf());
    let mut screen = live(store, &here);
    let mut line = thalyx_term::Line::new();

    let mut input = stdin.lock();
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 64];
    loop {
        screen.prompt = Prompt {
            line: line.as_string(),
            caret: line.cursor(),
            suggestion: None,
        };
        let canvas =
            thalyx_screen::compose(&screen, &mut typography, geometry.width, geometry.height);
        canvas.write_into(
            mapped.bytes_mut(),
            geometry.line_length,
            format_of(&geometry),
        )?;

        let read = input.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        pending.extend_from_slice(&chunk[..read]);

        let mut leaving = false;
        // `decode` answers `None` when what is buffered is the *prefix* of
        // something longer — half an arrow key across two reads. Stopping there
        // and waiting for more is the whole reason it reports a length; guessing
        // is how one arrow key becomes three characters in the line.
        while let Some((key, used)) = thalyx_term::decode(&pending) {
            pending.drain(..used);
            match key {
                thalyx_term::Key::Interrupt | thalyx_term::Key::EndOfInput => leaving = true,
                thalyx_term::Key::Char(c) => line.insert(c),
                thalyx_term::Key::Backspace => line.backspace(),
                thalyx_term::Key::Delete => line.delete(),
                thalyx_term::Key::Left => line.left(),
                thalyx_term::Key::Right => line.right(),
                thalyx_term::Key::Home => line.home(),
                thalyx_term::Key::End => line.end(),
                // Enter has nothing to run yet. It clears the line rather than
                // doing nothing, so that a person can tell the keyboard is
                // reaching us — see the note at the top about why the verbs are
                // not wired in yet.
                thalyx_term::Key::Enter => {
                    if !line.is_empty() {
                        screen.conversation.push(Turn::person(line.as_string()));
                        screen.conversation.push(Turn::machine(
                            "Los verbos todavía no pasan por la pantalla. Ver `pantalla` en Punto-Actual.",
                        ));
                    }
                    line.clear();
                }
                _ => {}
            }
        }
        if leaving {
            break;
        }
    }
    Ok(())
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
