# Working on Thalyx

Read this before anything else. Then read
`vault/06-Pendientes/Punto-Actual.md`, which says where the project is right
now and what the next step is.

**If Cesar opens with the output of a command and no explanation**, it is from
`vault/09-Notas-Tecnicas/Primer-Arranque.md` — the first-boot procedure. That
note has every command, what each should print, and what each failure means. It
is written to be answered from itself.

## Who decides

**Cesar Manzo decides. You build.** That division is not a courtesy, it is the
arrangement:

- **Nothing happens without his approval**, however well justified it looks.
  Never change one of his decisions without asking him to approve the change
  first.
- **He never writes code.** You write all of it. He does not need to be
  familiar with syntax — he needs to be familiar with the *concepts that
  matter*. Explain what a thing does and what it costs, not how it is spelled.
- **He is the one who runs anything that needs real hardware.** This container
  has no BPF, no cgroup delegation, no Btrfs. See "The verification loop".

### How to ask him things

**Always with `AskUserQuestion`, with clickable options.** Never a question in
prose that he has to answer by typing an essay. If you need a decision, give
him the options and a recommendation; if you find yourself writing "what do you
think about X?" in a paragraph, stop and turn it into options.

If a question can be answered by reading the vault or the code, it is not a
question for him.

### And ask him less than feels safe

`vault/05-Decisiones-y-Debates/Ritmo-de-Construccion.md`, decreed 2026-08-25
after he pointed out that the project had spent days polishing and had not moved
the bar in `Filosofia-Fundacional.md`. **A question costs his time and stops the
build until he answers**, so it is spent only on what nobody else can answer:
changing one of his decrees, writing where something of his can be lost,
spending his hardware or his money, or scope the vault does not cover.

Everything else — the order of two decided things, a stale paragraph, a name, a
pendiente already written in `Tareas-Pendientes.md` — **gets done, and he gets
told what was done.** A pendiente already written there was already decided by
him; asking again is asking him to decide twice.

**A menu whose options are all cheap and already decided is a forbidden
question** — revised 2026-08-26, after he was offered exactly that the day
after the decree. Having a recommendation does not save it: the recommendation
was the thing to have done instead of asking. If none of the options you are
about to write down is one only he can answer, there is no question — there is
work.

**And cheap work does not ship one piece at a time.** Everything cheap that
does not need him goes in **one sprint**, together, each piece arriving with
the tests or the tool that shows it came out right. A whole sprint spent on one
simple thing is the failure this decree exists to stop, and asking permission
first makes it worse rather than better.

None of that lowers a rule below. Fast is delivering the whole thing; fast is
not delivering the easy half, and `NOT PROVEN` is still `NOT PROVEN`.

## Language

- **Spanish** — neutral Mexican, **never voseo** — for conversation with him
  and for the entire vault.
- **English** for absolutely everything else: code, comments, identifiers,
  schemas, commit messages, CLI output, file names, test names.

Two checks, both of which must come back empty:

```sh
# No voseo anywhere in the vault.
grep -rniE "\b(podés|tenés|querés|hacés|sabés|fijate|mirá|dejá|sos)\b" vault/

# Every wikilink resolves. Skips code fences (TOML's [[table]] is not a link)
# and handles the escaped pipe a link needs inside a markdown table.
python3 - <<'EOF'
import pathlib, re
notes = {p.stem for p in pathlib.Path("vault").rglob("*.md")}
for path in pathlib.Path("vault").rglob("*.md"):
    if path.parent.name == "Plantillas":
        continue          # templates carry placeholder links on purpose
    fenced = False
    for line in path.read_text().splitlines():
        if line.lstrip().startswith("```"):
            fenced = not fenced
            continue
        if fenced:
            continue
        for link in re.findall(r"\[\[([^\]]+)\]\]", line):
            target = link.split("|")[0].split("\\")[0].split("#")[0].strip()
            target = target.split("/")[-1]
            if target and target not in notes:
                print(f"{path}: [[{target}]] does not exist")
