//! `thalyx enforce` — pushing granted permissions into the kernel.
//!
//! The core records what a module is allowed to do. The LSM enforces what it
//! is told. This is the command that carries one to the other, and until it
//! runs, a module's permissions are bookkeeping rather than protection.
//!
//! It reports that difference plainly rather than hiding it. A permission
//! shown as granted while nothing enforces it is the failure mode with no
//! symptom, and the only defence against it is saying so out loud.

use clap::Subcommand;
use std::path::PathBuf;
use thalyx_core::Store;
use thalyx_core::permissions::Registry;
use thalyx_permd::{BpftoolStore, PolicyStore};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// How long a JIT grant lasts before the kernel stops honouring it.
const JIT_LIFETIME_NS: u64 = 30 * 1_000_000_000;

#[derive(Subcommand)]
pub enum EnforceCommand {
    /// Report whether the kernel is enforcing, and for whom
    Status,
    /// Push a module's granted permissions into the kernel
    Apply {
        module_id: String,
        /// The cgroup the module runs in
        #[arg(long)]
        cgroup: PathBuf,
    },
    /// Withdraw a module's policy from the kernel
    Revoke {
        /// The cgroup the module runs in
        #[arg(long)]
        cgroup: PathBuf,
    },
}

pub fn run(store_root: &std::path::Path, command: EnforceCommand) -> Fallible {
    let store = Store::open(store_root)?;
    let kernel = BpftoolStore::default_map();

    match command {
        EnforceCommand::Status => status(&store, &kernel),

        EnforceCommand::Apply { module_id, cgroup } => {
            require_kernel(&kernel)?;

            // Only what is actually in force. A grant recorded for a module
            // that is not the current version is inert, and pushing it to the
            // kernel would make it real — turning a leftover record into an
            // actual permission.
            let registry = Registry::load(store.permissions_path())?;
            let grants = thalyx_core::effective_permissions(&store, &registry, &module_id);

            if grants.is_empty() {
                println!("{module_id} holds no permissions in force; nothing to apply");
                return Ok(());
            }

            let permissions: Vec<thalyx_manifest::Permission> = grants
                .iter()
                .map(|grant| thalyx_manifest::Permission {
                    resource: grant.resource.clone(),
                    action: grant.action.clone(),
                    kind: grant.kind,
                })
                .collect();

            let id = thalyx_permd::cgroup_id(&cgroup)?;
            let policy = thalyx_permd::apply(&kernel, id, &permissions, now_ns(), JIT_LIFETIME_NS)?;

            println!("{module_id} → cgroup {id}");
            for permission in &permissions {
                println!("  {}", permission.describe());
            }
            println!();
            println!("policy written: allowed=0x{:x}", policy.allowed);
            if policy.expires_ns > 0 {
                println!("  expires on its own; the kernel enforces the deadline");
            }
            Ok(())
        }

        EnforceCommand::Revoke { cgroup } => {
            require_kernel(&kernel)?;
            let id = thalyx_permd::cgroup_id(&cgroup)?;
            thalyx_permd::revoke(&kernel, id)?;
            println!("policy for cgroup {id} withdrawn");
            println!("  takes effect at the next hook; nothing has to be notified");
            Ok(())
        }
    }
}

fn status(store: &Store, kernel: &BpftoolStore) -> Fallible {
    let available = kernel.is_available();

    println!(
        "kernel policy map: {}",
        if available { "present" } else { "NOT PRESENT" }
    );

    let registry = Registry::load(store.permissions_path())?;
    let installed = store.installed()?;

    let mut recorded = 0;
    for (id, _) in &installed {
        recorded += thalyx_core::effective_permissions(store, &registry, id).len();
    }

    println!("modules installed:  {}", installed.len());
    println!("permissions in force (recorded): {recorded}");

    println!();
    if !available {
        println!("The kernel side is not loaded, so nothing is enforced.");
        println!("Whatever the permission registry says, every installed module");
        println!("currently runs unconstrained.");
        println!();
        println!("  make -C lsm load     attach the enforcement programs");
        return Ok(());
    }

    if recorded > 0 {
        println!("Recorded permissions are not automatically enforced: each module");
        println!("needs its cgroup bound to a policy with `thalyx enforce apply`.");
        println!("Until then the registry is bookkeeping, not protection.");
    }

    Ok(())
}

fn require_kernel(kernel: &BpftoolStore) -> Fallible {
    if kernel.is_available() {
        return Ok(());
    }
    Err("the kernel policy map is not present; run `make -C lsm load` first".into())
}

/// Boot-relative nanoseconds, matching `bpf_ktime_get_boot_ns()` in the LSM.
///
/// Read from `/proc/uptime` rather than a wall clock: the kernel compares
/// against its own boot-relative clock, and a wall clock would drift from it
/// across suspend — silently extending or cutting short every JIT grant.
fn now_ns() -> u64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| {
            contents
                .split_whitespace()
                .next()
                .and_then(|seconds| seconds.parse::<f64>().ok())
        })
        .map(|seconds| (seconds * 1_000_000_000.0) as u64)
        .unwrap_or(0)
}
