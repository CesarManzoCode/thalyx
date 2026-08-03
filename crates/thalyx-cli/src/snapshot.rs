//! `thalyx snapshot` — moments of a subvolume, kept so one can be returned to.
//!
//! Taking one is cheap and safe: Btrfs shares the blocks, nothing is copied,
//! and nothing existing is touched. Returning to one is neither, which is why
//! it is `thalyx restore` and not a flag here.

use clap::Subcommand;
use std::path::PathBuf;
use thalyx_core::Store;
use thalyx_snapshot::{Btrfs, Snapshots, Volumes};

type Fallible = Result<(), Box<dyn std::error::Error>>;

#[derive(Subcommand)]
pub enum SnapshotCommand {
    /// Keep this moment of a subvolume
    Take {
        /// The subvolume. Defaults to the current directory.
        #[arg(default_value = ".")]
        subvolume: PathBuf,
        /// A word for what this moment is, added to the name
        #[arg(long, default_value = "manual")]
        label: String,
    },
    /// Moments that have been kept
    List {
        #[arg(default_value = ".")]
        subvolume: PathBuf,
    },
    /// Delete a snapshot
    ///
    /// The moment it held cannot be recovered. It does not touch the live
    /// tree — that is `restore`, which is a different command for a reason.
    Forget {
        name: String,
        #[arg(default_value = ".")]
        subvolume: PathBuf,
    },
}

pub fn run(store: &Store, command: SnapshotCommand, request_id: &str) -> Fallible {
    match command {
        SnapshotCommand::Take { subvolume, label } => {
            let subvolume = subvolume.canonicalize()?;
            let snapshots = Snapshots::of(Btrfs::new(), &subvolume);

            let taken = thalyx_core::snapshots::take(store, &snapshots, &label, request_id)?;

            println!("kept {}", taken.name);
            println!("  of  {}", subvolume.display());
            println!("  at  {}", taken.path.display());
            println!();
            println!("Btrfs shares the blocks, so this cost almost nothing and copied");
            println!("nothing. `thalyx restore {}` returns to it.", taken.name);
            Ok(())
        }

        SnapshotCommand::List { subvolume } => {
            let subvolume = subvolume.canonicalize()?;
            let volumes = Btrfs::new();

            // Said before an empty list, because "no snapshots" and "this is
            // not a subvolume, so there could never be any" are different
            // facts that produce identical output.
            match volumes.is_subvolume(&subvolume) {
                Ok(false) => {
                    println!("{} is not a Btrfs subvolume.", subvolume.display());
                    println!("Nothing here can be snapshotted, so there is nothing to list.");
                    return Ok(());
                }
                Ok(true) => {}
                Err(error) => {
                    println!("could not ask Btrfs about {}: {error}", subvolume.display());
                    return Ok(());
                }
            }

            let snapshots = Snapshots::of(volumes, &subvolume);
            let kept = snapshots.list()?;

            if kept.is_empty() {
                println!("no snapshots of {}", subvolume.display());
                println!("`thalyx snapshot take` keeps this moment.");
                return Ok(());
            }

            println!("snapshots of {}", subvolume.display());
            for snapshot in &kept {
                println!("  {}", snapshot.name);
            }
            println!();
            println!("Oldest first. The name carries the moment it was taken, which is");
            println!("why the order is the name's and not the filesystem's — a snapshot");
            println!("keeps its source's timestamps, not its own.");
            Ok(())
        }

        SnapshotCommand::Forget { name, subvolume } => {
            let subvolume = subvolume.canonicalize()?;
            let snapshots = Snapshots::of(Btrfs::new(), &subvolume);

            thalyx_core::snapshots::forget(store, &snapshots, &name, request_id)?;

            println!("forgot {name}");
            println!("  the live tree is untouched; the moment is gone for good");
            Ok(())
        }
    }
}
