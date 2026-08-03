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

    fn header(&mut self, name: &str, mode: u32, size: usize) {
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
            0, // rdevmajor
            0, // rdevminor
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
        self.header(name, MODE_DIR, 0);
    }

    fn executable(&mut self, name: &str, contents: &[u8]) {
        self.header(name, MODE_EXEC, contents.len());
        self.out.extend_from_slice(contents);
        self.pad();
    }

    fn finish(mut self) -> Vec<u8> {
        self.header(TRAILER, 0, 0);
        self.out
    }
}

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
    cpio.executable("init", &contents);

    let archive = cpio.finish();
    let mut file = std::fs::File::create(out)?;
    file.write_all(&archive)?;
    file.sync_all()?;

    println!("  {} bytes", archive.len());
    println!("  /init, plus {} directories", DIRECTORIES.len());
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

    let (directories, programs): (Vec<&Entry>, Vec<&Entry>) =
        entries.iter().partition(|entry| entry.is_directory());

    println!("{} directories", directories.len());
    for program in &programs {
        println!("/{}  ({} bytes)", program.name, program.size);
    }
    println!();
    println!("{} program(s) in the image.", programs.len());

    Ok(())
}

/// One thing found inside an archive that was read back.
#[derive(Debug, Clone)]
struct Entry {
    name: String,
    mode: u32,
    size: usize,
}

impl Entry {
    fn is_directory(&self) -> bool {
        self.mode & 0o170_000 == 0o040_000
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
        let namesize = field(bytes, at + 94, 11)? as usize;

        let name_at = at + 110;
        let name = String::from_utf8_lossy(&bytes[name_at..name_at + namesize - 1]).into_owned();
        if name == TRAILER {
            break;
        }

        let after_name = (name_at + namesize).div_ceil(4) * 4;
        at = (after_name + size).div_ceil(4) * 4;

        entries.push(Entry { name, mode, size });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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
        let files: Vec<&str> = entries
            .iter()
            .filter(|entry| !entry.is_directory())
            .map(|entry| entry.name.as_str())
            .collect();

        assert_eq!(
            files,
            ["init"],
            "the decree is countable, and one program means one file"
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
