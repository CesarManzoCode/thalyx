//! Every size and offset this crate writes, checked against the kernel's own
//! headers rather than against a reading of them.
//!
//! Four files, captured verbatim:
//!
//! | in this directory | in Linux |
//! |---|---|
//! | `uapi_efi.h` | `block/partitions/efi.h` |
//! | `uapi_msdos_fs.h` | `include/uapi/linux/msdos_fs.h` |
//! | `linux_uuid.h` | `include/linux/uuid.h` |
//! | `uapi_fs.h` | `include/uapi/linux/fs.h` |
//!
//! Rule 6 of `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md` asks for one real
//! sample when parsing another tool's format. A writer needs it more than a parser
//! does: a parser that misreads a field gets a wrong answer, and a writer that
//! misplaces one produces a table nothing will read — and a GPT with a wrong
//! checksum is not reported as broken, it is *ignored*, so the disk comes back
//! looking as though nothing was written to it at all.
//!
//! `linux_uuid.h` is here for one reason and it is worth stating: `efi_guid_t` is
//! not defined in `efi.h`. `include/linux/efi.h` line 75 says
//! `typedef guid_t efi_guid_t __aligned(__alignof__(u32));`, and `guid_t` is in this
//! file as sixteen bytes. That one typedef is the only link in this chain that is
//! written down here rather than read from a captured file.
//!
//! ## The parser is graded before anything is measured with it
//!
//! Rule 5, and this project has been bitten by it eight times. The parser below is
//! small and specific and could be silently wrong, so four structs whose size the
//! headers state **in their own text** are checked first:
//! `legacy_mbr` against `SECTOR_SIZE`, `fat_boot_fsinfo` against `SECTOR_SIZE`,
//! `msdos_dir_entry` against `MSDOS_DIR_BITS` (which the header defines as its
//! log₂), and `guid_t` against `UUID_SIZE`. Between them they exercise arrays,
//! nested structs, bitfields, comma-separated declarators and constant bounds —
//! every path the real measurements use.
//!
//! ## What this file cannot establish
//!
//! That a kernel reads what comes out. Stage 20 of `dev/verify.sh` is where the
//! kernel is handed the table and asked what partitions it found.

use std::collections::HashMap;

fn captured(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{name} is part of the repository"))
}

/// Everything a C compiler would see, with the comments gone.
///
/// Stripped up front rather than skipped line by line. `thalyx-btrfs`'s parser skips
/// them line by line and that is where its one defect was: a field with a trailing
/// comment was dropped in silence, and the struct came out 96 bytes short.
fn text() -> String {
    let joined = [
        captured("uapi_efi.h"),
        captured("uapi_msdos_fs.h"),
        captured("linux_uuid.h"),
    ]
    .join("\n");

    let mut out = String::with_capacity(joined.len());
    let bytes: Vec<char> = joined.chars().collect();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == '/' && at + 1 < bytes.len() && bytes[at + 1] == '*' {
            at += 2;
            while at + 1 < bytes.len() && !(bytes[at] == '*' && bytes[at + 1] == '/') {
                at += 1;
            }
            at += 2;
            out.push(' ');
            continue;
        }
        if bytes[at] == '/' && at + 1 < bytes.len() && bytes[at + 1] == '/' {
            while at < bytes.len() && bytes[at] != '\n' {
                at += 1;
            }
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    out
}

/// `#define NAME <integer>` in whatever base the header states it.
fn constants(text: &str) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("#define ") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let (Some(name), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        // `MSDOS_DPS` and friends are expressions; only plain integers are wanted.
        let value = value.trim_end_matches("ULL").trim_end_matches("UL");
        let parsed = match value.strip_prefix("0x") {
            Some(digits) => u64::from_str_radix(digits, 16),
            None => value.parse::<u64>(),
        };
        if let Ok(number) = parsed {
            out.insert(name.to_string(), number);
        }
    }
    out
}

struct Header {
    text: String,
    constants: HashMap<String, u64>,
    sizes: HashMap<String, usize>,
}

/// A parsed struct: every field's offset, and the whole thing's size.
struct Shape {
    offsets: HashMap<String, usize>,
    size: usize,
}

