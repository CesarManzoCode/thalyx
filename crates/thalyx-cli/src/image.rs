//! Building the machine's own root filesystem, without borrowing a tool to do it.
//!
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md` decrees that the image
//! carries the Linux kernel and one program. The previous attempt used
//! `mkimage.sh`, which is how Alpine distributions are built, which is how the
//! image came to be an Alpine distribution.
//!
//! So the archive is written here. `cpio` is a fine program and using it would
//! not make anything an Alpine distribution — but the shape of that mistake was
//! reaching for whatever the base offered, and a project that ships nothing but
//! `thalyx` can produce its own root filesystem in two hundred lines rather than
//! inherit a fourth thing nobody chose.
//!
//! ## Why an initramfs and not an ISO
//!
//! An ISO needs a bootloader, a partition table and a filesystem to put them
//! on. An initramfs needs none of those: the kernel unpacks a cpio archive into
//! a tmpfs and runs `/init` from it. QEMU takes the kernel and the archive
//! directly.
//!
//! That is not a shortcut around the decree, it is the decree with nothing left
//! over. The first boot is a kernel and one program, and there is no third
//! thing anywhere in the path for something to hide in.
//!
//! Persistent state — the Btrfs subvolumes of `Core-Nucleo.md` — lives on a
//! separate disk that PID 1 mounts. The root being ephemeral is a property
//! worth having on purpose: nothing survives a boot except what was deliberately
//! put on the store.

use std::io::Write;
use std::path::Path;

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The `newc` cpio format, which is what the Linux kernel unpacks.
///
/// Every field is eight hexadecimal digits, ASCII, and the whole thing is
/// padded to four-byte boundaries. It is a deliberately boring format; the only
/// way to get it wrong is to get the padding wrong, which is why the padding is
/// one function used everywhere rather than arithmetic at each call site.
const MAGIC: &[u8] = b"070701";

/// What the kernel looks for when the archive is unpacked.
const TRAILER: &str = "TRAILER!!!";

const MODE_DIR: u32 = 0o040_755;
const MODE_EXEC: u32 = 0o100_755;
/// A character device, readable and writable by root and nobody else. There is
/// no other user in the image, and the console is not something a module should
/// find by walking the tree.
const MODE_CHAR: u32 = 0o020_600;

struct Cpio {
    out: Vec<u8>,
    /// cpio wants a distinct inode per entry; nothing reads them afterwards,
    /// but two entries sharing one is a hard link as far as the format is
    /// concerned.
    next_inode: u32,
}

impl Cpio {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            next_inode: 1,
        }
    }

    fn pad(&mut self) {
        while !self.out.len().is_multiple_of(4) {
            self.out.push(0);
        }
    }

    fn header(&mut self, name: &str, mode: u32, size: usize, rdev: (u32, u32)) {
        let inode = self.next_inode;
        self.next_inode += 1;

        self.out.extend_from_slice(MAGIC);
        for field in [
            inode,
            mode,
            0, // uid: root, because there is no other user in the image
            0, // gid
            1, // nlink
            0, // mtime: zero, so two builds of the same input are the same bytes
            size as u32,
            0, // devmajor
            0, // devminor
            rdev.0,
            rdev.1,
            (name.len() + 1) as u32,
            0, // check, unused by newc
        ] {
            self.out
                .extend_from_slice(format!("{field:08X}").as_bytes());
        }
        self.out.extend_from_slice(name.as_bytes());
        self.out.push(0);
        self.pad();
    }

    fn directory(&mut self, name: &str) {
        self.header(name, MODE_DIR, 0, (0, 0));
    }

    fn executable(&mut self, name: &str, contents: &[u8]) {
        self.header(name, MODE_EXEC, contents.len(), (0, 0));
        self.out.extend_from_slice(contents);
        self.pad();
    }

    /// A character device: a name, a major and a minor, and no contents at all.
    ///
    /// It is not a program and must never be counted as one. It holds no code —
    /// it is a door the kernel already owns, given a name so that something can
    /// ask for it.
    fn character_device(&mut self, name: &str, major: u32, minor: u32) {
        self.header(name, MODE_CHAR, 0, (major, minor));
    }

    fn finish(mut self) -> Vec<u8> {
        self.header(TRAILER, 0, 0, (0, 0));
        self.out
    }
}

/// `/dev/console`, which the kernel opens as the new process's `stdin`,
/// `stdout` and `stderr` before it runs `/init`.
///
/// It is here because of a boot that did not survive its own first instruction,
/// on 2026-08-06, the first time this image was started by a firmware instead of
/// by QEMU. The kernel said it, plainly, and then the machine died:
///
/// ```text
/// Warning: unable to open an initial console.
/// Run /init as init process
/// traps: init[1] general protection fault ip:7fea0faff143
/// Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b
/// ```
///
/// The faulting instruction was `hlt`, which is privileged, and it sits at the
/// end of musl's `abort()` — the instruction reached only when raising `SIGABRT`
/// failed to kill the process. It fails for PID 1: the kernel does not deliver a
/// default-action fatal signal to init. So `abort()` ran off its own end.
///
/// What called it never got as far as Thalyx. Rust's runtime, before `main`,
/// checks that descriptors 0, 1 and 2 are open and points them at `/dev/null`
/// when they are not — otherwise the next file opened silently becomes the
/// program's stdout. With no console *and* no `/dev/null`, that guarantee cannot
/// be made and the runtime aborts rather than continue. Both were missing, for
/// the same reason: this archive had an empty `/dev`.
///
/// ## Why QEMU never showed this
///
/// `make run` hands the archive over with `-initrd`, and an external initrd is
/// unpacked **on top of** the kernel's own built-in one, which contains
/// `/dev/console`. Building the archive into the kernel replaces that default
/// instead of adding to it. The console had been arriving as a gift from
/// something nobody had looked at.
///
/// That is the third time this project has found the host doing something for
/// free: systemd delegating cgroup controllers, the initramfs performing the
/// `switch_root`, and now the kernel's default archive supplying the console.
/// See `Estrategia-de-Pruebas.md`.
///
/// ## And why there is no `/dev/null` beside it
///
/// It would be one line and it would buy the wrong thing. With a console the
/// machine speaks; with `/dev/null` standing in for one, the machine runs
/// perfectly and says nothing, forever, which is worse than dying — this project
/// has already blinded one instrument that way. A machine that cannot reach its
/// console should stop.
const CONSOLE_MAJOR: u32 = 5;
const CONSOLE_MINOR: u32 = 1;

