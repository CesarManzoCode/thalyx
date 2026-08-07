//! Every size and offset this crate writes, checked against the kernel's own
//! header rather than against a reading of it.
//!
//! `tests/uapi_btrfs_tree.h` and `tests/uapi_btrfs.h` are
//! `include/uapi/linux/btrfs_tree.h` and `include/uapi/linux/btrfs.h`, captured
//! verbatim. Rule 6 of `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md` asks for
//! one real sample for a parser; a writer needs the same thing for the same
//! reason, and more urgently — a parser that misreads a field gets a wrong answer,
//! and a writer that misplaces one produces a filesystem nobody can read at all.
//!
//! Both files, because the structs are in the first and the array bounds that
//! size them are in the second. Capturing only the one with the structs in it
//! produced a parser that could not resolve `BTRFS_UUID_SIZE` and gave up on
//! `btrfs_root_item` — which is the good failure: a bound it could not resolve is
//! refused rather than assumed, because an assumed 16 would have been right and an
//! assumed anything else would have shifted every offset after it.
//!
//! The parser below is small and specific: it takes the packed structs out of the
//! headers and adds up their fields. It is not a C parser and does not pretend to
//! be. What makes it trustworthy is that it is checked *by the headers it reads* —
//! `the_parser_agrees_with_sizes_the_header_states_itself` uses sizes the headers
//! state in their own text, so a parser that had stopped matching the files fails
//! before any of the real claims are evaluated.

use std::collections::HashMap;

