//! What Thalyx wrote, handed to the people who write Btrfs for a living.
//!
//! `btrfs check` walks the trees, verifies every block's checksum, follows every
//! back reference, adds up what each block group says is used against what is
//! actually allocated in it, and resolves every logical address through the chunk
//! map. That is close to everything a mount validates, and it is available on a
//! machine with no Btrfs support in its kernel — which is every machine this
//! project develops on.
//!
//! **It is not a mount.** `btrfs check` reads the filesystem with btrfs-progs'
//! own code, and btrfs-progs and the kernel have disagreed before. Passing here
//! establishes that the format is right; it does not establish that Cesar's kernel
//! will mount it, and nothing in this file should be quoted as though it did.
//!
//! ## The skip, and the control
//!
//! btrfs-progs is a *development* dependency and deliberately not a runtime one:
//! the whole point of this crate is that the image has no `mkfs.btrfs` on it. So
//! these tests skip where it is absent, print `NOT PROVEN`, and
//! `THALYX_REQUIRE_BTRFS_PROGS=1` turns the skip into a failure — one variable for
//! one requirement, per rule 3 of
//! `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md`.
//!
//! And there is a control, per rule 4. A test that says "`btrfs check` accepted
//! this" looks identical to one where `btrfs check` was invoked wrongly, or where
//! its exit status was ignored, or where it silently declined to look at the file
//! at all. So one test damages a filesystem this crate wrote and requires that
//! `btrfs check` *refuse* it. Without that, a green run would mean nothing.

use std::path::Path;
use std::process::Command;

/// A device-sized sparse file to format.
fn device(bytes: u64) -> tempfile::NamedTempFile {
    let file = tempfile::Builder::new()
        .prefix("thalyx-store-")
        .suffix(".img")
        .tempfile()
        .expect("a temporary file");
    file.as_file()
        .set_len(bytes)
        .expect("a sparse file of the requested size");
    file
}

/// The uuids, fixed rather than random, so a failing run can be reproduced with
/// the same bytes.
fn uuids() -> thalyx_btrfs::Uuids {
    thalyx_btrfs::Uuids {
        fsid: [0x11; 16],
        device: [0x22; 16],
        chunk_tree: [0x33; 16],
        subvolume: [0x44; 16],
    }
}

/// Whether `btrfs` is here, and what to do about it not being.
///
/// Returns `false` after saying so, unless the requirement is demanded — in which
/// case it fails, because a check that could not be made must be capable of being
/// turned into a failure by the person who knows their machine can make it.
fn btrfs_progs_available() -> bool {
    let present = Command::new("btrfs")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success());
    if present {
        return true;
    }
    if std::env::var_os("THALYX_REQUIRE_BTRFS_PROGS").is_some() {
        panic!(
            "THALYX_REQUIRE_BTRFS_PROGS is set and `btrfs` is not on the PATH. \
             Install btrfs-progs, or unset the variable and accept that this claim \
             is NOT PROVEN on this machine."
        );
    }
    eprintln!(
        "NOT PROVEN: btrfs-progs is not installed, so nothing checked whether the \
         filesystem Thalyx wrote is valid. Set THALYX_REQUIRE_BTRFS_PROGS=1 to make \
         this a failure instead of a skip."
    );
    false
}

/// `btrfs check <path>`: whether it was happy, and everything it said.
fn check(path: &Path) -> (bool, String) {
    let output = Command::new("btrfs")
        .arg("check")
        .arg(path)
        .output()
        .expect("btrfs is on the PATH, which was checked");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), said)
}

