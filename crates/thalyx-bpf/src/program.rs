//! The programs in the object, and the two rewrites they need before loading.
//!
//! ## Where a program's attach point comes from
//!
//! `SEC("lsm/file_open")` is the whole declaration. The kernel does not read
//! that string; it wants a BTF type id, and the id is of a function named
//! `bpf_lsm_file_open` in the kernel's own BTF. So the section name is the
//! hook name with a prefix, and the loader turns one into the other and then
//! looks it up. A kernel that does not expose that hook has no such function,
//! and the failure is "this kernel does not have `bpf_lsm_file_open`" rather
//! than a verifier error about instructions.
//!
//! ## The map rewrite, which is not CO-RE
//!
//! Separate from the CO-RE relocations and easy to conflate with them. Where
//! the program says `&thalyx_policy`, clang leaves a 64-bit immediate load with
//! a zero in it and an ELF relocation naming the map. A map only has an
//! identity once it has been created, so the number that goes there is a file
//! descriptor — which means **the maps must exist before any program is
//! loaded**, and the descriptors must still be open when it is.
//!
//! That ordering is the single easiest thing to get wrong here, and getting it
//! wrong produces `BPF_MAP_FD` pointing at whatever else the process has open.

use crate::elf::Elf;

#[derive(Debug, thiserror::Error)]
pub enum ProgramError {
    #[error("the object has no program sections; nothing in it starts with `lsm/`")]
    NoPrograms,

    #[error("program `{section}` refers to `{name}`, which is not a map this object declares")]
    UnknownMap { section: String, name: String },

    #[error(
        "the relocation at byte {offset} of `{section}` is not a 64-bit immediate load \
         (opcode {opcode:#04x}), so it is not a map reference"
    )]
    NotAMapLoad {
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

    #[error("program `{0}` has no instructions")]
    Empty(String),
}

type Result<T> = std::result::Result<T, ProgramError>;

/// One program, ready to be relocated and loaded.
pub struct ProgramSpec {
    /// The ELF section, e.g. `lsm/file_open`.
    pub section: String,
    /// The name the kernel will show it under, from the C function.
    pub name: String,
    /// The kernel function this attaches to, e.g. `bpf_lsm_file_open`.
    pub attach_to: String,
    /// A copy, because loading rewrites it.
    pub instructions: Vec<u8>,
    /// Where a map descriptor has to be written, and which map.
    pub map_uses: Vec<(usize, String)>,
}

/// The prefix clang's `SEC()` uses for an LSM hook.
const LSM_SECTION: &str = "lsm/";

/// What the kernel calls the same hook in its own BTF.
const LSM_SYMBOL: &str = "bpf_lsm_";

/// Every LSM program in the object.
pub fn programs(elf: &Elf<'_>) -> Result<Vec<ProgramSpec>> {
    let mut out = Vec::new();

    for (index, section) in elf.sections.iter().enumerate() {
        let Some(hook) = section.name.strip_prefix(LSM_SECTION) else {
            continue;
        };
        if section.bytes.is_empty() {
            return Err(ProgramError::Empty(section.name.clone()));
        }

        // The function name, for the kernel's own listing. A section holds one
        // program here; the first non-section symbol in it with a name is the
        // function. Falling back to the hook keeps a program loadable rather
        // than failing over a label.
        let name = elf
            .symbols
            .iter()
            .find(|s| {
                s.section == index && s.value == 0 && !s.name.is_empty() && s.name != section.name
            })
            .map(|s| s.name.clone())
            .unwrap_or_else(|| hook.to_string());

        let mut map_uses = Vec::new();
        if let Some(relocations) = elf.relocations.get(&index) {
            for relocation in relocations {
                let symbol =
                    elf.symbols
                        .get(relocation.symbol)
                        .ok_or_else(|| ProgramError::UnknownMap {
                            section: section.name.clone(),
                            name: format!("<symbol {}>", relocation.symbol),
                        })?;
                map_uses.push((relocation.offset as usize, symbol.name.clone()));
            }
        }

        out.push(ProgramSpec {
            section: section.name.clone(),
            name,
            attach_to: format!("{LSM_SYMBOL}{hook}"),
            instructions: section.bytes.to_vec(),
            map_uses,
        });
    }

    if out.is_empty() {
        return Err(ProgramError::NoPrograms);
    }
    Ok(out)
}

/// `BPF_LD | BPF_IMM | BPF_DW` — the 64-bit immediate load, and the only
/// instruction a map reference can be.
const LD_IMM64: u8 = 0x18;

/// The src_reg value that tells the kernel "this immediate is a map fd".
///
/// Without it the immediate is just a number and the verifier rejects the
/// program for using an integer as a pointer — which is the better of the two
/// possible failures, and still one whose message names the instruction rather
/// than the missing flag.
const BPF_PSEUDO_MAP_FD: u8 = 1;

