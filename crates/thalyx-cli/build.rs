//! Put the BPF object inside the binary, or say plainly that there is none.
//!
//! `vault/01-Filosofia/Filosofia-Fundacional.md` says the image holds the Linux
//! kernel and one program, and `make -C image count` is the number that proves
//! it. The old loader looked for `/lib/thalyx/thalyx_lsm.bpf.o` — a second file
//! — and the boot message suggesting somebody put it there was suggesting the
//! decree be broken. So the object travels inside the one program.
//!
//! ## Why this does not compile it
//!
//! Building the object needs clang, and `vmlinux.h`, which needs bpftool and a
//! kernel that publishes BTF. Making `cargo build` depend on all three would
//! mean this workspace stops building on any machine that lacks one of them,
//! for a crate most of which has nothing to do with BPF.
//!
//! So `make -C lsm` builds it and this picks it up. When it is not there the
//! binary is built without it and **says so at boot** rather than pretending —
//! the same distinction `thalyx session` draws everywhere else between a thing
//! that is absent and a thing that could not be checked.

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let object = root.join("lsm/thalyx_lsm.bpf.o");

    // Rebuild when the object appears, changes, or is removed. Without this a
    // binary built before `make -C lsm` would keep reporting no enforcement
    // after the object existed, and the obvious conclusion would be that the
    // loader is broken.
    println!("cargo:rerun-if-changed={}", object.display());
    println!("cargo:rerun-if-changed=build.rs");

    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let generated = if object.is_file() {
        format!(
            "pub const OBJECT: Option<&[u8]> = Some(include_bytes!(r\"{}\"));\n\
             pub const ORIGIN: &str = r\"{}\";\n",
            object.display(),
            object.display()
        )
    } else {
        // Named as a distinct state rather than an empty slice. A zero-length
        // object would parse as a broken ELF and report a corrupt file, which
        // is a different problem with a different fix.
        format!(
            "pub const OBJECT: Option<&[u8]> = None;\n\
             pub const ORIGIN: &str = r\"{}\";\n",
            object.display()
        )
    };

    std::fs::write(out.join("lsm_object.rs"), generated).expect("writing the generated object");
}
