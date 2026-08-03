//! The boundary where Rust's guarantees end.
//!
//! Thalyx isolates modules with namespaces, mounts and seccomp. None of those
//! are wrapped by the standard library, so somewhere the project has to call
//! the kernel directly. This crate is that somewhere, and it is the **only**
//! crate in the workspace where `unsafe` is permitted at all — everything else
//! keeps `unsafe_code = "forbid"`.
//!
//! Everything here is a thin wrapper: it converts arguments, makes one call,
//! and turns the result into an [`std::io::Result`]. There is no logic, because
//! logic in this crate would be logic nobody could check as easily.
//!
//! ## Why not a syscall wrapper crate
//!
//! `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md` decrees Thalyx's own
//! implementation of isolation. That was about not delegating the *mechanism*
//! to a third-party sandbox, and a libc binding is not that. The reason to
//! write these by hand anyway is narrower: the unsafe stays visible. A
//! dependency would move it out of sight without removing it, and what makes
//! this crate defensible is that it can be read.

use std::ffi::{CString, OsStr};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub use libc::{
    CLONE_NEWIPC, CLONE_NEWNET, CLONE_NEWNS, CLONE_NEWPID, CLONE_NEWUSER, CLONE_NEWUTS,
};
pub use libc::{
    MS_BIND, MS_NODEV, MS_NOEXEC, MS_NOSUID, MS_PRIVATE, MS_RDONLY, MS_REC, MS_REMOUNT,
};

/// Unmount lazily: detach now, let the kernel clean up when the last user goes.
pub use libc::MNT_DETACH;

/// Detach the calling process into new namespaces.
///
/// `CLONE_NEWPID` is the odd one: it does not move the caller, it makes the
/// caller's *children* the first processes of a new PID namespace. Everything
/// else takes effect immediately. See `launch.rs` for why that shapes the whole
/// two-stage launch.
pub fn unshare(flags: i32) -> io::Result<()> {
    // SAFETY: `unshare` takes an integer flag set and touches no memory. The
    // only failure modes are returning -1 with errno set, which is checked.
    #[allow(unsafe_code)]
    let result = unsafe { libc::unshare(flags) };
    check(result)
}

/// Attach a filesystem, or change the propagation of an existing mount.
///
/// `source`, `fstype` and `data` are optional because the kernel accepts NULL
/// for each depending on what is being done — a propagation change needs none
/// of them, a fresh `proc` needs only the type.
pub fn mount(
    source: Option<&Path>,
    target: &Path,
    fstype: Option<&str>,
    flags: u64,
    data: Option<&str>,
) -> io::Result<()> {
    let source = source.map(path_to_c).transpose()?;
    let target = path_to_c(target)?;
    let fstype = fstype.map(str_to_c).transpose()?;
    let data = data.map(str_to_c).transpose()?;

    // SAFETY: every pointer is either NULL or derived from a `CString` that
    // outlives the call, and each is NUL-terminated by construction. The
    // kernel only reads them.
    #[allow(unsafe_code)]
    let result = unsafe {
        libc::mount(
            source.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
            target.as_ptr(),
            fstype.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
            flags,
            data.as_ref()
                .map_or(std::ptr::null(), |s| s.as_ptr().cast()),
        )
    };
    check(result)
}

/// Detach a filesystem.
pub fn umount2(target: &Path, flags: i32) -> io::Result<()> {
    let target = path_to_c(target)?;
    // SAFETY: the pointer comes from a `CString` that outlives the call and is
    // NUL-terminated by construction. The kernel only reads it.
    #[allow(unsafe_code)]
    let result = unsafe { libc::umount2(target.as_ptr(), flags) };
    check(result)
}

