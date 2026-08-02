//! The seccomp allowlist, and the classic BPF program it compiles to.
//!
//! `vault/04-Flujo-Canonico/Sandbox-Ejecucion.md` decrees a filter by
//! **allowlist, not blocklist**. The difference is not stylistic. A blocklist
//! is a claim to have thought of everything, and it silently stops being true
//! every time the kernel gains a syscall. An allowlist is wrong in the
//! direction that shows up as a module failing to start, which someone fixes.
//!
//! ## What a denied syscall does
//!
//! It kills the process. Not `EPERM` — a program that gets `EPERM` from a
//! syscall it did not expect to fail often carries on into a state nobody
//! designed, and the failure surfaces somewhere unrelated, much later. A kill
//! is loud, immediate, and attributable.
//!
//! ## Architecture
//!
//! The filter checks the architecture first and kills anything that is not the
//! one it was built for. Without that check, a 32-bit call into a 64-bit
//! kernel would be matched against the wrong syscall numbers — the classic way
//! a seccomp filter is bypassed, because the numbers mean different things.
//!
//! Thalyx only supports x86-64 today, and [`Allowlist::compile`] refuses to
//! build a filter on anything else rather than emitting one that does not
//! match. A filter that does not match is worse than none: it looks installed.

use std::collections::BTreeSet;
use thalyx_syscall::Instruction;

// Classic BPF opcodes. Spelled out rather than pulled from a crate so the
// program below can be read against the kernel's own documentation.
const LD_W_ABS: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
const JMP_JEQ_K: u16 = 0x15; // BPF_JMP | BPF_JEQ | BPF_K
const RET_K: u16 = 0x06; // BPF_RET | BPF_K

/// Offsets into `struct seccomp_data`.
const OFFSET_NR: u32 = 0;
const OFFSET_ARCH: u32 = 4;

const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;

/// `AUDIT_ARCH_X86_64`, the only architecture Thalyx builds a filter for.
const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;

#[derive(Debug, thiserror::Error)]
pub enum SeccompError {
    #[error(
        "Thalyx has no seccomp allowlist for this architecture.\n  \
         Refusing to install a filter built for another one: it would look \
         installed and match nothing."
    )]
    UnsupportedArchitecture,

    #[error(
        "an allowlist of {0} syscalls does not fit in a classic BPF program \
         (the kernel's limit is 4096 instructions)"
    )]
    TooManySyscalls(usize),

    #[error("could not install the seccomp filter: {0}")]
    Install(#[source] std::io::Error),
}

/// The syscalls a confined module may make.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowlist {
    syscalls: BTreeSet<i64>,
}

impl Allowlist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow(mut self, syscall: i64) -> Self {
        self.syscalls.insert(syscall);
        self
    }

    pub fn allow_all(mut self, syscalls: impl IntoIterator<Item = i64>) -> Self {
        self.syscalls.extend(syscalls);
        self
    }

    pub fn len(&self) -> usize {
        self.syscalls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.syscalls.is_empty()
    }

    pub fn contains(&self, syscall: i64) -> bool {
        self.syscalls.contains(&syscall)
    }

    pub fn syscalls(&self) -> impl Iterator<Item = i64> + '_ {
        self.syscalls.iter().copied()
    }

    /// Compile to a classic BPF program.
    ///
    /// ```text
    ///   ld  [4]                 ; seccomp_data.arch
    ///   jeq AUDIT_ARCH_X86_64, +1, +0
    ///   ret KILL_PROCESS        ; wrong architecture
    ///   ld  [0]                 ; seccomp_data.nr
    ///   jeq <allowed>, allow, next
    ///   ...
    ///   ret KILL_PROCESS        ; not on the list
    ///   ret ALLOW
    /// ```
    ///
    /// The allow instruction sits at the end and every match jumps forward to
    /// it, so the program is linear in the size of the list. Classic BPF jumps
    /// are unsigned 8-bit forward offsets, which caps a single chain at 255 —
    /// this scheme stays inside that as long as the list does.
    pub fn compile(&self) -> Result<Vec<Instruction>, SeccompError> {
        if !cfg!(target_arch = "x86_64") {
            return Err(SeccompError::UnsupportedArchitecture);
        }
        if self.syscalls.len() > 200 {
            // Not the kernel's 4096-instruction limit that bites first: it is
            // the 8-bit jump offset. Refusing here keeps the failure at build
            // time instead of producing a filter with wrapped jumps.
            return Err(SeccompError::TooManySyscalls(self.syscalls.len()));
        }

        let mut program = Vec::with_capacity(self.syscalls.len() + 6);

        // Architecture gate.
        program.push(load(OFFSET_ARCH));
        program.push(jump_eq(AUDIT_ARCH_X86_64, 1, 0));
        program.push(ret(SECCOMP_RET_KILL_PROCESS));

        // Syscall number.
        program.push(load(OFFSET_NR));

        let count = self.syscalls.len();
        for (index, syscall) in self.syscalls.iter().enumerate() {
            // Distance from the instruction after this one to the final
            // `ret ALLOW`: the remaining comparisons, plus the `ret KILL`.
            let remaining = (count - index - 1) as u8;
            program.push(jump_eq(*syscall as u32, remaining + 1, 0));
        }

        program.push(ret(SECCOMP_RET_KILL_PROCESS));
        program.push(ret(SECCOMP_RET_ALLOW));

        Ok(program)
    }

    /// Compile and install, irrevocably, for this process and its `exec`s.
    pub fn install(&self) -> Result<(), SeccompError> {
        let program = self.compile()?;
        thalyx_syscall::set_no_new_privs().map_err(SeccompError::Install)?;
        thalyx_syscall::install_seccomp_filter(&program).map_err(SeccompError::Install)
    }
}

