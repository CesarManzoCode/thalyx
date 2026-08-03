//! PID 1.
//!
//! `vault/02-Arquitectura/Core-Nucleo.md`: **`thalyx` es PID 1.** Not systemd,
//! not OpenRC, not busybox-run, not s6. The decree used to leave the choice
//! open between two third-party inits, and leaving it open is how an image
//! skeleton came out using a third one nobody picked — the same way the login
//! arrived. There is no init here to inherit from because there is nothing else
//! in the image: see `vault/09-Notas-Tecnicas/Construccion-del-ISO.md`.
//!
//! ## What being PID 1 actually obliges
//!
//! Three things, and the last one is the one people forget:
//!
//! 1. **Nothing is mounted.** The kernel hands over a root filesystem and
//!    nothing else — no `/proc`, no `/dev`, no cgroups. Everything the rest of
//!    Thalyx assumes exists has to be put there first.
//! 2. **Nothing else will start anything.** The session runs because this
//!    starts it.
//! 3. **Every orphan on the machine becomes this process's child.** An init
//!    that does not reap them fills the process table with zombies until
//!    nothing can fork, and the failure arrives long after the cause.
//!
//! ## Why it reports rather than aborts
//!
//! A mount that fails does not stop the boot. It is recorded and the machine
//! comes up saying what is missing, because a system that refuses to boot tells
//! you nothing about *why* from a screen you cannot reach. `thalyx session`
//! already distinguishes absent from unreadable, and the readings it takes are
//! the same ones this produced — so a half-mounted machine boots into an honest
//! description of itself instead of a kernel panic.
//!
//! The store is the same: it is mounted here, reported here, and **never
//! created here**. See `store_disk`, which holds the reasoning and the layout.
//! A boot with no store still comes up, and says in as many words that nothing
//! it is told will survive it.

use std::path::Path;
use thalyx_syscall::mount_flags::HARDENED;
use thalyx_syscall::{EBUSY, RebootCommand, reboot};

type Fallible = Result<(), Box<dyn std::error::Error>>;

/// One filesystem the machine needs before anything else can work.
struct Needed {
    target: &'static str,
    fstype: &'static str,
    flags: u64,
    data: Option<&'static str>,
    /// Why it is here, so that removing one is a decision rather than a tidy-up.
    because: &'static str,
}

const FILESYSTEMS: &[Needed] = &[
    Needed {
        target: "/proc",
        fstype: "proc",
        flags: HARDENED,
        data: None,
        because: "every process identity Thalyx reads comes from here",
    },
    Needed {
        target: "/sys",
        fstype: "sysfs",
        flags: HARDENED,
        data: None,
        because: "the LSM order and the kernel's own reporting live under it",
    },
    Needed {
        // Not hardened: device nodes are the entire point, and the kernel
        // populates this one itself.
        target: "/dev",
        fstype: "devtmpfs",
        flags: 0,
        data: Some("mode=0755"),
        because: "there is no other way to get a console or a disk node",
    },
    Needed {
        target: "/run",
        fstype: "tmpfs",
        flags: HARDENED,
        data: Some("mode=0755"),
        because: "runtime state that must not survive a boot",
    },
    Needed {
        target: "/sys/kernel/security",
        fstype: "securityfs",
        flags: HARDENED,
        data: None,
        because: "without it the LSM order cannot even be read",
    },
    Needed {
        target: "/sys/fs/bpf",
        fstype: "bpf",
        flags: HARDENED,
        data: None,
        because: "thalyx-lsm's policy map is pinned here and read by permd",
    },
    Needed {
        target: "/sys/fs/cgroup",
        fstype: "cgroup2",
        flags: HARDENED,
        data: Some("nsdelegate"),
        because: "a module's confinement is a cgroup; without this nothing runs",
    },
];

