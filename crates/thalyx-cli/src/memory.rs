//! `thalyx memory` — what the agent remembers between sessions.
//!
//! Phase 1 has no agent, so this is a human driving the primitive by hand.
//! That is the double-route principle doing its job early: whatever the agent
//! will be able to record and recall, a person can record and recall here
//! first, and the two go through the same code.
//!
//! Everything printed here keeps the two separations the decree asks for.
//! Facts and notes are never listed together, and a fact that can no longer be
//! checked is never printed as though it could.

use clap::Subcommand;
use std::path::{Path, PathBuf};
use thalyx_memory::{LexicalEmbedder, Memory, Recall, Recollection, Standing, Witness};

type Fallible = Result<(), Box<dyn std::error::Error>>;

#[derive(Subcommand)]
pub enum MemoryCommand {
    /// Record something that happened
    Remember {
        task: String,
        text: String,
        /// A path this fact is about. Repeat for several.
        ///
        /// This is what the fact is checked against later. A fact recorded
        /// with none can never be confirmed, and says so when recalled.
        #[arg(long = "about")]
        about: Vec<PathBuf>,
    },
    /// Record something the agent worked out. Discardable.
    Note { task: String, text: String },
    /// Everything remembered about a task, re-checked now
    Recall { task: String },
    /// Find records that look like some text
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Drop the inferences for a task, keeping the facts
    ForgetNotes { task: String },
    /// Tasks with something remembered about them
    Tasks,
}

pub fn run(store_root: &Path, command: MemoryCommand) -> Fallible {
    let path = store_root.join("state").join("memory.db");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let embedder = LexicalEmbedder;
    let memory = Memory::open(&path, &embedder)?;

    match command {
        MemoryCommand::Remember { task, text, about } => {
            let witness = Witness::over(&about);
            memory.remember_fact(&task, &text, &witness, &embedder)?;

            println!("recorded a fact about {task}");
            if witness.is_empty() {
                println!();
                println!("Nothing was given for it to be checked against, so it can never");
                println!("be confirmed later. Pass --about <path> for a fact that should");
                println!("stop being asserted when the thing it describes changes.");
            } else {
                println!("  checked against {} path(s)", witness.paths.len());
            }
            Ok(())
        }

        MemoryCommand::Note { task, text } => {
            memory.note(&task, &text, &embedder)?;
            println!("recorded a note about {task}");
            println!("  notes are the agent's inference, and `forget-notes` drops them");
            Ok(())
        }

        MemoryCommand::Recall { task } => {
            let recalled = memory.recall(&task)?;
            print_recollection(&task, &recalled);
            Ok(())
        }

        MemoryCommand::Search { query, limit } => {
            let recall = memory.search(&query, limit, &embedder)?;
            print_recall(&recall);
            Ok(())
        }

        MemoryCommand::ForgetNotes { task } => {
            let dropped = memory.forget_notes(&task)?;
            println!("dropped {dropped} note(s) about {task}");
            println!("  the facts are untouched; there is no way to delete one");
            Ok(())
        }

        MemoryCommand::Tasks => {
            let tasks = memory.tasks()?;
            if tasks.is_empty() {
                println!("nothing remembered yet");
                return Ok(());
            }
            for task in tasks {
                println!("{task}");
            }
            Ok(())
        }
    }
}

fn print_recollection(task: &str, recalled: &Recollection) {
    if recalled.is_empty() {
        println!("nothing remembered about {task}");
        return;
    }

    // Facts first, and under their own heading. The two layers are never
    // interleaved: a reader skimming this must not be able to mistake one for
    // the other.
    if !recalled.facts.is_empty() {
        println!("what happened");
        for fact in &recalled.facts {
            let marker = match fact.standing {
                Standing::Verified => "ok  ",
                Standing::Unverified { .. } => "STALE",
                Standing::Unwitnessed => "?   ",
            };
            println!("  {marker} {}", fact.record.text);
            if !matches!(fact.standing, Standing::Verified) {
                println!("        {}", fact.standing.describe());
            }
        }
    }

    if !recalled.notes.is_empty() {
        println!();
        println!("what the agent worked out (inference, not record)");
        for note in &recalled.notes {
            println!("  · {}", note.text);
        }
    }

    let unverifiable = recalled.no_longer_verifiable().count();
    if unverifiable > 0 {
        println!();
        println!("{unverifiable} fact(s) can no longer be checked. They are kept, because");
        println!("no longer verifiable is not the same as false — something they");
        println!("describe changed without going through Thalyx.");
    }
}

fn print_recall(recall: &Recall) {
    if recall.hits.is_empty() {
        println!("nothing matched");
        println!();
        println!("{}", recall.describe());
        return;
    }

    for hit in &recall.hits {
        let layer = match hit.record.layer {
            thalyx_memory::Layer::Fact => "fact",
            thalyx_memory::Layer::Note => "note",
        };
        println!(
            "{:.2}  {layer}  {}  {}",
            hit.similarity, hit.record.task, hit.record.text
        );
        if let Some(standing) = &hit.standing
            && !standing.is_verified()
        {
            println!("            {}", standing.describe());
        }
    }

    // Always, and never as a footnote nobody reads. What kind of matching
    // produced these results changes what they are worth.
    println!();
    println!("{}", recall.describe());
}
