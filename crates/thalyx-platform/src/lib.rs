//! What a Thalyx transaction needs from a machine, and nothing about Linux.
//!
//! `vault/09-Notas-Tecnicas/Frontera-de-Plataforma.md`, which is the Thalyx
//! side of Thalyx-Kernel's `vault/integration/thalyx.md` (INT-001). That note
//! names seven properties a backend has to provide — `WorkControl`,
//! `ObjectAuthority`, `VersionedState`, `ProgramLaunch`, `MessageTransport`,
//! `MonotonicClock` and `EvidenceSink` — and says in the same breath that they
//! are not to become a `Platform::do_anything`. This crate is what the one
//! vertical that gets measured actually needed from each of them:
//!
//! `contexto → agent → hacer/QuickJS → tools → validation → freeze → publish
//! or abandon → durable evidence`.
//!
//! ## Why this exists at all
//!
//! Until this crate, `hacer` was Btrfs. Not in the obvious places only — the
//! snapshot and the restore — but in the shape of every sentence: the boundary
//! was a subvolume the session stood in, a validation was about whatever the
//! live tree held when the checker looked, a commit was *letting a snapshot
//! go*, and evidence was a file renamed into the store. A second backend that
//! wanted to carry out the same transaction over immutable versions would have
//! had to rewrite `exec.rs`, and a rewrite of the thing being compared is how a
//! comparison between two machines stops being a comparison of machines.
//!
//! So the semantics — what a refused step does, what a failing check does, which
//! verdict gates a commit, what the answer carries — stayed where they were, and
//! everything they asked of the machine now goes through [`Platform`]. There
//! are two backends on Linux: `linux-current`, which is exactly the mechanism
//! that existed, and `linux-managed`, which is the managed model of
//! [`managed`] over a local store service. A third one, over Thalyx-Kernel's own
//! primitives, is meant to be written against this crate and no other change.
//!
//! ## What each property became, and what it did not
//!
//! - **VersionedState** is [`state::VersionedState`]: open a boundary, observe
//!   what changed, name the candidate a verdict is about, keep or abandon. It
//!   is the one that carries real weight.
//! - **ProgramLaunch** is [`launch::ProgramLaunch`]: a program, its arguments
//!   and explicit grants, and an exit status with the accounting the machine
//!   itself produced.
//! - **ObjectAuthority** is [`authority`]: the grants a launch receives are a
//!   list of objects with rights, and a publication is made by a principal with
//!   a sequence rather than by whoever holds a path. It is types and not a trait,
//!   because nothing in the vertical asks an authority a question — it hands one
//!   over.
//! - **WorkControl** is [`work::WorkControl`]: admission of an effect against
//!   the work that asked for it, so a closed work cannot start a process or
//!   publish.
//! - **MessageTransport** is [`transport`]: the managed client and its service
//!   speak encoded messages and nothing else, which is what lets the service be
//!   somewhere other than this process.
//! - **MonotonicClock** is [`clock::MonotonicClock`], and [`trace`] is the one
//!   timing record every backend fills in, so that comparing them does not
//!   start by inventing three stopwatches.
//! - **EvidenceSink** is [`evidence::EvidenceSink`]: keep a run's record before
//!   answering, and fetch it by handle.
//!
//! What is deliberately **not** behind this boundary: the verbs themselves,
//! QuickJS, the parser, the semantic provider and the answer. Those are Thalyx,
//! and a backend that could change them would be a second Thalyx.

pub mod authority;
pub mod clock;
pub mod content;
pub mod evidence;
pub mod launch;
pub mod managed;
pub mod profile;
pub mod state;
pub mod trace;
pub mod transport;
pub mod work;

/// One machine, as a transaction sees it.
///
/// Accessors rather than one trait with every method on it, so that a backend
/// is visibly made of its parts and a reader of `exec.rs` can see which
/// property each call leans on. The borrows are sequential by construction:
/// nothing in the transaction holds two parts at once.
pub trait Platform {
    /// What this backend guarantees, property by property.
    fn profile(&self) -> &profile::Profile;
    fn clock(&self) -> &dyn clock::MonotonicClock;
    fn state(&mut self) -> &mut dyn state::VersionedState;
    fn launcher(&mut self) -> &mut dyn launch::ProgramLaunch;
    fn work(&mut self) -> &mut dyn work::WorkControl;
    fn evidence(&mut self) -> &mut dyn evidence::EvidenceSink;
}
