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
    MS_BIND, MS_MOVE, MS_NODEV, MS_NOEXEC, MS_NOSUID, MS_PRIVATE, MS_RDONLY, MS_REC, MS_REMOUNT,
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

// ──────────────────────────────────────────────── making a Btrfs subvolume
//
// `vault/09-Notas-Tecnicas/Construccion-del-ISO.md` decrees an image holding the
// kernel and one program, so there is no `btrfs` binary to run — the same shape
// as `bpftool` for the LSM and `cpio` for the initramfs, and the same answer.
// `thalyx-snapshot` shells out to `btrfs` and is right to, because it runs on a
// host that has it; an installer running inside the image cannot.

/// `BTRFS_IOC_SUBVOL_CREATE`, from `include/uapi/linux/btrfs.h`:
/// `_IOW(BTRFS_IOCTL_MAGIC, 14, struct btrfs_ioctl_vol_args)`.
///
/// Written out because `_IOW` is a macro and this crate has no C. Not taken on
/// trust either: `thalyx-btrfs` carries that header captured verbatim and
/// `tests/ioctl.rs` recomputes this number out of its text, including the
/// argument size the encoding folds in. An ioctl number that is wrong in the
/// size field does not fail cleanly — the kernel matches on the whole word, so
/// the answer is `ENOTTY` on a filesystem that supports the call perfectly.
///
/// Typed `u64`, which is the width of an ioctl number, and converted at the call
/// site. `libc::Ioctl` is `c_ulong` against glibc and `c_int` against musl, and
/// the image is headed for a static musl build — a constant declared as either one
/// would stop compiling when the target changed, on a line that has nothing to do
/// with the target.
pub const BTRFS_IOC_SUBVOL_CREATE: u64 = 0x5000_940e;

/// The longest a subvolume name may be, `BTRFS_VOL_NAME_MAX`.
///
/// The `name` field of `btrfs_ioctl_vol_args` is 4088 bytes, and the kernel
/// nevertheless refuses anything past 255 for this ioctl. Checking the field's
/// size instead of this would send a 300-byte name to the kernel and get back a
/// bare `EINVAL`.
pub const BTRFS_VOL_NAME_MAX: usize = 255;

/// Create a Btrfs subvolume called `name` inside the directory `parent` refers to.
///
/// `parent` must be a descriptor on a directory of a mounted Btrfs filesystem —
/// the ioctl is answered by the filesystem the descriptor belongs to, so the
/// mount is the caller's business and not this crate's.
///
/// The name is refused here when it cannot be a single directory entry. That is
/// conversion rather than logic: a name holding a NUL is not a shorter name, it
/// is a different name, and passing one on would create a subvolume whose title
/// the caller never asked for.
pub fn btrfs_subvolume_create(parent: std::os::fd::BorrowedFd<'_>, name: &str) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    // `struct btrfs_ioctl_vol_args`: `__s64 fd` then `char name[4088]`. Built as
    // bytes rather than as a `repr(C)` struct for the reason `thalyx-btrfs`
    // builds every on-disk shape that way — the layout is the kernel's, and a
    // Rust type that happens to agree today is agreement by coincidence.
    const NAME_AT: usize = 8;
    const ARGS_LEN: usize = 4096;

    if name.is_empty() || name == "." || name == ".." {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{name}` cannot be a subvolume name"),
        ));
    }
    if name.len() > BTRFS_VOL_NAME_MAX {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "a subvolume name may be at most {BTRFS_VOL_NAME_MAX} bytes and `{name}` is {}",
                name.len()
            ),
        ));
    }
    if name.contains('/') || name.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{name}` is not a single name: a subvolume name holds no `/` and no NUL"),
        ));
    }

    let mut args = [0u8; ARGS_LEN];
    // `fd` is left zero: this ioctl ignores it. `BTRFS_IOC_SNAP_CREATE` shares
    // the struct and does read it, which is why the field is here at all.
    args[NAME_AT..NAME_AT + name.len()].copy_from_slice(name.as_bytes());

    // The request narrows to `c_int` on a musl target, and 0x5000940e is positive
    // in 32 bits, so nothing is lost. Asserted rather than assumed, because the
    // day it stops being true the symptom is a different ioctl being called.
    let request = BTRFS_IOC_SUBVOL_CREATE as libc::Ioctl;
    debug_assert_eq!(request as u64, BTRFS_IOC_SUBVOL_CREATE);

    // SAFETY: `args` is a 4096-byte buffer this function owns for the whole
    // call, which is exactly `sizeof(struct btrfs_ioctl_vol_args)` and the size
    // the ioctl number itself declares. The name is NUL-terminated because the
    // buffer starts zeroed and the name is shorter than the field. `parent` is
    // borrowed, so it cannot be closed underneath the call.
    #[allow(unsafe_code)]
    let result = unsafe { libc::ioctl(parent.as_raw_fd(), request, args.as_mut_ptr()) };
    check(result)
}

// ─────────────────────────────────────── making the kernel look at a new table
//
// An installer writes a partition table onto a disk the kernel already has open,
// and the kernel does not notice. Nothing appears under `/dev`, so the next step
// — writing a filesystem into partition one — has nowhere to write it. `partprobe`
// and `blockdev --rereadpt` are the two programs a person would reach for, and the
// image holds the kernel and one program. Fourth time, same answer.

/// `BLKRRPART`, from `include/uapi/linux/fs.h`: `_IO(0x12, 95)`.
///
/// Spelled out because `_IO` is a C macro and this workspace has no C.
/// `thalyx-install` carries that header captured verbatim and its `tests/ioctl.rs`
/// recomputes this number from the header's own text.
///
/// `_IO` and not `_IOW`: the call takes no argument at all, so the size field is
/// zero. A number built with a size in it would be rejected by a kernel that
/// supports the call — `ENOTTY`, which reads as "this kernel is too old".
pub const BLKRRPART: u64 = 0x125f;

/// Ask the kernel to read the partition table on `disk` again.
///
/// `disk` must be a descriptor on the **whole** block device, not on a partition
/// of it, and nothing may have a partition of that disk mounted — the kernel
/// answers `EBUSY` rather than pulling a mounted filesystem out from under its
/// users, which is the right refusal and worth passing on unchanged.
///
/// What comes back is a fact about the kernel's view, never about the bytes: a
/// table this accepts is still a table something else may reject. What it buys is
/// that `/dev/sda1` exists, so the next step has somewhere to write.
pub fn reread_partition_table(disk: std::os::fd::BorrowedFd<'_>) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    let request = BLKRRPART as libc::Ioctl;
    debug_assert_eq!(request as u64, BLKRRPART);

    // SAFETY: `BLKRRPART` takes no argument — its size field is zero, so the
    // kernel reads nothing through the third parameter. `disk` is borrowed for
    // the call, so it cannot be closed underneath it.
    #[allow(unsafe_code)]
    let result = unsafe { libc::ioctl(disk.as_raw_fd(), request, 0) };
    check(result)
}

/// The terminal put into raw mode, and put back when this is dropped.
///
/// Raw mode is what makes an arrow key reach the program instead of being
/// swallowed by the kernel's own line editor. It is also the most dangerous
/// thing in this file, and the danger is why this is a guard and not two
/// functions: **a session that exits without restoring the terminal leaves the
/// machine unusable.** No echo, no line editing, no Ctrl-C — and on the image
/// there is no second terminal to recover from, because the session *is* the
/// machine.
///
/// So the restore rides on `Drop`, which runs on the ordinary path and while a
/// panic unwinds. It cannot cover a `SIGKILL` or an abort, and nothing can.
pub struct RawMode {
    fd: std::os::fd::RawFd,
    saved: libc::termios,
}

impl RawMode {
    /// Turn off the kernel's line discipline, or say why it could not be.
    ///
    /// `None` when the input is not a terminal — a pipe has no line discipline
    /// to turn off, and treating that as a failure would stop a session from
    /// being driven by a script. The caller falls back to reading whole lines.
    pub fn enter(terminal: std::os::fd::BorrowedFd<'_>) -> Option<Self> {
        use std::os::fd::AsRawFd;
        let fd = terminal.as_raw_fd();

        // SAFETY: `tcgetattr` writes one `termios` through the pointer, which is
        // to a live local. `fd` is borrowed for the call.
        #[allow(unsafe_code)]
        let saved = unsafe {
            let mut saved: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &raw mut saved) != 0 {
                return None;
            }
            saved
        };

        let mut raw = saved;
        // ICANON: stop waiting for a newline before handing bytes over — this is
        // what lets a keystroke arrive as it is pressed. ECHO: stop the kernel
        // printing the key, because the editor draws the line itself and both
        // doing it prints everything twice.
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        // ISIG stays on deliberately. Ctrl-C must keep working: a person whose
        // machine is one terminal needs a way out that does not depend on the
        // program being well.
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;

        // SAFETY: `raw` is a live, fully initialised `termios` copied from one
        // the kernel produced. `TCSANOW` applies it without draining output.
        #[allow(unsafe_code)]
        let applied = unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw const raw) };
        if applied != 0 {
            return None;
        }
        Some(Self { fd, saved })
    }

    /// The same, and with the kernel's signal keys turned off as well.
    ///
    /// **Only the screen uses this, and the reason is the opposite of the one
    /// that keeps `ISIG` on above.** At a text prompt, Ctrl-C sending `SIGINT`
    /// is the escape hatch: the process dies and the person gets their machine
    /// back. With the console in [`GraphicsMode`] that same escape hatch is the
    /// trap — the process dies before `Drop` can put the console back, and what
    /// the person gets back is a black screen on a machine that is running
    /// fine, with no second terminal to fix it from.
    ///
    /// So the screen takes the signal keys itself and treats Ctrl-C as
    /// [`crate`]'s caller sees fit — which is to leave, restoring the console on
    /// the way out. The hatch is the same size; it just goes through `Drop`.
    pub fn enter_without_signals(terminal: std::os::fd::BorrowedFd<'_>) -> Option<Self> {
        let guard = Self::enter(terminal)?;

        // SAFETY: `guard.fd` was a terminal a moment ago, when `enter` read and
        // wrote its `termios` through it. Reading it again writes one `termios`
        // through a pointer to a live local.
        #[allow(unsafe_code)]
        let mut raw = unsafe {
            let mut raw: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(guard.fd, &raw mut raw) != 0 {
                return None;
            }
            raw
        };
        // IXON as well as ISIG: Ctrl-S with flow control on freezes the terminal,
        // and a frozen terminal in graphics mode looks exactly like a crash.
        raw.c_lflag &= !libc::ISIG;
        raw.c_iflag &= !libc::IXON;

        // SAFETY: `raw` is a live, fully initialised `termios` read from the
        // kernel and modified in two flags.
        #[allow(unsafe_code)]
        let applied = unsafe { libc::tcsetattr(guard.fd, libc::TCSANOW, &raw const raw) };
        if applied != 0 {
            return None;
        }
        // `guard` still carries the `termios` from before any of this, so
        // dropping it restores what the terminal had at the start.
        Some(guard)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // SAFETY: `self.saved` is the `termios` this guard read from the kernel
        // and has not modified. The descriptor was valid when the guard was made
        // and the guard does not outlive the borrow it came from.
        #[allow(unsafe_code)]
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &raw const self.saved);
        }
    }
}