#[test]
fn a_filesystem_thalyx_wrote_passes_btrfs_check_with_no_errors() {
    if !btrfs_progs_available() {
        return;
    }
    let file = device(8 * 1024 * 1024 * 1024);
    thalyx_btrfs::write(file.path(), thalyx_btrfs::LABEL, &uuids(), 1_786_041_195)
        .expect("writing a store onto an 8 GiB device");

    let (ok, said) = check(file.path());
    assert!(ok, "btrfs check refused a filesystem Thalyx wrote:\n{said}");

    // The exit status is not enough on its own. `btrfs check` reports some
    // problems on stdout and still exits zero, which would make a broken
    // filesystem indistinguishable from a sound one by status alone.
    assert!(
        said.contains("no error found"),
        "btrfs check exited zero without saying it found nothing wrong:\n{said}"
    );
}

#[test]
fn btrfs_check_refuses_a_filesystem_that_has_been_damaged() {
    // The control. Without it, every other test in this file is satisfied by a
    // `btrfs check` that does not actually look — a wrong argument, a status
    // nobody read, a file it declined to open. One flipped bit in one tree block
    // has to come back as a refusal, or the instrument is not measuring anything.
    if !btrfs_progs_available() {
        return;
    }
    let file = device(8 * 1024 * 1024 * 1024);
    thalyx_btrfs::write(file.path(), thalyx_btrfs::LABEL, &uuids(), 1_786_041_195)
        .expect("writing a store");

    // The middle of the root tree's block, which is the first block of the
    // metadata chunk. Both copies, because damaging one of a DUP pair is
    // something Btrfs is designed to survive — and a test that expected a
    // refusal there would be asserting that the redundancy does not work.
    use thalyx_btrfs::layout::{Geometry, Plan};
    let plan = Plan::new(Geometry::default());
    let mut image = std::fs::OpenOptions::new()
        .write(true)
        .open(file.path())
        .expect("reopening the image");
    for stripe in &plan.metadata().stripes {
        use std::io::{Seek, SeekFrom, Write};
        image
            .seek(SeekFrom::Start(stripe.0 + 200))
            .expect("seeking into the root tree's block");
        image.write_all(&[0xFF; 16]).expect("damaging the block");
    }
    drop(image);

    let (ok, said) = check(file.path());
    assert!(
        !ok,
        "btrfs check accepted a filesystem with sixteen bytes overwritten in \
         both copies of its root tree, so it is not checking anything:\n{said}"
    );
}

#[test]
fn the_smallest_device_this_will_format_produces_a_filesystem_that_checks_out() {
    // The geometry is fixed for every device, so the smallest one is where an
    // arithmetic error surfaces: a chunk running past the end, or a superblock
    // mirror written outside the device.
    if !btrfs_progs_available() {
        return;
    }
    let file = device(thalyx_btrfs::layout::MINIMUM_DEVICE);
    thalyx_btrfs::write(file.path(), thalyx_btrfs::LABEL, &uuids(), 0)
        .expect("writing a store onto the smallest permitted device");

    let (ok, said) = check(file.path());
    assert!(ok, "btrfs check refused the smallest store:\n{said}");
    assert!(said.contains("no error found"), "{said}");
}

#[test]
fn btrfs_progs_reports_the_label_thalyx_wrote() {
    // Read back by something other than this crate. `superblock::identify` and
    // `format::write` share the offset constants, so the two of them agreeing
    // would also be satisfied by both being wrong in the same place — which is
    // the failure `store_disk.rs` would then inherit as a store nothing can find.
    if !btrfs_progs_available() {
        return;
    }
    let file = device(thalyx_btrfs::layout::MINIMUM_DEVICE);
    thalyx_btrfs::write(file.path(), thalyx_btrfs::LABEL, &uuids(), 0).expect("writing a store");

    let output = Command::new("btrfs")
        .args(["inspect-internal", "dump-super"])
        .arg(file.path())
        .output()
        .expect("btrfs is on the PATH");
    let said = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "dump-super failed:\n{said}");
    assert!(
        said.contains(&format!("label\t\t\t{}", thalyx_btrfs::LABEL)),
        "btrfs-progs does not see the label Thalyx wrote:\n{said}"
    );
    assert!(
        said.contains("[match]"),
        "btrfs-progs does not agree about the superblock's checksum:\n{said}"
    );
}