impl ProgramSpec {
    /// Write the map descriptors into the instructions that refer to them.
    ///
    /// `descriptors` maps a map's name to its open file descriptor. Every use
    /// must be satisfied: a map this cannot resolve is an error and never a
    /// zero, because descriptor 0 is standard input and the verifier would
    /// happily be told that it is a map.
    pub fn relocate_maps(&mut self, descriptors: &dyn Fn(&str) -> Option<i32>) -> Result<()> {
        for (offset, name) in &self.map_uses {
            let slot = self
                .instructions
                .get_mut(*offset..*offset + 16)
                .ok_or_else(|| ProgramError::OutsideProgram {
                    section: self.section.clone(),
                    offset: *offset,
                    len: 0,
                })?;

            if slot[0] != LD_IMM64 {
                return Err(ProgramError::NotAMapLoad {
                    section: self.section.clone(),
                    offset: *offset,
                    opcode: slot[0],
                });
            }

            let descriptor = descriptors(name).ok_or_else(|| ProgramError::UnknownMap {
                section: self.section.clone(),
                name: name.clone(),
            })?;

            // The low nibble of byte 1 is dst_reg and must be left alone; the
            // high nibble is src_reg and is where the pseudo-map marker goes.
            slot[1] = (slot[1] & 0x0f) | (BPF_PSEUDO_MAP_FD << 4);
            slot[4..8].copy_from_slice(&descriptor.to_le_bytes());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURED: &[u8] = include_bytes!("../tests/captured/thalyx_lsm.bpf.o");

    fn specs() -> Vec<ProgramSpec> {
        let elf = Elf::parse(CAPTURED).unwrap();
        programs(&elf).expect("the captured object has programs")
    }

    #[test]
    fn both_hooks_are_found_and_named_the_way_the_kernel_names_them() {
        // `SEC("lsm/file_open")` has to become a lookup for `bpf_lsm_file_open`
        // in the kernel's BTF. Getting the prefix wrong gives "this kernel does
        // not expose that hook" on a kernel that does.
        let specs = specs();
        let attach: Vec<&str> = specs.iter().map(|p| p.attach_to.as_str()).collect();
        assert!(attach.contains(&"bpf_lsm_socket_connect"), "{attach:?}");
        assert!(attach.contains(&"bpf_lsm_file_open"), "{attach:?}");
    }

    #[test]
    fn each_program_carries_the_instructions_it_was_compiled_into() {
        let specs = specs();
        assert_eq!(specs.len(), 2);
        for spec in &specs {
            assert!(!spec.instructions.is_empty(), "{} is empty", spec.section);
            assert_eq!(
                spec.instructions.len() % 8,
                0,
                "{} is not a whole number of instructions",
                spec.section
            );
        }
    }

    #[test]
    fn every_map_the_program_uses_is_a_place_a_descriptor_must_go() {
        // Five references per program in this object. Each is an instruction
        // holding a zero until a map exists to put there.
        let specs = specs();
        for spec in &specs {
            assert!(
                !spec.map_uses.is_empty(),
                "{} refers to no maps, which cannot be right",
                spec.section
            );
            for (_, name) in &spec.map_uses {
                assert!(name.starts_with("thalyx_"), "unexpected map {name}");
            }
        }
    }

    #[test]
    fn relocating_writes_the_descriptor_and_marks_it_as_a_map() {
        // Both halves. Without the marker the verifier sees an integer used as
        // a pointer; without the descriptor it sees map 0, which is stdin.
        let mut spec = specs().pop().unwrap();
        let (offset, _) = spec.map_uses[0].clone();
        spec.relocate_maps(&|_| Some(7)).unwrap();

        assert_eq!(
            spec.instructions[offset + 1] >> 4,
            BPF_PSEUDO_MAP_FD,
            "the instruction is not marked as carrying a map descriptor"
        );
        assert_eq!(
            &spec.instructions[offset + 4..offset + 8],
            &7i32.to_le_bytes(),
            "the descriptor is not in the immediate"
        );
    }

    #[test]
    fn relocating_leaves_the_destination_register_alone() {
        // src_reg and dst_reg share a byte. Overwriting the whole byte loads
        // the map into the wrong register, and the program still verifies.
        let mut spec = specs().pop().unwrap();
        let (offset, _) = spec.map_uses[0].clone();
        let before = spec.instructions[offset + 1] & 0x0f;
        spec.relocate_maps(&|_| Some(7)).unwrap();
        assert_eq!(spec.instructions[offset + 1] & 0x0f, before);
    }

    #[test]
    fn a_map_that_was_never_created_stops_the_load_rather_than_becoming_zero() {
        // Descriptor 0 is standard input. A loader that defaulted would hand
        // the verifier stdin and call it a policy map.
        let mut spec = specs().pop().unwrap();
        let error = spec
            .relocate_maps(&|_| None)
            .expect_err("an unresolvable map must not be silently zero");
        assert!(matches!(error, ProgramError::UnknownMap { .. }), "{error}");
    }

    #[test]
    fn a_relocation_landing_on_the_wrong_instruction_is_refused() {
        // The offset naming an instruction that is not a 64-bit immediate load
        // means the object is not what this thinks it is, and patching it would
        // corrupt whatever really is there.
        let mut spec = specs().pop().unwrap();
        spec.map_uses = vec![(8, "thalyx_policy".to_string())];
        // Byte 8 is instruction 1, which in both programs is a register move.
        let error = spec
            .relocate_maps(&|_| Some(3))
            .expect_err("a non-load must not be patched");
        assert!(matches!(error, ProgramError::NotAMapLoad { .. }), "{error}");
    }
}
