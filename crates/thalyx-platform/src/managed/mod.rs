//! The managed model: versions, private work, and publication by CAS.
//!
//! Thalyx-Kernel's `vault/architecture/persistence.md` and ADR-003, from the
//! client's side. The service is one writer; published roots are immutable and
//! named by what they hold; work happens in a private workspace forked from a
//! published generation; a candidate is frozen before it is validated; and
//! publishing is a compare-and-swap on the generation, prepared and committed in
//! a durable log with the receipt that decided it. Abandoning discards the
//! private workspace and touches nothing anybody else can see.
//!
//! [`protocol`] is the whole conversation, and [`client::Managed`] is everything
//! Thalyx does with it. Neither knows what is on the other end of the
//! [`crate::transport::Transport`]: on Linux it is `thalyx-managed`'s store over
//! a loopback; on Thalyx-Kernel it is meant to be the K4 state service, and the
//! client is meant not to change.

pub mod client;
pub mod protocol;
