# What is actually true right now

Every claim here is either checkable with a command in this repository or
marked as not yet checked. That distinction is the project's main working rule,
and the reason this file exists separately from the README: the README says what
Thalyx is, this file says how much of it has been proven and where.

The short version lives in [the README](../README.md#where-it-actually-stands).
Everything below is the long one.

> Dates are the day the check was last run. `sudo ./dev/verify.sh` re-runs all
> of it on a machine that can, and never counts a check it could not make as a
> pass.

---

## What is built

**Built and covered by tests — over 1,300 of them**, across unit tests, fault injection
that kills the real binary at each point of the atomic commit, and end-to-end
runs of the exit criterion. `cargo test --workspace` runs all of it.

**The machine can be worked in without the agent.** `ls`, `cat`, `cd`, `pwd`,
`clear` to look; `mkdir`, `touch`, `cp`, `mv`, `rm` to change, with `*` and `?`;
and a terminal that is a terminal — arrows, line editing, a 500-line history,
and tab completing verbs at the start of a line and file names after it. Every
one of them is the same Rust inside `thalyx`, so `make -C image count` still
says one program.

**And those verbs have a second face a program can ask for.** `structured on`,
and every one of them answers with one JSON object per line instead of a
sentence — built from the same fact the person is shown, so the two cannot drift
apart. It hides nothing a person would not be shown, reports exact byte counts
rather than `1.2 kB`, and answers even where a person is told nothing, because a
parser cannot tell silence from a hang. One typed line produces exactly one
object, so a command that touched three files is still one answer. `structured
off` brings the sentences back, and the acknowledgement carries those words for
anyone who turned it on by accident.

**The machine also describes itself.** `describe` answers with all 39 verbs —
names, arguments, flags, which `op` each answers with, whether it can change
anything, and the errors it can give. Nothing on Linux can do that: `--help` is
prose, written once per tool, inconsistent between any two, and often absent.

**And text in a file can be changed, by either route.** `editar notas.txt` opens
a screen — arrows, Ctrl-O to write, Ctrl-X to leave, Ctrl-U to take back the last
change — while `editar notas.txt cambiar 12 <text>` addresses lines and answers
with one object, because a program cannot drive a screen that redraws. Both are
the same engine, so they cannot come to disagree about what the file now says.
The save is a write-then-rename, so a machine that loses power mid-save has
either the old file or the new one; line endings, a missing final newline and the
file's mode are all preserved; and anything that is not text, or is over 4 MiB,
is refused rather than opened and written back mangled.

**And a tree can be searched, three ways that are not the same question.**
`encontrar *.rs` finds files by name anywhere below, `contenido "fn main"` finds
the lines that hold that text — literally, so a dot is a dot — and `buscar
login` answers out of the semantic index instead: where a name is *declared* and
every place it is *used*, without the false positives a text search gets from
comments. The first two read the tree, so they answer about a log or a
`Makefile` or a language nobody wrote a parser for; the third reads the index,
so it answers better wherever it applies. All three page their answers with a
total and a cursor, refuse a tree of more than 20,000 files rather than making
somebody wait for an answer that never comes, skip binaries instead of spraying
them at a terminal that has no second window to recover in, and report what they
could not read rather than counting it as nothing found.

**And the machine answers about itself as a machine.** `procesos` lists what is
running with its number, its state, its parent and what it actually occupies;
`memoria` says how much there is and — separately, and named — how much something
new could actually get, because a healthy Linux keeps `free` near zero on purpose
and a person shown only that number starts killing things. `matar 4711` asks one
process to stop and `matar 4711 forzar` makes it. The signal goes through a
pidfd, so it reaches the process that number named when it was read and never one
that inherited the number since — the race every tool taking a pid on its command
line lives with. PID 1 and the session itself are refused, each naming the verb
that does that job properly, and `ensayo matar 4711` prints exactly which process
that number is, with its whole command line, and sends nothing.

**A signal the kernel accepts and drops is refused rather than reported as
having worked.** A kernel thread has every signal ignored from the moment
`kthreadd` starts it, and a process that has already exited is only a row in the
table until its parent collects it; `kill -9` on either returns success and
changes nothing. Answering "asked to stop" for something that never moved
teaches that Thalyx is unreliable when Thalyx was only credulous — so both are
named, and the already-ended one carries the number of the parent that can clear
it, because a remedy saying "stop the parent" without saying which cannot be
followed.

**A name can have a space in it, and a star can be a name.** The line is split
into words with POSIX's rules as far as POSIX goes today — `'…'`, `"…"`, `\x` —
and a quote that is never closed is refused with its own word rather than
guessed at, because guessing where a name ends is how `rm` acts on something
nobody named. There is no shell language: no pipes, no redirection, no
variables. Expansion stays in the verb and that is a decree, which keeps both
Unix habits where they already were: `rm "*.log"` removes the file actually
called that, the way it does in bash, and `encontrar "*.rs"` is still a pattern,
the way `find . -name "*.rs"` is.

**The network can be seen, and the answer says it cannot be used.** `red` lists
every interface with its kind, hardware address, link state, negotiated speed and
driver, and counts how many of them are actually a card — which is not how many
there are, because the kernel's own software interfaces report an Ethernet type
and carry an address. Two facts survive into what is printed rather than being
flattened: an interface that is down does not report a missing cable, it refuses
the question, and a speed nobody measured is absent rather than shown as a
number. There is no address here, no DHCP and no resolver — those are separate
programs everywhere else, and here they would have to live inside `thalyx` — so
every answer says so, in both faces, because this is the one listing whose things
no other verb can act on.

**And a mistake is cheaper here.** `ensayo rm *.log` works out exactly what
would go and touches nothing — and says `would remove`, never `removed`, because
a sentence reporting a completed act that never happened teaches that the next
sentence cannot be trusted either. It is built so the rehearsal *is* the check
half of the real operation rather than a second implementation that could
disagree with it.
Every action that changed something says how to undo it — and a delete says
plainly that it cannot be undone, because inside `/home` no rollback of ours
reaches.

**No answer can eat a context window.** A listing, an index query, a search, the
history — each comes back with how many there are, how many were sent, and a
cursor for the rest. The cursor names the last row rather than an offset, so a
file deleted between two pages changes *what comes next* and never quietly skips
something; when the tree moved underneath it, the answer says so in the same
object as the rows. A person is never cut off: a window is a fact about a
*context* window, and on the image there is no pager to get the rest back with.

**Searching returns symbols, not lines.** `buscar login` says where the name is
declared, what kind of thing it is, and every place it is used — with neither
comments nor strings in the list, which is the half `grep` cannot do. Over this
repository's own sources it finds close to four thousand declared names.

**Something can be attempted and taken back whole.** `intento empezar <label>`
takes a Btrfs snapshot, and after any amount of work `intento abandonar` returns
the tree exactly as it was while `intento confirmar` keeps it. Both faces are
shown what abandoning would cost — which files would be *deleted* rather than
reverted — before anything moves.

A program can say yes in one call instead of two, by naming the attempt and the
**exact state of the tree** it is authorising the destruction of. That state is a
digest over every path with its size, its modification time, its change time and
its inode; any write by anybody, including a write to a file the caller had
already changed, makes the claim stale and the rollback is refused rather than
done. It is checked inside the lock, immediately before the tree is replaced. A
person is never asked for a digest — they are shown the cost and answer about the
tree in front of them.

**Several operations can be one transaction.** `hacer <program>` takes a list of
requests, what must be true when they are done, and what to do if it is not. It
opens the boundary, runs them in order, observes what really changed, runs the
checks, and commits or rolls the whole thing back — before it answers. Every step
goes through the same argument check and the same workspace boundary a single
request goes through, so composing reaches nothing a caller could not have
reached one call at a time. A check that could not be run counts as a failure and
never as a pass. The answer is small on purpose; everything the machine produced
and did not send back is kept in the store and fetched with `evidencia <id>`.

What that is for is written as a hypothesis rather than a result:
`vault/09-Notas-Tecnicas/Trabajo-Entre-Inferencias.md`. Whether it makes an agent
cheaper or faster is not measured, and this page will not say it is until it is.

**The machine says what it did, and what the kernel saw.** `historia` reads the
journal from inside a session, newest first, saying in a field that it covers
what Thalyx did and not everything that happened. `cambios` drains the BPF ring
buffer the watcher fills: who changed something and how, with the two things a
ring cannot give said plainly — it is not a history, because reading empties it,
and it names no files.

**The semantic index is reachable from a session.** `indexar`, then `depende
<file>` and `usan <file>` — *what refers to this*, which no amount of walking
directories can answer, because dependency is not a property of location. Every
answer carries the index's freshness in the same object as the rows, so a tree
that changed behind Thalyx's back is reported as stale rather than answered as
though it had not.

What is **not** exposed yet: the journal, module listing, and the three things
that need hardware this container does not have — a named attempt that can be
abandoned whole, "what changed since I looked", and a foreign agent running as a
task with a grant that expires. Those are tracked in
`vault/02-Arquitectura/Superficie-para-el-LLM.md` with what each one costs.

## What has been proven, and where

**Verified on real hardware.** `sudo ./dev/verify.sh`, on one machine with a BPF
LSM, cgroup v2 and Btrfs. The most recent run, on **2026-08-25**, reported
**156 proven, 2 not proven, 0 failed**. The whole kernel side is proven: the LSM
denies for real, a module runs confined, Thalyx attaches its own LSM with no
`bpftool`, the module's channel survives the sandbox, and the mutation ring
buffer is mapped and drained from a real kernel pin — four records read, and a
second read with none of them left.

**The run before it, on 2026-08-24, reported two failures, and they were one
thing seen twice.** Both stages asked whether a confined module may put a thread
of its own on an ordinary scheduling policy, and both asked with `chrt --other`
— which on util-linux 2.41 makes that request through `sched_setattr`, a second
door to the same capability that carries its argument behind a pointer where no
seccomp filter can read it, and which Thalyx denies. The filter was doing
exactly what it was decreed to do; the instrument was measuring the version of
util-linux. The ordinary column now asks with `chrt --idle`, which no version
sends through the closed door, and `--other` stays as a report — `strace` naming
which of the two calls this machine's `chrt` used — so the cost of the closed
door is measured on the machine that pays it rather than assumed.

**A count that moves is not a score.** The run before those reported 134 on a
machine with no kernel or image built (`image/build/` is not in the repository,
so it does not survive a clean), which contracts the thirteen checks of the boot
stage into a single `NOT PROVEN` line. The rule the project took from it is that
a marker and the `NOT PROVEN` lines below it are one result: a count that drops
is not a regression until you know what stopped running, and the number does not
say — the list under it does. `make -C image` brings those thirteen back.

Before that one, the run that reported 143 failed once, and the failure was the
harness rather than Thalyx — the ninth instance of the project's fifth testing
rule. Two tests in `llama.rs` failed with `ETXTBSY`: the kernel refusing to
execute a file another thread still had open for writing, through the window
between `fork` and `exec`. It had failed once a year earlier,
been "fixed" by a guess that said it was a guess, and only became a diagnosis
when a twelve-core machine failed it twice in one run. The retry lives in the
harness and deliberately not in production code, next to a test that reproduces
the refusal on purpose.

Also proven on that hardware, and not reachable from a container at all: the
kernel mounting a Btrfs filesystem Thalyx wrote byte by byte with no
`mkfs.btrfs`; a named attempt over real Btrfs, where abandoning restored a file
and deleted the one made during the attempt, atomically; `cambios` mapping the
watcher's ring buffer through a real kernel pin and reading tens of thousands of
records; the bounded listing with its cursor; and the symbol index finding close
to four thousand declared names over this repository's own sources. The BPF LSM
has denied a real network connection to a process that lacked the permission,
and only to that process.

**Running a program nobody signed — built 2026-08-25, proven in part.**
`ejecutar <ruta>` is stage 36. It confines a foreign binary the way a module is
confined and withholds the two things a signature justifies: there is no channel
to Thalyx's API, so nothing can be installed or granted through this verb, and
there is no unconfined mode, so a machine that cannot enforce refuses instead of
running it with a warning. What has been exercised in a container: the control
column (the same script outside the sandbox reaches both paths, or "it could not
reach them" inside means nothing), the rehearsal, a `n` at the prompt leaving
nothing on disk, the journal recording `run_foreign` and never `run_module`, and
the structured face refusing with `needs_a_human`. What a confined guest can see
needs the LSM attached and is `NOT PROVEN` until the next run on hardware — and
the `NOT PROVEN` line says one true thing besides: the refusal comes from the
core, which is only reached after the `y` was read and accepted.

**Attached is not enforcing — fixed 2026-08-25.** The first real run of
`ejecutar` refused with "the kernel policy map is not loaded", correctly, and
pointed at `make -C lsm load`. That target lands in observe mode deliberately,
so the remedy left the machine in the one state where the verb *would* have
launched a guest under a kernel that denies nothing. The cause was a question
that answers itself: `is_available()` asks whether the policy map opens, and
every caller read it as "the kernel is enforcing". The mode lives in a second
map, `thalyx_enforcing`, which nothing in Rust had ever read — only the Makefile
did, through `bpftool`. A guest is now refused while the kernel only observes; a
module still runs there, but the report warns and the journal records the run as
degraded; `thalyx enforce status` prints the mode; and stage 36 switches
enforcement on for its own run and puts it back. No test caught this because the
fake had no state for it: `MemoryStore` could not represent "loaded and denying
nothing", so neither a test nor its control could name the failure. It can now.

**And on 2026-08-26 Thalyx learned to change that mode itself.** Reading it was
half the hole. Switching it was still `make -C lsm enforce` — `bpftool`, which
the image does not carry and is never going to — so inside a running Thalyx
every refusal whose remedy was "make it binding" named a command that does not
exist there. The verbs are `negar` and `observar` at a prompt, and
`thalyx enforce mode <enforcing|observing>` on a machine with a shell: four
bytes written into `thalyx_enforcing` with `bpf(2)`, then read back, because
`bpf_obj_get` succeeds on any map and a write into the wrong one would report a
machine that had started denying. Two verbs rather than one with an argument, so
a typo cannot disarm the machine, and only the direction that *takes protection
away* asks a human — the structured face is refused outright there, the way it
is for `ejecutar`. Stage 37 measures all of it with `bpftool` rather than with
Thalyx, which is the harness rule: asking Thalyx whether its own four bytes
landed would pass on a build where the read and the write are wrong in the same
direction.

**`ensayo` now reaches every verb that changes the machine.** `correr` was the
last one that answered that it could not be rehearsed, and the reason was the
same missing piece: what a run would be allowed to do is a question for the
kernel side. `thalyx_core::foresee_run` is the run's own code stopped one line
before the program exists — not a second implementation of it — so it reports
the program, the isolation, the permissions **in force**, whether the run would
start at all, and whether it would be degraded. `editar` was closed the same
day and by the same shape: the edit applies in memory and then saves, so the
rehearsal is that path with the save left out. Stage 38 asks the one question a
container cannot: what the rehearsal says on a machine that can really enforce,
denying and observing, which are the two answers that look identical if the
degraded flag is wrong.

**Still unproven, and named as such by the run itself.** `verify.sh`'s summary
is the authority on this, not a count written down here: whatever it could not
exercise on the day it ran comes back as `NOT PROVEN`, with the reason, and is
listed again at the end. The development container is the extreme case — it has
no BPF LSM, no delegated cgroup controllers and no Btrfs, so most of the run is
`NOT PROVEN` there, and that is the point of the distinction rather than a
failure of it.

**The image boots, and does the six steps by itself.** A kernel built from
`allnoconfig` comes up in QEMU with one program inside it, attaches its own
enforcement with no `bpftool` and no second file, mounts its Btrfs store,
installs a signed module from its own repository through the trusted path, runs
it confined, reverts it, powers itself off — and on the next boot says what it
was asked to do and that the install it made no longer checks out. That whole
sequence is stage 16 of `verify.sh`, typed into a real machine from a cold
boot.

## Current limits and coverage gaps

- **Physical boot and installation are proven.** On 2026-08-07 a PC booted
  Thalyx from USB through its own firmware, used HDMI and a real xHCI keyboard,
  listed the machine's disks, installed itself onto a second disk, and booted
  again without the installation medium. That closed Phase 1.
- **Two hardware configurations remain unexercised.** The completed installation
  targeted removable media, so an internal disk has not received Thalyx, and the
  machine had no NVMe device. Those are coverage gaps, not failed checks.

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
- **The conversational agent has a model, and three of its four tiers are
  measured.** `vault/02-Arquitectura/Gamas-de-Modelo.md` decrees four sizes of
  one family; `thalyx agent model use <tier> --weights <gguf>` selects one and
  llama.cpp is invoked as a process. On 2026-08-08 the light, medium and high
  tiers ran the twenty-case bench on one machine — a Ryzen 5 5600G with 16 GB
  and no GPU. **The largest tier is not measured**: the process was killed for
  running out of memory before its first inference finished, which is recorded
  as no measurement rather than as a score of zero. A machine with no model
  configured still says *"I have no model loaded"* and still does everything the
  rules can do, which is the double route being real rather than polite.
- **No tier abstained even once.** Across all three measured tiers, every
  ambiguous utterance produced a module id instead of a request for
  clarification. The decree calls abstention the most important measurement, so
  this is the largest open result in the project — and the grammar does not help
  with it: it constrains the *shape* of an answer, never its truth.
- **The predictive scheduler is Phase 2.** It is design, not code.
- **`thalyx_watch` has never been loaded by Thalyx's own loader.** The loader is
  proven on the LSM object — it loads it, attaches it, and that enforcement
  denies — and the watcher's ring buffer has been read on hardware through a real
  kernel pin. But the watcher itself is still loaded with `bpftool`: ten hooks
  instead of two, and one map type the LSM does not use. It is *likely* the same
  loader works, and likely is not proven.

## Three limits of the enforcement, stated rather than discovered

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

## And one contradiction, since it is the honest thing to publish

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
