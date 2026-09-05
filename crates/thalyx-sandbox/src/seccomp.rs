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

use std::collections::{BTreeMap, BTreeSet};
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

/// Offset of `seccomp_data.args[0]`, and the stride between arguments.
///
/// Each argument is the whole 64-bit register the syscall was made with. On a
/// little-endian machine that is two words: the low half at `OFFSET_ARGS + 8n`,
/// the high half four bytes after it. Classic BPF loads 32 bits at a time, so
/// **an argument check that reads only the low half is not a check** — the
/// caller chooses the top 32 bits too, and a filter that never looks at them
/// compares the half it was shown. Every guard below reads both.
const OFFSET_ARGS: u32 = 16;
const ARG_STRIDE: u32 = 8;

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

    #[error(
        "syscall {0} is both allowed outright and guarded.\n  \
         The plain allowance answers first and ignores the arguments, so the \
         guard would never run and would look installed."
    )]
    GuardedAndAllowed(i64),

    #[error(
        "internal: a jump from instruction {from} to {to} goes backwards, and \
         classic BPF has only forward offsets"
    )]
    BackwardJump { from: usize, to: usize },
}

/// `SCHED_OTHER`, the ordinary time-sharing policy every thread starts on.
const SCHED_OTHER: u32 = 0;
/// `SCHED_BATCH`: the same share, with the scheduler told this thread is not
/// interactive and may be woken less eagerly.
const SCHED_BATCH: u32 = 3;
/// `SCHED_IDLE`: run only when nothing else wants the processor.
const SCHED_IDLE: u32 = 5;

/// Not a policy — a flag OR'd into one, saying children do not inherit it.
///
/// The first version of this guard permitted the three policies above and
/// nothing else, which is what the manual page suggests and what anyone would
/// write. Then the trace was read instead of imagined: Node asks for
/// `0x40000000`, which is `SCHED_OTHER | SCHED_RESET_ON_FORK`, on every one of
/// its threads. A guard built from the policy list alone would have killed a
/// foreign agent on the exact call the guard exists to let through — and it
/// would have looked like the guard working.
///
/// `vault/09-Notas-Tecnicas/Que-Necesita-Un-Agente-Ajeno.md` carries the
/// capture. `Estrategia-de-Pruebas.md` rule 6 is the general form of it: a
/// value you invented is a test of your model of the format.
const SCHED_RESET_ON_FORK: u32 = 0x4000_0000;

/// A syscall allowed only when one of its arguments is a value on a list.
///
/// The same allowlist shape as the syscall table itself, one level down, and
/// for the same reason: a guard written as "everything except the dangerous
/// values" is a claim to have thought of every value the kernel will ever
/// define for that argument.
///
/// It exists because `sched_setscheduler` is two different requests wearing one
/// name. A program arranging its own threads inside the share of processor the
/// cgroup already gave it is ordinary, and the runtime of every foreign agent
/// measured so far does it before it does anything else. A program asking for a
/// **real-time** policy is asking to hold a processor against everything else on
/// the machine, Thalyx included, and no cgroup limit takes that back. Denying
/// the call outright pays the first to stop the second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guard {
    /// The syscall this guards.
    pub syscall: i64,
    /// Which of `args[0..6]` decides. The rest are not looked at.
    pub argument: u8,
    /// The values that argument may hold. Anything else is killed — including
    /// a value that differs only in the top 32 bits of the register.
    pub permitted: BTreeSet<u32>,
}

impl Guard {
    /// `sched_setscheduler`, for every policy that is not a real-time one.
    ///
    /// Absent, deliberately and by name: `SCHED_FIFO` (1), `SCHED_RR` (2) and
    /// `SCHED_DEADLINE` (6). Those are the three that can starve the rest of
    /// the machine, and they are the whole reason this is a guard rather than
    /// a line in the allowlist.
    pub fn scheduling_without_real_time() -> Self {
        // `policy` is the second argument: sched_setscheduler(pid, policy, param).
        //
        // Each ordinary policy appears twice, once alone and once with the
        // reset-on-fork flag. Enumerating the six rather than masking the flag
        // off keeps the permitted set literally visible in the compiled filter,
        // and the flag cannot turn any of these into a real-time policy: it is
        // bit 30 and the policies are small integers.
        let mut permitted = BTreeSet::new();
        for policy in [SCHED_OTHER, SCHED_BATCH, SCHED_IDLE] {
            permitted.insert(policy);
            permitted.insert(policy | SCHED_RESET_ON_FORK);
        }

        Guard {
            syscall: libc::SYS_sched_setscheduler,
            argument: 1,
            permitted,
        }
    }