/// `TIOCGWINSZ`, from `include/uapi/asm-generic/ioctls.h`: `0x5413`.
///
/// Spelled out for the same reason as [`BLKRRPART`]: `_IO` is a C macro and this
/// workspace has no C.
pub const TIOCGWINSZ: u64 = 0x5413;

/// How many columns wide the terminal is, or `None` when that cannot be asked.
///
/// `None` is a real answer and not a failure. Output redirected to a file has no
/// width, and a caller that treated "could not ask" as "zero columns" would lay
/// out one character per line — rule 10, in the place where getting it wrong is
/// visible on every screen.
///
/// The caller picks the fallback, because what to do without a width depends on
/// what is being printed and this function has no business deciding it.
pub fn terminal_width(terminal: std::os::fd::BorrowedFd<'_>) -> Option<u16> {
    use std::os::fd::AsRawFd;

    // The kernel's `struct winsize`: four `unsigned short`, in this order.
    // Declared here rather than borrowed from libc so the layout this code
    // depends on is written down where the ioctl number is.
    #[repr(C)]
    #[derive(Default)]
    struct WinSize {
        rows: u16,
        columns: u16,
        x_pixels: u16,
        y_pixels: u16,
    }

    let mut size = WinSize::default();
    let request = TIOCGWINSZ as libc::Ioctl;

    // SAFETY: `TIOCGWINSZ` writes exactly one `struct winsize` through the third
    // parameter, and `WinSize` is that struct with `repr(C)`. The pointer is to a
    // live local that outlives the call, and `terminal` is borrowed so it cannot
    // be closed underneath it.
    #[allow(unsafe_code)]
    let result = unsafe { libc::ioctl(terminal.as_raw_fd(), request, &raw mut size) };

    // Zero columns is what a kernel reports for something that is not a terminal
    // with a size, and it is not a width. Passed on as `None` rather than as a
    // number no layout can use.
    if result < 0 || size.columns == 0 {
        return None;
    }
    Some(size.columns)
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

/// Make `path` the process's root directory.
///
/// Not a containment boundary and never used as one here — a process with the
/// privilege to call this can walk back out of it. It exists for the one job
/// `pivot_root` cannot do: adopting a root that has just been moved into place
/// underneath the process, which is what PID 1 does to get off the initramfs.
/// See `thalyx-cli`'s `init::leave_the_initramfs`.
pub fn chroot(path: &Path) -> io::Result<()> {
    let path = path_to_c(path)?;
    // SAFETY: the pointer comes from a `CString` that outlives the call.
    #[allow(unsafe_code)]
    let result = unsafe { libc::chroot(path.as_ptr()) };
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

/// Take an exclusive lock on an open file, waiting for whoever holds it.
///
/// `flock(2)` rather than `fcntl` locking, and the difference matters: an
/// `fcntl` lock is dropped when *any* descriptor on the file is closed in the
/// process, which makes it fragile in a program that opens the store from
/// several places. A `flock` lock belongs to the open file description and is
/// released when that description goes — which for Thalyx means when the
/// process holding it exits, including when it is killed.
///
/// That last property is what makes it safe to hold across a commit. A crash
/// mid-operation releases the lock without anything having to notice, so the
/// next run reconciles rather than waiting forever on a dead holder.
pub fn lock_exclusive(fd: std::os::fd::BorrowedFd<'_>) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    // SAFETY: `flock` takes a descriptor and a flag word, and touches no
    // memory. The descriptor is borrowed, so it is open for the call.
    #[allow(unsafe_code)]
    let result = unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_EX) };
    check(result)
}

/// Take an exclusive lock only if nobody holds it. `Ok(false)` means somebody does.
///
/// Exists for the diagnostic that answers "is another Thalyx running?" without
/// blocking behind it.
pub fn try_lock_exclusive(fd: std::os::fd::BorrowedFd<'_>) -> io::Result<bool> {
    use std::os::fd::AsRawFd;

    // SAFETY: as [`lock_exclusive`]; `LOCK_NB` only changes whether the call
    // waits.
    #[allow(unsafe_code)]
    let result = unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };

    if result == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::EWOULDBLOCK) => Ok(false),
        _ => Err(error),
    }
}

/// `struct open_how`, the third argument of `openat2(2)`.
///
/// Passed by pointer with its size, so the kernel reads exactly the structure
/// this build compiled. A field added to a later kernel's version is not our
/// problem: it reads `size` bytes and rejects anything it does not recognise.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct OpenHow {
    pub flags: u64,
    pub mode: u64,
    pub resolve: u64,
}

/// Refuse to resolve outside the directory the descriptor names.
///
/// Absolute paths, `..` past the root, and symlinks pointing out are all
/// rejected by the kernel *during* resolution — which is the property no
/// userspace check can have, because a userspace check and the open that
/// follows it are two separate moments.
pub const RESOLVE_BENEATH: u64 = 0x08;

/// Refuse to traverse `/proc/self/fd`-style links, which are not really links.
pub const RESOLVE_NO_MAGICLINKS: u64 = 0x02;

/// Refuse to cross a mount point during resolution.
pub const RESOLVE_NO_XDEV: u64 = 0x01;

/// Open a path relative to a directory, with the kernel enforcing containment.
///
/// This exists because [`std::fs::canonicalize`] followed by `File::open` is
/// two operations with a gap between them, and anything that can write inside
/// the directory can swap a name for a symlink in that gap. The check and the
/// open have to be the same syscall or they are not a check at all.
pub fn open_beneath(
    dirfd: std::os::fd::BorrowedFd<'_>,
    relative: &Path,
    flags: i32,
    mode: u32,
    resolve: u64,
) -> io::Result<std::fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};

    let relative = path_to_c(relative)?;
    let how = OpenHow {
        flags: flags as u64,
        mode: mode as u64,
        resolve,
    };

    // SAFETY: both pointers outlive the call, and `open_how`'s size is passed
    // explicitly so the kernel reads exactly the structure handed to it. There
    // is no libc wrapper for this syscall.
    #[allow(unsafe_code)]
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            dirfd.as_raw_fd(),
            relative.as_ptr(),
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>(),
        )
    };

    if fd < 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: the kernel returned a fresh descriptor that nothing else owns.
    #[allow(unsafe_code)]
    Ok(unsafe { std::fs::File::from_raw_fd(fd as libc::c_int) })
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

/// A handle on a *process*, not on a number.
///
/// A pid is not an identity. Between reading `/proc/4711` and signalling 4711,
/// that process can exit and the kernel can hand the number to something else —
/// on a busy machine minutes of work can pass in that window. Every tool that
/// takes a pid on the command line has this hole and lives with it.
///
/// A pidfd closes it. The handle refers to the process itself, so a signal sent
/// through it either reaches the process it was opened for or fails with
/// `ESRCH`, and there is no third outcome where it reaches a stranger. That
/// difference is the whole reason `matar` goes through here rather than through
/// `kill(2)`.
#[derive(Debug)]
pub struct ProcessHandle {
    fd: std::os::fd::OwnedFd,
    pid: i32,
}

impl ProcessHandle {
    pub fn pid(&self) -> i32 {
        self.pid
    }
}

/// Take a handle on a living process.
///
/// `ESRCH` means it is not there — which after a `procesos` listing means it
/// exited in between, and is a different fact from "no such process ever".
/// Reported as it comes so the caller can tell a person which one happened.
pub fn open_process(pid: i32) -> io::Result<ProcessHandle> {
    // SAFETY: `pidfd_open` takes a pid and a flag word and touches no memory of
    // ours. There is no libc wrapper on every target this builds for, so it
    // goes through `syscall(2)`.
    #[allow(unsafe_code)]
    let raw = unsafe { libc::syscall(SYS_PIDFD_OPEN, pid as libc::c_long, 0 as libc::c_long) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the kernel just returned this descriptor and nothing else holds
    // it, so taking ownership here is the only claim on it.
    #[allow(unsafe_code)]
    let fd = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw as i32) };
    Ok(ProcessHandle { fd, pid })
}

/// Ask a process to stop, or make it.
///
/// Through the handle, so it cannot land on a recycled pid. `siginfo` is passed
/// as null, which tells the kernel to build the same `siginfo` an ordinary
/// `kill(2)` would — deliberately not a hand-built one, because a caller that
/// forged `si_code` would be lying to the receiving process about who signalled
/// it.
pub fn signal_process(handle: &ProcessHandle, signal: Signal) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: the descriptor is owned and open for the length of the call, and
    // the two null pointers are the documented way to ask for the default
    // `siginfo` and no flags.
    #[allow(unsafe_code)]
    let outcome = unsafe {
        libc::syscall(
            SYS_PIDFD_SEND_SIGNAL,
            handle.fd.as_raw_fd() as libc::c_long,
            signal as libc::c_long,
            std::ptr::null::<libc::c_void>(),
            0 as libc::c_long,
        )
    };
    if outcome < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The two signals `matar` sends, and nothing else.
///
/// Not an integer, so no caller can send signal 9 believing it sent 15. The
/// distinction is the whole decision a person makes when they type `forzar`:
/// one lets a program save its work and the other does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Signal {
    /// Asked to stop. A program can catch this, write what it was holding, and
    /// exit — which is why it is the default and `forzar` is a word somebody
    /// has to type.
    Terminate = libc::SIGTERM,
    /// Made to stop. Cannot be caught, so nothing gets written on the way out.
    Kill = libc::SIGKILL,
}

