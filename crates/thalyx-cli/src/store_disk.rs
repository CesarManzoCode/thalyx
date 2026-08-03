//! The disk the machine keeps things on, and how PID 1 finds it.
//!
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md`: the root filesystem is an
//! initramfs that keeps nothing between boots, so **everything that survives a
//! reboot survives because it is on this disk**. Three subvolumes, decreed by
//! `vault/04-Flujo-Canonico/Journal-y-Snapshots.md`.
//!
//! ## Why PID 1 does not create them
//!
//! It mounts them; it never makes them. A machine that fabricated a fresh store
//! whenever it failed to find the old one would come up looking perfect on the
//! day the disk was not attached, and the human would find out what had
//! happened by noticing that everything they had installed was gone. Absent is
//! reported and nothing is written. The store is made once, by
//! `make -C image store`, on a machine that has `mkfs.btrfs` — the image does
//! not, and cannot: it has one program in it.
//!
//! ## Why the device comes from the kernel command line
//!
//! The alternative is probing `/dev/vda`, then `/dev/sda`, then whatever else
//! looks plausible, and mounting the first one that answers. That is a
//! heuristic that succeeds on the wrong disk exactly once, and the failure is
//! that Thalyx wrote its store onto something else's filesystem. `thalyx.store=`
//! says which one; nothing is guessed, and when the parameter is absent that is
//! reported as its own fact rather than as a missing disk.
//!
//! ## Why `system` holds the whole store and not just part of it
//!
//! `vault/04-Flujo-Canonico/Fase-Commit-Atomico.md` puts the staging area in the
//! same subvolume as its destination, because `rename(2)` returns `EXDEV`
//! across Btrfs subvolumes and not only across devices. So `/opt/thalyx` — with
//! `.staging/`, `modules/`, `state/` and the journal inside it — is one
//! subvolume. Mounting the `modules` subvolume *at* `/opt/thalyx/modules` would
//! read as the tidier arrangement and would break every atomic commit on the
//! machine, which is the bug the vault already records once.

use std::path::{Path, PathBuf};
use thalyx_syscall::mount_flags::{HARDENED, NODEV, NOSUID};

/// The kernel command-line parameter that names the disk.
const PARAMETER: &str = "thalyx.store=";

/// One subvolume, where it goes, and why it is separate from the others.
struct Subvolume {
    name: &'static str,
    target: &'static str,
    flags: u64,
    because: &'static str,
}

/// The three of `Journal-y-Snapshots.md`, in the order they must be mounted:
/// `/opt/thalyx` first, because the second one lands inside it.
const SUBVOLUMES: &[Subvolume] = &[
    Subvolume {
        name: "system",
        target: "/opt/thalyx",
        // Not noexec: a module's program lives under modules/<id>/<version>/
        // and the sandbox has to be able to execute it. Everything else on the
        // disk is mounted so that a file appearing there cannot become a way to
        // run something.
        flags: NOSUID | NODEV,
        because: "the store: staging, installed modules, state and the journal",
    },
    Subvolume {
        name: "modules",
        target: "/opt/thalyx/data",
        flags: HARDENED,
        because: "what modules write, so a snapshot can take back what one did",
    },
    Subvolume {
        name: "user",
        target: "/home",
        flags: HARDENED,
        because: "the human's own files, which no rollback of ours may touch",
    },
];

/// What came of trying to bring the store up.
pub enum Store {
    /// Every subvolume is mounted. The machine keeps what it is told.
    Mounted { device: PathBuf },
    /// No `thalyx.store=` on the command line, so no disk was even named.
    Unnamed,
    /// The disk was named and is not there.
    Absent { why: String },
    /// The disk is there and something about it did not work.
    ///
    /// Kept apart from [`Store::Absent`] because they call for opposite
    /// actions: one means attach the disk, the other means the disk is attached
    /// and was never made into a store. Rule 10 of `Estrategia-de-Pruebas.md` —
    /// a failure to read is not a failure to exist.
    Broken {
        device: PathBuf,
        failures: Vec<(&'static str, String)>,
    },
}

/// The disk named on the kernel command line, if one was.
///
/// Reads `/proc/cmdline`, so it must be called after `/proc` is mounted.
fn named_device() -> Option<PathBuf> {
    let cmdline = std::fs::read_to_string("/proc/cmdline").ok()?;
    cmdline
        .split_ascii_whitespace()
        .find_map(|word| word.strip_prefix(PARAMETER))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Mount the store, or say precisely which way it was not there.
pub fn mount() -> Store {
    let Some(device) = named_device() else {
        return Store::Unnamed;
    };

    // Checked before mounting so that "no disk" and "a disk that will not
    // mount" are told apart by something other than the errno of the mount,
    // which is ENODEV for both a missing device node and an unknown filesystem.
    if !device.exists() {
        return Store::Absent {
            why: format!("{} is not a device on this machine", device.display()),
        };
    }

    let mut failures = Vec::new();
    for subvolume in SUBVOLUMES {
        let target = Path::new(subvolume.target);
        if let Err(error) = std::fs::create_dir_all(target) {
            failures.push((subvolume.name, error.to_string()));
            continue;
        }
        let data = format!("subvol={}", subvolume.name);
        match thalyx_syscall::mount(
            Some(&device),
            target,
            Some("btrfs"),
            subvolume.flags,
            Some(&data),
        ) {
            Ok(()) => {}
            Err(error) => failures.push((subvolume.name, error.to_string())),
        }
    }

    if failures.is_empty() {
        Store::Mounted { device }
    } else {
        Store::Broken { device, failures }
    }
}

impl Store {
    /// Print what happened, in the shape the rest of the boot uses.
    ///
    /// One line per fact, `ok` or `no`, and never a summary that averages the
    /// two: a store where two subvolumes mounted and one did not is not
    /// two-thirds of a store, it is a machine that will fail later at the point
    /// that touches the third.
    pub fn report(&self) {
        match self {
            Store::Mounted { device } => {
                println!("  ok  store        {} — three subvolumes", device.display());
            }
            Store::Unnamed => {
                println!("  no  store        no {PARAMETER} on the kernel command line");
                println!("      nothing will survive this boot. The disk is not missing;");
                println!("      nobody told me which one it is.");
            }
            Store::Absent { why } => {
                println!("  no  store        {why}");
                println!("      nothing will survive this boot. Making a store is");
                println!("      `sudo make -C image store`; I will not make one myself,");
                println!("      because a machine that did could never tell you it");
                println!("      had lost the old one.");
            }
            Store::Broken { device, failures } => {
                println!(
                    "  no  store        {} is there and did not mount:",
                    device.display()
                );
                for (name, error) in failures {
                    println!("      subvol={name}: {error}");
                }
                println!("      that is a different problem from a missing disk: this");
                println!("      one is attached. It was probably never made into a");
                println!("      store, or was made by a different version.");
            }
        }
    }
}

/// What the store layout is, for anyone who has to build one.
///
/// The builder in `image/Makefile` creates exactly these names, and the test
/// below reads that file to check it rather than trusting it.
///
/// Test-only: nothing in a running system needs to ask, because the thing that
/// mounts them is right here.
#[cfg(test)]
pub fn subvolume_names() -> impl Iterator<Item = &'static str> {
    SUBVOLUMES.iter().map(|s| s.name)
}

/// Report what PID 1 would mount, without a disk and without being PID 1.
pub fn describe() {
    println!("The store disk carries three subvolumes:");
    println!();
    for subvolume in SUBVOLUMES {
        println!("  {:<10} -> {}", subvolume.name, subvolume.target);
        println!("  {:<10}    {}", "", subvolume.because);
    }
    println!();
    println!("named by `{PARAMETER}<device>` on the kernel command line, and");
    println!("mounted — never created — by PID 1.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_root_is_one_subvolume_so_that_rename_stays_atomic() {
        // The mistake this guards is specific and has already been made once in
        // this project: putting `modules` at /opt/thalyx/modules looks tidier
        // and makes every atomic commit fail with EXDEV, because the staging
        // area would then be in a different subvolume from its destination.
        let modules = SUBVOLUMES
            .iter()
            .find(|s| s.name == "modules")
            .expect("the modules subvolume is decreed");
        assert_ne!(
            modules.target, "/opt/thalyx/modules",
            "staging and destination would be in different subvolumes"
        );
    }

    #[test]
    fn the_store_root_is_mounted_before_what_goes_inside_it() {
        // /opt/thalyx/data is under /opt/thalyx. Mounting it first would put it
        // on the initramfs, and the store mount would then hide it — leaving a
        // machine where module data silently went nowhere.
        let order: Vec<&str> = SUBVOLUMES.iter().map(|s| s.target).collect();
        let root = order.iter().position(|t| *t == "/opt/thalyx").unwrap();
        let data = order.iter().position(|t| *t == "/opt/thalyx/data").unwrap();
        assert!(root < data);
    }

    #[test]
    fn only_the_store_root_may_hold_something_executable() {
        // A module's program has to run, so /opt/thalyx cannot be noexec.
        // Everything else on the disk must be, or the noexec is decorative.
        for subvolume in SUBVOLUMES {
            if subvolume.target == "/opt/thalyx" {
                continue;
            }
            assert_eq!(
                subvolume.flags & HARDENED,
                HARDENED,
                "{} is missing nosuid/noexec/nodev",
                subvolume.name
            );
        }
    }

    #[test]
    fn nothing_on_the_disk_may_carry_a_setuid_bit_or_a_device_node() {
        // Including the store root, which is the one that gives up noexec. A
        // setuid binary inside a module bundle would otherwise be a way out of
        // every uid boundary the sandbox draws.
        for subvolume in SUBVOLUMES {
            assert_eq!(
                subvolume.flags & (NOSUID | NODEV),
                NOSUID | NODEV,
                "{} is missing nosuid/nodev",
                subvolume.name
            );
        }
    }

    #[test]
    fn the_builder_makes_exactly_the_subvolumes_this_mounts() {
        // Two lists that must agree, kept in two languages, disagree eventually.
        // The failure would be a machine that boots, reports a store, and is
        // missing the one subvolume nothing touched until later — so the
        // Makefile is read rather than trusted.
        //
        // Not a check that the file parses as make: only that every name PID 1
        // will mount is a subvolume somebody created, and that nothing else was
        // created that nothing mounts.
        let makefile = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../image/Makefile")
            .canonicalize()
            .expect("the image Makefile is part of the repository");
        let text = std::fs::read_to_string(&makefile).expect("reading the image Makefile");

        let mut created: Vec<&str> = text
            .lines()
            .filter_map(|line| line.trim().strip_prefix("btrfs subvolume create $(MNT)/"))
            .collect();
        created.sort_unstable();
        assert!(
            !created.is_empty(),
            "no `btrfs subvolume create` in {} — the store is no longer built there,\n\
             and this test would pass forever without checking anything",
            makefile.display()
        );

        let mut mounted: Vec<&str> = subvolume_names().collect();
        mounted.sort_unstable();

        assert_eq!(
            created, mounted,
            "the store builder and PID 1 disagree about what is on the disk"
        );
    }

    #[test]
    fn the_disk_the_builder_names_is_the_disk_the_machine_is_told_about() {
        // `thalyx.store=` on the QEMU command line is the only thing that makes
        // the store findable. A Makefile that attached the disk and forgot to
        // name it would produce a machine reporting "no store" with the store
        // plugged in — a failure whose message points away from its cause.
        let makefile = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../image/Makefile")
            .canonicalize()
            .expect("the image Makefile is part of the repository");
        let text = std::fs::read_to_string(&makefile).expect("reading the image Makefile");
        assert!(
            text.contains(&format!("{PARAMETER}$(STOREDEV)")),
            "the boot line does not carry {PARAMETER}"
        );
    }

    #[test]
    fn every_subvolume_says_why_it_is_separate() {
        for subvolume in SUBVOLUMES {
            assert!(
                !subvolume.because.is_empty(),
                "{} has no reason recorded",
                subvolume.name
            );
        }
    }
}