/// Clone a mount into a detached tree, returning a file descriptor for it.
///
/// A detached mount can be reconfigured before anyone can see it — which is the
/// whole point: [`mount_setattr`] applies an id mapping to it, and only then is
/// it attached with [`move_mount`]. There is no instant at which the mount
/// exists in the tree with the wrong ownership.
pub fn open_tree(path: &Path, flags: u32) -> io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;

    let path = path_to_c(path)?;

    // SAFETY: the pointer comes from a `CString` that outlives the call. There
    // is no libc wrapper for this syscall.
    #[allow(unsafe_code)]
    let fd = unsafe {
        libc::syscall(
            libc::SYS_open_tree,
            libc::AT_FDCWD,
            path.as_ptr(),
            flags as libc::c_ulong,
        )
    };

    if fd < 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: the kernel returned a fresh descriptor that nothing else owns.
    #[allow(unsafe_code)]
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd as libc::c_int) })
}

/// What [`mount_setattr`] changes. Mirrors the kernel's `struct mount_attr`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct MountAttr {
    pub attr_set: u64,
    pub attr_clr: u64,
    pub propagation: u64,
    pub userns_fd: u64,
}

/// Reconfigure a mount, including remapping the ids it presents.
pub fn mount_setattr(
    fd: std::os::fd::BorrowedFd<'_>,
    flags: u32,
    attr: &MountAttr,
) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    let empty = CString::new("").expect("no interior NUL in an empty string");

    // SAFETY: `attr` outlives the call and its size is passed explicitly, so
    // the kernel reads exactly the structure that was handed to it. The path is
    // the empty string with `AT_EMPTY_PATH`, meaning "the mount this descriptor
    // refers to".
    #[allow(unsafe_code)]
    let result = unsafe {
        libc::syscall(
            libc::SYS_mount_setattr,
            fd.as_raw_fd(),
            empty.as_ptr(),
            flags as libc::c_ulong,
            attr as *const MountAttr,
            std::mem::size_of::<MountAttr>(),
        ) as libc::c_int
    };
    check(result)
}

/// Attach a detached mount to a place in the tree.
pub fn move_mount(from: std::os::fd::BorrowedFd<'_>, to: &Path) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    let empty = CString::new("").expect("no interior NUL in an empty string");
    let to = path_to_c(to)?;

    // SAFETY: both pointers come from `CString`s that outlive the call.
    #[allow(unsafe_code)]
    let result = unsafe {
        libc::syscall(
            libc::SYS_move_mount,
            fd_or_cwd(from.as_raw_fd()),
            empty.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            MOVE_MOUNT_F_EMPTY_PATH as libc::c_ulong,
        ) as libc::c_int
    };
    check(result)
}

fn fd_or_cwd(fd: libc::c_int) -> libc::c_int {
    fd
}

/// Clone the mount and everything under it.
pub const OPEN_TREE_CLONE: u32 = 1;
/// Apply to the whole subtree.
pub const AT_RECURSIVE: u32 = 0x8000;
/// The path is empty; the descriptor is the target.
pub const AT_EMPTY_PATH_U32: u32 = 0x1000;
/// The mount presents ids translated through a user namespace.
pub const MOUNT_ATTR_IDMAP: u64 = 0x0010_0000;
const MOVE_MOUNT_F_EMPTY_PATH: u32 = 0x0000_0004;

/// Make `new_root` the process's root, and move the old one to `put_old`.
///
/// The kernel requires `new_root` to be a mount point, `put_old` to be
/// underneath it, and neither to be on a shared mount. Getting any of that
/// wrong returns `EINVAL` rather than half-succeeding, which is the one thing
/// that makes this syscall pleasant to use.
pub fn pivot_root(new_root: &Path, put_old: &Path) -> io::Result<()> {
    let new_root = path_to_c(new_root)?;
    let put_old = path_to_c(put_old)?;

    // SAFETY: both pointers come from `CString`s that outlive the call. There
    // is no libc wrapper for this syscall, so it goes through `syscall(2)`.
    #[allow(unsafe_code)]
    let result = unsafe {
        libc::syscall(libc::SYS_pivot_root, new_root.as_ptr(), put_old.as_ptr()) as libc::c_int
    };
    check(result)
}

