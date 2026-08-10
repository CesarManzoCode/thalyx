//! `intento` — begin something, and be able to take all of it back.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **D2**, and the
//! sentence [[Filosofia-Fundacional]] uses for the advantage no other operating
//! system has: *«intenta esto y si sale mal deshazlo»*.
//!
//! The reasoning lives in [`thalyx_core::attempt`], where it can be exercised on
//! a filesystem that is not Btrfs. What is here is the two faces and the one
//! decision that cannot be in the core: **who is asked before a tree is
//! replaced.** `Camino-Confiable` puts the confirmation in the layer that can
//! talk to a terminal, because a core that prompted could be made to prompt by
//! something that is not a human.
//!
//! ## Why abandoning asks and keeping does not
//!
//! Keeping touches nothing on the live tree; it only lets a snapshot go.
//! Abandoning replaces the tree, and the tree is **shared** — the person may
//! have written in it while the attempt was open, and their work is not the
//! agent's to discard because the agent changed its mind. So both faces are
//! shown exactly what would be lost before anything moves, and neither is
//! allowed to proceed on the first word.
//!
//! For a program that is one exchange and not a prompt: the first `abandonar`
//! answers with the difference and `done: false`, and the caller repeats it with
//! `si` to go ahead. It has seen the cost in the object it is answering, which
//! is what makes it a confirmation rather than a rubber stamp.
//!
//! ## What this container cannot check
//!
//! Btrfs. The policy — which attempt is open, what a second one does, what an
//! abandon aims at, what happens when the snapshot is gone — is covered by
//! `thalyx_core::attempt` against the directory fake. What only Cesar's machine
//! can exercise is that the snapshot is atomic, that it costs nothing, and that
//! the swap is a real `RENAME_EXCHANGE`. Here, `btrfs` is not installed at all,
//! so what runs is the refusal — and that the refusal is a refusal, and not a
//! copy pretending to be a snapshot, is itself worth a test.

use crate::files::{Face, Where};
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};
use thalyx_core::Store;
use thalyx_core::attempt::{self, Open};
use thalyx_snapshot::{Btrfs, Snapshots, Volumes};

type Fallible = Result<(), Box<dyn std::error::Error>>;

const OP: &str = "attempt";

/// The nearest subvolume at or above where the session is standing.
///
/// Walked upwards rather than assumed to be `/home`, because an attempt is
/// about the tree somebody is working in and Thalyx has more than one
/// subvolume. Returning `None` is an answer: on a filesystem with no subvolumes
/// there is nothing to snapshot, and the honest response is to say so instead of
/// copying a directory and calling it a snapshot — a copy is not atomic, takes
/// time proportional to the data, and something that took twenty minutes is a
/// picture of twenty minutes rather than of an instant.
fn subvolume_at_or_above(volumes: &Btrfs, from: &Path) -> Option<PathBuf> {
    let mut here = from.to_path_buf();
    loop {
        if volumes.is_subvolume(&here).unwrap_or(false) {
            return Some(here);
        }
        if !here.pop() {
            return None;
        }
    }
}

fn declined(face: Face, word: &str, why: &str) {
    if face == Face::Machine {
        println!("{}", thalyx_files::machine::declined(OP, word, why));
    } else {
        println!("\n  {why}\n");
    }
}

/// An open attempt, as both faces describe it.
fn open_fields(open: &Open) -> Vec<(&'static str, serde_json::Value)> {
    vec![
        ("attempt", json!(open.label)),
        ("snapshot", json!(open.snapshot)),
        ("subvolume", json!(open.subvolume.display().to_string())),
        ("since", json!(open.started_at)),
    ]
}

/// What a difference costs, in the words both faces use.
fn cost_fields(difference: &thalyx_snapshot::Difference) -> Vec<(&'static str, serde_json::Value)> {
    vec![
        // The number that decides the answer. A file made since the attempt
        // began has no older version to go back to: abandoning does not revert
        // it, it deletes it.
        ("would_delete", json!(difference.added_total)),
        ("would_revert", json!(difference.modified_total)),
        ("would_bring_back", json!(difference.removed_total)),
        // Counted as differences, because what could not be compared must not
        // be reported as identical.
        ("uncomparable", json!(difference.unreadable.len())),
        ("would_delete_named", json!(difference.added)),
        ("would_revert_named", json!(difference.modified)),
    ]
}

