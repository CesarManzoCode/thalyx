//! Writing a Btrfs filesystem, without `mkfs.btrfs` and without libbtrfs.
//!
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md`: the installed machine has
//! to create the disk it keeps everything on, and
//! `vault/01-Filosofia/Filosofia-Fundacional.md` says the image carries the Linux
//! kernel and one program. So `mkfs.btrfs` cannot be on it, and cannot be added:
//! the same shape as `bpftool` for the LSM, the same shape as `cpio` for the
//! initramfs, and the same answer — Thalyx writes the bytes itself. Decided by
//! Cesar, recorded in that note under *¿Quién crea el store?*.
//!
//! ## What this is not
//!
//! It is not a Btrfs implementation. It writes one filesystem, empty, with the
//! geometry in [`layout`], and it reads a superblock back to answer *what is this
//! device called*. It cannot allocate, cannot balance, cannot grow a tree past one
//! leaf, and refuses rather than trying — see [`leaf::LeafError`]. Once a store
//! exists, everything that happens to it happens through the kernel's Btrfs, which
//! is the only code here that was written by people who do this for a living.
//!
//! It performs no system calls beyond `open`, `seek`, `write` and `fsync` through
//! `std::fs`, so all of it can be exercised on a machine with no Btrfs support
//! whatsoever — which is every machine this project develops on.
//!
//! ## Who may call it
//!
//! Not PID 1. `crates/thalyx-cli/src/store_disk.rs` records the reason and it has
//! not changed: a machine that fabricates a store when it cannot find the old one
//! boots looking perfect on the day the disk was not attached, and the human finds
//! out by noticing that everything they installed is gone. Creating a store is an
//! explicit human act, which in practice means the installer.
//!
//! ## How any of this is known to be right
//!
//! Two instruments, and neither is a reading of the format.
//!
//! `tests/uapi_header.h` is `include/uapi/linux/btrfs_tree.h`, captured verbatim.
//! `tests/layout.rs` parses it and checks every structure size and every offset
//! this crate writes against it — rule 6 of
//! `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md`, applied to a writer instead
//! of a parser.
//!
//! `tests/against_btrfs_progs.rs` writes a filesystem and hands it to `btrfs
//! check`, which validates the trees, the back references, the block group
//! accounting and the chunk map — nearly everything a mount validates, without a
//! kernel that can mount. It **skips when btrfs-progs is absent and says
//! `NOT PROVEN`**, and `THALYX_REQUIRE_BTRFS_PROGS=1` turns the skip into a
//! failure. That is one variable for one requirement, per rule 3.
//!
//! What neither instrument establishes is a mount. `btrfs check` reads the
//! filesystem with btrfs-progs' own code, not the kernel's, and the two have
//! disagreed before. **Only Cesar's machine can mount this**, and until it has,
//! nothing here should be described as working.

pub mod crc32c;
pub mod disk;
pub mod format;
pub mod layout;
pub mod leaf;
pub mod superblock;

pub use format::{FormatError, LABEL, Uuids, Written, write};
pub use superblock::{Identity, ReadError, identify};
