//! CRC32C, and the two different ways Btrfs uses it.
//!
//! One primitive, two callers, and they do not agree about complementing — which
//! is the whole reason this is a module with a comment rather than a one-liner.
//! Both conventions were established by computing them against an image
//! `mkfs.btrfs` wrote, not by reading the kernel and believing the reading:
//! rule 6 of `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md` asks for one
//! captured real sample, and the first attempt at this file got the name hash
//! wrong in exactly the way a plausible reading of the source does.

/// Castagnoli, reflected. Not the polynomial of `crc32` — a filesystem
/// checksummed with the wrong one is a filesystem every block of which the
/// kernel reports as corrupt.
const POLYNOMIAL: u32 = 0x82F6_3B78;

/// The table, built once at first use.
///
/// `LazyLock` rather than a `const fn` table so the eight-shift derivation
/// stays visible: a 256-entry literal table is a thing nobody can check by
/// reading, and this one is checked by the tests below against a real image.
static TABLE: std::sync::LazyLock<[u32; 256]> = std::sync::LazyLock::new(|| {
    let mut table = [0u32; 256];
    for (index, entry) in table.iter_mut().enumerate() {
        let mut value = index as u32;
        for _ in 0..8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ POLYNOMIAL
            } else {
                value >> 1
            };
        }
        *entry = value;
    }
    table
});

/// The raw primitive: fold `data` into `state` and return the new state.
///
/// No initial or final complement. That is the caller's business, and the two
/// callers below differ about it — which is why this is exposed as the raw
/// thing rather than as something that has already decided.
fn fold(mut state: u32, data: &[u8]) -> u32 {
    let table = &*TABLE;
    for byte in data {
        state = table[usize::from((state ^ u32::from(*byte)) as u8)] ^ (state >> 8);
    }
    state
}

/// The checksum of a metadata block or a superblock.
///
/// Standard CRC32C: complement in, complement out. Btrfs reaches this through
/// the kernel's crypto shash, which is where the two complements come from.
///
/// Note what is *not* here: the range. A block's checksum covers the block from
/// the end of the checksum field to the end of the block, and choosing that
/// range is the caller's, because a caller that got it wrong by starting at zero
/// would produce a self-consistent checksum over a field that includes itself.
pub fn checksum(data: &[u8]) -> [u8; 4] {
    (fold(u32::MAX, data) ^ u32::MAX).to_le_bytes()
}

/// The hash Btrfs uses as the key offset of a directory entry.
///
/// The kernel calls the raw primitive with a seed of `~1` and **does not
/// complement the result** — it is `crc32c((u32)~1, name, len)` against the bare
/// `__crc32c_le`, not against the shash that [`checksum`] goes through. Applying
/// the standard convention here yields a number that looks entirely reasonable
/// and puts every directory entry under a key the kernel will not look for.
pub fn name_hash(name: &[u8]) -> u64 {
    u64::from(fold(!1u32, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_hash_matches_the_one_mkfs_btrfs_wrote() {
        // Captured from `btrfs inspect-internal dump-tree` on an image
        // mkfs.btrfs 6.6.3 wrote: the root tree's DIR_ITEM for the `default`
        // subvolume sits at key offset 2378154706.
        //
        // This is the number the first draft got wrong. It had complemented the
        // result, the way `checksum` must, and produced 1916812589 — a hash that
        // is stable, looks fine, and means the kernel resolves the default
        // subvolume by finding nothing.
        assert_eq!(name_hash(b"default"), 2_378_154_706);
    }

    #[test]
    fn a_block_checksum_is_crc32c_with_the_published_check_value() {
        // CRC32C of the nine ASCII digits, which is the check value the
        // Castagnoli parameters are published with. A vector rather than a real
        // block, because a real one means carrying 4 KiB of superblock — what
        // checks this against `mkfs.btrfs` is `tests/against_btrfs_progs.rs`,
        // which makes btrfs-progs verify a superblock this crate wrote.
        //
        // The wrong polynomial passes nothing here, which is the point: `crc32`
        // and `crc32c` differ only in that constant, and a filesystem summed
        // with the wrong one has every block reported corrupt.
        assert_eq!(checksum(b"123456789"), 0xE306_9283u32.to_le_bytes());
    }

    #[test]
    fn the_two_conventions_do_not_agree_and_that_is_the_point() {
        // If a later simplification unifies them, this fails. It exists because
        // "surely these are the same function" is exactly the thought that
        // produced the first wrong version.
        let name = b"default";
        let unified = u64::from(u32::from_le_bytes(checksum(name)));
        assert_ne!(name_hash(name), unified);
    }
}
