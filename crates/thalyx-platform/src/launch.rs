//! ProgramLaunch: start a program with named grants and read how it ended.
//!
//! What a validation needs from a launcher is small and fixed — an exit status,
//! both streams, and whether they were cut — plus whatever the machine itself
//! charged for the run. The accounting is an open map on purpose: on Linux it is
//! a cgroup and an isolation flag, on Thalyx-Kernel it is a scope and the CPU the
//! kernel charged to it, and neither is a field the other has.

use crate::authority::Grant;
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    /// An absolute path. A launcher has no search path to resolve a bare name
    /// against, and one that invented a `PATH` would be deciding what a name
    /// means on somebody's behalf.
    pub program: PathBuf,
    pub arguments: Vec<String>,
    /// Every object the program may reach, and nothing else.
    pub grants: Vec<Grant>,
    pub environment: Vec<(String, String)>,
    /// The confinement profile, by the name the launcher knows it by.
    pub profile: String,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Launched {
    /// `None` when it did not exit on its own.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    /// What the machine charged, in the machine's own terms.
    pub accounting: Map<String, Value>,
}

pub trait ProgramLaunch {
    /// Run it to the end. `Err` is a program that could not be started under the
    /// confinement asked for — which a validation reports as `not_proven`, never
    /// as the program having failed.
    fn launch(&mut self, request: &LaunchRequest) -> Result<Launched, String>;
}