EOF
```

## What this project is

Read this first, in `vault/01-Filosofia/Filosofia-Fundacional.md`, where it is
kept verbatim in Cesar's words:

> **Thalyx es el sistema operativo.** El kernel de Linux es un componente que
> Thalyx gestiona, no el anfitrión sobre el que descansa. No hay capas
> intermedias, no hay distribuciones — no hay nada que no sea Thalyx. Los
> módulos y el agente se comunican exclusivamente a través de la API de Thalyx,
> no a través de POSIX, no a través de libc, no a través de scripts de shell.
> […] Si Linux desaparece, Thalyx encuentra otro motor. Si Thalyx desaparece, no
> hay sistema. **Thalyx es el todo. Sin Thalyx no hay nada.**

**Anything in this repository that contradicts that text is wrong**, however
confidently it is written and whenever it was written. Three decrees contradicted
it for three days without anyone noticing, and the way it was caught was somebody
asking why there would be a login at boot that nobody had built.

The practical form of it: the image carries the Linux kernel and one program.
That is checkable rather than quotable — `make -C image count`, and it says one
or it says the decree is broken.

An operating system where AI is a first-class citizen rather than an
application, and the human stays sovereign. The design lives in an Obsidian
vault **inside this repository** (`vault/`). Do not reorganise it.

The vault is the authority. Code implements decrees; it does not invent them.
When you implement something, the module documentation should say which vault
note it comes from, and when building teaches something the decree did not
anticipate, **write it back into the vault as a revision** — that is how the
design stays true rather than becoming a story about an older version of
itself.

A decision that is not in the vault has not been made.

## The rules that produced this codebase

These were all learned by something going wrong. They are recorded in
`vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md` and they are not optional.

1. **Every real defect came from running the system**, not from reading it. A
   test that something was produced correctly is not a test that it works —
   installed modules were unexecutable for weeks while every test passed.
2. **Asking the system whether it worked proves nothing.** Isolation tests ask
   the *confined program* what it can see, with an "outside" control column.
3. **A test that skips must say it skipped**, print `NOT PROVEN`, and there is
   one environment variable **per requirement** that turns the skip into a
   failure. One variable for four capabilities means the only way to demand
   what a machine has is to demand what it has not.
4. **Every denial test needs a baseline and a control.** Without the first, a
   denial and an operation that never worked look identical. Without the
   second, a policy that breaks everything looks like one that works.
5. **The instrument includes the harness.** Before believing something Thalyx
   claims is false, rule out that the thing that asked got it wrong. This has
   now happened fourteen times: `curl -s`, bpffs permissions, a `pipefail`
   pipeline, an unprepared cgroup arena, a test that inferred its own
   precondition, a stale local `main` read as the state of the repository, a
   test suite that raced with itself for an executable it had just written, and
   — twice, for the same reason — a parser tested only against fixtures its
   author invented. The second of that pair accused llama.cpp of ignoring a
   grammar it had just obeyed, because every fixture agreed with the parser
   about where an answer stops. The stale `main` is the cheapest of them, and it
   came back on 2026-08-26 because the rule was written short: `main` and
   `origin/main` are different questions, **and `origin/main` is only a
   question about the repository after a `fetch`** — before that it is a
   question about the last time this machine looked. Reading it unfetched
   produced a whole diagnosis ("G1 is not on `main`") that was false end to
   end. The ninth is the one that
   took a year: it failed once in twenty-five runs, was "fixed" by a guess that
   said it was a guess, and only became a diagnosis when a twelve-core machine
   failed it twice in one run and the error — `ETXTBSY` — was finally captured.
   The tenth and eleventh are both a set read from the wrong place: `verify.sh`
   grepping for a sentence the probe had stopped printing, which turned seven
   denials into seven vacuous passes and was caught only by the positive
   control beside them; and `dev/foreign-agent-needs.sh` taking the permitted
   syscalls out of a file that also names the forbidden ones. The twelfth is the
   sharpest: a test that asked whether a module may arrange its own threads by
   running `chrt --other`, which makes the guarded call on util-linux 2.40 and a
   denied one on 2.41 — so it passed in the container and failed on his machine,
   having measured util-linux rather than the filter.
6. **A parser for another tool's output needs one captured real sample,
   verbatim.** A hand-written fixture proves the parser matches your model of
   the format, not the format.
7. **A one-sided measurement does not need a quiet machine.** Pick the
   threshold so the direction ambient noise cannot reach is the one that
   answers the question.
8. **A fake must model the property under test.** A fake that fails it is not a
   fake, it is a different system.
9. **Fail closed.** A corrupt field, an unreadable map, a value written by a
   version that does not exist yet — all of them must produce the cautious
   answer, never the fast one.
10. **A failure to read is not a failure to exist.** Say which one happened.
11. **A test that writes something machine-global has changed the machine it
    was measuring.** `THALYX_ROOT` isolates the store and nothing else. On
    2026-08-27 three tests in `the_guard_can_be_switched.rs` typed `negar` at a
    real prompt, armed Cesar's kernel, and every stage of `verify.sh` after
    them measured a machine nobody had asked for. What distinguishes the case
    is not "it touches the machine" — a cgroup is made and removed and has an
    owner — but **a global switch with no owner**, whose value is some other
    check's precondition. Such a test asks first and skips with `NOT PROVEN`.
    And the danger is not "a test about the switch": the next culprit was
    `catalogue_is_true.rs`, which types every verb the machine advertises and
    had no idea one of them was that one. The precondition lives in
    `tests/machine_guard/mod.rs` so that a file which does not know can use it.
    On 2026-08-28 the same rule turned up somewhere nobody had looked:
    **descriptors 0, 1 and 2 belong to the process**, and `cargo test` runs one
    binary's tests as threads inside one process — so the capture that lets the
    screen show a verb's output caught `libtest`'s progress lines instead of its
    own, while passing alone and passing with `--test-threads=1`. What has no
    owner is not isolated by an environment variable; the only real separation is
    a separate process, which in Rust means a separate crate — hence
    `thalyx-capture`.

12. **The binary that gets verified has to be the binary that ships.** `verify.sh`
    compiles against glibc from end to end, and the image is a static musl
    build; the stage called "the image" packs the *glibc* binary to count the
    programs inside it. So five ioctl requests cast to `libc::c_ulong` — which
    is what glibc's `ioctl` takes, where musl's takes `c_int` — went through 189
    checks clean on 2026-08-28 and then stopped `make -C image` dead, with the
    person who runs the hardware finding it. It is rule 8 pointed at the
    compiler: a build with a different configuration is a different system.
    Stage 2 now runs the image Makefile's own build line, and
    `THALYX_REQUIRE_IMAGE_BUILD=1` turns its skips into failures.

## How to write code here

- **Rust 2024, cargo workspace, `clippy` with warnings denied, `cargo fmt`.**
- **`unsafe` lives in `thalyx-syscall` and nowhere else.** Every other crate
  sets `unsafe_code = "forbid"`. Inside that crate each block carries
  `#[allow(unsafe_code)]` and a SAFETY comment.
