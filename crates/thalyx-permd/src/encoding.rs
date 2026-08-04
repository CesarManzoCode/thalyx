//! Byte encoding of map keys and values.
//!
//! The kernel reads these bytes as C structures. Getting the layout or the
//! byte order wrong does not fail loudly — it silently grants the wrong
//! permissions, which is the worst possible way for an encoding bug to
//! behave. Hence a module of its own, with tests that pin the exact bytes.

use crate::Policy;

/// A cgroup id as the map key: a `__u64`, native byte order.
///
/// The map is written and read on the same machine, so native order is
/// correct — and on every platform Thalyx targets that is little-endian. It is
/// spelled explicitly rather than left to a cast so the assumption is visible.
pub fn cgroup_key_bytes(cgroup_id: u64) -> [u8; 8] {
    cgroup_id.to_ne_bytes()
}

/// A policy as the map value.
///
/// Mirrors `struct policy` in the BPF program:
///
/// ```c
/// struct policy {
///     __u32 allowed;
///     __u32 flags;
///     __u64 expires_ns;
/// };
/// ```
///
/// No padding is needed: two 32-bit fields fill the first eight bytes, so the
/// 64-bit field is already aligned. If a field is ever added, the alignment
/// has to be rechecked on both sides — the compiler will insert padding
/// silently and the kernel will read a shifted structure.
pub fn policy_bytes(policy: &Policy) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&policy.allowed.to_ne_bytes());
    bytes[4..8].copy_from_slice(&policy.flags.to_ne_bytes());
    bytes[8..16].copy_from_slice(&policy.expires_ns.to_ne_bytes());
    bytes
}

/// How many bytes a policy occupies in the map.
pub const POLICY_BYTES: usize = 16;

/// The inverse of [`policy_bytes`], for reading an entry back.
///
/// Native byte order on both sides, because the writer and the reader are the
/// same machine — the map is in its kernel, not on a wire.
pub fn policy_from_bytes(bytes: &[u8; POLICY_BYTES]) -> Policy {
    Policy {
        allowed: u32::from_ne_bytes(bytes[0..4].try_into().expect("four bytes")),
        flags: u32::from_ne_bytes(bytes[4..8].try_into().expect("four bytes")),
        expires_ns: u64::from_ne_bytes(bytes[8..16].try_into().expect("eight bytes")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_policy_encodes_to_the_layout_the_kernel_reads() {
        let policy = Policy {
            allowed: 6,
            flags: 0,
            expires_ns: 0,
        };
        let bytes = policy_bytes(&policy);

        // This is byte for byte what the enforcement demonstration wrote by
        // hand on real hardware and what the kernel accepted.
        assert_eq!(bytes, [0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn every_field_lands_where_the_c_struct_expects_it() {
        let policy = Policy {
            allowed: 0x01020304,
            flags: 0x05060708,
            expires_ns: 0x090a0b0c0d0e0f10,
        };
        let bytes = policy_bytes(&policy);

        assert_eq!(&bytes[0..4], &0x01020304u32.to_ne_bytes());
        assert_eq!(&bytes[4..8], &0x05060708u32.to_ne_bytes());
        assert_eq!(&bytes[8..16], &0x090a0b0c0d0e0f10u64.to_ne_bytes());
    }

    #[test]
    fn the_value_is_exactly_the_size_of_the_c_struct() {
        // A mismatch here is rejected by the kernel at map update time, which
        // is the one encoding mistake that does fail loudly. Keeping the
        // assertion anyway makes the intent explicit.
        assert_eq!(policy_bytes(&Policy::default()).len(), 16);
        assert_eq!(cgroup_key_bytes(0).len(), 8);
    }

    #[test]
    fn a_cgroup_id_round_trips() {
        let id = 26_556u64; // the id from the first successful enforcement run
        assert_eq!(u64::from_ne_bytes(cgroup_key_bytes(id)), id);
    }

    #[test]
    fn a_policy_survives_the_round_trip_through_the_map() {
        // The bytes go into the kernel and come back out of it, and nothing
        // in between says what they mean. A field written at one offset and
        // read at another produces a policy that is plausible and wrong —
        // which is a permission granted for something nobody asked about.
        let policy = Policy {
            allowed: 0x2,
            flags: 0x1,
            expires_ns: 1_700_000_000_000_000_000,
        };
        assert_eq!(policy_from_bytes(&policy_bytes(&policy)), policy);
    }
}
