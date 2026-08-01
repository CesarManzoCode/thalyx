# Development environment

Thalyx enforces permissions from inside the kernel, so part of it cannot be
developed against the machine you are sitting at. A policy that denies the
wrong thing locks the host until you reboot it, and the store Thalyx publishes
into needs Btrfs subvolumes it is allowed to own.

Everything here builds a guest that has both, from nothing, with no manual
steps.

## Getting started

```sh
make -C dev preflight   # can this machine do it at all
make -C dev vm          # build the guest (first run downloads ~700 MB)
make -C dev run         # boot it — Ctrl-A X to quit
make -C dev check       # prove the guest can actually enforce
```

`preflight` checks the host. The one it most often catches on AMD boards is
**SVM disabled in the BIOS** — it ships off on many A320-class boards, and
without it there is no KVM and the guest runs at a crawl.

`check` runs inside the guest and does something the other checks cannot: it
compiles a real BPF LSM program and asks the kernel to load it. Every other
signal — kernel version, `CONFIG_BPF_LSM=y` — can look correct while attaching
still fails, because distributions enable the config and then leave `bpf` out
of the default LSM order. The hooks exist and nothing can reach them. Only
trying it tells you the truth.

## Working on the LSM

```sh
make -C dev sync        # copy the source into the guest
make -C dev ssh
# inside the guest:
cd ~/thalyx/lsm
make            # compile
make load       # attach, in observe mode
make status     # what it would deny
make enforce    # start denying
```

`load` deliberately attaches in **observe mode**: hooks are live, denials are
logged, nothing is actually blocked. A security policy should be measurable
before it is binding — run the system, see what it *would* have stopped, and
turn it on once that list looks right. It also means the first run of an
untested policy is not the one that can lock you out.

## Why the guest is configured the way it is

**`lsm=...,bpf` on the kernel command line.** Without it the guest looks
capable and silently is not. This is the single most common way BPF LSM work
stalls.

**Btrfs at `/opt/thalyx`, in a loopback file.** The atomic commit publishes
with `rename` inside one subvolume and the rollback demonstration needs
snapshots; neither works on ext4. A file rather than a second disk keeps the
guest to one image and still gives real subvolumes.

**bpffs at `/sys/fs/bpf`.** The policy map is pinned there. It is the
interface between `thalyx-permd`, which decides policy, and the kernel
program, which only reads it.

## What this is not

It is not the Phase 1 image. That one is built with Alpine's own tooling and
is what a stranger boots to satisfy the exit criterion — see
`vault/09-Notas-Tecnicas/Construccion-del-ISO.md`. This guest is a workbench:
Ubuntu, because its cloud images are small, current and predictable, and
because nothing here ships.