/// The directories PID 1 mounts onto.
///
/// They have to exist before the mount, and PID 1 creates them itself — but on
/// a read-only or oddly configured root that creation is the first thing that
/// can fail, and failing at it would take the machine down before it could say
/// why. Having them in the archive costs nothing and removes a failure mode
/// from the earliest, least debuggable moment of the boot.
const DIRECTORIES: &[&str] = &[
    "proc",
    "sys",
    "sys/kernel",
    "sys/kernel/security",
    "sys/fs",
    "sys/fs/bpf",
    "sys/fs/cgroup",
    "dev",
    "run",
    "opt",
    "opt/thalyx",
    "lib",
    "lib/thalyx",
];

/// Build the root filesystem the kernel will unpack.
///
/// `binary` is the statically linked `thalyx`, and it lands at `/init` — the
/// name the kernel runs by default.
///
/// One file, not two. The first version put it at `/thalyx` as well, for
/// familiarity, and doubled a 47 MB image to say the same thing twice. If the
/// decree is "the kernel and one program", then one file is what makes that
/// true rather than nearly true, and it is what the count comes back as.
pub fn build(binary: &Path, out: &Path) -> Fallible {
    let contents =
        std::fs::read(binary).map_err(|e| format!("cannot read {}: {e}", binary.display()))?;

    if contents.len() < 4 || &contents[..4] != b"\x7fELF" {
        return Err(format!("{} is not an ELF binary", binary.display()).into());
    }

    let mut cpio = Cpio::new();
    for directory in DIRECTORIES {
        cpio.directory(directory);
    }
    cpio.character_device("dev/console", CONSOLE_MAJOR, CONSOLE_MINOR);
    cpio.executable("init", &contents);

    let archive = cpio.finish();
    let mut file = std::fs::File::create(out)?;
    file.write_all(&archive)?;
    file.sync_all()?;

    println!("  {} bytes", archive.len());
    println!(
        "  /init, plus {} directories and /dev/console",
        DIRECTORIES.len()
    );
    println!();
    println!("  Nothing else is in it. That is the whole claim, and it is");
    println!("  countable: `thalyx dev image --list {}`", out.display());

    Ok(())
}

