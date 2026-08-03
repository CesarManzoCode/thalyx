# Working on Thalyx

Read this before anything else. Then read
`vault/06-Pendientes/Punto-Actual.md`, which says where the project is right
now and what the next step is.

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
   now happened six times: `curl -s`, bpffs permissions, a `pipefail` pipeline,
   an unprepared cgroup arena, a test that inferred its own precondition, and a
   parser tested only against fixtures its author invented.
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
- Branch names must say what they are. A branch called `claude/...` says
  nothing about itself and is not acceptable. Work goes on `main` unless he
  says otherwise.
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
