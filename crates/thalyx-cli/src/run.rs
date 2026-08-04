//! `thalyx module run` — the human's route to running a module confined.
//!
//! Everything of substance is in `thalyx_core::run`. This file is the CLI's
//! half: it collects arguments, hands them over, and reports what happened
//! plainly enough that "confined" and "unconfined" cannot be confused.

use std::ffi::OsString;
use thalyx_core::Store;
use thalyx_journal::Origin;
use thalyx_permd::KernelStore;

type Fallible = Result<(), Box<dyn std::error::Error>>;

pub fn run(
    root: &std::path::Path,
    module_id: &str,
    profile: &str,
    entrypoint: &str,
    args: Vec<OsString>,
    unconfined: bool,
    request_id: String,
) -> Fallible {
    let store = Store::open(root)?;
    let policies = KernelStore::default_map();

    // The helper is this binary. It re-executes itself into the module's
    // cgroup and only then becomes the module, so the module's first
    // instruction runs in a process that is already confined.
    let helper = std::env::current_exe()?;

    let outcome = thalyx_core::run(
        &store,
        &policies,
        thalyx_core::RunRequest {
            module_id,
            profile,
            entrypoint,
            args,
            helper,
            request_id,
            origin: Origin::UserUtterance,
            unconfined,
        },
    )?;

    println!();
    println!("{} {}", outcome.module_id, outcome.version);
    println!("  ran: {}", outcome.program.display());

    match (outcome.cgroup_id, outcome.policy) {
        (Some(id), Some(policy)) => {
            println!("  confined to cgroup {id}, allowed=0x{:x}", policy.allowed);
            if let Some(isolation) = &outcome.isolation {
                println!("  {isolation}");
            }
            if let Some(uid) = outcome.uid {
                println!("  ran as user {uid}, which is this module's and no other's");
            }
            for permission in &outcome.permissions {
                println!("    {}", permission.describe());
            }
            if outcome.permissions.is_empty() {
                println!("    (no permissions; every guarded operation is denied)");
            }
            if !outcome.isolated {
                println!();
                println!("  WARNING: this profile isolates nothing beyond the cgroup.");
                println!("  The journal records this run as degraded.");
            }
        }
        _ => {
            println!("  RAN UNCONFINED — nothing enforced its permissions.");
            println!("  The journal records this run as degraded.");
        }
    }

    // What the module said over its channel.
    //
    // Printed by Thalyx and not by the module, which is the whole arrangement:
    // a module has no terminal, and everything it wants a human to see passes
    // through here. That is also why the marker says who is speaking — text
    // from a module must never be able to look like Thalyx talking.
    //
    // And why every line goes through `sanitise`. Routing the text through
    // Thalyx accomplishes nothing on its own if the text may then contain a
    // newline and repaint the marker, or an escape sequence and repaint the
    // screen. The marker is only a marker if the module cannot draw one.
    if !outcome.said.is_empty() {
        println!();
        println!(
            "  {} said:",
            thalyx_core::trusted_path::sanitise(&outcome.module_id)
        );
        for (level, text) in &outcome.said {
            let marker = match level {
                thalyx_abi::Level::Info => " ",
                thalyx_abi::Level::Warning => "!",
                thalyx_abi::Level::Error => "x",
            };
            for line in thalyx_core::trusted_path::sanitise_block(text) {
                println!("  {marker} {line}");
            }
        }
    }

    // A module that said more than Thalyx will hold has to be reported as
    // such. A list that silently stopped growing looks exactly like a module
    // that stopped talking, and the two are different events.
    if outcome.dropped_notices > 0 {
        println!(
            "  … and {} more notice(s), past what Thalyx keeps for one run.",
            outcome.dropped_notices
        );
    }

    if let Some(error) = &outcome.channel_error {
        println!();
        println!("  the module's channel to Thalyx broke: {error}");
        println!("  anything it asked for after that point did not happen.");
    }

    println!();
    match outcome.exit_code {
        Some(0) => println!("  exited cleanly"),
        Some(code) => println!("  exited with status {code}"),
        None => println!("  terminated by a signal"),
    }

    Ok(())
}
