//! The one timing record every backend fills in.
//!
//! EXP-13 compares the same Thalyx over `linux-current`, `linux-managed` and
//! Thalyx-Kernel. A comparison whose three sides each grew their own stopwatch
//! is a comparison of stopwatches, so the spans are taken **by the transaction**,
//! at the calls it makes into [`crate::Platform`], with the clock the backend
//! provides — and the backend never writes a span itself.
//!
//! ## How to read one
//!
//! Spans nest the way the calls do: a `program` span contains the `request`,
//! `observe` and `validate` spans its program caused, and a `validate` span
//! contains its `launch`. So `phases` totals are per phase and are **not** meant
//! to be added across phases. `whole_ns` is the transaction from before the
//! boundary opened to after it settled.
//!
//! The span list is bounded, the totals are not: a program that makes five
//! hundred requests still has exact per-phase totals, and says how many spans
//! were not listed.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TRACE_SCHEMA: &str = "thalyx-trace-v1";

/// How many spans a trace lists before only the totals keep counting.
pub const MOST_SPANS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Opening the boundary.
    Open,
    /// The whole QuickJS run, when the request was a program.
    Program,
    /// One request through the verb door.
    Request,
    /// Observing what changed.
    Observe,
    /// One validation, launch included.
    Validate,
    /// Freezing a candidate so a verdict can name what it was about.
    Candidate,
    /// One launched process.
    Launch,
    /// Keeping or abandoning.
    Settle,
    /// Reading the end state.
    End,
}

impl Phase {
    pub fn word(self) -> &'static str {
        match self {
            Phase::Open => "open",
            Phase::Program => "program",
            Phase::Request => "request",
            Phase::Observe => "observe",
            Phase::Validate => "validate",
            Phase::Candidate => "candidate",
            Phase::Launch => "launch",
            Phase::Settle => "settle",
            Phase::End => "end",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub phase: Phase,
    /// What inside the phase: a verb, a check, `keep` or `abandon`.
    pub name: String,
    /// Offset from the start of the transaction.
    pub start_ns: u64,
    pub ns: u64,
    pub ok: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Total {
    pub count: u64,
    pub ns: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    pub schema: String,
    pub backend: String,
    pub whole_ns: u64,
    pub spans: Vec<Span>,
    pub spans_not_listed: u64,
    pub phases: BTreeMap<String, Total>,
    #[serde(skip)]
    origin_ns: u64,
}

impl Trace {
    pub fn new(backend: &str, origin_ns: u64) -> Self {
        Self {
            schema: TRACE_SCHEMA.to_string(),
            backend: backend.to_string(),
            origin_ns,
            ..Self::default()
        }
    }

    /// One span, from two readings of the backend's clock.
    pub fn record(
        &mut self,
        phase: Phase,
        name: impl Into<String>,
        started_ns: u64,
        ended_ns: u64,
        ok: bool,
    ) {
        let ns = ended_ns.saturating_sub(started_ns);
        let total = self.phases.entry(phase.word().to_string()).or_default();
        total.count += 1;
        total.ns = total.ns.saturating_add(ns);
        if self.spans.len() < MOST_SPANS {
            self.spans.push(Span {
                phase,
                name: name.into(),
                start_ns: started_ns.saturating_sub(self.origin_ns),
                ns,
                ok,
            });
        } else {
            self.spans_not_listed += 1;
        }
    }

    pub fn finish(&mut self, ended_ns: u64) {
        self.whole_ns = ended_ns.saturating_sub(self.origin_ns);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_totals_keep_counting_after_the_list_stops() {
        let mut trace = Trace::new("test", 100);
        for n in 0..(MOST_SPANS as u64 + 5) {
            trace.record(Phase::Request, "read", 100 + n, 102 + n, true);
        }
        assert_eq!(trace.spans.len(), MOST_SPANS);
        assert_eq!(trace.spans_not_listed, 5);
        let total = trace.phases["request"];
        assert_eq!(total.count, MOST_SPANS as u64 + 5);
        assert_eq!(total.ns, 2 * (MOST_SPANS as u64 + 5));
        assert_eq!(
            trace.spans[0].start_ns, 0,
            "offsets are from the transaction"
        );
    }
}
