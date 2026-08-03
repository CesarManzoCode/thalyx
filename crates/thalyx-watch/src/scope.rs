//! Narrowing the kernel's count from "this machine" to "this tree".
//!
//! The machine-wide counter is correct and nearly useless: it moves whenever
//! anything anywhere changes, so the shortcut it exists for — skip the walk
//! when nothing has changed — fires only on a machine nobody is using.
//!
//! `thalyx_watch.bpf.c` can do better. Given the identity of a tree's root, it
//! walks up from each mutation's dentry and counts the change against that
//! tree, or determines that it happened outside every watched tree and counts
//! it against none. This module is the userspace half: working out that
//! identity, checking the one precondition the kernel walk depends on, and
//! reading the result back.
//!
//! ## The precondition, and why it is checked rather than assumed
//!
//! The walk climbs `d_parent`, which never crosses a mount point — a file
//! reached through a mount lives on a different superblock and its walk stops
//! at *that* filesystem's root, having never seen the watched dentry. So a
//! change inside a mount inside a watched tree would be missed, and a missed
//! change is the one failure this design refuses.
//!
//! Hence [`mounts_under`]. A tree with anything mounted below it cannot be
//! scoped, and says so, and falls back to the machine-wide count. One read of
//! `/proc/mounts` turns an assumption into a precondition.

use std::path::{Path, PathBuf};

/// A tree root, as the kernel identifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchedRoot {
    /// The kernel's `s_dev`, not the encoded `st_dev` a stat returns.
    pub dev: u32,
    pub ino: u64,
}

impl WatchedRoot {
    /// Identify a directory the way `thalyx_watch.bpf.c` will see it.
    pub fn of(path: &Path) -> std::io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path)?;
        Ok(Self {
            dev: kernel_dev(metadata.dev()),
            ino: metadata.ino(),
        })
    }

    /// The exact bytes of `struct root_key`.
    ///
    /// `__u64 ino; __u32 dev; __u32 pad;` little-endian, padding explicitly
    /// zero. A BPF hash compares keys byte for byte, so a padding hole left
    /// uninitialised on either side would make the same root fail to match
    /// itself.
    pub fn key_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&self.ino.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.dev.to_le_bytes());
        bytes
    }
}

/// Convert the `st_dev` a stat returns into the kernel's internal `dev_t`.
///
/// These are not the same number, and nothing complains when they are confused
/// — the lookup simply never matches, and the tree's count sits at zero
/// forever, which reads as "nothing ever changed here".
///
/// The kernel encodes `st_dev` on the way out (`new_encode_dev`, splitting the
/// minor around the major) and this is the inverse (`new_decode_dev`), with
/// `MKDEV` putting them back together as major:20 | minor.
pub fn kernel_dev(st_dev: u64) -> u32 {
    let encoded = st_dev as u32;
    let major = (encoded & 0x000f_ff00) >> 8;
    let minor = (encoded & 0xff) | ((encoded >> 12) & 0x000f_ff00);
    (major << 20) | minor
}

/// Mount points that sit inside a tree.
///
/// Pure over the text of `/proc/mounts` so the rule can be tested without
/// arranging real mounts. Any result at all means the tree cannot be scoped:
/// the kernel's walk would never climb out of the mounted filesystem into the
/// watched one.
pub fn mounts_under(tree: &Path, proc_mounts: &str) -> Vec<PathBuf> {
    let tree = tree.to_string_lossy();
    // A prefix without the separator would make `/home/work` look like a mount
    // inside `/home/wo`.
    let prefix = if tree.ends_with('/') {
        tree.to_string()
    } else {
        format!("{tree}/")
    };

    proc_mounts
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(unescape)
        .filter(|point| point.starts_with(&prefix))
        .map(PathBuf::from)
        .collect()
}

/// `/proc/mounts` escapes space, tab, newline and backslash as octal.
///
/// A path left escaped compares unequal to the real one, so a mount under a
/// directory whose name has a space in it would go unnoticed — and going
/// unnoticed is exactly the failure this check exists to prevent.
fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();

    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let digits: String = chars.clone().take(3).collect();
        match u8::from_str_radix(&digits, 8) {
            Ok(byte) if digits.len() == 3 => {
                out.push(byte as char);
                chars.nth(2);
            }
            // A lone backslash is a legal character in a path.
            _ => out.push('\\'),
        }
    }

    out
}

