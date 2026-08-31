//! A second crate that uses the name, so a rename has to cross a file.
//!
//! Two crates and not one file with two mentions: `edits_by_file` is a claim
//! about *per file* counts, and a fixture that only ever had one file could
//! not tell that claim from a total.

use lantern::LanternRegistry;

pub fn open() -> LanternRegistry {
    LanternRegistry::new()
}

pub fn count(registry: &LanternRegistry) -> u32 {
    registry.lit()
}

/// An alias, because a rename that only rewrote the plain mentions would pass
/// a test that had none.
pub type Registry = LanternRegistry;
