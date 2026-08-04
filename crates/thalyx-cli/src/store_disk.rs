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
