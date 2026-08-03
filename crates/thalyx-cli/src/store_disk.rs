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

/// Every block device the kernel knows about, by name.
///
/// From `/sys/block`, which is the kernel's own list and not a guess about
/// which names are plausible. `Err` is a failure to read and is kept separate
/// from an empty list, because those two say opposite things about the machine.
fn block_devices() -> std::io::Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir("/sys/block")?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    Ok(names)
}

/// Why the named device is not here, in the terms that decide what to do next.
///
/// "No disk at all" and "disks, but not that one" send you to different halves
/// of the problem — a kernel with no driver for the controller against a disk
/// that was attached under another name or never attached. The first version of
/// this said only "is not a device on this machine", which is true of both and
/// useful for neither. Rule 10 of `Estrategia-de-Pruebas.md`, applied to the one
/// message a person reads while looking at a machine that came up empty.
fn why_absent(device: &Path) -> String {
    let named = device.display();
    match block_devices() {
        Ok(names) if names.is_empty() => format!(
            "{named} is not here, and neither is any other disk. Either nothing\n      \
             was attached, or this kernel has no driver for the controller it\n      \
             was attached to"
        ),
        Ok(names) => format!(
            "{named} is not here. The disks that are: {}",
            names.join(", ")
        ),
        // Not "there are no disks". /sys was mounted a few lines ago and this
        // still failed, which is a stranger fact than a missing store.
        Err(error) => format!("{named} is not here, and /sys/block could not be read: {error}"),
    }
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
            why: why_absent(&device),
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
                println!("      nothing will survive this boot. Making one is");
                println!("      `make -C image store-stage` and then");
                println!("      `sudo make -C image store`. I will not make one myself,");
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

    /// The image builder, read rather than trusted.
    fn image_makefile() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../image/Makefile")
            .canonicalize()
            .expect("the image Makefile is part of the repository");
        std::fs::read_to_string(&path).expect("reading the image Makefile")
    }

    /// One target's recipe: every line after `name:` that is indented.
    fn recipe_of<'a>(makefile: &'a str, target: &str) -> Vec<&'a str> {
        let mut lines = makefile.lines().skip_while(|l| !l.starts_with(target));
        let header = lines.next().unwrap_or_default();
        assert!(
            header.starts_with(target),
            "no `{target}` target in the image Makefile"
        );
        lines
            .take_while(|l| l.starts_with('\t') || l.trim().is_empty())
            .collect()
    }

    #[test]
    fn the_target_that_needs_root_builds_nothing() {
        // `store` used to depend on `store-stage`, so `sudo make store` was one
        // command. It failed at once — sudo resets PATH and rustup lives under
        // the user's home — and the failure was the small half. Had it worked it
        // would have run the whole Rust build as root: every dependency's build
        // script executing with privilege, and root-owned files left in the
        // target directory that the next ordinary build could not replace.
        let makefile = image_makefile();

        let header = makefile
            .lines()
            .find(|l| l.starts_with("store:"))
            .expect("a `store` target");
        assert_eq!(
            header.trim(),
            "store:",
            "the target that runs as root has prerequisites, so root will build them"
        );

        for line in recipe_of(&makefile, "store:") {
            for tool in ["cargo ", "rustup ", "$(MAKE)"] {
                assert!(
                    !line.contains(tool),
                    "`store` runs `{tool}` as root: {}",
                    line.trim()
                );
            }
        }
    }

    #[test]
    fn the_privileged_target_refuses_rather_than_assuming_the_stage_is_there() {
        // And it looks for the stamp, not the directory. An interrupted stage
        // leaves the directory sitting there looking finished, and a disk built
        // from it would be missing whatever had not happened yet.
        let makefile = image_makefile();
        let recipe: String = recipe_of(&makefile, "store:").join("\n");
        assert!(
            recipe.contains("test -f $(STAMP)"),
            "`store` does not check that anything was staged"
        );
        assert!(
            recipe.contains("store-stage"),
            "`store` refuses without naming the command that fixes it"
        );
    }

    #[test]
    fn a_missing_disk_says_whether_there_are_any_disks_at_all() {
        // Two failures that read identically and are fixed differently: nothing
        // was attached, against a disk that came up under another name. The
        // first version said only "is not a device on this machine", which is
        // true of both — and that is the one line somebody reads while looking
        // at a machine that came up with nothing on it.
        //
        // This runs where /sys/block exists and is populated, so it exercises
        // the branch that lists them. The two other branches are unreachable
        // from a test that does not control /sys, and are written to be read.
        let why = why_absent(Path::new("/dev/nothing-is-called-this"));
        assert!(
            why.contains("/dev/nothing-is-called-this"),
            "the message does not name the disk that was asked for: {why}"
        );

        match block_devices() {
            Ok(names) if !names.is_empty() => {
                assert!(
                    why.contains(&names[0]),
                    "there are disks and the message does not list them: {why}"
                );
            }
            // A machine with no block devices at all — a container, usually.
            // The claim then is the opposite one, and it is still a claim.
            _ => assert!(
                why.contains("neither is any other disk") || why.contains("could not be read"),
                "no disks here, and the message did not say so: {why}"
            ),
        }
    }

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
        let text = image_makefile();

        let mut created: Vec<&str> = text
            .lines()
            .filter_map(|line| line.trim().strip_prefix("btrfs subvolume create $(MNT)/"))
            .collect();
        created.sort_unstable();
        assert!(
            !created.is_empty(),
            "no `btrfs subvolume create` in image/Makefile — the store is no longer\n\
             built there, and this test would pass forever without checking anything"
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
        let text = image_makefile();
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
