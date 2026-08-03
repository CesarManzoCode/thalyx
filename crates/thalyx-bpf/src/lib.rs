//! Loading a BPF object without libbpf and without bpftool.
//!
//! `vault/09-Notas-Tecnicas/Construccion-del-ISO.md` decrees that the image
//! carries the Linux kernel and one program. `thalyx-lsm` is what makes
//! permissions real, and until now the only way to attach it was to invoke
//! `bpftool` — a second program, which the image does not have and cannot have,
//! from a shell, which it also does not have. The machine boots and says so.
//!
//! So the loading happens here, in Thalyx, and the object travels inside the
//! binary rather than beside it.
//!
//! ## What this crate is and is not
//!
//! It is a reader and a relocator: ELF in, a description of maps and programs
//! out, with every offset the kernel needs already patched. It performs no
//! system calls at all — those live in `thalyx-syscall`, which is the one crate
//! allowed `unsafe`. That split is what lets almost all of this be exercised on
//! a machine with no BPF whatsoever, which is most of them.
//!
//! It is not libbpf. It reads the one object this project compiles, refuses
//! anything it does not recognise, and says which part it refused. A general
//! loader is written to accept every object there is; rule 9 wants the opposite
//! from something about to put a program in the kernel.

pub mod btf;
pub mod core;
pub mod elf;
pub mod loader;
pub mod maps;
pub mod program;

pub use btf::{Btf, BtfError};
pub use core::CoreError;
pub use elf::{Elf, ElfError};
pub use loader::{LoadError, Loaded, kernel_btf, load};
pub use maps::{MapError, MapSpec};
pub use program::{ProgramError, ProgramSpec};

#[cfg(test)]
mod uapi {
    /// The uapi header as it really is, captured rather than remembered.
    /// See `tests/captured/bpf-uapi-enums.h`.
    const HEADER: &str = include_str!("../tests/captured/bpf-uapi-enums.h");

    /// The position of a name in a C enum whose entries have no explicit
    /// values, which is what its number is.
    fn value_of(enumeration: &str, wanted: &str) -> u32 {
        let start = HEADER
            .find(&format!("enum {enumeration} {{"))
            .unwrap_or_else(|| panic!("no `enum {enumeration}` in the captured header"));
        let body = &HEADER[start..];
        let end = body.find("};").expect("the enum is closed");

        let mut index = 0u32;
        for line in body[..end].lines().skip(1) {
            let line = line.trim();
            // Comment and blank lines are not entries, and a `__MAX_` sentinel
            // is one but is never asked for.
            if line.is_empty() || line.starts_with("/*") || line.starts_with('*') {
                continue;
            }
            let name = line.trim_end_matches(',');
            assert!(
                !name.contains('='),
                "`{name}` has an explicit value, so counting positions is no longer sound"
            );
            if name == wanted {
                return index;
            }
            index += 1;
        }
        panic!("`{wanted}` is not in `enum {enumeration}`");
    }

    #[test]
    fn the_attach_type_is_the_lsm_one_and_not_the_entry_before_it() {
        // This is the bug that cost a hardware run. `BPF_MODIFY_RETURN` sits
        // immediately before `BPF_LSM_MAC`, and using it made the kernel run
        // the modify-return check against an LSM hook. Both are asserted, so
        // that an off-by-one in either direction is caught here rather than by
        // a verifier message on somebody's machine.
        assert_eq!(value_of("bpf_attach_type", "BPF_MODIFY_RETURN"), 26);
        assert_eq!(
            value_of("bpf_attach_type", "BPF_LSM_MAC"),
            thalyx_syscall::BPF_LSM_MAC
        );
    }

    #[test]
    fn the_program_type_is_the_lsm_one() {
        assert_eq!(
            value_of("bpf_prog_type", "BPF_PROG_TYPE_LSM"),
            thalyx_syscall::BPF_PROG_TYPE_LSM
        );
    }

    #[test]
    fn the_captured_header_is_the_real_one_and_not_a_summary() {
        // If somebody trims this file down to "the interesting bits", the
        // positions stop being the numbers and every assertion above becomes a
        // tautology about a list that agrees with itself.
        assert!(
            value_of("bpf_prog_type", "BPF_PROG_TYPE_UNSPEC") == 0,
            "the enum does not start where a C enum starts"
        );
        assert!(
            HEADER.lines().count() > 80,
            "the captured header has been shortened, and the counting is no longer sound"
        );
    }
}
