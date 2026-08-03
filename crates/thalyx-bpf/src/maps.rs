//! What the `.maps` section actually says, once the types are walked.
//!
//! ## The encoding, because it is not guessable
//!
//! A map declared like this:
//!
//! ```c
//! struct {
//!     __uint(type, BPF_MAP_TYPE_HASH);
//!     __uint(max_entries, 4096);
//!     __type(key, __u64);
//!     __type(value, struct policy);
//! } thalyx_policy SEC(".maps");
//! ```
//!
//! contains no numbers at all in the data section — the section is 16 bytes of
//! zeroes per map. Every value is carried in the *types*:
//!
//! - `__uint(name, N)` expands to `int (*name)[N]`: a pointer to an array of N
//!   elements. **The number is the array's length.**
//! - `__type(name, T)` expands to `T *name`: a pointer to T. The number wanted
//!   is `sizeof(T)`, so the pointer is followed and the target measured.
//!
//! This is what libbpf does, and the reason is that it needs no compiler
//! support: it is ordinary C that happens to encode integers in a place the
//! type information preserves.
//!
//! ## Why an unknown member is not ignored
//!
//! A member this does not understand is refused by name. Skipping it would
//! silently drop `map_flags` — which is where `BPF_F_NO_PREALLOC` lives, among
//! others — and produce a map that works, mostly, and differs from the declared
//! one in a way nothing would ever report.

use crate::btf::{Btf, BtfError, kind};

#[derive(Debug, thiserror::Error)]
pub enum MapError {
    #[error("reading the types of map `{name}`: {source}")]
    Types {
        name: String,
        #[source]
        source: BtfError,
    },

    #[error("map `{name}` declares `{member}`, which this loader does not know")]
    UnknownMember { name: String, member: String },

    #[error("map `{name}` has no `{member}`, and one is required for a {kind}")]
    Missing {
        name: String,
        member: &'static str,
        kind: &'static str,
    },

    #[error("map `{name}` declares `{member}` as something other than a pointer")]
    NotAPointer { name: String, member: String },

    #[error("there is no .maps section, so this object declares no maps")]
    NoMapsSection,
}

type Result<T> = std::result::Result<T, MapError>;

/// One map, in the terms `BPF_MAP_CREATE` wants.
#[derive(Debug, PartialEq, Eq)]
pub struct MapSpec {
    pub name: String,
    pub map_type: u32,
    pub key_size: u32,
    pub value_size: u32,
    pub max_entries: u32,
    pub flags: u32,
}

/// Map types that carry no keys or values, where a key size would be wrong
/// rather than merely absent.
///
/// A ringbuffer's `max_entries` is a byte size and its key and value sizes must
/// both be zero; passing 8 and 16 there is rejected by the kernel with a
/// message about the map, which is at least honest but sends the reader looking
/// at the declaration rather than at the loader.
fn is_keyless(map_type: u32) -> bool {
    const RINGBUF: u32 = 27;
    const STACK_TRACE: u32 = 7;
    matches!(map_type, RINGBUF | STACK_TRACE)
}

/// The integer behind `__uint(name, N)`: a pointer to an array of N.
fn uint_member(btf: &Btf, name: &str, member: &str, type_id: u32) -> Result<u32> {
    let fail = |source| MapError::Types {
        name: name.to_string(),
        source,
    };
    let (_, pointer) = btf.resolved(type_id).map_err(fail)?;
    if pointer.kind != kind::PTR {
        return Err(MapError::NotAPointer {
            name: name.to_string(),
            member: member.to_string(),
        });
    }
    let (_, array) = btf.resolved(pointer.size_or_type).map_err(fail)?;
    let (_, count) = array.array.ok_or(MapError::NotAPointer {
        name: name.to_string(),
        member: member.to_string(),
    })?;
    Ok(count)
}

/// The size behind `__type(name, T)`: a pointer to T, measured.
fn type_member(btf: &Btf, name: &str, member: &str, type_id: u32) -> Result<u32> {
    let fail = |source| MapError::Types {
        name: name.to_string(),
        source,
    };
    let (_, pointer) = btf.resolved(type_id).map_err(fail)?;
    if pointer.kind != kind::PTR {
        return Err(MapError::NotAPointer {
            name: name.to_string(),
            member: member.to_string(),
        });
    }
    btf.size_of(pointer.size_or_type).map_err(fail)
}

