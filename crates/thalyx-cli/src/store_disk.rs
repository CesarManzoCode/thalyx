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
//! reported and nothing is written.
//!
//! The store is made once, by a human, with `thalyx disk format` — or by
//! `make -C image store` for the development disk, which still uses `mkfs.btrfs`
//! on purpose so that the regression net for the boot stages is not the same code
//! as the thing being tested. Neither path is reachable from PID 1.
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

/// The alias every command module in this crate uses.
type Fallible = Result<(), Box<dyn std::error::Error>>;

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

/// Things a human does to the disk itself, none of which PID 1 can reach.
#[derive(clap::Subcommand)]
pub enum DiskCommand {
    /// What PID 1 would mount off a store disk, and where
    Layout,

    /// Ask a device what filesystem it holds and what it is called
    Identify {
        /// The block device or image file to read
        device: PathBuf,
    },

    /// Write an empty Thalyx store onto a device, destroying what is there
    ///
    /// This is the human act the decree requires. PID 1 mounts a store and is
    /// forbidden from making one, because a machine that fabricated a store when
    /// it could not find the old one would boot looking perfect on the day the
    /// disk was not attached.
    Format {
        /// The block device or image file to write
        device: PathBuf,
        /// The filesystem label. The default is what an installed machine looks
        /// for, so changing it produces a store Thalyx will not find by name.
        #[arg(long, default_value = thalyx_btrfs::LABEL)]
        label: String,
        /// Skip the confirmation. For scripts and tests.
        #[arg(long)]
        yes: bool,
        /// Write the filesystem and stop, leaving it without subvolumes.
        ///
        /// What comes out is not a store: PID 1 mounts `subvol=system` and there
        /// will not be one. For writing an image file on a machine that cannot
        /// mount Btrfs, which is the only case where the second half is
        /// impossible rather than merely unwanted.
        #[arg(long)]
        no_subvolumes: bool,
        /// Where to put the mount points the subvolume step needs.
        #[arg(long, default_value = WORKSPACE)]
        workspace: PathBuf,
    },

    /// Create the three subvolumes on a store that has none
    ///
    /// The other half of `format`, separable because it needs things `format`
    /// does not: root, a kernel with Btrfs, and a block device. Safe to run on a
    /// store that already has them — it says so rather than failing, because
    /// otherwise the only way to fix two-of-three would be to reformat.
    Subvolumes {
        /// The block device to work on. Not an image file: see the error it gives.
        device: PathBuf,
        /// Where to put the mount points it needs.
        #[arg(long, default_value = WORKSPACE)]
        workspace: PathBuf,
    },
}

/// Where the subvolume step puts its mount points.
///
/// `/run` and not `/tmp`, because inside the image there is no `/tmp` — the
/// archive carries thirteen directories and that is not one of them. A default of
/// `std::env::temp_dir()` would work on every development machine and fail on the
/// only machine that matters.
const WORKSPACE: &str = "/run/thalyx/store-setup";

/// Run one of them.
pub fn run(command: DiskCommand) -> Fallible {
    match command {
        DiskCommand::Layout => {
            describe();
            println!();
            describe_plan();
            Ok(())
        }
        DiskCommand::Identify { device } => {
            report_identity(&device, &thalyx_btrfs::identify(&device)?);
            Ok(())
        }
        DiskCommand::Format {
            device,
            label,
            yes,
            no_subvolumes,
            workspace,
        } => format(&device, &label, yes, no_subvolumes, &workspace),
        DiskCommand::Subvolumes { device, workspace } => subvolumes(&device, &workspace),
    }
}

/// Where a store Thalyx writes puts its chunks, on any device.
///
/// Printed because it is the only way to ask. `dev/verify.sh` damages both copies
/// of the metadata chunk as its control — a mount that succeeds on anything would
/// make the mount above it establish nothing — and it takes the offsets from here
/// rather than repeating them. Two copies of a layout in two languages disagree
/// eventually, and this disagreement would be a control that damages an
/// unallocated part of the device, finds the filesystem mounts anyway, and reports
/// that the kernel accepts anything.
fn describe_plan() {
    use thalyx_btrfs::layout::{Geometry, Plan};

    let plan = Plan::new(Geometry::default());
    println!("A store Thalyx writes has this shape on the device:");
    println!();
    println!(
        "  {:<10} {:>12} {:>12}  copies at",
        "chunk", "logical", "length"
    );
    for chunk in &plan.chunks {
        use thalyx_btrfs::disk::block_group;
        let kind = if chunk.flags & block_group::DATA != 0 {
            "data"
        } else if chunk.flags & block_group::SYSTEM != 0 {
            "system"
        } else {
            "metadata"
        };
        let copies: Vec<String> = chunk
            .stripes
            .iter()
            .map(|stripe| stripe.0.to_string())
            .collect();
        println!(
            "  {:<10} {:>12} {:>12}  {}",
            kind,
            chunk.logical.0,
            chunk.length,
            copies.join(", ")
        );
    }
    println!();
    println!(
        "  {} bytes of the device, all of it above the reserved first megabyte",
        plan.device_used
    );
    println!("  and none of it covering a superblock.");
}

