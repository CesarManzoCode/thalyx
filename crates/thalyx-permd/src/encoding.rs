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

/// Format bytes the way `bpftool map update ... key hex ...` expects.
pub fn as_hex_args(bytes: &[u8]) -> Vec<String> {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
    fn hex_arguments_are_two_digits_each() {
        // bpftool parses these positionally; a single-digit byte would shift
        // every later one and write a different policy than intended.
        let args = as_hex_args(&cgroup_key_bytes(5));
        assert_eq!(args, vec!["05", "00", "00", "00", "00", "00", "00", "00"]);
        assert!(args.iter().all(|a| a.len() == 2));
    }
}