/// `syscall(2)` by number, because glibc grew wrappers for these later than the
/// kernels this has to run on and there is no reason to depend on which.
#[cfg(target_arch = "x86_64")]
const SYS_PIDFD_OPEN: libc::c_long = 434;
#[cfg(target_arch = "aarch64")]
const SYS_PIDFD_OPEN: libc::c_long = 434;
#[cfg(target_arch = "x86_64")]
const SYS_PIDFD_SEND_SIGNAL: libc::c_long = 424;
#[cfg(target_arch = "aarch64")]
const SYS_PIDFD_SEND_SIGNAL: libc::c_long = 424;

/// Clock ticks in a second, which is what `/proc/<pid>/stat` counts time in.
pub fn clock_ticks() -> u64 {
    // SAFETY: as above.
    #[allow(unsafe_code)]
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks > 0 { ticks as u64 } else { 100 }
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

// ─────────────────────────────────────────────── the channel a module speaks on
//
// `vault/02-Arquitectura/API-Interna-de-Modulos.md` gives a module one socket,
// already open, on a fixed descriptor. Everything a module can do to the system
// arrives on it, so how it gets there is part of the security argument and not
// plumbing: a path could resolve elsewhere, an environment variable could be
// forged, and an inherited descriptor can be neither.

/// The descriptor a module finds its channel on.
///
/// Kept here as well as in `thalyx-abi` because both sides have to agree and
/// neither depends on the other: `thalyx-abi` must stay free of `unsafe`, and
/// this crate must stay free of everything else.
pub const CHANNEL_FD: std::os::fd::RawFd = 3;

/// Let a descriptor survive `exec`.
///
/// Rust sets `FD_CLOEXEC` on everything it opens, which is the right default
/// and exactly wrong for the one descriptor whose entire purpose is to outlive
/// two `exec`s and arrive in the module.
pub fn clear_cloexec(fd: std::os::fd::BorrowedFd<'_>) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    // SAFETY: `fcntl` with `F_GETFD` reads no memory and takes no pointer; the
    // descriptor is borrowed for the duration of the call, so it cannot have
    // been closed by another owner in between.
    #[allow(unsafe_code)]
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: same call, with an integer argument. `flags` came from the
    // kernel one line above, so clearing one bit of it cannot produce a set
    // the kernel did not already accept.
    #[allow(unsafe_code)]
    let outcome = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, flags & !libc::FD_CLOEXEC) };
    if outcome < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Put a descriptor on a specific number, and leave it open across `exec`.
///
/// `dup2` deliberately does not copy `FD_CLOEXEC` to the new descriptor, which
/// is the property this relies on: the copy that lands on [`CHANNEL_FD`] is the
/// one that survives into the module.
///
/// A no-op when the descriptor is already on that number — `dup2` onto itself
/// is defined to do nothing, *including* not clearing the flag, which would
/// leave the channel silently closed at the next `exec`.
///
/// Takes numbers rather than a [`std::os::fd::BorrowedFd`] because that is what
/// the caller has. The descriptor crossed two `exec`s to get here, so no Rust
/// value owns it and none can be made to without `unsafe` — which would have to
/// happen in a crate that is not allowed any. An invalid number comes back as
/// `EBADF`, which is the same answer a borrowed descriptor would have given.
pub fn place_on(from: std::os::fd::RawFd, onto: std::os::fd::RawFd) -> io::Result<()> {
    if from == onto {
        // SAFETY: `fcntl` with `F_GETFD` reads no memory and takes no pointer.
        // A number that names nothing returns `EBADF` rather than misbehaving.
        #[allow(unsafe_code)]
        let flags = unsafe { libc::fcntl(from, libc::F_GETFD) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: same call with an integer argument, built from a flag set the
        // kernel returned one line above.
        #[allow(unsafe_code)]
        let outcome = unsafe { libc::fcntl(from, libc::F_SETFD, flags & !libc::FD_CLOEXEC) };
        if outcome < 0 {
            return Err(io::Error::last_os_error());
        }
        return Ok(());
    }

    // SAFETY: two integers, no memory. If `onto` names an open descriptor the
    // kernel closes it atomically, which is the documented behaviour and the
    // reason this is not a close-then-dup race.
    #[allow(unsafe_code)]
    let outcome = unsafe { libc::dup2(from, onto) };
    if outcome < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A second name for a descriptor, on whatever number the kernel has free.
///
/// The saving half of a redirection: [`place_on`] destroys what was on a
/// number, so the only way back is to have taken a copy of it first. A caller
/// that redirects without this leaves the process with no stdout at all, which
/// on the machine's own session means a screen that never says anything again.
pub fn duplicate(fd: std::os::fd::BorrowedFd<'_>) -> io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::{AsRawFd, FromRawFd};

    // SAFETY: one integer in, one out, no memory touched. `F_DUPFD_CLOEXEC`
    // rather than plain `dup` because a saved copy of stdout has no business
    // reaching a module across `exec` — see `clear_cloexec` for the one
    // descriptor that does.
    #[allow(unsafe_code)]
    let copy = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if copy < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `copy` is a descriptor the kernel just created and nothing else
    // owns, so making it an `OwnedFd` gives it exactly one owner.
    #[allow(unsafe_code)]
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(copy) })
}

/// A file that exists only in memory, with no name and no filesystem under it.
///
/// For catching what a verb prints while the screen holds the display. A
/// temporary file would need somewhere to put it, and the image mounts no
/// `/tmp` — `vault/02-Arquitectura/Arranque-y-Init.md`'s list is `/proc`,
/// `/sys`, `/dev`, `/run` and the three under `/sys`. Anonymous memory needs
/// none of them, cannot collide with another session's file, and is gone when
/// the descriptor closes even if the process is killed mid-verb.
pub fn memory_file(name: &str) -> io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;

    let label = std::ffi::CString::new(name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a name with a NUL in it"))?;
    // SAFETY: the pointer is to a NUL-terminated string that outlives the call,
    // which is the only requirement `memfd_create` places on it. The flag is
    // the documented close-on-exec one.
    #[allow(unsafe_code)]
    let fd = unsafe { libc::memfd_create(label.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a fresh descriptor from the kernel with no other owner.
    #[allow(unsafe_code)]
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
}

/// The channel Thalyx left open, from inside a module.
///
/// Refuses anything that is not a socket. Without that check a module started
/// by hand would pick up whatever happened to be on descriptor 3 — a log file,
/// a terminal — and start writing frames into it, which looks from the outside
/// like a module talking to a system that is not there.
pub fn inherited_channel() -> io::Result<std::os::unix::net::UnixStream> {
    use std::os::fd::FromRawFd;

    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();

    // SAFETY: `fstat` writes one `struct stat` through the pointer, which
    // points at a live allocation of exactly that type and outlives the call.
    #[allow(unsafe_code)]
    let outcome = unsafe { libc::fstat(CHANNEL_FD, stat.as_mut_ptr()) };
    if outcome < 0 {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "there is no channel on descriptor 3: this program was not started by Thalyx",
        ));
    }

    // SAFETY: `fstat` returned success, so it initialised the whole struct.
    #[allow(unsafe_code)]
    let mode = unsafe { stat.assume_init() }.st_mode;
    if mode & libc::S_IFMT != libc::S_IFSOCK {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "descriptor 3 is not a socket: this program was not started by Thalyx",
        ));
    }

    // SAFETY: the descriptor is open — `fstat` just succeeded on it — and
    // nothing else in this process owns it, because it arrived across `exec`
    // rather than from any Rust code that could still hold it.
    #[allow(unsafe_code)]
    Ok(unsafe { std::os::unix::net::UnixStream::from_raw_fd(CHANNEL_FD) })
}

/// Start a program with its channel already on [`CHANNEL_FD`].
///
/// The confined path does not need this: it re-executes `thalyx` twice, and the
/// last stage places the descriptor itself with [`place_on`] before becoming
/// the module. Nothing re-executes on the unconfined path, so the only moment
/// left to renumber the descriptor is between `fork` and `exec` — which is what
/// `pre_exec` is, and which needs `unsafe`.
///
/// Kept here rather than at the call site for that reason alone. `thalyx-core`
/// forbids `unsafe`, and a mode that exists to be honest about what it degrades
/// should not have to degrade that too.
pub fn spawn_with_channel(
    command: &mut std::process::Command,
    channel: std::os::fd::RawFd,
) -> io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;

    // SAFETY: `pre_exec` runs between `fork` and `exec`, where only
    // async-signal-safe calls are allowed. `dup2` is one — it is on POSIX's
    // list — and the closure makes no allocation, takes no lock, and touches no
    // memory shared with the parent. It captures one integer by copy.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(move || {
            if channel == CHANNEL_FD {
                let flags = libc::fcntl(channel, libc::F_GETFD);
                if flags < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::fcntl(channel, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::last_os_error());
                }
                return Ok(());
            }
            if libc::dup2(channel, CHANNEL_FD) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    command.spawn()
}

// ─────────────────────────────────────────────────────── a terminal of our own
//
// `TerminalConfirmer` refuses to confirm when stdin is not a terminal, because
// silence is not consent. That is correct, and it means anything that drives
// the session prompt has to give it a real terminal — inside QEMU the serial
// console is one, and in a test harness something has to make one.
//
// The harness used `script(1)`, and that is how the most important stage in
// `dev/verify.sh` came to be skipped in its entirety. Fedora ships `script` in
// `util-linux-script`, a subpackage that is not installed by default, so on the
// one machine that can actually verify Thalyx the stage covering four of the
// six exit-criterion steps printed NOT PROVEN and moved on.
//
// Rule 5 of `Estrategia-de-Pruebas.md`: the instrument includes the harness.
// A criterion that cannot be checked without a tool the machine may not have is
// a criterion that will not be checked. Thalyx already writes its own initramfs
// and loads its own BPF rather than inheriting a fourth thing nobody chose;
// eighty lines of `posix_openpt` is the same decision, and it removes an
// external dependency from the verification of the one thing that ends Phase 1.

/// A pseudoterminal pair: the side to drive from, and the side to hand a child.
pub struct Pty {
    /// What the driver reads and writes. The child's output arrives here and
    /// what is written here appears on the child's stdin.
    pub controller: std::os::fd::OwnedFd,
    /// What the child gets as its stdin, stdout and stderr.
    pub follower: std::os::fd::OwnedFd,
}

