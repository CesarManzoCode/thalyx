use crate::keystore::Keystore as Keys;

pub fn boot() -> Keys {
    crate::keystore::unlock()
}
