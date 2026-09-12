//! MonotonicClock.
//!
//! One source of "how long", so that every span a backend reports is measured
//! the same way. Wall-clock timestamps in evidence (`at`) are a different
//! question — when something happened, for a person reading it later — and are
//! not this.

use std::time::Instant;

pub trait MonotonicClock {
    /// Nanoseconds since an origin that does not move while the process lives.
    fn now_ns(&self) -> u64;
}

/// `CLOCK_MONOTONIC`, through `std::time::Instant`.
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MonotonicClock for SystemClock {
    fn now_ns(&self) -> u64 {
        // Saturating rather than wrapping: 584 years of uptime is not a case,
        // and a wrapped duration would be a negative span in a report.
        u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}
