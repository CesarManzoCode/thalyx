//! Reading the BPF ring buffer the watcher writes into.
//!
//! `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **B3**. Half of it
//! has existed since `thalyx_watch.bpf.c` was written: every mutation the hooks
//! see is pushed into `thalyx_mutations`, a `BPF_MAP_TYPE_RINGBUF`, and the
//! comment above that map says in as many words that reading it *«needs a
//! consumer that mmaps the map and follows the ring protocol»*. Nothing has ever
//! consumed it.
//!
//! ## The protocol is here and the mmap is not
//!
//! Everything in this file is a pure function over bytes: given where the
//! consumer had got to, where the producer has got to, and the data area, it
//! yields the records in between and the position to write back. It knows
//! nothing about file descriptors.
//!
//! That split is what makes this testable at all. `CLAUDE.md` says the container
//! this is written in has no BPF and no bpffs, and rule 8 says a fake must model
//! the property under test. **The property under test is the protocol** — where
//! a record starts, what makes one unfinished, what happens at the wrap — and a
//! byte array models that exactly, because the kernel's side of the contract is
//! the byte layout and nothing else. What a byte array cannot model is `mmap`
//! and the double mapping, and that is precisely what is *not* in this file.
//!
//! ## The layout, from `kernel/bpf/ringbuf.c`
//!
//! Records are eight-byte aligned and begin with an eight-byte header: a 32-bit
//! length whose top two bits are flags, then a page offset this side ignores.
//!
//! - **busy** (bit 31): the producer reserved this record and has not submitted
//!   it. Everything after it is unreadable too, so consumption stops — reading
//!   past a busy record is reading memory the kernel is still writing.
//! - **discard** (bit 30): reserved and then thrown away. It occupies space and
//!   carries nothing, so it is skipped rather than yielded.
//!
//! ## What this cannot answer, and must therefore say
//!
//! A ring buffer is **consumed**. What is read is gone, and there is no going
//! back to it — so "what changed since X" for any X older than the last read is
//! not a question this can answer, whatever the decree hoped. What it answers is
//! *what is in the ring now*, and anything that wants a history has to keep one.
//! Reporting a drain as a history would be the worst kind of wrong here: two
//! callers asking in turn would each be told a different, confident, incomplete
//! story.
//!
//! And a record carries a cgroup, a pid, a kind and a command name. **It does
//! not carry a path.** The count is machine-wide and the attribution map is
//! per-tree; neither is a filename. A caller that needs to know *which file*
//! still has to walk, and this says so rather than letting it assume.

use std::path::{Path, PathBuf};

/// Where the loader pins the ring the watcher writes detail into.
pub const DEFAULT_RING: &str = "/sys/fs/bpf/thalyx/maps/thalyx_mutations";

/// The top bit: reserved and not yet submitted.
const BUSY: u32 = 1 << 31;
/// The next bit: reserved and thrown away.
const DISCARD: u32 = 1 << 30;
/// What is left is the length.
const LENGTH: u32 = !(BUSY | DISCARD);
/// Every record starts on an eight-byte boundary, header included.
const HEADER: usize = 8;

/// One mutation, as `thalyx_watch.bpf.c` writes it.
///
/// The field order and widths are the C struct's, written out rather than
/// derived, because the two sides of this are compiled by different compilers
/// from different languages and the only thing holding them together is that
/// somebody wrote the layout down in both places and tested it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutation {
    /// Which cgroup did it. This is what separates a change the agent made from
    /// one the person made, and it is the most useful field in the record.
    pub cgroup_id: u64,
    pub pid: u32,
    pub kind: Kind,
    /// The program's name, up to sixteen bytes, as the kernel keeps it.
    pub comm: String,
}

/// What kind of change it was.
///
/// The numbers are `THALYX_CREATED` and friends in `thalyx_watch.bpf.c`. An
/// unknown one is kept as [`Kind::Unknown`] rather than dropped or guessed:
/// rule 9 says a value written by a version that does not exist yet gets the
/// cautious answer, and the cautious answer to "what kind of change was this"
/// is *one I do not recognise*, never *none*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Created,
    Removed,
    Renamed,
    Written,
    Retitled,
    Unknown(u32),
}

impl Kind {
    fn of(raw: u32) -> Self {
        match raw {
            0 => Kind::Created,
            1 => Kind::Removed,
            2 => Kind::Renamed,
            3 => Kind::Written,
            4 => Kind::Retitled,
            other => Kind::Unknown(other),
        }
    }

