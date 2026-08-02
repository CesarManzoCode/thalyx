//! What the unit tests cannot prove: that the kernel agrees.
//!
//! Every defect found in this project so far came from running the system, not
//! from reviewing it. The unit tests in `cgroup.rs` exercise a directory shaped
//! like a cgroup; only these exercise a cgroup.
//!
//! They need a writable cgroup2 mount. Where there is none they report that
//! plainly instead of passing quietly — a green result that proved nothing is
//! how a security tool comes to read as armed while it is disarmed. Set
//! `THALYX_REQUIRE_CGROUP_TESTS=1` and they fail instead of skipping.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use thalyx_manifest::{Permission, PermissionKind};
use thalyx_permd::{MemoryStore, PolicyStore};
use thalyx_sandbox::{Cgroup, Confinement, cgroup, profile};

/// A scratch cgroup to create children under, or a reason there is none.
struct Arena(PathBuf);

impl Drop for Arena {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn arena(label: &str) -> Option<Arena> {
    let mount = match cgroup::mount_point() {
        Ok(mount) => mount,
        Err(error) => return unavailable(&error.to_string()),
    };

    let path = mount.join(format!("thalyx-test-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir(&path);

    match std::fs::create_dir(&path) {
        Ok(()) => Some(Arena(path)),
        Err(error) => unavailable(&format!("cannot create {}: {error}", path.display())),
    }
}

fn unavailable(reason: &str) -> Option<Arena> {
    let message =
        format!("NOT PROVEN: no writable cgroup2 filesystem available for this test ({reason})");

    assert!(
        std::env::var_os("THALYX_REQUIRE_CGROUP_TESTS").is_none(),
        "{message}"
    );

    eprintln!("{message}");
    eprintln!("  This test did not run. It did not pass.");
    None
}

fn permission(resource: &str, action: &str) -> Permission {
    Permission {
        resource: resource.to_string(),
        action: action.to_string(),
        kind: PermissionKind::Persistent,
    }
}

/// A process that stays alive until it is killed.
fn sleeper() -> Child {
    Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("sleep should exist")
}

fn reap(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Wait for a condition the kernel applies asynchronously to our writes.
fn settles(mut check: impl FnMut() -> bool) -> bool {
    for _ in 0..100 {
        if check() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

#[test]
fn a_created_cgroup_is_one_the_kernel_recognises() {
    let Some(arena) = arena("create") else { return };

    let cgroup = Cgroup::ensure(&arena.0, "org.thalyx.demo").expect("create");

    // The kernel populates the interface files itself. If it did not, this is
    // an ordinary directory and everything downstream is theatre.
    assert!(cgroup::is_cgroup2(cgroup.path()));
    assert!(cgroup.is_empty().unwrap());

    // The id the policy is keyed on is the directory's inode, and it is what
    // `bpf_get_current_cgroup_id()` will report for anything inside.
    let id = cgroup.id().unwrap();
    assert_ne!(id, 0);
    assert_eq!(id, inode(cgroup.path()));

    cgroup.remove().expect("remove");
    assert!(!cgroup.path().exists());
}

#[test]
fn a_real_process_joins_and_is_seen_as_a_member() {
    let Some(arena) = arena("join") else { return };

    let cgroup = Cgroup::ensure(&arena.0, "org.thalyx.demo").expect("create");
    let child = sleeper();
    let pid = child.id();

    cgroup.join(pid).expect("join");
    assert!(
        cgroup.contains(pid).unwrap(),
        "the kernel did not report the process as a member of the cgroup it was written into"
    );

    reap(child);

    assert!(
        settles(|| cgroup.is_empty().unwrap_or(false)),
        "the cgroup still reports members after the only process exited"
    );
    cgroup.remove().expect("remove");
}

#[test]
fn a_confinement_is_released_only_once_it_is_empty() {
    // The rule this test exists for: teardown withdraws the policy before it
    // removes the directory, and does neither while anything is still running.
    let Some(arena) = arena("release") else {
        return;
    };
    let policies = MemoryStore::new();

    let confinement = Confinement::establish(
        &policies,
        &arena.0,
        "org.thalyx.demo",
        profile::resolve(profile::DIAGNOSTIC).unwrap(),
        &[permission("net", "outbound")],
        0,
        0,
    )
    .expect("establish");

    let id = confinement.cgroup_id();
    let path = confinement.cgroup().path().to_path_buf();
    assert!(policies.get(id).unwrap().is_some());

    let child = sleeper();
    let pid = child.id();
    confinement.cgroup().join(pid).expect("join");

    // Occupied: nothing is torn down, because the process inside holds
    // permissions the human confirmed and would lose them mid-flight.
    let confinement = {
        let occupied = Confinement::establish(
            &policies,
            &arena.0,
            "org.thalyx.demo",
            profile::resolve(profile::DIAGNOSTIC).unwrap(),
            &[permission("net", "outbound")],
            0,
            0,
        )
        .expect("re-establish");
        assert!(!occupied.release().unwrap());
        assert!(path.is_dir());
        assert!(policies.get(id).unwrap().is_some());
        confinement
    };

    reap(child);
    assert!(settles(|| confinement.cgroup().is_empty().unwrap_or(false)));

    assert!(confinement.release().unwrap());
    assert_eq!(
        policies.get(id).unwrap(),
        None,
        "the policy outlived the cgroup it was keyed on; the next cgroup to be \
         given that inode would inherit it"
    );
    assert!(!path.exists());
}

#[test]
fn reusing_a_module_cgroup_keeps_the_same_identity() {
    // Two instances of one module share a cgroup and therefore one policy. If
    // the id changed between runs, the second would be enforcing against an
    // entry written for the first.
    let Some(arena) = arena("reuse") else { return };

    let first = Cgroup::ensure(&arena.0, "org.thalyx.demo").expect("create");
    let id = first.id().unwrap();

    let second = Cgroup::ensure(&arena.0, "org.thalyx.demo").expect("reuse");
    assert_eq!(second.id().unwrap(), id);
    assert_eq!(second.path(), first.path());

    first.remove().expect("remove");
}

fn inode(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).expect("metadata").ino()
}

#[test]
fn resource_limits_are_written_where_the_kernel_reads_them() {
    // Needs the controllers delegated, which a container often does not have.
    // Where they are missing this reports NOT PROVEN rather than passing: the
    // whole point of the limit is that it is really there.
    let Some(arena) = arena("limits") else { return };

    let needed = ["memory", "pids"];
    if let Err(error) = thalyx_sandbox::limits::delegate(&arena.0, &needed) {
        eprintln!("NOT PROVEN: this cgroup2 mount cannot hand down {needed:?}");
        eprintln!("  {error}");
        eprintln!("  Resource limits were not exercised. This test did not pass.");
        // Its own flag: having a cgroup2 mount and having controllers
        // delegated to you are different things, and a container commonly has
        // the first without the second.
        assert!(
            std::env::var_os("THALYX_REQUIRE_CONTROLLER_TESTS").is_none(),
            "{error}"
        );
        return;
    }

    let cgroup = Cgroup::ensure(&arena.0, "org.thalyx.demo").expect("create");
    let limits = thalyx_sandbox::Limits {
        memory_max: Some(64 << 20),
        pids_max: Some(32),
        cpu_max: None,
    };
    limits.apply(cgroup.path()).expect("apply");

    // Read back through the kernel, not through our own struct.
    let memory = std::fs::read_to_string(cgroup.path().join("memory.max")).unwrap();
    assert_eq!(memory.trim(), (64 << 20).to_string());
    let pids = std::fs::read_to_string(cgroup.path().join("pids.max")).unwrap();
    assert_eq!(pids.trim(), "32");

    cgroup.remove().expect("remove");
}
