//! cgroup v2 resource limits.
//!
//! The identity half of the cgroup is in [`crate::cgroup`]; this is the other
//! half of what `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md` decrees for the
//! `module_standard` profile: `memory.max`, `pids.max` and `cpu.max`.
//!
//! ## A limit that cannot be applied is refused
//!
//! Not warned about — refused. It is the same rule
//! `vault/03-Primitivas/Permisos-JIT.md` states for permissions the kernel
//! cannot express: *a promise the system cannot keep is worse than a refusal,
//! because only one of the two is visible.* A module started after "could not
//! set memory.max" scrolled past runs with no memory limit at all, and looks
//! exactly like one that is bounded.
//!
//! ## Controllers have to be handed down
//!
//! A cgroup can only set `memory.max` if its **parent** enabled `memory` in
//! `cgroup.subtree_control`. Enabling it there requires the parent to hold no
//! processes of its own — the "no internal processes" rule. Thalyx's parent
//! cgroup holds only module cgroups, never processes, which is what makes this
//! possible at all.

use crate::{Result, SandboxError};
use std::path::Path;

/// The controllers `module_standard` needs handed down to it.
pub const REQUIRED_CONTROLLERS: [&str; 3] = ["memory", "pids", "cpu"];

/// A CPU bandwidth cap: `quota` microseconds of runtime per `period`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuMax {
    pub quota_us: u64,
    pub period_us: u64,
}

impl std::fmt::Display for CpuMax {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.quota_us, self.period_us)
    }
}

/// What a confined module may consume.
///
/// Every field is optional, and `None` means "not capped" rather than "zero".
/// The distinction matters: a default of zero would be a limit nobody chose,
/// applied to every module.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    pub memory_max: Option<u64>,
    pub pids_max: Option<u64>,
    pub cpu_max: Option<CpuMax>,
}

impl Limits {
    pub fn is_empty(&self) -> bool {
        self.memory_max.is_none() && self.pids_max.is_none() && self.cpu_max.is_none()
    }

    /// The controllers these limits actually need.
    pub fn controllers(&self) -> Vec<&'static str> {
        let mut needed = Vec::new();
        if self.memory_max.is_some() {
            needed.push("memory");
        }
        if self.pids_max.is_some() {
            needed.push("pids");
        }
        if self.cpu_max.is_some() {
            needed.push("cpu");
        }
        needed
    }

    /// Write these limits into a cgroup.
    ///
    /// Called before anything is placed in the cgroup, so a module is never
    /// briefly unbounded. Any limit that cannot be written fails the call.
    pub fn apply(&self, cgroup: &Path) -> Result<()> {
        if let Some(bytes) = self.memory_max {
            write_limit(cgroup, "memory.max", &bytes.to_string())?;
        }
        if let Some(count) = self.pids_max {
            write_limit(cgroup, "pids.max", &count.to_string())?;
        }
        if let Some(cpu) = self.cpu_max {
            write_limit(cgroup, "cpu.max", &cpu.to_string())?;
        }
        Ok(())
    }
}

/// The controllers a cgroup can hand down to its children.
pub fn available_controllers(cgroup: &Path) -> Result<Vec<String>> {
    let path = cgroup.join("cgroup.controllers");
    let contents =
        std::fs::read_to_string(&path).map_err(|source| SandboxError::io(&path, source))?;
    Ok(contents.split_whitespace().map(str::to_string).collect())
}

/// The controllers a cgroup is currently handing down.
pub fn enabled_controllers(cgroup: &Path) -> Result<Vec<String>> {
    let path = cgroup.join("cgroup.subtree_control");
    let contents =
        std::fs::read_to_string(&path).map_err(|source| SandboxError::io(&path, source))?;
    Ok(contents.split_whitespace().map(str::to_string).collect())
}

/// Hand the named controllers down to a cgroup's children.
///
/// Idempotent: a controller already enabled is left alone rather than written
/// again, because the kernel rejects `+memory` on a cgroup that already has it
/// in some configurations, and a spurious failure here would refuse to run a
/// module that was perfectly confinable.
pub fn delegate(parent: &Path, controllers: &[&str]) -> Result<()> {
    if controllers.is_empty() {
        return Ok(());
    }

    let available = available_controllers(parent)?;
    let missing: Vec<&str> = controllers
        .iter()
        .copied()
        .filter(|c| !available.iter().any(|a| a == c))
        .collect();

    if !missing.is_empty() {
        return Err(SandboxError::ControllersUnavailable {
            cgroup: parent.to_path_buf(),
            missing: missing.iter().map(|c| c.to_string()).collect(),
            available,
        });
    }

    let already = enabled_controllers(parent)?;
    let to_enable: Vec<&str> = controllers
        .iter()
        .copied()
        .filter(|c| !already.iter().any(|e| e == c))
        .collect();

    if to_enable.is_empty() {
        return Ok(());
    }

    let directive = to_enable
        .iter()
        .map(|c| format!("+{c}"))
        .collect::<Vec<_>>()
        .join(" ");

    let path = parent.join("cgroup.subtree_control");
    std::fs::write(&path, &directive).map_err(|source| SandboxError::io(&path, source))
}

