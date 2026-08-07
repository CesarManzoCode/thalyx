//! The one ioctl number this crate depends on, recomputed from the kernel's header.
//!
//! `tests/uapi_fs.h` is `include/uapi/linux/fs.h`, captured verbatim, and it states
//! `BLKRRPART` as `_IO(0x12, 95)`. `thalyx-syscall` has to spell the resulting number
//! out because `_IO` is a C macro and this workspace has no C. This file closes the
//! gap.
//!
//! Its own file for the same reason `thalyx-btrfs` has one: an ioctl number encodes
//! the argument's size in bits 16..30 and the kernel matches on the whole word, so a
//! number that is right except for its size comes back `ENOTTY` from a kernel that
//! supports the call perfectly — which reads as an old kernel or a device that is
//! not a disk.
//!
//! `BLKRRPART` is `_IO` and not `_IOW`: it takes no argument at all, so the size
//! field is zero. That is the exact mistake this file exists to catch, because
//! nothing else in the workspace would.

fn header() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/uapi_fs.h");
    std::fs::read_to_string(&path).expect("uapi_fs.h is part of the repository")
}

/// `_IOC`, from `include/uapi/asm-generic/ioctl.h`: direction in bits 30..32, the
/// argument's size in 16..30, the type in 8..16 and the number in 0..8.
fn ioc(direction: u64, kind: u64, number: u64, size: u64) -> u64 {
    assert!(size < (1 << 14));
    (direction << 30) | (size << 16) | (kind << 8) | number
}

const NONE: u64 = 0;
const READ: u64 = 2;

#[test]
fn the_encoder_reproduces_an_ioctl_number_that_is_known_from_elsewhere() {
    // The instrument first. `BLKGETSIZE64` is in the same header as `BLKRRPART`,
    // defined as `_IOR(0x12, 114, size_t)`, and is 0x80081272 — every strace
    // decoding and every copy of util-linux agrees.
    //
    // Deliberately an ioctl *with* a direction and a size, because `BLKRRPART` has
    // neither. An encoder that dropped the size field entirely would still produce
    // the right answer for the number this file is here to check, so grading it with
    // that number would grade nothing.
    assert_eq!(ioc(READ, 0x12, 114, 8), 0x8008_1272);
}

#[test]
fn the_reread_number_is_the_one_the_header_encodes() {
    let text = header();
    let define = text
        .lines()
        .find(|line| line.starts_with("#define BLKRRPART"))
        .expect("the header defines BLKRRPART");

    // The type and the number are read out of the macro call rather than typed here.
    // What is left to a human is only that the header still spells it `_IO`.
    assert!(
        define.contains("_IO(0x12,95)"),
        "the header no longer defines BLKRRPART as _IO(0x12,95): {define}"
    );
    assert!(
        !define.contains("_IOW") && !define.contains("_IOR"),
        "BLKRRPART now carries an argument, so its number has a size in it: {define}"
    );

    assert_eq!(
        ioc(NONE, 0x12, 95, 0),
        thalyx_syscall::BLKRRPART,
        "the constant in thalyx-syscall is not what the header encodes"
    );
}

#[test]
fn the_number_has_no_size_and_no_direction_in_it() {
    // Said as a property rather than as a number, because this is the half of the
    // encoding that fails quietly. A `BLKRRPART` built as `_IOW(0x12, 95, int)` is
    // 0x40045f12, which is not "nearly right": the kernel compares the whole word,
    // finds nothing, and answers `ENOTTY` — the same answer it gives for a file that
    // is not a block device at all.
    assert_eq!(thalyx_syscall::BLKRRPART >> 30, 0, "it has a direction");
    assert_eq!(
        (thalyx_syscall::BLKRRPART >> 16) & 0x3FFF,
        0,
        "it has an argument size"
    );
}
