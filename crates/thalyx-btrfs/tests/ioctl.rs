//! The one ioctl number this crate depends on, recomputed from the kernel's header.
//!
//! `tests/uapi_btrfs.h` is `include/uapi/linux/btrfs.h`, captured verbatim, and it
//! states `BTRFS_IOC_SUBVOL_CREATE` as `_IOW(BTRFS_IOCTL_MAGIC, 14, struct
//! btrfs_ioctl_vol_args)`. `thalyx-syscall` has to spell the resulting number out,
//! because `_IOW` is a C macro and that crate contains no C. This file closes the
//! gap: it takes the three inputs out of the header's own text, applies `_IOW`, and
//! compares.
//!
//! Worth its own file because of how this failure looks. An ioctl number encodes
//! the argument's size in bits 16..30, and the kernel matches on the whole word —
//! so a wrong size gives `ENOTTY` from a filesystem that supports the call
//! perfectly, which reads as "this kernel is too old" or "this is not Btrfs". Rule
//! 5 of `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md`: the instrument would
//! have been blamed last.

/// `include/uapi/linux/btrfs.h`, as shipped.
fn header() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/uapi_btrfs.h");
    std::fs::read_to_string(&path).expect("uapi_btrfs.h is part of the repository")
}

/// A `#define NAME <integer>` the header states, in whatever base it states it.
fn constant(text: &str, name: &str) -> u64 {
    for line in text.lines() {
        let Some(rest) = line.strip_prefix(&format!("#define {name} ")) else {
            continue;
        };
        let value = rest.split_whitespace().next().unwrap_or_default();
        let parsed = match value.strip_prefix("0x") {
            Some(digits) => u64::from_str_radix(digits, 16),
            None => value.parse::<u64>(),
        };
        if let Ok(number) = parsed {
            return number;
        }
    }
    panic!("the header does not define {name} as an integer");
}

/// `_IOC` from `include/uapi/asm-generic/ioctl.h`: direction in bits 30..32, the
/// argument's size in 16..30, the type in 8..16 and the number in 0..8.
///
/// Written out here for the same reason `thalyx-syscall` writes its answer out:
/// there is no C in this workspace to ask.
fn ioc(direction: u64, kind: u64, number: u64, size: u64) -> u64 {
    assert!(
        size < (1 << 14),
        "an ioctl argument of {size} bytes does not fit in the size field"
    );
    (direction << 30) | (size << 16) | (kind << 8) | number
}

const READ: u64 = 2;
const WRITE: u64 = 1;

#[test]
fn the_encoder_reproduces_an_ioctl_number_that_is_known_from_elsewhere() {
    // The instrument first, graded against a number it did not produce.
    // `BTRFS_IOC_SUBVOL_GETFLAGS` is `_IOR(0x94, 25, __u64)` and is 0x8008_9419 —
    // btrfs-progs' sources and every strace decoding of it agree.
    //
    // Deliberately a *read* ioctl and not a write one. An encoder with the
    // direction constant wired into the wrong place still reproduces every `_IOW`
    // number, so a control written with `_IOW` would grade nothing.
    assert_eq!(
        ioc(READ, 0x94, 25, 8),
        0x8008_9419,
        "the ioctl encoding in this file is wrong, so nothing below it means anything"
    );
}

#[test]
fn the_subvolume_create_number_is_the_one_the_header_encodes() {
    let text = header();

    let magic = constant(&text, "BTRFS_IOCTL_MAGIC");
    assert_eq!(magic, 0x94, "the header's BTRFS_IOCTL_MAGIC is not 0x94");

    // `struct btrfs_ioctl_vol_args { __s64 fd; char name[BTRFS_PATH_NAME_MAX + 1]; }`
    // — eight bytes plus the field, whose bound the header also states. Taken from
    // the header rather than written as 4096, because 4096 is exactly the kind of
    // round number that looks right while being a coincidence.
    let path_name_max = constant(&text, "BTRFS_PATH_NAME_MAX");
    let size = 8 + (path_name_max + 1);
    assert_eq!(
        size, 4096,
        "btrfs_ioctl_vol_args is {size} bytes by the header"
    );

    // The number in the `#define` is not parsed out of the macro call; the nr is
    // the one thing here that has to be read by a human and typed. It is 14, and
    // the assertion below is that the header still says so at that number.
    let define = text
        .lines()
        .find(|line| line.starts_with("#define BTRFS_IOC_SUBVOL_CREATE "))
        .expect("the header defines BTRFS_IOC_SUBVOL_CREATE");
    assert!(
        define.contains("_IOW(BTRFS_IOCTL_MAGIC, 14,"),
        "the header no longer defines BTRFS_IOC_SUBVOL_CREATE as _IOW(magic, 14, ...): {define}"
    );

    assert_eq!(
        ioc(WRITE, magic, 14, size),
        thalyx_syscall::BTRFS_IOC_SUBVOL_CREATE,
        "the constant in thalyx-syscall is not what the header encodes"
    );
}