fn write_limit(cgroup: &Path, file: &str, value: &str) -> Result<()> {
    let path = cgroup.join(file);
    std::fs::write(&path, value).map_err(|source| SandboxError::LimitNotApplied {
        limit: file.to_string(),
        value: value.to_string(),
        cgroup: cgroup.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cpu_cap_is_written_the_way_the_kernel_reads_it() {
        // `cpu.max` is two numbers on one line, quota then period. Swapping
        // them would produce a valid write and a wildly different limit.
        let cpu = CpuMax {
            quota_us: 50_000,
            period_us: 100_000,
        };
        assert_eq!(cpu.to_string(), "50000 100000");
    }

    #[test]
    fn limits_ask_only_for_the_controllers_they_need() {
        assert!(Limits::default().controllers().is_empty());

        let memory_only = Limits {
            memory_max: Some(1),
            ..Default::default()
        };
        assert_eq!(memory_only.controllers(), vec!["memory"]);

        let all = Limits {
            memory_max: Some(1),
            pids_max: Some(2),
            cpu_max: Some(CpuMax {
                quota_us: 1,
                period_us: 2,
            }),
        };
        assert_eq!(all.controllers(), vec!["memory", "pids", "cpu"]);
    }

    #[test]
    fn no_limit_is_not_the_same_as_a_limit_of_zero() {
        // A derived default that wrote `0` everywhere would cap every module
        // at nothing, and the symptom would be modules mysteriously dying.
        let none = Limits::default();
        assert!(none.is_empty());
        assert_eq!(none.memory_max, None);

        let dir = tempfile::tempdir().unwrap();
        none.apply(dir.path())
            .expect("applying nothing writes nothing");
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    #[test]
    fn a_limit_that_cannot_be_written_fails_rather_than_being_skipped() {
        // A real cgroup without the memory controller delegated to it has no
        // `memory.max` at all, and opening it fails with ENOENT. A temporary
        // directory cannot model that — writing there would simply create the
        // file — so the target here is a directory that does not exist, which
        // produces the same errno by the same route.
        //
        // The point being tested is the reaction, not the errno: the module
        // must not run.
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("no-such-cgroup");
        let limits = Limits {
            memory_max: Some(1 << 30),
            ..Default::default()
        };

        let error = limits.apply(&absent).unwrap_err();
        assert!(matches!(error, SandboxError::LimitNotApplied { .. }));

        let message = error.to_string();
        assert!(message.contains("memory.max"), "{message}");
        assert!(
            message.contains("unbounded"),
            "the message should say what running anyway would mean: {message}"
        );
    }

    #[test]
    fn delegation_names_what_is_missing_instead_of_failing_vaguely() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cgroup.controllers"), "cpuset hugetlb\n").unwrap();
        std::fs::write(dir.path().join("cgroup.subtree_control"), "").unwrap();

        let error = delegate(dir.path(), &REQUIRED_CONTROLLERS).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("memory"), "{message}");
        assert!(message.contains("cpuset"), "{message}");
    }

    #[test]
    fn delegation_only_writes_the_controllers_that_are_not_already_enabled() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cgroup.controllers"), "memory pids cpu\n").unwrap();
        std::fs::write(dir.path().join("cgroup.subtree_control"), "memory\n").unwrap();

        delegate(dir.path(), &REQUIRED_CONTROLLERS).unwrap();

        let written = std::fs::read_to_string(dir.path().join("cgroup.subtree_control")).unwrap();
        assert_eq!(written, "+pids +cpu");
    }

    #[test]
    fn delegation_with_everything_already_enabled_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cgroup.controllers"), "memory pids cpu\n").unwrap();
        std::fs::write(
            dir.path().join("cgroup.subtree_control"),
            "memory pids cpu\n",
        )
        .unwrap();

        delegate(dir.path(), &REQUIRED_CONTROLLERS).unwrap();

        let written = std::fs::read_to_string(dir.path().join("cgroup.subtree_control")).unwrap();
        assert_eq!(
            written, "memory pids cpu\n",
            "it should not have been touched"
        );
    }
}
