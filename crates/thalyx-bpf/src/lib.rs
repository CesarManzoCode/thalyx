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
