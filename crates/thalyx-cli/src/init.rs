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

/// Where the root is re-attached from. An ordinary directory in the initramfs,
/// empty for the whole of one second and then never looked at again.
const NEW_ROOT: &str = "/newroot";

/// Get off the initramfs, so a module can be pivoted into a root of its own.
///
/// ## What this is fixing
///
/// The kernel hands PID 1 a root that is the root of its mount namespace, and
/// **the root of a mount namespace has no parent**. `do_pivot_root` refuses
/// that outright:
///
/// ```c
/// if (!mnt_has_parent(root_mnt))
///     goto out4; /* not attached */   -- EINVAL
/// ```
///
/// It survives `unshare(CLONE_NEWNS)`, because the copy is a namespace root
/// too. So a module got its cgroup, its policy, its user, its namespaces and
/// its limits — all correctly — and then `pivot_root` said `Invalid argument`
/// and nothing else.
///
/// Every other Linux is off the initramfs before anything runs: `switch_root`
/// moves the real filesystem onto `/` and `chroot`s into it, which leaves the
/// process root a *child* of the kernel's internal `rootfs`. Nobody writes
/// this down because on those systems it has already happened.
///
/// ## Why a bind and not a tmpfs
///
/// `switch_root` moves a different filesystem in, because there is one to move.
/// Here there is nothing but the initramfs, and copying it into a tmpfs would
/// duplicate the six megabytes of `/init` in RAM to change nothing.
///
/// A bind of `/` gives the same topology for free: the same inodes, the same
/// pages, one more entry in the mount table, and a process root that has a
/// parent. `__do_loopback` clears `MNT_LOCKED` on a bind, which is what makes
/// the move afterwards legal — the initramfs itself carries that flag and
/// could not be moved.
///
/// ## Why it is first
///
/// The bind is not recursive, so it must happen while nothing else is mounted.
/// A recursive one would work too and would duplicate every mount underneath
/// it; doing it first costs nothing and leaves one honest mount table.
///
/// The sequence is `switch_root`'s, and the order is not decoration: the
/// process changes directory into the new root **before** it is moved, so that
/// `chroot(".")` afterwards names a root that is already resolved. Moving
/// first and then naming the path would name the old one.
fn leave_the_initramfs() -> Result<String, String> {
    let new_root = Path::new(NEW_ROOT);
    let root = Path::new("/");

    std::fs::create_dir_all(new_root).map_err(|error| format!("{NEW_ROOT}: {error}"))?;

    thalyx_syscall::mount(Some(root), new_root, None, thalyx_syscall::MS_BIND, None)
        .map_err(|error| format!("could not bind / at {NEW_ROOT}: {error}"))?;

    thalyx_syscall::chdir(new_root)
        .map_err(|error| format!("could not enter {NEW_ROOT}: {error}"))?;

    thalyx_syscall::mount(Some(new_root), root, None, thalyx_syscall::MS_MOVE, None)
        .map_err(|error| format!("could not move {NEW_ROOT} onto /: {error}"))?;

    thalyx_syscall::chroot(Path::new("."))
        .map_err(|error| format!("could not adopt the moved root: {error}"))?;

    thalyx_syscall::chdir(root)
        .map_err(|error| format!("could not return to / after the switch: {error}"))?;

    Ok("moved off the initramfs, so a module can be pivoted into a root".to_string())
}

/// Whether a module can be pivoted into a root of its own, read from the
/// kernel rather than assumed from having done the switch above.
///
/// The switch reporting success is not the same fact as the root being
/// pivotable, and this is the one that matters. A boot that says `ok` to the
/// first and nothing about the second would look exactly like the boots that
/// installed a module and then could not run it.
fn root_is_pivotable() -> Result<String, String> {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo")
        .map_err(|error| format!("/proc/self/mountinfo could not be read: {error}"))?;

    match thalyx_sandbox::rootfs::root_mount_has_a_parent(&mountinfo) {
        Some(true) => {
            Ok("the root is attached, so a module can be given one of its own".to_string())
        }
        Some(false) => Err(
            "the root is a namespace root with no parent, so pivot_root will refuse \
             every module with EINVAL"
                .to_string(),
        ),
        // Rule 10, and it costs nothing to keep: no line for `/` is a failure
        // to read, not a root without a parent.
        None => Err("no mount for / in /proc/self/mountinfo, so this cannot be told".to_string()),
    }
}