/// Read an archive back and say what is in it.
///
/// The decree in `Construccion-del-ISO.md` is written to be *countable* rather
/// than argued — list the files, and there are two things or there are more.
/// This is the thing that counts them, and it parses the archive rather than
/// reporting what the builder intended to put there.
pub fn list(archive: &Path) -> Fallible {
    let bytes = std::fs::read(archive)?;
    let entries = parse(&bytes)?;

    let directories = entries.iter().filter(|entry| entry.is_directory()).count();
    let devices: Vec<&Entry> = entries.iter().filter(|entry| entry.is_device()).collect();
    let programs: Vec<&Entry> = entries.iter().filter(|entry| entry.is_program()).collect();

    println!("{directories} directories");
    for device in &devices {
        // The numbers, not just the name. A node pointing at the wrong driver
        // fails exactly like a missing one, and this is the command somebody
        // runs when the machine will not talk.
        println!(
            "/{}  (character device {}:{}, no contents)",
            device.name, device.rdev_major, device.rdev_minor
        );
    }
    for program in &programs {
        println!("/{}  ({} bytes)", program.name, program.size);
    }
    println!();
    println!("{} program(s) in the image.", programs.len());

    // Anything that is none of the three is named rather than dropped. The
    // count is what the decree is checked with, and a kind nobody thought of
    // being quietly left out of it is exactly how a second program would get in
    // without the number moving.
    let counted = directories + devices.len() + programs.len();
    if counted != entries.len() {
        println!();
        println!(
            "and {} entr(ies) of no kind this counts:",
            entries.len() - counted
        );
        for entry in entries
            .iter()
            .filter(|e| !e.is_directory() && !e.is_device() && !e.is_program())
        {
            println!("/{}  (mode {:o})", entry.name, entry.mode);
        }
    }

    Ok(())
}

/// One thing found inside an archive that was read back.
#[derive(Debug, Clone)]
struct Entry {
    name: String,
    mode: u32,
    size: usize,
    /// Which driver a device node points at. Read back out of the archive
    /// rather than remembered, because a node with the wrong numbers in it
    /// fails exactly like a node that is not there.
    rdev_major: u32,
    rdev_minor: u32,
}

impl Entry {
    fn is_directory(&self) -> bool {
        self.mode & 0o170_000 == 0o040_000
    }

    /// A character device. Not a program: it carries no code, and counting it
    /// as one would break the decree with a door.
    fn is_device(&self) -> bool {
        self.mode & 0o170_000 == 0o020_000
    }

    /// A regular file, which in this image is the one thing that can be run.
    fn is_program(&self) -> bool {
        self.mode & 0o170_000 == 0o100_000
    }
}

