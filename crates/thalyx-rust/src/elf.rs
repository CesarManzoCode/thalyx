//! Just enough ELF to ask a program what it will need before it is asked to
//! run.
//!
//! ## Why this is here and not `ldd`
//!
//! `ldd` answers the question *on the machine running it*: it starts the real
//! loader against the real `/lib`, so it says what a Fedora would resolve, not
//! what a Thalyx will. The question this crate has is the opposite one — does
//! the **artifact** carry everything its own programs ask for, on a machine
//! that has nothing else at all. Reading the headers answers that without a
//! machine to answer it on, which is also what makes it testable in a
//! container.
//!
//! The reader is small on purpose: `PT_INTERP`, and `DT_NEEDED` out of
//! `PT_DYNAMIC`. Nothing else about ELF is any of this crate's business.
//! `thalyx-bpf` has its own reader for the *relocatable* objects clang emits;
//! this one reads executables and shared objects, and they overlap in nothing
//! but the magic number.

use std::path::Path;

/// What a program says it will need.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Needs {
    /// The dynamic loader, from `PT_INTERP`. `None` for a static binary and
    /// for a shared object, which are different things and are both legal.
    pub interpreter: Option<String>,
    /// Every `DT_NEEDED`, in the order the header lists them.
    pub libraries: Vec<String>,
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// A NUL-terminated string at an offset, without running off the end.
fn string_at(bytes: &[u8], at: usize) -> Option<String> {
    let rest = bytes.get(at..)?;
    let end = rest.iter().position(|byte| *byte == 0)?;
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;

/// What one file will ask the loader for.
///
/// `None` when the bytes are not an ELF64 little-endian file this can read.
/// Not an error type: every caller's next move is the same — a file that is
/// not an ELF is not a program whose libraries anybody has to find.
pub fn needs(bytes: &[u8]) -> Option<Needs> {
    if bytes.len() < 64 || &bytes[..4] != b"\x7fELF" || bytes[4] != 2 || bytes[5] != 1 {
        return None;
    }
    let phoff = u64_at(bytes, 0x20)? as usize;
    let phentsize = u16_at(bytes, 0x36)? as usize;
    let phnum = u16_at(bytes, 0x38)? as usize;

    let mut needs = Needs::default();
    let mut loads: Vec<(u64, u64, u64)> = Vec::new();
    let mut dynamic: Option<(usize, usize)> = None;

    for index in 0..phnum {
        let at = phoff.checked_add(index.checked_mul(phentsize)?)?;
        let kind = u32_at(bytes, at)?;
        let offset = u64_at(bytes, at + 0x08)?;
        let vaddr = u64_at(bytes, at + 0x10)?;
        let filesz = u64_at(bytes, at + 0x20)?;
        match kind {
            PT_LOAD => loads.push((vaddr, offset, filesz)),
            PT_INTERP => needs.interpreter = string_at(bytes, offset as usize),
            PT_DYNAMIC => dynamic = Some((offset as usize, filesz as usize)),
            _ => {}
        }
    }

    // A virtual address is not a file offset. Every string the dynamic section
    // names lives at a *vaddr*, and the only thing that translates one is the
    // load map — which is why this walks `PT_LOAD` rather than assuming the
    // two coincide. They do coincide in most binaries, and a reader that
    // assumed it would be right until the first file where they do not.
    let file_offset = |vaddr: u64| -> Option<usize> {
        loads
            .iter()
            .find(|(base, _, size)| vaddr >= *base && vaddr < base + size)
            .map(|(base, offset, _)| (offset + (vaddr - base)) as usize)
    };

    if let Some((offset, size)) = dynamic {
        let mut strtab = None;
        let mut wanted: Vec<u64> = Vec::new();
        let mut at = offset;
        while at + 16 <= offset + size {
            let tag = u64_at(bytes, at)?;
            let value = u64_at(bytes, at + 8)?;
            match tag {
                DT_NULL => break,
                DT_NEEDED => wanted.push(value),
                DT_STRTAB => strtab = Some(value),
                _ => {}
            }
            at += 16;
        }
        if let Some(strtab) = strtab.and_then(file_offset) {
            for offset in wanted {
                if let Some(name) = string_at(bytes, strtab + offset as usize) {
                    needs.libraries.push(name);
                }
            }
        }
    }
    Some(needs)
}

/// The same, for a file on disk.
pub fn needs_of(path: &Path) -> Option<Needs> {
    needs(&std::fs::read(path).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_is_not_an_elf_is_not_read_as_one() {
        assert_eq!(needs(b"#!/bin/sh\necho hello\n"), None);
        assert_eq!(needs(&[]), None);
        // Long enough not to be rejected on length, and still not an ELF.
        assert_eq!(needs(&[0u8; 128]), None);
    }

    #[test]
    fn a_real_binary_names_its_loader_and_its_libraries() {
        // Rule 6 of `Estrategia-de-Pruebas.md`, applied one level down: a
        // hand-written ELF would prove this reader matches somebody's model of
        // the format. The one real ELF every test run is guaranteed to have is
        // the test binary itself.
        let me = std::env::current_exe().expect("this test's own binary");
        let needs = needs_of(&me).expect("the test binary is an ELF64");
        // Statically linked test binaries exist — this asserts the reader got
        // a coherent answer, not that the host links dynamically.
        if let Some(interpreter) = &needs.interpreter {
            assert!(interpreter.starts_with('/'), "{needs:?}");
            assert!(!needs.libraries.is_empty(), "{needs:?}");
            assert!(
                needs.libraries.iter().any(|name| name.contains("libc")),
                "{needs:?}"
            );
        }
        for name in &needs.libraries {
            assert!(!name.is_empty(), "{needs:?}");
        }
    }
}
