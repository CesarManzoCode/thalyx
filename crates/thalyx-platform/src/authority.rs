//! ObjectAuthority, in the two shapes the vertical hands authority over in.
//!
//! A launch receives **objects with rights**, never "the workspace" as an
//! ambient idea; a publication is made by a **principal with a sequence**, never
//! by whoever can name a path. Both used to be implicit in `exec.rs` — a
//! permission list assembled next to a `run_foreign` call, and a commit that
//! was whoever held the store lock — and implicit authority is the kind a second
//! backend quietly gets different.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One object a launched program may reach, and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub object: PathBuf,
    pub read: bool,
    pub write: bool,
}

impl Grant {
    pub fn read(object: impl Into<PathBuf>) -> Self {
        Self {
            object: object.into(),
            read: true,
            write: false,
        }
    }

    pub fn read_write(object: impl Into<PathBuf>) -> Self {
        Self {
            object: object.into(),
            read: true,
            write: true,
        }
    }
}

/// Who is asking for a state transition, and which of their requests this is.
///
/// The persistence contract Thalyx-Kernel's K4 implements: a principal keeps a
/// strictly increasing sequence, a repeated sequence is answered from what was
/// recorded rather than executed again, and a lost reply is recovered by asking
/// about the request instead of guessing. A timeout is not an abort.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RequestId {
    pub principal: String,
    pub sequence: u64,
}
