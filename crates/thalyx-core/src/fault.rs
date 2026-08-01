//! Fault injection points.
//!
//! The central claim of Thalyx is that a commit is atomic: **published or not
//! published, never halfway**. A claim like that is not demonstrated by
//! reasoning about it, so the install path carries named interruption points
//! and a test can force the process to die at any of them.
//!
//! Two modes:
//!
//! - `Abort` kills the process immediately with `SIGABRT`. No unwinding, no
//!   destructors, no cleanup — the closest thing to a power loss that can be
//!   arranged from inside the process. This is the mode the fault-injection
//!   tests use.
//! - `Error` returns a normal error instead, which is useful for exercising
//!   the error paths themselves.
//!
//! Selected through the `THALYX_FAULT_POINT` and `THALYX_FAULT_MODE`
//! environment variables, so a test can spawn the real CLI binary and kill it
//! for real rather than simulate the failure in-process.
//!
//! See `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md`.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    /// After verification succeeded, before anything was written.
    PostVerify,
    /// After the artifact was extracted into staging, before the commit.
    PostStage,
    /// Between the directory rename and the symlink swap.
    ///
    /// This is the one that matters. The version directory is already in its
    /// final location, but `current` still points at the previous version, so
    /// the module must be reported as *not installed*.
    MidCommit,
    /// After the symlink swap, before permissions became effective and before
    /// the journal was written.
    PostCommit,
}

impl FaultPoint {
    fn as_str(self) -> &'static str {
        match self {
            FaultPoint::PostVerify => "post-verify",
            FaultPoint::PostStage => "post-stage",
            FaultPoint::MidCommit => "mid-commit",
            FaultPoint::PostCommit => "post-commit",
        }
    }

    pub const ALL: [FaultPoint; 4] = [
        FaultPoint::PostVerify,
        FaultPoint::PostStage,
        FaultPoint::MidCommit,
        FaultPoint::PostCommit,
    ];
}

impl std::fmt::Display for FaultPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FaultPoint {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "post-verify" => Ok(FaultPoint::PostVerify),
            "post-stage" => Ok(FaultPoint::PostStage),
            "mid-commit" => Ok(FaultPoint::MidCommit),
            "post-commit" => Ok(FaultPoint::PostCommit),
            other => Err(format!("unknown fault point `{other}`")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FaultMode {
    /// Die immediately, without unwinding or cleanup.
    #[default]
    Abort,
    /// Return an error through the normal path.
    Error,
}

pub const FAULT_POINT_ENV: &str = "THALYX_FAULT_POINT";
pub const FAULT_MODE_ENV: &str = "THALYX_FAULT_MODE";

/// Trip the configured fault point, if this is it.
///
/// Called at each named point in the install path. Returns `Ok(())` when no
/// fault is configured, which is every non-test run.
pub fn checkpoint(point: FaultPoint) -> crate::Result<()> {
    let Ok(configured) = std::env::var(FAULT_POINT_ENV) else {
        return Ok(());
    };
    if configured.parse::<FaultPoint>() != Ok(point) {
        return Ok(());
    }

    let mode = match std::env::var(FAULT_MODE_ENV).as_deref() {
        Ok("error") => FaultMode::Error,
        _ => FaultMode::Abort,
    };

    match mode {
        FaultMode::Abort => {
            // Deliberately harsh: no unwinding, no Drop, no flush.
            eprintln!("thalyx: injected fault at {point}, aborting");
            std::process::abort();
        }
        FaultMode::Error => Err(crate::CoreError::InjectedFault(point)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fault_points_round_trip_through_strings() {
        for point in FaultPoint::ALL {
            assert_eq!(point.to_string().parse::<FaultPoint>(), Ok(point));
        }
    }

    #[test]
    fn unknown_fault_points_are_rejected() {
        assert!("halfway".parse::<FaultPoint>().is_err());
    }
}