/// Pull one key's value out of `bpftool map lookup ... -j`.
///
/// Absent is `Ok(None)`, not zero: a tree the kernel was never told to watch
/// and a tree nothing has happened in look identical as numbers, and only one
/// of them may be believed.
pub fn parse_lookup(output: &str) -> Option<u64> {
    let start = output.find("\"value\"")?;
    let rest = &output[start + "\"value\"".len()..];
    let after_colon = rest.trim_start().strip_prefix(':')?.trim_start();
    let close = after_colon.find(']')?;

    let mut bytes = [0u8; 8];
    let mut seen = 0usize;

    for field in after_colon.strip_prefix('[')?[..close - 1].split(',') {
        let field = field.trim().trim_matches('"');
        if field.is_empty() {
            continue;
        }
        let digits = field.strip_prefix("0x").unwrap_or(field);
        let byte = u8::from_str_radix(digits, 16).ok()?;
        if seen >= bytes.len() {
            return None;
        }
        bytes[seen] = byte;
        seen += 1;
    }

    (seen == bytes.len()).then(|| u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stat_device_becomes_the_number_the_kernel_holds() {
        // Major 8, minor 1 — the first partition of the first SCSI disk, and
        // the value `stat` prints as "Device 801h".
        //
        // Pinned because confusing the two encodings fails silently: the BPF
        // lookup never matches, the tree's count stays at zero, and zero reads
        // as "nothing has ever changed here".
        assert_eq!(kernel_dev(0x801), (8 << 20) | 1);

        // Major 259, minor 3 — an NVMe partition, where the major does not fit
        // in the low byte and the encoding stops being an identity.
        let encoded = 3u32 | (259 << 8);
        assert_eq!(kernel_dev(encoded as u64), (259 << 20) | 3);

        // A minor above 255, which is the case the split encoding exists for.
        let encoded = (0x123 & 0xff) | (8 << 8) | ((0x123 & !0xffu32) << 12);
        assert_eq!(kernel_dev(encoded as u64), (8 << 20) | 0x123);
    }

    #[test]
    fn the_key_bytes_match_the_struct_the_program_declares() {
        let root = WatchedRoot {
            dev: 0x0080_0001,
            ino: 0x1122_3344_5566_7788,
        };
        let bytes = root.key_bytes();

        assert_eq!(&bytes[..8], &0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(&bytes[8..12], &0x0080_0001u32.to_le_bytes());
        assert_eq!(
            &bytes[12..],
            &[0, 0, 0, 0],
            "the padding must be zero, or the same root hashes differently \
             every time it is looked up"
        );
    }

    #[test]
    fn a_tree_with_nothing_mounted_under_it_can_be_scoped() {
        let mounts = "\
/dev/nvme0n1p3 / btrfs rw,relatime 0 0
/dev/nvme0n1p3 /home btrfs rw,relatime 0 0
tmpfs /tmp tmpfs rw,nosuid 0 0
";
        assert!(mounts_under(Path::new("/home/user/project"), mounts).is_empty());
    }

    #[test]
    fn a_mount_inside_the_tree_is_found() {
        // The one thing that breaks the kernel's walk: it climbs d_parent,
        // which stops at the root of the mounted filesystem and never reaches
        // the watched dentry above it.
        let mounts = "\
/dev/nvme0n1p3 / btrfs rw 0 0
tmpfs /home/user/project/target tmpfs rw 0 0
";
        assert_eq!(
            mounts_under(Path::new("/home/user/project"), mounts),
            [PathBuf::from("/home/user/project/target")]
        );
    }

    #[test]
    fn the_tree_being_a_mount_point_itself_is_not_a_mount_inside_it() {
        // Perfectly scopable: the root of a subvolume is exactly the dentry
        // the walk is looking for.
        let mounts = "/dev/nvme0n1p3 /home/user/project btrfs rw 0 0\n";
        assert!(mounts_under(Path::new("/home/user/project"), mounts).is_empty());
    }

    #[test]
    fn a_sibling_with_a_longer_name_is_not_inside_the_tree() {
        // Without the separator, `/home/work` reads as a mount inside
        // `/home/wo`, and a perfectly scopable tree would be refused forever
        // for a reason nobody could find.
        let mounts = "tmpfs /home/workspace tmpfs rw 0 0\n";
        assert!(mounts_under(Path::new("/home/work"), mounts).is_empty());
    }

    #[test]
    fn an_escaped_mount_point_is_compared_unescaped() {
        // `/proc/mounts` writes a space as `\040`. Left escaped, a mount under
        // a directory whose name contains a space goes unnoticed — which is
        // precisely the miss this check exists to prevent.
        let mounts = "tmpfs /home/user/my\\040project/target tmpfs rw 0 0\n";
        assert_eq!(
            mounts_under(Path::new("/home/user/my project"), mounts),
            [PathBuf::from("/home/user/my project/target")]
        );
    }

    #[test]
    fn a_lone_backslash_in_a_path_survives() {
        assert_eq!(unescape("/home/a\\b"), "/home/a\\b");
        assert_eq!(unescape("/home/a\\040b"), "/home/a b");
    }

    #[test]
    fn a_lookup_reads_its_value_little_endian() {
        let output = r#"{"key":["0x01","0x00"],
                         "value":["0x2a","0x01","0x00","0x00","0x00","0x00","0x00","0x00"]}"#;
        assert_eq!(parse_lookup(output), Some(0x12a));
    }

    #[test]
    fn a_key_that_is_not_there_is_none_and_never_zero() {
        // A tree the kernel was never told to watch and a tree nothing has
        // happened in are the same number and completely different facts.
        assert_eq!(
            parse_lookup("Error: can't lookup element: No such file"),
            None
        );
        assert_eq!(parse_lookup(r#"{"key":["0x01"]}"#), None);
    }

    #[test]
    fn a_short_value_is_refused_rather_than_padded() {
        let output = r#"{"key":["0x01"],"value":["0x2a","0x00"]}"#;
        assert_eq!(parse_lookup(output), None);
    }
}