/// Hand the resource controllers down from the cgroup root.
///
/// A cgroup's `cgroup.controllers` is whatever its parent put in
/// `cgroup.subtree_control`, and the kernel starts with the root handing down
/// nothing. On any other Linux systemd does this before anything else runs, so
/// the `thalyx` cgroup inherits `memory` and `pids` without anyone here having
/// asked. **There is no systemd in this image**, so nobody asked, and a module
/// could not be given the limits its profile declares:
///
/// ```text
/// `/sys/fs/cgroup/thalyx` cannot hand down the controller(s) ["memory", "pids"]
/// It has: []
/// ```
///
/// The refusal was right — the limits would not have applied and the module
/// would have looked bounded without being bounded. What was missing is this.
///
/// The root cgroup is the one cgroup exempt from the no-internal-process rule,
/// which is why this can be written while PID 1 itself lives there.
///
/// The list is taken from the profile every module runs under rather than
/// written out here. A second list of controllers, kept beside the first, is a
/// list that ends up disagreeing — and the disagreement would be a machine that
/// boots reporting everything fine and refuses the first module it is given.
fn delegate_controllers() -> Result<String, String> {
    let profile = thalyx_sandbox::profile::module_standard();
    let needed = profile.limits.controllers();
    if needed.is_empty() {
        return Ok("none needed by the module profile".to_string());
    }

    let root = thalyx_sandbox::cgroup::mount_point().map_err(|error| error.to_string())?;
    thalyx_sandbox::limits::delegate(&root, &needed).map_err(|error| error.to_string())?;

    Ok(format!(
        "{} handed down at {}",
        needed.join(", "),
        root.display()
    ))
}

/// The BPF object, built into this binary by `build.rs`.
///
/// `None` when `make -C lsm` had not been run when this was compiled. That is a
/// different fact from a kernel that cannot load it, and the boot says which.
pub mod embedded {
    include!(concat!(env!("OUT_DIR"), "/lsm_object.rs"));
}

/// Where the links and maps are pinned, which is where `thalyx-permd` looks.
const PIN_ROOT: &str = "/sys/fs/bpf/thalyx";

/// Attach the kernel side before anything can ask for a permission.
///
/// Ordering rather than tidiness: `module run` refuses to start a module while
/// the policy map is absent, so a session that came up first would spend its
/// early life reporting an enforcement layer that was merely late.
///
/// This used to invoke `bpftool` and look for a second file on disk. Both were
/// impossible in an image holding one program, and the message it printed
/// suggested a fix that would have broken the founding decree. The object is
/// now inside this binary and the loading is `thalyx_bpf`, which makes the four
/// `bpf(2)` calls itself.
fn attach_lsm() -> Result<String, String> {
    let Some(object) = embedded::OBJECT else {
        return Err(format!(
            "no BPF object was built into me. `make -C lsm` produces {}, \
             and this binary was compiled before it existed",
            embedded::ORIGIN
        ));
    };

    // Read before anything is created, because the failure is worth telling
    // apart: no BTF means CONFIG_DEBUG_INFO_BTF is off and the fix is a kernel
    // rebuild, not anything the loader can do.
    let kernel = thalyx_bpf::kernel_btf().map_err(|error| error.to_string())?;
    let loaded = thalyx_bpf::load(object, &kernel).map_err(|error| error.to_string())?;

    let pinned = loaded.pin(Path::new(PIN_ROOT));

    let hooks = loaded.links.len();
    let maps = loaded.maps.len();

    // Only now may the descriptors be dropped. Pinning is what makes them
    // outlive this function; dropping them first would detach enforcement
    // between one line of the boot and the next.
    drop(loaded);

    pinned.map_err(|error| error.to_string())?;
    Ok(format!(
        "{hooks} hook(s) live, {maps} map(s) pinned under {PIN_ROOT}"
    ))
}

