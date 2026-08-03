# The image

Implements `vault/09-Notas-Tecnicas/Construccion-del-ISO.md`.

> **None of this has ever run.** It was written in a container with no Alpine
> tooling, no QEMU, and a network policy that refuses the Alpine repositories.
> Every file here is a proposal, and the first `make iso` is also the first
> time anyone finds out what is wrong with it. The tested half of this work is
> `thalyx session`; the image around it is not.

## The one file that matters

`overlay/etc/inittab`.

An Alpine base puts a getty on tty1, which is a login, which is a shell. Nobody
ever decreed that login — it is what the base hands you for free if that file is
left alone. And leaving it alone would break the decree in
`Decision-Capa-vs-SO-Nuevo.md` while appearing to satisfy it:

> Thalyx es **dueño del arranque**, del sistema de módulos, de la política de
> permisos y de los requisitos de filesystem.

A system you reach by logging into somebody else's session and running a command
does not own the boot. So there is no getty in that file and no shell in the
image. Not hidden, not behind a flag — not installed.

That is also the answer to "is this an operating system or a program". On Linux
you are always in Linux running a program, and there is always a way out. Here
leaving has nowhere to go, and `thalyx session` says so **only when it is
true**: run on a development machine it reports that something else booted it,
which is how you can tell the sentence is not decoration.

## What goes in

- `thalyx`, built statically against musl.
- A repository at `/opt/thalyx/repo` holding `dev.thalyx.hola`, signed with a
  key generated during the build and discarded with it. The image's repository
  exists so a first boot can install something with no network, per the exit
  criterion — it is not a distribution channel, and a publisher key that
  outlived the build would be a key nobody meant to trust.
- `thalyx-lsm`, attached by an init script that runs **before** the session.
  Ordering rather than tidiness: `module run` refuses to start a module while
  the policy map is absent, so a session that came up first would spend its
  early life reporting an enforcement layer that was merely late.

## What is not decided yet

- **The Btrfs subvolume layout is created by the Makefile, not by an
  installer.** Phase 1 boots a prepared machine. Whoever writes the installer
  inherits this decision and should re-open it.
- **`s6` or `busybox-run`.** `Core-Nucleo.md` decrees "s6 or busybox-run, not
  systemd" and leaves the choice open; this uses OpenRC because that is what an
  Alpine profile gives, which is a third answer nobody picked. It needs
  deciding rather than inheriting — the same mistake as the login, one layer
  down.
- **What happens on `apagar`.** The session names the command and nothing
  implements it.
