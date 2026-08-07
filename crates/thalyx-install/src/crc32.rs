//! CRC-32, the one a GPT header is checked with.
//!
//! **Not the CRC32C that `thalyx-btrfs` uses**, and the difference has already
//! cost this project a day in the other direction — `crate::crc32c` there records
//! a bug where one primitive was applied with two conventions and produced a
//! stable, plausible, wrong answer. Two checksums with the same name and different
//! polynomials in one repository is exactly that shape again, so this module says
//! which one it is in its first line and the test below pins it to the published
//! check value rather than to anything computed here.
//!
//! CRC-32/ISO-HDLC: polynomial 0x04C11DB7 reflected to 0xEDB88320, initial value
//! all ones, final complement. It is what zlib, PNG, Ethernet and the UEFI
//! specification all mean by "CRC32".
//!
//! ## Why this is written out rather than depended on
//!
//! Two hundred bytes against a crate, on a checksum whose correctness is checked
//! by one published constant. The dependency would not be wrong; it would be one
//! more thing in a binary that becomes an operating system.

/// The reflected polynomial, expanded once.
///
/// A `const fn` rather than a `static` built at startup: the table is the same on
/// every machine and nothing should be able to run before it exists.
const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 != 0 {
                0xEDB8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

/// The CRC-32 of `bytes`, as a GPT header records it.
pub fn of(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc = TABLE[((crc ^ u32::from(*byte)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_check_value_comes_back() {
        // 0xCBF43926 for "123456789" is the check value the CRC catalogue states
        // for CRC-32/ISO-HDLC. It is the whole reason this test is worth writing:
        // it is a number this file cannot have produced, so a table built with a
        // wrong polynomial or a missing final complement fails here rather than on
        // a disk that firmware will not read.
        assert_eq!(of(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn it_is_not_the_crc32c_the_btrfs_writer_uses() {
        // The same input under the two polynomials, asserted to differ. There are
        // two checksums in this repository called "CRC32" and using one where the
        // other belongs does not fail loudly: it writes a stable, plausible number
        // that the reader rejects much later. `thalyx-btrfs` has that story
        // recorded once already.
        let crc32c = u32::from_le_bytes(thalyx_btrfs::crc32c::checksum(b"123456789"));
        assert_ne!(of(b"123456789"), crc32c);
    }

    #[test]
    fn the_empty_input_is_zero_and_not_the_initial_value() {
        // A CRC that forgot its final complement answers 0xFFFFFFFF here, and every
        // other input would still look like a plausible checksum. This is the
        // cheapest place that mistake shows.
        assert_eq!(of(b""), 0);
    }
}