    /// The word a program matches on.
    pub fn word(self) -> String {
        match self {
            Kind::Created => "created".to_string(),
            Kind::Removed => "removed".to_string(),
            Kind::Renamed => "renamed".to_string(),
            Kind::Written => "written".to_string(),
            Kind::Retitled => "retitled".to_string(),
            // Named with its number, so a report from a machine running a newer
            // watcher says something a person can act on.
            Kind::Unknown(raw) => format!("unknown_{raw}"),
        }
    }
}

/// The size of one record's payload, in bytes: `struct mutation`.
///
/// `u64 + u32 + u32 + char[16]`, with no padding at the end because the
/// alignment is eight and the total is already a multiple of it.
pub const RECORD: usize = 8 + 4 + 4 + 16;

/// What one pass over the ring found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Drained {
    pub records: Vec<Mutation>,
    /// Where the consumer position must be written back to.
    pub consumed_to: u64,
    /// Records that were reserved and discarded by the producer. Counted rather
    /// than ignored: they are mutations the kernel saw and chose not to
    /// describe, so a caller adding up what it was told is not adding up what
    /// happened.
    pub discarded: usize,
    /// Records whose payload was not the size this version expects. Counted and
    /// skipped, never decoded: a struct that grew by a field would otherwise be
    /// read with every field after the first one wrong, confidently.
    pub unexpected_size: usize,
    /// Whether the pass stopped at a record the producer had not finished
    /// writing. Not an error — it is the ordinary end of a pass on a busy
    /// machine — but a caller that reported "nothing more" would be wrong.
    pub stopped_at_busy: bool,
}

/// Read everything the producer has finished writing.
///
/// `data` is the ring's data area and its length must be the map's
/// `max_entries`, which the kernel requires to be a power of two — the masking
/// below is the kernel's own wraparound arithmetic and is wrong for any other
/// size. A caller that cannot establish that size must not guess one.
pub fn drain(data: &[u8], consumer_pos: u64, producer_pos: u64) -> Drained {
    let mut found = Drained {
        consumed_to: consumer_pos,
        ..Drained::default()
    };

    if data.is_empty() || !data.len().is_power_of_two() {
        // Fail closed. Masking with a size that is not a power of two silently
        // reads the wrong offsets, and every record after the first would be
        // garbage that decodes without complaining.
        return found;
    }
    let mask = (data.len() - 1) as u64;
    let mut at = consumer_pos;

    while at < producer_pos {
        let offset = (at & mask) as usize;
        let Some(header) = read_u32(data, offset) else {
            break;
        };

        if header & BUSY != 0 {
            // Everything after this is unreadable too: the producer writes in
            // order, so a record it has not submitted is the boundary of what
            // exists. Reading past it reads memory being written.
            found.stopped_at_busy = true;
            break;
        }

        let length = (header & LENGTH) as usize;
        if length == 0 {
            // Not a record. Unwritten ring memory reads as zeros, and a
            // zero-length record would advance the position by nothing and spin
            // here forever — which is the one failure mode a consumer must not
            // have, because it hangs the caller rather than answering it wrong.
            break;
        }
        let step = HEADER + length.next_multiple_of(8);
        let Some(next) = at.checked_add(step as u64) else {
            break;
        };

        if header & DISCARD != 0 {
            found.discarded += 1;
        } else if length != RECORD {
            found.unexpected_size += 1;
        } else if let Some(record) = decode(data, (offset + HEADER) & mask as usize, mask) {
            found.records.push(record);
        } else {
            found.unexpected_size += 1;
        }

        at = next;
        found.consumed_to = at;
    }

    found
}

/// One record's bytes, read with the ring's wraparound.
///
/// The kernel maps the data area twice in a row so a record that straddles the
/// end can be read as one contiguous run. This side has only the bytes it was
/// given, so it does the wrap itself — which is what makes the same function
/// correct against a plain array in a test and against the double mapping on a
/// real machine.
fn decode(data: &[u8], start: usize, mask: u64) -> Option<Mutation> {
    let mut bytes = [0u8; RECORD];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = *data.get(((start + index) as u64 & mask) as usize)?;
    }

    let cgroup_id = u64::from_ne_bytes(bytes[0..8].try_into().ok()?);
    let pid = u32::from_ne_bytes(bytes[8..12].try_into().ok()?);
    let kind = Kind::of(u32::from_ne_bytes(bytes[12..16].try_into().ok()?));
    let comm = &bytes[16..32];
    let end = comm
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(comm.len());

    Some(Mutation {
        cgroup_id,
        pid,
        kind,
        // Lossy on purpose. A command name is bytes and this is for a report; a
        // name that would not convert must not stop the record being counted.
        comm: String::from_utf8_lossy(&comm[..end]).into_owned(),
    })
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_ne_bytes(bytes.try_into().ok()?))
}