/// Open a pseudoterminal.
///
/// `posix_openpt` then `grantpt`, `unlockpt` and `ptsname` — in that order,
/// which is not stylistic: the follower cannot be opened until `unlockpt` has
/// run, and `ptsname` is what says which device to open.
///
/// `ptsname` returns a pointer into storage the C library owns and reuses, so
/// the name is copied out immediately rather than held. A second call anywhere
/// in the process would otherwise change what the first one returned.
pub fn open_pty() -> io::Result<Pty> {
    use std::os::fd::{FromRawFd, OwnedFd};

    // SAFETY: takes an integer flag set and returns a descriptor or -1. No
    // memory is touched.
    #[allow(unsafe_code)]
    let controller = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
    if controller < 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: the descriptor was returned by the kernel one line above and is
    // owned here; nothing else holds it. Wrapped before the fallible calls
    // below so that an early return closes it rather than leaking it.
    #[allow(unsafe_code)]
    let controller = unsafe { OwnedFd::from_raw_fd(controller) };

    {
        use std::os::fd::AsRawFd;
        let raw = controller.as_raw_fd();

        // SAFETY: both take the descriptor and touch no memory.
        #[allow(unsafe_code)]
        let granted = unsafe { libc::grantpt(raw) };
        if granted < 0 {
            return Err(io::Error::last_os_error());
        }
        #[allow(unsafe_code)]
        let unlocked = unsafe { libc::unlockpt(raw) };
        if unlocked < 0 {
            return Err(io::Error::last_os_error());
        }
    }

    let name = {
        use std::os::fd::AsRawFd;

        // SAFETY: `ptsname` returns a pointer to storage the C library owns, or
        // NULL. It is checked for NULL and the bytes are copied out before this
        // scope ends, so nothing here outlives the library's buffer — which is
        // reused by the next call from anywhere in the process.
        #[allow(unsafe_code)]
        let pointer = unsafe { libc::ptsname(controller.as_raw_fd()) };
        if pointer.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the pointer is non-NULL and, by `ptsname`'s contract, points
        // at a NUL-terminated string.
        #[allow(unsafe_code)]
        let bytes = unsafe { std::ffi::CStr::from_ptr(pointer) };
        bytes.to_bytes().to_vec()
    };

    let path = CString::new(name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "pty name contains a NUL"))?;

    // SAFETY: the pointer comes from a `CString` that outlives the call.
    #[allow(unsafe_code)]
    let follower = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_NOCTTY) };
    if follower < 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: freshly returned by the kernel, owned by nothing else.
    #[allow(unsafe_code)]
    let follower = unsafe { OwnedFd::from_raw_fd(follower) };

    Ok(Pty {
        controller,
        follower,
    })
}

/// Start a child whose stdin, stdout and stderr are a terminal it controls.
///
/// The `setsid` and `TIOCSCTTY` have to happen between `fork` and `exec`, which
/// is why this lives here: a terminal is not a controlling terminal until a
/// session leader claims it, and a process that merely has a pty on descriptor
/// 0 will still fail `isatty`-adjacent expectations around job control and
/// signals.
///
/// The order is load-bearing. `setsid` first, because a process that is already
/// a group leader cannot create a session; `TIOCSCTTY` second, because only a
/// session leader with no controlling terminal may claim one.
pub fn spawn_with_terminal(
    command: &mut std::process::Command,
    follower: std::os::fd::BorrowedFd<'_>,
) -> io::Result<std::process::Child> {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;

    let follower = follower.as_raw_fd();

    // SAFETY: `pre_exec` runs between `fork` and `exec`, where only
    // async-signal-safe calls are permitted. `setsid`, `ioctl`, `dup2` and
    // `close` are all on that list. The closure allocates nothing, takes no
    // lock, and captures one integer by copy.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(follower, libc::TIOCSCTTY, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            for target in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
                if libc::dup2(follower, target) < 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            // The original is closed only when it is not one of the three it was
            // just copied onto. Closing it unconditionally would shut the
            // terminal the child is about to use.
            if follower > libc::STDERR_FILENO {
                libc::close(follower);
            }
            Ok(())
        });
    }

    command.spawn()
}

/// Whether a descriptor is a terminal.
///
/// Used to answer the question honestly rather than by inference. A caller that
/// concluded "no terminal" from a failed read would be reporting the wrong
/// thing.
pub fn is_a_terminal(fd: std::os::fd::BorrowedFd<'_>) -> bool {
    use std::os::fd::AsRawFd;

    // SAFETY: takes a descriptor and touches no memory. Returns 1 or 0.
    #[allow(unsafe_code)]
    let answer = unsafe { libc::isatty(fd.as_raw_fd()) };
    answer == 1
}

/// Tell a terminal how big it is.
///
/// A pty the kernel has just made has **no window size** — `TIOCGWINSZ` on it
/// answers zero rows — and a full-screen program that asks correctly refuses to
/// draw on it. That is the right refusal and it made `thalyx dev pty` unable to
/// exercise the editor at all: rule 5 again, the instrument includes the
/// harness, and a pty with no window is not the terminal the harness exists to
/// supply.
///
/// So whoever makes a pty says how big it is. This is not a fallback inside
/// [`terminal_size`] — a program guessing its own screen size is the failure
/// that one refuses to commit.
pub fn set_terminal_size(
    fd: std::os::fd::BorrowedFd<'_>,
    rows: u16,
    columns: u16,
) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    let size = libc::winsize {
        ws_row: rows,
        ws_col: columns,
        // The pixel dimensions. Zero is what every terminal emulator reports for
        // these unless it is drawing graphics, and nothing here reads them.
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: `TIOCSWINSZ` reads one `winsize` through the pointer, which is to
    // a live, fully initialised local. `fd` is borrowed for the call.
    #[allow(unsafe_code)]
    let set = unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCSWINSZ, &raw const size) };
    if set != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// How many rows and columns the terminal has, or `None` if it will not say.
///
/// `None` rather than a default, and that is the decision worth writing down.
/// Assuming 80x24 when the kernel declines is how a full-screen editor draws
/// twenty-four rows onto a screen with ten and leaves fourteen rows of a file on
/// a screen that scrolled them away — the person sees a mangled file and
/// concludes the editor corrupted it. A caller that gets `None` must decide what
/// to do about it in the open, which is rule 10: this is a failure to *read* the
/// size, and it is not a size.
///
/// A pipe has no window, so this answering `None` down a pipe is correct and
/// not a fallback.
pub fn terminal_size(fd: std::os::fd::BorrowedFd<'_>) -> Option<(u16, u16)> {
    use std::os::fd::AsRawFd;

    // SAFETY: `TIOCGWINSZ` writes one `winsize` through the pointer, which is to
    // a live local zeroed first so a driver that fills only part of it cannot
    // leave the rest reading as stack garbage. `fd` is borrowed for the call.
    #[allow(unsafe_code)]
    let size = unsafe {
        let mut size: libc::winsize = std::mem::zeroed();
        if libc::ioctl(fd.as_raw_fd(), libc::TIOCGWINSZ, &raw mut size) != 0 {
            return None;
        }
        size
    };
    // A terminal that reports zero of either is one that does not know, and it
    // does happen — a serial console before anything has asked it. Zero rows is
    // not a small screen, it is no answer, and treating it as one divides the
    // editor's arithmetic by nothing.
    if size.ws_row == 0 || size.ws_col == 0 {
        return None;
    }
    Some((size.ws_row, size.ws_col))
}

// ────────────────────────────────────────────── what the kernel has been saying
//
// On a machine with no shell there is no `dmesg`, and the kernel's console
// output is the only place some failures ever appear. That is fine while it
// scrolls past during boot and stops being fine the moment somebody is trying
// to type: an info-level message arriving mid-line steps on the prompt, and the
// human cannot tell whether the machine is waiting for them.
//
// So Thalyx turns the console down and gives back a way to look. Turning it
// down without the second half would be hiding, which is the one thing this
// system is not allowed to do.

/// Where the kernel keeps what it has said.
const KMSG: &str = "/dev/kmsg";

/// How loud the kernel is on the console, as `/proc/sys/kernel/printk` sets it.
const PRINTK: &str = "/proc/sys/kernel/printk";

/// One line the kernel emitted.
pub struct KernelMessage {
    /// Syslog priority: 0 is emergency, 7 is debug.
    pub priority: u8,
    /// The kernel's own record number, which only ever goes up.
    ///
    /// Kept rather than discarded because it is the only thing that can answer
    /// "what has the kernel said **since I last looked**". Counting records
    /// cannot: the ring buffer overwrites its oldest entries, so a count can go
    /// down while messages are being added, and a session that inferred "new"
    /// from a count would go quiet exactly when the kernel was loudest.
    pub sequence: u64,
    /// Seconds since boot, as the kernel counts them.
    pub seconds: f64,
    pub text: String,
}

impl KernelMessage {
    /// Whether this is something that went wrong, rather than something that
    /// happened. `KERN_WARNING` is 4; anything below it is worse.
    pub fn is_trouble(&self) -> bool {
        self.priority <= 4
    }
}

/// Set how much of the kernel's own output reaches the console.
///
/// 4 keeps warnings and errors and drops the rest. Nothing is lost: the ring
/// buffer still has everything, and [`kernel_messages`] reads it.
pub fn set_console_loglevel(level: u8) -> io::Result<()> {
    std::fs::write(PRINTK, format!("{level}\n"))
}

/// Everything in the kernel's ring buffer, oldest first.
///
/// Opened non-blocking because a plain read at the end of the buffer waits for
/// the next message forever, and this is called from a session that has a human
/// in front of it.
///
/// Each `read` returns exactly one record and fails with `EINVAL` if the buffer
/// is too small for it, which is why this uses a fixed 8 KiB one rather than
/// anything line-oriented — the failure mode of getting that wrong is reading
/// nothing at all and reporting a quiet kernel.
pub fn kernel_messages() -> io::Result<Vec<KernelMessage>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(KMSG)?;

    let mut out = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if let Some(message) = parse_kmsg(&buffer[..read]) {
                    out.push(message);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            // The reader fell behind and records were overwritten. The kernel
            // says so and the next read resumes; losing the oldest lines is not
            // a reason to report none.
            Err(error) if error.raw_os_error() == Some(libc::EPIPE) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(out)
}

/// One `/dev/kmsg` record: `priority,sequence,microseconds,flags;text`.
///
/// Checked against a real record rather than one written here, per the rule
/// about invented fixtures:
///
/// ```text
/// 5,0,0,-;Linux version 6.18.5 (builder@sandboxing) (gcc (GCC) 15.2.0, ...
/// ```
///
/// Anything that does not parse is dropped rather than guessed at. A record
/// this does not understand is a record from a kernel that changed the format,
/// and inventing a priority for it would be worse than not showing it.
fn parse_kmsg(record: &[u8]) -> Option<KernelMessage> {
    let record = String::from_utf8_lossy(record);
    let (header, rest) = record.split_once(';')?;
    let mut fields = header.split(',');
    let priority: u8 = fields.next()?.trim().parse().ok()?;
    let sequence: u64 = fields.next()?.trim().parse().ok()?;
    let microseconds: u64 = fields.next()?.trim().parse().ok()?;

    // Continuation lines start with a space and belong to the record above.
    // Kept on one line so a message never arrives split in half.
    let text = rest
        .lines()
        .next()?
        .trim_end_matches('\n')
        .replace("\\x0a", " ");

    Some(KernelMessage {
        priority,
        sequence,
        seconds: microseconds as f64 / 1_000_000.0,
        text: text.to_string(),
    })
}

