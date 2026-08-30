/// The third. There is also exactly one `Unmistakable` here, which is the
/// positive control: without it, a machine that called every name ambiguous
/// would pass the ambiguity test.
pub struct Config {
    pub gamma: u32,
}

pub struct Unmistakable {
    pub only: u32,
}

pub fn make() -> Config {
    Config { gamma: 3 }
}

pub fn sole() -> Unmistakable {
    Unmistakable { only: 0 }
}