/// The pinned ring, mapped.
///
/// Two mappings, as `kernel/bpf/ringbuf.c` requires and refuses otherwise: one
/// writable page holding the consumer position, and a read-only region holding
/// the producer position followed by the data area mapped twice. The kernel
/// rejects a writable producer mapping, which is the mechanism that stops a
/// consumer corrupting the ring — and it is why these are two calls and not one.
pub struct Ring {
    /// Writable, one page. Only its first eight bytes are ever written.
    consumer: thalyx_syscall::Mapped,
    /// Read-only. Position first, then the data.
    producer: thalyx_syscall::Mapped,
    page: usize,
    data_size: usize,
    /// Held so the mappings stay valid: unmapping does not depend on it, but
    /// nothing else should reuse the descriptor while this exists.
    _map: std::os::fd::OwnedFd,
}

impl Ring {
    /// Open the ring pinned at `path`, whose data area is `data_size` bytes.
    ///
    /// `data_size` is the map's `max_entries` and is **not** guessed: a size
    /// that is wrong makes the wraparound arithmetic read the wrong offsets and
    /// every record decode into confident garbage. A caller that cannot
    /// establish it must not call this.
    pub fn open(path: &Path, data_size: usize) -> std::result::Result<Self, WatchError> {
        if !data_size.is_power_of_two() {
            return Err(WatchError::Unreadable {
                reason: format!("a ring's data area must be a power of two, not {data_size}"),
                output: path.display().to_string(),
            });
        }

        let map = thalyx_syscall::bpf_obj_get(path).map_err(|error| {
            // Told apart, because they send somebody to different places: no
            // pin means the watcher is not loaded, and a pin that will not open
            // means it is loaded and this cannot reach it.
            if error.kind() == std::io::ErrorKind::NotFound {
                WatchError::NotPinned(path.to_path_buf())
            } else {
                WatchError::Spawn(error)
            }
        })?;

        let page = thalyx_syscall::page_size();
        let consumer = thalyx_syscall::map_shared(std::os::fd::AsFd::as_fd(&map), 0, page, true)
            .map_err(WatchError::Spawn)?;
        // The full length the kernel expects: its own position page, then the
        // data area mapped twice so a record that straddles the end is
        // contiguous. This side does its own wraparound and does not rely on
        // the second copy, but asking for a shorter mapping is refused.
        let producer = thalyx_syscall::map_shared(
            std::os::fd::AsFd::as_fd(&map),
            page as u64,
            page + 2 * data_size,
            false,
        )
        .map_err(WatchError::Spawn)?;

        Ok(Self {
            consumer,
            producer,
            page,
            data_size,
            _map: map,
        })
    }

    /// Read everything the kernel has finished writing, and tell it so.
    ///
    /// The write-back is what lets the kernel reclaim the space. A consumer that
    /// read and never advanced would fill a one-megabyte ring and then silently
    /// drop every mutation after it — the ring would look empty forever after
    /// looking full once.
    pub fn drain(&self) -> Drained {
        let positions = self.consumer.bytes();
        let producer_bytes = self.producer.bytes();

        let consumer_pos = read_u64(positions, 0).unwrap_or(0);
        let producer_pos = read_u64(producer_bytes, 0).unwrap_or(0);

        let data = match producer_bytes.get(self.page..self.page + self.data_size) {
            Some(data) => data,
            // Fail closed. A mapping shorter than it should be means something
            // about this kernel's layout is not what this code believes, and
            // reading it anyway is how a caller gets confident nonsense.
            None => return Drained::default(),
        };

        let found = drain(data, consumer_pos, producer_pos);
        if found.consumed_to != consumer_pos {
            self.consumer.write_first_u64(found.consumed_to);
        }
        found
    }
}