#[cfg(test)]
mod kmsg_tests {
    use super::*;

    #[test]
    fn a_real_record_parses_into_its_three_parts() {
        // Captured verbatim from /dev/kmsg on 2026-08-03, not written to match
        // what the parser expects.
        let record = b"5,0,0,-;Linux version 6.18.5 (builder@sandboxing) (gcc (GCC) 15.2.0)";
        let message = parse_kmsg(record).expect("a record this shape parses");
        assert_eq!(message.priority, 5);
        // Second field, and it is the one the prompt uses to know what the
        // kernel has said since a human last looked. Read off a captured record
        // rather than assumed, because taking the timestamp for the sequence
        // would still be monotonic and still be wrong.
        assert_eq!(message.sequence, 0);
        assert_eq!(message.seconds, 0.0);
        assert!(message.text.starts_with("Linux version 6.18.5"));
        assert!(!message.is_trouble(), "notice level is not trouble");
    }

    #[test]
    fn a_timestamp_in_microseconds_becomes_seconds() {
        let message = parse_kmsg(b"6,42,1287361,-;BTRFS: device label thalyx-store").unwrap();
        assert!(
            (message.seconds - 1.287361).abs() < 1e-9,
            "{}",
            message.seconds
        );
    }

    #[test]
    fn an_error_is_told_apart_from_something_that_merely_happened() {
        // The whole reason priority is kept. Without this the session would
        // print a kernel panic in the same ink as a device being scanned.
        assert!(
            parse_kmsg(b"3,1,0,-;something failed")
                .unwrap()
                .is_trouble()
        );
        assert!(parse_kmsg(b"0,1,0,-;emergency").unwrap().is_trouble());
        assert!(!parse_kmsg(b"7,1,0,-;debugging").unwrap().is_trouble());
    }

    #[test]
    fn a_record_that_does_not_parse_is_dropped_rather_than_guessed_at() {
        // A kernel that changed the format, or a partial read. Inventing a
        // priority for it would put an unknown line under a heading that
        // claims to know how bad it is.
        assert!(parse_kmsg(b"no semicolon here").is_none());
        assert!(parse_kmsg(b"notanumber,0,0,-;text").is_none());
        assert!(parse_kmsg(b"5,0,notanumber,-;text").is_none());
        assert!(parse_kmsg(b"").is_none());
    }
}

// ──────────────────────────────────────────────────────────── the bpf syscall
//
// `vault/09-Notas-Tecnicas/Construccion-del-ISO.md` says the image holds the
// kernel and one program. Attaching thalyx-lsm used to mean invoking bpftool,
// which is a second program, from a shell, which is a third. So Thalyx makes
// the calls itself.
//
// Everything above the syscall — reading the object, working out map shapes,
// resolving CO-RE offsets — is in `thalyx-bpf`, which forbids unsafe and needs
// no kernel to be tested. What is left here is the four calls themselves.
//
// ## Why the argument structures are written out
//
// `union bpf_attr` is one union with a member per command, and each member is a
// different length. The kernel reads exactly the number of bytes it is told,
// requires anything past its own idea of the structure to be zero, and
// zero-fills anything short. So each command gets its own `repr(C)` struct
// whose layout is the prefix of that union member, and its own size — which is
// both simpler than modelling the union and the only way the padding is
// checkable by reading.
//
// Getting a field's offset wrong here does not fail loudly. It passes a
// plausible number in the wrong slot, and the kernel does what that number
// says.

use std::os::fd::{BorrowedFd, OwnedFd};

/// `bpf` is not in libc's exports on every target, so it goes through
/// `syscall(2)` by number. 321 is x86-64.
#[cfg(target_arch = "x86_64")]
const SYS_BPF: libc::c_long = 321;
#[cfg(target_arch = "aarch64")]
const SYS_BPF: libc::c_long = 280;

/// The commands used here, by their number in `enum bpf_cmd`.
///
/// Public so that `thalyx-bpf` can check every one of them against a captured
/// copy of the uapi header. Written from memory these are exactly the kind of
/// constant that is wrong by one and says nothing about it — which has already
/// happened once here, to `BPF_LSM_MAC`.
pub mod bpf_cmd {
    pub const MAP_CREATE: u32 = 0;
    pub const MAP_LOOKUP_ELEM: u32 = 1;
    pub const MAP_UPDATE_ELEM: u32 = 2;
    pub const MAP_DELETE_ELEM: u32 = 3;
    pub const PROG_LOAD: u32 = 5;
    pub const OBJ_PIN: u32 = 6;
    pub const OBJ_GET: u32 = 7;
    pub const PROG_GET_FD_BY_ID: u32 = 13;
    pub const OBJ_GET_INFO_BY_FD: u32 = 15;
    pub const RAW_TRACEPOINT_OPEN: u32 = 17;
    pub const LINK_GET_FD_BY_ID: u32 = 30;
    pub const LINK_GET_NEXT_ID: u32 = 31;
}

/// `BPF_PROG_TYPE_LSM`.
pub const BPF_PROG_TYPE_LSM: u32 = 29;

/// `BPF_LSM_MAC`, the expected attach type for a mandatory-access-control hook.
///
/// **27, and it was written as 26 first.** 26 is `BPF_MODIFY_RETURN`, which is
/// the entry immediately before it, so the kernel applied the modify-return
/// check to an LSM hook and refused with `bpf_lsm_socket_connect() is not
/// modifiable`. That message is better than most and it still took a run on
/// real hardware to see, because nothing here can check an enum value against
/// a kernel that is not present.
///
/// So it is checked against a captured copy of the uapi header instead:
/// `crates/thalyx-bpf/tests/captured/bpf-uapi-enums.h`, and the test counts
/// the entries rather than comparing to a number somebody typed.
pub const BPF_LSM_MAC: u32 = 27;

/// Make the call. Returns the kernel's result, which for the commands here is a
/// file descriptor or a negative error.
fn bpf(command: u32, attr: &[u8]) -> io::Result<i32> {
    // SAFETY: `syscall` is variadic and the kernel reads exactly `attr.len()`
    // bytes from the pointer, which is a slice this call frame owns and which
    // outlives the call. No memory is retained by the kernel past it: every
    // command here either copies what it needs or returns a descriptor.
    #[allow(unsafe_code)]
    let result = unsafe {
        libc::syscall(
            SYS_BPF,
            command as libc::c_long,
            attr.as_ptr(),
            attr.len() as libc::c_long,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(result as i32)
}

/// A name as the kernel wants it: sixteen bytes, NUL-padded, truncated rather
/// than refused.
///
/// Truncation is deliberate. A map called something long is a cosmetic problem;
/// refusing to load enforcement over it is not.
fn kernel_name(name: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    let bytes = name.as_bytes();
    let take = bytes.len().min(15);
    out[..take].copy_from_slice(&bytes[..take]);
    out
}

#[repr(C)]
#[derive(Default)]
struct MapCreateAttr {
    map_type: u32,
    key_size: u32,
    value_size: u32,
    max_entries: u32,
    map_flags: u32,
    inner_map_fd: u32,
    numa_node: u32,
    map_name: [u8; 16],
    map_ifindex: u32,
    btf_fd: u32,
    btf_key_type_id: u32,
    btf_value_type_id: u32,
    btf_vmlinux_value_type_id: u32,
    map_extra: u64,
}

/// Create one map. The descriptor is what the programs get relocated with, so
/// it has to stay open until every program that uses it is loaded.
pub fn bpf_map_create(
    name: &str,
    map_type: u32,
    key_size: u32,
    value_size: u32,
    max_entries: u32,
    flags: u32,
) -> io::Result<OwnedFd> {
    let attr = MapCreateAttr {
        map_type,
        key_size,
        value_size,
        max_entries,
        map_flags: flags,
        map_name: kernel_name(name),
        ..Default::default()
    };
    let descriptor = bpf(bpf_cmd::MAP_CREATE, as_bytes(&attr))?;
    Ok(owned(descriptor))
}

#[repr(C)]
#[derive(Default)]
struct ProgLoadAttr {
    prog_type: u32,
    insn_cnt: u32,
    insns: u64,
    license: u64,
    log_level: u32,
    log_size: u32,
    log_buf: u64,
    kern_version: u32,
    prog_flags: u32,
    prog_name: [u8; 16],
    prog_ifindex: u32,
    expected_attach_type: u32,
    prog_btf_fd: u32,
    func_info_rec_size: u32,
    func_info: u64,
    func_info_cnt: u32,
    line_info_rec_size: u32,
    line_info: u64,
    line_info_cnt: u32,
    attach_btf_id: u32,
    attach_btf_obj_fd: u32,
    core_relo_cnt: u32,
    fd_array: u64,
    core_relos: u64,
    core_relo_rec_size: u32,
    log_true_size: u32,
}

/// What the verifier said, when it said no.
///
/// Carried rather than discarded because a rejected BPF program produces one of
/// the most specific error messages in the kernel — the instruction, the
/// register, and what it held — and throwing it away leaves `EINVAL`. The
/// project has a rule about this: the machine says which thing failed.
#[derive(Debug)]
pub struct VerifierRejection {
    pub error: io::Error,
    pub log: String,
}

impl std::fmt::Display for VerifierRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)?;
        if !self.log.is_empty() {
            // The last lines are the ones about the instruction that failed;
            // the rest is the trace that led there.
            let tail: Vec<&str> = self.log.trim_end().lines().rev().take(12).collect();
            for line in tail.into_iter().rev() {
                write!(f, "\n      {line}")?;
            }
        }
        Ok(())
    }
}

