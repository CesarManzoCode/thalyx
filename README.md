# Thalyx

**[🇲🇽 Léelo en español →](README.es.md)**

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

The last paragraph is a decree, and part of it is a *destination* rather than a
description. What is true today is written where it can be checked: see
[What is actually true right now](#what-is-actually-true-right-now).

---

## What this is for

Read this before running anything, because the commands only make sense once
you know what they are trying to demonstrate.

On Windows, Linux and macOS, an AI agent is a **guest**. To do anything it has
to pretend to be a human: drive a keyboard and mouse, or call APIs that are a
tracing of human interaction. Permissions were designed for human processes, the
scheduler for human workloads, the filesystem for human hierarchy. Every one of
those is a ceiling on what an agent can do fluently, and none of them can be
lifted from inside an application.

Thalyx inverts the relationship: instead of the AI adapting to the OS, the OS is
built around the AI — while the human keeps a **complete, undegraded path** that
never goes through the agent at all.

That second half is the part that constrains everything. An OS where the AI is
the only way to get things done is a worse OS, not a better one.

### The two rules the whole design answers to

**Double route.** Everything the agent can do, a human can do directly, without
the agent and without losing capability. The agent is an accelerator, never a
mandatory intermediary. This has a consequence the design is built around:
Thalyx never has complete knowledge of its own filesystem — because you are free
to change things behind its back — so no destructive operation is allowed to
assume it does.

**The agent is not trusted.** It sits outside the trusted computing base. It
cannot execute anything directly, cannot compose the prompts you authorise
against, and cannot let untrusted text it has read decide what happens. The core
revalidates everything it produces.

If you only take one idea from this repository, take the second one. Most
systems that put an LLM near a shell are one prompt injection away from
disaster. Thalyx is built on the assumption that the model **will** eventually
be talked into trying something, and arranges for that to be survivable.

---

## Boot it: the whole thing, from nothing

This is the part worth doing. At the end you will have booted an operating
system that is a Linux kernel and exactly one program — no distribution, no
shell, no `ls`, no package manager — installed a signed module into it, been
asked to authorise what that module wanted, taken it back, and rebooted into a
machine that still remembered the conversation.

It is written for **Linux Mint**, and works the same on Ubuntu and Debian. It
has also been done on Fedora.

### What you need

| | |
|---|---|
| **Disk** | ~15 GB free. The kernel source and build are most of it |
| **RAM** | 4 GB. QEMU is given 2 GB |
| **Time** | 20–60 minutes, almost entirely compiling the Linux kernel |
| **Rights** | `sudo` exactly once, to format a disk image |
| **Network** | To download the kernel source and the Rust toolchain |

Your own machine is not modified. Nothing is installed outside this directory
except the packages you choose to install and the Rust toolchain, and nothing
touches your bootloader, your partitions or your running system. Thalyx boots
**inside QEMU**, as a virtual machine.

### Step 0 — get the code and ask what is missing

```sh
git clone https://github.com/CesarManzoCode/thalyx.git
cd thalyx
make -C image doctor
```

`doctor` downloads nothing and builds nothing. It exists because of a specific
kind of misery: what stops people here is never a hard problem, it is a missing
package — found **one at a time**, each one only after everything before it
succeeded. A missing `bc` costs you the entire kernel download and build before
it surfaces, and then the next missing tool costs it again.

So `doctor` finds them all at once and prints the single line that fixes them:

```sh
sudo apt install bc bison build-essential btrfs-progs clang curl dwarves \
                 file flex libbpf-dev libelf-dev libssl-dev qemu-system-x86 \
                 tar xz-utils
```

Run `make -C image doctor` again afterwards. It will tell you if anything is
still missing, including things it could not check the first time.

**Rust is separate**, because the version `apt` carries is older than this
workspace needs and brings no `rustup` to add the static target with:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then open a new terminal, or `source "$HOME/.cargo/env"`.

### Step 0b — the kernel you are about to compile is already pinned

Nothing to do here. It is written down because it is worth understanding, and
because it is the whole difference between Thalyx and a distribution.

**Thalyx compiles its own kernel.** That tarball is not a dependency it links
against — it *becomes the most privileged half of the machine you are about to
run*. HTTPS tells you who served the bytes. It does not tell you what the bytes
were, and a CDN that served something else would produce a kernel nobody
checked, on a machine that would boot and say nothing about it.

So `image/Makefile` carries the digest of the exact tarball this image builds,
and the build refuses anything else. It was established on 2026-08-06 against
kernel.org's **signed** list of digests, and the key that signed it is recorded
next to the digest — a bare hash tells you what was accepted, not what
established it.

If a `make` ever says the digest does not match, stop. The file you downloaded
is not the file kernel.org signed.

**To re-establish it yourself**, or after changing `KVERSION`:

```sh
make -C image pin-kernel
```

It prints four commands rather than running them, on purpose: a target that
downloaded the tarball and recorded its own hash would look like verification
and be none — it would prove the file did not change between two reads of it,
which nobody was ever worried about. What establishes anything is the signature,
and checking a signature means *you* deciding whose key to trust. Compare the
fingerprint it prints.

### Step 1 — build and boot

```sh
make -C image              # kernel, program, image. The kernel is the long part
make -C image store-stage  # what goes on the machine's disk
sudo make -C image store   # format it. The one command that needs root
make -C image run          # boot
```

`sudo` appears exactly once, and only to format a disk image with Btrfs and copy
files into it. Nothing else asks for a password, and `make run` must not — a
boot that needed root would put QEMU and everything inside it under root for no
reason.

Before you go on, look at what you built:

```sh
make -C image count
```

It lists what is inside the image. The answer is the Linux kernel and **one**
program. That is the founding claim of the project, and it is countable rather
than quotable — if it ever says two, the claim is broken.

The machine comes up, says what it does and does not have, and waits. There is
**no login**, because there is nobody else to be. There is **no shell**: what is
not a word the session knows does not exist.

### Steps 2 to 6 — at the machine's own prompt

Type these one at a time and read what comes back. The point is not the
commands; it is what each one demonstrates.

```
> disponibles
```

What the local repository holds. The disk ships with a signed module — the
`greeter` — **deliberately not installed**. A machine that booted with it
already in place would make the next step impossible to perform.

```
> instalar dev.thalyx.greeter
```

Thalyx verifies the signature against the publisher's key, recomputes the
artifact's digest itself rather than believing the manifest, and then **stops
and asks you**. What you see is drawn by Thalyx, inside a frame, listing every
permission the module will hold:

```
┌─ Thalyx — capability authorisation ──────────────────
│ Greeter (dev.thalyx.greeter)
│ version 1.0.0
│
│ This module permanently requests:
│   · read access to /opt/thalyx/data/greeter/notes.txt
│
│ These permissions come from the module's signed manifest.
│ They stay in force until you revoke them by hand.
└──────────────────────────────────────────────────────
Confirm? [y/N]
```

**That frame is a security mechanism, not decoration.** It is generated by the
core from the signed manifest — the agent cannot compose it, cannot reword it,
and cannot show you a subset of what is being requested. Try answering `n`
first: nothing is installed, and nothing is remembered either.

Then install it for real, and look at what you granted:

```
> permisos
> modulos
```

Now run it:

```
> correr dev.thalyx.greeter
```

The module asks Thalyx who it is — it does not know its own name — reads the one
file it was granted, and is refused `/etc/shadow`, which it was not. Everything
it says to you arrives **through Thalyx**, labelled, because a module has no
terminal of its own. It cannot print to your screen.

Take it back, then ask the machine what it will still know later:

```
> revertir
> recuerdos
> apagar
```

Now boot it again and ask once more:

```sh
make -C image run
```

```
> recuerdos
```

It tells you what you asked it to do, **and that the installation it made no
longer checks out** — with nobody having told it the module is gone. That is the
difference between a memory and a log: it went and looked. A record that simply
replayed what it was told would still be claiming the install stands.

Those six steps were the entire exit criterion for Phase 1
(`vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md`) — not a list of
components, but a person outside the project doing exactly this, from this file,
with nobody helping. That last part was suspended on 2026-08-06. The steps
still have to work, and they are checked on every change and on every hardware
run; what is no longer required right now is a stranger performing them.

### If something fails

Read what it printed before assuming the worst. The machine is built to
distinguish **"it is not there"** from **"I could not look"**, and it says which
one happened. `NOT PROVEN` never means the same as a pass.

- **`make run` says nothing to boot** — `make -C image` did not finish.
- **`make run` says no store disk** — you skipped `store-stage` or `store`.
- **The prompt says nothing enforces a permission yet** — the kernel came up
  without the BPF LSM. `correr` refuses rather than running a module with
  nothing enforcing its permissions, which is deliberate: a module running
  unconfined behaves exactly like a confined one right up until it does
  something it should not have been able to do.
- **Anything else** — `estado` re-reads the machine, `nucleo` shows what the
  kernel has been saying. There is no `dmesg` in there; this is how you look.

---

## What is actually true right now

Every claim in this section is either checkable with a command in this
repository or marked as not yet checked. That distinction is the project's
main working rule.

**Built and covered by tests: 785 of them**, across unit tests, fault injection
that kills the real binary at each point of the atomic commit, and end-to-end
runs of the exit criterion. `cargo test --workspace` runs all of it.

**Verified on real hardware**: 110 checks, on one machine with a BPF LSM, cgroup
v2 and Btrfs — `sudo ./dev/verify.sh`, on 2026-08-07 — including the kernel
mounting a Btrfs filesystem Thalyx wrote byte by byte with no `mkfs.btrfs`. Two
things failed and both were the harness rather than Thalyx; both are fixed. One
thing went unproven, and it is a thing that does not exist yet rather than a
check that could not be made: the agent has no model. The BPF LSM has denied a
real network connection to a process that lacked the permission, and only to
that process.

**Not yet run anywhere**: five more checks covering Thalyx creating a store's
three Btrfs subvolumes through `BTRFS_IOC_SUBVOL_CREATE`, since there is no
`btrfs` binary in the image. They need a kernel with Btrfs, so the development
container reports them as `NOT PROVEN` and says so.

**The image boots, and does the six steps by itself.** A kernel built from
`allnoconfig` comes up in QEMU with one program inside it, attaches its own
enforcement with no `bpftool` and no second file, mounts its Btrfs store,
installs a signed module from its own repository through the trusted path, runs
it confined, reverts it, powers itself off — and on the next boot says what it
was asked to do and that the install it made no longer checks out. That whole
sequence is stage 16 of `verify.sh`, typed into a real machine from a cold
boot.

### Not yet true, stated plainly

- **No real PC has booted this, only a virtual one.** On 2026-08-07 a UEFI
  firmware found `\EFI\BOOT\BOOTX64.EFI` on a disk Thalyx wrote and started it —
  no `-kernel`, no `-append`, no boot loader — and the machine found its own
  store with nothing naming it, put its session on the screen through the
  firmware's framebuffer, took `apagar` from the keyboard and powered down.
  `thalyx install <disk>` writes the GPT, a 512 MiB FAT32 boot partition holding
  the kernel, and the rest as a Btrfs store with its three subvolumes, with no
  `sgdisk`, no `mkfs.vfat` and no `mkfs.btrfs`, because the image holds the Linux
  kernel and one program. **What remains is hardware**: the USB keyboard (xHCI +
  HID), NVMe/AHCI disks, and USB storage — none of which `make run-installed`
  reaches, because everything it attaches is virtio and its keyboard is PS/2. The
  same file that installs is the medium: `dd` it to a USB stick and a PC started
  from it installs itself onto its own disk with the session's `discos` and
  `instalar-en`, reading the kernel off the stick with Thalyx's own FAT reader and
  mounting nothing.
- **`make -C image run-hardware` is as close to that as a machine gets, and it has
  never run.** QEMU emulates xHCI, NVMe, AHCI and a USB disk, and the kernel driver
  that talks to an emulated controller is the same one that talks to real silicon —
  so it answers that the options are compiled, that the drivers bind, and that
  `/dev/nvme0n1` comes back with its partitions named `nvme0n1p1`. It does not
  answer real firmware, real silicon, or a physical stick in a physical port, and
  it is not a substitute for either.
- **Five kernel options in this project's history were found by booting or by
  reading, and by no build check.** Four were found by booting. The fifth,
  `CONFIG_USB_STORAGE`, was found on 2026-08-07 by reading the config while
  preparing the hardware run — the UEFI specification has the *firmware* read the
  boot medium, so a machine without that driver boots off a USB stick, looks
  entirely healthy, and only fails two commands later when the installer looks for
  that stick in `/sys/block`. `config-check` stops the build when Kconfig drops an
  option that was asked for; it cannot see one nobody asked for, which is what the
  five tests over `thalyx.config` are for. Expect more of them on the first real
  machine.
- **Nobody outside the project has done the six steps**, and that is no longer
  the exit criterion — it was suspended the same day, in favour of the ISO. The
  steps still have to work and are checked on every change; what is suspended
  is *who types them*.
- **The conversational agent has no model.** The deterministic half is built and
  works; there is no LLM behind it. The session says *"I have no model loaded"*
  rather than pretending. Model selection is decreed
  (`vault/03-Primitivas/Gamas-de-Modelo.md`) and not implemented.
- **The predictive scheduler is Phase 2.** It is design, not code.
- **`thalyx_watch` has never been loaded without `bpftool`.** The BPF loader
  Thalyx carries is proven on the LSM object — it loads it, attaches it, and
  that enforcement denies. The watcher is ten hooks instead of two and has not
  been tried. Likely is not proven.

### Three limits of the enforcement, stated rather than discovered

- **The LSM enforces by class of action, not by path.** Every absolute read
  grant becomes one `FS_READ` bit and every write grant one `FS_WRITE` bit, and
  the BPF program checks the bit. What confines a module to the *particular*
  paths it was granted is the root filesystem — which contains nothing else —
  the module's own uid, and the checks in Thalyx's internal API, which open
  under a descriptor for the granted directory with the kernel refusing to leave
  it. The LSM is a second, coarser layer. Calling it per-path enforcement would
  be claiming more than it does.
- **`net/outbound` has not been exercised end to end on hardware.** The LSM
  denying a connection to a module *without* the grant is proven and
  reproducible (`make -C lsm demo`). A module *with* the grant opening a
  connection is implemented and unit-tested, and has not yet run on a machine.
- **Snapshots need `btrfs-progs`, which the image does not have and cannot.**
  `thalyx-snapshot` shells out to `btrfs`, so snapshot and restore work on a
  host that has it — where `dev/verify.sh` exercises them — and not inside the
  minimal image, which carries one program.

### And one contradiction, since it is the honest thing to publish

The founding decree says modules speak to Thalyx **exclusively** through its
API, not through POSIX and not through libc. Today a module is a dynamically
linked Linux binary: the sandbox mounts `/usr`, `/lib`, `/bin` and `/etc`
read-only so it can start at all, and the seccomp filter permits around 120
syscalls.

The distinction that does hold, and is what the code actually implements:

> **The Thalyx API is the only *mediated* surface.** It is not the only
> reachable one.

Identity, permissions, granted files and speaking to the human exist through it
and nowhere else. What stays reachable through POSIX is bounded by three layers
that do exist: a root filesystem containing nothing that was not mounted into
it, a filter that kills what is not on its allowlist, and the LSM. None of that
makes a module a program that does not speak POSIX. It makes speaking POSIX get
it nowhere the human did not authorise.

Closing the gap fully — static modules, no libc, a much smaller filter — is a
Phase 2 decision recorded in `vault/02-Arquitectura/Sistema-de-Modulos.md`.

---

## Try it as a program instead

Everything below runs Thalyx on top of the Linux you already have. That is a
**test bench and not a way to use Thalyx** — the vault calls it scaffold rather
than destination — and the program will tell you so: started from a shell, the
session says *this is not the machine*, because it reads its own parent to find
out rather than being told.

It is here because it is how the system gets verified.

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

Watch the atomic commit survive being killed at its most dangerous instant —
between the directory rename and the symlink swap:

```sh
THALYX_FAULT_POINT=mid-commit ./target/debug/thalyx module install demo.thmod --yes
# process dies with SIGABRT: no unwinding, no cleanup, no chance to tidy up

./target/debug/thalyx module list     # not installed
./target/debug/thalyx store status    # one inert orphan, store consistent
./target/debug/thalyx module install demo.thmod   # retry succeeds
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

---

## Verifying it on a real machine

Most of what Thalyx claims cannot be checked in a container. The BPF LSM needs a
kernel with `bpf` in its LSM order, resource limits need delegated cgroup
controllers, and "enforcement is real" means a connection actually gets denied.

```sh
sudo ./dev/verify.sh
```

It never counts a check it could not make as a pass. Anything the machine cannot
do is reported as `NOT PROVEN`, with the reason, and listed again in the summary
— because a green run that exercised nothing looks exactly like a green run that
exercised everything.

It leaves nothing loaded: the LSM is detached on the way out, including on
Ctrl-C.

---

## The four base primitives

| Primitive | What it does | Where it lives |
|---|---|---|
| Graph filesystem | Query files by semantic relationships instead of paths | Userspace (SQLite) |
| Just-in-time permissions | The agent requests temporary access; the OS grants and revokes it | Kernel (`thalyx-lsm`) + userspace broker |
| Persistent memory | Task state survives across sessions and reboots | Userspace |
| Predictive scheduler | Adjust process priority from context | Userspace (Phase 2) |

---

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
  thalyx-bpf/       Thalyx's own BPF loader: no libbpf, no bpftool

lsm/            BPF LSM programs: enforcement, and the filesystem watcher
image/          the machine: kernel configuration, initramfs, store disk
dev/            verify.sh — one command that checks every claim on real hardware
modules/        dev.thalyx.greeter, the first module written against the API

vault/          Design vault (Obsidian, Spanish)
```

The vault is the authority: code implements decrees, it does not invent them.
Start at `vault/00-Indice/Indice-Principal.md` for the reading order.
`vault/06-Pendientes/Punto-Actual.md` says where the project stands right now
and what the next step is; it is updated whenever something is finished.

The vault is written in Spanish. Everything else — code, schemas, identifiers,
commit messages, CLI output — is in English.

---

## Roadmap

- **Phase 1** — The machine and what runs on it: core, LSM, permission broker,
  semantic index, module manager, sandbox, local agent, CLI, Btrfs snapshots.
  Ends when an outsider can boot it, install, revert and reboot unaided — see
  **Boot it** above, which is that list of steps and nothing else.
- **Phase 2** — Empirical validation. Benchmarks decide whether primitives move
  into the kernel.
- **Phase 3** — Kernel migration, if the numbers justify it.
- **Phase 4** — Ecosystem.

## License

GPLv3 for userspace components. GPLv2 for anything that links against the Linux
kernel, which is GPLv2-only — see
`vault/05-Decisiones-y-Debates/Decision-Licencia.md`.
