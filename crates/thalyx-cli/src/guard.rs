//! `negar` and `observar` — Thalyx switching its own kernel guard on and off.
//!
//! The decree is `vault/02-Arquitectura/Programas-Ajenos.md`, and this closes
//! the hole that note's revisions opened on 2026-08-25. Thalyx learned to
//! **read** the mode that day — the `thalyx_enforcing` map, with `bpf(2)`, no
//! `bpftool` — and that reading is why `ejecutar` refuses a guest while the
//! kernel is only watching. What it could not do was **change** it: that was
//! `make -C lsm enforce`, which is `bpftool`, which the image does not carry
//! and is never going to.
//!
//! So on the only machine that matters, every refusal whose remedy is "make it
//! binding" named a command that does not exist there. A2 says the error names
//! its remedy; a remedy that cannot be run is prose. This is the same hole
//! `Cargador-BPF-Propio` closed for loading and left open for the mode.
//!
//! ## Why arming and disarming are two verbs and not one with an argument
//!
//! `Busqueda.md` settled the shape: a verb whose meaning depends on a word
//! after it can be asked for wrong in silence. That reason is worth more here
//! than it was there, because the two directions are not comparable. Arming is
//! the machine doing what it was built to do. Disarming takes the confinement
//! off everything running right now, a foreign guest mid-run included — and a
//! typo in an argument must never be able to reach it.
//!
//! ## Why only one of them asks a human
//!
//! `Camino-Confiable.md` spends a human's attention on what cannot be undone
//! by noticing. `negar` tightens: if it breaks something, the something says
//! so, loudly, at the moment it happens. `observar` loosens, and a machine
//! that has quietly stopped denying looks exactly like one that is denying and
//! has nothing to deny — the failure with no symptom this whole subsystem
//! exists to refuse to have. So the loosening asks, and the tightening does
//! not.

use crate::files::Face;
use serde_json::json;
use thalyx_permd::{Enforcement, Mode, PolicyStore, StoreError};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The `op` each direction carries in its structured answer.
fn op_for(mode: Mode) -> &'static str {
    match mode {
        Mode::Enforcing => "deny",
        Mode::Observing => "observe",
    }
}

/// `negar` / `observar`, and `ensayo` in front of either.
pub fn set(policies: &dyn PolicyStore, mode: Mode, face: Face, rehearsing: bool) -> Fallible {
    // `describe` promises `rehearse` for `ensayo`, and an answer that came back
    // under the real verb's `op` would be read as the real verb having run.
    let op = if rehearsing { "rehearse" } else { op_for(mode) };

    // Under `rehearse`, every answer says which verb it stood in for —
    // including the refusals. Without it `ensayo negar` and `ensayo observar`
    // come back as the same object on a machine with nothing loaded, and a
    // caller cannot tell which of the two it just asked about.
    let stood_in_for = rehearsing.then(|| op_for(mode));

    // Before the machine is read, and deliberately: "a program may not ask to
    // disarm this machine" is a fact about the request, not about what happens
    // to be pinned. Asking the kernel first would make the same request answer
    // `unreadable` on a machine with nothing loaded — a different word for a
    // refusal that has the same reason on every machine there is.
    if mode == Mode::Observing && !rehearsing && face.is_machine() {
        face.say(thalyx_files::machine::refused_with(
            op,
            "needs_a_human",
            "confirm_at_a_terminal",
            "taking the guard off this machine is not something a program may ask for. \
             Silence is not consent.",
            vec![("changed", json!(false))],
        ));
        return Ok(());
    }

    // Asked before anything else, because all three of the answers below need
    // it and because it is the one question whose wrong answer is dangerous:
    // rule 10 says a failure to read is not a failure to exist, and neither of
    // those is a machine that is enforcing.
    let before = policies.enforcement();

    if let Enforcement::Unreadable(reason) = &before {
        // Not "switch it anyway". The mode flag is pinned by the loader that
        // pins the policy map, so unreadable here means the write would land
        // somewhere unknown or nowhere — and reporting a switch that did not
        // happen is exactly the state this verb was built to end.
        refuse(
            face,
            op,
            "unreadable",
            "load_the_kernel_side",
            &format!("the kernel guard cannot be read, so it will not be moved: {reason}"),
            stood_in_for,
        );
        return Ok(());
    }

    let already = matches!(
        (&before, mode),
        (Enforcement::Enforcing, Mode::Enforcing) | (Enforcement::Observing, Mode::Observing)
    );

    if rehearsing {
        rehearsal(face, mode, &before, already);
        return Ok(());
    }

    // Idempotent and said so, rather than silently writing the same four bytes
    // again. A person who typed it twice learns nothing from a second identical
    // report, and a program cannot tell "it changed" from "it already was"
    // unless one of them says which.
    if already {
        unchanged(face, op, mode);
        return Ok(());
    }

    if mode == Mode::Observing && !consented()? {
        return Ok(());
    }

    match policies.set_enforcement(mode) {
        Ok(()) => moved(face, op, mode, &before),
        Err(error) => refuse(
            face,
            op,
            word_for(&error),
            remedy_for(&error),
            &error.to_string(),
            stood_in_for,
        ),
    }
    Ok(())
}

