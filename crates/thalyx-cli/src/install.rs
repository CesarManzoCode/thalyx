//! `thalyx install` — the act that turns a disk into a machine.
//!
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md`, at the end of *Los tres
//! subvolúmenes*: "Lo que sigue faltando: el instalador. Particionar el disco (GPT),
//! escribir la partición EFI con el kernel adentro, y formatear la otra como store.
//! Las dos piezas que costaban ya están; lo que falta es el acto que las junta."
//! This is that act. [`thalyx_install`] is where the bytes are written.
//!
//! ## Why it is a top-level verb and not `thalyx disk install`
//!
//! `disk` holds the things a human does to one disk. This does something to a
//! machine: after it, a PC that had no operating system has Thalyx and boots without
//! the medium that installed it. It is the largest thing Thalyx can be asked to do
//! and the name should say so.
//!
//! ## The confirmation
//!
//! The same one `thalyx disk format` uses, and for the same reason, which is worth
//! repeating rather than referring to: this destroys a whole disk, the argument is
//! one word, `/dev/sda` and `/dev/sdb` differ by a keystroke, and a `y` confirms a
//! sentence the human stopped reading. Typing the path back is the only answer that
//! cannot be given by accident to the wrong disk.
//!
//! What is different here is what gets said before the question. `format` says what
//! is on the disk now; this says that **and** what will replace it, because a
//! confirmation that names only the loss is half a question.

use std::path::{Path, PathBuf};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// Where the subvolume step puts its mount points. Same as `thalyx disk`, and the
/// same reason: the image has no `/tmp`.
const WORKSPACE: &str = "/run/thalyx/store-setup";

/// Where the kernel that goes on the boot partition comes from.
///
/// Two cases and they are not the same act. One is a person naming a file they
/// built; the other is the machine reading its own boot medium, which is what an
/// install from inside the image has to do because there is no path to name.
enum Kernel {
    Named(PathBuf),
    OnTheMedium(PathBuf),
}

#[derive(clap::Args)]
pub struct InstallArgs {
    /// The whole disk to install onto, e.g. /dev/nvme0n1. Everything on it is lost.
    pub device: PathBuf,

    /// The kernel to put on the boot partition, as \EFI\BOOT\BOOTX64.EFI
    ///
    /// A bzImage built with CONFIG_EFI_STUB, which is a valid UEFI application —
    /// there is no bootloader here and none is wanted. `image/build/bzImage` is what
    /// `make -C image kernel` produces.
    ///
    /// Without it, the medium this machine was booted from is found and the kernel
    /// is read off it. That is the path an install *inside* the machine takes, where
    /// there is no shell and no path anybody could type.
    #[arg(long)]
    pub kernel: Option<PathBuf>,

    /// Print what would be written and do nothing
    #[arg(long)]
    pub plan: bool,

    /// Skip the confirmation. For scripts and tests.
    #[arg(long)]
    pub yes: bool,

    /// Where to put the mount points the store's subvolumes need.
    #[arg(long, default_value = WORKSPACE)]
    pub workspace: PathBuf,
}

pub fn run(args: InstallArgs) -> Fallible {
    // Measured before anything is said, so that the plan printed below is the plan
    // that would be written and not an illustration of one.
    let sectors = device_sectors(&args.device)?;
    let plan = thalyx_install::Plan::of(&args.device, sectors)?;

    println!("About to install Thalyx onto {}.", args.device.display());
    println!();
    describe_what_is_there(&args.device);
    println!();
    describe(&plan);
    println!();

    // Resolved **before** anything is said about destroying the disk, and before the
    // confirmation. An install that asked, got a yes, wiped the disk and only then
    // found it had no kernel to write would have destroyed the disk for nothing —
    // and this is the one command where that cannot be undone.
    let kernel = match &args.kernel {
        Some(named) => {
            if !named.exists() {
                return Err(format!(
                    "there is no kernel at {}. Nothing has been written.\n  \
                     `make -C image kernel` produces one at image/build/bzImage.",
                    named.display()
                )
                .into());
            }
            Kernel::Named(named.clone())
        }
        None => {
            let found = thalyx_install::medium::find(Some(&args.device)).map_err(|error| {
                format!(
                    "no --kernel was given and no boot medium was found, so there is \n                       nothing to install. Nothing has been written.\n\n  {error}"
                )
            })?;
            println!(
                "  no --kernel given, so the kernel comes off {} — {} bytes",
                found.device.display(),
                found.kernel_bytes
            );
            Kernel::OnTheMedium(found.device)
        }
    };
    if let Kernel::Named(path) = &kernel {
        println!(
            "  the boot partition will hold {} — {} bytes",
            path.display(),
            std::fs::metadata(path)?.len()
        );
    }
    println!();

    if args.plan {
        println!("  --plan: nothing was written.");
        return Ok(());
    }

    println!(
        "  Everything on {} will be gone. This cannot be undone.",
        args.device.display()
    );
    println!();

    if args.yes {
        println!("  confirmed with --yes");
    } else {
        // Silence is not consent, the same rule the capability prompt keeps.
        let asked = crate::ask::Accepts::Exactly(args.device.display().to_string());
        match crate::ask::confirm("  Type the disk's path to confirm: ", &asked) {
            crate::ask::Answered::Yes => {}
            crate::ask::Answered::No => {
                eprintln!("  that is not {}; refusing", args.device.display());
                return Err("the install was not confirmed".into());
            }
            crate::ask::Answered::NoOneToAsk => {
                eprintln!("  no terminal available to confirm; refusing");
                return Err("the install was not confirmed".into());
            }
            crate::ask::Answered::Unreadable => {
                eprintln!("  the answer could not be read; refusing");
                return Err("the install was not confirmed".into());
            }
        }
    }
    println!();

    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);

    // Taken off the medium only once the human has said yes, because reading forty
    // megabytes off a USB stick is not something to do to answer a `--plan`.
    let staged = match &kernel {
        Kernel::Named(path) => path.clone(),
        Kernel::OnTheMedium(device) => {
            std::fs::create_dir_all(&args.workspace)?;
            let staged = args.workspace.join("bzImage");
            let mut volume = thalyx_install::medium::Volume::open(device)?.ok_or_else(|| {
                format!(
                    "{} stopped being a readable FAT32 volume between finding it and \n  \
                     reading it. Nothing has been written.",
                    device.display()
                )
            })?;
            let bytes = volume.extract_boot_file(&staged)?;
            println!(
                "  ok  kernel       {bytes} bytes taken off {}",
                device.display()
            );
            staged
        }
    };

    let installed = thalyx_install::install(&args.device, &staged, &args.workspace, seconds)
        .inspect_err(|_| {
            // The disk is partly written by the time most failures here can happen,
            // and a message that stopped at the error would leave a person guessing
            // whether their old data is still there. It is not.
            println!();
            println!(
                "  The install did not finish. Whatever was on {} before is",
                args.device.display()
            );
            println!("  gone either way — the partition table is written first. Running");
            println!("  this again is safe and is the way to finish it.");
        })?;

    report(&installed);
    Ok(())
}

/// How big the disk is, in 512-byte sectors.
fn device_sectors(device: &Path) -> Result<u64, Box<dyn std::error::Error>> {
    use std::io::Seek;
    let mut file = std::fs::File::open(device)
        .map_err(|error| format!("could not open {}: {error}", device.display()))?;
    Ok(file.seek(std::io::SeekFrom::End(0))? / thalyx_install::gpt::SECTOR)
}

/// What is on the disk right now, read rather than assumed.
fn describe_what_is_there(device: &Path) {
    match thalyx_install::partitions::of(device) {
        Ok(existing) if existing.is_empty() => {
            println!("  it has no partitions the kernel can see");
        }
        Ok(existing) => {
            println!("  it has {} partition(s) now:", existing.len());
            for (number, path) in &existing {
                let what = match thalyx_btrfs::identify(path) {
                    Ok(thalyx_btrfs::Identity::Btrfs { label, .. }) if label.is_empty() => {
                        "btrfs, with no label".to_string()
                    }
                    Ok(thalyx_btrfs::Identity::Btrfs { label, .. }) => {
                        format!("btrfs, labelled `{label}`")
                    }
                    // Everything that is not Btrfs comes back the same way, and
                    // saying "not btrfs" would read as "empty". Thalyx cannot
                    // identify a filesystem it does not write, and pretending
                    // otherwise about a disk it is about to destroy is the wrong
                    // direction to be vague in.
                    Ok(_) => "something Thalyx does not recognise".to_string(),
                    Err(error) => format!("unreadable: {error}"),
                };
                println!("    {number}  {}  {what}", path.display());
            }
        }
        Err(error) => {
            println!("  could not read what is there: {error}");
            println!("  that is not permission to proceed — it is one more thing");
            println!("  unknown about a disk about to be overwritten.");
        }
    }
}

/// What will replace it.
fn describe(plan: &thalyx_install::Plan) {
    let mib = |sectors: u64| sectors * thalyx_install::gpt::SECTOR / (1024 * 1024);
    println!("  it will become:");
    println!(
        "    1  {:>8} MiB  FAT32, \\EFI\\BOOT\\BOOTX64.EFI — what the firmware starts",
        mib(plan.esp_sectors())
    );
    println!(
        "    2  {:>8} MiB  btrfs `{}` — system, modules, user",
        mib(plan.store_sectors()),
        thalyx_btrfs::LABEL
    );
    println!();
    println!("  There is no bootloader. The kernel is a UEFI application and the");
    println!("  firmware starts it directly, so the boot partition holds one file.");
}

/// What happened, one line per fact.
fn report(installed: &thalyx_install::Installed) {
    println!(
        "  ok  table        {} — 2 partitions, GPT with both copies",
        installed.device.display()
    );
    println!(
        "  ok  boot         {} — FAT32, {} free cluster(s)",
        installed.esp.display(),
        installed.boot.free_clusters
    );
    println!(
        "      {} — {} bytes",
        thalyx_install::fat::BOOT_PATH.join("\\"),
        installed.boot.kernel_bytes
    );
    println!(
        "  ok  store        {} — {} bytes, labelled `{}`",
        installed.store.display(),
        installed.filesystem.total_bytes,
        installed.filesystem.label
    );

    for (name, made) in &installed.subvolumes.subvolumes {
        let what = match made {
            thalyx_btrfs::Made::Created => "created",
            thalyx_btrfs::Made::AlreadyThere => "already there",
        };
        println!("  ok  subvolume    {name} — {what}");
    }
    println!();
    for (name, why) in &installed.subvolumes.mounted {
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
    if installed.subvolumes.is_a_store() {
        println!("  This disk is a Thalyx machine. Take the medium out and start it.");
    } else {
        // Printed and not returned as an error, because the boot half did work and
        // a person needs to know which half did not. The exit code is still a
        // failure — `install` returns the error from the subvolume step.
        println!("  The boot partition is written and the store is not finished.");
        println!("  `thalyx disk subvolumes` on the second partition completes it.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workspace_default_is_a_directory_the_image_actually_has() {
        // The same claim `store_disk.rs` makes about its own default, made again
        // because this is a second copy of the constant. `/tmp` is what a person
        // reaches for and the image has thirteen directories and no `/tmp` — so the
        // mount points would fail to be created on the only machine where installing
        // is the point.
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
    fn the_path_written_on_the_boot_partition_is_the_one_the_image_builds() {
        // Two places produce a boot medium — `image/Makefile` for the ISO and this
        // for the installed disk — and they must agree on the one path a firmware
        // looks for with nothing configured. If they drift, the ISO boots and the
        // machine it installs does not, which is the worst possible time to find out.
        let makefile = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../image/Makefile")
            .canonicalize()
            .expect("the image Makefile is part of the repository");
        let text = std::fs::read_to_string(&makefile).expect("reading the image Makefile");
        let path = thalyx_install::fat::BOOT_PATH.join("/");
        assert!(
            text.contains(&path),
            "image/Makefile does not build {path}, which is what the installer writes"
        );
    }
}
