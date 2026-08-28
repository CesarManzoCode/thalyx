//! Loading a keyboard layout into the kernel, and saying which one is there.
//!
//! The tables and the reason they exist are in `thalyx_term::keymap`. What is
//! here is the half that touches a console: putting a layout in, asking the
//! kernel what it now holds, and the two faces that report it.
//!
//! ## The failure this can cause, and the way back
//!
//! Loading a layout is the one thing in this program that can make a machine
//! **unusable while looking perfectly healthy**: the screen draws, the session
//! answers, and the keys produce the wrong letters. On the image there is no
//! second terminal to fix it from.
//!
//! Two things hold against that, and they are the same pair the screen has:
//!
//! - `teclado ingles` puts back the kernel's own compiled-in map — not something
//!   close to it, the same table — and `thalyx_term::keymap` has the test that
//!   the letters of `teclado ingles` are on the same keys in both layouts, so
//!   the way back is typable from where you would need it.
//! - `thalyx.teclado=no` on the boot entry comes up on whatever the kernel
//!   started with and loads nothing, which is the only recovery that does not
//!   need another medium.
//!
//! ## Why the check asks the kernel
//!
//! Rule 5. Thalyx's record of what it sent proves that it sent it. Every ioctl
//! here can fail — on a console that is not a console, on a kernel built without
//! `VT` — and a machine that reported the layout it had asked for would be
//! reporting the request. So [`loaded`] reads entries back out with `KDGKBENT`
//! and reports what is *there*, and [`load`] verifies before it says it worked.

use std::os::fd::{AsFd, BorrowedFd};
use thalyx_term::keymap::{self, Layout, Produces};

/// The console whose table is set. The same one the screen takes.
///
/// Not searched for: a machine with two consoles that quietly picked one would
/// load a layout onto a keyboard nobody is typing on.
pub const CONSOLE: &str = "/dev/console";

/// The kernel command line's word for «leave the keyboard alone».
const KEYBOARD_PARAMETER: &str = "thalyx.teclado=";

/// Whether the boot entry asked for no layout at all.
///
/// Same shape as `thalyx.pantalla`, and for the same reason: the value that has
/// to be exactly right is the one that turns the feature **off**, never the one
/// that leaves it on, so a typo cannot be the difference between a machine that
/// can be typed on and one that cannot.
pub fn said_leave_it_alone(cmdline: &str) -> bool {
    cmdline
        .split_ascii_whitespace()
        .filter_map(|word| word.strip_prefix(KEYBOARD_PARAMETER))
        .any(|value| value == "no" || value == "kernel")
}

fn boot_entry_said_no() -> bool {
    // Rule 10: unreadable is not «it said no». Failing to read the command line
    // is not permission to override what the machine was told.
    std::fs::read_to_string("/proc/cmdline")
        .map(|cmdline| said_leave_it_alone(&cmdline))
        .unwrap_or(false)
}

/// What happened, in the same three-way shape the rest of the boot uses.
pub enum Loading {
    /// The layout is in, and reading it back agrees.
    Loaded {
        name: &'static str,
        entries: usize,
        accents: usize,
    },
    /// Nothing was attempted, and why. Not a failure: a session under bash has
    /// no business changing the keyboard of the machine it is a guest on.
    LeftAlone(String),
    /// It was attempted and it did not take.
    Failed(String),
}

impl Loading {
    /// One line, for the boot report and the `estado` panel.
    pub fn briefly(&self) -> String {
        match self {
            Loading::Loaded {
                name,
                entries,
                accents,
            } => format!("{name} — {entries} key(s), {accents} accent(s)"),
            Loading::LeftAlone(why) => format!("left as the kernel had it — {why}"),
            Loading::Failed(why) => format!("could not be loaded — {why}"),
        }
    }
}