/// The stable word a caller matches on, per failure and not per store.
fn word_for(error: &StoreError) -> &'static str {
    match error {
        StoreError::NotPinned(_) => "not_loaded",
        StoreError::ModeDidNotTake { .. } => "did_not_take",
        StoreError::ModeNotWritable => "no_mode_flag",
        StoreError::Kernel { .. } => "kernel_refused",
    }
}

/// A2: every one of them names something that can actually be run here.
///
/// Deliberately not `make -C lsm load`. Inside the image there is no `make`,
/// no `bpftool` and no shell — naming them is how the refusal Cesar read on
/// 2026-08-25 sent him to the one command that left the machine still not
/// denying.
fn remedy_for(error: &StoreError) -> &'static str {
    match error {
        StoreError::NotPinned(_) | StoreError::ModeNotWritable => "load_the_kernel_side",
        StoreError::ModeDidNotTake { .. } => "check_what_is_pinned",
        StoreError::Kernel { .. } => "run_as_root",
    }
}

fn refuse(
    face: Face,
    op: &str,
    word: &str,
    remedy: &str,
    message: &str,
    stood_in_for: Option<&'static str>,
) {
    if face.is_machine() {
        // Said in every refusal of this verb, because the whole question a
        // caller is asking is what the guard is doing now, and a refusal that
        // omits it forces a second round trip to find out.
        let mut extra = vec![("changed", json!(false))];
        if let Some(verb) = stood_in_for {
            extra.push(("verb", json!(verb)));
        }
        face.say(thalyx_files::machine::refused_with(
            op, word, remedy, message, extra,
        ));
    } else {
        println!();
        println!("  {message}");
        println!();
    }
}

fn unchanged(face: Face, op: &str, mode: Mode) {
    let said = format!("the kernel guard is already {}", mode.describe());
    if face.is_machine() {
        face.say(thalyx_files::machine::answer(
            op,
            vec![
                ("mode", json!(mode.describe())),
                ("changed", json!(false)),
                ("message", json!(said)),
            ],
        ));
    } else {
        println!();
        println!("  {said}.");
        println!();
    }
}

fn moved(face: Face, op: &str, mode: Mode, before: &Enforcement) {
    let from = before.describe();
    if face.is_machine() {
        face.say(thalyx_files::machine::answer(
            op,
            vec![
                ("mode", json!(mode.describe())),
                ("was", json!(from)),
                ("changed", json!(true)),
            ],
        ));
        return;
    }

    println!();
    match mode {
        Mode::Enforcing => {
            println!("  The kernel is denying now. Every policy written into it is real,");
            println!("  and anything running outside what it was granted stops here.");
        }
        Mode::Observing => {
            println!("  The kernel is watching and denying nothing. Every denial is");
            println!("  written to the ring and applied to none of them — including for");
            println!("  whatever is confined right now.");
            println!();
            println!("  `negar` puts it back.");
        }
    }
    println!();
}

