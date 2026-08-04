# Thalyx

An open-source operating system designed from the kernel outward, where AI is a
first-class citizen rather than one more application, and the human stays
sovereign.

> **Thalyx is the operating system.** The Linux kernel is a component Thalyx
> manages, not the host it rests on. No intermediate layers, no distributions —
> nothing that is not Thalyx. Modules and the agent speak to it through Thalyx's
> API, not through POSIX, not through libc, not through shell scripts. If Linux
> disappeared, Thalyx would find another engine. If Thalyx disappeared, there is
> no system.
>
> The image carries the Linux kernel and one program, and that is countable
> rather than quotable: `make -C image count`.

> **Status: Phase 1, core infrastructure built.** Three of the four base
> primitives — the fourth, the predictive scheduler, is Phase 2 — and the
> canonical flow are built and **verified on real hardware**: 641 tests and 72
> checks on one machine with a BPF LSM, cgroup v2 and Btrfs, with nothing left
> unproven *there*. Modules install atomically, run confined, and can be rolled
> back; the kernel LSM actually denies; subvolumes can be snapshotted and
> restored. **The image boots**: on 2026-08-03 a kernel built from `allnoconfig`
> came up in QEMU with one program inside it, mounted its seven filesystems, and
> said out loud what it does and does not have. It now has a store of its own —
> a Btrfs disk made at build time, because `mkfs.btrfs` cannot live in an image
> that holds one program — carrying the first module in a local repository, and
> a session with no shell behind it that can install it, show what it asks for
> on the trusted path, run it, and undo the whole thing. It also carries its own
> BPF loader — no bpftool, no second file — and that loader attached enforcement
> on real hardware, and the enforcement it attached denied a connection. What it
> did through that session it writes down on its own disk, so a restart finds it
> still knowing what was asked and re-checking what it did rather than repeating
> it. **Every one of the six steps that end Phase 1 can now be performed** — see
> **Boot it** below. Missing: a person outside the project performing them,
> which is what actually ends it, and the conversational agent, which the
> criterion does not ask for and the machine does not pretend to have.
>
> Nothing here is claimed without a check that could have failed. Anything a
> machine cannot verify is reported as `NOT PROVEN`, never as a pass.
>
> **Not yet re-verified on hardware:** an external security audit on 2026-08-04
> found nine real defects — among them a decreed global lock that nothing
> implemented, an interrupted upgrade that could leave the running version
> holding the *next* version's permissions, and a corrupt keystore that parsed
> as an empty one, which trusts every publisher it is offered. All are fixed and
> covered by tests that were confirmed to fail without the fix. The 72 hardware
> checks have not been run since. Until they are, that half of this paragraph
> describes the commit before this one.

## Boot it

This is Thalyx as Thalyx: a kernel and one program, booting in QEMU, with no
distribution underneath and no shell behind it. It is written for a machine
running Linux Mint, Ubuntu or Debian, and it has been done on Fedora too.

**Start here.** It says everything that is missing, all of it at once, and
prints the one command that installs the lot:

```sh
make -C image doctor
```

It downloads nothing and builds nothing. Missing packages are the only thing
that stops people at this step, and finding them one at a time means finding
each one *after* the last thing succeeded — so an absent `bc` costs the whole
kernel build, and the tool after it costs another.

Then, in order:

```sh
make -C image              # kernel, program, image. The kernel is the long part
make -C image store-stage  # what goes on the machine's disk
sudo make -C image store   # format it. The one command that needs root
make -C image run          # boot
```

`sudo` appears exactly once, and only to format a disk image with Btrfs and
copy files into it. Nothing else in this asks for a password, and `make run`
must not — a boot that needed root would put QEMU and everything inside it
under root for no reason.

### What to do at the prompt

The machine comes up, says what it does and does not have, and waits. There is
no login, because there is nobody else to be. There is no shell behind it: what
is not a word the session knows does not exist.

```
  > disponibles                     what the local repository holds
  > instalar dev.thalyx.greeter     answer the permission prompt yourself
  > permisos                        what is granted, and to whom
  > correr dev.thalyx.greeter       the module speaks to Thalyx through its API
  > revertir                        take the install back
  > recuerdos                       what the machine will still know afterwards
  > apagar
```

Then boot it again with `make -C image run` and type `recuerdos`. It will tell
you what you asked it to do, and that the install it made no longer checks out
— which is the difference between a memory and a log: it went and looked.

