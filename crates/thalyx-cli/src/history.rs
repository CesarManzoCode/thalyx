//! What was done to this machine, answered by the machine.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **F2**: *«qué se
//! hizo aquí y por qué» contestado por el sistema y no reconstruido de la
//! conversación*. The journal has been written since [[Journal-y-Snapshots]] and
//! read by exactly one thing — `thalyx journal`, a subcommand of the CLI. A
//! caller living in a session could not ask it anything.
//!
//! ## Why this is worth a verb
//!
//! It is the first cost, discovery, paid once instead of every session. An agent
//! that comes back tomorrow either reads what the system recorded, or
//! reconstructs it from a conversation that ended — and a reconstruction is a
//! guess wearing a fact's clothes. Worse, the two disagree in the one case that
//! matters: when something was rolled back, or was never committed, the
//! conversation says it was done and the journal says it was not.
//!
//! ## The caveat that is not optional
//!
//! The journal records **operations Thalyx performed**. It is not a record of
//! what happened to the machine: a person with a shell can move a file and
//! nothing here will know. The human face has said so in two lines since it was
//! written, and the structured face says it in a field — because a caller that
//! read this as "everything that happened" would conclude that nothing else did,
//! which is rule 10 with the two halves the wrong way round.

use crate::files::Face;
use serde_json::json;
use thalyx_core::Store;
use thalyx_journal::{Entry, Journal, Outcome};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// What the outcome is, as a word a program matches on and a sentence to relay.
///
/// Written out rather than derived from the enum, for the reason the whole
/// machine face is written out: a derived name is decided by Rust's variant
/// spelling, so renaming `NotCommitted` would silently rename a field somebody
/// else parses.
fn outcome_fields(outcome: &Outcome) -> (&'static str, Option<&str>) {
    match outcome {
        Outcome::Intended => ("intended", None),
        Outcome::Success => ("success", None),
        Outcome::Rejected { reason } => ("rejected", Some(reason)),
        Outcome::NotCommitted { reason } => ("not_committed", Some(reason)),
        Outcome::Degraded { reason } => ("degraded", Some(reason)),
    }
}

fn entry_object(entry: &Entry) -> serde_json::Value {
    let (outcome, reason) = outcome_fields(&entry.outcome);
    json!({
        "at": entry.timestamp,
        "operation": entry.operation,
        "module": entry.module_id,
        "version": entry.version,
        "outcome": outcome,
        // `null` and not absent when there is none. A key that appears only on
        // the bad day is a key nobody handles on the bad day.
        "reason": reason,
        // Whether this line settles anything. An `intended` with no terminal
        // entry after it is an operation that was interrupted, and a caller
        // that could not tell would report it as done.
        "settled": entry.outcome.is_terminal(),
        "request_id": entry.request_id,
        // Where the field that motivated this came from —
        // `vault/11-Seguridad/Marcado-de-Origen.md`. It is what lets a human
        // separate what a model did from what they did.
        "origin": match entry.origin {
            thalyx_journal::Origin::UserUtterance => "user_utterance",
            thalyx_journal::Origin::SystemState => "system_state",
            thalyx_journal::Origin::UntrustedContent => "untrusted_content",
        },
        "snapshot": entry.snapshot,
        "notes": entry.notes,
    })
}

/// What a cursor into the history names.
///
/// The entry's position in the file, **inverted**, because the answer is newest
/// first and the window pages by an ascending key. The position itself is
/// stable — the journal is append-only, so entry seven is entry seven forever —
/// which is what makes a cursor into it mean the same thing on the next call.
fn history_key(numbered: &(usize, Entry)) -> Vec<u8> {
    (u64::MAX - numbered.0 as u64).to_be_bytes().to_vec()
}

/// `historia [limite=N] [cursor=…]` — what this machine did, newest first.
pub fn show(store: &Store, rest: &str, face: Face) -> Fallible {
    let op = "history";

    let Some(given) = crate::words::asked(face, op, rest) else {
        return Ok(());
    };
    let (extra, window) = match crate::index::asked_of(&given) {
        Ok(both) => both,
        Err(why) => {
            declined(face, op, "bad_cursor", &why.to_string());
            return Ok(());
        }
    };
    if !extra.is_empty() {
        // Named rather than ignored. A caller that typed a filter this does not
        // have would otherwise read a full history as an answer to a narrower
        // question, and nothing would say the filter did nothing.
        declined(
            face,
            op,
            "unknown_argument",
            &format!("`{extra}` is not something `historia` takes"),
        );
        return Ok(());
    }

    let journal = match Journal::open(store.journal_path()) {
        Ok(journal) => journal,
        Err(error) => {
            declined(face, op, "unreadable", &error.to_string());
            return Ok(());
        }
    };
    let entries = match journal.entries() {
        Ok(entries) => entries,
        // Rule 10: a failure to read is not a failure to exist. An empty
        // history and an unreadable one are different facts, and only one of
        // them means nothing has been done here.
        Err(error) => {
            declined(face, op, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    let mut numbered: Vec<(usize, Entry)> = entries.into_iter().enumerate().collect();
    numbered.reverse();

    let page = match thalyx_files::window::page(numbered, history_key, &window) {
        Ok(page) => page,
        Err(why) => {
            declined(face, op, "unordered", &why.to_string());
            return Ok(());
        }
    };

    if face == Face::Machine {
        let rows: Vec<serde_json::Value> = page
            .rows
            .iter()
            .map(|(_, entry)| entry_object(entry))
            .collect();
        let mut carried = vec![
            ("entries", json!(rows)),
            // The caveat the human face has printed since it was written, as a
            // field. A caller that read this as "everything that happened"
            // would conclude that nothing else did.
            ("covers", json!("operations_thalyx_performed")),
            ("complete_record_of_the_machine", json!(false)),
        ];
        carried.extend(thalyx_files::machine::window_fields(&page));
        println!("{}", thalyx_files::machine::answer(op, carried));
        return Ok(());
    }

    println!();
    if page.total == 0 {
        println!("  nothing has been done here that I recorded.");
        println!();
        return Ok(());
    }
    println!(
        "  showing {} of {}, newest first",
        page.rows.len(),
        page.total
    );
    println!("  this is what Thalyx did, not everything that happened.");
    println!();
    for (_, entry) in &page.rows {
        let (outcome, reason) = outcome_fields(&entry.outcome);
        let subject = match (&entry.module_id, &entry.version) {
            (Some(id), Some(version)) => format!("{id} {version}"),
            (Some(id), None) => id.clone(),
            _ => "—".to_string(),
        };
        let tail = match reason {
            Some(reason) => format!("  — {reason}"),
            None => String::new(),
        };
        println!(
            "    {}  {:<9}  {:<12} {}{}",
            entry.timestamp, outcome, entry.operation, subject, tail
        );
    }
    if page.more {
        println!();
        // The way to the rest, in the answer that cut it. A person told there is
        // more and not how to reach it has been told about a wall.
        println!(
            "  {} older. `historia limite=50` shows more.",
            page.total - page.rows.len()
        );
    }
    println!();
    Ok(())
}

fn declined(face: Face, op: &str, word: &str, why: &str) {
    if face == Face::Machine {
        println!("{}", thalyx_files::machine::declined(op, word, why));
    } else {
        println!("\n  {why}\n");
    }
}