    /// `socketpair`, for the one domain that cannot reach off this machine.
    ///
    /// A `socketpair` is two connected descriptors and nothing else — there is
    /// no address, no route, nobody to accept it. But the syscall takes a
    /// domain, and a filter that allowed it outright would be allowing whatever
    /// domain a future kernel decides to implement `socketpair` for. `AF_UNIX`
    /// is the one this needs and the one that is provably local, so it is the
    /// only one on the list, for the same reason the scheduling guard names
    /// three policies instead of excluding three.
    ///
    /// It is **not** in `module_standard`, and the asymmetry with `socket` is
    /// the point: `socket` stays denied there and here, so nothing under either
    /// filter can construct a descriptor that reaches the network. This one
    /// exists because a Rust program that spawns a child process makes it —
    /// `std::process` reports the child's `execve` failure back over a
    /// `SOCK_SEQPACKET` pair — and a compiler tree is nothing but spawned
    /// children.
    pub fn local_socket_pairs() -> Self {
        // `socketpair(domain, type, protocol, sv)`: the domain is the first.
        Guard {
            syscall: libc::SYS_socketpair,
            argument: 0,
            permitted: BTreeSet::from([libc::AF_UNIX as u32]),
        }
    }
}

/// The syscalls a confined module may make.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowlist {
    syscalls: BTreeSet<i64>,
    /// Keyed by syscall so the same one cannot be guarded twice with two
    /// different answers.
    guards: BTreeMap<i64, Guard>,
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

    /// Allow a syscall only for some of its arguments.
    ///
    /// Kept apart from [`Allowlist::allow`] rather than folded into it, because
    /// "this is allowed" and "this is allowed under a condition" are different
    /// facts and a caller reading the list is entitled to see which one it has.
    /// [`Allowlist::contains`] answers only the first, so a guarded syscall
    /// reads as absent there — which is the cautious way round.
    pub fn guard(mut self, guard: Guard) -> Self {
        self.guards.insert(guard.syscall, guard);
        self
    }

    /// Every guarded syscall, in syscall order.
    pub fn guards(&self) -> impl Iterator<Item = &Guard> + '_ {
        self.guards.values()
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
    ///
    ///   ; one block per guarded syscall, before the plain chain
    ///   jeq <guarded>, next, past-this-block
    ///   ld  [args[n] high]
    ///   jeq 0, next, kill       ; the top half of the register must be empty
    ///   ld  [args[n] low]
    ///   jeq <permitted>, allow, next
    ///   ...
    ///   ret KILL_PROCESS        ; the argument was not on its list
    ///
    ///   jeq <allowed>, allow, next
    ///   ...
    ///   ret KILL_PROCESS        ; not on the list
    ///   ret ALLOW
    /// ```
    ///
    /// The guard blocks come **first**, and that ordering is not cosmetic: the
    /// plain chain jumps straight to `ret ALLOW` on a match and never looks at
    /// an argument, so a syscall reachable there is unconditionally allowed
    /// whatever a later block says. [`SeccompError::GuardedAndAllowed`] refuses
    /// the list that would make that happen rather than compiling it.
    ///
    /// The `ret ALLOW` sits at the end and every match jumps forward to it, so
    /// the program is linear in the size of the list. Classic BPF jumps are
    /// unsigned 8-bit forward offsets, which caps a single chain at 255 — the
    /// distances are computed and checked here rather than assumed, because a
    /// wrapped jump produces a filter that installs cleanly and permits the
    /// wrong set.
    pub fn compile(&self) -> Result<Vec<Instruction>, SeccompError> {
        if !cfg!(target_arch = "x86_64") {
            return Err(SeccompError::UnsupportedArchitecture);
        }
        if let Some(both) = self.guards.keys().find(|s| self.syscalls.contains(s)) {
            return Err(SeccompError::GuardedAndAllowed(*both));
        }

        // The architecture gate keeps a kill of its own, so the first thing in
        // the program reads as one check rather than as part of the chain.
        let mut slots: Vec<Slot> = vec![
            Slot::Fixed(load(OFFSET_ARCH)),
            Slot::Fixed(jump_eq(AUDIT_ARCH_X86_64, 1, 0)),
            Slot::Fixed(ret(SECCOMP_RET_KILL_PROCESS)),
            Slot::Fixed(load(OFFSET_NR)),
        ];

        for guard in self.guards.values() {
            let start = slots.len();
            // `jeq`, the two loads and their comparison, one per permitted
            // value, and the block's own kill.
            let length = 4 + guard.permitted.len() + 1;
            let argument = OFFSET_ARGS + ARG_STRIDE * u32::from(guard.argument);

            slots.push(Slot::Jump {
                value: guard.syscall as u32,
                on_match: Go::Next,
                on_miss: Go::At(start + length),
            });
            slots.push(Slot::Fixed(load(argument + 4)));
            slots.push(Slot::Jump {
                value: 0,
                on_match: Go::Next,
                on_miss: Go::Kill,
            });
            slots.push(Slot::Fixed(load(argument)));
            for value in &guard.permitted {
                slots.push(Slot::Jump {
                    value: *value,
                    on_match: Go::Allow,
                    on_miss: Go::Next,
                });
            }
            slots.push(Slot::Fixed(ret(SECCOMP_RET_KILL_PROCESS)));

            debug_assert_eq!(slots.len(), start + length, "the block length is wrong");
        }

        for syscall in &self.syscalls {
            slots.push(Slot::Jump {
                value: *syscall as u32,
                on_match: Go::Allow,
                on_miss: Go::Next,
            });
        }

        let kill = slots.len();
        slots.push(Slot::Fixed(ret(SECCOMP_RET_KILL_PROCESS)));
        let allow = slots.len();
        slots.push(Slot::Fixed(ret(SECCOMP_RET_ALLOW)));

        self.resolve(&slots, kill, allow)
    }

    /// Turn the slots into instructions, computing every jump distance.
    fn resolve(
        &self,
        slots: &[Slot],
        kill: usize,
        allow: usize,
    ) -> Result<Vec<Instruction>, SeccompError> {
        let mut program = Vec::with_capacity(slots.len());

        for (index, slot) in slots.iter().enumerate() {
            match slot {
                Slot::Fixed(instruction) => program.push(*instruction),
                Slot::Jump {
                    value,
                    on_match,
                    on_miss,
                } => {
                    let distance = |go: Go| -> Result<u8, SeccompError> {
                        let to = match go {
                            Go::Next => index + 1,
                            Go::Allow => allow,
                            Go::Kill => kill,
                            Go::At(at) => at,
                        };
                        let steps = to
                            .checked_sub(index + 1)
                            .ok_or(SeccompError::BackwardJump { from: index, to })?;
                        u8::try_from(steps).map_err(|_| {
                            SeccompError::TooManySyscalls(self.syscalls.len() + self.guards.len())
                        })
                    };

                    program.push(jump_eq(*value, distance(*on_match)?, distance(*on_miss)?));
                }
            }
        }

        Ok(program)
    }

    /// Compile and install, irrevocably, for this process and its `exec`s.
    pub fn install(&self) -> Result<(), SeccompError> {
        let program = self.compile()?;
        thalyx_syscall::set_no_new_privs().map_err(SeccompError::Install)?;
        thalyx_syscall::install_seccomp_filter(&program).map_err(SeccompError::Install)
    }
}