/// Run as PID 1: mount the world, start the session, and outlive it.
pub fn run() -> Fallible {
    println!();
    println!("  Thalyx");
    println!();

    // Before the mounts, because the bind is not recursive and anything
    // already mounted would be left behind under the old root.
    match leave_the_initramfs() {
        Ok(detail) => println!("  ok  root         {detail}"),
        Err(reason) => println!("  no  root         {reason}"),
    }

    let boot = mount_all();
    for target in &boot.mounted {
        println!("  ok  mounted {target}");
    }
    for (target, error) in &boot.failed {
        println!("  no  {target}: {error}");
    }

    // Right after the mounts, because `/dev` is what `/dev/console` needs and
    // because everything after this line is something a person may want to type
    // at. `crates/thalyx-term/src/keymap.rs` has the whole reason: the kernel
    // carries one keymap and it is US QWERTY, so until this line a machine whose
    // every sentence is in Spanish could not be typed in Spanish.
    //
    // Reported and never fatal, like a mount: a keyboard that came up in the
    // wrong layout is a machine somebody can still work on, and one that refuses
    // to boot over it is not.
    let keyboard = crate::keyboard::at_boot();
    let mark = match keyboard {
        crate::keyboard::Loading::Loaded { .. } => "ok",
        // Rule 10: nothing was attempted and it was attempted and failed are
        // different facts, and the second is the one worth looking into.
        crate::keyboard::Loading::LeftAlone(_) => "?",
        crate::keyboard::Loading::Failed(_) => "no",
    };
    println!("  {mark}  keyboard     {}", keyboard.briefly());

    // Asked of the kernel, now that /proc is there. Two separate facts: the
    // switch above ran, and the root it produced is one a module can be
    // pivoted out of. Only the second one decides whether a module runs.
    match root_is_pivotable() {
        Ok(detail) => println!("  ok  sandbox root {detail}"),
        Err(reason) => println!("  no  sandbox root {reason}"),
    }

    // Straight after the mounts: the cgroup root has to be handing down what a
    // module needs before anything tries to run one, and the first thing that
    // tries could be the first thing a person types.
    match delegate_controllers() {
        Ok(detail) => println!("  ok  controllers  {detail}"),
        // Not fatal. The machine comes up and says what it cannot do, which is
        // worth more from a screen you can read than from a kernel panic you
        // cannot — and every reading the session takes will agree with this
        // line rather than contradict it.
        Err(reason) => println!("  no  controllers  {reason}"),
    }

    // After the mounts because it reads /proc/cmdline, and before the session
    // because the session reports what is installed — and reading that from an
    // unmounted /opt/thalyx would report an empty machine rather than an
    // unmounted one.
    crate::store_disk::mount().report();
    // After the store, because the runtime it points at is on the store, and
    // before the session, because the session is what answers `context` and
    // `rename`.
    crate::store_disk::link_runtime_loader();

    match attach_lsm() {
        Ok(detail) => println!("  ok  thalyx-lsm  {detail}"),
        // Multi-line on purpose: a verifier rejection names the instruction and
        // the register, and that is the whole difference between "it did not
        // load" and knowing why. Squeezing it onto one line would throw away
        // the only part worth reading.
        Err(reason) => {
            println!("  no  thalyx-lsm  {}", reason.lines().next().unwrap_or(""));
            for line in reason.lines().skip(1) {
                println!("      {line}");
            }
        }
    }

    // Turned down only now, with the boot's own reporting already printed.
    //
    // Until this point the kernel talking over Thalyx is harmless and often the
    // only clue about what went wrong. From here there is a human at a prompt,
    // and a message arriving mid-line steps on it — the machine looks like it
    // stopped listening.
    //
    // This used to be 4, and 4 was wrong twice. The first real machine to boot
    // this, on 2026-08-07, had a wireless receiver that would not enumerate, so
    // the kernel retried it forever and `usb 1-6: device descriptor read/64,
    // error -110` — priority 3, an error, correctly graded — landed on the
    // prompt every few seconds until the session was unusable with a keyboard
    // that worked perfectly. **A threshold on severity cannot see repetition**,
    // and a message that repeats without end has stopped being information.
    //
    // And 4 did not mean what the line printed here said it meant: the console
    // drops everything at priority >= level, so 4 suppressed warnings while
    // `KernelMessage::is_trouble` counts warnings as trouble. The two halves of
    // one judgement disagreed by a level, in the direction where the screen said
    // more than it showed.
    //
    // 1 leaves only emergencies, which are the messages about a machine that is
    // dying and has nothing left to interrupt. Nothing is hidden and nothing is
    // silent: the ring buffer still has every word, `nucleo` reads all of it,
    // and the prompt announces new trouble as it arrives — see
    // `session::KernelWatch`, which is the other half of this change and without
    // which this one would be exactly the hiding this system is not allowed to
    // do.
    match thalyx_syscall::set_console_loglevel(1) {
        Ok(()) => {
            println!("  ok  kernel talk  emergencies only; the prompt says when there is more")
        }
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

    /// The kernel configuration the machine is built from.
    fn kernel_config() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../image/thalyx.config")
            .canonicalize()
            .expect("image/thalyx.config is part of the repository");
        std::fs::read_to_string(&path).expect("reading image/thalyx.config")
    }

    #[test]
    fn the_last_console_on_the_built_in_line_is_the_one_a_pc_actually_has() {
        // The kernel prints to every `console=` it is given, and the **last** one
        // is what becomes `/dev/console` — which is the one file this session talks
        // through. So the order in that string is not cosmetic: it decides whether
        // a person looking at a PC sees anything at all.
        //
        // A firmware appends nothing, so the last one on this line wins there. QEMU
        // appends `console=ttyS0` after it — `arch/x86/kernel/setup.c` concatenates
        // the built-in line first and the bootloader's after — so the serial keeps
        // winning under `make run` and stage 16 is untouched.
        //
        // Worth a test because reordering two words in a string is an edit nobody
        // reviews, and the failure it causes is a machine that boots perfectly and
        // shows a blank screen. That is the third item on the risk list in
        // Construccion-del-ISO.md, and it reads as "it does not work" while being
        // "you cannot look".
        let config = kernel_config();
        let line = config
            .lines()
            .find_map(|line| line.strip_prefix("CONFIG_CMDLINE=\""))
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("thalyx.config sets a built-in command line");

        let consoles: Vec<&str> = line
            .split_whitespace()
            .filter_map(|word| word.strip_prefix("console="))
            .collect();
        assert!(
            consoles.iter().any(|console| console.starts_with("ttyS0")),
            "nothing goes to the serial port, so QEMU and stage 16 see nothing: {line}"
        );
        assert_eq!(
            consoles.last(),
            Some(&"tty0"),
            "the last console on the built-in line is not the screen, so a machine \
             started by a firmware would put its session somewhere a PC has no \
             hardware for: {line}"
        );
    }

    #[test]
    fn the_serial_console_is_told_a_speed_and_not_left_at_the_1970s_default() {
        // `console=ttyS0` with nothing after it is 9600 baud, and `printk` waits for
        // the characters to leave the port. On QEMU that is free — a pty has no baud
        // rate — so it was free on every machine this was ever tested on, and on a
        // real PC on 2026-08-07 it was about 30 of the 38.5 seconds the boot took.
        //
        // `nucleo lento` found it as an 18.27s silence at second 0.07, right after
        // the console registered and the kernel replayed the whole log buffer into
        // it. This test exists because deleting five characters from a string in a
        // config file is an edit nobody reviews, and what it costs is not visible
        // anywhere a test currently runs.
        let config = kernel_config();
        let line = config
            .lines()
            .find_map(|line| line.strip_prefix("CONFIG_CMDLINE=\""))
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("thalyx.config sets a built-in command line");

        let serial = line
            .split_whitespace()
            .filter_map(|word| word.strip_prefix("console="))
            .find(|console| console.starts_with("ttyS0"))
            .expect("the built-in line names a serial console");
        assert_eq!(
            serial, "ttyS0,115200",
            "the serial console has no speed on it, so the 8250 driver uses 9600 \
             baud and every kernel message is paid for character by character: {line}"
        );
    }

    #[test]
    fn the_kernel_is_not_left_ignoring_most_of_the_cpus_the_machine_has() {
        // A real PC said `CPU topo: CPU limit of 2 reached. Ignoring further CPUs`
        // on 2026-08-07. Nobody chose 2: `allnoconfig` runs with SMP off, where
        // NR_CPUS is 1, and turning SMP on afterwards lifts it only to the bottom of
        // its range.
        //
        // `config-check` cannot see this class of defect at all — it compares what
        // thalyx.config asks for against what came out, so an option nobody asks for
        // has no line to compare. That is why the check lives here instead.
        let config = kernel_config();
        assert!(
            config
                .lines()
                .any(|line| line.trim() == "CONFIG_NR_CPUS=64"),
            "thalyx.config does not ask for a CPU count, so olddefconfig picks the \
             bottom of the range and the machine quietly runs on two cores"
        );
    }

    #[test]
    fn the_screen_the_session_lands_on_has_a_driver_and_a_font_behind_it() {
        // `console=tty0` names a virtual terminal, and a virtual terminal with no
        // framebuffer under it is a console that registers, accepts every write,
        // and displays nothing. Same for a framebuffer console with no font
        // compiled in: it initialises, reports nothing wrong, and draws blanks.
        //
        // `config-check` in image/Makefile catches an option that Kconfig dropped.
        // It cannot catch one nobody asked for, which is the mistake that cost this
        // project CONFIG_SECURITY_NETWORK and a whole boot. This is that check, for
        // the four options the console needs to be visible.
        let config = kernel_config();
        for option in [
            "CONFIG_VT=y",
            "CONFIG_VT_CONSOLE=y",
            "CONFIG_FB_EFI=y",
            "CONFIG_FRAMEBUFFER_CONSOLE=y",
            "CONFIG_FONT_8x16=y",
        ] {
            assert!(
                config.lines().any(|line| line.trim() == option),
                "thalyx.config does not ask for {option}, so `console=tty0` would \
                 have nothing behind it"
            );
        }
    }

    #[test]
    fn the_machine_can_see_a_disk_that_is_not_qemus() {
        // `thalyx install` writes onto a block device, and until 2026-08-07 the
        // only block device this kernel could see was virtio. An installer that
        // cannot see the disk it is installing onto is not an installer, and the
        // symptom on real hardware is the store step reporting that there are no
        // disks — which reads as a broken disk.
        let config = kernel_config();
        for (option, why) in [
            (
                "CONFIG_BLK_DEV_NVME=y",
                "an NVMe disk, which is what a machine bought since about 2016 has",
            ),
            ("CONFIG_SATA_AHCI=y", "a SATA disk"),
            (
                "CONFIG_BLK_DEV_SD=y",
                "the SCSI layer AHCI hands its ports to; without it the controller is found and no /dev/sda appears",
            ),
            (
                "CONFIG_PCI_MSI=y",
                "the interrupts NVMe allocates its queues around",
            ),
            (
                "CONFIG_VIRTIO_BLK=y",
                "QEMU's disk, which every stage of verify.sh boots with",
            ),
        ] {
            assert!(
                config.lines().any(|line| line.trim() == option),
                "thalyx.config does not ask for {option}, so the machine cannot see {why}"
            );
        }
    }

    #[test]
    fn the_medium_the_machine_booted_from_is_a_disk_the_kernel_can_read_too() {
        // The firmware reads the boot medium with its own driver — the UEFI
        // specification obliges it to — so a kernel with no USB storage driver
        // still boots off a stick and shows every sign of being fine.
        //
        // The step that needs the kernel to read it is `instalar-en`, which finds
        // the medium by walking /sys/block. Without this the stick is enumerated
        // as a USB device and never becomes a block device, so `discos` does not
        // list it and the installer reports that there is no Thalyx medium — on a
        // machine that is, visibly, running off one.
        //
        // Found on 2026-08-07 by reading the config rather than by booting, which
        // is the only reason it was found before the USB stick was already in a
        // PC. Same family as the console coming free from the kernel's own
        // initramfs: a thing that worked because something nobody looked at was
        // doing it, right up until that something was not there.
        let config = kernel_config();
        for (option, why) in [
            (
                "CONFIG_USB_STORAGE=y",
                "the USB stick it booted from, which is the medium `instalar-en` reads the kernel out of",
            ),
            (
                "CONFIG_SCSI=y",
                "anything usb-storage attaches, since it presents its device through the SCSI layer",
            ),
        ] {
            assert!(
                config.lines().any(|line| line.trim() == option),
                "thalyx.config does not ask for {option}, so the machine cannot see {why}"
            );
        }
    }

    #[test]
    fn the_machine_can_be_typed_at_by_a_keyboard_that_is_not_emulated() {
        // QEMU's keyboard arrives over the serial console, so none of this was ever
        // needed and none of it can be exercised by a VM. It is the one part of the
        // exit criterion that only real hardware answers, and the failure it
        // prevents is a machine that boots, shows its prompt, and cannot be
        // answered.
        let config = kernel_config();
        for option in [
            "CONFIG_INPUT=y",
            "CONFIG_HID=y",
            "CONFIG_HID_GENERIC=y",
            "CONFIG_USB_HID=y",
            "CONFIG_USB_XHCI_HCD=y",
            // The built-in keyboard of many laptops still arrives through the PS/2
            // controller even where there is no PS/2 socket on the case.
            "CONFIG_SERIO_I8042=y",
            "CONFIG_KEYBOARD_ATKBD=y",
        ] {
            assert!(
                config.lines().any(|line| line.trim() == option),
                "thalyx.config does not ask for {option}, so a PC's keyboard would \
                 not reach the session"
            );
        }
    }

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
