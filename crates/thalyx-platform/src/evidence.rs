//! EvidenceSink: where a run's record is kept, and how it is fetched back.
//!
//! Kept **before** the answer is composed, and never inside what a rollback
//! replaces — `exec.rs` has always had both rules; this is the seam they now go
//! through.

/// What a fetch found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    Found(Vec<u8>),
    /// Nothing was kept under that handle.
    Absent,
    /// Something is there and could not be read — rule 10: a failure to read is
    /// not a failure to exist.
    Unreadable(String),
}

pub trait EvidenceSink {
    /// Keep a run's record under its handle. Not `keep`, which is what a
    /// boundary does with work: the two are different acts on different things,
    /// and one backend implements both.
    fn record(&mut self, id: &str, body: &[u8]) -> std::io::Result<()>;
    fn fetch(&mut self, id: &str) -> Fetched;
}
