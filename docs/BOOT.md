# Booting the machine

This is the walkthrough the project is judged by. At the end you will have
booted an operating system that is a Linux kernel and exactly one program — no
distribution, no shell, no `ls`, no package manager — installed a signed module
into it, been asked to authorise what that module wanted, taken it back, and
rebooted into a machine that still remembered the conversation.

Those six steps were the entire exit criterion for Phase 1
(`vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md`). They are checked on
every change and on every hardware run.

It is written for **Linux Mint**, and works the same on Ubuntu and Debian. It
has also been done on Fedora.

## What you need

| What | How much |
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

## Step 0 — get the code and ask what is missing

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

## Step 0b — the kernel you are about to compile is already pinned

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

## Step 1 — build and boot

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

## Steps 2 to 6 — at the machine's own prompt

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

Those six steps were the entire exit criterion for Phase 1 — not a list of
components, but a person outside the project doing exactly this, from this file,
with nobody helping. That last part was suspended on 2026-08-06 in favour of the
ISO. The steps still have to work, and they are checked on every change and on
every hardware run; what is no longer required right now is a stranger
performing them.

## If something fails

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
