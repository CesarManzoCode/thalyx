/// One of three declarations of `Config` in this workspace.
pub struct Config {
    pub alpha: u32,
}

pub fn make() -> Config {
    Config { alpha: 1 }
}
