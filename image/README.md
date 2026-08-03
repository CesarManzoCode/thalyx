# The machine

`vault/09-Notas-Tecnicas/Construccion-del-ISO.md`:

> **La imagen contiene el kernel de Linux y un programa: `thalyx`.**

That is written to be countable, and this is how you count it:

```sh
make count
```

If it says anything other than `1 program(s) in the image`, the decree is broken
and the number says so before anyone has to argue about it.

## What has run and what has not

| | |
|---|---|
| `make image`, `make count` | **Tested.** It is `thalyx dev image`, Rust with tests |
| `make binary` | Untried: the musl target could not be installed where this was written |
| `make kernel` | **Never run.** No route to kernel.org from here |
| `make run` | **Never run.** No QEMU here |

The first `make kernel` is also the first time anyone finds out what is wrong
with `thalyx.config`. Expect the build to want options that are missing — each
one is a line that file should have had, and adding it is the right fix, not a
workaround.

## Why an initramfs and not an ISO

An ISO needs a bootloader, a partition table, and a filesystem to hold them. An
initramfs needs none of those: the kernel unpacks a cpio archive into a tmpfs
and runs `/init`. QEMU is handed the kernel and the archive and nothing else.

That is not a shortcut around the decree. It is the decree with nothing left
over — there is no third thing anywhere in the path for something to hide in,
which is exactly how an Alpine base got in last time.

## Why the kernel starts from `allnoconfig`

Because the opposite direction cannot be audited. A distribution kernel with
options switched off still contains everything nobody remembered to switch off,
and nobody can say what is in it. Starting from nothing means what is on is what
somebody decided to put there, and `thalyx.config` is that list with a reason
beside each group.

## The store

Persistent state is on a separate virtio disk, so the root filesystem keeps
nothing across a boot. PID 1 mounts the three subvolumes `Core-Nucleo.md`
decrees; it does not create them. A machine that quietly made itself a new store
whenever it could not find one could never tell you it had lost the old one.

## What is missing, and it is the one that will bite

`thalyx-lsm` is not attached at boot. The loader shelled out to `bpftool`, and
there is no bpftool in this image and no shell to run it from — see the note in
`crates/thalyx-cli/src/init.rs`. The machine will boot and say so: enforcement
absent, in the same words `thalyx session` uses on any machine that lacks it.

That is honest, and it is also the largest hole. Loading BPF from inside Thalyx
is the work that closes it.
