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

#[derive(Subcommand)]
pub enum EnforceCommand {
    /// Report whether the kernel is enforcing, and for whom
    Status,
    /// Say nothing; exit 0 only if every hook is live
    ///
    /// The check `lsm/demo-enforcement.sh` and `make status` need. They used to
    /// test whether a directory existed in bpffs, which answered for the loader
    /// that happens to create that directory — so enforcement Thalyx attached
    /// itself read as not attached, and the demo refused to run against it.
    Attached {
        /// Exit 0 if *any* hook is live, not only if all of them are.
        ///
        /// Two different questions. A demo about to test a denial needs every
        /// hook; a loader about to attach needs to know whether it would be
        /// stacking on top of something, and one live hook is enough for that.
        #[arg(long)]
        any: bool,
    },
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
    /// Load thalyx-lsm into the kernel and attach it, with no bpftool
    ///
    /// This is what PID 1 does at boot. It is a command as well so that the
    /// loader can be exercised on a machine that is not the image — which is
    /// every machine that can currently check it.
    Attach,
    /// Detach thalyx-lsm by removing what holds it
    Detach,
}

pub fn run(store_root: &std::path::Path, command: EnforceCommand) -> Fallible {
    // Before the store is opened, because this asks the kernel a question that
    // has nothing to do with what is installed — and a machine with no store
    // still has hooks, or does not, and should be able to say which.
    if let EnforceCommand::Attached { any } = command {
        return attached_quietly(any);
    }

    let store = Store::open(store_root)?;
    let kernel = BpftoolStore::default_map();

    match command {
        EnforceCommand::Status => status(&store, &kernel),
        EnforceCommand::Attached { .. } => unreachable!("returned above"),

        EnforceCommand::Attach => attach(),
        EnforceCommand::Detach => detach(),

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
            let policy = thalyx_permd::apply(
                &kernel,
                id,
                &permissions,
                thalyx_permd::boot_ns(),
                thalyx_permd::DEFAULT_JIT_LIFETIME_NS,
            )?;

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

/// What of thalyx-lsm is in the kernel's decision path right now.
///
/// Two separate readings, deliberately, because they fail apart: the maps can
/// be pinned with nothing attached (a loader ran and stopped), and hooks can be
/// live with the policy map missing (enforcement that no permission can be
/// written into). One line saying "loaded" would hide both.
fn attachment() -> Result<thalyx_bpf::Attachment, String> {
    let Some(object) = embedded::OBJECT else {
        return Err(format!(
            "no BPF object was built into this binary; `make -C lsm` produces {}",
            embedded::ORIGIN
        ));
    };
    thalyx_bpf::attachment(object).map_err(|error| error.to_string())
}

/// Exit 0 when every hook is live, and print nothing.
///
/// For the scripts that used to test whether a directory existed in bpffs —
/// which answered for the loader that made that directory and no other, and so
/// reported enforcement Thalyx had attached as absent.
fn attached_quietly(any: bool) -> Fallible {
    match attachment() {
        Ok(state) if state.is_complete() || (any && !state.is_absent()) => Ok(()),
        Ok(state) => Err(state.describe().into()),
        Err(error) => Err(error.into()),
    }
}

fn status(store: &Store, kernel: &BpftoolStore) -> Fallible {
    let available = kernel.is_available();

    match attachment() {
        Ok(state) => println!("enforcement:       {}", state.describe()),
        Err(error) => println!("enforcement:       COULD NOT BE READ — {error}"),
    }

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
        println!("`thalyx module run` establishes a module's confinement itself:");
        println!("it writes the policy, then launches the module inside the cgroup");
        println!("that policy is keyed on, and withdraws both when it exits.");
        println!();
        println!("`apply` and `revoke` below are for binding a cgroup by hand —");
        println!("for inspection and for processes Thalyx did not start.");
    }

    Ok(())
}

fn require_kernel(kernel: &BpftoolStore) -> Fallible {
    if kernel.is_available() {
        return Ok(());
    }
    Err("the kernel policy map is not present; run `make -C lsm load` first".into())
}

/// Where PID 1 pins what it loads. The same path, because a loader that pinned
/// somewhere else would leave `thalyx-permd` looking at nothing.
const PIN_ROOT: &str = "/sys/fs/bpf/thalyx";

/// The object, embedded by `build.rs`. See `init.rs` for why it is not a file.
mod embedded {
    include!(concat!(env!("OUT_DIR"), "/lsm_object.rs"));
}

/// Load and attach, exactly as the boot does it.
fn attach() -> Fallible {
    let Some(object) = embedded::OBJECT else {
        return Err(format!(
            "no BPF object was built into this binary.\n  \
             `make -C lsm` produces {}, and this was compiled before it existed.",
            embedded::ORIGIN
        )
        .into());
    };

    // Refused rather than layered. Two sets of links on the same hooks both
    // run, both deny, and detaching one leaves the other — so "it is still
    // denying after I unloaded it" becomes the puzzle.
    if std::path::Path::new(PIN_ROOT).join("links").exists() {
        return Err(format!(
            "something is already attached at {PIN_ROOT}.\n  \
             `thalyx enforce detach` first; attaching twice leaves two sets of\n  \
             live hooks and removing one of them changes nothing you can see."
        )
        .into());
    }

    let kernel = thalyx_bpf::kernel_btf()?;
    let loaded = thalyx_bpf::load(object, &kernel)?;

    println!();
    for (name, _) in &loaded.maps {
        println!("  map     {name}");
    }
    for (name, _) in &loaded.links {
        println!("  live    {name}");
    }

    loaded.pin(std::path::Path::new(PIN_ROOT))?;
    // Only after pinning: the descriptors are what keep it alive until the
    // pins do.
    drop(loaded);

    println!();
    println!("  attached, and pinned under {PIN_ROOT}.");
    println!("  Nothing is denied yet: the policy map is empty and no module");
    println!("  has been placed in a cgroup it knows about.");
    println!();
    Ok(())
}

/// Remove the pins, which is what lets the kernel free the links.
///
/// Links go first. A map removed while a link still references it is freed only
/// when the link is, so the reverse order leaves enforcement live with its
/// policy unreachable — denying on a map nothing can write to.
fn detach() -> Fallible {
    let root = std::path::Path::new(PIN_ROOT);
    if !root.exists() {
        println!("  nothing is attached at {PIN_ROOT}.");
        return Ok(());
    }

    let mut removed = 0;
    for directory in ["links", "maps"] {
        let path = root.join(directory);
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            std::fs::remove_file(entry.path())?;
            removed += 1;
        }
        let _ = std::fs::remove_dir(&path);
    }
    let _ = std::fs::remove_dir(root);

    println!("  detached: {removed} pin(s) removed from {PIN_ROOT}");
    Ok(())
}