#[test]
fn the_name_limit_is_the_one_the_kernel_enforces_and_not_the_field_size() {
    // Two numbers in the same header, 255 and 4088, and the ioctl accepts the
    // smaller one. Using the field's size would produce names the kernel rejects
    // with a bare EINVAL from inside a call that looks correct.
    let text = header();
    assert_eq!(
        constant(&text, "BTRFS_VOL_NAME_MAX") as usize,
        thalyx_syscall::BTRFS_VOL_NAME_MAX
    );

    // And that it really is the smaller of the two, with the larger one also
    // taken from the header rather than written here — the mistake being guarded
    // against is picking up whichever number the eye landed on first.
    let field = constant(&text, "BTRFS_PATH_NAME_MAX") as usize + 1;
    assert!(
        thalyx_syscall::BTRFS_VOL_NAME_MAX < field,
        "the limit ({}) is not smaller than the {field}-byte field it goes in, \
         so the two have been confused",
        thalyx_syscall::BTRFS_VOL_NAME_MAX
    );
}

#[test]
fn a_name_that_cannot_be_one_directory_entry_is_refused_before_the_kernel_sees_it() {
    // Runs on any filesystem, because none of these reach the ioctl — which is
    // the property being checked as much as the refusal itself. A name with a `/`
    // in it handed to the kernel gets `EINVAL`, and `EINVAL` from this ioctl is
    // also what a filesystem with no space left says; the caller cannot tell a
    // typo from a full disk.
    use std::os::fd::AsFd;
    let directory = std::fs::File::open(env!("CARGO_MANIFEST_DIR")).unwrap();

    let long = "x".repeat(thalyx_syscall::BTRFS_VOL_NAME_MAX + 1);
    for name in ["", ".", "..", "system/data", "with\0a\0nul", long.as_str()] {
        let Err(error) = thalyx_syscall::btrfs_subvolume_create(directory.as_fd(), name) else {
            panic!("`{name}` was accepted as a subvolume name");
        };
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::InvalidInput,
            "`{name}` came back as {error:?} rather than as a name that cannot exist"
        );
    }

    // And the boundary from the other side, so the length check is not simply
    // refusing everything: exactly the maximum is a name the kernel may have.
    // Nothing is created — this directory is not Btrfs, and if it ever is, the
    // error is the interesting outcome rather than the refusal.
    let longest = "x".repeat(thalyx_syscall::BTRFS_VOL_NAME_MAX);
    if let Err(error) = thalyx_syscall::btrfs_subvolume_create(directory.as_fd(), &longest) {
        assert_ne!(
            error.kind(),
            std::io::ErrorKind::InvalidInput,
            "a name of exactly {} bytes was refused as unrepresentable",
            thalyx_syscall::BTRFS_VOL_NAME_MAX
        );
    }
}

#[test]
fn the_three_decreed_names_all_fit_and_are_single_names() {
    // Cheap, and it is the check that would have caught a decree revision that
    // added a subvolume called `system/data`, which is two names and would create
    // nothing while looking like it had.
    for name in thalyx_btrfs::DECREED {
        assert!(!name.is_empty());
        assert!(name.len() <= thalyx_syscall::BTRFS_VOL_NAME_MAX);
        assert!(!name.contains('/'), "`{name}` is not a single name");
    }
}
