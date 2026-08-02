//! `thalyx-watch` — the kernel side of the index's freshness.
//!
//! `thalyx_watch.bpf.c` counts every filesystem mutation its hooks see, in a
//! BPF array map. This reads that counter and hands it to
//! `thalyx_graph::watch`, which owns all the reasoning about what may be
//! concluded from it.
//!
//! Read through `bpftool`, for the same reasons `thalyx-permd` writes policy
//! that way: no build-time dependency on kernel headers, and every value can
//! be checked by hand outside the program while debugging.
//!
//! ## What this cannot do yet, and why it is worth saying
//!
//! The counter is **machine-wide**. It records that something on this computer
//! changed, not that anything in the indexed tree did. So the shortcut it was
//! built for — "the count has not moved, skip the walk" — will essentially
//! never fire on a machine that is doing anything at all.
//!
//! Scoping it to a tree needs the *path* of each mutation, which is in the
//! ring buffer beside this counter and needs a consumer that mmaps the map and
//! follows the ring protocol. That is the next piece, and it is deliberately
//! not written blind: it can only be exercised on a machine with BPF.
//!
//! What is real today is the diagnosis — `thalyx graph status` can say how
//! much the kernel has seen — and the discipline around trusting it, which is
//! the part that would otherwise be got wrong later.

use std::path::{Path, PathBuf};
use thalyx_graph::MutationCounter;

/// Where the loader pins the watcher's counter.
pub const DEFAULT_MAP: &str = "/sys/fs/bpf/thalyx/maps/thalyx_mutation_count";

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("could not run bpftool: {0}")]
    Spawn(#[source] std::io::Error),

    #[error("the mutation counter is not pinned at {0}; is thalyx-watch loaded?")]
    NotPinned(PathBuf),

    #[error("bpftool failed: {0}")]
    Bpftool(String),

    #[error("could not make sense of what bpftool printed: {reason}\n  {output}")]
    Unreadable { reason: String, output: String },
}

/// The counter kept by `thalyx_watch.bpf.c`.
pub struct KernelCounter {
    map: PathBuf,
    bpftool: PathBuf,
    use_sudo: bool,
}

impl KernelCounter {
    pub fn new(map: impl Into<PathBuf>) -> Self {
        Self {
            map: map.into(),
            bpftool: PathBuf::from("bpftool"),
            use_sudo: !running_as_root(),
        }
    }

    pub fn default_map() -> Self {
        Self::new(DEFAULT_MAP)
    }

    pub fn with_bpftool(mut self, path: impl Into<PathBuf>) -> Self {
        self.bpftool = path.into();
        self
    }

    pub fn map(&self) -> &Path {
        &self.map
    }

    /// Whether the watcher is loaded at all.
    pub fn is_available(&self) -> bool {
        self.dump().is_ok()
    }