- **Comments explain why, and name the failure they prevent.** Not what the
  line does. If a comment could be deleted without losing a reason, delete it.
- **Match the surrounding code.** This codebase has a voice: long explanatory
  module docs, test names that are sentences, comments that name the mistake
  that was almost made.
- **Tests are named as claims.** `a_counter_that_moved_falls_back_to_the_walk`,
  not `test_counter_2`.
- Prefer shelling out to `bpftool` / `btrfs` over linking a library: no
  build-time dependency on kernel headers, and every step can be reproduced by
  hand while debugging.

## Commits and branches

- Conventional-commit prefixes (`feat:`, `fix:`, `docs:`), then a body that
  explains **why**, including what went wrong and what it would have cost.
  Look at `git log` — the bodies are the record of the reasoning.
- **Never commit directly to `main`.** Every piece of work starts on its own
  branch, and `main` only ever receives finished work.
- **Branch names are `type/description`**, with the same types the commits use:
  `feat/`, `fix/`, `docs/`, `build/`, `test/`. The description says what the
  branch is *about*, in English, in words — `feat/installer-and-drivers`, not
  `claude/whatever-7tfum1`. A branch whose name says nothing about itself is not
  acceptable.
- **When he needs to run something, the work goes to `main` first.** He
  verifies on his machine with `git pull`, and `git pull` on `main` is what he
  runs — so before telling him to pull, merge the branch into `main` and push
  both. Do not leave him to find the work on a branch he was never told about.
- **Never open a pull request unless he asks for one.**

## The verification loop

This container cannot check most of what Thalyx claims: no BPF LSM, no
delegated cgroup controllers, no Btrfs, no `bpftool`. His machine (Fedora 43,
kernel 7.0, Btrfs, `bpf` in the LSM order) can check all of it.

```
git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
```

That script is the contract between what you write and what is true. **Every
new claim gets a stage in it.** It reports `PROVEN` / `NOT PROVEN` / `FAILED`
and never counts a check it could not make as a pass.

When you write something that only his hardware can exercise — anything in
`lsm/`, anything touching Btrfs — say so plainly, tell him what to run, and do
not stack a second unverified change on top of the first. If the first one
breaks, he needs to know which one it was.

Before writing to `lsm/`, remember that a BPF program that fails the verifier
takes the whole watcher down, visibly but completely. `make -C lsm diagnose`
shows what libbpf actually said.

## Keeping this from going stale

**Update `vault/06-Pendientes/Punto-Actual.md` every time you finish
something.** It exists so a new session never has to be told what happened, and
so nothing important lives only in a conversation that ended.

Also update, when they change:
- `vault/09-Notas-Tecnicas/Estado-de-Implementacion.md` — what is built.
- `vault/06-Pendientes/Tareas-Pendientes.md` — what is decided and what is not.
- `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md` — when a new class of
  mistake is found, write the rule that would have caught it.
- `README.md` — when the status paragraph stops being true.
