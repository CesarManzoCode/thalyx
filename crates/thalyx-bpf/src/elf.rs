//! Just enough ELF to read what clang emits for BPF.
//!
//! ## Why this is written and not taken from a crate
//!
//! Not size, and not pride: **failure modes**. Everything here answers a
//! malformed file the same way — with a named error saying which field of which
//! structure did not make sense — and that is the behaviour rule 9 asks for,
//! from a loader that is about to put a program in the kernel. A general ELF
//! library is written to read every ELF there is; this reads one shape of file,
//! refuses everything else, and says which part it refused.
//!
//! It is also small enough to be read in one sitting, which matters for a file
//! whose output the kernel will execute.
//!
//! ## What it does not do
//!
//! No 32-bit, no big-endian, no dynamic linking, no program headers. A BPF
//! object from clang is ELF64, little-endian, relocatable, and that is the only
//! thing this accepts — checked, and refused by name when it is something else.

use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum ElfError {
    #[error("not an ELF file: it does not start with \\x7fELF")]
    NotElf,

    #[error("{what}: this reads 64-bit little-endian relocatable ELF and nothing else")]
    WrongShape { what: &'static str },

    #[error("{what} runs past the end of the file: wants {wants} bytes at {at}, file is {len}")]
    Truncated {
        what: &'static str,
        at: usize,
        wants: usize,
        len: usize,
    },

    #[error("section {index} names string {offset}, which is past the end of the string table")]
    BadName { index: usize, offset: usize },

    #[error(
        "section {index} is a {kind} and its link field points at {link}, which does not exist"
    )]
    BadLink {
        index: usize,
        kind: &'static str,
        link: usize,
    },
}

type Result<T> = std::result::Result<T, ElfError>;

/// One section, with its bytes.
pub struct Section<'a> {
    pub name: String,
    pub kind: u32,
    pub bytes: &'a [u8],
    /// `sh_link`, which for a relocation section names the symbol table and
    /// for a symbol table names its string table.
    pub link: usize,
    /// `sh_info`, which for a relocation section names the section it patches.
    pub info: usize,
}

/// One symbol.
pub struct Symbol {
    pub name: String,
    /// Offset within its section.
    pub value: u64,
    /// Which section it belongs to.
    pub section: usize,
}

/// One relocation: patch the instruction at `offset` to refer to `symbol`.
pub struct Relocation {
    pub offset: u64,
    pub symbol: usize,
    pub kind: u32,
}

pub struct Elf<'a> {
    pub sections: Vec<Section<'a>>,
    pub symbols: Vec<Symbol>,
    /// Relocations by the index of the section they patch.
    pub relocations: HashMap<usize, Vec<Relocation>>,
}

/// Section types, from the ELF specification, named rather than spelled.
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_REL: u32 = 9;

const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ET_REL: u16 = 1;
const EM_BPF: u16 = 247;

fn u16_at(bytes: &[u8], at: usize, what: &'static str) -> Result<u16> {
    let slice = bytes.get(at..at + 2).ok_or(ElfError::Truncated {
        what,
        at,
        wants: 2,
        len: bytes.len(),
    })?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], at: usize, what: &'static str) -> Result<u32> {
    let slice = bytes.get(at..at + 4).ok_or(ElfError::Truncated {
        what,
        at,
        wants: 4,
        len: bytes.len(),
    })?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn u64_at(bytes: &[u8], at: usize, what: &'static str) -> Result<u64> {
    let slice = bytes.get(at..at + 8).ok_or(ElfError::Truncated {
        what,
        at,
        wants: 8,
        len: bytes.len(),
    })?;
    let mut eight = [0u8; 8];
    eight.copy_from_slice(slice);
    Ok(u64::from_le_bytes(eight))
}

