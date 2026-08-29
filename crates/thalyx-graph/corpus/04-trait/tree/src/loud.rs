pub fn shout<T: crate::speaks::Speaks>(thing: &T) -> String {
    thing.say().to_uppercase()
}