/// Load one program. `instructions` is already relocated.
///
/// A verifier log is always requested. It costs a buffer and it is the whole
/// difference between "the kernel said no" and knowing which instruction.
pub fn bpf_prog_load(
    name: &str,
    prog_type: u32,
    expected_attach_type: u32,
    attach_btf_id: u32,
    instructions: &[u8],
    license: &str,
) -> std::result::Result<OwnedFd, VerifierRejection> {
    let mut log = vec![0u8; 256 * 1024];
    let license = std::ffi::CString::new(license).unwrap_or_default();

    let attr = ProgLoadAttr {
        prog_type,
        insn_cnt: (instructions.len() / 8) as u32,
        insns: instructions.as_ptr() as u64,
        license: license.as_ptr() as u64,
        log_level: 1,
        log_size: log.len() as u32,
        log_buf: log.as_mut_ptr() as u64,
        prog_name: kernel_name(name),
        expected_attach_type,
        attach_btf_id,
        ..Default::default()
    };

    match bpf(bpf_cmd::PROG_LOAD, as_bytes(&attr)) {
        Ok(descriptor) => Ok(owned(descriptor)),
        Err(error) => {
            let end = log.iter().position(|b| *b == 0).unwrap_or(0);
            Err(VerifierRejection {
                error,
                log: String::from_utf8_lossy(&log[..end]).into_owned(),
            })
        }
    }
}

#[repr(C)]
#[derive(Default)]
struct RawTracepointAttr {
    name: u64,
    prog_fd: u32,
    padding: u32,
    cookie: u64,
}

/// Put a loaded LSM program into the kernel's decision path.
///
/// The returned descriptor is a **link**, and it is what makes the program live.
/// A loaded program that is not linked enforces nothing while looking, to
/// anything that lists programs, exactly like one that does — which is why
/// `make status` counts links and not pins.
pub fn bpf_attach_lsm(program: BorrowedFd<'_>) -> io::Result<OwnedFd> {
    use std::os::fd::AsRawFd;
    let attr = RawTracepointAttr {
        // Null: for an LSM program the kernel takes the attach point from the
        // program's own attach_btf_id rather than from a name here.
        name: 0,
        prog_fd: program.as_raw_fd() as u32,
        ..Default::default()
    };
    Ok(owned(bpf(bpf_cmd::RAW_TRACEPOINT_OPEN, as_bytes(&attr))?))
}

#[repr(C)]
#[derive(Default)]
struct ObjPinAttr {
    pathname: u64,
    bpf_fd: u32,
    file_flags: u32,
}

/// Pin a map or a link into bpffs so it outlives the process that made it.
///
/// Without this every map and link is freed when Thalyx's descriptors close,
/// which for PID 1 is never — but `thalyx-permd` runs as a separate process and
/// finds the policy map by its pin. Unpinned, enforcement would exist and
/// nothing could write a permission into it.
pub fn bpf_obj_pin(object: BorrowedFd<'_>, path: &Path) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let path = path_to_c(path)?;
    let attr = ObjPinAttr {
        pathname: path.as_ptr() as u64,
        bpf_fd: object.as_raw_fd() as u32,
        file_flags: 0,
    };
    bpf(bpf_cmd::OBJ_PIN, as_bytes(&attr))?;
    Ok(())
}

/// Open something that was pinned into bpffs, by its path.
///
/// The other half of [`bpf_obj_pin`]. `thalyx-permd` writes a module's
/// permissions into the policy map, and it is a different process from the one
/// that created it — the pin is the only way back to that map.
///
/// This used to be `bpftool map update pinned ...`, which is a second program
/// and a shell to invoke it from. Inside the image there is neither, so every
/// permission write failed and `correr` refused to run anything confined,
/// **including on a machine whose enforcement was attached and working.**
pub fn bpf_obj_get(path: &Path) -> io::Result<OwnedFd> {
    let path = path_to_c(path)?;
    let attr = ObjPinAttr {
        pathname: path.as_ptr() as u64,
        bpf_fd: 0,
        file_flags: 0,
    };
    let descriptor = bpf(bpf_cmd::OBJ_GET, as_bytes(&attr))?;
    Ok(owned(descriptor))
}

#[repr(C)]
#[derive(Default)]
struct MapElemAttr {
    map_fd: u32,
    /// The kernel reads a `u64` here even on 32-bit, and the field after it is
    /// where the padding of the previous one would otherwise land. Written out
    /// rather than left to alignment, because a wrong offset here is a key read
    /// from the wrong bytes and a permission granted to the wrong cgroup.
    _pad: u32,
    key: u64,
    value_or_next_key: u64,
    flags: u64,
}

/// Write one entry. `flags` is BPF_ANY: create it or replace it.
///
/// The buffers have to outlive the call and the kernel copies out of them, so
/// they are borrowed rather than owned — a caller that passed a temporary would
/// be handing the kernel a pointer into a dropped allocation.
pub fn bpf_map_update(map: BorrowedFd<'_>, key: &[u8], value: &[u8]) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let attr = MapElemAttr {
        map_fd: map.as_raw_fd() as u32,
        key: key.as_ptr() as u64,
        value_or_next_key: value.as_ptr() as u64,
        ..Default::default()
    };
    bpf(bpf_cmd::MAP_UPDATE_ELEM, as_bytes(&attr))?;
    Ok(())
}

/// Remove one entry. `ENOENT` is the caller's to interpret.
pub fn bpf_map_delete(map: BorrowedFd<'_>, key: &[u8]) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let attr = MapElemAttr {
        map_fd: map.as_raw_fd() as u32,
        key: key.as_ptr() as u64,
        ..Default::default()
    };
    bpf(bpf_cmd::MAP_DELETE_ELEM, as_bytes(&attr))?;
    Ok(())
}

/// Read one entry back, into a buffer the caller sized.
///
/// `Ok(false)` means the kernel said ENOENT — there is no such key. Every other
/// error is returned, because "I could not look" and "it is not there" are
/// different answers and collapsing them is rule 10.
pub fn bpf_map_lookup(map: BorrowedFd<'_>, key: &[u8], value: &mut [u8]) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    let attr = MapElemAttr {
        map_fd: map.as_raw_fd() as u32,
        key: key.as_ptr() as u64,
        value_or_next_key: value.as_mut_ptr() as u64,
        ..Default::default()
    };
    match bpf(bpf_cmd::MAP_LOOKUP_ELEM, as_bytes(&attr)) {
        Ok(_) => Ok(true),
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(false),
        Err(error) => Err(error),
    }
}

#[repr(C)]
#[derive(Default)]
struct IdAttr {
    /// `start_id` walking, `id` fetching. One field, two names, one union.
    id: u32,
    /// Written by the kernel, not by this side.
    next_id: u32,
    open_flags: u32,
}

#[repr(C)]
#[derive(Default)]
struct ObjInfoAttr {
    bpf_fd: u32,
    info_len: u32,
    info: u64,
}

/// The prefix of `struct bpf_link_info` that is the same for every link type.
///
/// Everything past `prog_id` is a union of one structure per kind of link, so
/// this deliberately stops here — and the kernel copies out only the bytes it
/// is asked for, which makes a short structure the supported way to read a
/// prefix rather than a trick.
///
/// `link_type` is never read. It is here because removing it would move the two
/// fields that are read by four bytes each, and the kernel would fill them from
/// the wrong place while every one of these tests still passed.
#[repr(C)]
#[derive(Default)]
#[allow(dead_code)]
struct LinkInfo {
    link_type: u32,
    id: u32,
    prog_id: u32,
}

/// `struct bpf_prog_info` up to and including `name`.
///
/// The offsets are not guessable and getting one wrong does not fail: `name`
/// would be read from the middle of some other field and the program would be
/// reported as called whatever those bytes spell. So the layout is checked
/// against a captured copy of the header in `thalyx-bpf`, which computes the
/// offsets rather than comparing them to numbers somebody typed.
///
/// Only `prog_type` and `name` are ever read. Every other field is here to put
/// those two where the kernel writes them, so none of them may be deleted.
#[repr(C)]
#[derive(Default)]
#[allow(dead_code)]
struct ProgInfo {
    prog_type: u32,
    id: u32,
    tag: [u8; 8],
    jited_prog_len: u32,
    xlated_prog_len: u32,
    jited_prog_insns: u64,
    xlated_prog_insns: u64,
    load_time: u64,
    created_by_uid: u32,
    nr_map_ids: u32,
    map_ids: u64,
    name: [u8; 16],
}

/// Where this crate believes the fields it reads live.
///
/// Exposed so that `thalyx-bpf` can check them against a captured copy of the
/// uapi header rather than against a number somebody typed twice. Getting
/// `name` wrong does not fail: it reads sixteen bytes of another field and
/// reports a program called whatever those bytes spell, and Thalyx would then
/// say enforcement is absent on a machine where it is live.
pub mod info_layout {
    use super::{LinkInfo, ProgInfo};

    pub const PROG_NAME_OFFSET: usize = std::mem::offset_of!(ProgInfo, name);
    pub const PROG_PREFIX_LEN: usize = std::mem::size_of::<ProgInfo>();
    pub const LINK_PROG_ID_OFFSET: usize = std::mem::offset_of!(LinkInfo, prog_id);
    pub const LINK_PREFIX_LEN: usize = std::mem::size_of::<LinkInfo>();

    use super::MapElemAttr;

    pub const ELEM_KEY_OFFSET: usize = std::mem::offset_of!(MapElemAttr, key);
    pub const ELEM_VALUE_OFFSET: usize = std::mem::offset_of!(MapElemAttr, value_or_next_key);
    pub const ELEM_FLAGS_OFFSET: usize = std::mem::offset_of!(MapElemAttr, flags);
}

/// One link that is live in the kernel right now, and the program it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveLink {
    pub link_id: u32,
    pub program_id: u32,
    pub program_type: u32,
    /// As the kernel holds it: at most fifteen characters.
    pub program: String,
}