/// Change the working directory.
///
/// Needed straight after [`pivot_root`], which leaves the process's cwd
/// pointing into the old root — a directory the module must not keep a handle
/// on.
pub fn chdir(path: &Path) -> io::Result<()> {
    let path = path_to_c(path)?;
    // SAFETY: the pointer comes from a `CString` that outlives the call.
    #[allow(unsafe_code)]
    let result = unsafe { libc::chdir(path.as_ptr()) };
    check(result)
}

/// Set the hostname, which is meaningful only inside a UTS namespace.
pub fn sethostname(name: &str) -> io::Result<()> {
    let bytes = name.as_bytes();
    // SAFETY: the pointer and length describe a slice that outlives the call,
    // and the kernel only reads `len` bytes from it.
    #[allow(unsafe_code)]
    let result = unsafe { libc::sethostname(bytes.as_ptr().cast(), bytes.len()) };
    check(result)
}

/// Drop every supplementary group.
///
/// Must happen before [`set_gid`] and [`set_uid`]: once the process is no
/// longer root it can no longer change its group list, and a module that kept
/// the supplementary groups of whatever started Thalyx would carry access
/// nobody granted it.
pub fn drop_supplementary_groups() -> io::Result<()> {
    // SAFETY: a count of zero with a NULL list is the documented way to clear
    // the group list; the kernel reads no memory.
    #[allow(unsafe_code)]
    let result = unsafe { libc::setgroups(0, std::ptr::null()) };
    check(result)
}

/// Become this group, irrevocably.
pub fn set_gid(gid: u32) -> io::Result<()> {
    // SAFETY: `setresgid` takes three integers and touches no memory. All three
    // are set so no saved id is left behind for the process to return to.
    #[allow(unsafe_code)]
    let result = unsafe { libc::setresgid(gid, gid, gid) };
    check(result)
}

/// Become this user, irrevocably.
///
/// `setresuid` rather than `setuid`: `setuid` from root leaves the saved
/// set-user-id at zero, and a process with a saved uid of zero can go back.
/// Setting all three closes that door.
pub fn set_uid(uid: u32) -> io::Result<()> {
    #[allow(unsafe_code)]
    let result = unsafe { libc::setresuid(uid, uid, uid) };
    check(result)
}

/// The effective user this process is running as.
pub fn effective_uid() -> u32 {
    #[allow(unsafe_code)]
    unsafe {
        libc::geteuid()
    }
}

/// Forbid this process and its descendants from ever gaining privileges.
///
/// Required before installing a seccomp filter without `CAP_SYS_ADMIN`, and
/// worth setting regardless: it is what stops a setuid binary reachable inside
/// the sandbox from handing back what the sandbox took away.
pub fn set_no_new_privs() -> io::Result<()> {
    // SAFETY: `prctl` with this option reads no memory; the trailing arguments
    // are required by the variadic signature and ignored by the kernel.
    #[allow(unsafe_code)]
    let result = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    check(result)
}

/// One instruction of a classic BPF filter.
///
/// Laid out exactly as the kernel's `struct sock_filter`. The layout is pinned
/// by tests in `thalyx-sandbox`, because getting it wrong would not fail —
/// it would install a filter that permits something else.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub code: u16,
    pub jt: u8,
    pub jf: u8,
    pub k: u32,
}