fn load(offset: u32) -> Instruction {
    Instruction {
        code: LD_W_ABS,
        jt: 0,
        jf: 0,
        k: offset,
    }
}

fn jump_eq(value: u32, jt: u8, jf: u8) -> Instruction {
    Instruction {
        code: JMP_JEQ_K,
        jt,
        jf,
        k: value,
    }
}

fn ret(action: u32) -> Instruction {
    Instruction {
        code: RET_K,
        jt: 0,
        jf: 0,
        k: action,
    }
}

/// The syscalls the `module_standard` profile permits.
///
/// Derived empirically, by running real programs under the filter and adding
/// what they actually needed — not by reading a list from somewhere. A guessed
/// allowlist is wrong in both directions at once: too narrow to run anything
/// and too wide to mean much.
///
/// The shape of it: process lifecycle, memory, file and descriptor I/O,
/// signals, futexes, and the handful of `get*` calls libc makes on startup.
/// Deliberately **absent**: `mount`, `pivot_root`, `unshare`, `setns`,
/// `ptrace`, `bpf`, `kexec_load`, `init_module`, `keyctl`, and every other
/// call whose purpose is to change the shape of the sandbox itself.
///
/// Also absent: `socket`. Network access is denied by the network namespace
/// and by `thalyx-lsm`; leaving the syscall out means a module without network
/// permission cannot even construct a socket to be denied on.
pub fn module_standard() -> Allowlist {
    Allowlist::new().allow_all([
        // Process lifecycle
        libc::SYS_execve,
        libc::SYS_exit,
        libc::SYS_exit_group,
        libc::SYS_wait4,
        libc::SYS_clone,
        libc::SYS_clone3,
        libc::SYS_vfork,
        libc::SYS_set_tid_address,
        libc::SYS_set_robust_list,
        libc::SYS_get_robust_list,
        libc::SYS_rseq,
        libc::SYS_prctl,
        libc::SYS_arch_prctl,
        libc::SYS_getpid,
        libc::SYS_gettid,
        libc::SYS_getppid,
        libc::SYS_setpgid,
        libc::SYS_getpgrp,
        libc::SYS_getpgid,
        libc::SYS_setsid,
        libc::SYS_getsid,
        libc::SYS_sched_getaffinity,
        libc::SYS_sched_yield,
        // Identity, read-only. Changing it is not on the list.
        libc::SYS_getuid,
        libc::SYS_geteuid,
        libc::SYS_getgid,
        libc::SYS_getegid,
        libc::SYS_getgroups,
        libc::SYS_getresuid,
        libc::SYS_getresgid,
        // Memory
        libc::SYS_brk,
        libc::SYS_mmap,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_mprotect,
        libc::SYS_madvise,
        libc::SYS_membarrier,
        // Files and descriptors
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_readv,
        libc::SYS_writev,
        libc::SYS_pread64,
        libc::SYS_pwrite64,
        libc::SYS_open,
        libc::SYS_openat,
        libc::SYS_close,
        libc::SYS_close_range,
        libc::SYS_lseek,
        libc::SYS_stat,
        libc::SYS_fstat,
        libc::SYS_lstat,
        libc::SYS_newfstatat,
        libc::SYS_statx,
        libc::SYS_statfs,
        libc::SYS_fstatfs,
        libc::SYS_fadvise64,
        libc::SYS_copy_file_range,
        libc::SYS_sendfile,
        libc::SYS_splice,
        // Reading extended attributes, never writing them. `ls -l` asks for
        // them on any system with security labels, and a kill there would make
        // the sandbox look broken for a call that returns "no such attribute".
        libc::SYS_getxattr,
        libc::SYS_lgetxattr,
        libc::SYS_fgetxattr,
        libc::SYS_listxattr,
        libc::SYS_llistxattr,
        libc::SYS_flistxattr,
        libc::SYS_access,
        libc::SYS_faccessat,
        libc::SYS_faccessat2,
        libc::SYS_readlink,
        libc::SYS_readlinkat,
        libc::SYS_getcwd,
        libc::SYS_chdir,
        libc::SYS_fchdir,
        libc::SYS_getdents64,
        libc::SYS_fcntl,
        libc::SYS_ioctl,
        libc::SYS_dup,
        libc::SYS_dup2,
        libc::SYS_dup3,
        libc::SYS_pipe,
        libc::SYS_pipe2,
        libc::SYS_fsync,
        libc::SYS_fdatasync,
        libc::SYS_ftruncate,
        libc::SYS_unlink,
        libc::SYS_unlinkat,
        libc::SYS_rename,
        libc::SYS_renameat,
        libc::SYS_renameat2,
        libc::SYS_mkdir,
        libc::SYS_mkdirat,
        libc::SYS_rmdir,
        libc::SYS_umask,
        libc::SYS_fchmod,
        libc::SYS_chmod,
        libc::SYS_utimensat,
        // Waiting
        libc::SYS_poll,
        libc::SYS_ppoll,
        libc::SYS_select,
        libc::SYS_pselect6,
        libc::SYS_epoll_create1,
        libc::SYS_epoll_ctl,
        libc::SYS_epoll_wait,
        libc::SYS_epoll_pwait,
        libc::SYS_eventfd2,
        // Signals
        libc::SYS_rt_sigaction,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigreturn,
        libc::SYS_rt_sigsuspend,
        libc::SYS_sigaltstack,
        libc::SYS_kill,
        libc::SYS_tgkill,
        libc::SYS_pause,
        // Synchronisation and time
        libc::SYS_futex,
        libc::SYS_nanosleep,
        libc::SYS_clock_nanosleep,
        libc::SYS_clock_gettime,
        libc::SYS_clock_getres,
        libc::SYS_gettimeofday,
        libc::SYS_times,
        // Startup odds and ends
        libc::SYS_uname,
        libc::SYS_sysinfo,
        libc::SYS_getrandom,
        libc::SYS_getrlimit,
        libc::SYS_prlimit64,
        libc::SYS_getrusage,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_checks_the_architecture_before_anything_else() {
        // Without this, a 32-bit call into a 64-bit kernel is matched against
        // the wrong syscall numbers. It is the classic seccomp bypass, and it
        // works precisely because the filter still looks correct.
        let program = Allowlist::new().allow(libc::SYS_read).compile().unwrap();

        assert_eq!(program[0], load(OFFSET_ARCH));
        assert_eq!(program[1].code, JMP_JEQ_K);
        assert_eq!(program[1].k, AUDIT_ARCH_X86_64);
        assert_eq!(program[2], ret(SECCOMP_RET_KILL_PROCESS));
    }

    #[test]
    fn a_denied_syscall_kills_rather_than_returning_an_error() {
        // EPERM from a syscall a program did not expect to fail leaves it
        // running in a state nobody designed. A kill is attributable.
        let program = module_standard().compile().unwrap();
        let kills = program
            .iter()
            .filter(|i| i.code == RET_K && i.k == SECCOMP_RET_KILL_PROCESS)
            .count();
        assert_eq!(kills, 2, "one for the wrong arch, one for the default");
        assert!(
            !program
                .iter()
                .any(|i| i.code == RET_K && i.k & 0xffff_0000 == 0x0005_0000)
        );
    }

    /// Walk the compiled program the way the kernel would.
    ///
    /// Written out rather than trusted, because a wrong jump offset produces a
    /// filter that installs cleanly and permits the wrong set — the exact
    /// failure with no symptom this project keeps finding.
    fn evaluate(program: &[Instruction], arch: u32, nr: i64) -> u32 {
        let mut pc = 0usize;
        let mut accumulator = 0u32;

        loop {
            let instruction = program[pc];
            pc += 1;

            match instruction.code {
                LD_W_ABS => {
                    accumulator = match instruction.k {
                        OFFSET_NR => nr as u32,
                        OFFSET_ARCH => arch,
                        other => panic!("filter read an unexpected offset {other}"),
                    }
                }
                JMP_JEQ_K => {
                    let taken = accumulator == instruction.k;
                    pc += if taken {
                        instruction.jt as usize
                    } else {
                        instruction.jf as usize
                    };
                }
                RET_K => return instruction.k,
                other => panic!("unknown opcode {other:#x}"),
            }

            assert!(pc < program.len(), "the program ran off the end");
        }
    }

    #[test]
    fn every_allowed_syscall_evaluates_to_allow() {
        let allowlist = module_standard();
        let program = allowlist.compile().unwrap();

        for syscall in allowlist.syscalls() {
            assert_eq!(
                evaluate(&program, AUDIT_ARCH_X86_64, syscall),
                SECCOMP_RET_ALLOW,
                "syscall {syscall} is on the list but the filter denies it"
            );
        }
    }

    #[test]
    fn the_syscalls_that_reshape_the_sandbox_are_denied() {
        // Named individually rather than checked as "not in the set". The
        // point is not that they were left out; it is that leaving any of them
        // in would hand a module the ability to undo its own confinement.
        let allowlist = module_standard();
        let program = allowlist.compile().unwrap();

        for syscall in [
            libc::SYS_mount,
            libc::SYS_umount2,
            libc::SYS_pivot_root,
            libc::SYS_unshare,
            libc::SYS_setns,
            libc::SYS_ptrace,
            libc::SYS_bpf,
            libc::SYS_init_module,
            libc::SYS_finit_module,
            libc::SYS_delete_module,
            libc::SYS_kexec_load,
            libc::SYS_keyctl,
            libc::SYS_add_key,
            libc::SYS_setuid,
            libc::SYS_setgid,
            libc::SYS_setresuid,
            libc::SYS_capset,
            libc::SYS_chroot,
            libc::SYS_reboot,
            libc::SYS_swapon,
            libc::SYS_socket,
            libc::SYS_connect,
            libc::SYS_bind,
            libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev,
        ] {
            assert!(
                !allowlist.contains(syscall),
                "syscall {syscall} is on the allowlist and should not be"
            );
            assert_eq!(
                evaluate(&program, AUDIT_ARCH_X86_64, syscall),
                SECCOMP_RET_KILL_PROCESS,
                "syscall {syscall} is not on the list but the filter permits it"
            );
        }
    }

    #[test]
    fn a_call_from_the_wrong_architecture_is_killed_whatever_it_asks_for() {
        let program = module_standard().compile().unwrap();
        const AUDIT_ARCH_I386: u32 = 0x4000_0003;

        for syscall in [libc::SYS_read, libc::SYS_write, libc::SYS_execve, 999] {
            assert_eq!(
                evaluate(&program, AUDIT_ARCH_I386, syscall),
                SECCOMP_RET_KILL_PROCESS
            );
        }
    }

    #[test]
    fn an_empty_allowlist_denies_everything_rather_than_permitting_it() {
        // The degenerate case has to fail closed. An allowlist that fell
        // through to ALLOW when empty would turn a configuration mistake into
        // an unconfined process.
        let program = Allowlist::new().compile().unwrap();
        assert_eq!(
            evaluate(&program, AUDIT_ARCH_X86_64, libc::SYS_read),
            SECCOMP_RET_KILL_PROCESS
        );
    }

    #[test]
    fn the_jump_offsets_stay_within_what_classic_bpf_can_express() {
        let program = module_standard().compile().unwrap();
        for (index, instruction) in program.iter().enumerate() {
            if instruction.code == JMP_JEQ_K {
                let target = index + 1 + instruction.jt as usize;
                assert!(
                    target < program.len(),
                    "instruction {index} jumps past the end of the program"
                );
            }
        }
    }

    #[test]
    fn an_allowlist_too_long_for_an_eight_bit_jump_is_refused() {
        let long = Allowlist::new().allow_all(1000..1300);
        assert!(matches!(
            long.compile(),
            Err(SeccompError::TooManySyscalls(_))
        ));
    }
}