    fn dump(&self) -> std::result::Result<String, WatchError> {
        let map = self.map.to_string_lossy().into_owned();
        let args = ["map", "dump", "pinned", &map, "-j"];

        let mut command = if self.use_sudo {
            let mut c = std::process::Command::new("sudo");
            c.arg(&self.bpftool);
            c
        } else {
            std::process::Command::new(&self.bpftool)
        };
        let output = command.args(args).output().map_err(WatchError::Spawn)?;

        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if message.contains("No such file") {
                return Err(WatchError::NotPinned(self.map.clone()));
            }
            return Err(WatchError::Bpftool(message));
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl MutationCounter for KernelCounter {
    fn total(&self) -> thalyx_graph::Result<u64> {
        let output = self.dump().map_err(|error| thalyx_graph::GraphError::Io {
            path: self.map.clone(),
            source: std::io::Error::other(error.to_string()),
        })?;

        parse_total(&output).map_err(|error| thalyx_graph::GraphError::Io {
            path: self.map.clone(),
            source: std::io::Error::other(error.to_string()),
        })
    }

    /// Always false, today.
    ///
    /// Two reasons, and the second is the one that matters. The hook set is
    /// `inode_create`, `inode_unlink` and `inode_rename`, which does not see a
    /// write through an already-open descriptor. And the count is machine-wide
    /// rather than scoped to the indexed tree.
    ///
    /// Returning true here would switch on a shortcut that makes the index
    /// answer "current" for a tree it has no information about.
    fn claims_complete_coverage(&self) -> bool {
        false
    }
}

/// Pull the single value out of `bpftool map dump ... -j`.
///
/// A pure function over the text, so the one part of this that can be got
/// wrong without a kernel is the part that is tested.
///
/// bpftool prints map values as arrays of little-endian byte strings:
///
/// ```json
/// [{"key":["0x00","0x00","0x00","0x00"],
///   "value":["0x2a","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]}]
/// ```
pub fn parse_total(output: &str) -> std::result::Result<u64, WatchError> {
    let unreadable = |reason: &str| WatchError::Unreadable {
        reason: reason.to_string(),
        output: output.trim().to_string(),
    };

    // Deliberately not a JSON dependency: the shape is one array of one object
    // with one field of interest, and the bytes are what matter. Pulling the
    // value list out by hand keeps this crate's dependencies at nothing.
    let start = output
        .find("\"value\"")
        .ok_or_else(|| unreadable("no value field"))?;
    let open = output[start..]
        .find('[')
        .ok_or_else(|| unreadable("the value is not a byte array"))?;
    let close = output[start + open..]
        .find(']')
        .ok_or_else(|| unreadable("the value array is not closed"))?;

    let body = &output[start + open + 1..start + open + close];

    let mut bytes = [0u8; 8];
    let mut seen = 0usize;

    for field in body.split(',') {
        let field = field.trim().trim_matches('"');
        if field.is_empty() {
            continue;
        }
        let digits = field.strip_prefix("0x").unwrap_or(field);
        let byte =
            u8::from_str_radix(digits, 16).map_err(|_| unreadable("a value byte is not hex"))?;

        if seen >= bytes.len() {
            return Err(unreadable("the value is wider than the u64 it should be"));
        }
        bytes[seen] = byte;
        seen += 1;
    }

    if seen != bytes.len() {
        return Err(unreadable("the value is narrower than a u64"));
    }

    // Little-endian, matching how the kernel laid the u64 out in the map.
    Ok(u64::from_le_bytes(bytes))
}

/// Effective uid, read from procfs rather than through libc.
fn running_as_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("Uid:"))
                .and_then(|line| line.split_whitespace().nth(2))
                .and_then(|uid| uid.parse::<u32>().ok())
        })
        .is_some_and(|uid| uid == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_counter_reads_as_zero() {
        let output = r#"[{"key":["0x00","0x00","0x00","0x00"],
                          "value":["0x00","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]}]"#;
        assert_eq!(parse_total(output).unwrap(), 0);
    }

    #[test]
    fn the_bytes_are_read_little_endian() {
        // Getting the order wrong does not fail. It reports a number that is
        // wrong by a factor of billions, and every comparison against a
        // baseline silently means something else.
        let output = r#"[{"key":["0x00","0x00","0x00","0x00"],
                          "value":["0x2a","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]}]"#;
        assert_eq!(parse_total(output).unwrap(), 42);

        let output = r#"[{"key":["0x00","0x00","0x00","0x00"],
                          "value":["0x00","0x01","0x00","0x00","0x00","0x00","0x00","0x00"]}]"#;
        assert_eq!(parse_total(output).unwrap(), 256);
    }

    #[test]
    fn a_large_count_survives_the_full_width() {
        let output = r#"[{"key":["0x00","0x00","0x00","0x00"],
                          "value":["0xff","0xff","0xff","0xff","0xff","0xff","0xff","0xff"]}]"#;
        assert_eq!(parse_total(output).unwrap(), u64::MAX);
    }

    #[test]
    fn a_value_of_the_wrong_width_is_refused_rather_than_padded() {
        // A short value would otherwise be read as a small number, which looks
        // exactly like "the counter went backwards" and would break coverage
        // for a reason that was never real.
        let short = r#"[{"key":["0x00"],"value":["0x01","0x00"]}]"#;
        assert!(matches!(
            parse_total(short),
            Err(WatchError::Unreadable { .. })
        ));

        let long = r#"[{"key":["0x00"],
                        "value":["0x01","0x00","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]}]"#;
        assert!(matches!(
            parse_total(long),
            Err(WatchError::Unreadable { .. })
        ));
    }

    #[test]
    fn anything_that_is_not_a_dump_is_refused() {
        for output in [
            "",
            "Error: bpf obj get (/sys/fs/bpf/x): No such file or directory",
            r#"[{"key":["0x00"]}]"#,
            r#"[{"key":["0x00"],"value":["zz","00","00","00","00","00","00","00"]}]"#,
        ] {
            assert!(
                parse_total(output).is_err(),
                "`{output}` should not parse as a counter"
            );
        }
    }

    #[test]
    fn the_kernel_counter_never_claims_coverage_it_does_not_have() {
        // The hook set misses writes through an open descriptor, and the count
        // is machine-wide rather than scoped to the indexed tree. Claiming
        // otherwise would switch on a shortcut that makes the index answer
        // "current" about a tree it knows nothing about.
        assert!(!KernelCounter::default_map().claims_complete_coverage());
    }
}