/// What happened while coming up, in the same three-way shape the session uses.
pub struct Boot {
    pub mounted: Vec<&'static str>,
    pub failed: Vec<(&'static str, String)>,
}

fn mount_all() -> Boot {
    let mut boot = Boot {
        mounted: Vec::new(),
        failed: Vec::new(),
    };

    for needed in FILESYSTEMS {
        let target = Path::new(needed.target);
        if let Err(error) = std::fs::create_dir_all(target) {
            boot.failed.push((needed.target, error.to_string()));
            continue;
        }
        match thalyx_syscall::mount(None, target, Some(needed.fstype), needed.flags, needed.data) {
            Ok(()) => boot.mounted.push(needed.target),
            // Already mounted is not a failure. It happens when the kernel
            // mounted devtmpfs itself, which some configurations do.
            Err(error) if error.raw_os_error() == Some(EBUSY) => boot.mounted.push(needed.target),
            Err(error) => boot.failed.push((needed.target, error.to_string())),
        }
    }

    boot
}

/// Attach the kernel side before anything can ask for a permission.
///
/// Ordering rather than tidiness: `module run` refuses to start a module while
/// the policy map is absent, so a session that came up first would spend its
/// early life reporting an enforcement layer that was merely late.
fn attach_lsm() -> Result<(), String> {
    let loader = Path::new("/lib/thalyx/thalyx_lsm.bpf.o");
    if !loader.exists() {
        return Err(format!("{} is not in the image", loader.display()));
    }
    // Deliberately not implemented by shelling out to bpftool: there is no
    // bpftool in the image, and there is no shell to run it from. This is the
    // one place where the decision to ship nothing but `thalyx` has a cost that
    // has to be paid in code rather than avoided, and it is not paid yet.
    Err("loading the LSM from inside PID 1 is not implemented".to_string())
}

/// Run as PID 1: mount the world, start the session, and outlive it.
pub fn run() -> Fallible {
    println!();
    println!("  Thalyx");
    println!();

    let boot = mount_all();
    for target in &boot.mounted {
        println!("  ok  mounted {target}");
    }
    for (target, error) in &boot.failed {
        println!("  no  {target}: {error}");
    }

    // After the mounts because it reads /proc/cmdline, and before the session
    // because the session reports what is installed — and reading that from an
    // unmounted /opt/thalyx would report an empty machine rather than an
    // unmounted one.
    crate::store_disk::mount().report();

    match attach_lsm() {
        Ok(()) => println!("  ok  thalyx-lsm attached"),
        Err(reason) => println!("  no  thalyx-lsm: {reason}"),
    }

    // Turned down only now, with the boot's own reporting already printed.
    //
    // Until this point the kernel talking over Thalyx is harmless and often the
    // only clue about what went wrong. From here there is a human at a prompt,
    // and an info-level message arriving mid-line steps on it — the machine
    // looks like it stopped listening. Warnings and errors still come through,
    // and `nucleo` in the session reads the whole ring buffer, so this turns the
    // volume down and hides nothing.
    match thalyx_syscall::set_console_loglevel(4) {
        Ok(()) => println!("  ok  kernel talk  warnings and worse only; `nucleo` shows the rest"),
        // Not fatal, and not silent: a machine whose prompt keeps getting
        // interrupted should say why rather than leave you guessing.
        Err(error) => println!("  no  kernel talk  still at full volume: {error}"),
    }

    println!();

    // The session, forever. If it exits, it comes back: there is nothing else
    // for this machine to be doing, and a machine that fell out of its session
    // into nothing would be unusable and silent about why.
    loop {
        let child = std::process::Command::new("/init").arg("session").spawn();

        let child = match child {
            Ok(child) => child.id() as i32,
            Err(error) => {
                println!("  the session could not be started: {error}");
                println!("  nothing else can be done from here. Halting in 30s.");
                std::thread::sleep(std::time::Duration::from_secs(30));
                let error = reboot(RebootCommand::PowerOff);
                return Err(format!("could not power off either: {error}").into());
            }
        };

        match thalyx_syscall::wait_for(child) {
            Ok(_code) => {}
            Err(error) => println!("  waiting on the session failed: {error}"),
        }

        // Anything orphaned while the session ran is this process's now.
        while thalyx_syscall::reap_one().is_some() {}
    }
}

/// Where PID 1 mounts things, for whoever has to make sure they exist.
///
/// Exposed so the image builder's tests can check the archive against this list
/// rather than against a copy of it. Two lists that must agree, kept in two
/// places, disagree eventually — and the failure would land at the least
/// debuggable moment of a boot.
///
/// Test-only: nothing in a running system needs to ask, because the thing that
/// mounts them is right here.
#[cfg(test)]
pub fn mount_targets() -> impl Iterator<Item = &'static str> {
    FILESYSTEMS.iter().map(|n| n.target)
}

/// Report what PID 1 *would* do, without doing any of it.
///
/// Exists because the only machine that can run the real thing is one that has
/// already booted this — so on a development host the list is the only part
/// that can be checked at all.
pub fn describe() {
    println!("As PID 1, Thalyx would mount:");
    println!();
    for needed in FILESYSTEMS {
        println!("  {:<24} {}", needed.target, needed.fstype);
        println!("  {:<24} {}", "", needed.because);
    }
    println!();
    crate::store_disk::describe();
    println!();
    println!("then attach thalyx-lsm, then start the session and reap orphans");
    println!("for as long as the machine is on.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_filesystem_says_why_it_is_there() {
        // So that removing one is a decision somebody makes rather than a
        // tidy-up: an entry with no reason is an entry nobody can argue with.
        for needed in FILESYSTEMS {
            assert!(
                !needed.because.is_empty(),
                "{} has no reason recorded",
                needed.target
            );
        }
    }

    #[test]
    fn nothing_is_mounted_executable_unless_it_has_to_be() {
        // /dev is the exception and it is deliberate: device nodes are the
        // point. Every other mount carries nosuid, noexec and nodev, so a file
        // appearing under any of them cannot become a way to run something.
        for needed in FILESYSTEMS {
            if needed.target == "/dev" {
                continue;
            }
            assert_eq!(
                needed.flags & HARDENED,
                HARDENED,
                "{} is missing nosuid/noexec/nodev",
                needed.target
            );
        }
    }

    #[test]
    fn the_policy_map_and_the_cgroup_tree_are_both_mounted() {
        // The two the sandbox cannot work without, named explicitly so that a
        // reordering or a deletion trips this rather than a boot.
        let targets: Vec<&str> = FILESYSTEMS.iter().map(|n| n.target).collect();
        assert!(targets.contains(&"/sys/fs/bpf"));
        assert!(targets.contains(&"/sys/fs/cgroup"));
        assert!(targets.contains(&"/sys/kernel/security"));
    }
}