/// Both headers, as shipped, concatenated.
///
/// `btrfs_tree.h` includes `btrfs.h`, so this is what the compiler sees. The
/// struct definitions are all in the first and the constants they need are spread
/// across both.
fn header() -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    ["uapi_btrfs_tree.h", "uapi_btrfs.h"]
        .iter()
        .map(|name| {
            std::fs::read_to_string(dir.join(name))
                .unwrap_or_else(|_| panic!("{name} is part of the repository"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The width of one C type, and of the fixed-size arrays the header uses.
fn width(kind: &str, sizes: &HashMap<String, usize>) -> Option<usize> {
    Some(match kind {
        "__u8" | "char" | "__s8" => 1,
        "__le16" | "__u16" => 2,
        "__le32" | "__u32" => 4,
        "__le64" | "__u64" | "__s64" => 8,
        other => *sizes.get(other.trim_start_matches("struct "))?,
    })
}

/// Constants the header defines as plain integers, for array bounds.
fn constants(text: &str) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("#define ") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let (Some(name), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        if let Ok(number) = value
            .trim_end_matches("ULL")
            .trim_end_matches("UL")
            .parse::<usize>()
        {
            out.insert(name.to_string(), number);
        }
    }
    out
}

/// Field name to offset, plus the total size, for one struct in the header.
///
/// Nested structs are resolved from `sizes`, so callers parse in dependency
/// order. A struct naming something absent from `sizes` returns `None` rather
/// than guessing a width — a guess would silently shift every offset after it.
fn parse(
    text: &str,
    name: &str,
    sizes: &HashMap<String, usize>,
    constants: &HashMap<String, usize>,
) -> Option<(HashMap<String, usize>, usize)> {
    let start = text.find(&format!("struct {name} {{"))?;
    let body = &text[start..];
    let end = body.find("\n}")?;
    let body = &body[..end];

    let mut offsets = HashMap::new();
    let mut at = 0usize;
    for line in body.lines().skip(1) {
        let line = line.trim();
        // Comments and the header's own blank lines. `/*` also catches the
        // multi-line comment bodies, whose continuation lines start with `*`.
        if line.is_empty()
            || line.starts_with("/*")
            || line.starts_with('*')
            || line.starts_with("//")
        {
            continue;
        }
        // A trailing comment on the same line as the field: `__le64 ctransid; /*
        // updated when an inode changes */`. Stripping the semicolon without
        // stripping this first silently drops the field — which is exactly what
        // happened, and the symptom was `btrfs_root_item` measuring 343 instead
        // of 439, i.e. the parser quietly losing the six fields that carry
        // trailing comments. Rule 5: the harness was the thing that was wrong.
        let line = match line.find("/*") {
            Some(at) => line[..at].trim_end(),
            None => line,
        };
        let Some(declaration) = line.strip_suffix(';') else {
            continue;
        };
        let words: Vec<&str> = declaration.split_whitespace().collect();
        // `struct btrfs_timespec atime` is three words for one field; everything
        // else is two.
        let (kind, mut field) = match words.as_slice() {
            ["struct", nested, field] => (sizes.get(*nested).map(|_| *nested)?, *field),
            [kind, field] => (*kind, *field),
            _ => continue,
        };

        // An array: the element count multiplies the width.
        let mut count = 1usize;
        if let Some(open) = field.find('[') {
            let bound = &field[open + 1..field.len() - 1];
            count = bound
                .parse::<usize>()
                .ok()
                .or_else(|| constants.get(bound).copied())?;
            field = &field[..open];
        }
        offsets.insert(field.to_string(), at);
        at += width(kind, sizes)? * count;
    }
    Some((offsets, at))
}

/// Every struct this crate writes, in dependency order.
fn all() -> (
    HashMap<String, usize>,
    HashMap<String, HashMap<String, usize>>,
) {
    let text = header();
    let constants = constants(&text);
    let mut sizes: HashMap<String, usize> = HashMap::new();
    let mut fields: HashMap<String, HashMap<String, usize>> = HashMap::new();

    for name in [
        "btrfs_disk_key",
        "btrfs_timespec",
        "btrfs_inode_item",
        "btrfs_root_backup",
        "btrfs_root_item",
        "btrfs_dir_item",
        "btrfs_dev_item",
        "btrfs_stripe",
        "btrfs_chunk",
        "btrfs_header",
        "btrfs_item",
        "btrfs_key_ptr",
        "btrfs_dev_extent",
        "btrfs_block_group_item",
        "btrfs_extent_item",
        "btrfs_super_block",
    ] {
        let (offsets, size) = parse(&text, name, &sizes, &constants)
            .unwrap_or_else(|| panic!("could not parse struct {name} out of the captured header"));
        sizes.insert(name.to_string(), size);
        fields.insert(name.to_string(), offsets);
    }
    (sizes, fields)
}

#[test]
fn the_parser_agrees_with_sizes_the_header_states_itself() {
    // The instrument, checked before anything is measured with it. Rule 5: before
    // believing something the parser says is wrong, rule out that the parser got
    // it wrong — and here the header states two of the answers in its own text,
    // so it can grade its own reader.
    //
    // `btrfs_super_block` is 4096 by the static_assert in fs/btrfs/fs.h, and
    // `btrfs_disk_key` is 17 because it is three fields the header packs.
    let (sizes, _) = all();
    assert_eq!(
        sizes["btrfs_super_block"], 4096,
        "the parser does not reproduce the superblock's asserted size, so nothing \
         it says about offsets can be trusted"
    );
    assert_eq!(sizes["btrfs_disk_key"], 17);
}

#[test]
fn every_item_this_crate_encodes_is_the_length_the_header_gives_it() {
    use thalyx_btrfs::disk;

    let (sizes, _) = all();

    assert_eq!(disk::Key::ENCODED_LEN, sizes["btrfs_disk_key"]);
    assert_eq!(thalyx_btrfs::leaf::HEADER_LEN, sizes["btrfs_header"]);

    assert_eq!(
        disk::inode_item(0o040_755, 0, 16384, 1, 0).len(),
        sizes["btrfs_inode_item"]
    );
    assert_eq!(
        disk::root_item(0, 0, 1, 0, 16384, [0; 16], 0).len(),
        sizes["btrfs_root_item"]
    );
    assert_eq!(
        disk::dev_item(1, 0, 0, 4096, [0; 16], [0; 16]).len(),
        sizes["btrfs_dev_item"]
    );
    assert_eq!(
        disk::block_group_item(0, 0).len(),
        sizes["btrfs_block_group_item"]
    );
    assert_eq!(
        disk::dev_extent(0, 0, [0; 16]).len(),
        sizes["btrfs_dev_extent"]
    );

    // The ones that carry a trailing payload, so the fixed part is what is
    // checked and the payload is subtracted.
    let name = b"default";
    assert_eq!(
        disk::dir_item(disk::Key::new(0, 0, 0), disk::FILE_TYPE_DIR, name).len() - name.len(),
        sizes["btrfs_dir_item"]
    );
    let stripe = (1u64, 0u64, [0u8; 16]);
    assert_eq!(
        disk::chunk_item(0, 65536, 4096, 0, &[stripe, stripe]).len(),
        sizes["btrfs_chunk"] + sizes["btrfs_stripe"],
        "a chunk item is the struct plus one extra stripe, since the struct \
         already contains the first"
    );

    // A skinny tree block's extent item is the extent item plus a one-byte
    // reference type and an eight-byte object id, inline.
    assert_eq!(
        disk::tree_block_extent(1, 5).len(),
        sizes["btrfs_extent_item"] + 1 + 8
    );
}

#[test]
fn the_superblock_offsets_this_crate_pins_are_the_ones_the_header_puts_them_at() {
    // Three numbers written as literals in `format.rs`, because accumulating them
    // from the field list there would be a second copy of the layout with nothing
    // checking it. This is the thing that checks them.
    let (_, fields) = all();
    let superblock = &fields["btrfs_super_block"];
    use thalyx_btrfs::format::super_offset;

    assert_eq!(superblock["label"], super_offset::LABEL);
    assert_eq!(
        superblock["cache_generation"],
        super_offset::CACHE_GENERATION
    );
    assert_eq!(
        superblock["sys_chunk_array"],
        super_offset::SYSTEM_CHUNK_ARRAY
    );
}

#[test]
fn the_fields_the_writer_puts_before_the_label_are_where_the_header_wants_them() {
    // `format.rs` writes everything from `fsid` to `dev_item` as one contiguous
    // run and asserts that it lands exactly on the label. That assert catches a
    // wrong total; it cannot catch two fields that are individually wrong and
    // cancel out. These are the ones whose position a reader of this crate would
    // have to take on trust.
    let (_, fields) = all();
    let superblock = &fields["btrfs_super_block"];

    for (field, offset) in [
        ("csum", 0),
        ("fsid", 32),
        ("bytenr", 48),
        ("flags", 56),
        ("magic", 64),
        ("generation", 72),
        ("root", 80),
        ("chunk_root", 88),
        ("total_bytes", 112),
        ("bytes_used", 120),
        ("sectorsize", 144),
        ("nodesize", 148),
        ("sys_chunk_array_size", 160),
        ("incompat_flags", 188),
        ("csum_type", 196),
        ("dev_item", 201),
    ] {
        assert_eq!(
            superblock[field], offset,
            "the header puts `{field}` at {}, not at {offset}",
            superblock[field]
        );
    }
}

#[test]
fn the_chunk_array_is_large_enough_for_the_system_chunks_that_go_in_it() {
    // A superblock carries the system chunks inline, and the field is 2 KiB. This
    // crate writes one, so there is a lot of room — but the failure mode if a
    // later change added system chunks is a write past the field and into the
    // backup roots, which is not the kind of thing to discover on a disk.
    let (_, fields) = all();
    let superblock = &fields["btrfs_super_block"];
    let array = superblock["super_roots"] - superblock["sys_chunk_array"];
    assert_eq!(array, 2048, "the system chunk array is not 2 KiB");

    // One key plus a two-stripe chunk item is what this crate puts there.
    let stripe = (1u64, 0u64, [0u8; 16]);
    let written = thalyx_btrfs::disk::Key::ENCODED_LEN
        + thalyx_btrfs::disk::chunk_item(0, 65536, 4096, 0, &[stripe, stripe]).len();
    assert!(written < array, "{written} bytes into a {array}-byte field");
}
