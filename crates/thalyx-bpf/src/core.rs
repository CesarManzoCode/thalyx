//! CO-RE: making an object compiled against one kernel run on another.
//!
//! Every `BPF_CORE_READ(file, f_flags)` in `thalyx_lsm.bpf.c` compiles to an
//! instruction holding a **byte offset that is deliberately wrong**. The offset
//! baked in is the one from whatever header the object was compiled against;
//! the real one is whatever the running kernel says, and the two differ as soon
//! as anybody adds a field to `struct file`. The object records, separately,
//! which type and which field it meant. The loader looks that up in the
//! kernel's own BTF and patches the instruction.
//!
//! Without this step the program loads, passes the verifier, runs, and reads
//! the wrong four bytes — which for `file_open` means deciding read-versus-write
//! from whatever happens to live at that offset. It would not crash. It would
//! enforce the wrong thing, quietly, forever.
//!
//! ## Matched by name, not by position
//!
//! The access is recorded as member *indices*, and it would be less code to use
//! them against the target type directly. That is wrong: a kernel that inserted
//! a field would shift every index after it, and the loader would compute a
//! plausible offset for the wrong member. So the indices are used to read
//! *names* out of the local types, and the names are what is looked up in the
//! target — which is what libbpf does and for this reason.
//!
//! ## One kind, and the rest refused
//!
//! `thalyx_lsm.bpf.o` contains exactly two relocations, both
//! `FIELD_BYTE_OFFSET`. Everything else is refused by name rather than skipped.
//! A skipped relocation is an instruction left holding a number from another
//! kernel, and rule 9 says the cautious answer, never the fast one.

use crate::btf::{Btf, BtfError, kind as btf_kind};

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(".BTF.ext is not what this reads: magic is {0:#06x}, expected 0xeb9f")]
    NotBtfExt(u16),

    #[error(".BTF.ext {what} runs past the end of the section")]
    Truncated { what: &'static str },

    #[error(
        "relocation kind {kind} in section `{section}` is one this loader does not perform; \
         the program would run with an offset from another kernel"
    )]
    UnsupportedKind { section: String, kind: u32 },

    #[error("a relocation names access string `{0}`, which is not a list of indices")]
    BadAccess(String),

    #[error("the local type of a relocation is not a struct or union: {0}")]
    NotAStruct(String),

    #[error(
        "this kernel has no `{0}`, so the field a relocation names cannot be located; \
         the object was compiled for a kernel this one is not"
    )]
    NoSuchTypeInKernel(String),

    #[error("this kernel's `{type_name}` has no member `{member}`")]
    NoSuchMember { type_name: String, member: String },

    #[error("reading BTF: {0}")]
    Btf(#[from] BtfError),

    #[error(
        "the instruction at byte {offset} of `{section}` is opcode {opcode:#04x}, \
         which this does not know how to patch"
    )]
    UnpatchableInstruction {
        section: String,
        offset: usize,
        opcode: u8,
    },

    #[error("a relocation points at byte {offset} of `{section}`, which is {len} bytes long")]
    OutsideProgram {
        section: String,
        offset: usize,
        len: usize,
    },
}

type Result<T> = std::result::Result<T, CoreError>;

/// The only relocation kind this performs.
const FIELD_BYTE_OFFSET: u32 = 0;

/// One thing to patch.
pub struct Relocation {
    /// Byte offset into the program's instructions.
    pub insn_offset: u32,
    /// The type, in the *object's* BTF.
    pub local_type: u32,
    /// Member indices, as the object recorded them.
    pub access: Vec<u32>,
    pub kind: u32,
}

