pub trait Speaks {
    fn say(&self) -> &'static str;
}