/// Put a layout into the console, and check that it went in.
pub fn load(console: BorrowedFd<'_>, layout: &Layout) -> Loading {
    let mut entries = 0usize;
    for (table, keys) in layout.tables {
        for (keycode, value) in keys.iter().enumerate() {
            // `NR_KEYS` is 256 and a keycode is a byte, so this cannot truncate;
            // the cast is here rather than a `u8` loop because the index is what
            // names the key.
            let keycode = keycode as u8;
            if let Err(error) = thalyx_syscall::set_keymap_entry(console, *table, keycode, *value) {
                // Said differently depending on whether anything got through,
                // because they are different situations for the person reading
                // it. Failing on the very first key is a console that is not a
                // console and the keyboard is untouched; failing halfway leaves
                // a keyboard that is half of two layouts, which is the one worth
                // alarming somebody about. Written as one sentence for both, it
                // alarmed about the harmless case the first time it ran.
                let left_behind = if entries == 0 {
                    "nothing was changed, so the keyboard is as it was".to_string()
                } else {
                    format!(
                        "{entries} key(s) were already changed, so this keyboard is \
                         now half of two layouts — `teclado ingles` puts it back"
                    )
                };
                return Loading::Failed(format!(
                    "table {table}, key {keycode}: {error}. {left_behind}"
                ));
            }
            entries += 1;
        }
    }

    let accents: Vec<(u32, u32, u32)> = layout
        .accents
        .iter()
        .map(|accent| {
            (
                u32::from(accent.dead),
                u32::from(accent.base),
                u32::from(accent.made),
            )
        })
        .collect();
    if let Err(error) = thalyx_syscall::set_accents(console, &accents) {
        return Loading::Failed(format!(
            "the keys went in and the accent table did not: {error}. \
             The dead keys on this layout now swallow the letter after them"
        ));
    }

    // Rule 5, and it is not ceremony: every ioctl above can succeed on a
    // descriptor that is not a console without anything changing.
    match loaded(console) {
        OnTheKeyboard::This(there) if there.name == layout.name => Loading::Loaded {
            name: layout.name,
            entries,
            accents: layout.accents.len(),
        },
        OnTheKeyboard::This(there) => Loading::Failed(format!(
            "every entry was accepted and the keyboard reads as `{}`, not `{}`",
            there.name, layout.name
        )),
        OnTheKeyboard::SomethingElse => Loading::Failed(
            "every entry was accepted and the keyboard matches no layout this \
             machine has, so what is on it now cannot be named"
                .to_string(),
        ),
        // Not a success. Every ioctl above can be accepted by a descriptor that
        // is not a console, so a load nobody could check is a load nobody
        // should believe. Rule 9.
        OnTheKeyboard::Unreadable(why) => Loading::Failed(format!(
            "every entry was accepted and the keyboard cannot be read back, so \
             there is no way to say what is on it: {why}"
        )),
    }
}