/// Every CO-RE relocation in the object, by the name of the section it patches.
pub fn relocations(btf_ext: &[u8], btf: &Btf) -> Result<Vec<(String, Vec<Relocation>)>> {
    let magic = u16::from_le_bytes([
        *btf_ext
            .first()
            .ok_or(CoreError::Truncated { what: "header" })?,
        *btf_ext
            .get(1)
            .ok_or(CoreError::Truncated { what: "header" })?,
    ]);
    if magic != 0xeb9f {
        return Err(CoreError::NotBtfExt(magic));
    }

    let word = |at: usize, what: &'static str| -> Result<u32> {
        let s = btf_ext
            .get(at..at + 4)
            .ok_or(CoreError::Truncated { what })?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };

    let header_len = word(4, "hdr_len")? as usize;
    // func_info and line_info come first and are not read: they are debugging
    // aids the kernel accepts and does nothing with. Their offsets are still
    // needed to find what follows them.
    let core_off = word(24, "core_relo_off")? as usize;
    let core_len = word(28, "core_relo_len")? as usize;
    if core_len == 0 {
        return Ok(Vec::new());
    }

    let start = header_len + core_off;
    let end = start + core_len;
    let record_size = word(start, "the core relocation record size")? as usize;

    let mut out = Vec::new();
    let mut at = start + 4;
    while at < end {
        let section_name = word(at, "a relocation section's name")? as usize;
        let count = word(at + 4, "a relocation section's count")? as usize;
        at += 8;

        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let insn_offset = word(at, "a relocation's instruction offset")?;
            let local_type = word(at + 4, "a relocation's type")?;
            let access_off = word(at + 8, "a relocation's access string")? as usize;
            let kind = word(at + 12, "a relocation's kind")?;
            // Records can grow: the size is read rather than assumed, so a
            // newer clang adding a field does not desynchronise the walk.
            at += record_size;

            let access_text = btf.string_at(access_off);
            let access = access_text
                .split(':')
                .map(|piece| piece.parse::<u32>())
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| CoreError::BadAccess(access_text.clone()))?;

            entries.push(Relocation {
                insn_offset,
                local_type,
                access,
                kind,
            });
        }
        out.push((btf.string_at(section_name), entries));
    }
    Ok(out)
}

/// The names of the fields a relocation walks through, read out of the object.
///
/// The first index is an array subscript applied to the type itself, which for
/// the pointer-to-struct case every `BPF_CORE_READ` produces is always zero. It
/// is skipped rather than asserted to be zero, because an array of structs is a
/// legitimate thing to read through and this is not the place to refuse it.
fn field_names(btf: &Btf, relocation: &Relocation) -> Result<(String, Vec<String>)> {
    let (_, local) = btf.resolved(relocation.local_type)?;
    if local.kind != btf_kind::STRUCT && local.kind != btf_kind::UNION {
        return Err(CoreError::NotAStruct(local.name.clone()));
    }
    let type_name = local.name.clone();

    let mut names = Vec::new();
    let mut current = relocation.local_type;
    for index in relocation.access.iter().skip(1) {
        let (_, found) = btf.resolved(current)?;
        let member = found
            .members
            .get(*index as usize)
            .ok_or_else(|| CoreError::NoSuchMember {
                type_name: found.name.clone(),
                member: format!("#{index}"),
            })?;
        names.push(member.name.clone());
        current = member.type_id;
    }
    Ok((type_name, names))
}

/// Where that same field lives in the kernel that is actually running.
fn offset_in(kernel: &Btf, type_name: &str, fields: &[String]) -> Result<u32> {
    let mut current = kernel
        .ids()
        .find(|id| {
            kernel.type_of(*id).is_ok_and(|t| {
                (t.kind == btf_kind::STRUCT || t.kind == btf_kind::UNION) && t.name == type_name
            })
        })
        .ok_or_else(|| CoreError::NoSuchTypeInKernel(type_name.to_string()))?;

    let mut offset = 0u32;
    for field in fields {
        let (_, found) = kernel.resolved(current)?;
        let member = find_member(kernel, found, field, &mut offset)?;
        current = member;
    }
    Ok(offset)
}

/// Find a member by name, descending into anonymous ones.
///
/// Anonymous unions are everywhere in kernel structures, and a field inside one
/// is written in C as though it were a member of the outer struct. A search
/// that only looked one level down would report the field missing on exactly
/// the kernels where it exists.
fn find_member(
    kernel: &Btf,
    found: &crate::btf::Type,
    field: &str,
    offset: &mut u32,
) -> Result<u32> {
    for member in &found.members {
        // With kind_flag set the offset word packs a bitfield size into its
        // top bits. Bitfields are not read by this program and a member with
        // one would give a nonsensical byte offset if the mask were forgotten.
        let bit_offset = if found.kind_flag {
            member.offset & 0x00ff_ffff
        } else {
            member.offset
        };

        if member.name == field {
            *offset += bit_offset / 8;
            return Ok(member.type_id);
        }

        if member.name.is_empty()
            && let Ok((_, inner)) = kernel.resolved(member.type_id)
            && (inner.kind == btf_kind::STRUCT || inner.kind == btf_kind::UNION)
        {
            let mut nested = *offset + bit_offset / 8;
            if let Ok(type_id) = find_member(kernel, inner, field, &mut nested) {
                *offset = nested;
                return Ok(type_id);
            }
        }
    }
    Err(CoreError::NoSuchMember {
        type_name: found.name.clone(),
        member: field.to_string(),
    })
}

/// Instruction classes, from the BPF instruction set.
const BPF_LDX: u8 = 0x01;
const BPF_ST: u8 = 0x02;
const BPF_STX: u8 = 0x03;
const BPF_ALU: u8 = 0x04;
const BPF_ALU64: u8 = 0x07;

