//! Terminal output, and the terminal end of the trusted path.

use std::io::{IsTerminal, Write};
use thalyx_core::install::Confirmer;
use thalyx_core::permissions::Registry;
use thalyx_core::trusted_path::CapabilityPrompt;
use thalyx_core::{Store, keystore::Keystore};
use thalyx_journal::{Journal, Outcome};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// Displays a core-generated prompt and reads the answer.
///
/// Note what this type cannot do: it receives a [`CapabilityPrompt`] the core
/// built and prints it. It has no way to alter the text, because there is no
/// text parameter — the prompt carries structured fields only.
/// See `vault/11-Seguridad/Camino-Confiable.md`.
pub struct TerminalConfirmer {
    assume_yes: bool,
}

impl TerminalConfirmer {
    pub fn new(assume_yes: bool) -> Self {
        Self { assume_yes }
    }
}

impl Confirmer for TerminalConfirmer {
    fn confirm(&mut self, prompt: &CapabilityPrompt) -> bool {
        println!("{}", prompt.render());

        if self.assume_yes {
            println!("  confirmed with --yes");
            return true;
        }

        if !std::io::stdin().is_terminal() {
            // Silence is not consent. A non-interactive caller that wants to
            // accept has to say so with --yes.
            eprintln!("  no terminal available to confirm; refusing");
            return false;
        }

        print!("  Confirm? [y/N] ");
        let _ = std::io::stdout().flush();

        let mut answer = String::new();
        if std::io::stdin().read_line(&mut answer).is_err() {
            return false;
        }
        matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    }
}

pub fn module_list(store: &Store) -> Fallible {
    let installed = store.installed()?;
    if installed.is_empty() {
        println!("no modules installed");
        return Ok(());
    }

    let keystore = Keystore::load(store.keystore_path())?;
    let registry = Registry::load(store.permissions_path())?;

    for (id, version) in installed {
        let permissions = registry.effective(&id).len();
        let pinned = keystore
            .pinned(&id)
            .map(|k| format!("{}…", &k.key[..24.min(k.key.len())]))
            .unwrap_or_else(|| "unpinned".to_string());
        println!("{id}  {version}");
        println!("  publisher {pinned}");
        println!("  {permissions} permission(s) in force");
    }
    Ok(())
}

pub fn journal(store: &Store, limit: usize) -> Fallible {
    let journal = Journal::open(store.journal_path())?;
    let entries = journal.entries()?;

    if entries.is_empty() {
        println!("journal is empty");
        return Ok(());
    }

    println!(
        "showing {} of {} entries",
        limit.min(entries.len()),
        entries.len()
    );
    println!("note: the journal records only operations Thalyx performed.");
    println!("      it is not a complete record of what happened to the system.");
    println!();

    for entry in entries.iter().rev().take(limit).rev() {
        let (marker, detail) = match &entry.outcome {
            Outcome::Intended => ("→   ", "  — about to commit".to_string()),
            Outcome::Success => ("ok  ", String::new()),
            Outcome::Rejected { reason } => ("rej ", format!("  — {reason}")),
            Outcome::NotCommitted { reason } => ("fail", format!("  — {reason}")),
            Outcome::Degraded { reason } => ("warn", format!("  — {reason}")),
        };
        let subject = match (&entry.module_id, &entry.version) {
            (Some(id), Some(version)) => format!("{id} {version}"),
            (Some(id), None) => id.clone(),
            _ => "—".to_string(),
        };
        println!(
            "{marker} {}  {}  {subject}{detail}",
            entry.timestamp, entry.operation
        );
        for note in &entry.notes {
            println!("       {note}");
        }
    }
    Ok(())
}

pub fn permissions(store: &Store) -> Fallible {
    let registry = Registry::load(store.permissions_path())?;

    // A grant is in force only while its module is the current version.
    // Listing the raw registry would show permissions for modules that are not
    // installed, which is precisely the orphan grant the design forbids —
    // and worse, it would show it to the human as if it were real.
    let mut in_force = Vec::new();
    let mut inert = Vec::new();

    for (module_id, grants) in registry.all() {
        if store.is_installed(module_id) {
            in_force.push((module_id, grants));
        } else {
            inert.push((module_id, grants.len()));
        }
    }

    if in_force.is_empty() {
        println!("no permissions in force");
    }

    for (module_id, grants) in in_force {
        println!("{module_id}");
        for grant in grants {
            println!(
                "  {} {} ({}) granted {}",
                grant.action, grant.resource, grant.kind, grant.granted_at
            );
        }
    }

    if !inert.is_empty() {
        println!();
        println!("inert records ({} module(s) not installed):", inert.len());
        for (module_id, count) in inert {
            println!("  {module_id}  {count} recorded, none in force");
        }
        println!();
        println!("These grant nothing: a permission holds only while its module");
        println!("is current. `thalyx store clean` clears the records.");
    }

    Ok(())
}

pub fn store_status(store: &Store) -> Fallible {
    println!("root      {}", store.root().display());
    println!("modules   {}", store.installed()?.len());

    let orphans = store.orphaned_versions()?;
    let staging = std::fs::read_dir(store.staging_root())
        .map(|entries| entries.count())
        .unwrap_or(0);

    let unresolved = Journal::open(store.journal_path())?.unresolved_intents()?;

    println!("staging   {staging} leftover director(ies)");
    println!("intents   {} unresolved", unresolved.len());
    for intent in &unresolved {
        let subject = match (&intent.module_id, &intent.version) {
            (Some(id), Some(version)) => format!("{id} {version}"),
            _ => "—".to_string(),
        };
        println!("          {subject}  announced {}", intent.timestamp);
    }
    println!(
        "orphans   {} version(s) not pointed at by `current`",
        orphans.len()
    );

    for (id, version) in &orphans {
        println!("          {id} {version}");
    }

    if !unresolved.is_empty() {
        println!();
        println!("an unresolved intent is a question, not a lost operation: the disk");
        println!("has the answer. `thalyx store reconcile` settles them.");
    }

    if orphans.is_empty() && staging == 0 && unresolved.is_empty() {
        println!();
        println!("store is consistent.");
    } else if !orphans.is_empty() || staging > 0 {
        println!();
        println!("leftovers are inert: nothing points at them, so no module is");
        println!("half-installed. `thalyx store clean` reclaims the space.");
    }
    Ok(())
}
