# Thalyx

An open-source operating system designed from the kernel outward, where AI is a
first-class citizen rather than one more application, and the human stays
sovereign.

> **Status: design phase.** There is no code in this repository yet. What exists
> is the design vault — the record of every decision, why it was made, and what
> was deliberately deferred.

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
