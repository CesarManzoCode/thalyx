//! `thalyx-watch` — the kernel side of the index's freshness.
//!
//! `thalyx_watch.bpf.c` counts every filesystem mutation its hooks see, in a
//! per-CPU BPF map. This reads that counter and hands it to
//! `thalyx_graph::watch`, which owns all the reasoning about what may be
//! concluded from it.
//!
//! Read through `bpftool`, for the same reasons `thalyx-permd` writes policy
//! that way: no build-time dependency on kernel headers, and every value can
//! be checked by hand outside the program while debugging.
//!
//! ## What this covers, and what it still does not
//!
//! The hook set is complete for this machine: everything that creates,
//! removes, renames or rewrites a file passes through one of the programs in
//! [`REQUIRED_HOOKS`], including a write through a descriptor that was already
//! open when the watcher attached. Until that was true the counter could only
//! ever be decoration, because "the count has not moved" was compatible with a
//! file having been rewritten in place.
//!
//! Two things are still outside it, and neither is pretended away:
//!
//! - The counter is **machine-wide**. It records that something on this
//!   computer changed, not that anything in the indexed tree did. That is the
//!   safe direction — it costs walks that were not needed, never a missed
//!   change — but it means the shortcut fires only on a quiet machine.
//!   Scoping it to a tree is the next piece.
//! - A filesystem another machine can write changes with no hook firing here
//!   at all. No hook set closes that, which is why
//!   [`thalyx_graph::Trust::Counter`] stays an explicit choice and
//!   `Watcher::verify` has to agree first.

use std::path::{Path, PathBuf};
use thalyx_graph::MutationCounter;

/// Where the loader pins the watcher's counter.
pub const DEFAULT_MAP: &str = "/sys/fs/bpf/thalyx/maps/thalyx_mutation_count";

/// Every program `thalyx_watch.bpf.c` must have attached for the count to mean
/// "nothing changed".
///
/// Named individually rather than counted, because a missing one is not a
/// smaller number — it is a specific way a file can change in silence, and the
/// report should be able to say which.
pub const REQUIRED_HOOKS: &[&str] = &[
    "thalyx_create",
    "thalyx_unlink",
    "thalyx_rename",
    "thalyx_mkdir",
    "thalyx_rmdir",
    "thalyx_symlink",
    "thalyx_link",
    "thalyx_mknod",
    // The one that took the longest to be there, and the one that decides
    // whether any of this can be believed: writes through an open descriptor.
    "thalyx_write",
    "thalyx_setattr",
];

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
    ///
    /// Loaded, not readable. Whether `bpftool` could read the pinned map at
    /// all — parsing what it printed is a separate question, and a map that is
    /// pinned and full of counts which Thalyx fails to parse is a bug here,
    /// not an absent watcher.
    pub fn is_available(&self) -> bool {
        self.dump().is_ok()
    }

    /// Why the counter could not be read, when it could not.
    ///
    /// `Ok(None)` means it was read fine. Split out so a caller can tell "the
    /// watcher is not attached" — which is a thing to go and fix — from "it is
    /// attached and Thalyx could not make sense of it", which is a defect in
    /// Thalyx. Reporting the second as the first sends the human to reload
    /// something that was already working.
    pub fn unreadable(&self) -> Option<WatchError> {
        match self.dump() {
            Ok(output) => parse_total(&output).err(),
            Err(error) => Some(error),
        }
    }

    /// Which of [`REQUIRED_HOOKS`] are not loaded.
    ///
    /// Empty means the watcher sees every way a process on this machine can
    /// change a file. Anything in it names a hole.
    ///
    /// Read from `bpftool prog show` rather than from the pin directory: the
    /// pins say the object was written to bpffs, the program list says the
    /// kernel accepted it. The load is all-or-nothing — `bpftool prog loadall
    /// ... autoattach` fails the whole object if any one program cannot
    /// attach — so in practice this is all of them or none, and `make status`
    /// counts the live LSM links as the independent check.
    pub fn missing_hooks(&self) -> std::result::Result<Vec<&'static str>, WatchError> {
        let listing = self.bpftool(&["prog", "show", "-j"])?;
        Ok(missing_among(&program_names(&listing)))
    }

    fn dump(&self) -> std::result::Result<String, WatchError> {
        let map = self.map.to_string_lossy().into_owned();
        self.bpftool(&["map", "dump", "pinned", &map, "-j"])
    }

    fn bpftool(&self, args: &[&str]) -> std::result::Result<String, WatchError> {
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

/// Which of [`REQUIRED_HOOKS`] are absent from a list of loaded programs.
///
/// Split out from [`KernelCounter::missing_hooks`] so the rule can be tested
/// without a kernel. The rule is the part with a mistake available in it: a
/// prefix match, or a count instead of a set, would let nine hooks vouch for
/// ten.
pub fn missing_among(loaded: &[String]) -> Vec<&'static str> {
    REQUIRED_HOOKS
        .iter()
        .copied()
        .filter(|hook| !loaded.iter().any(|name| name == hook))
        .collect()
}

