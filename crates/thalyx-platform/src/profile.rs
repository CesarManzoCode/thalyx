//! What a backend guarantees, property by property.
//!
//! In the vocabulary Thalyx-Kernel's `abi/schema/k5-proto-v1.json` fixes for the
//! same purpose: features a consumer combines and demands, **not a level**. A
//! backend that satisfies part of a profile says which part, and never rounds a
//! partial guarantee up to the name of the whole one — `vault/integration/thalyx.md`
//! says an answer never turns a fallback into the success of a stronger profile.
//!
//! The engine features of that fixture are not here: the vertical this boundary
//! was cut for does not reach the engine, and a feature nothing reads is a claim
//! nothing checks.

use serde::{Deserialize, Serialize};

/// The five requirements `managed-local-v1` is made of, each said separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedLocal {
    /// Published versions cannot be changed in place.
    pub immutable_roots: bool,
    /// Publication is a durable compare-and-swap on a generation.
    pub durable_cas_publication: bool,
    /// Grants are explicit and a closed work cannot have effects.
    pub defined_grants_and_closure: bool,
    /// How resources a work spends are bounded, in this backend's terms.
    pub explicit_resources: String,
    /// Whether anything but the service can write the store, and what stops it.
    pub exclusive_store_writer: String,
    /// All of the above, and only when all of the above.
    pub holds: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// `linux-current`, `linux-managed`, and whatever the next one is called.
    pub backend: String,
    pub managed_local_v1: ManagedLocal,
    /// The workspace is a directory anything with a path may write.
    pub mutable_files: bool,
    /// A validation may compile the candidate with the real toolchain.
    pub type_check: bool,
    /// What a verdict is about: `nothing_named` (a mutable tree at some
    /// instant) or `candidate_content_identity`.
    pub validation_binding: String,
    pub publication: String,
    pub rollback: String,
    pub evidence: String,
    pub launch: String,
    pub work: String,
    pub transport: String,
    pub power_cut_durability: String,
}