/// Which layout the console actually holds, asked of the kernel.
///
/// Three answers and not two, and the third one is the whole of rule 10. *No
/// layout of mine is on this keyboard* and *I could not ask the keyboard* look
/// identical from a function that returns `Option`, and they send a person to
/// different places: the first to `teclado latino`, the second to why the
/// console is not a console. The report said the first while meaning the second
/// for exactly as long as it took to run it once.
pub enum OnTheKeyboard {
    /// One of ours, named.
    This(&'static Layout),
    /// Read, and it matches none of them. Somebody loaded something else.
    SomethingElse,
    /// Not read at all.
    Unreadable(String),
}

pub fn loaded(console: BorrowedFd<'_>) -> OnTheKeyboard {
    let mut there = Vec::new();
    for keycode in PROBE_KEYS {
        match thalyx_syscall::keymap_entry(console, 0, keycode) {
            Ok(value) => there.push(Some(value)),
            Err(error) => return OnTheKeyboard::Unreadable(error.to_string()),
        }
    }

    // By probing the keys the layouts disagree about rather than by comparing
    // all 3072 entries: a layout somebody loaded with `loadkeys` on a
    // development machine is not one of ours and the honest answer there is
    // `SomethingElse` — but a machine that said so because one function key
    // differs would be useless.
    keymap::LAYOUTS
        .iter()
        .copied()
        .find(|layout| {
            PROBE_KEYS
                .iter()
                .zip(&there)
                .all(|(keycode, value)| layout.plainly(*keycode) == *value)
        })
        .map_or(OnTheKeyboard::SomethingElse, OnTheKeyboard::This)
}

/// The keys that tell the layouts apart: `ñ`, `¿`, and the acute accent.
///
/// Three and not one, because one key is a coincidence — every layout has
/// *something* on keycode 39.
const PROBE_KEYS: [u8; 3] = [39, 13, 26];

/// Load the layout a Thalyx machine comes up on. Called from PID 1.
pub fn at_boot() -> Loading {
    if boot_entry_said_no() {
        return Loading::LeftAlone("the boot entry said thalyx.teclado=no".to_string());
    }
    let Some(layout) = keymap::by_name(keymap::AT_BOOT) else {
        return Loading::Failed(format!(
            "this build has no layout called `{}`",
            keymap::AT_BOOT
        ));
    };
    let console = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(CONSOLE)
    {
        Ok(console) => console,
        Err(error) => return Loading::LeftAlone(format!("{CONSOLE} could not be opened: {error}")),
    };
    load(console.as_fd(), layout)
}

// ---------------------------------------------------------------------------
// The verb.
// ---------------------------------------------------------------------------

/// `teclado` — say which layout the keyboard has, or put another one on it.
pub fn run(rest: &str, face: crate::files::Face) -> std::io::Result<()> {
    let asked = rest.trim();

    let console = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(CONSOLE)
    {
        Ok(console) => console,
        Err(error) => {
            // Rule 10: this is *could not look*, and it is said as that rather
            // than as «there is no layout».
            refuse(
                face,
                "no_console",
                "run_this_on_the_machine",
                &format!(
                    "No puedo abrir {CONSOLE}: {error}. Aquí eso casi siempre                      quiere decir que Thalyx es un programa dentro de otra cosa,                      y el teclado es de quien lo arrancó."
                ),
            );
            return Ok(());
        }
    };

    if asked.is_empty() {
        let there = loaded(console.as_fd());
        let mode = thalyx_syscall::keyboard_mode(console.as_fd());
        let keys: Vec<(u8, String)> = PROBE_KEYS
            .iter()
            .map(|keycode| {
                let says = match thalyx_syscall::keymap_entry(console.as_fd(), 0, *keycode) {
                    Ok(value) => match keymap::produces(value) {
                        Produces::Letter(letter) => letter.to_string(),
                        Produces::Dead => "acento".to_string(),
                        Produces::Something => "no es una letra".to_string(),
                    },
                    Err(error) => format!("no se pudo leer: {error}"),
                };
                (*keycode, says)
            })
            .collect();

        let named = match &there {
            OnTheKeyboard::This(layout) => Some(layout.name),
            OnTheKeyboard::SomethingElse | OnTheKeyboard::Unreadable(_) => None,
        };
        report(face, named, &there, &mode, &keys);
        if !face.is_machine() {
            println!();
            match &there {
                OnTheKeyboard::This(layout) => println!("  teclado  {}", layout.name),
                OnTheKeyboard::SomethingElse => println!(
                    "  teclado  ninguno de los que traigo — alguien lo cambió con otra cosa"
                ),
                OnTheKeyboard::Unreadable(why) => {
                    println!("  teclado  ? — no se pudo leer: {why}")
                }
            }
            // The mode matters and is invisible: a layout with `ñ` on it, on a
            // keyboard that is not in unicode mode, sends one byte that is `ñ`
            // in no encoding the screen reads. The symptom is a letter coming
            // out wrong, which nobody attributes to a mode.
            match mode {
                Ok(thalyx_syscall::K_UNICODE) => println!("  modo     unicode"),
                Ok(other) => println!(
                    "  modo     {other} — no es unicode, así que una tecla \n\
                     \x20         acentuada manda un byte que no es esa letra"
                ),
                Err(ref error) => println!("  modo     ? — no se pudo leer: {error}"),
            }
            for (keycode, says) in &keys {
                println!("  tecla {keycode:<3}  {says}");
            }
            println!("  otras    {}", keymap::names().join(", "));
            println!();
        }
        return Ok(());
    }

    let Some(layout) = keymap::by_name(asked) else {
        refuse(
            face,
            "no_such_layout",
            "name_one_it_has",
            &format!(
                "No tengo una distribución que se llame `{asked}`. Tengo: {}.",
                keymap::names().join(", ")
            ),
        );
        return Ok(());
    };

    match load(console.as_fd(), layout) {
        Loading::Loaded {
            name,
            entries,
            accents,
        } => {
            let mode = thalyx_syscall::keyboard_mode(console.as_fd());
            report(face, Some(name), &OnTheKeyboard::This(layout), &mode, &[]);
            if !face.is_machine() {
                println!();
                println!("  El teclado es `{name}` — {entries} tecla(s), {accents} acento(s).");
                println!();
                println!("  Compruébalo tecleando ñ. Si sale otra cosa,");
                println!("  `teclado ingles` vuelve a lo que traía el kernel.");
                println!();
            }
        }
        Loading::LeftAlone(why) => refuse(face, "left_alone", "nothing_to_do", &why),
        Loading::Failed(why) => refuse(face, "not_loaded", "teclado_ingles", &why),
    }
    Ok(())
}

/// `ensayo teclado <distribución>` — what loading it would do, without doing it.
///
/// D1: every verb that changes the machine can be rehearsed. This one earns it
/// twice over, because the change it makes is to the instrument a person would
/// use to undo it.
///
/// Two halves with different failure modes, kept apart on purpose. What the
/// layout **would** produce is read from the tables and cannot fail. What is on
/// the keyboard **now** is read from the console and can, and where it does this
/// says so rather than reporting the layout as absent — rule 10.
pub fn foresee(rest: &str, face: crate::files::Face) {
    use serde_json::json;

    let asked = rest.trim();
    if asked.is_empty() {
        refuse(
            face,
            "incomplete",
            "name_a_layout",
            &format!("Cuál distribución. Tengo: {}.", keymap::names().join(", ")),
        );
        return;
    }
    let Some(layout) = keymap::by_name(asked) else {
        refuse(
            face,
            "no_such_layout",
            "name_one_it_has",
            &format!(
                "No tengo una distribución que se llame `{asked}`. Tengo: {}.",
                keymap::names().join(", ")
            ),
        );
        return;
    };

    let console = std::fs::OpenOptions::new().read(true).open(CONSOLE).ok();
    let entries: usize = layout.tables.iter().map(|(_, keys)| keys.len()).sum();

    // The keys that would change, and what each would go from and to. This is
    // the whole value of the rehearsal: `ñ` appearing in the «would be» column
    // is what tells a person the layout is the one they meant.
    let mut changing = Vec::new();
    // Why the «now» column is empty, if it is. Opening the console and being
    // able to ask it about a key are two different things — under bash the open
    // succeeds and the ioctl comes back `ENOTTY` — and a column of `?` with no
    // reason beside it is the shape rule 10 exists to stop.
    let mut unreadable: Option<String> = None;
    for keycode in PROBE_KEYS {
        let would = layout.plainly(keycode).map(keymap::produces);
        let now = match console.as_ref() {
            None => None,
            Some(console) => match thalyx_syscall::keymap_entry(console.as_fd(), 0, keycode) {
                Ok(value) => Some(keymap::produces(value)),
                Err(error) => {
                    unreadable.get_or_insert_with(|| error.to_string());
                    None
                }
            },
        };
        changing.push((keycode, now, would));
    }
    let why_not = match (&console, &unreadable) {
        (None, _) => Some(format!("no pude abrir {CONSOLE}")),
        (Some(_), Some(error)) => Some(format!("{CONSOLE} no contesta sobre teclas: {error}")),
        _ => None,
    };

    let says = |produced: Option<Produces>| match produced {
        Some(Produces::Letter(letter)) => letter.to_string(),
        Some(Produces::Dead) => "acento".to_string(),
        Some(Produces::Something) => "no es una letra".to_string(),
        None => "?".to_string(),
    };

    if face.is_machine() {
        face.say(thalyx_files::machine::answer(
            "rehearse",
            vec![
                ("verb", json!("keyboard")),
                ("layout", json!(layout.name)),
                ("would_set_keys", json!(entries)),
                ("would_set_accents", json!(layout.accents.len())),
                // `null` in `now` is *the console could not be read*, and it is
                // not the same as a key that produces nothing.
                (
                    "keys",
                    json!(
                        changing
                            .iter()
                            .map(|(keycode, now, would)| json!({
                                "keycode": keycode,
                                "now": now.map(|_| says(*now)),
                                "would_be": says(*would),
                            }))
                            .collect::<Vec<_>>()
                    ),
                ),
                // Rule 10 as a field: `null` in `now` above means *not read*,
                // and this says which kind of not-read it was.
                ("unreadable", json!(why_not)),
                ("would_change_the_machine", json!(true)),
                ("changed_anything", json!(false)),
            ],
        ));
        return;
    }

    println!();
    println!(
        "  `teclado {asked}` pondría `{}` — {entries} tecla(s), {} acento(s).",
        layout.name,
        layout.accents.len()
    );
    println!();
    if let Some(why) = &why_not {
        println!("  No sé qué hay ahora: {why}.");
        println!("  Lo de abajo es lo que la distribución trae, no un cambio.");
        println!();
    }
    for (keycode, now, would) in &changing {
        println!(
            "    tecla {keycode:<3}  {:<16} → {}",
            says(*now),
            says(*would)
        );
    }
    println!();
    println!("  No se cambió nada.");
    println!();
}

/// The structured face, which gets the facts and not the sentence.
///
/// `Superficie-para-el-LLM.md`: what a person reads is prose, and what a program
/// reads is the values it would otherwise have to parse back out of the prose.
/// The layout's name, the keyboard's mode and what each probe key produces are
/// three separate questions, so they are three fields.
fn report(
    face: crate::files::Face,
    layout: Option<&str>,
    there: &OnTheKeyboard,
    mode: &std::io::Result<i32>,
    keys: &[(u8, String)],
) {
    use serde_json::json;
    if !face.is_machine() {
        return;
    }
    face.say(thalyx_files::machine::answer(
        OP,
        vec![
            ("layout", json!(layout)),
            // Rule 10 in a field, and it is the one this verb got wrong first:
            // `layout: null` alone cannot tell *this keyboard has something
            // else on it* from *this keyboard could not be asked*.
            (
                "read",
                json!(match there {
                    OnTheKeyboard::Unreadable(why) => json!({"ok": false, "why": why}),
                    _ => json!({"ok": true, "why": null}),
                }),
            ),
            // `null` here is *could not read the mode*, and it is not the same
            // as a mode that is not unicode.
            ("mode", json!(mode.as_ref().ok())),
            (
                "unicode",
                json!(matches!(mode, Ok(m) if *m == thalyx_syscall::K_UNICODE)),
            ),
            (
                "keys",
                json!(
                    keys.iter()
                        .map(|(keycode, says)| json!({"keycode": keycode, "produces": says}))
                        .collect::<Vec<_>>()
                ),
            ),
            ("available", json!(keymap::names())),
        ],
    ));
}

fn refuse(face: crate::files::Face, word: &str, remedy: &str, message: &str) {
    if face.is_machine() {
        face.say(thalyx_files::machine::refused_with(
            OP,
            word,
            remedy,
            message,
            vec![],
        ));
    } else {
        println!();
        println!("  {message}");
        println!();
    }
}

const OP: &str = "keyboard";

#[cfg(test)]
mod tests {
    use super::*;

    /// The escape hatch, and the asymmetry it has on purpose.
    #[test]
    fn only_an_exact_no_leaves_the_keyboard_alone() {
        assert!(said_leave_it_alone("root=/dev/sda thalyx.teclado=no quiet"));
        assert!(said_leave_it_alone("thalyx.teclado=kernel"));

        // Everything else leaves the machine typable in the language it speaks.
        // A typo in the boot entry must not be the difference.
        for cmdline in [
            "thalyx.teclado=si",
            "thalyx.teclado=latino",
            "thalyx.teclado=n",
            "thalyx.teclado=",
            "thalyx.teclado",
            "",
            // Not this parameter at all: a suffix match would let another
            // project's boot entry turn Thalyx's keyboard off.
            "otra.thalyx.teclado=no",
        ] {
            assert!(
                !said_leave_it_alone(cmdline),
                "`{cmdline}` was read as a refusal"
            );
        }
    }
}
