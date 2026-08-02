//! `thalyx module run` — the human's route to running a module confined.
//!
//! Everything of substance is in `thalyx_core::run`. This file is the CLI's
//! half: it collects arguments, hands them over, and reports what happened
//! plainly enough that "confined" and "unconfined" cannot be confused.

use std::ffi::OsString;
use thalyx_core::Store;
use thalyx_journal::Origin;
use thalyx_permd::BpftoolStore;

type Fallible = Result<(), Box<dyn std::error::Error>>;

pub fn run(
    root: &std::path::Path,
    module_id: &str,
    entrypoint: &str,
    args: Vec<OsString>,
    unconfined: bool,
    request_id: String,
) -> Fallible {
    let store = Store::open(root)?;
    let policies = BpftoolStore::default_map();

    // The helper is this binary. It re-executes itself into the module's
    // cgroup and only then becomes the module, so the module's first
    // instruction runs in a process that is already confined.
    let helper = std::env::current_exe()?;

    let outcome = thalyx_core::run(
        &store,
        &policies,
        thalyx_core::RunRequest {
            module_id,
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
            for permission in &outcome.permissions {
                println!("    {}", permission.describe());
            }
            if outcome.permissions.is_empty() {
                println!("    (no permissions; every guarded operation is denied)");
            }
        }
        _ => {
            println!("  RAN UNCONFINED — nothing enforced its permissions.");
            println!("  The journal records this run as degraded.");
        }
    }

    match outcome.exit_code {
        Some(0) => println!("  exited cleanly"),
        Some(code) => println!("  exited with status {code}"),
        None => println!("  terminated by a signal"),
    }

    Ok(())
}