/// Install a seccomp filter for this process and everything it execs.
///
/// The filter is irrevocable. That is the point: a module cannot undo it, and
/// neither can anything it starts.
pub fn install_seccomp_filter(program: &[Instruction]) -> io::Result<()> {
    if program.is_empty() {
        return Err(io::Error::other(
            "refusing to install an empty seccomp filter",
        ));
    }
    if program.len() > u16::MAX as usize {
        return Err(io::Error::other(
            "seccomp filter is too long for sock_fprog",
        ));
    }

    let fprog = libc::sock_fprog {
        len: program.len() as u16,
        filter: program.as_ptr() as *mut libc::sock_filter,
    };

    // SAFETY: `fprog` points at `program`, which outlives this call, and its
    // length matches the slice. The kernel copies the filter and reads nothing
    // else. `SECCOMP_SET_MODE_FILTER` is 1; the third argument is a pointer to
    // the `sock_fprog` the operation expects.
    #[allow(unsafe_code)]
    let result = unsafe {
        libc::syscall(
            libc::SYS_seccomp,
            SECCOMP_SET_MODE_FILTER,
            0,
            std::ptr::addr_of!(fprog),
        )
    };

    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

const SECCOMP_SET_MODE_FILTER: libc::c_ulong = 1;

fn path_to_c(path: &Path) -> io::Result<CString> {
    os_str_to_c(path.as_os_str())
}

fn str_to_c(value: &str) -> io::Result<CString> {
    os_str_to_c(OsStr::new(value))
}

fn os_str_to_c(value: &OsStr) -> io::Result<CString> {
    CString::new(value.as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains an interior NUL byte",
        )
    })
}

fn check(result: libc::c_int) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Swap two paths, atomically.
///
/// `renameat2` with `RENAME_EXCHANGE`. Both paths must exist; afterwards each
/// name refers to what the other one did, and there is no instant in between
/// where either is missing.
///
/// This is what makes returning a subvolume to a snapshot a single event. The
/// obvious alternative — move the live tree aside, move the snapshot in — has
/// a window where the tree the human works in does not exist at all. The data
/// is not lost there, but "published or not published, never half" is the
/// claim this project is built on, and a directory that vanishes for a
/// millisecond is half.
///
/// Not every filesystem implements it. The error is returned rather than
/// worked around here, so the caller can decide whether a slower path with a
/// recorded intent is acceptable.
pub fn exchange_paths(one: &Path, other: &Path) -> io::Result<()> {
    const RENAME_EXCHANGE: libc::c_uint = 1 << 1;

    let one = path_to_c(one)?;
    let other = path_to_c(other)?;

    // SAFETY: both pointers come from `CString`s that outlive the call, and
    // `AT_FDCWD` makes the paths resolve the way they read. glibc has no
    // wrapper for `renameat2` on every version Thalyx supports, so it goes
    // through `syscall(2)`.
    #[allow(unsafe_code)]
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            one.as_ptr(),
            libc::AT_FDCWD,
            other.as_ptr(),
            RENAME_EXCHANGE,
        ) as libc::c_int
    };
    check(result)
}

/// Set a file's modification time, in nanoseconds since the epoch.
///
/// `utimensat`. The standard library can read a modification time and cannot
/// write one, and copying a tree without carrying the times over produces
/// something that differs from its source in every file — which is exactly the
/// thing a snapshot must not do.
pub fn set_mtime(path: &Path, nanos: i64) -> io::Result<()> {
    let path = path_to_c(path)?;
    let times = [
        // UTIME_OMIT for the access time: reading a file is not a change, and
        // rewriting the atime here would make the copy differ from its source
        // in the one field nobody intended to touch.
        libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        },
        libc::timespec {
            tv_sec: nanos.div_euclid(1_000_000_000),
            tv_nsec: nanos.rem_euclid(1_000_000_000),
        },
    ];

    // SAFETY: the path pointer comes from a `CString` that outlives the call,
    // and `times` is a two-element array of exactly the type the kernel
    // expects, alive for the duration.
    #[allow(unsafe_code)]
    let result = unsafe { libc::utimensat(libc::AT_FDCWD, path.as_ptr(), times.as_ptr(), 0) };
    check(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instruction_has_the_layout_the_kernel_reads() {
        // A mismatch here does not fail loudly. It installs a filter that
        // permits something other than what was written, which is the worst
        // way for a security primitive to be wrong.
        assert_eq!(
            std::mem::size_of::<Instruction>(),
            std::mem::size_of::<libc::sock_filter>()
        );
        assert_eq!(
            std::mem::align_of::<Instruction>(),
            std::mem::align_of::<libc::sock_filter>()
        );
        assert_eq!(std::mem::size_of::<Instruction>(), 8);
    }

    #[test]
    fn an_empty_filter_is_refused_rather_than_installed() {
        // An empty program is undefined to the verifier, and a filter that
        // fails to install leaves the process running with none at all.
        assert!(install_seccomp_filter(&[]).is_err());
    }

    #[test]
    fn a_path_with_a_nul_byte_is_rejected_before_the_syscall() {
        let path = Path::new(unsafe_path_with_nul());
        assert!(mount(None, path, None, 0, None).is_err());
    }

    /// A path containing an interior NUL, which `CString` must refuse.
    fn unsafe_path_with_nul() -> &'static OsStr {
        OsStr::from_bytes(b"/tmp/a\0b")
    }
}