/// Every `"name"` in a `bpftool prog show -j` listing.
///
/// A pure function over the text, for the same reason [`parse_total`] is one:
/// the part that can be got wrong without a kernel is the part that gets
/// tested. Hand-rolled rather than parsed as JSON to keep this crate's
/// dependencies at nothing — it is one repeated field, not a document.
///
/// Names are truncated by the kernel to 15 characters, so the programs in
/// `thalyx_watch.bpf.c` are named to stay distinct within that limit. They
/// were not, once: `thalyx_inode_mkdir` and `thalyx_inode_mknod` both arrive
/// as `thalyx_inode_mk`, and one would have vouched for the other.
pub fn program_names(listing: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = listing;

    while let Some(at) = rest.find("\"name\":") {
        rest = &rest[at + "\"name\":".len()..];
        let Some(open) = rest.find('"') else { break };
        let Some(close) = rest[open + 1..].find('"') else {
            break;
        };
        names.push(rest[open + 1..open + 1 + close].to_string());
        rest = &rest[open + 1 + close..];
    }

    names
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

    /// True only when every hook in [`REQUIRED_HOOKS`] is loaded.
    ///
    /// Not a constant any more, and not an assertion: it is a question put to
    /// the kernel about what is actually attached, and it answers false for
    /// every reason it should — the watcher not loaded, a hook this kernel
    /// does not expose, `bpftool` missing, no permission to ask.
    ///
    /// Note what the claim covers. Every path by which a *process on this
    /// machine* can change a file passes through one of these hooks. It does
    /// not cover a filesystem some other machine can write. That is why this
    /// is only one of the two keys: [`thalyx_graph::Trust::Counter`] is still
    /// an explicit choice by the caller, and `Watcher::verify` still has to
    /// have agreed on the machine in question.
    ///
    /// The count being machine-wide is deliberately *not* a reason to answer
    /// false. Counting mutations outside the indexed tree costs walks that
    /// were not needed; it never hides one that was.
    fn claims_complete_coverage(&self) -> bool {
        matches!(self.missing_hooks(), Ok(missing) if missing.is_empty())
    }
}

