//! Where the name under test is declared.

/// The symbol a rename is asked about.
///
/// Named nothing like anything in the benchmark's corpus on purpose: a check
/// that shares a symbol with the thing it is meant to be independent of is not
/// independent of it.
pub struct LanternRegistry {
    lit: u32,
}

impl LanternRegistry {
    pub fn new() -> Self {
        LanternRegistry { lit: 0 }
    }

    pub fn light(&mut self) {
        self.lit += 1;
    }

    pub fn lit(&self) -> u32 {
        self.lit
    }
}

impl Default for LanternRegistry {
    fn default() -> Self {
        Self::new()
    }
}
