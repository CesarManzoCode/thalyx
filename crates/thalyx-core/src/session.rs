//! What a session is, so that a `session` permission can end with one.
//!
//! `vault/03-Primitivas/Permisos-JIT.md` decrees three kinds of permission and
//! says of the middle one: *válido mientras dure la sesión activa del
//! agente/usuario, se revoca al cerrar*. Nothing implemented that. A `session`
//! grant was written into the registry exactly like a `persistent` one and
//! never removed, so the kind existed in the schema and in the prompt and
//! nowhere else — a promise of expiry that nothing performed.
//!
//! ## What identifies a session
//!
//! A string that never repeats, and the rule is that simple on purpose. A
//! grant records the session it was made in; it is in force only while that
//! string is still the current one. Ending a session is therefore not a sweep
//! over the registry — it is writing a new string, after which every grant
//! naming the old one is inert without anything having had to find it.
//!
//! That matters more than it looks. A revocation implemented as a sweep fails
//! open when the sweep is interrupted: the process dies halfway and the grants
//! it had not reached yet are still live. This one fails closed, because the
//! single write that ends the session is the same write that invalidates all
//! of them at once.
//!
//! ## When there has never been a session
//!
//! The boot id. A machine that has not opened a named session still has a
//! bound — a `session` grant made on it dies at the next reboot rather than
//! living forever. And when the boot id cannot be read, [`Session::current`]
//! returns a value that matches nothing recorded, so `session` grants are
//! inert rather than eternal. Rule 9: the cautious answer, never the fast one.

use crate::store::Store;
use crate::{CoreError, Result};

/// Where the kernel publishes an identifier that changes on every boot.
const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";

/// The session id used when neither a named session nor a boot id can be read.
///
/// It is not a session anybody can be in: nothing is ever recorded under this
/// string, so every `session` grant compares unequal to it and holds nothing.
/// Spelled out rather than left as an empty string, because an empty string is
/// what a truncated file also produces and the two must not be confusable.
const NO_SESSION: &str = "no-session-could-be-established";

/// The session a grant is measured against.
pub struct Session;

impl Session {
    /// The current session id.
    ///
    /// Never fails. Every way of not knowing produces a value that makes
    /// `session` grants inert, which is the only answer that cannot hand
    /// somebody a permission the decree says should already be gone.
    pub fn current(store: &Store) -> String {
        if let Ok(named) = std::fs::read_to_string(store.session_path()) {
            let named = named.trim();
            if !named.is_empty() {
                return named.to_string();
            }
        }

        match std::fs::read_to_string(BOOT_ID) {
            Ok(boot) if !boot.trim().is_empty() => format!("boot:{}", boot.trim()),
            _ => NO_SESSION.to_string(),
        }
    }

    /// End the current session and begin a new one.
    ///
    /// One operation rather than two, and it is deliberate. "End" and "begin"
    /// as separate calls leave a state in between with no session at all,
    /// which would fall back to the boot id — and a grant made before any
    /// named session was opened was recorded under exactly that, so ending a
    /// session would bring those older grants back to life. Rolling to a fresh
    /// id in one write means no id is ever current twice.
    pub fn roll(store: &Store) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let path = store.session_path();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }

        // Written through the same durable path as the rest of the state: a
        // half-written session id is a session nobody is in, and while that
        // fails closed it also silently drops permissions the human is still
        // using.
        crate::keystore::write_durably(&path, id.as_bytes())?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn a_session_id_is_stable_while_the_session_lasts() {
        let (_dir, store) = store();
        let first = Session::current(&store);
        assert_eq!(first, Session::current(&store));
        assert!(!first.is_empty());
    }

    #[test]
    fn rolling_the_session_produces_an_id_nobody_had_before() {
        let (_dir, store) = store();
        let before = Session::current(&store);

        let rolled = Session::roll(&store).unwrap();
        assert_ne!(rolled, before);
        assert_eq!(Session::current(&store), rolled);

        // And again: two rolls never land on the same id, which is what makes
        // a grant from the first session unable to come back.
        let again = Session::roll(&store).unwrap();
        assert_ne!(again, rolled);
        assert_ne!(again, before);
    }

    #[test]
    fn an_empty_session_file_is_not_a_session_called_nothing() {
        // A truncated write must not produce a session id that a grant could
        // have been recorded under. It falls through to the boot id.
        let (_dir, store) = store();
        std::fs::write(store.session_path(), "").unwrap();

        let current = Session::current(&store);
        assert!(!current.is_empty());
        assert!(current.starts_with("boot:") || current == NO_SESSION);
    }
}