/// Every map the object declares, in the order the section lists them.
pub fn declared(btf: &Btf) -> Result<Vec<MapSpec>> {
    let section = btf
        .id_of(".maps")
        .and_then(|id| btf.type_of(id).ok())
        .filter(|t| t.kind == kind::DATASEC)
        .ok_or(MapError::NoMapsSection)?;

    let mut out = Vec::new();
    for entry in &section.sec_info {
        let variable = btf
            .type_of(entry.type_id)
            .map_err(|source| MapError::Types {
                name: format!("<type {}>", entry.type_id),
                source,
            })?;
        let name = variable.name.clone();
        let fail = |source| MapError::Types {
            name: name.clone(),
            source,
        };

        let (_, definition) = btf.resolved(variable.size_or_type).map_err(fail)?;

        let mut map_type = None;
        let mut key_size = None;
        let mut value_size = None;
        let mut max_entries = None;
        let mut flags = 0u32;

        for member in &definition.members {
            match member.name.as_str() {
                "type" => map_type = Some(uint_member(btf, &name, "type", member.type_id)?),
                "max_entries" => {
                    max_entries = Some(uint_member(btf, &name, "max_entries", member.type_id)?)
                }
                "map_flags" => flags = uint_member(btf, &name, "map_flags", member.type_id)?,
                "key" => key_size = Some(type_member(btf, &name, "key", member.type_id)?),
                "value" => value_size = Some(type_member(btf, &name, "value", member.type_id)?),
                // The explicit-size spellings, which win over the type ones
                // because a declaration carrying both means the author wanted
                // the number rather than the measurement.
                "key_size" => key_size = Some(uint_member(btf, &name, "key_size", member.type_id)?),
                "value_size" => {
                    value_size = Some(uint_member(btf, &name, "value_size", member.type_id)?)
                }
                other => {
                    return Err(MapError::UnknownMember {
                        name: name.clone(),
                        member: other.to_string(),
                    });
                }
            }
        }

        let map_type = map_type.ok_or(MapError::Missing {
            name: name.clone(),
            member: "type",
            kind: "map",
        })?;
        let max_entries = max_entries.ok_or(MapError::Missing {
            name: name.clone(),
            member: "max_entries",
            kind: "map",
        })?;

        // A keyless map must be created with zeroes, and a keyed one must not
        // be created with a guess. The two halves are stated together because
        // defaulting either to zero is how a map ends up the wrong shape.
        let (key_size, value_size) = if is_keyless(map_type) {
            (0, 0)
        } else {
            (
                key_size.ok_or(MapError::Missing {
                    name: name.clone(),
                    member: "key",
                    kind: "keyed map",
                })?,
                value_size.ok_or(MapError::Missing {
                    name: name.clone(),
                    member: "value",
                    kind: "keyed map",
                })?,
            )
        };

        out.push(MapSpec {
            name,
            map_type,
            key_size,
            value_size,
            max_entries,
            flags,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::Elf;

    const CAPTURED: &[u8] = include_bytes!("../tests/captured/thalyx_lsm.bpf.o");

    fn specs() -> Vec<MapSpec> {
        let elf = Elf::parse(CAPTURED).unwrap();
        let btf = Btf::parse(elf.section(".BTF").unwrap().bytes).unwrap();
        declared(&btf).expect("the captured object's maps are readable")
    }

    #[test]
    fn the_three_maps_come_out_exactly_as_the_c_declares_them() {
        // Written against `lsm/thalyx_lsm.bpf.c`, read from clang's output.
        // Every one of these numbers is one the kernel will hold Thalyx to: a
        // wrong value_size is accepted and then reads the wrong bytes for the
        // life of the machine.
        let specs = specs();

        let policy = specs.iter().find(|m| m.name == "thalyx_policy").unwrap();
        assert_eq!(policy.map_type, 1, "BPF_MAP_TYPE_HASH");
        assert_eq!(policy.max_entries, 4096);
        assert_eq!(policy.key_size, 8, "a cgroup id is a __u64");
        assert_eq!(policy.value_size, 16, "struct policy is 4 + 4 + 8");

        let enforcing = specs.iter().find(|m| m.name == "thalyx_enforcing").unwrap();
        assert_eq!(enforcing.map_type, 2, "BPF_MAP_TYPE_ARRAY");
        assert_eq!(enforcing.max_entries, 1);
        assert_eq!(enforcing.key_size, 4);
        assert_eq!(enforcing.value_size, 4);
    }

    #[test]
    fn a_ringbuffer_is_created_with_no_key_and_a_size_in_bytes() {
        // The one map whose max_entries is not a count. Giving it a key size
        // is rejected by the kernel, and defaulting its sizes from the absent
        // declaration is how that happens.
        let specs = specs();
        let denials = specs.iter().find(|m| m.name == "thalyx_denials").unwrap();
        assert_eq!(denials.map_type, 27, "BPF_MAP_TYPE_RINGBUF");
        assert_eq!(denials.max_entries, 256 * 1024, "bytes, not entries");
        assert_eq!(denials.key_size, 0);
        assert_eq!(denials.value_size, 0);
    }

    #[test]
    fn the_maps_the_policy_store_writes_to_are_all_here() {
        // `thalyx-permd` writes thalyx_policy and thalyx_enforcing by name. A
        // loader that created two of the three would leave the machine
        // reporting enforcement while one half of it was missing.
        let specs = specs();
        let names: Vec<&str> = specs.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names.len(), 3, "{names:?}");
        for wanted in ["thalyx_policy", "thalyx_denials", "thalyx_enforcing"] {
            assert!(names.contains(&wanted), "{wanted} missing from {names:?}");
        }
    }

    #[test]
    fn an_object_with_no_maps_section_says_that_rather_than_returning_none() {
        // "No maps declared" and "the section is missing" are different facts.
        // The second one means the object is not what the loader was handed.
        let btf = Btf::parse(&[
            0x9f, 0xeb, 1, 0, 24, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ])
        .expect("an empty but valid BTF header");
        assert!(matches!(declared(&btf), Err(MapError::NoMapsSection)));
    }
}