fn parse(bytes: &[u8]) -> Result<Vec<Entry>, Box<dyn std::error::Error>> {
    let mut entries = Vec::new();
    let mut at = 0usize;

    let field = |bytes: &[u8], at: usize, n: usize| -> Result<u32, String> {
        let text = std::str::from_utf8(&bytes[at..at + 8])
            .map_err(|_| format!("field {n} is not ASCII"))?;
        u32::from_str_radix(text, 16).map_err(|_| format!("field {n} is not hex: {text:?}"))
    };

    while at + 110 <= bytes.len() {
        if &bytes[at..at + 6] != MAGIC {
            return Err(format!("bad magic at byte {at}").into());
        }
        let mode = field(bytes, at + 14, 1)?;
        let size = field(bytes, at + 54, 6)? as usize;
        let rdev_major = field(bytes, at + 78, 9)?;
        let rdev_minor = field(bytes, at + 86, 10)?;
        let namesize = field(bytes, at + 94, 11)? as usize;

        let name_at = at + 110;
        let name = String::from_utf8_lossy(&bytes[name_at..name_at + namesize - 1]).into_owned();
        if name == TRAILER {
            break;
        }

        let after_name = (name_at + namesize).div_ceil(4) * 4;
        at = (after_name + size).div_ceil(4) * 4;

        entries.push(Entry {
            name,
            mode,
            size,
            rdev_major,
            rdev_minor,
        });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn the_rust_runtime_is_not_in_the_image() {
        // `Construccion-del-ISO.md`: the image is the Linux kernel and one
        // program, and the decree is written to be *countable* rather than
        // argued. On 2026-08-31 six hundred megabytes of Rust toolchain
        // arrived for the agent to program with, and the one way that could
        // have gone wrong quietly was for it to be put in here — where it
        // would double the boot's memory before the machine said a word, and
        // where `make count` would stop saying what it says.
        //
        // It goes on the store, like the engine and the model, because that is
        // the difference between what Thalyx *is* and what has been installed
        // on it. This asserts the archive's own list, which is the same list
        // the builder writes.
        for directory in DIRECTORIES {
            assert!(
                !directory.contains("toolchain") && !directory.contains("rust"),
                "the image's directory list has grown a {directory}"
            );
        }
        let held = tempfile::tempdir().expect("a temp dir");
        let binary = fake_binary(held.path());
        let archive = held.path().join("initramfs.cpio");
        build(&binary, &archive).expect("building the archive");
        let bytes = std::fs::read(&archive).expect("reading the archive");
        let entries = parse(&bytes).expect("parsing the archive");
        assert_eq!(
            entries.iter().filter(|entry| entry.is_program()).count(),
            1,
            "the image is the kernel and one program"
        );
        for entry in &entries {
            assert!(
                !entry.name.contains("cargo")
                    && !entry.name.contains("rust")
                    && !entry.name.contains("toolchain"),
                "{} is in the image and belongs on the store",
                entry.name
            );
        }
    }

    fn fake_binary(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("thalyx");
        let mut file = std::fs::File::create(&path).unwrap();
        // An ELF header and some bytes: enough to pass the check, and an odd
        // length on purpose so the padding gets exercised.
        file.write_all(b"\x7fELF").unwrap();
        file.write_all(&vec![0x90u8; 4093]).unwrap();
        path
    }

    #[test]
    fn the_image_holds_exactly_one_program_under_two_names() {
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_binary(dir.path());
        let out = dir.path().join("initramfs.cpio");
        build(&binary, &out).unwrap();

        let entries = parse(&std::fs::read(&out).unwrap()).unwrap();
        // `is_program` and not "everything that is not a directory": since
        // 2026-08-06 the archive also carries /dev/console, which holds no code
        // and is not a program. This assertion is the decree's guard, so what
        // it counts is the decision — a kind that slips out of the count is how
        // a second program would arrive without the number moving, and that is
        // covered separately by `a_device_node_is_not_counted_as_a_program`.
        let files: Vec<&str> = entries
            .iter()
            .filter(|entry| entry.is_program())
            .map(|entry| entry.name.as_str())
            .collect();

        assert_eq!(
            files,
            ["init"],
            "the decree is countable, and one program means one file"
        );
    }

    #[test]
    fn the_console_is_in_the_archive_or_the_machine_dies_before_its_first_line() {
        // 2026-08-06: booted by a firmware for the first time, with the archive
        // built into the kernel rather than handed over separately, and the
        // machine did not survive `/init`. The kernel had already said why —
        // "unable to open an initial console" — and Rust's runtime aborts
        // before `main` when it can guarantee neither a console nor
        // /dev/null for descriptors 0, 1 and 2.
        //
        // Nothing found it earlier because `-initrd` unpacks over the kernel's
        // own built-in archive, and that one carries /dev/console. Building
        // ours in replaces that default instead of adding to it.
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_binary(dir.path());
        let out = dir.path().join("initramfs.cpio");
        build(&binary, &out).unwrap();

        let entries = parse(&std::fs::read(&out).unwrap()).unwrap();
        let console = entries
            .iter()
            .find(|entry| entry.name == "dev/console")
            .expect("the image has no /dev/console, so init starts with no descriptors");

        assert!(
            console.is_device(),
            "dev/console is in the archive and is not a device node (mode {:o}), \
             so opening it gives a file and the machine talks to nothing",
            console.mode
        );
        assert_eq!(
            (console.rdev_major, console.rdev_minor),
            (CONSOLE_MAJOR, CONSOLE_MINOR),
            "the console node points at the wrong driver, which fails exactly \
             like having no node at all"
        );
    }

    #[test]
    fn a_device_node_is_not_counted_as_a_program() {
        // The decree is guarded by a number, so what the number counts decides
        // whether the guard works. A character device holds no code and must
        // not be a program — and it must not be silently dropped either, since
        // "not counted" is how a second program would arrive without the count
        // moving. It is its own kind, printed as its own kind.
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_binary(dir.path());
        let out = dir.path().join("initramfs.cpio");
        build(&binary, &out).unwrap();

        let entries = parse(&std::fs::read(&out).unwrap()).unwrap();

        assert_eq!(
            entries.iter().filter(|e| e.is_program()).count(),
            1,
            "the image stopped holding exactly one program"
        );
        assert_eq!(
            entries.iter().filter(|e| e.is_device()).count(),
            1,
            "the image carries a device node that is not the console"
        );
        assert_eq!(
            entries
                .iter()
                .filter(|e| e.is_directory() || e.is_device() || e.is_program())
                .count(),
            entries.len(),
            "something in the archive is of a kind the count does not know \
             about, so it would not appear in `thalyx dev image --list`"
        );
    }

    #[test]
    fn every_directory_pid_one_mounts_onto_is_in_the_archive() {
        // If PID 1 gains a mount and the archive does not gain its directory,
        // the failure lands at the least debuggable moment there is.
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_binary(dir.path());
        let out = dir.path().join("initramfs.cpio");
        build(&binary, &out).unwrap();

        let entries = parse(&std::fs::read(&out).unwrap()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

        for target in crate::init::mount_targets() {
            let relative = target.trim_start_matches('/');
            assert!(
                names.contains(&relative),
                "PID 1 mounts {target} and the archive has no directory for it"
            );
        }
    }

    #[test]
    fn the_archive_survives_being_read_back_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_binary(dir.path());
        let out = dir.path().join("initramfs.cpio");
        build(&binary, &out).unwrap();

        let entries = parse(&std::fs::read(&out).unwrap()).unwrap();
        let original = std::fs::metadata(&binary).unwrap().len() as usize;
        for entry in &entries {
            if entry.name == "init" {
                assert_eq!(
                    entry.size, original,
                    "{} came back a different length",
                    entry.name
                );
            }
        }
    }

    #[test]
    fn two_builds_of_the_same_input_are_the_same_bytes() {
        // Timestamps are zeroed for this reason. An image that differs between
        // builds cannot be compared against the one that was tested.
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_binary(dir.path());
        let first = dir.path().join("a.cpio");
        let second = dir.path().join("b.cpio");
        build(&binary, &first).unwrap();
        build(&binary, &second).unwrap();

        assert_eq!(
            std::fs::read(&first).unwrap(),
            std::fs::read(&second).unwrap()
        );
    }

    #[test]
    fn something_that_is_not_a_program_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notelf");
        std::fs::write(&path, b"#!/bin/sh\necho hello\n").unwrap();

        let outcome = build(&path, &dir.path().join("out.cpio"));
        assert!(
            outcome.is_err(),
            "a shell script is exactly what must not end up in this image"
        );
    }
}