impl Shape {
    /// The offset of a field, refusing rather than defaulting to zero.
    ///
    /// A missing field coming back as 0 would make every assertion about the first
    /// field pass for a parser that had stopped working entirely.
    fn at(&self, field: &str) -> usize {
        *self
            .offsets
            .get(field)
            .unwrap_or_else(|| panic!("the parser found no field called `{field}`"))
    }
}

impl Header {
    fn new() -> Self {
        let text = text();
        let constants = constants(&text);
        Self {
            text,
            constants,
            sizes: HashMap::new(),
        }
    }

    /// The width of one C type.
    fn width(&self, kind: &str) -> Option<usize> {
        let kind = kind.trim_start_matches("struct ").trim();
        Some(match kind {
            "__u8" | "u8" | "char" | "__s8" | "__le8" => 1,
            "__le16" | "__u16" | "u16" | "__s16" | "unsigned short" => 2,
            "__le32" | "__u32" | "u32" | "__s32" => 4,
            "__le64" | "__u64" | "u64" | "__s64" => 8,
            // The one link not read from a captured file. See the module comment.
            "efi_guid_t" => *self.sizes.get("guid_t")?,
            other => *self.sizes.get(other)?,
        })
    }

    /// An array bound: an integer, a constant the header defines, or the
    /// `72/sizeof(__le16)` form `gpt_entry` uses for its name field.
    fn bound(&self, expression: &str) -> Option<usize> {
        let expression = expression.trim();
        if let Ok(number) = expression.parse::<usize>() {
            return Some(number);
        }
        if let Some((left, right)) = expression.split_once('/')
            && let Some(kind) = right.trim().strip_prefix("sizeof(")
        {
            let kind = kind.trim_end_matches(')');
            return Some(left.trim().parse::<usize>().ok()? / self.width(kind)?);
        }
        self.constants.get(expression).map(|value| *value as usize)
    }

