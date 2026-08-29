//! `thalyx-screen` — the one screen that is Thalyx.
//!
//! The decree is `vault/02-Arquitectura/La-Pantalla.md`, taken by Cesar on
//! 2026-08-27: **one screen, no windows, no desktop, no launcher.** The
//! conversation is the centre and there is nowhere to *open* the agent, because
//! the agent is the screen.
//!
//! ## What is here, and what is deliberately not
//!
//! Everything in this crate is pure: state in, pixels out. **No device is
//! opened, no `ioctl` is made, nothing is displayed.** That is the same shape
//! `thalyx-term` and `thalyx-edit` already have, and it exists for the same
//! reason: the parts worth testing are *where things land* and *what a frame
//! looks like*, and those answers must not require a display to ask. The
//! container that builds Thalyx has no framebuffer.
//!
//! What stays outside: the `ioctl` that asks the kernel how this display packs
//! a pixel, the `mmap` that receives the frame, taking the text console out of
//! the way, and reading the keyboard and the mouse. All of that is in
//! `thalyx-syscall`, which is where the `unsafe` lives.
//!
//! One consequence worth naming: because a frame is just memory, it can be
//! written to a PNG from a machine with no display — see [`png`]. The screen
//! can be *looked at* before there is a machine to show it on.
//!
//! ## Why a screen at all
//!
//! Rule 1 of `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md`: every real
//! defect came from running the system, not from reading it. Everything Thalyx
//! knows about itself was measured by an instrument this project wrote, mostly
//! on Fedora. A machine a person can sit in front of for an hour is a class of
//! instrument that did not exist, and it does not share the assumptions of
//! whoever wrote the code.

mod canvas;
mod color;
mod frame;
mod layout;
pub mod png;
pub mod sample;
mod state;
mod text;

pub use canvas::{Canvas, Channel, PixelFormat, Rect, UnsupportedFormat};
pub use color::Color;
pub use frame::{compose, editor_viewport};
pub use layout::{Layout, Metrics};
pub use state::{Bar, Confirmation, Editor, Guard, Panel, Prompt, Row, Screen, Tone, Turn, Voice};
pub use text::{Face, TextStyle, Typography};

/// The palette, by the role each colour plays.
pub mod palette {
    pub use crate::color::{
        AGENT, FACT, HEADING, HUMAN, INK, LINE, MUTED, OK, REFUSED, SURFACE, TRUST, TRUST_GROUND,
    };
}
