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
pub use libc::{MS_NODEV, MS_NOEXEC, MS_NOSUID, MS_PRIVATE, MS_RDONLY, MS_REC};

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

/// Set the hostname, which is meaningful only inside a UTS namespace.
pub fn sethostname(name: &str) -> io::Result<()> {
    let bytes = name.as_bytes();
    // SAFETY: the pointer and length describe a slice that outlives the call,
    // and the kernel only reads `len` bytes from it.
    #[allow(unsafe_code)]
    let result = unsafe { libc::sethostname(bytes.as_ptr().cast(), bytes.len()) };
    check(result)
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
