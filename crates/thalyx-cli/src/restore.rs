//! `thalyx restore` — the destructive one.
//!
//! `vault/04-Flujo-Canonico/Rollback-vs-Restore.md` gives this its own name
//! precisely so it can never be reached by somebody who meant the cheap one.
//! Everything here exists to make sure the human who answers has seen what the
//! answer costs.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use thalyx_core::Store;
use thalyx_core::trusted_path::RestorePrompt;
use thalyx_snapshot::{Btrfs, Snapshots};

type Fallible = Result<(), Box<dyn std::error::Error>>;

pub fn run(
    store: &Store,
    snapshot: &str,
    subvolume: PathBuf,
    assume_yes: bool,
    request_id: &str,
) -> Fallible {
    let subvolume = subvolume.canonicalize()?;
    let snapshots = Snapshots::of(Btrfs::new(), &subvolume);

    let plan = thalyx_core::restore::plan(&snapshots, snapshot)?;

    // It stops here, always. The decree's state check is not a filter that
    // lets quiet cases through — drift is the *normal* case, since the whole
    // point is a human undoing their own work. What it forbids is going ahead
    // without them having been told.
    if plan.is_a_no_op() {
        println!("{}", plan.what_it_costs());
        println!();
        println!("Nothing to do. The subvolume already matches that snapshot.");
        return Ok(());
    }

    let prompt = RestorePrompt {
        snapshot: plan.snapshot.clone(),
        subvolume: plan.subvolume.display().to_string(),
        deleted: plan.difference.added_total,
        reverted: plan.difference.modified_total,
        returned: plan.difference.removed_total,
        examples: plan.difference.added.clone(),
        unreadable: plan.difference.unreadable.len(),
    };
    println!("{}", prompt.render());
    println!();

    if !confirmed(assume_yes, plan.difference.added_total)? {
        println!("nothing was restored.");
        return Ok(());
    }

    let restored = thalyx_core::restore::apply(store, &snapshots, &plan, request_id)?;

    println!();
    println!("restored to {}", restored.snapshot);
    println!("  what was there is kept as {}", restored.replaced_kept_as);
    if !restored.atomic {
        // Said out loud rather than buried in the journal. The guarantee this
        // run got is weaker than the one the design describes, and the human
        // is the one who would have to recover from it.
        println!();
        println!("This filesystem has no atomic exchange, so there was a moment with");
        println!("no tree in place. The journal records that.");
    }
    Ok(())
}

/// Ask, over the trusted path.
///
/// Silence is not consent: with no terminal there is nobody to ask, and the
/// answer is no. That is the same rule the capability prompt follows, and it
/// matters more here — a restore driven by a script that nobody was watching
/// is exactly the thing that turns a safety feature into a data loss report.
fn confirmed(assume_yes: bool, deleted: usize) -> std::io::Result<bool> {
    if assume_yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        println!("refusing: there is no terminal to confirm on, and silence is not consent.");
        println!("Pass --yes only from something a human is watching.");
        return Ok(false);
    }

    // Typing the word, not a keystroke. `y` is muscle memory; the number of
    // files about to be deleted is on the screen right above this line, and
    // having to write the word is the last chance to read it.
    print!("Type `restore` to destroy {deleted} file(s) and go back: ");
    std::io::stdout().flush()?;

    let answer = crate::term::read_answer()?.unwrap_or_default();
    Ok(answer.trim() == "restore")
}