/// How big the ring's data area is, from `bpftool map show`.
///
/// Asked of the machine rather than hardcoded from `thalyx_watch.bpf.c`,
/// because the two can disagree: the source in this repository is not
/// necessarily the program that is loaded, and a size taken from the wrong one
/// is the exact input that makes every record decode into garbage.
pub fn data_size_of(pinned: &Path, bpftool: &Path, use_sudo: bool) -> Option<usize> {
    let mut command = if use_sudo {
        let mut c = std::process::Command::new("sudo");
        c.arg("-n").arg(bpftool);
        c
    } else {
        std::process::Command::new(bpftool)
    };
    let output = command
        .args(["map", "show", "pinned"])
        .arg(pinned)
        .arg("-j")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let entries = value.get("max_entries")?.as_u64()? as usize;
    entries.is_power_of_two().then_some(entries)
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let slice = bytes.get(offset..offset + 8)?;
    Some(u64::from_ne_bytes(slice.try_into().ok()?))
}

use crate::WatchError;

/// Where the ring lives on a machine, as a path.
pub fn default_ring() -> PathBuf {
    PathBuf::from(DEFAULT_RING)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ring the way the kernel lays one out, built by hand.
    struct Ring {
        data: Vec<u8>,
        producer: u64,
    }

    impl Ring {
        fn of(size: usize) -> Self {
            Self {
                data: vec![0; size],
                producer: 0,
            }
        }

        fn put(&mut self, header_flags: u32, payload: &[u8]) -> &mut Self {
            let mask = self.data.len() - 1;
            let offset = (self.producer as usize) & mask;
            let header = (payload.len() as u32) | header_flags;
            for (index, byte) in header.to_ne_bytes().iter().enumerate() {
                self.data[(offset + index) & mask] = *byte;
            }
            // The page-offset half of the header. This side ignores it, and
            // writing something non-zero is how the test proves that.
            for (index, byte) in 0xabcd_u32.to_ne_bytes().iter().enumerate() {
                self.data[(offset + 4 + index) & mask] = *byte;
            }
            for (index, byte) in payload.iter().enumerate() {
                self.data[(offset + HEADER + index) & mask] = *byte;
            }
            self.producer += (HEADER + payload.len().next_multiple_of(8)) as u64;
            self
        }
    }

    fn a_mutation(cgroup: u64, pid: u32, kind: u32, comm: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&cgroup.to_ne_bytes());
        bytes.extend_from_slice(&pid.to_ne_bytes());
        bytes.extend_from_slice(&kind.to_ne_bytes());
        let mut name = [0u8; 16];
        for (slot, byte) in name.iter_mut().zip(comm.bytes()) {
            *slot = byte;
        }
        bytes.extend_from_slice(&name);
        bytes
    }

    #[test]
    fn a_submitted_record_comes_back_whole() {
        let mut ring = Ring::of(4096);
        ring.put(0, &a_mutation(42, 7, 2, "mv"));

        let found = drain(&ring.data, 0, ring.producer);
        assert_eq!(found.records.len(), 1);
        assert_eq!(
            found.records[0],
            Mutation {
                cgroup_id: 42,
                pid: 7,
                kind: Kind::Renamed,
                comm: "mv".to_string(),
            }
        );
        // And the consumer position lands exactly where the producer is, or the
        // next pass re-reads what this one already returned.
        assert_eq!(found.consumed_to, ring.producer);
    }

    #[test]
    fn nothing_written_yields_nothing_and_moves_nothing() {
        let ring = Ring::of(4096);
        let found = drain(&ring.data, 0, 0);
        assert!(found.records.is_empty());
        assert_eq!(found.consumed_to, 0);
        assert!(!found.stopped_at_busy);
    }

    #[test]
    fn a_record_the_producer_has_not_finished_stops_the_pass_where_it_is() {
        let mut ring = Ring::of(4096);
        ring.put(0, &a_mutation(1, 1, 0, "touch"));
        let after_the_good_one = ring.producer;
        ring.put(BUSY, &a_mutation(2, 2, 1, "rm"));

        let found = drain(&ring.data, 0, ring.producer);

        // The one that was finished is returned; the one being written is not
        // read at all. Reading past it would be reading memory the kernel is
        // still writing into.
        assert_eq!(found.records.len(), 1);
        assert_eq!(found.records[0].comm, "touch");
        assert!(found.stopped_at_busy);
        // And the position stops before it, so the next pass picks it up.
        assert_eq!(found.consumed_to, after_the_good_one);
    }

    #[test]
    fn a_discarded_record_is_counted_and_not_decoded() {
        let mut ring = Ring::of(4096);
        ring.put(DISCARD, &a_mutation(9, 9, 0, "gone"));
        ring.put(0, &a_mutation(1, 1, 3, "dd"));

        let found = drain(&ring.data, 0, ring.producer);

        // Counted, because it is a mutation the kernel saw and chose not to
        // describe. A caller adding up what it was told would otherwise not be
        // adding up what happened.
        assert_eq!(found.discarded, 1);
        assert_eq!(found.records.len(), 1);
        assert_eq!(found.records[0].comm, "dd");
    }

    #[test]
    fn a_record_that_straddles_the_end_of_the_ring_is_read_as_one() {
        // The case a plain slice read gets wrong and the kernel's double mapping
        // hides on a real machine. If this were only ever tested on hardware,
        // the bug would appear once every megabyte of events and look random.
        let size = 128;
        let mut ring = Ring::of(size);
        // Put the producer near the end so the next record wraps.
        ring.producer = (size - 16) as u64;
        let payload = a_mutation(1234, 56, 4, "chmod");
        ring.put(0, &payload);

        let found = drain(&ring.data, (size - 16) as u64, ring.producer);
        assert_eq!(found.records.len(), 1, "{found:?}");
        assert_eq!(
            found.records[0],
            Mutation {
                cgroup_id: 1234,
                pid: 56,
                kind: Kind::Retitled,
                comm: "chmod".to_string(),
            }
        );
    }

    #[test]
    fn a_payload_of_an_unexpected_size_is_skipped_rather_than_misread() {
        let mut ring = Ring::of(4096);
        // A struct that grew by a field, from a watcher this version does not
        // know. Decoded anyway, every field after the first would be wrong and
        // nothing would say so.
        let mut bigger = a_mutation(1, 1, 0, "new");
        bigger.extend_from_slice(&[0u8; 8]);
        ring.put(0, &bigger);
        ring.put(0, &a_mutation(2, 2, 1, "old"));

        let found = drain(&ring.data, 0, ring.producer);
        assert_eq!(found.unexpected_size, 1);
        assert_eq!(found.records.len(), 1);
        assert_eq!(found.records[0].comm, "old");
    }

    #[test]
    fn a_kind_this_version_does_not_know_is_named_rather_than_dropped() {
        let mut ring = Ring::of(4096);
        ring.put(0, &a_mutation(1, 1, 99, "future"));

        let found = drain(&ring.data, 0, ring.producer);
        // Rule 9: a value written by a version that does not exist yet gets the
        // cautious answer, and the cautious answer to "what kind was this" is
        // *one I do not recognise*, never *none*.
        assert_eq!(found.records[0].kind, Kind::Unknown(99));
        assert_eq!(found.records[0].kind.word(), "unknown_99");
    }

    #[test]
    fn resuming_from_a_position_returns_only_what_came_after_it() {
        let mut ring = Ring::of(4096);
        ring.put(0, &a_mutation(1, 1, 0, "first"));
        let mark = ring.producer;
        ring.put(0, &a_mutation(2, 2, 1, "second"));

        let found = drain(&ring.data, mark, ring.producer);
        assert_eq!(found.records.len(), 1);
        assert_eq!(found.records[0].comm, "second");
    }

    #[test]
    fn a_data_area_that_is_not_a_power_of_two_yields_nothing_instead_of_garbage() {
        // Fail closed. The masking is the kernel's own wraparound arithmetic and
        // is wrong for any other size — every record after the first would be
        // read at the wrong offset and decode without complaining.
        let data = vec![0u8; 100];
        let found = drain(&data, 0, 4096);
        assert!(found.records.is_empty());
        assert_eq!(found.consumed_to, 0);
    }

    #[test]
    fn several_records_come_back_in_the_order_the_kernel_wrote_them() {
        let mut ring = Ring::of(4096);
        for n in 0..5u32 {
            ring.put(0, &a_mutation(n as u64, n, n % 5, &format!("p{n}")));
        }

        let found = drain(&ring.data, 0, ring.producer);
        let names: Vec<&str> = found.records.iter().map(|r| r.comm.as_str()).collect();
        // Order is information here: a rename followed by a delete is not the
        // same story as a delete followed by a rename.
        assert_eq!(names, vec!["p0", "p1", "p2", "p3", "p4"]);
    }
}