/// `intento [empezar <etiqueta> | confirmar | abandonar [si]]`.
pub fn run(store: &Store, here: &Where, rest: &str, face: Face, request_id: &str) -> Fallible {
    let rest = rest.trim();
    let (word, tail) = match rest.split_once(char::is_whitespace) {
        Some((word, tail)) => (word, tail.trim()),
        None => (rest, ""),
    };

    match word {
        "" => status(store, face),
        "empezar" | "start" | "iniciar" => begin(store, here, tail, face, request_id),
        "confirmar" | "keep" | "guardar" => keep(store, face, request_id),
        "abandonar" | "abandon" | "deshacer" => abandon(store, tail, face, request_id),
        other => {
            declined(
                face,
                "unknown_argument",
                &format!("`{other}` is not one of `empezar`, `confirmar`, `abandonar`"),
            );
            Ok(())
        }
    }
}

/// Whatever attempt is open, and what abandoning it would cost right now.
fn status(store: &Store, face: Face) -> Fallible {
    let open = match attempt::open(store) {
        Ok(open) => open,
        // Rule 10, and the sharpest case of it here: a record that could not be
        // read reported as "nothing open" is what would let a caller start a
        // second attempt over a live snapshot.
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    let Some(open) = open else {
        if face == Face::Machine {
            println!(
                "{}",
                thalyx_files::machine::answer(OP, vec![("open", json!(false))])
            );
        } else {
            println!();
            println!("  No attempt is open. `intento empezar <etiqueta>` starts one,");
            println!("  and everything after it can be taken back in one word.");
            println!();
        }
        return Ok(());
    };

    let snapshots = Snapshots::of(Btrfs::new(), &open.subvolume);
    let cost = attempt::what_abandoning_costs(store, &snapshots);

    if face == Face::Machine {
        let mut carried = vec![("open", json!(true))];
        carried.extend(open_fields(&open));
        match &cost {
            Ok((_, plan)) => {
                carried.push(("can_be_abandoned", json!(true)));
                carried.extend(cost_fields(&plan.difference));
            }
            // An attempt that can no longer be abandoned is still an open
            // attempt, and saying only the first half would leave a caller
            // believing it has a way back that it does not.
            Err(error) => {
                carried.push(("can_be_abandoned", json!(false)));
                carried.push(("why_not", json!(error.to_string())));
            }
        }
        println!("{}", thalyx_files::machine::answer(OP, carried));
        return Ok(());
    }

    println!();
    println!("  `{}` is open, since {}.", open.label, open.started_at);
    println!("  on {}", open.subvolume.display());
    match &cost {
        Ok((_, plan)) => {
            println!();
            println!("  {}", plan.what_it_costs());
            println!();
            println!("  `intento confirmar` keeps all of it.");
            println!("  `intento abandonar` puts it back as it was.");
        }
        Err(error) => {
            println!();
            println!("  {error}");
            println!("  This attempt can no longer be undone. `intento confirmar`");
            println!("  closes it and keeps the work that is there.");
        }
    }
    println!();
    Ok(())
}

fn begin(store: &Store, here: &Where, label: &str, face: Face, request_id: &str) -> Fallible {
    let label = if label.is_empty() { "attempt" } else { label };

    let volumes = Btrfs::new();
    let Some(subvolume) = subvolume_at_or_above(&volumes, here.at()) else {
        // Named rather than approximated. A copy of a directory is not a
        // snapshot, and a caller told "started" would make thirty changes
        // believing it could take them back.
        declined(
            face,
            "not_a_subvolume",
            &format!(
                "nothing at or above {} is a Btrfs subvolume, so there is nothing to \
                 come back to and no attempt was started",
                here.at().display()
            ),
        );
        return Ok(());
    };

    let snapshots = Snapshots::of(volumes, &subvolume);
    match attempt::begin(store, &snapshots, label, request_id) {
        Ok(open) => {
            if face == Face::Machine {
                let mut carried = vec![("open", json!(true)), ("began", json!(true))];
                carried.extend(open_fields(&open));
                // The two ways out, in the answer that opened it. A caller that
                // has to look up how to settle what it just started is one that
                // will leave attempts open.
                carried.push(("keep", json!("intento confirmar")));
                carried.push(("abandon", json!("intento abandonar")));
                println!("{}", thalyx_files::machine::answer(OP, carried));
            } else {
                println!();
                println!(
                    "  `{}` is open on {}.",
                    open.label,
                    open.subvolume.display()
                );
                println!("  Everything from here can be taken back with");
                println!("  `intento abandonar`, or kept with `intento confirmar`.");
                println!();
            }
            Ok(())
        }
        Err(error) => {
            let word = match &error {
                thalyx_core::CoreError::Attempt(
                    thalyx_core::attempt::AttemptError::AlreadyOpen(_),
                ) => "already_open",
                _ => "unreadable",
            };
            declined(face, word, &error.to_string());
            Ok(())
        }
    }
}

fn keep(store: &Store, face: Face, request_id: &str) -> Fallible {
    let open = match attempt::open(store) {
        Ok(Some(open)) => open,
        Ok(None) => {
            declined(face, "none_open", "no attempt is open");
            return Ok(());
        }
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    let snapshots = Snapshots::of(Btrfs::new(), &open.subvolume);
    match attempt::keep(store, &snapshots, request_id) {
        Ok(open) => {
            if face == Face::Machine {
                let mut carried = vec![("open", json!(false)), ("kept", json!(true))];
                carried.extend(open_fields(&open));
                // Said plainly, because it is what the caller just gave up.
                carried.push(("reversible", json!(false)));
                println!("{}", thalyx_files::machine::answer(OP, carried));
            } else {
                println!();
                println!("  `{}` is closed and the work is kept.", open.label);
                println!("  There is no longer a way back to where it started.");
                println!();
            }
            Ok(())
        }
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            Ok(())
        }
    }
}

fn abandon(store: &Store, tail: &str, face: Face, request_id: &str) -> Fallible {
    let open = match attempt::open(store) {
        Ok(Some(open)) => open,
        Ok(None) => {
            declined(face, "none_open", "no attempt is open");
            return Ok(());
        }
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    let snapshots = Snapshots::of(Btrfs::new(), &open.subvolume);
    let (open, plan) = match attempt::what_abandoning_costs(store, &snapshots) {
        Ok(both) => both,
        Err(error) => {
            declined(face, "snapshot_gone", &error.to_string());
            return Ok(());
        }
    };

    let said_yes = matches!(tail, "si" | "sí" | "yes" | "y");

    if !said_yes {
        // The confirmation, in whichever face is asking. Both are shown exactly
        // what would be lost first, because the tree is shared: a person may
        // have written in it while the attempt was open, and their work is not
        // the agent's to discard because the agent changed its mind.
        if face == Face::Machine {
            let mut carried = vec![
                ("open", json!(true)),
                ("done", json!(false)),
                ("needs", json!("confirmation")),
                ("confirm_with", json!("intento abandonar si")),
            ];
            carried.extend(open_fields(&open));
            carried.extend(cost_fields(&plan.difference));
            println!("{}", thalyx_files::machine::answer(OP, carried));
            return Ok(());
        }

        println!();
        println!(
            "  Abandoning `{}` would return {}",
            open.label,
            open.subvolume.display()
        );
        println!("  to how it was at {}.", open.started_at);
        println!();
        println!("  {}", plan.what_it_costs());
        for name in &plan.difference.added {
            println!("    {name}  — made since, would be deleted");
        }
        println!();

        if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            // Silence is not consent, and this is the one verb in the session
            // that can destroy a person's work.
            println!("  There is no terminal to confirm on, so nothing was undone.");
            println!("  `intento abandonar si` is the way to say yes without one.");
            println!();
            return Ok(());
        }
        print!("  Undo all of it? [y/N] ");
        let _ = std::io::stdout().flush();
        let answer = crate::term::read_answer()
            .ok()
            .flatten()
            .unwrap_or_default();
        if !matches!(answer.trim(), "y" | "Y" | "s" | "S" | "si" | "sí") {
            println!();
            println!("  Nothing was undone. `{}` is still open.", open.label);
            println!();
            return Ok(());
        }
    }

    match attempt::abandon(store, &snapshots, &open, &plan, request_id) {
        Ok(restored) => {
            if face == Face::Machine {
                let mut carried = vec![
                    ("open", json!(false)),
                    ("done", json!(true)),
                    ("abandoned", json!(true)),
                    // Two different guarantees, and an audit that cannot tell
                    // them apart cannot say whether an interruption was
                    // survivable.
                    ("atomic", json!(restored.atomic)),
                    ("replaced_kept_as", json!(restored.replaced_kept_as)),
                ];
                carried.extend(open_fields(&open));
                carried.extend(cost_fields(&plan.difference));
                println!("{}", thalyx_files::machine::answer(OP, carried));
            } else {
                println!();
                println!("  {} is back as it was.", open.subvolume.display());
                println!("  What was there is kept as {}.", restored.replaced_kept_as);
                println!();
            }
            Ok(())
        }
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            Ok(())
        }
    }
}
