//! WorkControl: whether the work that asked for an effect is still allowed to
//! have one.
//!
//! Thalyx-Kernel's persistence contract has the service mark an *admission of
//! effect* against the invocation before it starts publishing, so that a work
//! whose scope was fenced cannot begin an effect after the fence. Linux has no
//! object for "the work" — a process is not a task and a task is not a
//! transaction — so `linux-current` admits everything and says that is what it
//! does, and the managed model carries a scope with a fence.

use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Start a process on the work's behalf.
    Launch,
    /// Make the work's result visible to anybody else.
    Publish,
}

impl Effect {
    pub fn word(self) -> &'static str {
        match self {
            Effect::Launch => "start a process",
            Effect::Publish => "publish",
        }
    }
}

pub trait WorkControl {
    /// May the work have this effect now?
    fn admit(&mut self, effect: Effect) -> Result<(), String>;

    /// Charge an admitted effect's cost to the work.
    fn charge(&mut self, effect: Effect, nanoseconds: u64);

    /// What the work was admitted, refused and charged. For the evidence.
    fn report(&self) -> Value;
}

/// No work object at all: the process is the only principal, and it is admitted
/// every effect it asks for.
///
/// This is what Thalyx on Linux has always been, written down rather than
/// implied, so that a comparison with a backend that fences can say which one
/// could have refused.
#[derive(Debug, Default)]
pub struct Ambient {
    launches: u64,
    launch_ns: u64,
    publishes: u64,
}

impl WorkControl for Ambient {
    fn admit(&mut self, _effect: Effect) -> Result<(), String> {
        Ok(())
    }

    fn charge(&mut self, effect: Effect, nanoseconds: u64) {
        match effect {
            Effect::Launch => {
                self.launches += 1;
                self.launch_ns = self.launch_ns.saturating_add(nanoseconds);
            }
            Effect::Publish => self.publishes += 1,
        }
    }

    fn report(&self) -> Value {
        json!({
            "kind": "ambient_process",
            "launches": self.launches,
            "launch_ns": self.launch_ns,
            "publishes": self.publishes,
        })
    }
}

/// The switch that closes a work from outside it.
#[derive(Debug, Clone, Default)]
pub struct Fence(Arc<AtomicBool>);

impl Fence {
    pub fn close(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_closed(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A work scope with a fence: admitted until somebody closes it, refused after.
#[derive(Debug)]
pub struct Scoped {
    id: String,
    fence: Fence,
    launches: u64,
    launch_ns: u64,
    publishes: u64,
    refused: Vec<String>,
}

impl Scoped {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            fence: Fence::default(),
            launches: 0,
            launch_ns: 0,
            publishes: 0,
            refused: Vec::new(),
        }
    }

    /// A handle that closes this work, for whoever is entitled to close it.
    pub fn fence(&self) -> Fence {
        self.fence.clone()
    }
}

impl WorkControl for Scoped {
    fn admit(&mut self, effect: Effect) -> Result<(), String> {
        if self.fence.is_closed() {
            let why = format!(
                "the work `{}` was closed before it could {}",
                self.id,
                effect.word()
            );
            self.refused.push(why.clone());
            return Err(why);
        }
        Ok(())
    }

    fn charge(&mut self, effect: Effect, nanoseconds: u64) {
        match effect {
            Effect::Launch => {
                self.launches += 1;
                self.launch_ns = self.launch_ns.saturating_add(nanoseconds);
            }
            Effect::Publish => self.publishes += 1,
        }
    }

    fn report(&self) -> Value {
        json!({
            "kind": "scope_with_fence",
            "scope": self.id,
            "closed": self.fence.is_closed(),
            "launches": self.launches,
            "launch_ns": self.launch_ns,
            "publishes": self.publishes,
            "refused": self.refused,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_admits_until_it_is_fenced_and_refuses_after() {
        let mut work = Scoped::new("w1");
        assert!(work.admit(Effect::Launch).is_ok());
        work.fence().close();
        let refused = work
            .admit(Effect::Publish)
            .expect_err("a fenced work publishes nothing");
        assert!(refused.contains("w1"), "{refused}");
        assert_eq!(work.report()["refused"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn ambient_work_admits_everything_and_says_that_is_what_it_is() {
        let mut work = Ambient::default();
        assert!(work.admit(Effect::Publish).is_ok());
        assert_eq!(work.report()["kind"], json!("ambient_process"));
    }
}
