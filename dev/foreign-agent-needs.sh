#!/usr/bin/env bash
#
# What a foreign agent needs in order to start, measured rather than guessed.
#
# `vault/06-Pendientes/Tareas-Pendientes.md` has carried this since the decree
# of 2026-08-09 that the bar is Claude Code and any other already-written agent
# running on Thalyx better than on Linux:
#
#     Averiguar qué necesita exactamente un agente ajeno para arrancar. No por
#     suposición: tomar Claude Code, mirar qué llama, y hacer la lista. Es
#     barato y no se ha hecho, y sin ella todo lo de abajo es adivinado.
#
# This is that list, and it is a script rather than a paragraph for the reason
# `Estrategia-de-Pruebas.md` gives: a procedure printed for a person is code
# that never runs, so it rots without anyone finding out. Run it again on any
# machine and it answers again.
#
# ## What it measures, and what it deliberately does not
#
# It traces the agent **starting** — `--version`, which loads the runtime and
# exits. That is the right question and a narrow one: `Superficie-para-el-LLM.md`
# says a foreign agent would not start today, and starting is what this settles.
#
# It does **not** measure the agent working. No network, no terminal, no
# subprocesses, no files written. Those are more syscalls and more paths, and
# claiming this list covers them would be exactly the assumption the pending
# item exists to stop.
#
# ## Reading the output
#
# The syscall column is compared against `module_standard`'s allowlist, which is
# the filter a module actually gets. The path column is compared against
# `rootfs::SYSTEM_PATHS` and `DEVICE_NODES`, which is what a module actually
# sees. Anything the agent opened that is under neither is named.
#
# A path that was opened is not necessarily a path that is needed: a program
# that reads `/sys/…/transparent_hugepage/enabled` may well carry on without it.
# This script says what happened, not what is required, and it says which of the
# two it is saying.

set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(dirname "$HERE")"
WORK="${TMPDIR:-/tmp}/thalyx-foreign-agent.$$"
mkdir -p "$WORK"
trap 'rm -rf "$WORK"' EXIT

AGENT="${THALYX_FOREIGN_AGENT:-$(command -v claude || true)}"
ARGUMENT="${THALYX_FOREIGN_AGENT_ARG:---version}"

say() { printf '  %s\n' "$*"; }
# Not named `head`: defining a function with the name of a coreutil shadows it
# for the rest of the script, and the first thing this one did was shadow the
# `head -1` two lines below its own use.
rule() { printf '\n%s\n' "$*"; printf '%s\n' "$(printf '─%.0s' $(seq 1 ${#1}))"; }

if [ -z "$AGENT" ]; then
    say "NOT MEASURED  no foreign agent found."
    say "              Set THALYX_FOREIGN_AGENT to one, or install Claude Code."
    exit 0
fi
if ! command -v strace > /dev/null 2>&1; then
    say "NOT MEASURED  strace is not here, and there is no second way to ask."
    exit 0
fi

rule "the agent"
say "$AGENT $ARGUMENT"
"$AGENT" $ARGUMENT 2>&1 | head -1 | sed 's/^/  /'

# `-f` because the runtime is threaded before it does anything interesting, and
# a trace of the first thread alone would report a fraction of the truth.
if ! strace -f -qq -o "$WORK/trace" "$AGENT" $ARGUMENT > /dev/null 2>&1; then
    say "the agent did not start under strace; nothing was measured"
    exit 1
fi

python3 - "$WORK/trace" "$REPO" <<'PY'
import re, sys, collections, pathlib

trace, repo = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])

calls = collections.Counter()
opens = {}
call_line = re.compile(r"^\s*\d+\s+(?:<\.\.\. )?([a-z_0-9]+)[( ]")
open_line = re.compile(r'\bopenat\(AT_FDCWD, "([^"]+)"|\bopen\("([^"]+)"')

for line in trace.open(errors="replace"):
    found = call_line.match(line)
    if found:
        calls[found.group(1)] += 1
    where = open_line.search(line)
    if where:
        path = where.group(1) or where.group(2)
        # Whether it got the file matters more than that it asked: a program
        # that carried on after ENOENT has told us the path is optional, and
        # that is the one thing a trace can settle about need.
        opens[path] = "ENOENT" not in line.split("=")[-1]

# The filter a module actually gets.
seccomp = (repo / "crates/thalyx-sandbox/src/seccomp.rs").read_text()
allowed = set(re.findall(r"libc::SYS_([a-z_0-9]+)", seccomp))

# A guarded syscall is allowed for some of its arguments and killed for the
# rest, so counting it as plainly allowed would report a condition as a
# permission — the same collapse rule 10 forbids between "absent" and
# "unreadable". They come out of the allowed set and get their own line.
guarded = set(re.findall(r"syscall: libc::SYS_([a-z_0-9]+)", seccomp))
allowed -= guarded

# What a module actually sees.
rootfs = (repo / "crates/thalyx-sandbox/src/rootfs.rs").read_text()
system = re.search(r"SYSTEM_PATHS: \[&str; \d+\] = \[([^\]]*)\]", rootfs)
devices = re.search(r"DEVICE_NODES: \[&str; \d+\] = \[([^\]]*)\]", rootfs)
system = tuple(re.findall(r'"([^"]+)"', system.group(1))) if system else ()
devices = set(re.findall(r'"([^"]+)"', devices.group(1))) if devices else set()

def rule(title):
    print(f"\n{title}")
    print("─" * len(title))

rule("syscalls, against module_standard's filter")
missing = sorted(set(calls) - allowed - guarded)
conditional = sorted(set(calls) & guarded)
print(f"  {len(calls)} distinct to start; {len(calls) - len(missing)} allowed")
if conditional:
    print(f"  {len(conditional)} of those only for some of their arguments:")
    for name in conditional:
        print(f"      {name}   ({calls[name]}x)   guarded, not plainly allowed")
    print("      this script does not check the arguments; the guard's own")
    print("      tests do, and dev/verify.sh asks a confined module")
if missing:
    print(f"  {len(missing)} not allowed at all:")
    for name in missing:
        print(f"      {name}   ({calls[name]}x)")
else:
    print("  none are missing outright")

rule("paths, against what a module's root holds")
# `/proc` is mounted into the sandbox after the pivot, so it is present even
# though it is not in SYSTEM_PATHS.
outside = []
for path, got_it in sorted(opens.items()):
    if path.startswith(system) or path in devices or path.startswith("/proc"):
        continue
    outside.append((path, got_it))

print(f"  {len(opens)} paths opened; {len(opens) - len(outside)} are inside what a module gets")
if outside:
    print(f"  {len(outside)} are not:")
    for path, got_it in outside:
        mark = "had it here" if got_it else "ENOENT, and it started anyway"
        print(f"      {path}")
        print(f"          {mark}")

rule("what this does not answer")
print("  - the agent working: no network, no terminal, no subprocess, no writes")
print("  - whether a path it read here is a path it needs")
print("  - anything the agent shells out to (a shell, git, a search tool)")
PY
