/// The second `Config`, and nothing in the text distinguishes it from the
/// first — which is the point: a scan cannot tell these apart and neither can
/// a search, so the machine must not pretend it has.
pub struct Config {
    pub beta: u32,
}

pub fn make() -> Config {
    Config { beta: 2 }
}