#[test]
fn the_backup_superblock_is_a_superblock_and_not_a_copy_of_the_first_ones_address() {
    // Every mirror records its own offset and is checksummed after that. A mirror
    // carrying the primary's `bytenr` is refused by the kernel as a superblock
    // found somewhere it does not claim to be — and it is the exact mistake that
    // writing the block once and copying it produces.
    if !btrfs_progs_available() {
        return;
    }
    let file = device(8 * 1024 * 1024 * 1024);
    let written = thalyx_btrfs::write(file.path(), thalyx_btrfs::LABEL, &uuids(), 0)
        .expect("writing a store");
    assert_eq!(
        written.superblocks, 2,
        "an 8 GiB device holds the first two mirrors and not the third, which is \
         at 256 GiB"
    );

    let output = Command::new("btrfs")
        .args(["inspect-internal", "dump-super", "-s", "1"])
        .arg(file.path())
        .output()
        .expect("btrfs is on the PATH");
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "reading the backup superblock:\n{said}"
    );
    assert!(
        said.contains("bytenr\t\t\t67108864"),
        "the backup superblock does not say it is at 64 MiB:\n{said}"
    );
    assert!(
        said.contains("[match]"),
        "the backup superblock's checksum does not verify:\n{said}"
    );

    // And that the kernel could recover from it: `--super 1` makes btrfs check
    // read the filesystem starting from the backup rather than the primary.
    let output = Command::new("btrfs")
        .args(["check", "--super", "1"])
        .arg(file.path())
        .output()
        .expect("btrfs is on the PATH");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && said.contains("no error found"),
        "the filesystem cannot be read from its backup superblock, which is the \
         one thing a backup superblock is for:\n{said}"
    );
}

#[test]
fn a_device_too_small_for_a_backup_superblock_is_refused_and_says_why() {
    // No btrfs-progs needed: the refusal happens before anything is written.
    let file = device(32 * 1024 * 1024);
    let error = thalyx_btrfs::write(file.path(), thalyx_btrfs::LABEL, &uuids(), 0)
        .expect_err("a 32 MiB device is below the minimum");
    let said = error.to_string();
    assert!(
        said.contains("64 MiB"),
        "the refusal does not say that the limit is the backup superblock, so it \
         reads as an arbitrary size rule: {said}"
    );

    // And nothing was written. A refusal that had already put a superblock down
    // would leave a device that something might try to mount.
    match thalyx_btrfs::identify(file.path()) {
        Ok(thalyx_btrfs::Identity::NotBtrfs) => {}
        other => panic!("a refused format left something behind: {other:?}"),
    }
}

#[test]
fn what_thalyx_wrote_is_what_thalyx_reads_back() {
    // The round trip that `store_disk.rs` will depend on: a store is found by
    // asking each device what it is called.
    let file = device(thalyx_btrfs::layout::MINIMUM_DEVICE);
    let uuids = uuids();
    thalyx_btrfs::write(file.path(), thalyx_btrfs::LABEL, &uuids, 0).expect("writing a store");

    assert_eq!(
        thalyx_btrfs::identify(file.path()).expect("reading the superblock back"),
        thalyx_btrfs::Identity::Btrfs {
            label: thalyx_btrfs::LABEL.to_string(),
            fsid: uuids.fsid,
        }
    );
}

#[test]
fn a_device_that_was_never_formatted_reports_no_filesystem_rather_than_no_label() {
    // The baseline for the round trip above, and the distinction the whole label
    // scheme rests on. Without it, "this disk is not ours" and "this disk is ours
    // and has no name" would be the same answer, and rule 10 says they may not be.
    let file = device(thalyx_btrfs::layout::MINIMUM_DEVICE);
    assert_eq!(
        thalyx_btrfs::identify(file.path()).expect("reading a blank device"),
        thalyx_btrfs::Identity::NotBtrfs
    );
}