/// A NUL-terminated name out of a string table.
fn name_at(table: &[u8], offset: usize) -> Option<String> {
    let rest = table.get(offset..)?;
    let end = rest.iter().position(|b| *b == 0)?;
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

impl<'a> Elf<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() < 64 || &bytes[..4] != b"\x7fELF" {
            return Err(ElfError::NotElf);
        }
        // Refused one field at a time so the message says which assumption the
        // file broke. "Wrong shape" with no detail sends the reader to guess.
        if bytes[4] != ELFCLASS64 {
            return Err(ElfError::WrongShape { what: "not 64-bit" });
        }
        if bytes[5] != ELFDATA2LSB {
            return Err(ElfError::WrongShape {
                what: "not little-endian",
            });
        }
        if u16_at(bytes, 16, "e_type")? != ET_REL {
            return Err(ElfError::WrongShape {
                what: "not a relocatable object",
            });
        }
        // Checked because loading a program compiled for another machine would
        // otherwise fail inside the verifier, where the message is about
        // instructions rather than about the file being for the wrong target.
        if u16_at(bytes, 18, "e_machine")? != EM_BPF {
            return Err(ElfError::WrongShape {
                what: "not compiled for BPF",
            });
        }

        let table_at = u64_at(bytes, 40, "e_shoff")? as usize;
        let entry_size = u16_at(bytes, 58, "e_shentsize")? as usize;
        let count = u16_at(bytes, 60, "e_shnum")? as usize;
        let names_index = u16_at(bytes, 62, "e_shstrndx")? as usize;

        // Read the headers first, without names: the section holding the names
        // is one of them, so nothing can be named until they are all located.
        struct Raw {
            name_offset: usize,
            kind: u32,
            offset: usize,
            size: usize,
            link: usize,
            info: usize,
        }
        let mut raw = Vec::with_capacity(count);
        for index in 0..count {
            let at = table_at + index * entry_size;
            raw.push(Raw {
                name_offset: u32_at(bytes, at, "sh_name")? as usize,
                kind: u32_at(bytes, at + 4, "sh_type")?,
                offset: u64_at(bytes, at + 24, "sh_offset")? as usize,
                size: u64_at(bytes, at + 32, "sh_size")? as usize,
                link: u32_at(bytes, at + 40, "sh_link")? as usize,
                info: u32_at(bytes, at + 44, "sh_info")? as usize,
            });
        }

        let names = {
            let header = raw.get(names_index).ok_or(ElfError::BadLink {
                index: names_index,
                kind: "section name table",
                link: names_index,
            })?;
            bytes
                .get(header.offset..header.offset + header.size)
                .ok_or(ElfError::Truncated {
                    what: "the section name table",
                    at: header.offset,
                    wants: header.size,
                    len: bytes.len(),
                })?
        };

        let mut sections = Vec::with_capacity(count);
        for (index, header) in raw.iter().enumerate() {
            // SHT_NOBITS (.bss) occupies no file space; every other section
            // must actually be in the file.
            let body: &[u8] = if header.kind == 8 {
                &[]
            } else {
                bytes
                    .get(header.offset..header.offset + header.size)
                    .ok_or(ElfError::Truncated {
                        what: "a section body",
                        at: header.offset,
                        wants: header.size,
                        len: bytes.len(),
                    })?
            };
            sections.push(Section {
                name: name_at(names, header.name_offset).ok_or(ElfError::BadName {
                    index,
                    offset: header.name_offset,
                })?,
                kind: header.kind,
                bytes: body,
                link: header.link,
                info: header.info,
            });
        }

        let symbols = Self::read_symbols(&sections)?;
        let relocations = Self::read_relocations(&sections)?;

        Ok(Elf {
            sections,
            symbols,
            relocations,
        })
    }

    fn read_symbols(sections: &[Section<'a>]) -> Result<Vec<Symbol>> {
        let Some((index, table)) = sections
            .iter()
            .enumerate()
            .find(|(_, s)| s.kind == SHT_SYMTAB)
        else {
            // No symbols at all is legal ELF and useless here: every map
            // reference is a symbol. Reported as an empty list rather than an
            // error, so the caller says what it was looking for.
            return Ok(Vec::new());
        };

        let names = sections
            .get(table.link)
            .filter(|s| s.kind == SHT_STRTAB)
            .ok_or(ElfError::BadLink {
                index,
                kind: "symbol table",
                link: table.link,
            })?;

        let mut symbols = Vec::new();
        for at in (0..table.bytes.len()).step_by(24) {
            if at + 24 > table.bytes.len() {
                break;
            }
            let name_offset = u32_at(table.bytes, at, "st_name")? as usize;
            symbols.push(Symbol {
                name: name_at(names.bytes, name_offset).unwrap_or_default(),
                section: u16_at(table.bytes, at + 6, "st_shndx")? as usize,
                value: u64_at(table.bytes, at + 8, "st_value")?,
            });
        }
        Ok(symbols)
    }

    fn read_relocations(sections: &[Section<'a>]) -> Result<HashMap<usize, Vec<Relocation>>> {
        let mut out: HashMap<usize, Vec<Relocation>> = HashMap::new();
        for section in sections.iter().filter(|s| s.kind == SHT_REL) {
            let mut entries = Vec::new();
            for at in (0..section.bytes.len()).step_by(16) {
                if at + 16 > section.bytes.len() {
                    break;
                }
                let info = u64_at(section.bytes, at + 8, "r_info")?;
                entries.push(Relocation {
                    offset: u64_at(section.bytes, at, "r_offset")?,
                    // The symbol index is the high half and the type the low
                    // half, which is the one detail of this format most likely
                    // to be remembered backwards.
                    symbol: (info >> 32) as usize,
                    kind: (info & 0xffff_ffff) as u32,
                });
            }
            out.insert(section.info, entries);
        }
        Ok(out)
    }

    /// A section by name, if it is there.
    pub fn section(&self, name: &str) -> Option<&Section<'a>> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// The index of a section by name.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.sections.iter().position(|s| s.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// clang's real output, not something written to match this parser.
    /// See `tests/captured/README.md`.
    const CAPTURED: &[u8] = include_bytes!("../tests/captured/thalyx_lsm.bpf.o");

    #[test]
    fn the_captured_object_has_the_sections_the_loader_needs() {
        let elf = Elf::parse(CAPTURED).expect("clang's output parses");
        for wanted in [
            "lsm/socket_connect",
            "lsm/file_open",
            ".maps",
            "license",
            ".BTF",
            ".BTF.ext",
        ] {
            assert!(
                elf.section(wanted).is_some(),
                "no {wanted} section; the loader has nothing to work from"
            );
        }
    }

    #[test]
    fn the_license_says_gpl_because_the_kernel_will_refuse_anything_else() {
        // Not a formality: an LSM program using GPL-only helpers is rejected at
        // load time if this string is wrong, with an error about the helper
        // rather than about the licence.
        let elf = Elf::parse(CAPTURED).unwrap();
        let license = elf.section("license").unwrap();
        assert_eq!(&license.bytes[..3], b"GPL");
    }

    #[test]
    fn every_map_reference_in_the_program_is_a_relocation_to_be_patched() {
        // The three maps are referenced by name from both programs. Until each
        // of those references is rewritten to a map file descriptor, the
        // program is not loadable — so counting them is counting the work.
        let elf = Elf::parse(CAPTURED).unwrap();
        let program = elf.index_of("lsm/socket_connect").unwrap();
        let relocations = elf
            .relocations
            .get(&program)
            .expect("the program refers to maps and so must have relocations");

        let names: Vec<&str> = relocations
            .iter()
            .map(|r| elf.symbols[r.symbol].name.as_str())
            .collect();
        assert!(names.contains(&"thalyx_policy"), "{names:?}");
        assert!(names.contains(&"thalyx_denials"), "{names:?}");
        assert!(names.contains(&"thalyx_enforcing"), "{names:?}");
    }

    #[test]
    fn a_relocation_points_at_an_instruction_that_is_really_there() {
        // The offset is a byte offset into the program's instructions, and a
        // relocation past the end would be patched into whatever followed.
        let elf = Elf::parse(CAPTURED).unwrap();
        let index = elf.index_of("lsm/socket_connect").unwrap();
        let size = elf.sections[index].bytes.len() as u64;
        for relocation in &elf.relocations[&index] {
            assert!(
                relocation.offset + 16 <= size,
                "a relocation at {} in a {size}-byte program",
                relocation.offset
            );
        }
    }

    #[test]
    fn something_that_is_not_an_elf_file_is_refused_by_name() {
        assert!(matches!(Elf::parse(b"not an elf"), Err(ElfError::NotElf)));
        assert!(matches!(Elf::parse(&[]), Err(ElfError::NotElf)));
    }

    #[test]
    fn an_elf_for_another_machine_is_refused_before_the_verifier_sees_it() {
        // Otherwise the failure arrives from the kernel, phrased as a problem
        // with instructions, for a file that was simply for the wrong target.
        let mut wrong = CAPTURED.to_vec();
        wrong[18] = 62; // EM_X86_64
        wrong[19] = 0;
        let Err(error) = Elf::parse(&wrong) else {
            panic!("an x86 object was accepted as a BPF object");
        };
        assert!(
            error.to_string().contains("not compiled for BPF"),
            "{error}"
        );
    }

    #[test]
    fn a_truncated_object_says_where_it_ran_out_rather_than_panicking() {
        // Rule 9: a corrupt file produces the cautious answer. Half a BPF
        // object is exactly what an interrupted build leaves behind.
        for keep in [64, 100, 500, CAPTURED.len() / 2] {
            match Elf::parse(&CAPTURED[..keep]) {
                Ok(_) => panic!("{keep} bytes of an object should not parse"),
                Err(error) => {
                    let text = error.to_string();
                    assert!(
                        text.contains("past the end") || text.contains("does not exist"),
                        "at {keep} bytes the error was: {text}"
                    );
                }
            }
        }
    }
}