/// One position in the program, before jump distances are known.
///
/// The distances used to be arithmetic done inline — "the remaining
/// comparisons, plus the kill" — which is correct for exactly one program
/// shape. A guard block is a second shape, and the third would have been
/// written by somebody counting instructions in their head.
enum Slot {
    Fixed(Instruction),
    /// `jeq value`, with where each outcome goes.
    Jump {
        value: u32,
        on_match: Go,
        on_miss: Go,
    },
}

/// Where an outcome of a comparison goes.
#[derive(Debug, Clone, Copy)]
enum Go {
    /// The instruction after this one.
    Next,
    /// The single `ret ALLOW`, which is the last instruction.
    Allow,
    /// The `ret KILL_PROCESS` before it, which is the default answer.
    Kill,
    /// A position in the program: how a guard block is stepped over.
    At(usize),
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
/// Also absent: `socket`, `connect` and `bind`. Network access is denied by the
/// network namespace and by `thalyx-lsm`; leaving the syscalls out means a
/// module without network permission cannot even construct a socket to be
/// denied on.
///
/// **Guarded rather than absent**: `sched_setscheduler`. See [`Guard`] for why
/// it is neither simply allowed nor simply denied, and
/// `vault/09-Notas-Tecnicas/Que-Necesita-Un-Agente-Ajeno.md` for the measurement
/// that put it here — it is the one call of the forty-one a foreign agent makes
/// to start that this list did not already have.
///
/// **Absent, and it is the same capability**: `sched_setattr`. It sets a policy
/// too, and a guard cannot be written for it: the policy lives in a struct
/// behind a pointer, and a seccomp filter compares registers and cannot follow
/// one. Allowing it would be a second door onto `SCHED_FIFO` with nothing
/// watching it, so it stays shut — and that costs something real, named here
/// because a cost nobody wrote down gets rediscovered as a bug: util-linux 2.41
/// sets an *ordinary* policy through it, so `chrt --other` cannot run under this
/// filter on a machine that new. `Sandbox-Ejecucion.md` carries the decision.
///
/// A module that **was** granted `net/outbound` gets them added, by
/// [`for_permissions`]. That is not a weakening of this list — it is what
/// makes the grant mean anything. See the note there.
///
/// `recvfrom` and `sendto` **are** present, and the distinction is the whole
/// point. They act on a descriptor the module already holds and cannot create
/// one, so with `socket` absent the only socket a module can ever have is the
/// channel Thalyx handed it — which is the one thing it must be able to use.
///
/// They were missing at first, and the way that showed up is worth recording:
/// every module up to then had been a shell script, and `/bin/sh` never touched
/// a socket. The first module written against the internal API died on `SIGSYS`
/// at its first answer, because a Rust `UnixStream` reads with `recv(2)` rather
/// than `read(2)` — which is invisible from the source and obvious in a trace.
pub fn module_standard() -> Allowlist {
    Allowlist::new()
        .guard(Guard::scheduling_without_real_time())
        .allow_all([
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
            // The two questions asked *before* the guarded call, and the reason
            // they are here is that leaving them out killed the program on its
            // way to a call the guard permits. `chrt --idle 0 true` — which is
            // how `dev/verify.sh` asks a confined module whether it may arrange
            // its own threads — reads the legal priority range first:
            //
            //     sched_get_priority_min(SCHED_IDLE)     = 0
            //     sched_get_priority_max(SCHED_IDLE)     = 0
            //     sched_setscheduler(0, SCHED_IDLE, [0]) = 0
            //
            // With these absent it died with SIGSYS on the first line, and the
            // *real-time* column of that same check read as the guard working
            // — a program that dies before reaching a call is indistinguishable
            // from one the guard stopped. Both answer a policy number with a
            // constant and change nothing, so there is nothing here to guard.
            libc::SYS_sched_get_priority_min,
            libc::SYS_sched_get_priority_max,
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
            // The channel to Thalyx. A Unix socket is read with `recv` and written
            // with `send`, not with `read` and `write`, so without these two a
            // module cannot answer or be answered — see the note above the list.
            libc::SYS_recvfrom,
            libc::SYS_sendto,
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
            // **Issued by the kernel, not by the program.** When a signal
            // interrupts a blocking call the kernel rewrites the register to
            // `restart_syscall` and re-enters — and seccomp filters that
            // re-entry like any other call. So a filter without it kills any
            // module that happens to be inside `futex`, `poll` or `wait4` when
            // a signal arrives, at a moment nothing in the module chose. That
            // is how `cargo` died on 2026-08-30: a `SIGCHLD` from `rustc`
            // landed on a thread waiting in `futex`, and the resume was denied.
            //
            // It permits nothing new. The only call it can resume is one that
            // was already entered, and a call is only entered if this filter
            // let it through.
            libc::SYS_restart_syscall,
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

/// The syscalls a semantic provider needs that an ordinary module does not.
///
/// `vault/03-Primitivas/Semantica-Compilada.md`, and measured rather than
/// guessed — `dev/foreign-agent-needs.sh` traced a real `cargo check` against
/// `module_standard` and named exactly two calls it makes that the filter does
/// not permit. Both were found by running the thing, which is rule 1: before
/// this, `cargo` under the module filter died on `SIGSYS` and `verify.sh`
/// reported it as `cargo exited 159` — a number that says a program was killed
/// and not which call killed it.
///
/// - **`flock`.** Cargo takes an advisory lock on `Cargo.lock`, on
///   `$CARGO_HOME/.package-cache` and on the build directory, twenty times in
///   the smallest possible build. It is the *first* of the two it reaches, so
///   it is the one that was killing the process. A lock on a descriptor the
///   process already holds reaches nothing it could not already read or write:
///   the grant is what decided that, and this call cannot widen it.
/// - **`linkat`.** `rustc` puts its output in place by hard-linking it —
///   `target/debug/deps/lib….rmeta` gets a second name rather than a second
///   copy — and this is the call that was killing `rustc` once Cargo could get
///   far enough to start one. A hard link can only be made inside a directory
///   the process can already write, so it reaches nothing the grant did not
///   already hand over.
/// - **`inotify_init1`, `inotify_add_watch`, `inotify_rm_watch`.**
///   rust-analyzer starts a `VfsLoader` thread the moment it answers
///   `initialize`, and that thread's first act is to open an inotify
///   descriptor. It is a *thread* of the server, and the filter's action is
///   `KILL_PROCESS` — so the whole language server died, and what the surface
///   above it reported was `` `initialize`: the server stopped``, a sentence
///   about a protocol rather than about a syscall. An inotify watch can only
///   be placed on a path the process can already open, so the rootfs grant is
///   still what bounds it; `inotify_init` — the legacy call with no flags,
///   which nothing modern makes — is deliberately not here.
/// - **`socketpair`, guarded to `AF_UNIX`.** Rust's `std::process` hands the
///   child's `execve` errno — and its pidfd — back to the parent over a
///   `SOCK_SEQPACKET` pair, on the fork path it takes whenever `posix_spawn`
///   cannot serve the request. Cargo spawns `rustc` that way, and
///   rust-analyzer spawns Cargo. See [`Guard::local_socket_pairs`] for why it
///   is a guard and not a line.
/// - **`fork`.** The other half of that same fork path, and it was missed for
///   exactly the reason rule 12 exists. `socketpair` was added by watching the
///   pair get made; what makes the child is the next call, and **glibc's
///   `fork()` is written on top of `clone(2)`** — so every measurement taken in
///   a container, and every test compiled by `dev/verify.sh`, reached `clone`
///   and passed. **musl's `fork()` issues `SYS_fork` itself**, and the
///   toolchain inside Thalyx is `x86_64-unknown-linux-musl`. So the call the
///   filter had never been asked about was the one the shipping binary makes:
///   rust-analyzer, a few seconds after it loads a workspace, spawns the
///   `cargo` that runs build scripts and the proc-macro server, and Rust takes
///   the fork path for it (a `pre_exec` closure, a process group, a pidfd — any
///   one of them refuses `posix_spawn`). `KILL_PROCESS` is the action, the
///   spawning thread is a thread of the server, and so the whole language
///   server died with `SIGSYS` — as a caller that had already been answered
///   once saw it, `the server stopped listening: Broken pipe`.
///
///   It permits nothing a `clone` already on this list does not. `fork` takes
///   no arguments: it cannot share an address space, cannot ask for a pidfd,
///   cannot detach a namespace. It is the least capable way there is to make a
///   process, and the process it makes inherits this same filter.
///
/// Nothing else. In particular `socket`, `connect`, `bind`, `ptrace`, `mount`
/// and `bpf` are as absent here as they are from `module_standard`: a compiler
/// tree is confined *more* than an ordinary module in every dimension that
/// costs — its own root filesystem holding only the granted tree, its own pid
/// namespace, its own empty network namespace — and the only thing it gets more
/// of is two calls and two numbers.
pub fn semantic_provider() -> Allowlist {
    module_standard()
        .allow(libc::SYS_flock)
        .allow(libc::SYS_fork)
        .allow(libc::SYS_linkat)
        .allow(libc::SYS_inotify_init1)
        .allow(libc::SYS_inotify_add_watch)
        .allow(libc::SYS_inotify_rm_watch)
        .guard(Guard::local_socket_pairs())
}

/// The syscalls a module needs to *use* an outbound network grant.
///
/// Kept apart from [`module_standard`] and added only for a module the human
/// granted `net/outbound`, which is the point: an allowlist that always
/// contained these would give every module the ability to build a socket and
/// lean entirely on the LSM to refuse it. Two independent denials is the
/// arrangement `Sandbox-Ejecucion.md` asks for, and this is the half that
/// lives in the sandbox.
///
/// ## The state this closes
///
/// Before this existed, a grant of `net/outbound` did something strictly
/// perverse. `Profile::for_permissions` dropped the network namespace so the
/// module *could* reach the network, and the filter went on refusing `socket`
/// unconditionally — so the granted module could not open a connection either
/// way, and had been handed the host's network namespace for nothing. It was
/// the one combination that cost isolation and returned no capability.
///
/// Deliberately narrow. `socket` and `connect` are what an outbound
/// conversation needs; `bind` and `listen` are not here, because inbound is a
/// different permission that nothing grants yet, and `getsockopt`/`setsockopt`
/// are present only because a connecting libc asks for them before it will
/// report an error.
pub fn outbound_network() -> impl IntoIterator<Item = i64> {
    [
        libc::SYS_socket,
        libc::SYS_connect,
        libc::SYS_getsockopt,
        libc::SYS_setsockopt,
        libc::SYS_getsockname,
        libc::SYS_getpeername,
        // Name resolution reads `/etc/resolv.conf` and talks UDP, and a libc
        // resolver uses these on the socket it just made. Without them a
        // granted module can reach an address and not a name, which is a
        // distinction nobody asked for.
        libc::SYS_recvmsg,
        libc::SYS_sendmsg,
    ]
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
        let allowlist = module_standard();
        let program = allowlist.compile().unwrap();
        let kills = program
            .iter()
            .filter(|i| i.code == RET_K && i.k == SECCOMP_RET_KILL_PROCESS)
            .count();
        assert_eq!(
            kills,
            2 + allowlist.guards().count(),
            "one for the wrong arch, one for the default, and one per guard \
             block for an argument that was not on its list"
        );
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
        evaluate_call(program, arch, nr, [0; 6])
    }

    /// The same, for a call whose arguments the filter is going to look at.
    ///
    /// The arguments are `u64` and not `u32` because that is what the kernel
    /// puts in `seccomp_data`, and because a guard that only ever saw 32-bit
    /// values in its own tests would pass while ignoring the half of the
    /// register an attacker chooses.
    fn evaluate_call(program: &[Instruction], arch: u32, nr: i64, args: [u64; 6]) -> u32 {
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
                        offset if offset >= OFFSET_ARGS => {
                            let word = (offset - OFFSET_ARGS) / 4;
                            let argument = args[(word / 2) as usize];
                            if word.is_multiple_of(2) {
                                argument as u32
                            } else {
                                (argument >> 32) as u32
                            }
                        }
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
    fn a_module_may_resume_a_call_a_signal_interrupted() {
        // `restart_syscall` is issued by the kernel, not by the program: when a
        // signal interrupts a blocking call the register is rewritten and the
        // syscall re-entered, and seccomp filters the re-entry. So a filter
        // without it kills any module that is inside `futex`, `poll` or
        // `nanosleep` when a signal lands — at a moment nothing in the module
        // chose, which is the worst possible shape for a denial.
        //
        // This is asserted against the compiled program rather than by running
        // something, and that is a real limit worth naming: producing a
        // restarted syscall on purpose needs a signal handler with `SA_RESTART`,
        // and `unsafe` lives in `thalyx-syscall`. What proved it is the kernel's
        // own record of the kill — `comm="cargo" ... syscall=219` — and stage 59
        // of `dev/verify.sh` is where it stays proven by a real Cargo.
        let program = module_standard().compile().unwrap();
        assert_eq!(
            evaluate(&program, AUDIT_ARCH_X86_64, libc::SYS_restart_syscall),
            SECCOMP_RET_ALLOW
        );
    }

    #[test]
    fn a_compiler_tree_may_watch_the_tree_it_was_given_and_a_module_may_not() {
        // rust-analyzer's `VfsLoader` thread opens an inotify descriptor as
        // soon as the server answers `initialize`, and the filter's action is
        // `KILL_PROCESS` — so the whole language server went down and the
        // surface above it reported that the server had stopped. `comm="VfsLoader"
        // ... syscall=294` in the kernel's audit log is what named it.
        //
        // Both columns, for the same reason as everywhere else here: without
        // the second, a filter that permitted everything would pass this.
        let provider = semantic_provider().compile().unwrap();
        let module = module_standard().compile().unwrap();
        for syscall in [
            libc::SYS_inotify_init1,
            libc::SYS_inotify_add_watch,
            libc::SYS_inotify_rm_watch,
        ] {
            assert_eq!(
                evaluate(&provider, AUDIT_ARCH_X86_64, syscall),
                SECCOMP_RET_ALLOW,
                "a semantic provider cannot watch syscall {syscall}"
            );
            assert_eq!(
                evaluate(&module, AUDIT_ARCH_X86_64, syscall),
                SECCOMP_RET_KILL_PROCESS,
                "an ordinary module was given syscall {syscall}, which nothing asked for"
            );
        }
    }

    #[test]
    fn a_compiler_tree_may_fork_and_a_module_may_not() {
        // `fork` and `clone` are two syscalls and the filter treats them as
        // two, which is the entire defect: a compiler tree spawning a child
        // through glibc reaches `clone` and lives, and the same source
        // compiled against musl reaches `fork` and dies. Both are asserted
        // here so that a future edit cannot "simplify" one into the other.
        let provider = semantic_provider().compile().unwrap();
        let module = module_standard().compile().unwrap();

        assert_eq!(
            evaluate(&provider, AUDIT_ARCH_X86_64, libc::SYS_fork),
            SECCOMP_RET_ALLOW,
            "a semantic provider cannot fork, so its rust-analyzer dies the \
             moment it spawns the cargo that runs build scripts"
        );
        assert_eq!(
            evaluate(&module, AUDIT_ARCH_X86_64, libc::SYS_fork),
            SECCOMP_RET_KILL_PROCESS,
            "an ordinary module was given `fork`, which nothing asked for: it \
             has no compiler under it, and `clone` is what its own std uses"
        );
        // The control on the other side. If `fork` had been added by widening
        // the family, `vfork` and `clone` would have arrived with it and the
        // module column above would be the only thing left saying so.
        for syscall in [libc::SYS_clone, libc::SYS_vfork] {
            assert_eq!(
                evaluate(&module, AUDIT_ARCH_X86_64, syscall),
                SECCOMP_RET_ALLOW,
                "syscall {syscall} was already on the module list and is not \
                 what this change is about"
            );
        }
    }

    #[test]
    fn a_socket_pair_is_permitted_for_one_domain_and_no_other() {
        // The guard, read the way the kernel reads it — including the top half
        // of the register, which is the half a caller chooses and a guard that
        // only ever saw 32-bit values would never look at.
        let program = semantic_provider().compile().unwrap();
        let call = |domain: u64| {
            evaluate_call(
                &program,
                AUDIT_ARCH_X86_64,
                libc::SYS_socketpair,
                [domain, 0, 0, 0, 0, 0],
            )
        };
        assert_eq!(call(libc::AF_UNIX as u64), SECCOMP_RET_ALLOW);
        for refused in [
            libc::AF_INET as u64,
            libc::AF_INET6 as u64,
            libc::AF_NETLINK as u64,
            // The same value in the low half, with a top half the guard would
            // miss if it read only the word it was shown.
            (1u64 << 32) | libc::AF_UNIX as u64,
        ] {
            assert_eq!(
                call(refused),
                SECCOMP_RET_KILL_PROCESS,
                "socketpair was permitted for domain {refused}"
            );
        }
        // And the plain syscall stays absent on both, which is the decree the
        // guard exists to keep: no module and no provider constructs a socket.
        for allowlist in [semantic_provider(), module_standard()] {
            assert_eq!(
                evaluate(
                    &allowlist.compile().unwrap(),
                    AUDIT_ARCH_X86_64,
                    libc::SYS_socket
                ),
                SECCOMP_RET_KILL_PROCESS
            );
        }
    }

    #[test]
    fn the_provider_adds_to_the_module_filter_and_takes_nothing_away() {
        // The relationship the whole arrangement rests on. If the provider's
        // list were ever written out by hand instead of built from the
        // module's, a syscall dropped from it would look like a tighter filter
        // and be a different one.
        let module = module_standard();
        let provider = semantic_provider();
        for syscall in module.syscalls() {
            assert!(
                provider.contains(syscall),
                "syscall {syscall} is in module_standard and not in semantic_provider"
            );
        }
        // Six plain allowances, and the socket pair is not among them: a
        // guarded syscall is not in `len()` and reads as absent from
        // `contains`, which is the cautious way round and the reason it gets
        // its own assertion rather than a number.
        assert_eq!(provider.len(), module.len() + 6);
        assert_eq!(provider.guards().count(), module.guards().count() + 1);
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

    /// The three claims a guard has to make at once, which is why they are one
    /// test: what it lets through, what it stops, and that it is looking at the
    /// argument rather than at nothing.
    ///
    /// Rule 4 of `Estrategia-de-Pruebas.md`. Without the first row, a guard
    /// that killed every call would pass the denial rows. Without the denial
    /// rows, a guard that allowed everything would pass the first.
    #[test]
    fn a_thread_may_arrange_itself_and_may_not_take_the_machine() {
        let program = module_standard().compile().unwrap();

        let asking = |policy: u64| {
            let mut args = [0u64; 6];
            args[1] = policy;
            evaluate_call(
                &program,
                AUDIT_ARCH_X86_64,
                libc::SYS_sched_setscheduler,
                args,
            )
        };

        // SCHED_OTHER, SCHED_BATCH, SCHED_IDLE, each alone and with the
        // reset-on-fork flag. The fourth of these is the value a real foreign
        // agent was traced asking for; the rest are the same claim.
        for policy in [0, 3, 5, 0x4000_0000, 0x4000_0003, 0x4000_0005] {
            assert_eq!(
                asking(policy),
                SECCOMP_RET_ALLOW,
                "policy {policy:#x} is ordinary and the filter killed it"
            );
        }

        // SCHED_FIFO, SCHED_RR, SCHED_DEADLINE — and the same three with the
        // flag, which does not launder them.
        for policy in [1, 2, 6, 0x4000_0001, 0x4000_0002, 0x4000_0006] {
            assert_eq!(
                asking(policy),
                SECCOMP_RET_KILL_PROCESS,
                "policy {policy:#x} can hold a processor against the machine \
                 and the filter allowed it"
            );
        }

        // And a policy nobody has defined yet. Rule 9: the value written by a
        // version that does not exist gets the cautious answer.
        assert_eq!(asking(99), SECCOMP_RET_KILL_PROCESS);
    }

    #[test]
    fn a_policy_hidden_in_the_top_half_of_the_register_does_not_get_through() {
        // The classic way an argument check is bypassed, and it is invisible
        // from the source: `policy` is an `int`, the filter reads a 64-bit
        // register, and a filter that compares only the low word says yes to
        // `0xffffffff_00000000` — which the kernel reads as SCHED_OTHER only if
        // it truncates the same way the filter guessed it would.
        //
        // There is no need to know which way the kernel truncates. Refusing
        // anything with bits up there is the answer that is right either way.
        let program = module_standard().compile().unwrap();
        let mut args = [0u64; 6];
        args[1] = 0x0000_0001_0000_0000;

        assert_eq!(
            evaluate_call(
                &program,
                AUDIT_ARCH_X86_64,
                libc::SYS_sched_setscheduler,
                args
            ),
            SECCOMP_RET_KILL_PROCESS
        );
    }

    #[test]
    fn a_guarded_syscall_reads_as_absent_from_the_plain_list() {
        // `contains` answers "is this allowed outright", and a guarded syscall
        // is not. A caller that got `true` here would conclude the arguments do
        // not matter, which is the opposite of what the guard says.
        let allowlist = module_standard();
        assert!(!allowlist.contains(libc::SYS_sched_setscheduler));
        assert!(
            allowlist
                .guards()
                .any(|g| g.syscall == libc::SYS_sched_setscheduler)
        );
    }

    #[test]
    fn a_syscall_that_is_both_guarded_and_allowed_is_refused_rather_than_compiled() {
        // The one mistake that would undo a guard without changing how the
        // filter looks: the plain chain matches first and jumps straight to
        // ALLOW, so the block below it is never reached. Compiling that would
        // produce a filter that installs, passes every allow test, and ignores
        // the argument entirely.
        let both = Allowlist::new()
            .guard(Guard::scheduling_without_real_time())
            .allow(libc::SYS_sched_setscheduler);

        assert!(matches!(
            both.compile(),
            Err(SeccompError::GuardedAndAllowed(_))
        ));
    }

    #[test]
    fn a_guard_block_does_not_swallow_the_syscalls_after_it() {
        // The guard blocks sit before the plain chain and each one ends in a
        // kill. A block whose "this is not my syscall" jump landed one
        // instruction short would send every call after it into that kill, and
        // the symptom would be a module dying on `read`.
        let allowlist = module_standard();
        let program = allowlist.compile().unwrap();

        for syscall in allowlist.syscalls() {
            assert_eq!(
                evaluate(&program, AUDIT_ARCH_X86_64, syscall),
                SECCOMP_RET_ALLOW,
                "syscall {syscall} was allowed until a guard block was put in \
                 front of it"
            );
        }
    }

    #[test]
    fn a_module_may_use_the_socket_it_was_given_and_may_not_make_another() {
        // The pair of claims that lets a module talk to Thalyx without letting
        // it talk to anything else. `recvfrom` and `sendto` act on a descriptor
        // it already holds; `socket`, `connect` and `bind` are what it would
        // need to obtain one, and they are absent.
        //
        // Written as one test because separating them would let either half
        // pass alone, and either half alone is wrong: without the first the
        // channel is dead, without the second the channel is a hole.
        let allowlist = module_standard();

        for syscall in [libc::SYS_recvfrom, libc::SYS_sendto] {
            assert!(
                allowlist.contains(syscall),
                "syscall {syscall} is missing, so a module cannot answer Thalyx"
            );
        }
        for syscall in [libc::SYS_socket, libc::SYS_connect, libc::SYS_bind] {
            assert!(
                !allowlist.contains(syscall),
                "syscall {syscall} would let a module make a socket of its own"
            );
        }
    }
}