/// What `ensayo negar` and `ensayo observar` answer.
///
/// It reports the flag it read rather than the flag it assumes, which is the
/// only thing that makes a rehearsal of this verb worth having: the question a
/// person is really asking is "is this machine denying", and a rehearsal that
/// answered from the argument would say the same thing on a machine with no
/// kernel side loaded at all.
fn rehearsal(face: Face, mode: Mode, before: &Enforcement, already: bool) {
    let said = if already {
        format!(
            "nothing would change; the kernel guard is already {}",
            mode.describe()
        )
    } else {
        format!(
            "the kernel guard would go from {} to {}",
            before.describe(),
            mode.describe()
        )
    };

    if face.is_machine() {
        face.say(thalyx_files::machine::answer(
            "rehearse",
            vec![
                ("verb", json!(op_for(mode))),
                ("mode", json!(before.describe())),
                ("would_change", json!(!already)),
                // The human is asked before this one runs for real, and a
                // caller planning a sequence needs to know that before it gets
                // there rather than when it stalls.
                ("would_confirm", json!(mode == Mode::Observing)),
                ("message", json!(said)),
            ],
        ));
    } else {
        println!();
        println!("  {said}.");
        if mode == Mode::Observing && !already {
            println!("  It would ask first.");
        }
        println!();
    }
}

/// The trusted path, for the direction that takes protection away.
///
/// Same shape as `ejecutar`, minus the structured face, which was turned away
/// further up: a session with no terminal is not a session that consented, and
/// a read that failed is not a yes.
fn consented() -> Result<bool, Box<dyn std::error::Error>> {
    // Checked by `crate::ask`, after the context below rather than before it —
    // see the note in `foreign.rs`. On the display this is the difference
    // between a verb that can be finished and one that can only be read about.
    println!();
    println!("  This takes the kernel guard off the whole machine.");
    println!();
    println!("    Every policy stays written and none of them is applied.");
    println!("    Anything confined right now stops being confined, including");
    println!("    a program nobody signed that is running this second.");
    println!();
    // A read that failed is not a yes, and it is not an empty answer either.
    match crate::ask::confirm("  Stop denying? [y/N] ", &crate::ask::Accepts::Yes) {
        crate::ask::Answered::Yes => {}
        crate::ask::Answered::No => {
            println!();
            println!("  The guard stays on.");
            println!();
            return Ok(false);
        }
        crate::ask::Answered::NoOneToAsk => {
            println!();
            println!("  There is no terminal to confirm on, so the guard stays on.");
            println!("  Silence is not consent.");
            println!();
            return Ok(false);
        }
        crate::ask::Answered::Unreadable => {
            println!();
            println!("  Could not read the answer; the guard stays on.");
            println!();
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use thalyx_permd::MemoryStore;

    #[test]
    fn arming_moves_the_flag_the_hooks_read_and_not_only_what_is_printed() {
        let store = MemoryStore::observing();
        // The baseline. Without it, a store that was already enforcing and a
        // verb that works look identical.
        assert_eq!(store.enforcement(), Enforcement::Observing);

        set(&store, Mode::Enforcing, Face::Machine, false).expect("answers");

        assert_eq!(store.enforcement(), Enforcement::Enforcing);
    }

    #[test]
    fn the_structured_face_cannot_disarm_the_machine_and_the_flag_proves_it() {
        let store = MemoryStore::new();
        assert_eq!(store.enforcement(), Enforcement::Enforcing);

        set(&store, Mode::Observing, Face::Machine, false).expect("answers");

        // The control that matters. A refusal printed by a verb that switched
        // the mode anyway reads exactly like one that did not.
        assert_eq!(store.enforcement(), Enforcement::Enforcing);
    }

    #[test]
    fn a_rehearsal_of_either_direction_leaves_the_flag_where_it_was() {
        let store = MemoryStore::observing();

        set(&store, Mode::Enforcing, Face::Machine, true).expect("answers");
        assert_eq!(store.enforcement(), Enforcement::Observing);

        let armed = MemoryStore::new();
        set(&armed, Mode::Observing, Face::Machine, true).expect("answers");
        assert_eq!(armed.enforcement(), Enforcement::Enforcing);
    }

    #[test]
    fn a_machine_with_nothing_loaded_is_refused_rather_than_reported_as_switched() {
        let store = MemoryStore::unavailable();

        set(&store, Mode::Enforcing, Face::Machine, false).expect("answers");

        // `unavailable()` reports the mode as unreadable, which is the state a
        // detached machine is really in — and rule 10 says that is neither
        // "observing" nor a machine that can be armed.
        assert!(matches!(store.enforcement(), Enforcement::Unreadable(_)));
    }

    #[test]
    fn asking_for_the_mode_it_is_already_in_is_not_reported_as_a_change() {
        let store = MemoryStore::new();

        set(&store, Mode::Enforcing, Face::Machine, false).expect("answers");

        assert_eq!(store.enforcement(), Enforcement::Enforcing);
    }
}