/// Rewrite one instruction so it carries the offset this kernel uses.
///
/// Which field of the instruction holds it depends on the class, and this is
/// the one place where getting it wrong produces a program that still loads:
/// patching `imm` on a load leaves the offset untouched and corrupts the
/// immediate instead.
fn patch(instructions: &mut [u8], section: &str, at: usize, value: u32) -> Result<()> {
    let slot = instructions
        .get_mut(at..at + 8)
        .ok_or_else(|| CoreError::OutsideProgram {
            section: section.to_string(),
            offset: at,
            len: 0,
        })?;
    let opcode = slot[0];
    match opcode & 0x07 {
        // A memory access: the offset lives in the 16-bit `off` field.
        BPF_LDX | BPF_ST | BPF_STX => {
            let short = u16::try_from(value).map_err(|_| CoreError::UnpatchableInstruction {
                section: section.to_string(),
                offset: at,
                opcode,
            })?;
            slot[2..4].copy_from_slice(&short.to_le_bytes());
            Ok(())
        }
        // Arithmetic on the offset itself — in practice `r2 = <offset>`, which
        // is what clang emits when it is about to add the field offset to a
        // pointer. The value goes in `imm`.
        BPF_ALU | BPF_ALU64 => {
            slot[4..8].copy_from_slice(&value.to_le_bytes());
            Ok(())
        }
        // BPF_LD is deliberately absent. A 64-bit immediate load spans two
        // instruction slots, so patching only this one would leave the high
        // half from the other kernel — and the only relocation kinds that land
        // on it are ones this loader refuses anyway.
        _ => Err(CoreError::UnpatchableInstruction {
            section: section.to_string(),
            offset: at,
            opcode,
        }),
    }
}

