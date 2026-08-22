//! `cambios` — what the kernel has seen change, and who did it.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **B3**. The
//! producing half has existed since `thalyx_watch.bpf.c` was written: every
//! mutation the hooks see is pushed into `thalyx_mut_ring`, and the comment
//! above that map has said since the day it was written that reading it needs a
//! consumer nobody had built. [[Tareas-Pendientes]] listed it as a ring buffer
//! that says *what* changed and that **nadie consume**.
//!
//! ## Two things the decree hoped for that a ring buffer cannot give
//!
//! Both are said in the answer rather than left to be discovered, because a
//! caller that assumes either one will be confidently wrong in a way it has no
//! way to check.
//!
//! **It is not a history.** A ring buffer is consumed: what one caller reads is
//! gone, and there is no going back to it. So *«qué cambió desde X»* for an X
//! older than the last read is not a question this can answer. What it answers
//! is what is in the ring now. A verb that reported a drain as a history would
//! tell two callers asking in turn two different confident stories, and neither
//! could tell.
//!
//! **It does not name files.** A record carries a cgroup, a pid, a kind and a
//! command name. The counter is machine-wide and the attribution map is
//! per-tree; neither is a path. What this is good for is *who* changed
//! something and *how* — which is the question `Marcado-de-Origen` cares about,
//! and the one that separates what the agent did from what the person did.
//!
//! ## What cannot be checked in the container this was written in
//!
//! The mapping. There is no BPF and no bpffs here, so `Ring::open` has never
//! run — only the protocol under it, which is a pure function over bytes and is
//! covered exhaustively in `thalyx_watch::ring`. `dev/verify.sh` stage 27 is the
//! other half, and it is Cesar's machine that runs it.

use crate::files::Face;
use serde_json::json;
use thalyx_watch::ring;

type Fallible = Result<(), Box<dyn std::error::Error>>;

const OP: &str = "changes";

/// `cambios [limite=N]` — drain what the kernel has queued.
pub fn show(rest: &str, face: Face) -> Fallible {
    let Some(given) = crate::words::asked(face, OP, rest) else {
        return Ok(());
    };
    let (extra, window) = match crate::index::asked_of(&given) {
        Ok(both) => both,
        Err(why) => {
            declined(face, "bad_cursor", &why.to_string());
            return Ok(());
        }
    };
    if !extra.is_empty() {
        declined(
            face,
            "unknown_argument",
            &format!("`{extra}` is not something `cambios` takes"),
        );
        return Ok(());
    }

    let pinned = ring::default_ring();
    let bpftool = std::path::PathBuf::from("bpftool");
    let Some(data_size) = ring::data_size_of(&pinned, &bpftool, !running_as_root()) else {
        // Rule 10, and the distinction that decides where somebody goes next:
        // the watcher not being loaded is a thing to go and load, and it is not
        // the same fact as a ring that is loaded and empty. Reporting the
        // second would leave a caller concluding that nothing on the machine
        // has changed — which is exactly the wrong conclusion.
        declined(
            face,
            "not_loaded",
            &format!(
                "nothing is pinned at {}, so what the kernel saw cannot be read — \
                 this is not the same as nothing having changed",
                pinned.display()
            ),
        );
        return Ok(());
    };

    let ring = match ring::Ring::open(&pinned, data_size) {
        Ok(ring) => ring,
        Err(error) => {
            declined(face, "unreadable", &error.to_string());
            return Ok(());
        }
    };

    let found = ring.drain();
    let numbered: Vec<(usize, ring::Mutation)> =
        found.records.iter().cloned().enumerate().collect();
    let page = match thalyx_files::window::page(numbered, record_key, &window) {
        Ok(page) => page,
        Err(why) => {
            declined(face, "unordered", &why.to_string());
            return Ok(());
        }
    };

    if face == Face::Machine {
        let rows: Vec<serde_json::Value> = page
            .rows
            .iter()
            .map(|(_, record)| {
                json!({
                    "kind": record.kind.word(),
                    "pid": record.pid,
                    // The field that separates what the agent did from what the
                    // person did, which is the whole reason this is worth
                    // reading at all.
                    "cgroup": record.cgroup_id,
                    "program": record.comm,
                })
            })
            .collect();

        let mut carried = vec![
            ("mutations", json!(rows)),
            // Said, not implied. Reading this emptied it: the same question
            // asked again will answer differently, and a caller that treated
            // this as a history it could re-read would be wrong quietly.
            ("is_a_history", json!(false)),
            ("consumed_by_reading", json!(true)),
            // What a record can and cannot say. A caller that needs to know
            // which file still has to walk the tree.
            ("names_paths", json!(false)),
            ("granularity", json!("actor_and_kind")),
            // Mutations the kernel saw and did not describe. Counted, so a
            // caller adding up what it was told is adding up what happened.
            ("discarded_by_the_kernel", json!(found.discarded)),
            ("unrecognised_records", json!(found.unexpected_size)),
            // Not an error: it is the ordinary end of a pass on a busy machine.
            // But a caller that reported "nothing more" would be wrong.
            ("more_being_written", json!(found.stopped_at_busy)),
        ];
        carried.extend(thalyx_files::machine::window_fields(&page));
        println!("{}", thalyx_files::machine::answer(OP, carried));
        return Ok(());
    }

    println!();
    if found.records.is_empty() {
        println!("  The kernel has queued nothing since this was last read.");
        println!("  Reading empties it, so this is not a history of the machine.");
        println!();
        return Ok(());
    }
    println!(
        "  {} change(s) the kernel saw, and reading them emptied the queue:",
        page.total
    );
    println!();
    for (_, record) in &page.rows {
        println!(
            "    {:<10} by {} ({}), in cgroup {}",
            record.kind.word(),
            record.comm,
            record.pid,
            record.cgroup_id
        );
    }
    if found.discarded > 0 || found.unexpected_size > 0 {
        println!();
        println!(
            "  {} the kernel did not describe, {} I did not recognise.",
            found.discarded, found.unexpected_size
        );
    }
    println!();
    println!("  These say who and how, never which file.");
    println!();
    Ok(())
}

/// What a cursor into a drained pass names: the position it came out at.
///
/// Position and not content. The records are in the order the kernel wrote
/// them, and that order is information — a rename then a delete is not the same
/// story as a delete then a rename — so any key built from the fields would
/// re-sort them and tell a different story. Two identical mutations by the same
/// program are also two events, and a content key would collide and page past
/// one of them.
fn record_key(numbered: &(usize, ring::Mutation)) -> Vec<u8> {
    (numbered.0 as u64).to_be_bytes().to_vec()
}

fn running_as_root() -> bool {
    thalyx_syscall::effective_uid() == 0
}

fn declined(face: Face, word: &str, why: &str) {
    if face == Face::Machine {
        println!("{}", thalyx_files::machine::declined(OP, word, why));
    } else {
        println!("\n  {why}\n");
    }
}