/// Every BPF link the kernel currently has, with the program each one runs.
///
/// This is the only honest answer to "is enforcement attached?". A pinned map
/// says a loader ran. A pinned program says it loaded. Neither says the program
/// is in a decision path, and a program that is loaded and unlinked lists
/// identically to one that is live — which is exactly how a security tool reads
/// as armed while it is disarmed.
///
/// Needs `CAP_SYS_ADMIN`. Without it the walk fails with `EPERM` on the first
/// step, and that is returned as an error rather than as an empty list: a
/// failure to read is not a failure to exist, and reporting "nothing is
/// attached" to a process that was not allowed to look would be the same lie in
/// the other direction.
pub fn live_links() -> io::Result<Vec<LiveLink>> {
    use std::os::fd::AsFd;

    let mut out = Vec::new();
    let mut cursor = 0u32;

    loop {
        // Mutable, because this is the one command here whose argument
        // structure the kernel writes into: `next_id` is an output. Passing it
        // through the shared-reference path every other command uses would have
        // the kernel writing through a `&`, which is not something this
        // language lets you take back later.
        let mut attr = IdAttr {
            id: cursor,
            ..Default::default()
        };
        match bpf_writing(bpf_cmd::LINK_GET_NEXT_ID, as_bytes_mut(&mut attr)) {
            Ok(_) => {}
            // The documented end of the walk, and the only error that means
            // "there are no more" rather than "something went wrong".
            Err(error) if error.raw_os_error() == Some(libc::ENOENT) => break,
            Err(error) => return Err(error),
        }
        cursor = attr.next_id;

        let link = match bpf(
            bpf_cmd::LINK_GET_FD_BY_ID,
            as_bytes(&IdAttr {
                id: cursor,
                ..Default::default()
            }),
        ) {
            Ok(descriptor) => owned(descriptor),
            // A link that went away between being listed and being opened is
            // not an error: something detached while this was looking, and the
            // rest of the walk is still true.
            Err(error) if error.raw_os_error() == Some(libc::ENOENT) => continue,
            Err(error) => return Err(error),
        };

        let mut info = LinkInfo::default();
        object_info(link.as_fd(), &mut info)?;

        let program = match bpf(
            bpf_cmd::PROG_GET_FD_BY_ID,
            as_bytes(&IdAttr {
                id: info.prog_id,
                ..Default::default()
            }),
        ) {
            Ok(descriptor) => owned(descriptor),
            Err(error) if error.raw_os_error() == Some(libc::ENOENT) => continue,
            Err(error) => return Err(error),
        };

        let mut program_info = ProgInfo::default();
        object_info(program.as_fd(), &mut program_info)?;

        out.push(LiveLink {
            link_id: info.id,
            program_id: info.prog_id,
            program_type: program_info.prog_type,
            program: from_kernel_name(&program_info.name),
        });
    }

    Ok(out)
}

/// Ask the kernel to fill in an info structure for an open object.
fn object_info<T>(object: BorrowedFd<'_>, info: &mut T) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let attr = ObjInfoAttr {
        bpf_fd: object.as_raw_fd() as u32,
        info_len: std::mem::size_of::<T>() as u32,
        info: std::ptr::from_mut(info) as u64,
    };
    bpf(bpf_cmd::OBJ_GET_INFO_BY_FD, as_bytes(&attr))?;
    Ok(())
}

/// The name the kernel holds, which is at most fifteen characters and NUL-padded.
fn from_kernel_name(bytes: &[u8; 16]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// What a name will look like once the kernel has it.
///
/// Callers compare the names in an object against the names the kernel reports,
/// and `thalyx_socket_connect` is twenty-one characters. Comparing the full
/// name to the truncated one finds nothing, and "enforcement is not attached"
/// would be the answer on a machine where it is.
pub fn kernel_visible_name(name: &str) -> String {
    from_kernel_name(&kernel_name(name))
}

/// Make the call with a buffer the kernel is allowed to write back into.
///
/// Separate from `bpf` so that the commands with no output field keep taking a
/// shared reference: which of them writes back is a property worth being able
/// to see in the signature.
fn bpf_writing(command: u32, attr: &mut [u8]) -> io::Result<i32> {
    // SAFETY: the same contract as `bpf`, with the one difference that the
    // kernel writes up to `attr.len()` bytes back. The pointer is derived from
    // a `&mut` that outlives the call, so nothing else can be reading it.
    #[allow(unsafe_code)]
    let result = unsafe {
        libc::syscall(
            SYS_BPF,
            command as libc::c_long,
            attr.as_mut_ptr(),
            attr.len() as libc::c_long,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(result as i32)
}

/// The bytes of an argument structure, exactly as long as the structure is.
fn as_bytes<T>(value: &T) -> &[u8] {
    // SAFETY: `T` is one of the `repr(C)` argument structures above, all of
    // which are plain data with no padding this reads as uninitialised — every
    // one is built with `..Default::default()`, so every byte including padding
    // was written by zeroing. The slice borrows `value` and cannot outlive it.
    #[allow(unsafe_code)]
    unsafe {
        std::slice::from_raw_parts(
            std::ptr::from_ref(value).cast::<u8>(),
            std::mem::size_of::<T>(),
        )
    }
}

/// The bytes of an argument structure the kernel may write into.
fn as_bytes_mut<T>(value: &mut T) -> &mut [u8] {
    // SAFETY: `T` is a `repr(C)` argument structure of plain data, built by
    // zeroing, and the slice borrows `value` exclusively for as long as it
    // exists — so nothing else can observe the bytes while the kernel changes
    // them.
    #[allow(unsafe_code)]
    unsafe {
        std::slice::from_raw_parts_mut(
            std::ptr::from_mut(value).cast::<u8>(),
            std::mem::size_of::<T>(),
        )
    }
}

/// A region of another object's memory, mapped into this process.
///
/// Held rather than returned as a slice, because the mapping has to be undone
/// and a slice cannot carry that. `Drop` is the whole reason this is a type: a
/// consumer that leaked one mapping per read would exhaust the address space of
/// a long-running session, slowly, in a way that looks like a memory leak in
/// something else.
pub struct Mapped {
    address: *mut libc::c_void,
    length: usize,
}

// SAFETY: the pointer is a private view of a shared mapping and is only ever
// read through `bytes`, which borrows `self`. Nothing here has interior
// mutability of its own, so moving the handle between threads is no different
// from moving a raw pointer that nothing else dereferences.
#[allow(unsafe_code)]
unsafe impl Send for Mapped {}

impl Mapped {
    /// The mapped bytes.
    ///
    /// **Another process is writing into this.** For the BPF ring buffer that is
    /// the kernel, and it is the reason the caller must read positions before
    /// records and must never trust a length it has not bounds-checked — the
    /// bytes can change between two reads of the same offset.
    pub fn bytes(&self) -> &[u8] {
        // SAFETY: `address` and `length` come from a successful `mmap`, which
        // returns a mapping of exactly that length, and the mapping lives
        // exactly as long as `self` because `Drop` is what unmaps it. The
        // returned slice borrows `self`, so it cannot outlive the mapping.
        #[allow(unsafe_code)]
        unsafe {
            std::slice::from_raw_parts(self.address.cast::<u8>(), self.length)
        }
    }

    /// Write eight bytes at the start of the mapping.
    ///
    /// Only used for one thing: the BPF ring buffer's consumer position, which
    /// is the single value a consumer is allowed to write and which the kernel
    /// reads to know what may be reclaimed. Narrow on purpose — a general
    /// "write anywhere in this mapping" would be a much larger promise than
    /// anything here needs.
    pub fn write_first_u64(&self, value: u64) {
        if self.length < 8 {
            return;
        }
        // SAFETY: the mapping is at least eight bytes and was created writable
        // by the only constructor that allows this — `map_shared` with
        // `writable` set. `write_volatile` because the kernel reads this and the
        // compiler must not sink or elide the store.
        #[allow(unsafe_code)]
        unsafe {
            std::ptr::write_volatile(self.address.cast::<u64>(), value);
        }
    }
}

impl Drop for Mapped {
    fn drop(&mut self) {
        // SAFETY: unmapping exactly what was mapped, once, at the end of this
        // value's life. Nothing else holds the address: `bytes` borrows `self`.
        #[allow(unsafe_code)]
        unsafe {
            libc::munmap(self.address, self.length);
        }
    }
}

/// Map part of an object into this process, shared with whoever else has it.
///
/// `MAP_SHARED` and not `MAP_PRIVATE`: the point is to see what the kernel
/// writes. A private mapping would copy on first write and then quietly show a
/// snapshot of a ring buffer that is still moving, which is the failure mode
/// that looks like events disappearing.
pub fn map_shared(
    object: BorrowedFd<'_>,
    offset: u64,
    length: usize,
    writable: bool,
) -> io::Result<Mapped> {
    use std::os::fd::AsRawFd;

    let protection = if writable {
        libc::PROT_READ | libc::PROT_WRITE
    } else {
        libc::PROT_READ
    };

    // SAFETY: a null hint lets the kernel choose the address; `length` and
    // `offset` are passed through and validated by the kernel, which returns
    // MAP_FAILED rather than a bad mapping. The descriptor is borrowed for the
    // call only — `mmap` does not keep it, the mapping outlives it by design.
    #[allow(unsafe_code)]
    let address = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            length,
            protection,
            libc::MAP_SHARED,
            object.as_raw_fd(),
            offset as libc::off_t,
        )
    };

    if address == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    Ok(Mapped { address, length })
}

/// The kernel's page size.
///
/// Asked rather than assumed to be 4096. The BPF ring buffer's layout is
/// expressed in pages — its consumer position is one page, and the data area
/// begins one page into the producer mapping — so a wrong answer here does not
/// produce a smaller mapping, it produces one that reads the position where the
/// data should be.
///
/// `/proc/<pid>/statm` counts in pages too, so `procesos` reads this as well.
/// It is 16384 on some aarch64 kernels, and a memory figure four times too
/// small is worse than none — it is one somebody would act on.
pub fn page_size() -> usize {
    // SAFETY: `sysconf` reads a kernel-provided constant and touches no memory
    // this side owns. It returns -1 only for an unknown name, which
    // `_SC_PAGESIZE` is not on any Linux.
    #[allow(unsafe_code)]
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size > 0 { size as usize } else { 4096 }
}

/// Take ownership of a descriptor the kernel just returned.
fn owned(descriptor: i32) -> OwnedFd {
    use std::os::fd::FromRawFd;
    // SAFETY: `descriptor` came from a successful bpf(2), which returns a fresh
    // descriptor owned by this process and held by nothing else.
    #[allow(unsafe_code)]
    unsafe {
        OwnedFd::from_raw_fd(descriptor)
    }
}

#[cfg(test)]
mod bpf_tests {
    use super::*;

    #[test]
    fn each_argument_structure_is_the_length_the_kernel_expects() {
        // The kernel reads exactly as many bytes as it is told and requires
        // anything past its own structure to be zero. A field at the wrong
        // offset does not fail loudly: it passes a plausible number in the
        // wrong slot. These numbers come from `union bpf_attr` in
        // include/uapi/linux/bpf.h and are the one thing here that cannot be
        // checked by running it on a machine with no BPF.
        assert_eq!(std::mem::size_of::<MapCreateAttr>(), 72);
        assert_eq!(std::mem::size_of::<ProgLoadAttr>(), 144);
        assert_eq!(std::mem::size_of::<RawTracepointAttr>(), 24);
        assert_eq!(std::mem::size_of::<ObjPinAttr>(), 16);
    }

    #[test]
    fn the_fields_the_kernel_reads_are_where_it_looks_for_them() {
        // Spot-checked at the offsets most likely to be wrong: the ones after
        // a fixed-size name array, where a miscount shifts everything after it.
        let attr = ProgLoadAttr::default();
        let base = std::ptr::from_ref(&attr) as usize;
        assert_eq!(std::ptr::from_ref(&attr.prog_name) as usize - base, 48);
        assert_eq!(
            std::ptr::from_ref(&attr.expected_attach_type) as usize - base,
            68
        );
        assert_eq!(std::ptr::from_ref(&attr.attach_btf_id) as usize - base, 108);
    }

    #[test]
    fn a_name_too_long_for_the_kernel_is_cut_rather_than_refused() {
        // A map with a long name is cosmetic; refusing to load enforcement
        // over one is not.
        let name = kernel_name("a_very_long_map_name_indeed");
        assert_eq!(name.len(), 16);
        assert_eq!(name[15], 0, "the name must stay NUL-terminated");
        assert_eq!(&name[..15], b"a_very_long_map");
    }

    #[test]
    fn a_short_name_is_padded_with_zeroes_and_not_with_whatever_was_there() {
        let name = kernel_name("thalyx_policy");
        assert_eq!(&name[..13], b"thalyx_policy");
        assert!(name[13..].iter().all(|b| *b == 0));
    }
}

// ---------------------------------------------------------------------------
// The display.
//
// `vault/02-Arquitectura/La-Pantalla.md`. Everything that decides how the screen
// *looks* is in `thalyx-screen`, which is pure and testable with no display at
// all. What is here is only what needs a device: asking the kernel how this
// framebuffer is shaped, mapping it, and taking the text console out of the way.
// ---------------------------------------------------------------------------

/// `FBIOGET_VSCREENINFO`, from `include/uapi/linux/fb.h`: `0x4600`.
///
/// Spelled out for the same reason as [`BLKRRPART`]: `_IOR` is a C macro and
/// this workspace has no C.
///
/// `u64` here and `as libc::Ioctl` at the call site, which is the rule the whole
/// crate follows and which the display's five call sites broke on 2026-08-28:
/// `libc::ioctl` takes `c_ulong` against glibc and `c_int` against musl, so
/// `as libc::c_ulong` compiles on the machine that verifies Thalyx and stops
/// `make -C image` — the one build that produces a Thalyx machine, and the one
/// nothing else here exercises.
const FBIOGET_VSCREENINFO: u64 = 0x4600;
/// `FBIOGET_FSCREENINFO`: `0x4602`.
const FBIOGET_FSCREENINFO: u64 = 0x4602;

/// `KDGETMODE` and `KDSETMODE`, from `include/uapi/linux/kd.h`. Converted at the
/// call site for the reason above.
const KDGETMODE: u64 = 0x4B3B;
const KDSETMODE: u64 = 0x4B3A;
/// `KD_TEXT` is 0 and `KD_GRAPHICS` is 1.
const KD_TEXT: libc::c_long = 0;
const KD_GRAPHICS: libc::c_long = 1;

/// How this display is shaped, as the kernel describes it.
///
/// Every field is read rather than assumed. The one that costs the most when
/// guessed is `line_length`: a framebuffer commonly pads each row, and writing
/// `width * bytes` per row instead slides every row left by the padding, which
/// shears the whole picture diagonally and reads as a drawing bug rather than
/// as one ignored field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayGeometry {
    pub width: u32,
    pub height: u32,
    pub bits_per_pixel: u32,
    /// `(offset, length)` in bits, from `struct fb_bitfield`.
    pub red: (u32, u32),
    pub green: (u32, u32),
    pub blue: (u32, u32),
    /// Bytes from the start of one row to the start of the next.
    pub line_length: usize,
    /// How many bytes the mapping has.
    pub buffer_len: usize,
}