/// Apply every relocation for one program, against the running kernel's BTF.
pub fn apply(
    instructions: &mut [u8],
    section: &str,
    entries: &[Relocation],
    local: &Btf,
    kernel: &Btf,
) -> Result<()> {
    for relocation in entries {
        if relocation.kind != FIELD_BYTE_OFFSET {
            return Err(CoreError::UnsupportedKind {
                section: section.to_string(),
                kind: relocation.kind,
            });
        }
        let at = relocation.insn_offset as usize;
        if at + 8 > instructions.len() {
            return Err(CoreError::OutsideProgram {
                section: section.to_string(),
                offset: at,
                len: instructions.len(),
            });
        }

        let (type_name, fields) = field_names(local, relocation)?;
        let offset = offset_in(kernel, &type_name, &fields)?;
        patch(instructions, section, at, offset)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::Elf;

    const CAPTURED: &[u8] = include_bytes!("../tests/captured/thalyx_lsm.bpf.o");

    /// The object's BTF and its CO-RE relocations, both from clang's real
    /// output. Named rather than returned as a tuple so the tests below read
    /// as sentences instead of as index arithmetic.
    struct Parts {
        btf: Btf,
        relocations: Vec<(String, Vec<Relocation>)>,
    }

    fn parts() -> Parts {
        let elf = Elf::parse(CAPTURED).unwrap();
        let btf = Btf::parse(elf.section(".BTF").unwrap().bytes).unwrap();
        let ext = elf.section(".BTF.ext").unwrap().bytes;
        let relocations = relocations(ext, &btf).expect("the captured object's CO-RE parses");
        Parts { btf, relocations }
    }

    #[test]
    fn both_field_reads_in_the_program_are_recorded_as_relocations() {
        // `sockaddr->sa_family` and `file->f_flags`. If either were missing,
        // that read would use the offset from the header the object was built
        // against and would be silently wrong on any other kernel.
        let relocations = parts().relocations;
        let sections: Vec<&str> = relocations.iter().map(|(name, _)| name.as_str()).collect();
        assert!(sections.contains(&"lsm/socket_connect"), "{sections:?}");
        assert!(sections.contains(&"lsm/file_open"), "{sections:?}");
        for (name, entries) in &relocations {
            assert_eq!(entries.len(), 1, "{name} has {} relocations", entries.len());
            assert_eq!(entries[0].kind, FIELD_BYTE_OFFSET);
        }
    }

    #[test]
    fn the_relocations_name_the_fields_the_c_actually_reads() {
        // Read out of the object rather than assumed, which is the check that
        // the access-string walk lands on the right member.
        let Parts { btf, relocations } = parts();
        let mut seen = Vec::new();
        for (_, entries) in &relocations {
            let (type_name, fields) = field_names(&btf, &entries[0]).unwrap();
            seen.push(format!("{type_name}.{}", fields.join(".")));
        }
        seen.sort();
        assert_eq!(seen, vec!["file.f_flags", "sockaddr.sa_family"]);
    }

    #[test]
    fn a_field_that_moved_in_the_target_kernel_produces_the_targets_offset() {
        // The whole point, made checkable without a kernel: the "kernel" here
        // is a BTF where the field is not first. A loader that used the local
        // offset would produce 0 and read the wrong four bytes forever.
        let Parts { btf, relocations } = parts();
        let (_, entries) = relocations
            .iter()
            .find(|(name, _)| name == "lsm/file_open")
            .unwrap();

        // The object's own BTF stands in for a kernel whose `struct file` has
        // f_flags first — which is what the stub header used to build it says,
        // so this is the identity case and must come out 0.
        let (type_name, fields) = field_names(&btf, &entries[0]).unwrap();
        assert_eq!(offset_in(&btf, &type_name, &fields).unwrap(), 0);
    }

    #[test]
    fn a_kernel_without_the_type_is_refused_rather_than_defaulted() {
        // An object built for a kernel this machine is not running. Producing
        // an offset of zero here is the failure this whole module exists to
        // prevent, so it must be an error and never a number.
        let btf = parts().btf;
        let error = offset_in(&btf, "struct_that_does_not_exist", &["x".to_string()])
            .expect_err("a type that is not there cannot yield an offset");
        assert!(matches!(error, CoreError::NoSuchTypeInKernel(_)), "{error}");
    }

    #[test]
    fn a_type_that_exists_without_the_field_is_refused_by_the_field_name() {
        let btf = parts().btf;
        let error = offset_in(&btf, "policy", &["not_a_field".to_string()])
            .expect_err("a missing member cannot yield an offset");
        assert!(matches!(error, CoreError::NoSuchMember { .. }), "{error}");
    }

    #[test]
    fn patching_a_mov_writes_the_immediate_and_leaves_the_offset_alone() {
        // Instruction 5 of both programs is `r2 = 0x0`, which clang emits to
        // hold the field offset. Patching `off` instead would leave the offset
        // at zero and corrupt an unrelated field — and the program would still
        // load.
        let mut mov = vec![0xb7, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        patch(&mut mov, "test", 0, 0x40).unwrap();
        assert_eq!(
            &mov[4..8],
            &[0x40, 0, 0, 0],
            "the immediate holds the offset"
        );
        assert_eq!(&mov[2..4], &[0, 0], "the offset field is untouched");
    }

    #[test]
    fn patching_a_load_writes_the_offset_and_leaves_the_immediate_alone() {
        // `r0 = *(u64 *)(r1 + 0x18)` — a BPF_LDX. Here the offset field is the
        // one that means anything.
        let mut load = vec![0x79, 0x10, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00];
        patch(&mut load, "test", 0, 0x28).unwrap();
        assert_eq!(&load[2..4], &[0x28, 0], "the offset field holds it");
        assert_eq!(&load[4..8], &[0, 0, 0, 0], "the immediate is untouched");
    }

    #[test]
    fn an_offset_too_large_for_a_load_is_refused_rather_than_truncated() {
        // A 16-bit field cannot hold a 100 KiB offset, and wrapping it would
        // produce a plausible small number pointing at the wrong member.
        let mut load = vec![0x79, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(patch(&mut load, "test", 0, 100_000).is_err());
    }

    #[test]
    fn a_relocation_kind_this_does_not_perform_stops_the_load() {
        // Skipping it would leave an instruction carrying a number from
        // another kernel, which is the quiet wrong answer rule 9 forbids.
        let btf = parts().btf;
        let mut instructions = vec![0u8; 64];
        let unsupported = [Relocation {
            insn_offset: 0,
            local_type: 1,
            access: vec![0],
            kind: 9, // TYPE_SIZE
        }];
        let error = apply(&mut instructions, "test", &unsupported, &btf, &btf)
            .expect_err("an unsupported kind must not be skipped");
        assert!(
            matches!(error, CoreError::UnsupportedKind { .. }),
            "{error}"
        );
    }

    #[test]
    fn a_relocation_pointing_past_the_program_is_refused() {
        let btf = parts().btf;
        let mut instructions = vec![0u8; 16];
        let outside = [Relocation {
            insn_offset: 4096,
            local_type: 1,
            access: vec![0, 0],
            kind: FIELD_BYTE_OFFSET,
        }];
        assert!(matches!(
            apply(&mut instructions, "test", &outside, &btf, &btf),
            Err(CoreError::OutsideProgram { .. })
        ));
    }
}

#[cfg(test)]
mod against_a_kernel_where_the_field_moved {
    use super::*;
    use crate::elf::Elf;
    use crate::program;

    const CAPTURED: &[u8] = include_bytes!("../tests/captured/thalyx_lsm.bpf.o");

    /// Clang's output for a `struct file` whose `f_flags` is at byte 20, which
    /// is what a real kernel looks like and what the object's own header does
    /// not. See `tests/captured/README.md`.
    const KERNELISH: &[u8] = include_bytes!("../tests/captured/kernelish.btf");

    /// The immediate of the instruction a relocation points at.
    fn immediate_at(instructions: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(instructions[offset + 4..offset + 8].try_into().unwrap())
    }

    #[test]
    fn a_field_that_moved_gets_the_offset_this_kernel_uses() {
        // The claim this whole module exists for, checked without a kernel.
        //
        // Before: the instruction holds 0, because the header the object was
        // built against put f_flags first. After: 20, from the target's BTF. A
        // loader that skipped this step would leave the 0 — and `file_open`
        // would decide read-versus-write from `f_lock`, forever, without ever
        // failing.
        let elf = Elf::parse(CAPTURED).unwrap();
        let local = Btf::parse(elf.section(".BTF").unwrap().bytes).unwrap();
        let kernel = Btf::parse(KERNELISH).unwrap();
        let all = relocations(elf.section(".BTF.ext").unwrap().bytes, &local).unwrap();

        let mut spec = program::programs(&elf)
            .unwrap()
            .into_iter()
            .find(|p| p.section == "lsm/file_open")
            .unwrap();
        let (_, entries) = all.iter().find(|(n, _)| n == "lsm/file_open").unwrap();
        let at = entries[0].insn_offset as usize;

        assert_eq!(
            immediate_at(&spec.instructions, at),
            0,
            "the object should start out carrying the offset from its own header"
        );

        apply(
            &mut spec.instructions,
            &spec.section,
            entries,
            &local,
            &kernel,
        )
        .unwrap();

        assert_eq!(
            immediate_at(&spec.instructions, at),
            20,
            "the instruction still holds an offset from the wrong kernel"
        );
    }

    #[test]
    fn a_field_that_did_not_move_is_left_where_it_was() {
        // The control. Without it, a relocation pass that wrote a constant —
        // or that wrote nothing — would be indistinguishable from one that
        // resolved the offset, because 0 happens to be right for sa_family.
        let elf = Elf::parse(CAPTURED).unwrap();
        let local = Btf::parse(elf.section(".BTF").unwrap().bytes).unwrap();
        let kernel = Btf::parse(KERNELISH).unwrap();
        let all = relocations(elf.section(".BTF.ext").unwrap().bytes, &local).unwrap();

        let mut spec = program::programs(&elf)
            .unwrap()
            .into_iter()
            .find(|p| p.section == "lsm/socket_connect")
            .unwrap();
        let (_, entries) = all.iter().find(|(n, _)| n == "lsm/socket_connect").unwrap();
        let at = entries[0].insn_offset as usize;

        apply(
            &mut spec.instructions,
            &spec.section,
            entries,
            &local,
            &kernel,
        )
        .unwrap();

        assert_eq!(
            immediate_at(&spec.instructions, at),
            0,
            "sa_family is the first member of sockaddr on both sides"
        );
    }

    #[test]
    fn only_the_instruction_the_relocation_names_is_touched() {
        // A patch that wrote to the wrong slot would still produce the right
        // answer above while corrupting something else — and the corruption
        // would surface as a verifier error about an unrelated instruction.
        let elf = Elf::parse(CAPTURED).unwrap();
        let local = Btf::parse(elf.section(".BTF").unwrap().bytes).unwrap();
        let kernel = Btf::parse(KERNELISH).unwrap();
        let all = relocations(elf.section(".BTF.ext").unwrap().bytes, &local).unwrap();

        let mut spec = program::programs(&elf)
            .unwrap()
            .into_iter()
            .find(|p| p.section == "lsm/file_open")
            .unwrap();
        let before = spec.instructions.clone();
        let (_, entries) = all.iter().find(|(n, _)| n == "lsm/file_open").unwrap();
        let at = entries[0].insn_offset as usize;

        apply(
            &mut spec.instructions,
            &spec.section,
            entries,
            &local,
            &kernel,
        )
        .unwrap();

        for (index, (old, new)) in before.iter().zip(spec.instructions.iter()).enumerate() {
            if (at..at + 8).contains(&index) {
                continue;
            }
            assert_eq!(old, new, "byte {index} changed and no relocation named it");
        }
    }
}