    /// The body of a struct, by any of the three shapes these headers use it in:
    /// `struct NAME {`, `typedef struct _NAME {` and an anonymous
    /// `typedef struct { … } NAME;`.
    fn body(&self, name: &str) -> Option<&str> {
        let bytes: Vec<char> = self.text.chars().collect();
        let mut search = 0usize;
        while let Some(found) = self.text[search..].find("struct") {
            let start = search + found;
            search = start + 6;
            // The tag, if there is one, then the opening brace.
            let after = &self.text[start + 6..];
            let Some(brace) = after.find('{') else { break };
            let tag = after[..brace].trim();
            if tag.contains(';') || tag.contains('}') {
                continue; // a forward declaration or a use, not a definition
            }

            // Match the braces to find where the body ends.
            let open = start + 6 + brace;
            let mut depth = 0usize;
            let mut close = open;
            for (index, character) in bytes.iter().enumerate().skip(open) {
                match character {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = index;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if close == open {
                continue;
            }
            let trailing = self.text[close + 1..]
                .split(';')
                .next()
                .unwrap_or_default()
                .replace("__packed", " ");
            let named = tag == name
                || tag == format!("_{name}")
                || trailing.split_whitespace().any(|word| word == name);
            if named {
                return Some(&self.text[open + 1..close]);
            }
        }
        None
    }

    /// Parse a struct, following `select` into any union it contains.
    ///
    /// `select` names the branch of a union to take — `fat_boot_sector` holds the
    /// FAT16 and FAT32 fields as alternatives and this crate writes the second. The
    /// union's *size* is the largest branch, which is what a compiler would do; its
    /// field offsets are the selected branch's.
    fn parse(&self, name: &str, select: &str) -> Shape {
        let body = self
            .body(name)
            .unwrap_or_else(|| panic!("no struct `{name}` in the captured headers"));
        let (offsets, size) = self.parse_body(body, select, 0);
        Shape { offsets, size }
    }

    fn parse_body(&self, body: &str, select: &str, base: usize) -> (HashMap<String, usize>, usize) {
        let mut offsets = HashMap::new();
        let mut at = base;
        let mut bits = 0usize;

        let characters: Vec<char> = body.chars().collect();
        let mut index = 0usize;
        let mut statement = String::new();

        while index < characters.len() {
            let character = characters[index];
            if character == '{' {
                // A union or a nested anonymous struct. Take the whole thing.
                let mut depth = 0usize;
                let mut close = index;
                for (position, inner) in characters.iter().enumerate().skip(index) {
                    match inner {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                close = position;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let inner: String = characters[index + 1..close].iter().collect();
                let keyword = statement.trim().to_string();
                statement.clear();

                if keyword.starts_with("union") {
                    // Every branch, so the union's size is the largest of them and
                    // the offsets come from the one asked for.
                    let mut largest = 0usize;
                    let mut branch_offsets = HashMap::new();
                    for branch in split_branches(&inner) {
                        let (fields, size) = self.parse_body(&branch.body, select, at);
                        largest = largest.max(size - at);
                        if branch.name == select {
                            branch_offsets = fields;
                        }
                    }
                    assert!(
                        !branch_offsets.is_empty(),
                        "no branch called `{select}` in a union that has \
                         {:?}",
                        split_branches(&inner)
                            .iter()
                            .map(|b| b.name.clone())
                            .collect::<Vec<_>>()
                    );
                    offsets.extend(branch_offsets);
                    at += largest;
                } else {
                    let (fields, size) = self.parse_body(&inner, select, at);
                    offsets.extend(fields);
                    at = size;
                }
                // Skip past the trailing declarator and its semicolon.
                index = close + 1;
                while index < characters.len() && characters[index] != ';' {
                    index += 1;
                }
                index += 1;
                continue;
            }
            if character == ';' {
                let declaration = std::mem::take(&mut statement);
                self.field(&declaration, &mut offsets, &mut at, &mut bits);
                index += 1;
                continue;
            }
            statement.push(character);
            index += 1;
        }

        assert_eq!(bits % 8, 0, "a bitfield run did not end on a byte boundary");
        (offsets, at + bits / 8)
    }

    /// One declaration: `__le64 my_lba`, `__le16 time,date,start`,
    /// `u64 reserved:47`, `__u8 vol_label[MSDOS_NAME]`.
    fn field(
        &self,
        declaration: &str,
        offsets: &mut HashMap<String, usize>,
        at: &mut usize,
        bits: &mut usize,
    ) {
        let declaration = declaration.trim();
        if declaration.is_empty() || declaration.starts_with('#') {
            return;
        }

        if let Some((left, width)) = declaration.split_once(':') {
            // A bitfield. Its position inside the run does not matter to anything
            // this crate writes; what matters is that the run adds up to whole
            // bytes, which `parse_body` asserts.
            let name = left.split_whitespace().last().unwrap_or_default();
            offsets.insert(name.to_string(), *at + *bits / 8);
            *bits += width.trim().parse::<usize>().unwrap_or(0);
            return;
        }
        assert_eq!(*bits, 0, "a plain field followed a partial bitfield run");

        let words: Vec<&str> = declaration.split_whitespace().collect();
        if words.len() < 2 {
            return;
        }
        let kind = words[..words.len() - 1].join(" ");
        let Some(width) = self.width(&kind) else {
            panic!("the parser does not know how wide `{kind}` is");
        };

        for declarator in words[words.len() - 1].split(',') {
            let declarator = declarator.trim();
            let (name, count) = match declarator.split_once('[') {
                Some((name, bound)) => {
                    let bound = bound.trim_end_matches(']');
                    let count = self
                        .bound(bound)
                        .unwrap_or_else(|| panic!("the parser cannot resolve the bound `{bound}`"));
                    (name, count)
                }
                None => (declarator, 1),
            };
            offsets.insert(name.to_string(), *at);
            *at += width * count;
        }
    }
}

struct Branch {
    name: String,
    body: String,
}

/// The `struct { … } fat16;` alternatives inside a union.
fn split_branches(inner: &str) -> Vec<Branch> {
    let characters: Vec<char> = inner.chars().collect();
    let mut branches = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        if characters[index] == '{' {
            let mut depth = 0usize;
            let mut close = index;
            for (position, character) in characters.iter().enumerate().skip(index) {
                match character {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = position;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body: String = characters[index + 1..close].iter().collect();
            let name: String = characters[close + 1..]
                .iter()
                .take_while(|character| **character != ';')
                .collect();
            branches.push(Branch {
                name: name.trim().to_string(),
                body,
            });
            index = close + 1;
            continue;
        }
        index += 1;
    }
    branches
}

/// Everything, parsed in dependency order.
fn headers() -> Header {
    let mut header = Header::new();
    for name in [
        "guid_t",
        "gpt_entry_attributes",
        "gpt_mbr_record",
        "gpt_header",
        "gpt_entry",
        "legacy_mbr",
        "fat_boot_fsinfo",
        "msdos_dir_entry",
    ] {
        let shape = header.parse(name, "fat32");
        header.sizes.insert(name.to_string(), shape.size);
    }
    header
}

// ───────────────────────────────────────────── the instrument, graded first

#[test]
fn the_parser_reproduces_four_sizes_the_headers_state_themselves() {
    // Rule 5. Before believing anything this file says about an offset, rule out
    // that the thing measuring it is wrong — and these headers can grade their own
    // reader, because each of these four sizes appears in their text as something
    // other than a struct definition.
    let header = headers();
    let sector = header.constants["SECTOR_SIZE"] as usize;

    // `legacy_mbr` is a boot sector: 440 + 4 + 2 + four 16-byte records + 2.
    // Exercises arrays and nested structs.
    assert_eq!(
        header.sizes["legacy_mbr"], sector,
        "the parser makes an MBR {} bytes and a sector is {sector}",
        header.sizes["legacy_mbr"]
    );

    // `fat_boot_fsinfo` is one sector too, and its 480-byte reserved array is the
    // largest bound the parser has to resolve.
    assert_eq!(header.sizes["fat_boot_fsinfo"], sector);

    // The header defines MSDOS_DIR_BITS as log₂ of this struct's size, in its own
    // words. Exercises the comma-separated declarators in `__le16 time,date,start`.
    assert_eq!(
        header.sizes["msdos_dir_entry"],
        1 << header.constants["MSDOS_DIR_BITS"]
    );

    // And the bound that comes from another captured file entirely.
    assert_eq!(
        header.sizes["guid_t"],
        header.constants["UUID_SIZE"] as usize
    );

    // The bitfield path, which none of the four above go through.
    assert_eq!(header.sizes["gpt_entry_attributes"], 8);
}

// ─────────────────────────────────────────────────────── the partition table

#[test]
fn the_gpt_header_fields_are_where_this_crate_writes_them() {
    // Every one of these is a literal offset in `gpt.rs`, because accumulating them
    // from a field list there would be a second copy of the layout with nothing
    // checking it. This is the thing that checks them.
    let header = headers();
    let shape = header.parse("gpt_header", "fat32");

    for (field, offset) in [
        ("signature", 0),
        ("revision", 8),
        ("header_size", 12),
        ("header_crc32", 16),
        ("my_lba", 24),
        ("alternate_lba", 32),
        ("first_usable_lba", 40),
        ("last_usable_lba", 48),
        ("disk_guid", 56),
        ("partition_entry_lba", 72),
        ("num_partition_entries", 80),
        ("sizeof_partition_entry", 84),
        ("partition_entry_array_crc32", 88),
    ] {
        assert_eq!(
            shape.at(field),
            offset,
            "the header puts `{field}` at {}, not at {offset}",
            shape.at(field)
        );
    }

    // And the size, which is what the checksum covers. A checksum over the whole
    // 512-byte sector instead of these 92 bytes is a number no reader reproduces,
    // and the disk comes back reporting no partition table at all.
    assert_eq!(shape.size, thalyx_install::gpt::HEADER_LEN);
}

#[test]
fn the_gpt_entry_fields_are_where_this_crate_writes_them() {
    let header = headers();
    let shape = header.parse("gpt_entry", "fat32");

    for (field, offset) in [
        ("partition_type_guid", 0),
        ("unique_partition_guid", 16),
        ("starting_lba", 32),
        ("ending_lba", 40),
        ("attributes", 48),
        ("partition_name", 56),
    ] {
        assert_eq!(shape.at(field), offset, "`{field}`");
    }
    assert_eq!(shape.size, thalyx_install::gpt::ENTRY_LEN);

    // The name field's length, taken from the header's own `72/sizeof(__le16)`
    // rather than from the 36 written in `Partition::encode`. A name one unit too
    // long overwrites nothing here — it runs into the next entry, and the next
    // entry is a partition.
    let units = header.bound("72/sizeof(__le16)").unwrap();
    assert_eq!(units, 36);
    assert_eq!(shape.size - shape.at("partition_name"), units * 2);
}

#[test]
fn the_protective_mbr_fields_are_where_this_crate_writes_them() {
    let header = headers();
    let mbr = header.parse("legacy_mbr", "fat32");
    let record = header.parse("gpt_mbr_record", "fat32");

    // 446 is written as a literal in `gpt.rs`, and it is the one number in that
    // function a reader has to take on trust.
    assert_eq!(mbr.at("partition_record"), 446);
    assert_eq!(record.at("os_type"), 4);
    assert_eq!(record.at("starting_lba"), 8);
    assert_eq!(record.at("size_in_lba"), 12);
    assert_eq!(record.size, 16);
    assert_eq!(mbr.at("signature"), 510);
}

#[test]
fn the_constants_in_the_table_are_the_ones_the_header_defines() {
    // Written out in `gpt.rs` because there is no C here to include the header from.
    // A wrong signature is the failure with no diagnosis: Linux does not report a
    // GPT it does not recognise, it reports a disk with no partitions.
    let header = headers();
    let expect = |name: &str| header.constants[name];

    assert_eq!(expect("GPT_HEADER_SIGNATURE"), 0x5452_4150_2049_4645);
    assert_eq!(expect("GPT_HEADER_REVISION_V1"), 0x0001_0000);
    assert_eq!(expect("MSDOS_MBR_SIGNATURE"), 0xaa55);
    assert_eq!(expect("EFI_PMBR_OSTYPE_EFI_GPT"), 0xEE);
    assert_eq!(expect("GPT_PRIMARY_PARTITION_TABLE_LBA"), 1);
}

#[test]
fn the_esp_type_guid_is_the_one_the_header_names_and_in_the_order_it_states() {
    // The single most consequential constant in this crate: firmware finds an EFI
    // system partition by this number and by nothing else, so a disk with every file
    // in the right place and this wrong boots nothing, with no message.
    //
    // Both halves are taken from captured text. The numbers come out of
    // `PARTITION_SYSTEM_GUID`'s `EFI_GUID(…)` in `efi.h`; the byte order is
    // `GUID_INIT`'s, which `linux_uuid.h` writes out in full — the first three
    // fields little-endian and the last eight bytes as they are. Getting that order
    // wrong produces a different, entirely valid-looking GUID.
    let header = headers();
    let arguments = header
        .text
        .split("#define PARTITION_SYSTEM_GUID")
        .nth(1)
        .and_then(|rest| rest.split("EFI_GUID(").nth(1))
        .and_then(|rest| rest.split(')').next())
        .expect("efi.h defines PARTITION_SYSTEM_GUID with EFI_GUID");

    // The macro is written across four lines with backslash continuations, which
    // are not part of any number.
    let arguments = arguments.replace('\\', " ");
    let numbers: Vec<u64> = arguments
        .split(',')
        .map(|word| {
            let word = word.trim();
            u64::from_str_radix(word.trim_start_matches("0x"), 16)
                .unwrap_or_else(|_| panic!("`{word}` is not a number"))
        })
        .collect();
    assert_eq!(
        numbers.len(),
        11,
        "EFI_GUID takes three fields and eight bytes"
    );

    let (a, b, c) = (numbers[0] as u32, numbers[1] as u16, numbers[2] as u16);
    let mut expected = [0u8; 16];
    expected[0..4].copy_from_slice(&a.to_le_bytes());
    expected[4..6].copy_from_slice(&b.to_le_bytes());
    expected[6..8].copy_from_slice(&c.to_le_bytes());
    for (index, byte) in numbers[3..].iter().enumerate() {
        expected[8 + index] = *byte as u8;
    }

    assert_eq!(
        thalyx_install::gpt::ESP.bytes(),
        expected,
        "the ESP type GUID is not what `efi.h` names"
    );
}

// ───────────────────────────────────────────────────── the boot filesystem

#[test]
fn the_boot_sector_fields_are_where_this_crate_writes_them() {
    // Twenty-odd literal offsets in `fat.rs`. The fat32 half of the union is the one
    // that matters and it starts at 36, which means every field in it is also a test
    // of the sixteen fields before it.
    let header = headers();
    let shape = header.parse("fat_boot_sector", "fat32");

    for (field, offset) in [
        ("ignored", 0),
        ("system_id", 3),
        ("sector_size", 11),
        ("sec_per_clus", 13),
        ("reserved", 14),
        ("fats", 16),
        ("dir_entries", 17),
        ("sectors", 19),
        ("media", 21),
        ("fat_length", 22),
        ("secs_track", 24),
        ("heads", 26),
        ("hidden", 28),
        ("total_sect", 32),
        // and the FAT32 branch of the union
        ("length", 36),
        ("flags", 40),
        ("version", 42),
        ("root_cluster", 44),
        ("info_sector", 48),
        ("backup_boot", 50),
        ("drive_number", 64),
        ("signature", 66),
        ("vol_id", 67),
        ("vol_label", 71),
        ("fs_type", 82),
    ] {
        assert_eq!(
            shape.at(field),
            offset,
            "the header puts `{field}` at {}, not at {offset}",
            shape.at(field)
        );
    }
}

#[test]
fn the_fsinfo_and_directory_fields_are_where_this_crate_writes_them() {
    let header = headers();

    let info = header.parse("fat_boot_fsinfo", "fat32");
    assert_eq!(info.at("signature1"), 0);
    assert_eq!(info.at("signature2"), 484);
    assert_eq!(info.at("free_clusters"), 488);
    assert_eq!(info.at("next_cluster"), 492);

    let entry = header.parse("msdos_dir_entry", "fat32");
    assert_eq!(entry.at("name"), 0);
    assert_eq!(entry.at("attr"), 11);
    assert_eq!(entry.at("lcase"), 12);
    assert_eq!(entry.at("ctime"), 14);
    assert_eq!(entry.at("cdate"), 16);
    assert_eq!(entry.at("adate"), 18);
    // The one that makes FAT32 different from FAT16, and the one a writer forgets:
    // a cluster number is split across two fields sixteen bytes apart. A file whose
    // high half was never written is found at a cluster in the first 64 KiB of the
    // volume, which on an ESP is inside the FAT.
    assert_eq!(entry.at("starthi"), 20);
    assert_eq!(entry.at("time"), 22);
    assert_eq!(entry.at("date"), 24);
    assert_eq!(entry.at("start"), 26);
    assert_eq!(entry.at("size"), 28);
    assert_eq!(entry.size, 32);
}

#[test]
fn the_fat_constants_this_crate_writes_are_the_ones_the_header_defines() {
    let header = headers();
    let expect = |name: &str| header.constants[name];

    // FAT32 begins one cluster above where FAT16 ends. Written in `fat.rs` as 65525
    // and derived here, because the boundary decides what filesystem a reader thinks
    // it is looking at — the boot sector's `FAT32   ` string does not.
    assert_eq!(expect("MAX_FAT16") + 1, 65525);
    assert_eq!(expect("MAX_FAT32"), 0x0FFF_FFF6);
    assert_eq!(expect("EOF_FAT32"), 0x0FFF_FFFF);
    assert_eq!(expect("FAT_START_ENT"), 2);
    assert_eq!(expect("MSDOS_NAME"), 11);

    assert_eq!(expect("ATTR_VOLUME"), 8);
    assert_eq!(expect("ATTR_DIR"), 16);
    assert_eq!(expect("ATTR_ARCH"), 32);

    assert_eq!(expect("FAT_FSINFO_SIG1"), 0x4161_5252);
    assert_eq!(expect("FAT_FSINFO_SIG2"), 0x6141_7272);

    // The sector size both this crate and the partition table assume.
    assert_eq!(expect("SECTOR_SIZE"), thalyx_install::fat::SECTOR);
    assert_eq!(expect("SECTOR_SIZE"), thalyx_install::gpt::SECTOR);
}