/// Add up what `bpftool map dump ... -j` printed.
///
/// A pure function over the text, so the one part of this that can be got
/// wrong without a kernel is the part that is tested.
///
/// bpftool prints map values as arrays of little-endian byte strings. A plain
/// array map has one:
///
/// ```json
/// [{"key":["0x00","0x00","0x00","0x00"],
///   "value":["0x2a","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]}]
/// ```
///
/// A per-CPU map has one per core, under a `values` list:
///
/// ```json
/// [{"key":["0x00","0x00","0x00","0x00"],
///   "values":[{"cpu":0,"value":["0x2a","0x00", ...]},
///             {"cpu":1,"value":["0x07","0x00", ...]}]}]
/// ```
///
/// The counter became per-CPU when `file_permission` joined the hook set — one
/// contended cacheline in the write path of every core was not affordable. So
/// the total is the sum, and this reads both shapes: `"values"` does not match
/// a search for `"value"` including its closing quote, so the per-CPU entries
/// are found the same way the single value was.
///
/// The sum is not taken atomically across cores. It does not need to be. Any
/// read lands between the true total when it started and the true total when
/// it finished, so a later read is never smaller than an earlier one — and
/// monotonicity is the only property the freshness logic rests on.
pub fn parse_total(output: &str) -> std::result::Result<u64, WatchError> {
    let unreadable = |reason: &str| WatchError::Unreadable {
        reason: reason.to_string(),
        output: output.trim().to_string(),
    };

    // Deliberately not a JSON dependency: the shape is one repeated field and
    // the bytes are what matter. Pulling the value lists out by hand keeps
    // this crate's dependencies at nothing.
    let mut total: u64 = 0;
    let mut found = 0usize;
    let mut rest = output;

    while let Some(start) = rest.find("\"value\"") {
        rest = &rest[start + "\"value\"".len()..];

        // The bracket has to be the *next* thing, not the next one anywhere
        // ahead. bpftool prints the same numbers twice: once as byte arrays and
        // again under `formatted`, where a value is a plain integer —
        // `"value":1676`. Searching forward for a `[` from there finds the
        // bracket of some later entry and reads a number that belongs to
        // something else, and for the final one there is no bracket left at
        // all, which is how a working watcher reported itself unreadable.
        let after_colon = match rest.trim_start().strip_prefix(':') {
            Some(tail) => tail.trim_start(),
            None => continue,
        };
        if !after_colon.starts_with('[') {
            // The human-readable view of a value already counted. Skipping it
            // is why this is not "sum every value in the document".
            rest = after_colon;
            continue;
        }

        let close = after_colon
            .find(']')
            .ok_or_else(|| unreadable("the value array is not closed"))?;

        let body = &after_colon[1..close];
        rest = &after_colon[close..];

        let mut bytes = [0u8; 8];
        let mut seen = 0usize;

        for field in body.split(',') {
            let field = field.trim().trim_matches('"');
            if field.is_empty() {
                continue;
            }
            let digits = field.strip_prefix("0x").unwrap_or(field);
            let byte = u8::from_str_radix(digits, 16)
                .map_err(|_| unreadable("a value byte is not hex"))?;

            if seen >= bytes.len() {
                return Err(unreadable("a value is wider than the u64 it should be"));
            }
            bytes[seen] = byte;
            seen += 1;
        }

        if seen != bytes.len() {
            return Err(unreadable("a value is narrower than a u64"));
        }

        // Little-endian, matching how the kernel laid the u64 out in the map.
        // Saturating rather than wrapping: a total that wrapped would look
        // like the counter went backwards, which userspace reads as a reload
        // and therefore as a break in coverage — the cautious answer, and the
        // wrong reason. Saturating keeps it monotonic.
        total = total.saturating_add(u64::from_le_bytes(bytes));
        found += 1;
    }

    if found == 0 {
        return Err(unreadable("no value field"));
    }

    Ok(total)
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
    fn a_per_cpu_counter_is_the_sum_of_its_slots() {
        // The shape bpftool prints for BPF_MAP_TYPE_PERCPU_ARRAY. Reading only
        // the first slot would report a fraction of the real total — and a
        // total that is too small looks exactly like "less has changed", which
        // is the one direction of error this whole design refuses.
        let output = r#"[{"key":["0x00","0x00","0x00","0x00"],
            "values":[
                {"cpu":0,"value":["0x0a","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]},
                {"cpu":1,"value":["0x14","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]},
                {"cpu":2,"value":["0x00","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]},
                {"cpu":3,"value":["0x01","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]}
            ]}]"#;
        // cpu2 is idle and contributes nothing, which is the normal case for
        // most cores and must not be mistaken for the end of the list.
        assert_eq!(parse_total(output).unwrap(), 10 + 20 + 1);
    }

    #[test]
    fn the_output_a_real_bpftool_actually_prints() {
        // Verbatim from the first run of the ten-hook watcher on real hardware,
        // where the crafted fixtures above all passed and this failed.
        //
        // bpftool prints the same numbers twice: the byte arrays, and then a
        // `formatted` block where a value is a plain integer. The first parser
        // searched forward for the next `[` after each `"value"`, so the
        // formatted entries read brackets belonging to other entries, and the
        // last one found none at all — a watcher that was counting perfectly
        // reported itself unreadable, which then read as "not loaded".
        let output = r#"[{"key":["0x00","0x00","0x00","0x00"],"values":[{"cpu":0,"value":["0x8c","0x06","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":1,"value":["0xfb","0x06","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":2,"value":["0x63","0x08","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":3,"value":["0x62","0x06","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":4,"value":["0x18","0x08","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":5,"value":["0xc7","0x06","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":6,"value":["0xa7","0x06","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":7,"value":["0x17","0x0a","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":8,"value":["0xe0","0x05","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":9,"value":["0x31","0x06","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":10,"value":["0x38","0x05","0x00","0x00","0x00","0x00","0x00","0x00"]},{"cpu":11,"value":["0xf6","0x08","0x00","0x00","0x00","0x00","0x00","0x00"]}],"formatted":{"key":0,"values":[{"cpu":0,"value":1676},{"cpu":1,"value":1787},{"cpu":2,"value":2147},{"cpu":3,"value":1634},{"cpu":4,"value":2072},{"cpu":5,"value":1735},{"cpu":6,"value":1703},{"cpu":7,"value":2583},{"cpu":8,"value":1504},{"cpu":9,"value":1585},{"cpu":10,"value":1336},{"cpu":11,"value":2294}]}}]"#;

        // The sum of the twelve, counted once. bpftool's own formatted view is
        // the arithmetic check: 1676 + 1787 + ... + 2294.
        assert_eq!(parse_total(output).unwrap(), 22056);
    }

    #[test]
    fn a_formatted_view_with_no_byte_arrays_is_not_a_counter() {
        // The other half of skipping the formatted block: skipping must not
        // become "read it anyway if it is all that is there". Those integers
        // are decimal, and reading them as hex bytes would be a number wrong
        // by orders of magnitude with nothing to signal it.
        let output = r#"[{"key":0,"formatted":{"key":0,"values":[{"cpu":0,"value":1676}]}}]"#;
        assert!(matches!(
            parse_total(output),
            Err(WatchError::Unreadable { .. })
        ));
    }

    #[test]
    fn the_plural_values_field_is_not_mistaken_for_a_value() {
        // `"values"` contains `"value"`. Searching for the shorter one without
        // its closing quote would match the list header and then parse the
        // list of objects as if it were a list of bytes.
        let output = r#"[{"key":["0x00","0x00","0x00","0x00"],
            "values":[{"cpu":0,"value":["0x07","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]}]}]"#;
        assert_eq!(parse_total(output).unwrap(), 7);
    }

    #[test]
    fn a_single_valued_map_still_reads_the_same_way() {
        // The counter was a plain array before it was per-CPU. One reader for
        // both shapes means an older pinned map is read correctly rather than
        // rejected — and a rejected read is indistinguishable from a watcher
        // that is not loaded.
        let output = r#"[{"key":["0x00","0x00","0x00","0x00"],
                          "value":["0x2a","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]}]"#;
        assert_eq!(parse_total(output).unwrap(), 42);
    }

    #[test]
    fn one_unreadable_slot_fails_the_whole_read() {
        // Not "sum what parsed". A partial sum is a smaller number, and a
        // smaller number is a claim that less changed.
        let output = r#"[{"key":["0x00","0x00","0x00","0x00"],
            "values":[
                {"cpu":0,"value":["0x0a","0x00","0x00","0x00","0x00","0x00","0x00","0x00"]},
                {"cpu":1,"value":["0x14","0x00"]}
            ]}]"#;
        assert!(matches!(
            parse_total(output),
            Err(WatchError::Unreadable { .. })
        ));
    }

    #[test]
    fn names_come_out_of_a_program_listing() {
        let listing = r#"[{"id":30,"type":"lsm","name":"thalyx_write","tag":"a1"},
                          {"id":31,"type":"lsm","name":"thalyx_setattr","tag":"b2"}]"#;
        assert_eq!(program_names(listing), ["thalyx_write", "thalyx_setattr"]);
    }

    #[test]
    fn a_listing_with_no_names_yields_none_rather_than_failing() {
        assert!(program_names("[]").is_empty());
        assert!(program_names("").is_empty());
    }

    #[test]
    fn every_hook_present_is_the_only_way_to_have_no_holes() {
        let all: Vec<String> = REQUIRED_HOOKS.iter().map(|h| h.to_string()).collect();
        assert!(missing_among(&all).is_empty());
    }

    #[test]
    fn the_write_hook_missing_is_reported_by_name() {
        // The specific hole that made the counter undecidable for months: a
        // watcher with every other hook attached still cannot see a file being
        // rewritten through a descriptor it already had open.
        let loaded: Vec<String> = REQUIRED_HOOKS
            .iter()
            .filter(|hook| **hook != "thalyx_write")
            .map(|h| h.to_string())
            .collect();

        assert_eq!(missing_among(&loaded), ["thalyx_write"]);
    }

    #[test]
    fn nine_hooks_do_not_vouch_for_ten() {
        // A count comparison would pass here, and so would a prefix match.
        // Neither is a set comparison, and only a set comparison is true.
        let loaded: Vec<String> = REQUIRED_HOOKS
            .iter()
            .take(REQUIRED_HOOKS.len() - 1)
            .map(|h| h.to_string())
            .chain(["some_other_tools_program".to_string()])
            .collect();

        assert_eq!(loaded.len(), REQUIRED_HOOKS.len());
        assert!(!missing_among(&loaded).is_empty());
    }

    #[test]
    fn an_unloaded_watcher_claims_nothing() {
        // Every reason the question cannot be answered — no watcher, no
        // bpftool, no permission — has to come out as "no coverage". A
        // failure to ask must never read as a yes.
        let counter =
            KernelCounter::default_map().with_bpftool("/nonexistent/bpftool-that-is-not-installed");
        assert!(!counter.claims_complete_coverage());
    }

    #[test]
    fn the_hook_names_stay_within_what_the_kernel_keeps() {
        // The kernel truncates program names to 15 characters. Two hooks that
        // collide after truncation would let one vouch for the other, which
        // already nearly happened with inode_mkdir and inode_mknod.
        let mut truncated: Vec<&str> = REQUIRED_HOOKS
            .iter()
            .map(|name| &name[..name.len().min(15)])
            .collect();
        let before = truncated.len();
        truncated.sort_unstable();
        truncated.dedup();

        assert_eq!(
            truncated.len(),
            before,
            "two hook names are the same once the kernel truncates them"
        );
    }
}
