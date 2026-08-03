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

pub mod attached;
pub mod btf;
pub mod core;
pub mod elf;
pub mod loader;
pub mod maps;
pub mod program;

pub use attached::{AttachedError, Attachment, attachment};
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

        // Every entry that took a position, in order, so an alias can be
        // resolved to the one it names.
        let mut seen: Vec<String> = Vec::new();

        for line in body[..end].lines().skip(1) {
            let line = line.trim();
            // Comment and blank lines are not entries, and a `__MAX_` sentinel
            // is one but is never asked for.
            if line.is_empty() || line.starts_with("/*") || line.starts_with('*') {
                continue;
            }
            let name = line.trim_end_matches(',');

            // An alias — `BPF_PROG_RUN = BPF_PROG_TEST_RUN` is really in this
            // enum — names an entry that already exists and does **not** take a
            // position. Treating it as one would shift every command after it
            // by one, which is the same off-by-one that started all of this.
            // Nothing but an alias is accepted: a `= 7` would mean positions
            // have stopped being values and the counting is unsound.
            if let Some((alias, of)) = name.split_once('=') {
                let (alias, of) = (alias.trim(), of.trim());
                assert!(
                    seen.contains(&of.to_string()),
                    "`{alias}` is set to `{of}`, which is not an earlier entry — \
                     counting positions is no longer sound"
                );
                if alias == wanted {
                    return seen.iter().position(|s| s == of).expect("just checked") as u32;
                }
                continue;
            }

            if name == wanted {
                return seen.len() as u32;
            }
            seen.push(name.to_string());
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
    fn every_command_number_is_its_position_in_the_captured_enum() {
        // Eight commands, and a wrong one does not fail cleanly: `bpf(2)`
        // dispatches on this number, so an off-by-one runs a different command
        // against the same argument bytes. `LINK_GET_NEXT_ID` off by one is
        // `BPF_LINK_DETACH`, which would take enforcement down while walking it.
        use thalyx_syscall::bpf_cmd;
        assert_eq!(value_of("bpf_cmd", "BPF_MAP_CREATE"), bpf_cmd::MAP_CREATE);
        assert_eq!(value_of("bpf_cmd", "BPF_PROG_LOAD"), bpf_cmd::PROG_LOAD);
        assert_eq!(value_of("bpf_cmd", "BPF_OBJ_PIN"), bpf_cmd::OBJ_PIN);
        assert_eq!(
            value_of("bpf_cmd", "BPF_PROG_GET_FD_BY_ID"),
            bpf_cmd::PROG_GET_FD_BY_ID
        );
        assert_eq!(
            value_of("bpf_cmd", "BPF_OBJ_GET_INFO_BY_FD"),
            bpf_cmd::OBJ_GET_INFO_BY_FD
        );
        assert_eq!(
            value_of("bpf_cmd", "BPF_RAW_TRACEPOINT_OPEN"),
            bpf_cmd::RAW_TRACEPOINT_OPEN
        );
        assert_eq!(
            value_of("bpf_cmd", "BPF_LINK_GET_FD_BY_ID"),
            bpf_cmd::LINK_GET_FD_BY_ID
        );
        assert_eq!(
            value_of("bpf_cmd", "BPF_LINK_GET_NEXT_ID"),
            bpf_cmd::LINK_GET_NEXT_ID
        );
    }

    #[test]
    fn the_alias_in_the_command_enum_does_not_take_a_number_of_its_own() {
        // `BPF_PROG_RUN = BPF_PROG_TEST_RUN` is really there. Counting it as an
        // entry shifts every command after it by one — which would have been
        // invisible, because the first command past it that this crate uses is
        // `BPF_PROG_GET_NEXT_ID` and nothing else would have complained.
        //
        // No hand-written fixture would have had this line in it.
        assert_eq!(
            value_of("bpf_cmd", "BPF_PROG_RUN"),
            value_of("bpf_cmd", "BPF_PROG_TEST_RUN")
        );
        assert_eq!(value_of("bpf_cmd", "BPF_PROG_GET_NEXT_ID"), 11);
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

/// The two structures the kernel fills in, checked against the captured header.
///
/// `thalyx-syscall` declares a Rust prefix of each and reads two fields out of
/// it. Nothing about those offsets is guessable, and getting one wrong is
/// silent: `name` read four bytes early is the tail of `map_ids`, and the
/// program would be reported as called whatever those bytes spell. Thalyx would
/// then say enforcement is not attached on a machine where it is — the exact
/// shape of failure this crate exists to prevent, arriving through the code
/// that reports it.
///
/// So the offsets are computed from `tests/captured/bpf-uapi-structs.h` rather
/// than compared to numbers written twice.
#[cfg(test)]
mod layout {
    const HEADER: &str = include_str!("../tests/captured/bpf-uapi-structs.h");

    /// Size and alignment of the C types the captured structures use.
    ///
    /// Anything not in this table stops the walk rather than being assumed to
    /// be four bytes. A type this does not know is a type whose size this
    /// cannot compute, and a plausible wrong offset is the failure being
    /// guarded against.
    fn measure(declaration: &str) -> (usize, usize) {
        let (kind, count) = match declaration.split_once('[') {
            Some((kind, rest)) => {
                let inside = rest.trim_end_matches(']');
                (kind.trim(), length_of(inside))
            }
            None => (declaration.trim(), 1),
        };
        let unit = match kind {
            "__u8" | "char" => (1, 1),
            "__u32" => (4, 4),
            // `__aligned_u64` is a u64 that is eight-aligned even on the
            // architectures where a bare u64 would not be. On the ones Thalyx
            // targets they are the same thing, and saying so here is what makes
            // that an assumption somebody can find.
            "__u64" | "__aligned_u64" => (8, 8),
            other => panic!("the captured header uses `{other}`, whose size this cannot compute"),
        };
        (unit.0 * count, unit.1)
    }

    /// An array length, which in this header is either a number or one of two
    /// macros defined at the top of the capture.
    fn length_of(inside: &str) -> usize {
        if let Ok(number) = inside.trim().parse::<usize>() {
            return number;
        }
        let define = format!("#define {}", inside.trim());
        let line = HEADER
            .lines()
            .find(|l| l.starts_with(&define))
            .unwrap_or_else(|| panic!("`{inside}` is used as a length and is not defined here"));
        line[define.len()..]
            .trim()
            .trim_end_matches('U')
            .parse()
            .unwrap_or_else(|_| panic!("`{line}` is not a length this can read"))
    }

    /// Walk a structure until `stop_after`, returning that field's offset and
    /// the size of the prefix ending with it.
    fn offset_of(structure: &str, stop_after: &str) -> (usize, usize) {
        let start = HEADER
            .find(&format!("struct {structure} {{"))
            .unwrap_or_else(|| panic!("no `struct {structure}` in the captured header"));
        let body = &HEADER[start..];

        let mut offset = 0usize;
        let mut widest = 1usize;

        for line in body.lines().skip(1) {
            let line = line.trim();
            if line.is_empty() || line.starts_with("/*") || line.starts_with('*') {
                continue;
            }
            // Everything past the fields being measured: the union in
            // `bpf_link_info`, the closing brace, and the bitfields that follow
            // `ifindex` in `bpf_prog_info`. Reaching any of them before
            // `stop_after` means the field is not where this thinks it is.
            assert!(
                !line.starts_with("union") && !line.starts_with('}') && !line.contains(':'),
                "`{line}` came before `{stop_after}`, so the walk cannot go on"
            );

            // Strip the trailing comment C fields sometimes carry.
            let field = line.split("/*").next().unwrap_or(line);
            let field = field.trim().trim_end_matches(';').trim();
            let (kind, name) = field
                .rsplit_once(char::is_whitespace)
                .unwrap_or_else(|| panic!("`{field}` is not a field declaration this can read"));
            let bare = name.split('[').next().unwrap_or(name);

            let (size, align) = measure(&format!(
                "{}{}",
                kind.trim(),
                name.find('[').map(|at| &name[at..]).unwrap_or("")
            ));
            widest = widest.max(align);
            offset = offset.div_ceil(align) * align;

            if bare == stop_after {
                let end = offset + size;
                return (offset, end.div_ceil(widest) * widest);
            }
            offset += size;
        }
        panic!("`{stop_after}` is not a field of `struct {structure}`");
    }

    #[test]
    fn the_program_name_is_where_the_captured_header_puts_it() {
        // 64, and there is no way to arrive at that by thinking about it. Two
        // eight-byte pointers, a tag, and a `load_time` all have to be counted.
        let (offset, prefix) = offset_of("bpf_prog_info", "name");
        assert_eq!(offset, thalyx_syscall::info_layout::PROG_NAME_OFFSET);
        assert_eq!(prefix, thalyx_syscall::info_layout::PROG_PREFIX_LEN);
    }

    #[test]
    fn the_links_program_id_is_where_the_captured_header_puts_it() {
        let (offset, prefix) = offset_of("bpf_link_info", "prog_id");
        assert_eq!(offset, thalyx_syscall::info_layout::LINK_PROG_ID_OFFSET);
        assert_eq!(prefix, thalyx_syscall::info_layout::LINK_PREFIX_LEN);
    }

    #[test]
    fn a_field_this_cannot_measure_stops_the_walk_instead_of_being_assumed() {
        // The whole value of computing the offsets is that an unrecognised type
        // fails rather than being counted as four bytes and moving `name`.
        let panicked = std::panic::catch_unwind(|| measure("struct sockaddr")).is_err();
        assert!(panicked, "an unknown type was measured rather than refused");
    }

    #[test]
    fn the_capture_still_holds_the_fields_before_the_ones_being_measured() {
        // If somebody trims this header to "just the fields we read", every
        // offset above becomes zero and agrees with a Rust structure that would
        // also be wrong. The count is what makes the agreement mean something.
        assert!(
            HEADER.contains("__aligned_u64 map_ids;") && HEADER.contains("__u8  tag[BPF_TAG_SIZE]"),
            "the captured structure has been shortened, and the offsets it \
             produces no longer describe the kernel's"
        );
    }
}