/// Ask the kernel to stop the machine.
///
/// Only meaningful from PID 1: `reboot(2)` from anything else either fails or
/// takes the whole system down behind the init's back, which is why this is
/// wrapped rather than called from wherever it is convenient.
///
/// Returns on failure only. On success the kernel does not come back.
pub fn reboot(command: RebootCommand) -> io::Error {
    // SAFETY: `libc::reboot` takes an integer command and touches no memory of
    // ours. The only values passed are the two constants below, both of which
    // the kernel defines.
    #[allow(unsafe_code)]
    unsafe {
        libc::reboot(command as i32);
    }
    io::Error::last_os_error()
}

#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum RebootCommand {
    PowerOff = libc::RB_POWER_OFF,
    Restart = libc::RB_AUTOBOOT,
}

/// Reap one exited child, if any has exited.
///
/// PID 1 inherits every orphan on the system, and an init that does not reap
/// them fills the process table with zombies until nothing can fork. This is
/// the non-blocking form: it answers "has anything died" without waiting.
///
/// Returns the pid reaped, or `None` when nothing was waiting.
pub fn reap_one() -> Option<i32> {
    let mut status: i32 = 0;
    // SAFETY: `status` is a valid, initialised `i32` that outlives the call,
    // and WNOHANG makes this return immediately whether or not a child exists.
    #[allow(unsafe_code)]
    let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
    (pid > 0).then_some(pid)
}

/// Block until the given child exits, reaping anything else that dies meanwhile.
///
/// Distinct from [`reap_one`] because PID 1 has two jobs at once: it waits for
/// the session, and it is also the parent of last resort for every orphan on
/// the machine. Waiting on only the session would leave those unreaped.
pub fn wait_for(child: i32) -> io::Result<i32> {
    loop {
        let mut status: i32 = 0;
        // SAFETY: as above. Blocking form, so it returns when some child exits.
        #[allow(unsafe_code)]
        let pid = unsafe { libc::waitpid(-1, &mut status, 0) };
        if pid < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if pid == child {
            // The shape libc uses: low byte holds the signal, the next holds
            // the exit code.
            return Ok(if status & 0x7f == 0 {
                (status >> 8) & 0xff
            } else {
                128 + (status & 0x7f)
            });
        }
    }
}

/// Mount flags, re-exported so that callers do not need `libc` themselves.
///
/// The point of this crate is that it is the only one allowed to touch the
/// raw interface; a caller that had to depend on `libc` for a constant would
/// be halfway to bypassing that.
pub mod mount_flags {
    pub const NOSUID: u64 = libc::MS_NOSUID;
    pub const NOEXEC: u64 = libc::MS_NOEXEC;
    pub const NODEV: u64 = libc::MS_NODEV;
    pub const RDONLY: u64 = libc::MS_RDONLY;

    /// What everything gets unless it specifically needs otherwise: no setuid
    /// bits honoured, nothing executable, no device nodes.
    pub const HARDENED: u64 = NOSUID | NOEXEC | NODEV;
}

/// `EBUSY`, which for a mount means "already mounted" and is not a failure.
pub const EBUSY: i32 = libc::EBUSY;
