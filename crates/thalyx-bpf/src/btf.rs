//! BTF: the type information the kernel needs and clang puts in the object.
//!
//! Two things here depend on it, and neither is optional.
//!
//! **The maps.** A modern BPF object does not describe its maps in a struct the
//! loader can read. `__uint(type, BPF_MAP_TYPE_HASH)` compiles to a *pointer to
//! an array of 1 elements* whose element count is the number, and `__type(key,
//! __u64)` to a pointer to `__u64`. The map's real shape is only recoverable by
//! walking the types. That looks absurd written down and it is what libbpf
//! does, because it lets a map be declared with ordinary C and no macros the
//! compiler has to know about.
//!
//! **CO-RE.** Every `BPF_CORE_READ` in the program is a field access whose
//! offset is *not* baked in. The object records which type and which field, and
//! the loader looks the offset up in the running kernel's own BTF. That is what
//! makes one object work on kernels it was not compiled against — and it is why
//! this crate can be handed the same bytes on any machine.
//!
//! ## Refusing rather than guessing
//!
//! A type this does not understand is an error naming the kind. The alternative
//! — treating an unknown kind as size zero, or skipping it — produces a map
//! with the wrong value size, which the kernel accepts and which then silently
//! reads the wrong bytes forever. Rule 9.

use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum BtfError {
    #[error("the .BTF section is not BTF: magic is {0:#06x}, expected 0xeb9f")]
    NotBtf(u16),

    #[error("BTF version {0} is not 1, and this reads version 1")]
    WrongVersion(u8),

    #[error("BTF {what} runs past the end of the section")]
    Truncated { what: &'static str },

    #[error("BTF type {id} is kind {kind}, which this does not know how to read")]
    UnknownKind { id: u32, kind: u8 },

    #[error("BTF type {0} does not exist")]
    NoSuchType(u32),

    #[error("BTF type {0} has no size: it is a {1}")]
    NoSize(u32, &'static str),

    #[error("a chain of types starting at {0} never reached anything concrete")]
    TooDeep(u32),
}

type Result<T> = std::result::Result<T, BtfError>;

/// The kinds this understands, by their number in the format.
pub mod kind {
    pub const INT: u8 = 1;
    pub const PTR: u8 = 2;
    pub const ARRAY: u8 = 3;
    pub const STRUCT: u8 = 4;
    pub const UNION: u8 = 5;
    pub const ENUM: u8 = 6;
    pub const FWD: u8 = 7;
    pub const TYPEDEF: u8 = 8;
    pub const VOLATILE: u8 = 9;
    pub const CONST: u8 = 10;
    pub const RESTRICT: u8 = 11;
    pub const FUNC: u8 = 12;
    pub const FUNC_PROTO: u8 = 13;
    pub const VAR: u8 = 14;
    pub const DATASEC: u8 = 15;
    pub const FLOAT: u8 = 16;
    pub const DECL_TAG: u8 = 17;
    pub const TYPE_TAG: u8 = 18;
    pub const ENUM64: u8 = 19;
}

/// One member of a struct or union.
pub struct Member {
    pub name: String,
    pub type_id: u32,
    /// Bit offset, or with `kind_flag` set, the packed offset-and-size word.
    pub offset: u32,
}

/// One entry of a data section: which variable, where, how big.
pub struct SecInfo {
    pub type_id: u32,
    pub offset: u32,
    pub size: u32,
}

pub struct Type {
    pub name: String,
    pub kind: u8,
    /// For struct/union/int/enum: the size in bytes. For ptr/typedef/const/
    /// volatile/var/func: the type referred to. The format overlaps them in one
    /// field, and which one it is depends entirely on the kind.
    pub size_or_type: u32,
    /// Whether a struct's member offsets are packed with bitfield sizes.
    pub kind_flag: bool,
    pub members: Vec<Member>,
    pub sec_info: Vec<SecInfo>,
    /// For ARRAY: the element type and how many.
    pub array: Option<(u32, u32)>,
}

pub struct Btf {
    /// The string table, kept because `.BTF.ext` indexes into this one.
    strings: Vec<u8>,
    /// Indexed by type id. Id 0 is void and is not stored, so `types[0]` is
    /// type 1 — which is the single most likely place to be off by one, and
    /// [`Btf::type_of`] is the only way in.
    types: Vec<Type>,
    by_name: HashMap<String, u32>,
}

fn u16_at(bytes: &[u8], at: usize, what: &'static str) -> Result<u16> {
    let s = bytes.get(at..at + 2).ok_or(BtfError::Truncated { what })?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(bytes: &[u8], at: usize, what: &'static str) -> Result<u32> {
    let s = bytes.get(at..at + 4).ok_or(BtfError::Truncated { what })?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn name_at(strings: &[u8], offset: usize) -> String {
    let Some(rest) = strings.get(offset..) else {
        return String::new();
    };
    let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());
    String::from_utf8_lossy(&rest[..end]).into_owned()
}

impl Btf {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let magic = u16_at(bytes, 0, "header magic")?;
        if magic != 0xeb9f {
            return Err(BtfError::NotBtf(magic));
        }
        let version = *bytes.get(2).ok_or(BtfError::Truncated { what: "header" })?;
        if version != 1 {
            return Err(BtfError::WrongVersion(version));
        }

        let header_len = u32_at(bytes, 4, "hdr_len")? as usize;
        let type_off = u32_at(bytes, 8, "type_off")? as usize;
        let type_len = u32_at(bytes, 12, "type_len")? as usize;
        let str_off = u32_at(bytes, 16, "str_off")? as usize;
        let str_len = u32_at(bytes, 20, "str_len")? as usize;

        let types_bytes = bytes
            .get(header_len + type_off..header_len + type_off + type_len)
            .ok_or(BtfError::Truncated {
                what: "the type section",
            })?;
        let strings = bytes
            .get(header_len + str_off..header_len + str_off + str_len)
            .ok_or(BtfError::Truncated {
                what: "the string section",
            })?;

        let mut types = Vec::new();
        let mut by_name = HashMap::new();
        let mut at = 0usize;
        while at < types_bytes.len() {
            let id = types.len() as u32 + 1;
            let name_off = u32_at(types_bytes, at, "a type's name")? as usize;
            let info = u32_at(types_bytes, at + 4, "a type's info")?;
            let size_or_type = u32_at(types_bytes, at + 8, "a type's size")?;
            at += 12;

            let vlen = (info & 0xffff) as usize;
            let kind = ((info >> 24) & 0x1f) as u8;
            let kind_flag = (info >> 31) & 1 == 1;

            let mut members = Vec::new();
            let mut sec_info = Vec::new();
            let mut array = None;

            // How many bytes follow the common header, per kind. Getting one
            // of these wrong desynchronises everything after it, so each is
            // stated rather than derived.
            match kind {
                kind::INT | kind::DECL_TAG => at += 4,
                kind::VAR => at += 4,
                kind::ARRAY => {
                    let element = u32_at(types_bytes, at, "an array's element type")?;
                    let count = u32_at(types_bytes, at + 8, "an array's length")?;
                    array = Some((element, count));
                    at += 12;
                }
                kind::STRUCT | kind::UNION => {
                    for _ in 0..vlen {
                        members.push(Member {
                            name: name_at(
                                strings,
                                u32_at(types_bytes, at, "a member's name")? as usize,
                            ),
                            type_id: u32_at(types_bytes, at + 4, "a member's type")?,
                            offset: u32_at(types_bytes, at + 8, "a member's offset")?,
                        });
                        at += 12;
                    }
                }
                kind::DATASEC => {
                    for _ in 0..vlen {
                        sec_info.push(SecInfo {
                            type_id: u32_at(types_bytes, at, "a section entry's type")?,
                            offset: u32_at(types_bytes, at + 4, "a section entry's offset")?,
                            size: u32_at(types_bytes, at + 8, "a section entry's size")?,
                        });
                        at += 12;
                    }
                }
                kind::ENUM | kind::FUNC_PROTO => at += vlen * 8,
                kind::ENUM64 => at += vlen * 12,
                kind::PTR
                | kind::FWD
                | kind::TYPEDEF
                | kind::VOLATILE
                | kind::CONST
                | kind::RESTRICT
                | kind::FUNC
                | kind::FLOAT
                | kind::TYPE_TAG => {}
                other => return Err(BtfError::UnknownKind { id, kind: other }),
            }

            let name = name_at(strings, name_off);
            if !name.is_empty() {
                // First wins. A name can repeat across kinds — a struct and a
                // typedef of the same name — and the loader looks up by name
                // only for things it already knows the kind of.
                by_name.entry(name.clone()).or_insert(id);
            }
            types.push(Type {
                name,
                kind,
                size_or_type,
                kind_flag,
                members,
                sec_info,
                array,
            });
        }

        Ok(Btf {
            types,
            by_name,
            strings: strings.to_vec(),
        })
    }

    /// A name from the BTF string table.
    ///
    /// `.BTF.ext` records its section names and access strings as offsets into
    /// this table rather than carrying one of its own, which is why the table
    /// outlives parsing.
    pub fn string_at(&self, offset: usize) -> String {
        name_at(&self.strings, offset)
    }

    pub fn type_of(&self, id: u32) -> Result<&Type> {
        if id == 0 {
            return Err(BtfError::NoSuchType(0));
        }
        self.types
            .get(id as usize - 1)
            .ok_or(BtfError::NoSuchType(id))
    }

    pub fn id_of(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    /// Every type, so a caller can search by kind as well as by name.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        1..=(self.types.len() as u32)
    }

    /// Follow typedefs, const, volatile, restrict and type tags to the type
    /// underneath.
    ///
    /// Bounded rather than recursive: BTF from a kernel is not necessarily
    /// well-formed, a cycle is representable, and a loader that hung on a
    /// malformed file would be a worse failure than one that refused it.
    pub fn resolved(&self, mut id: u32) -> Result<(u32, &Type)> {
        let start = id;
        for _ in 0..32 {
            let found = self.type_of(id)?;
            match found.kind {
                kind::TYPEDEF | kind::VOLATILE | kind::CONST | kind::RESTRICT | kind::TYPE_TAG => {
                    id = found.size_or_type
                }
                _ => return Ok((id, found)),
            }
        }
        Err(BtfError::TooDeep(start))
    }

    /// How many bytes a value of this type occupies.
    ///
    /// This is what becomes a map's `key_size` and `value_size`, and a wrong
    /// answer here is accepted by the kernel and then reads the wrong bytes for
    /// the life of the machine. So a type with no defined size is an error and
    /// never a zero.
    pub fn size_of(&self, id: u32) -> Result<u32> {
        let (id, found) = self.resolved(id)?;
        match found.kind {
            kind::INT | kind::STRUCT | kind::UNION | kind::ENUM | kind::ENUM64 | kind::FLOAT => {
                Ok(found.size_or_type)
            }
            // Always 8 here: BPF is a 64-bit machine regardless of the host.
            kind::PTR => Ok(8),
            kind::ARRAY => {
                let (element, count) = found.array.ok_or(BtfError::NoSize(id, "array"))?;
                Ok(self.size_of(element)? * count)
            }
            kind::FWD => Err(BtfError::NoSize(id, "forward declaration")),
            kind::FUNC | kind::FUNC_PROTO => Err(BtfError::NoSize(id, "function")),
            _ => Err(BtfError::NoSize(id, "type with no size")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::Elf;

    const CAPTURED: &[u8] = include_bytes!("../tests/captured/thalyx_lsm.bpf.o");

    fn btf() -> Btf {
        let elf = Elf::parse(CAPTURED).expect("the captured object parses");
        Btf::parse(elf.section(".BTF").expect("it has BTF").bytes).expect("its BTF parses")
    }

    #[test]
    fn the_maps_section_names_the_three_maps_the_program_uses() {
        // The whole reason BTF is read at all. If this walk is wrong the loader
        // creates maps of the wrong shape, the kernel accepts them, and the
        // policy is read from the wrong bytes forever.
        let btf = btf();
        let maps = btf
            .id_of(".maps")
            .and_then(|id| btf.type_of(id).ok())
            .expect("a .maps data section");

        let names: Vec<&str> = maps
            .sec_info
            .iter()
            .filter_map(|entry| btf.type_of(entry.type_id).ok())
            .map(|t| t.name.as_str())
            .collect();

        assert_eq!(names.len(), 3, "{names:?}");
        for wanted in ["thalyx_policy", "thalyx_denials", "thalyx_enforcing"] {
            assert!(
                names.contains(&wanted),
                "{wanted} is missing from {names:?}"
            );
        }
    }

    #[test]
    fn a_struct_written_in_c_measures_what_c_says_it_measures() {
        // `struct policy { __u32 allowed; __u32 flags; __u64 expires_ns; }` is
        // 16 bytes. This is the number that becomes the map's value_size, and
        // the Rust side of the policy has to agree with it byte for byte.
        let btf = btf();
        let id = btf.id_of("policy").expect("struct policy is in the BTF");
        assert_eq!(btf.size_of(id).unwrap(), 16);
    }

    #[test]
    fn the_types_a_map_is_declared_with_resolve_to_their_sizes() {
        // `__type(key, __u64)` — through a typedef, which is exactly the chain
        // `resolved` exists to walk.
        let btf = btf();
        let id = btf.id_of("__u64").expect("__u64 is in the BTF");
        assert_eq!(btf.size_of(id).unwrap(), 8);
        assert_eq!(btf.size_of(btf.id_of("__u32").unwrap()).unwrap(), 4);
    }

    #[test]
    fn a_pointer_is_eight_bytes_even_when_the_host_is_not_sixty_four_bit() {
        // BPF is a 64-bit machine. Taking the host's pointer size would give a
        // map the wrong value size on a 32-bit builder and nowhere else, which
        // is the kind of bug that is found years later.
        let btf = btf();
        let pointer = btf
            .ids()
            .find(|id| btf.type_of(*id).is_ok_and(|t| t.kind == kind::PTR))
            .expect("the object has pointers");
        assert_eq!(btf.size_of(pointer).unwrap(), 8);
    }

    #[test]
    fn a_forward_declaration_has_no_size_and_says_so_instead_of_zero() {
        // Rule 9. A zero here becomes a zero-sized map value, which the kernel
        // rejects with a message about the map rather than about the type — or
        // worse, accepts.
        let btf = btf();
        if let Some(id) = btf
            .ids()
            .find(|id| btf.type_of(*id).is_ok_and(|t| t.kind == kind::FWD))
        {
            assert!(matches!(btf.size_of(id), Err(BtfError::NoSize(_, _))));
        }
    }

    #[test]
    fn something_that_is_not_btf_is_refused_by_its_magic() {
        assert!(matches!(
            Btf::parse(b"\x00\x00\x01\x00________________________"),
            Err(BtfError::NotBtf(_))
        ));
    }

    #[test]
    fn a_truncated_btf_section_is_refused_rather_than_read_short() {
        // An object cut off mid-BTF would otherwise produce a type list that
        // stops early — and a missing map looks exactly like a map that was
        // never declared.
        let elf = Elf::parse(CAPTURED).unwrap();
        let full = elf.section(".BTF").unwrap().bytes;
        for keep in [4, 12, 24, full.len() / 2] {
            assert!(
                Btf::parse(&full[..keep]).is_err(),
                "{keep} bytes of BTF should not parse"
            );
        }
    }
}
