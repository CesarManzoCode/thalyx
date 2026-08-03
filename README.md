# Thalyx

An open-source operating system designed from the kernel outward, where AI is a
first-class citizen rather than one more application, and the human stays
sovereign.

> **Status: Phase 1, core infrastructure built.** Three of the four base
> primitives — the fourth, the predictive scheduler, is Phase 2 — and the
> canonical flow are built and **verified on real hardware**: 392 tests and 44
> checks on one machine with a BPF LSM, cgroup v2 and Btrfs, with nothing left
> unproven *there*. Modules install atomically, run confined, and can be rolled
> back; the kernel LSM actually denies; subvolumes can be snapshotted and
> restored. Missing: the conversational agent, the bootable image, and any use
> by a person outside the project — which is what actually ends Phase 1.
>
> Nothing here is claimed without a check that could have failed. Anything a
> machine cannot verify is reported as `NOT PROVEN`, never as a pass.

## Try it

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

- **Phase 1** — Thalyx core on an Alpine base: core, LSM, permission broker,
  semantic index, module manager, sandbox, local agent, CLI, Btrfs snapshots.
  Ends when an outsider can install, revert and reboot unaided.
- **Phase 2** — Empirical validation. Benchmarks decide whether primitives move
  into the kernel.
- **Phase 3** — Kernel migration, if the numbers justify it.
- **Phase 4** — Ecosystem.

## License

GPLv3 for userspace components. GPLv2 for anything that links against the Linux
kernel, which is GPLv2-only — see `vault/05-Decisiones-y-Debates/Decision-Licencia.md`.