/// `struct fb_var_screeninfo` is 160 bytes on every architecture Thalyx
/// targets: it is all `__u32`, so there is no padding to differ.
const FB_VAR_SCREENINFO: usize = 160;
/// `struct fb_fix_screeninfo` is 80 bytes on 64-bit, where the two `unsigned
/// long` members are eight wide and force alignment.
const FB_FIX_SCREENINFO: usize = 80;

/// Ask the display how it is shaped.
pub fn display_geometry(framebuffer: BorrowedFd<'_>) -> io::Result<DisplayGeometry> {
    use std::os::fd::AsRawFd;

    let word = |bytes: &[u8], at: usize| -> u32 {
        u32::from_ne_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };

    let mut var = [0u8; FB_VAR_SCREENINFO];
    // SAFETY: the buffer is exactly the size the ioctl writes, it is a live
    // local, and the descriptor is borrowed for the call.
    #[allow(unsafe_code)]
    let read_var = unsafe {
        libc::ioctl(
            framebuffer.as_raw_fd(),
            FBIOGET_VSCREENINFO as libc::Ioctl,
            var.as_mut_ptr(),
        )
    };
    if read_var != 0 {
        return Err(io::Error::last_os_error());
    }

    let mut fix = [0u8; FB_FIX_SCREENINFO];
    // SAFETY: as above, with the size this second ioctl writes.
    #[allow(unsafe_code)]
    let read_fix = unsafe {
        libc::ioctl(
            framebuffer.as_raw_fd(),
            FBIOGET_FSCREENINFO as libc::Ioctl,
            fix.as_mut_ptr(),
        )
    };
    if read_fix != 0 {
        return Err(io::Error::last_os_error());
    }

    // Offsets into `fb_var_screeninfo`: xres, yres, then four more `__u32`
    // before `bits_per_pixel`, then the four `fb_bitfield`s of three `__u32`
    // each. Written as arithmetic on named steps rather than as bare numbers,
    // so that a field added upstream is a visible edit rather than a silent
    // shift.
    let bitfield = |which: usize| -> (u32, u32) {
        let base = 32 + which * 12;
        (word(&var, base), word(&var, base + 4))
    };

    // Offsets into `fb_fix_screeninfo`: `char id[16]`, then `unsigned long
    // smem_start` aligned to 8, then `__u32 smem_len`. `line_length` sits at 48
    // after three `__u16`s and their padding.
    let geometry = DisplayGeometry {
        width: word(&var, 0),
        height: word(&var, 4),
        bits_per_pixel: word(&var, 24),
        red: bitfield(0),
        green: bitfield(1),
        blue: bitfield(2),
        line_length: word(&fix, 48) as usize,
        buffer_len: word(&fix, 24) as usize,
    };

    // Rule 9: a display that answers nonsense is refused rather than mapped.
    // A zero here is not a small screen, it is a struct read at the wrong
    // offset, and mapping zero bytes then writing into it is the version of
    // this bug that takes the machine down instead of printing a sentence.
    if geometry.width == 0
        || geometry.height == 0
        || geometry.line_length == 0
        || geometry.buffer_len == 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "this display reports {}x{} with a {}-byte row and a {}-byte buffer, \
                 which is not a display",
                geometry.width, geometry.height, geometry.line_length, geometry.buffer_len
            ),
        ));
    }

    Ok(geometry)
}

impl Mapped {
    /// The mapped bytes, to write into.
    ///
    /// **Only the framebuffer uses this.** Everything else that maps something
    /// here is reading what the kernel wrote — a ring buffer, a map — and the
    /// narrow [`Mapped::write_first_u64`] exists so that those callers cannot
    /// write anywhere else by accident. A display is the one mapping whose
    /// whole purpose is to be overwritten.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: `address` and `length` come from a successful `mmap` of
        // exactly that length, and the mapping lives as long as `self` because
        // `Drop` is what unmaps it. `&mut self` is what makes this the only
        // live view of those bytes on this side; the device on the other side
        // is a display, which reads them and does not write them back.
        #[allow(unsafe_code)]
        unsafe {
            std::slice::from_raw_parts_mut(self.address.cast::<u8>(), self.length)
        }
    }
}

/// The console put into graphics mode for as long as this value lives.
///
/// ## Why this is a guard and not two calls
///
/// It is the same failure as [`RawMode`], one step worse. A console left in
/// graphics mode draws nothing at all: no shell, no kernel message, no login —
/// **a black screen on a machine that is running fine.** On the image there is
/// no second terminal to recover from, so a session that exits without
/// restoring the mode has bricked the display until the machine is power
/// cycled.
///
/// So the restore rides on `Drop`, which runs on the ordinary path and while a
/// panic unwinds. It cannot cover a `SIGKILL`, and nothing can.
///
/// ## What this buys besides the pixels
///
/// The kernel stops drawing the text console over the frame — which is also
/// what stops `printk` from landing on top of the screen. The 2026-08-07 boot
/// where a repeating USB error wrote over the prompt every few seconds cannot
/// happen here: in graphics mode those messages go to the log and not to the
/// glass. They are still readable with `nucleo`.
pub struct GraphicsMode {
    fd: std::os::fd::RawFd,
    saved: libc::c_long,
}

impl GraphicsMode {
    /// Take the console out of the way, or say why it could not be taken.
    ///
    /// The previous mode is read rather than assumed to be `KD_TEXT`, so that
    /// restoring puts back what was there. Assuming would be right today and
    /// wrong the first time something else has already claimed the console.
    pub fn enter(console: BorrowedFd<'_>) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let fd = console.as_raw_fd();

        let mut saved: libc::c_long = KD_TEXT;
        // SAFETY: `KDGETMODE` writes one `long` through the pointer, which is to
        // a live local. The descriptor is borrowed for the call.
        #[allow(unsafe_code)]
        let read = unsafe { libc::ioctl(fd, KDGETMODE as libc::Ioctl, &raw mut saved) };
        if read != 0 {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: `KDSETMODE` takes its argument by value, not by pointer.
        #[allow(unsafe_code)]
        let set = unsafe { libc::ioctl(fd, KDSETMODE as libc::Ioctl, KD_GRAPHICS) };
        if set != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { fd, saved })
    }
}

impl Drop for GraphicsMode {
    fn drop(&mut self) {
        // SAFETY: putting back the mode this guard read from the kernel, on a
        // descriptor that was valid when the guard was made and which the guard
        // does not outlive.
        #[allow(unsafe_code)]
        unsafe {
            libc::ioctl(self.fd, KDSETMODE as libc::Ioctl, self.saved);
        }
    }
}