/// Say what a device turned out to be, in the three terms that decide what to do.
fn report_identity(device: &Path, identity: &thalyx_btrfs::Identity) {
    let named = device.display();
    match identity {
        thalyx_btrfs::Identity::Btrfs { label, fsid } => {
            let called = if label.is_empty() {
                "btrfs, with no label".to_string()
            } else {
                format!("btrfs, labelled `{label}`")
            };
            println!("  {named}  {called}");
            println!(
                "  {:<width$}  fsid {}",
                "",
                hex(fsid),
                width = named.to_string().len()
            );
            if label == thalyx_btrfs::LABEL {
                println!("  this is a Thalyx store");
            }
        }
        thalyx_btrfs::Identity::NotBtrfs => {
            println!("  {named}  not btrfs");
        }
        thalyx_btrfs::Identity::Corrupt { expected, found } => {
            println!("  {named}  btrfs, and its superblock does not check out");
            println!("      expected {} and found {}", hex(expected), hex(found));
            println!("      that is different from `not btrfs`: this is a Thalyx-shaped");
            println!("      filesystem that has been damaged, not somebody else's disk.");
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Write a store, having said what is about to be destroyed.
///
/// The confirmation asks for the device's own path rather than for `y`. This is
/// the most destructive thing Thalyx can be asked to do and the argument is one
/// word long: `/dev/sda` and `/dev/sdb` differ by a keystroke, and a `y` confirms
/// a sentence the human has already stopped reading. Typing the path back is the
/// one answer that cannot be given by mistake to the wrong disk.
fn format(
    device: &Path,
    label: &str,
    yes: bool,
    no_subvolumes: bool,
    workspace: &Path,
) -> Fallible {
    use std::io::{IsTerminal, Write};

    println!("About to write a Thalyx store onto {}.", device.display());
    println!();
    // What is there now, said before the question rather than after it. A
    // confirmation that does not say what is being destroyed is a confirmation
    // about nothing.
    match thalyx_btrfs::identify(device) {
        Ok(identity) => report_identity(device, &identity),
        Err(error) => {
            println!("  could not read what is there: {error}");
            println!("  that is not permission to proceed — it is one more thing");
            println!("  unknown about a device about to be overwritten.");
        }
    }
    println!();
    println!("  Everything on it will be gone. This cannot be undone.");
    println!();

    if yes {
        println!("  confirmed with --yes");
    } else if !std::io::stdin().is_terminal() {
        // Silence is not consent, the same rule the capability prompt keeps.
        eprintln!("  no terminal available to confirm; refusing");
        return Err("formatting was not confirmed".into());
    } else {
        print!("  Type the device's path to confirm: ");
        let _ = std::io::stdout().flush();
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if answer.trim() != device.display().to_string() {
            eprintln!("  that is not {}; refusing", device.display());
            return Err("formatting was not confirmed".into());
        }
    }

    let written = thalyx_btrfs::write(
        device,
        label,
        &thalyx_btrfs::Uuids::random(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or(0),
    )?;

    println!();
    println!(
        "  ok  store        {} — {} bytes, labelled `{}`",
        device.display(),
        written.total_bytes,
        written.label
    );
    println!("      fsid {}", hex(&written.fsid));
    println!(
        "      {} superblock(s), {} bytes of metadata",
        written.superblocks, written.metadata_bytes
    );
    println!();

    if no_subvolumes {
        // Said plainly, because what is on the disk now is not a store and a
        // message that stopped at "ok" would read as though it were.
        println!("  --no-subvolumes: it has none, so PID 1 cannot mount it. `system`,");
        println!("  `modules` and `user` are what it looks for, and until they exist");
        println!("  this is a filesystem rather than a store.");
        println!();
        // Named as a block device rather than as this path, because the flag's
        // reason for existing is that this path is often a file — and `subvolumes`
        // on a file refuses. Printing the command with the file's name here would
        // hand the human something that cannot work.
        println!("  Finish it with `thalyx disk subvolumes` on the block device.");
        return Ok(());
    }

    // The filesystem is already on the disk at this point, so a failure here is
    // not "nothing happened" — it is a half-made store, and the message has to
    // say which half and how to finish it. Returned as an error all the same: an
    // installer calling this needs the non-zero exit, not the paragraph.
    subvolumes(device, workspace).inspect_err(|_| {
        println!();
        println!("  The filesystem is written and it has no subvolumes, so it is not a");
        println!("  store yet. Nothing above needs redoing: `thalyx disk subvolumes`");
        println!("  finishes it once whatever the line below asks for is in place.");
    })
}

/// Create the three, and report whether PID 1 could mount each one.
///
/// The report is per name and it is the mount that is reported, not the creation.
/// A directory called `system` that is not a subvolume gets created by nobody here
/// and would come back as "already there" — with a mount that failed. Printing the
/// creation alone would call that a success.
fn subvolumes(device: &Path, workspace: &Path) -> Fallible {
    use thalyx_btrfs::subvolume::{DECREED, Made};

    let outcome = thalyx_btrfs::subvolume::create(device, workspace, &DECREED)?;

    for (name, made) in &outcome.subvolumes {
        let what = match made {
            Made::Created => "created",
            // Not an error and not a success either. On a device being formatted
            // this cannot happen; on a repair it is the normal case.
            Made::AlreadyThere => "already there",
        };
        println!("  ok  subvolume    {name} — {what}");
    }

    println!();
    for (name, why) in &outcome.mounted {
        match why {
            None => println!("  ok  mountable    subvol={name}"),
            Some(reason) => {
                println!("  NO  mountable    subvol={name}");
                for line in reason.lines() {
                    println!("      {line}");
                }
            }
        }
    }

    println!();
    if outcome.is_a_store() {
        println!("  This is a store. PID 1 can mount it.");
        Ok(())
    } else {
        // A non-zero exit, because a store that PID 1 cannot mount is the exact
        // failure this whole command exists to prevent, and an installer calling
        // it needs the failure and not the printout.
        Err("the subvolumes are not all mountable, so this is not a store yet".into())
    }
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

    /// A README file from the repository root.
    fn readme(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(name)
            .canonicalize()
            .unwrap_or_else(|_| panic!("{name} is part of the repository"));
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("reading {name}"))
    }

    /// Every apt package `doctor` can name, taken from the Makefile itself.
    fn packages_doctor_can_name(makefile: &str) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();

        for line in makefile.lines() {
            // `$(call need,<command>,<package>,<why>)`
            if let Some(rest) = line.trim().strip_prefix("$(call need,")
                && let Some(package) = rest.split(',').nth(1)
            {
                found.push(package.trim().to_string());
            }
            // `echo "<package>" >> $(BUILD)/doctor.packages`
            if line.contains("doctor.packages")
                && let Some(start) = line.find('"')
                && let Some(end) = line[start + 1..].find('"')
            {
                found.push(line[start + 1..start + 1 + end].to_string());
            }
        }

        // `rust` does not come from apt and `kernel-pin` is not a package at
        // all; both are explained separately in the READMEs and deliberately
        // kept out of the install line.
        //
        // Anything holding a `$` is the `define need` macro's own body — the
        // literal `$(2)` — rather than a package. Caught by this test failing
        // on its first run, which is the parser being checked by the thing it
        // feeds rather than by inspection.
        found.retain(|p| p != "rust" && p != "kernel-pin" && !p.is_empty() && !p.contains('$'));
        found.sort();
        found.dedup();
        found
    }

    #[test]
    fn both_readmes_name_every_package_the_doctor_asks_for() {
        // The exit criterion is a person outside the project following **only**
        // the README. If `doctor` grows a prerequisite and the README does not,
        // that person installs an incomplete list, re-runs, and is sent round
        // again — which is the one-at-a-time misery `doctor` exists to end,
        // moved into the document instead of the build.
        //
        // Read from the Makefile rather than hardcoded here, so this cannot
        // drift the same way. `libbpf-dev` is in the list because it was
        // missing from both for a while: `clang` is present and the BPF headers
        // are a separate package, so the build failed after the kernel had
        // already been compiled.
        let makefile = image_makefile();
        let wanted = packages_doctor_can_name(&makefile);

        assert!(
            wanted.len() > 5,
            "the package list came back suspiciously short ({wanted:?}); the \
             parser above has probably stopped matching the Makefile"
        );

        for name in ["README.md", "README.es.md"] {
            let text = readme(name);
            for package in &wanted {
                assert!(
                    text.contains(package.as_str()),
                    "{name} never mentions `{package}`, which `doctor` will ask \
                     for — so somebody following only that file installs an \
                     incomplete list"
                );
            }
        }
    }

    #[test]
    fn the_english_readme_points_at_the_spanish_one_and_back() {
        // The person these are written for reads Spanish. A translation nothing
        // links to is a translation nobody finds.
        assert!(
            readme("README.md").contains("README.es.md"),
            "the English README does not link to the Spanish one"
        );
        assert!(
            readme("README.es.md").contains("README.md"),
            "the Spanish README does not link back"
        );
    }

    #[test]
    fn the_kernel_tarball_is_never_built_without_a_digest_to_check_it_against() {
        // Thalyx compiles its own kernel, so the tarball is not a dependency —
        // it becomes the most privileged half of the machine. It used to be
        // fetched over HTTPS and built, and TLS answers a different question:
        // who served the bytes, not what the bytes were.
        //
        // Read from the Makefile rather than exercised, because exercising it
        // means downloading a kernel. What has to stay true is that the recipe
        // refuses rather than proceeds, and "refuses" is one deleted line away
        // from "warns".
        let text = image_makefile();

        assert!(
            text.contains("KSHA256"),
            "the kernel tarball has no pinned digest at all"
        );

        let recipe = recipe_of(&text, "$(KTARBALL):").join("\n");
        assert!(
            recipe.contains("sha256sum") && recipe.contains("exit 1"),
            "the tarball recipe does not refuse a digest mismatch:\n{recipe}"
        );
        assert!(
            recipe.contains(".part"),
            "the tarball is downloaded straight to its final name, so a partial \
             download would be treated as finished by every later run"
        );
    }

    #[test]
    fn the_uefi_boot_is_not_handed_the_kernel_it_is_supposed_to_go_and_find() {
        // The whole claim of `run-uefi` is that a firmware, given a disk and
        // nothing else, finds something on it and starts it. `-kernel` is QEMU
        // being the bootloader — it loads the image into memory itself and the
        // medium is never read. A `run-uefi` carrying `-kernel` boots
        // beautifully and establishes nothing at all, and it looks identical to
        // one that works.
        //
        // That is the shape this project keeps meeting: a check that passes
        // because the thing it was measuring never had to happen. It is worth a
        // test because the repair somebody reaches for when the firmware does
        // not boot is precisely to add `-kernel` back.
        let text = image_makefile();
        let recipe = recipe_of(&text, "run-uefi:").join("\n");

        for handed in ["-kernel", "-initrd", "-append"] {
            assert!(
                !recipe.contains(handed),
                "`run-uefi` passes {handed}, so QEMU is the bootloader again and \
                 the medium is never read:\n{recipe}"
            );
        }
        assert!(
            recipe.contains("pflash") && recipe.contains("OVMF_CODE"),
            "`run-uefi` boots without UEFI firmware, so there is nothing that \
             could look for a boot medium:\n{recipe}"
        );
    }

    #[test]
    fn the_boot_medium_carries_one_file_and_the_firmware_can_find_it() {
        // `Filosofia-Fundacional.md` allows one program, and the decree is
        // countable rather than quotable. Extending it to the boot medium is
        // the whole reason there is no bootloader here: GRUB would be a second
        // program, in the same shape as the `bpftool` hole.
        //
        // \EFI\BOOT\BOOTX64.EFI is not a name somebody picked. It is the
        // removable-media fallback a UEFI firmware looks for when nothing is
        // configured, which is what a PC with no operating system is.
        let text = image_makefile();

        assert!(
            text.contains("$(ESP)/EFI/BOOT/BOOTX64.EFI"),
            "nothing is placed at the path a firmware with no configuration \
             looks for, so the medium would boot on the machine that was told \
             about it and on no other"
        );

        let recipe = recipe_of(&text, "$(ESP)/EFI/BOOT/BOOTX64.EFI:").join("\n");
        assert!(
            recipe.contains("$(BZIMAGE)"),
            "the file the firmware starts is not the kernel, which means \
             something else is doing the booting:\n{recipe}"
        );
    }

    #[test]
    fn the_kernel_refuses_to_build_if_the_image_did_not_go_inside_it() {
        // CONFIG_INITRAMFS_SOURCE cannot live in thalyx.config, because its
        // value is an absolute path belonging to whoever is building, and
        // `config-check` compares lines verbatim. So it is appended at
        // configure time — which puts it in exactly the category this file has
        // been burned by nine times: an option asked for, dropped without a
        // word by olddefconfig, and noticed much later.
        //
        // Later here means after the medium has been written: the kernel builds,
        // the firmware starts it, and it says `No working init found` with no
        // root filesystem and nothing to load one from.
        let text = image_makefile();
        let recipe = recipe_of(&text, "$(BZIMAGE):").join("\n");

        assert!(
            recipe.contains("CONFIG_INITRAMFS_SOURCE"),
            "the image is not built into the kernel, so the boot medium would \
             need a second file on it:\n{recipe}"
        );
        assert!(
            recipe.contains("initramfs-check"),
            "nothing verifies that CONFIG_INITRAMFS_SOURCE survived \
             olddefconfig:\n{recipe}"
        );

        let check = recipe_of(&text, "initramfs-check:").join("\n");
        assert!(
            check.contains("exit 1"),
            "initramfs-check warns instead of refusing, and a warning during a \
             kernel build is a warning nobody reads:\n{check}"
        );
    }

    #[test]
    fn the_pinning_procedure_names_the_key_that_actually_signed_the_list() {
        // Printed instructions are code with an output, and this output was
        // wrong for two days: `pin-kernel` said to fetch the release
        // maintainers' keys and verify `sha256sums.asc` with them, and that
        // file is signed by kernel.org's automated checksum key. gpg answered
        // `No public key` — three lines under a sentence saying that anything
        // short of `Good signature` means stop.
        //
        // What makes it worth a test rather than a fix is where it was found:
        // on the machine, by the one person who can run it, in the middle of
        // the step that everything else waits on. It could not be found here,
        // because this container's network policy cannot reach kernel.org.
        //
        // So the test asserts the one thing that is checkable without the
        // network — that the procedure names the key a real run came back
        // with, in both of the two places it is written down. Two copies of an
        // instruction is how the wrong one survives: the comment beside
        // KSHA256 is what a person editing the value reads, and the target is
        // what a person following the `doctor` reads.
        let text = image_makefile();
        let printed = recipe_of(&text, "pin-kernel:").join("\n");

        assert!(
            printed.contains("autosigner@kernel.org"),
            "`pin-kernel` does not name the key that signs sha256sums.asc, so \
             following it ends at `No public key`:\n{printed}"
        );
        assert!(
            !printed.contains("torvalds@kernel.org") && !printed.contains("gregkh@kernel.org"),
            "`pin-kernel` sends the reader after a release maintainer's key for \
             a file no maintainer signed:\n{printed}"
        );

        // The fingerprint, in both copies. `--locate-keys` asks the network
        // which key owns an address; with no fingerprint to compare against,
        // `Good signature` establishes that whoever answered agrees with
        // itself, which is not the question the pin exists to answer.
        let fingerprint = "B886 8C80 BA62 A1FF FAF5  FDA9 632D 3A06 589D A6B1";
        assert!(
            printed.contains(fingerprint),
            "`pin-kernel` asks for a good signature without saying whose, so \
             any key the lookup returns satisfies it:\n{printed}"
        );

        let beside_the_digest = text
            .split("KSHA256")
            .next()
            .expect("the Makefile has a KSHA256 line");
        assert!(
            beside_the_digest.contains("autosigner@kernel.org")
                && beside_the_digest.contains(fingerprint),
            "the comment above KSHA256 does not record what established the \
             digest, so the number cannot be re-checked by whoever inherits it"
        );
    }

    #[test]
    fn an_unpinned_kernel_is_reported_by_doctor_and_not_at_the_download() {
        // `doctor` promises to say everything that is missing *at once*. A
        // prerequisite it does not know about recreates precisely the failure
        // it exists to prevent: a person told "everything is here", who then
        // loses the fetch and the configure before hitting a wall.
        //
        // And it must not end up in the apt line. Telling somebody to
        // `apt install kernel-pin` is worse than saying nothing.
        let text = image_makefile();
        let recipe = recipe_of(&text, "doctor:").join("\n");

        assert!(
            recipe.contains("KSHA256"),
            "doctor does not check the kernel digest, so it will be found later \
             and alone:\n{recipe}"
        );
        assert!(
            recipe.contains("grep -vx kernel-pin"),
            "the kernel pin would be printed as something to apt install"
        );
    }

    #[test]
    fn nothing_is_downloaded_or_compiled_before_the_prerequisites_are_checked() {
        // Step 1 of the exit criterion is a person outside the project booting
        // this with no help. What stops that person is never a hard problem: it
        // is a missing package, found one at a time, each one only after
        // everything before it succeeded — so a missing `bc` costs the whole
        // kernel download and build, and the next missing tool costs it again.
        //
        // `doctor` collects them instead. It only helps if it runs *first*: as
        // the last prerequisite of `all` it would report a missing compiler
        // after make had already tried to use it. Order in a prerequisite list
        // is the kind of thing a later edit reshuffles without a thought, and
        // nothing else in this repository would notice.
        let text = image_makefile();
        let line = text
            .lines()
            .find(|l| l.starts_with("all:"))
            .expect("the image Makefile has an `all` target");
        let prerequisites: Vec<&str> = line.trim_start_matches("all:").split_whitespace().collect();
        assert_eq!(
            prerequisites.first(),
            Some(&"doctor"),
            "`all` builds before it checks: {line}"
        );

        // And that the silent one is among the checks. pahole's absence does
        // not fail the build: Kconfig drops CONFIG_DEBUG_INFO_BTF without a
        // word, the kernel builds and boots, and the only symptom appears
        // several steps later as thalyx-lsm failing to attach — with the blame
        // landing on the loader, which had nothing to do with it.
        assert!(
            recipe_of(&text, "doctor:")
                .iter()
                .any(|l| l.contains("pahole")),
            "the doctor does not check for pahole, and its absence is silent"
        );
    }

    #[test]
    fn the_kernel_is_checked_for_the_hooks_the_object_attaches_to() {
        // On 2026-08-04 the image booted, mounted everything, and then said it
        // could not attach because the kernel had no `bpf_lsm_socket_connect`.
        // `config-check` could not have caught it: it compares what
        // thalyx.config asked for against what came out, and nothing had asked
        // for CONFIG_SECURITY_NETWORK. Only the built kernel knows.
        let text = image_makefile();
        let line = text
            .lines()
            .find(|l| l.starts_with("all:"))
            .expect("the image Makefile has an `all` target");
        assert!(
            line.split_whitespace().any(|word| word == "hook-check"),
            "a full build no longer asks the kernel for its hooks: {line}"
        );

        // And the names are asked of the binary, never typed here. The rule is
        // the one `attached.rs` already keeps: two lists that have to agree,
        // kept in two places, disagree eventually — and this disagreement is a
        // kernel built without the hook the programs actually use, which is
        // exactly the failure this target exists to catch.
        let recipe = recipe_of(&text, "hook-check:");
        assert!(
            recipe.iter().any(|l| l.contains("enforce hooks")),
            "hook-check no longer asks the object what it attaches to"
        );
        for line in &recipe {
            assert!(
                !line.contains("bpf_lsm_"),
                "a hook name is written into the Makefile instead of read \
                 from the object: {line}"
            );
        }
    }

    #[test]
    fn the_disk_carries_the_module_to_install_and_not_the_module_installed() {
        // `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md` step 2 is a
        // person installing a signed module from a local repository, and step 3
        // is confirming its permissions on the trusted path. A machine that
        // boots with the module already installed makes both unperformable —
        // there is nothing left to install, so the prompt is never reached.
        //
        // That is a one-line change away at all times, and it would break the
        // exit criterion while every test still passed and the machine still
        // looked better: it would boot listing a module. Hence reading the
        // build rather than trusting it.
        let text = image_makefile();
        let recipe = recipe_of(&text, "store-stage:");

        let installs: Vec<&&str> = recipe
            .iter()
            .filter(|line| !line.trim_start().starts_with('#'))
            .filter(|line| line.contains("module install"))
            .collect();
        assert!(
            installs.is_empty(),
            "the store stage installs the module, so there is nothing left for a\n\
             person to install and the trusted path is never reached:\n  {installs:?}"
        );

        assert!(
            recipe.iter().any(|line| line.contains("/repo")),
            "nothing in the store stage puts a bundle in a repository, so the\n\
             machine would boot with nothing to install"
        );
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
    fn no_line_the_shell_runs_carries_a_backtick() {
        // Twice in one afternoon a help message was written as
        // `echo "run \`sudo make store\`"`, and inside double quotes a backtick
        // is command substitution: the message explaining what to run would
        // have run it. Comment lines are exempt because the shell discards them
        // without expanding anything, and the prose has to be able to quote a
        // command.
        for (number, line) in image_makefile().lines().enumerate() {
            let Some(recipe) = line.strip_prefix('\t') else {
                continue;
            };
            if recipe.trim_start_matches(['@', '-', '+']).starts_with('#') {
                continue;
            }
            assert!(
                !recipe.contains('`'),
                "image/Makefile:{}: a backtick on a line the shell runs — \
                 that is command substitution, not quoting: {}",
                number + 1,
                recipe.trim()
            );
        }
    }

    #[test]
    fn staticness_is_checked_against_the_elf_and_not_against_a_sentence() {
        // `file … | grep 'statically linked'` refused a binary that was
        // perfectly static: Rust links musl as static-pie and file(1) calls
        // that `static-pie linked`. Two phrases, one absence of a loader, and
        // the build stopped on the wording. What matters is whether the program
        // asks for an interpreter, which is a program header.
        let makefile = image_makefile();
        // Comments excluded: the note explaining the mistake has to be allowed
        // to quote it. Only what the shell actually runs is the check.
        let commands: String = makefile
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !commands.contains("statically linked"),
            "the static check is matching file(1)'s prose again"
        );
        assert!(
            commands.contains("readelf") && commands.contains("INTERP"),
            "nothing in the image Makefile checks for a dynamic loader"
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
    fn what_the_machine_remembers_lands_on_the_disk_and_not_on_the_tmpfs() {
        // Step 6 of the exit criterion is restarting the machine and finding
        // that it still knows what was being done. The root filesystem is a
        // tmpfs that keeps nothing, so where `memory.db` sits is the entire
        // difference between that step passing and it being unperformable —
        // and the two are indistinguishable right up until the power goes off,
        // which is the one moment the step is about.
        //
        // `crates/thalyx-cli/src/agent.rs` puts it at <store root>/state, so
        // what has to hold is that the store root is a mount from the disk.
        // Asserting it here, against the mount table itself, is the closest a
        // machine with no reboot can get to asserting the reboot.
        let memory = Path::new("/opt/thalyx").join("state").join("memory.db");
        let carrier = SUBVOLUMES
            .iter()
            .filter(|s| memory.starts_with(s.target))
            .max_by_key(|s| s.target.len())
            .expect("the memory has to be under something that comes off the disk");
        assert_eq!(
            carrier.name, "system",
            "what the machine remembers would not survive a boot"
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
    fn the_names_thalyx_creates_are_the_names_pid_1_mounts() {
        // The same failure as the Makefile test above, one crate over. `thalyx
        // disk format` creates what `thalyx_btrfs::DECREED` lists and PID 1
        // mounts what `SUBVOLUMES` lists; a machine whose installer made two of
        // three boots and reports a broken store, with the message naming a
        // mount that failed rather than a subvolume nobody made.
        let mut created: Vec<&str> = thalyx_btrfs::DECREED.to_vec();
        created.sort_unstable();
        let mut mounted: Vec<&str> = subvolume_names().collect();
        mounted.sort_unstable();
        assert_eq!(
            created, mounted,
            "thalyx-btrfs and PID 1 disagree about what a store is made of"
        );
    }

    #[test]
    fn the_workspace_default_is_a_directory_the_image_actually_has() {
        // `/tmp` is the obvious default and the image has no `/tmp`. The mount
        // points the subvolume step needs would fail to be created on the only
        // machine where this has to work, and the error would name a path a
        // developer sees on every other machine.
        let root = WORKSPACE
            .strip_prefix('/')
            .and_then(|rest| rest.split('/').next())
            .expect("the workspace default is an absolute path");
        let image = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/image.rs");
        let text = std::fs::read_to_string(&image).expect("image.rs is part of the crate");
        assert!(
            text.contains(&format!("\"{root}\",")),
            "the image archive has no `{root}` directory, so {WORKSPACE} cannot be made there"
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