Those seven lines are the whole of
`vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md`, which is the only
thing that ends Phase 1. Not a list of components: a person outside the project
doing exactly this, from this file, with nobody helping.

### If something fails

The machine is built to say which of "it is not there" and "I could not look"
happened, so read what it printed before assuming the first. `make -C image
count` says how many programs are inside the image, and the answer has to be
one.

## Try it as a program instead

Everything below runs Thalyx on top of a Linux you already have. That is a test
bench and not a way to use Thalyx —
`vault/05-Decisiones-y-Debates/Decision-Capa-vs-SO-Nuevo.md` calls it scaffold
rather than destination — and the program itself will tell you so: started from
a shell, the session says *this is not the machine*, because it reads its own
parent to find out rather than being told.

It is here because it is how the system gets verified — see **Verifying it on a
real machine** below.

```sh
cargo build

# Publisher side: a key and a signed bundle
./target/debug/thalyx dev keygen --out publisher.key
./target/debug/thalyx dev pack ./payload \
    --manifest manifest.toml --key publisher.key --out demo.thmod

# User side
export THALYX_ROOT=/tmp/thalyx-demo
./target/debug/thalyx module install demo.thmod
./target/debug/thalyx module list
./target/debug/thalyx permissions
./target/debug/thalyx journal

# Undo the install. Narrow and cheap: it takes back only what Thalyx published.
./target/debug/thalyx rollback --dry-run
./target/debug/thalyx rollback
```

The semantic index, and letting the kernel tell it when it is still current:

```sh
thalyx graph build ./crates
thalyx graph dependents thalyx-core/src/commit.rs
thalyx graph watcher                    # what the kernel's watcher can see
thalyx graph trust ./crates --counter   # earn the fast path, or be refused
```

What the agent will remember between sessions, driven by hand until it exists:

```sh
thalyx memory remember refactor "moved login() to auth.rs" --about src/auth.rs
thalyx memory recall refactor           # re-checked against the files, now
```

Snapshots, and the destructive command that returns to one:

```sh
thalyx snapshot take ~/work --label before-upgrade
thalyx snapshot list ~/work
thalyx restore <name> ~/work            # shows what it would destroy, then asks
```

To watch the atomic commit survive being killed in its most dangerous instant —
between the directory rename and the symlink swap:

```sh
THALYX_FAULT_POINT=mid-commit ./target/debug/thalyx module install demo.thmod --yes
# process dies with SIGABRT, no unwinding, no cleanup

./target/debug/thalyx module list     # not installed
./target/debug/thalyx store status    # one inert orphan, store consistent
./target/debug/thalyx module install demo.thmod   # retry succeeds
```

## Verifying it on a real machine

Most of what Thalyx claims cannot be checked in a container. The BPF LSM needs
a kernel with `bpf` in its LSM order, resource limits need delegated cgroup
controllers, and "enforcement is real" means a connection actually gets denied.

One command exercises all of it and reports what it managed to prove:

```sh
sudo ./dev/verify.sh
```

It never counts a check it could not make as a pass. Anything the machine
cannot do is reported as `NOT PROVEN`, with the reason, and listed again in the
summary — because a green run that exercised nothing looks exactly like a green
run that exercised everything.

It leaves nothing loaded: the LSM is detached on the way out, including on
Ctrl-C.

## The idea

On Windows, Linux and macOS, an AI agent is a guest. It has to simulate being a
human: driving a keyboard and mouse, or calling APIs that are a tracing of human
interaction. Permissions were designed for human processes, the scheduler for
human workloads, the filesystem for human hierarchy. Every one of those is a
ceiling on what an agent can do fluently.

Thalyx inverts the relationship. Instead of the AI adapting to the OS, the OS is
built around the AI — while the human keeps a complete, undegraded path that
never goes through the agent at all.

## The four base primitives

| Primitive | What it does | Where it lives |
|---|---|---|
| Graph filesystem | Query files by semantic relationships instead of paths | Userspace (SQLite) |
| Just-in-time permissions | The agent requests temporary access; the OS grants and revokes it | Kernel (`thalyx-lsm`) + userspace broker |
| Persistent memory | Task state survives across sessions and reboots | Userspace |
| Predictive scheduler | Adjust process priority from context | Userspace (Phase 2) |

Three limits of the current enforcement, stated here rather than discovered:

- **The LSM enforces by class of action, not by path.** Every absolute read
  grant becomes one `FS_READ` bit and every write grant one `FS_WRITE` bit, and
  the BPF program checks the bit. What confines a module to the *particular*
  paths it was granted is the root filesystem — which contains nothing else —
  the module's own uid, and the checks in Thalyx's internal API, which resolve
  under a descriptor for the granted directory with the kernel refusing to leave
  it. The LSM is a second, coarser layer. Calling it a per-path enforcement
  would be claiming more than it does.
- **`net/outbound` has not been exercised end to end on hardware.** The LSM
  denying a connection to a module without the grant is proven and reproducible
  (`make -C lsm demo`). A module *with* the grant opening a connection is
  implemented and covered by unit tests, and has not yet run on a machine.
- **Snapshots need `btrfs-progs`, which the image does not have and cannot.**
  `thalyx-snapshot` shells out to `btrfs`, so snapshot and restore work on a
  host with it installed — where `dev/verify.sh` exercises them — and not inside
  the minimal image, which carries one program. Making it native to Thalyx means
  the ioctls, and that is not built.

## Two principles that constrain everything

**Double route.** Everything the agent can do, the human can do directly,
without the agent and without losing capability. The agent is an accelerator,
never a mandatory intermediary. This has a consequence the design is built
around: Thalyx never has complete knowledge of its own filesystem, so no
destructive operation is allowed to assume it does.

**The agent is not trusted.** It sits outside the trusted computing base. It
cannot execute anything directly, cannot compose the prompts the human
authorises against, and cannot let untrusted text it has read determine what a
contract does. The core revalidates everything it produces.

## Repository layout

```
crates/
  thalyx-manifest/  .thmod parsing, validation, ed25519 signatures
  thalyx-contract/  structured contracts with per-field provenance
  thalyx-parser/    mechanical parser: Rust, Python, JS/TS, C, Go
  thalyx-graph/     the semantic index, and the discipline around its freshness
  thalyx-watch/     reads the kernel's mutation counter
  thalyx-memory/    persistent memory, with its own vector store
  thalyx-permd/     permissions translated into kernel policy
  thalyx-sandbox/   namespaces, seccomp, pivot_root, idmapped mounts, cgroups
  thalyx-syscall/   the only crate where `unsafe` is permitted
  thalyx-snapshot/  Btrfs subvolumes and snapshots
  thalyx-journal/   append-only operation journal
  thalyx-core/      verification, staging, atomic commit, permissions, rollback
  thalyx-cli/       the `thalyx` binary

lsm/            BPF LSM programs: enforcement, and the filesystem watcher
dev/            verify.sh — one command that checks every claim on real hardware

vault/          Design vault (Obsidian, Spanish)
  00-Indice/            Entry point — start here
  01-Filosofia/         Founding principles
  02-Arquitectura/      System architecture
  03-Primitivas/        The four base primitives
  04-Flujo-Canonico/    How an action flows end to end
  05-Decisiones-y-Debates/  Resolved debates and their reasoning
  06-Pendientes/        Open work
  07-Adopcion-y-Fases/  Roadmap and adoption gates
  08-Investigacion/     Research directions
  09-Notas-Tecnicas/    Implementation reference
  10-Carrera-.../       Personal and career context
  11-Seguridad/         Threat model and security decrees
```

Start at `vault/00-Indice/Indice-Principal.md`. It carries the reading order and
a snapshot of what is decided and what is still open.
`vault/06-Pendientes/Punto-Actual.md` says where the project stands right now
and what the next step is; it is updated whenever something is finished.

The vault is written in Spanish. Everything else — code, schemas, identifiers,
commit messages, CLI output — is in English.

## Roadmap

- **Phase 1** — The machine and what runs on it: core, LSM, permission broker,
  semantic index, module manager, sandbox, local agent, CLI, Btrfs snapshots.
  Ends when an outsider can boot it, install, revert and reboot unaided —
  see **Boot it** above, which is that list of steps and nothing else.
- **Phase 2** — Empirical validation. Benchmarks decide whether primitives move
  into the kernel.
- **Phase 3** — Kernel migration, if the numbers justify it.
- **Phase 4** — Ecosystem.

## License

GPLv3 for userspace components. GPLv2 for anything that links against the Linux
kernel, which is GPLv2-only — see `vault/05-Decisiones-y-Debates/Decision-Licencia.md`.
