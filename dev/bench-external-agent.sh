#!/usr/bin/env bash
# The same agent, the same task, two machines. Which one works better?
#
#   dev/bench-external-agent.sh --project <dir> [--socket <sock>]
#                               [--task read|change|reversible]
#                               [--workspace <dir>]
#
# `vault/07-Adopcion-y-Fases/Agentes-Externos.md`. The whole of Thalyx rests on a
# claim nobody has measured: that an operating system built around structured
# answers, a semantic index and a reversible boundary makes an AI agent better at
# its job than a pile of POSIX tools does. This is the harness that will
# eventually say whether that is true.
#
# ## What it is not
#
# **It is not a result.** One run of one task is an anecdote, and two arms that
# differ in a dozen uncontrolled ways are not a comparison. What exists here is
# the harness and the discipline: the same prompt, the same model, the same turn
# limit, the same starting state, and every number written down as it was
# observed. Reading a conclusion out of one pass would be exactly the kind of
# confident wrongness `Estrategia-de-Pruebas.md` was written against.
#
# ## The two arms
#
#   A  Claude Code with the tools it always has — Read, Edit, Grep, Glob, Bash —
#      on an ordinary copy of the project, on this host's filesystem.
#
#   B  The same Claude Code with **only** the Thalyx MCP tools, on a byte-identical
#      copy that lives inside a Thalyx machine.
#
# Arm B has its ordinary tools taken away on purpose. Left in, the model reaches
# for what it has used a billion times, calls `thalyx_state` once for politeness
# and does the work with `grep` — and the run measures nothing at all. Neither
# arm gets `--dangerously-skip-permissions`: the tools each arm may use are named,
# which is a control and not a shortcut.
#
# ## Where the numbers come from
#
#   - Claude Code's own `--output-format stream-json`, kept whole on disk. The
#     final `result` event carries turns, wall time, cost and token usage; the
#     `tool_use` and `tool_result` blocks carry every call the model made, by
#     name, and how many bytes each handed back. That last part is the reason
#     for the stream: it makes **both** arms measurable in the same units, where
#     before only arm B could be counted at all.
#     Nothing is estimated. A field the agent did not print is absent from the
#     summary, never zero. `dev/bench-summary.py` is the parser, and it is a
#     separate file so it can be checked against a captured real session
#     (`dev/samples/`) without spending a run — rule 6.
#   - `thalyx-mcp --metrics`, for arm B, kept *beside* the stream numbers rather
#     than merged with them. Arm B is therefore measured twice by two
#     independent instruments, and rule 5 says that is the point: if they
#     disagree, one is wrong, and an average would hide it.
#   - The workspace itself, hashed before and after, which is how the `change`
#     task's claim — that abandoning puts everything back exactly — is checked by
#     something other than the machine that made the claim.
#   - Whether the task was *done*, when the caller says what done means:
#     `--expect-file` is a list of strings the final answer has to contain,
#     written by hand from the project's ground truth. Without it the summary
#     reports no verdict rather than a guessed one — an agent that answered
#     confidently and wrongly must not score as a success.
#
# ## The `reversible` task, and the question it exists to ask
#
# The first three real runs said: on reading, Thalyx wins by a lot (-46% and
# -62% cost); on editing one file, the two arms are level (-4% cost, +24% wall)
# and arm B never opened an attempt. That is not a surprise — one file changed
# once is a task with nothing to reverse, so it cannot measure a reversible
# boundary. It measures an editor.
#
# `--task reversible` is the task that does exercise it: change one symbol in
# its definition and in everything that depends on it, find out what that
# touched, and then put the whole tree back **exactly**. The question is the one
# in `vault/07-Adopcion-y-Fases/Agentes-Externos.md`: when a job means changing
# several related places and then returning to the starting state with
# certainty, does the boundary cost the same agent less work than Linux does?
#
# Three things keep it from being a rigged question:
#
#   - **The prompt is one string.** There is exactly one `claude -p` in this
#     file and both arms are given the same variable. It names no tool, no MCP
#     server and not Thalyx. `--self-test` checks both of those by reading this
#     file, so the claim cannot rot.
#   - **Arm A may restore however it likes.** The copy it works in carries the
#     project's `.git`, and arm A has `Bash`; `git checkout -- .` is a perfectly
#     good answer and if it is the cheaper one, that is the result. The only
#     thing the prompt forbids is building and testing, which it forbids in both
#     arms — arm B has no shell and could not build if it wanted to, so leaving
#     it in would have measured `cargo` in one column and nothing in the other.
#   - **"Restored" is checked from outside.** Not by asking either machine: by
#     hashing the bytes. For arm A that is this host's copy, before and after.
#     For arm B the workspace is inside the VM, so it takes an export
#     (`make -C image agent-export`) and therefore a second pass — and until
#     that pass happens the summary says `not_proven` rather than assuming.
#     `THALYX_REQUIRE_RESTORE_CHECK=1` turns that skip into a failure, which is
#     rule 3: one variable per requirement.
#
# And the trap this task walks straight into, which the summary is built to
# catch: **an agent that does nothing restores the tree perfectly.** A verdict
# read off the hash alone would score a refusal as a triumph. So the summary
# reports, from each arm's own stream, whether the new name ever appeared in a
# tool call at all, and `reversible.passed` is a conjunction — it really changed
# things, the tree came back, and the answer named the files the ground truth
# says it had to.
#
# ## What the first real reversible run cost, and it was not an agent
#
# 2026-08-29. It ran, both arms answered correctly, and the summary failed both
# of them for reasons that were not the agent's. Two faults, both in the grader,
# both found by reading it against numbers the run had already printed:
#
#   - **The workspace boundary did not include `image/build`.** Arm B's only
#     reported difference between the tree it started from and the tree that came
#     back was `image/build/agent.sock` — the socket QEMU opens for the agent
#     channel. It is on this host because the benchmark is running and it is not
#     on the store because it did not exist when the project was staged. No agent
#     could have made it and none could have removed it. The machinery that
#     carries a measurement is not the thing measured, and the list of what is
#     machinery now lives in one place in `dev/bench-summary.py` — with what it
#     sets aside **reported** rather than dropped, so an exclusion cannot become
#     a hiding place.
#
#   - **The witness of the intermediate state was the one thing a correct answer
#     erases.** It was the mtimes, and step five of the task is *put everything
#     back exactly*. An agent that restores from a `cp -a` copy puts the contents
#     back and the mtimes back; arm A made six `Edit` calls and was recorded as a
#     workspace nothing had ever happened to. There are three witnesses now, with
#     different weaknesses on purpose: the `ctime`, which nothing in userspace can
#     set backwards; the answering tool's own `tool_result`, which is already
#     written in the stream and which no later restore reaches; and the adapter's
#     count for arm B. And four fields where there were two — what the model
#     asked for, what the tool answered, what an instrument outside the agent
#     saw, and how the tree ended up.
#
# Because the manifest lines are kept beside the digest, a run graded under the
# old boundary can be graded again under the new one without being run again:
#
#     dev/bench-external-agent.sh --task reversible --symbol <Name> \
#         --expect-file dev/bench-expect/<file>.txt --out <dir> --regrade
#
# writes `summary-regraded.json` and never touches `summary.json`, and
# `--forensics` prints every mutating call in each arm with the answer its tool
# gave it — which is the table that says whether six `Edit` calls edited
# anything.
#
# ## What that same run turned out not to have been, and it was worse
#
# A later reading of the forensics found arm A running commands like
#
#     cd /home/cesarmanzocode/thalyx
#
# while the harness had been given `--project /tmp/bench-thalyx`. The cause is
# one default in this file: `--out` defaults to `$ROOT/target/bench-external-agent`,
# `$ROOT` is the checkout this script lives in, and arm A's copy was made at
# `$OUT/a`. So `claude` was started **inside Cesar's own working clone of
# Thalyx**, and Claude Code collects `CLAUDE.md` from every ancestor of its
# working directory — this project's, which opens "read this before anything
# else" and names `vault/06-Pendientes/Punto-Actual.md`. Arm A began the task
# holding instructions about `~/thalyx` and went and worked in `~/thalyx`.
# **Passing a directory as the working directory is not a boundary**, and this
# harness had nothing else.
#
# The same reading found two more:
#
#   - the forensic table printed a `Bash` whose command was `git checkout -- …`
#     as `write=False`, because the classification was two-valued and a tool
#     name is a statement of intent, never evidence of an effect;
#   - arm B produced `0s wall, 0 stream events` **after** arm A had been paid
#     for in full, because the only check on arm B was `[ -S "$SOCKET" ]` —
#     which asks whether a file exists, and QEMU creates that file whether or
#     not anything inside the guest ever answers.
#
# ## What is checked before a cent is spent, and in this order
#
#   1. **arm B is alive.** `thalyx-mcp --preflight` over the real channel: the
#      hello, a `where`, and a `list .` whose entries are compared against
#      `--project`. Both verbs are reads, so the probe cannot disturb the
#      starting state it is clearing. Not READY stops the run *here*, with no
#      agent called in either arm.
#   2. **the two arms were given the same tree.** Arm A's copy and the stamp
#      `image/Makefile`'s `project-stage` wrote when it imported the project are
#      hashed by the same program, `bench-summary.py --import-stamp`, and
#      `provenance.json` carries both plus the source commit, the exclusions and
#      each arm's working directory. Different trees stop the run.
#   3. **arm A is anchored.** Its workspace is staged outside this checkout
#      (`--workspace`, default `$TMPDIR/thalyx-bench-arm-a`), every ancestor of
#      it is checked for a `CLAUDE.md`, a `.claude/`, a `.mcp.json` or a `.git`,
#      the process is started physically inside it, and a `PreToolUse` hook
#      refuses any call naming a path outside it.
#   4. **arm A stayed there.** Read back afterwards from the stream's own
#      `system init` event and every path in every `tool_use` block. A stray
#      call makes the run INVALID — and it is checked *between* the arms, which
#      is the last moment at which knowing costs less than arm B does.
#
# The four are separate on purpose. The first three are things that can be made
# true; the fourth is the only one that is *evidence*, because it needs nothing
# to have worked.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT=""
SOCKET="$ROOT/image/build/agent.sock"
TASK="read"
MODEL="${THALYX_BENCH_MODEL:-sonnet}"
TURNS="${THALYX_BENCH_TURNS:-30}"
OUT="${THALYX_BENCH_OUT:-$ROOT/target/bench-external-agent}"
SYMBOL="${THALYX_BENCH_SYMBOL:-}"
ARMS="AB"
EXPECT="${THALYX_BENCH_EXPECT:-}"
# The name the `reversible` task renames the symbol to. A suffix and not a new
# name, because the change has to be mechanical: there is no judgement in
# appending four letters, and `FooRenamed` contains `Foo`, so nothing that
# mentions the new name can be mistaken for something that mentions the old one.
MARKER="${THALYX_BENCH_MARKER:-}"
# Arm B's workspace after the run, exported off the store. See `restore_note`.
RESTORED_B=""
SELFTEST=""
# Where arm A's copy of the project is staged, and it is **not** under `--out`.
#
# That default is the whole of the 2026-08-29 fault. `--out` defaults to
# `$ROOT/target/bench-external-agent`, `$ROOT` is the checkout this script lives
# in, and arm A's copy used to be made at `$OUT/a` — so `claude` was started
# inside Cesar's own working clone of Thalyx. Claude Code collects `CLAUDE.md`
# from every ancestor of its working directory, and this project's opens with
# "read this before anything else"; the agent was handed instructions about
# `~/thalyx` and went and worked in `~/thalyx`. It was given `--project
# /tmp/bench-thalyx` the whole time.
#
# So the workspace is staged somewhere with nothing above it, `ancestry_check`
# refuses to start if that is not true, and `--out` keeps only the artefacts —
# streams, manifests, summaries — which no agent ever sees.
WORKSPACE_A="${THALYX_BENCH_WORKSPACE:-${TMPDIR:-/tmp}/thalyx-bench-arm-a}"
# Arm B's half of the provenance, written by `image/Makefile`'s `project-stage`
# at the moment it copied the project onto the store. Read and never recomputed:
# nothing on this host can hash a tree inside a live Btrfs image, so a stamp
# written by the importer is the only honest evidence there is.
IMPORT_MARK="${THALYX_BENCH_IMPORT_MARK:-$ROOT/image/build/agent-import.json}"
# Read a run that is already over with today's grader, without running an agent.
# Writes `summary-regraded.json` and never touches `summary.json`: the original
# grader's output is the record of what was believed at the time, and a run whose
# instrument turned out to be wrong is worth more with both readings kept than
# with the wrong one quietly replaced.
REGRADE=""
FORENSICS=""

while [ $# -gt 0 ]; do
    case "$1" in
        --project) PROJECT="$2"; shift 2 ;;
        --socket)  SOCKET="$2";  shift 2 ;;
        --task)    TASK="$2";    shift 2 ;;
        --model)   MODEL="$2";   shift 2 ;;
        --turns)   TURNS="$2";   shift 2 ;;
        --out)     OUT="$2";     shift 2 ;;
        --symbol)  SYMBOL="$2";  shift 2 ;;
        --arms)    ARMS="$2";    shift 2 ;;
        --expect-file) EXPECT="$2"; shift 2 ;;
        --marker)  MARKER="$2";  shift 2 ;;
        --restored-b) RESTORED_B="$2"; shift 2 ;;
        --workspace) WORKSPACE_A="$2"; shift 2 ;;
        --regrade) REGRADE=1; shift ;;
        --forensics) FORENSICS=1; shift ;;
        --self-test) SELFTEST=1; shift ;;
        *) echo "  unknown argument: $1"; exit 1 ;;
    esac
done

say() { printf '  %s\n' "$*"; }

# `--arms none` runs no agent at all and only rebuilds the summary. That is not
# a convenience: arm B's restored tree can only be hashed once the machine is
# down and the workspace has been exported, which is necessarily a second pass
# over the same output directory.
[ "$ARMS" = none ] && ARMS=""

# ── every path this harness was given becomes absolute, once, here ───────────
#
# The run of 2026-08-29 was given `--out target/bench-external-agent-3` and arm
# A came back with **`Settings file not found.`** — because `--settings
# $OUT/armA.settings.json` was handed to a `claude` that this script had
# deliberately started somewhere else. `run_arm` does `cd "$cwd"` before it
# execs, and a relative `--out` means every path derived from it is resolved
# against *the agent's* working directory rather than against the shell's. Arm
# A's is the staged workspace; arm B's is an empty directory inside `$OUT`
# itself. Neither of them is where the caller was standing when they typed it.
#
# So it is fixed in exactly one place. Not at each use — a fix repeated at
# eleven call sites is a fix that will be missing from the twelfth — and not by
# refusing a relative path either, which would be this harness telling its
# caller how to type instead of doing the one thing it needs to do to the
# argument.
#
# `Path.resolve()` and not `pwd -P` because `bench-summary.py` writes the
# provenance with `Path.resolve()`, and the comparison that decides whether arm
# A stayed in its workspace is between a path this file passed and a path that
# file resolved. Two normalisations that disagree about a symlinked `/tmp` is
# rule 5 again: the instrument disagreeing with itself.
absolute() {
    python3 -c 'import pathlib, sys
print(pathlib.Path(sys.argv[1]).expanduser().resolve())' "$1"
}

# The globals, in one function, so the self-test can run it from a directory
# that is not this one and check that the answer does not depend on where it
# was standing.
normalise_paths() {
    OUT="$(absolute "$OUT")"
    WORKSPACE_A="$(absolute "$WORKSPACE_A")"
    SOCKET="$(absolute "$SOCKET")"
    IMPORT_MARK="$(absolute "$IMPORT_MARK")"
    if [ -n "$PROJECT" ]; then
        PROJECT="$(absolute "$PROJECT")"
    fi
    if [ -n "$EXPECT" ]; then
        EXPECT="$(absolute "$EXPECT")"
    fi
    if [ -n "$RESTORED_B" ]; then
        RESTORED_B="$(absolute "$RESTORED_B")"
    fi
}

# Only `OUT`, for the self-test's two-directories check. Its own function
# because a subshell that ran the whole of `normalise_paths` would also resolve
# a socket and an import mark that this particular check has nothing to say
# about, and a check that asserts more than it means is a check that fails for
# reasons nobody wrote down.
normalise_paths_out() {
    absolute "$OUT"
}

normalise_paths

# What a tree is, for the purpose of "put it back exactly".
#
# **There is one implementation and it is in `dev/bench-summary.py`.** Until
# 2026-08-28 there were two: a `find -type f | xargs sha256sum` pipeline here,
# and the summary reasoning about what it produced — two programs that agreed
# by coincidence and could stop agreeing without anything failing. Rule 5, the
# instrument includes the harness.
#
# It also only ever compared contents. `-type f` does not match a symlink, so
# an agent that left `src/lib.rs` as a link to `/etc/passwd` had *deleted a
# file*, and one that left a source file mode 777, or a directory where a file
# was, had restored the tree perfectly as far as that pipeline could see. The
# manifest covers entry type, permission bits, symlink targets, contents,
# existence and absence — which is what the prompt's "byte for byte: no file
# left differing, no file added and no file removed" actually promises.
#
# The exclusions are unchanged and are not a taste: exactly what
# `image/Makefile`'s staging leaves out of the copy it puts on the store
# (`--exclude=./target --exclude=./node_modules`) plus `.git`, which both arms
# carry and which changes for reasons that are not the task — a `git status` in
# arm A rewrites the index. Hashing it would make arm A fail a restore it
# performed correctly, and only arm A, which is the one direction a comparison
# must never be wrong in.
tree_hash() {
    python3 "$ROOT/dev/bench-summary.py" --manifest "$1"
}

# Everything about one tree that a restore check needs, written beside the arm
# it belongs to: the digest, the manifest the digest is of, when each file was
# last written, and what the machinery roots the manifest leaves out are holding.
#
# **The manifest is the record and the digest is a summary of it.** That order
# is not a detail: on 2026-08-29 the exclusion list turned out to be missing
# `image/build`, and because the manifest lines were on disk the run could be
# graded again under the corrected boundary without being run again. A harness
# that had only kept the digest would have had to spend another run to find out
# what it already knew.
#
# The mtimes are one witness of three and the weakest of them, and it is worth
# saying why here as well as in the parser: it answers *is the workspace
# different now*, and the task's last step is **put it back**. An agent that
# restores from a `cp -a` copy restores the mtimes with the bytes and this
# witness sees nothing at all. The other two — a mutating tool's own answer in
# the stream, and `thalyx-mcp --metrics` for arm B — are the ones a correct
# restore cannot erase.
#
# `.setaside` is what makes the boundary honest: it says how many entries each
# machinery root held and a digest of their shapes, on both sides, so setting
# something aside is on the record instead of out of sight.
walk() {
    local tree="$1" into="$2"
    python3 "$ROOT/dev/bench-summary.py" --manifest       "$tree" > "$into"
    python3 "$ROOT/dev/bench-summary.py" --manifest-lines "$tree" > "$into.manifest"
    python3 "$ROOT/dev/bench-summary.py" --mtimes         "$tree" > "$into.mtimes"
    python3 "$ROOT/dev/bench-summary.py" --set-aside      "$tree" > "$into.setaside"
}

# ── where arm A is allowed to be, checked before anything is paid for ────────
#
# Telling a model "work in this directory" is not a control; it is a request.
# What makes arm A's workspace *the* workspace is that there is nothing above it
# saying otherwise, and that is what this checks, before `claude` is started.
#
# `CLAUDE.md` is the one that actually happened. Claude Code walks up from its
# working directory collecting project memory, so a workspace staged inside a
# checkout inherits that checkout's instructions — and the model then behaves
# exactly as those instructions say, in the tree those instructions are about.
# The same goes for `.claude/`, `.mcp.json` and a settings file. A `.git` above
# the workspace is the same failure wearing another hat: `git checkout -- .` and
# `git status`, which arm A is expressly allowed to use, would be about the
# outer repository.
ancestry_check() {
    local where="$1" trouble=0 at
    at="$(cd "$(dirname "$where")" && pwd)"
    while :; do
        for stray in CLAUDE.md CLAUDE.local.md .claude .mcp.json .git; do
            if [ -e "$at/$stray" ]; then
                say "$at/$stray is above the workspace, and Claude Code reads it"
                trouble=$((trouble + 1))
            fi
        done
        [ "$at" = / ] && break
        at="$(dirname "$at")"
    done
    return "$trouble"
}

# ── is arm B alive, before a cent is spent on arm A ──────────────────────────
#
# The run of 2026-08-29 paid for arm A in full and then arm B came back `0s
# wall, 0 stream events`. The only check between the money and that outcome was
# `[ -S "$SOCKET" ]`, which asks whether a *file* exists — and QEMU creates that
# file the instant it starts, whether or not anything inside the guest ever
# answers. So: the real channel, the real adapter, two read-only verbs, and the
# answer compared against `--project`. It costs nothing and it runs first.
#
# `THALYX_BENCH_PREFLIGHT_CMD` replaces the probe with something else, which is
# how the self-test exercises a dead machine, a stale one and a healthy one in a
# container that has no KVM. It is never used by a real run.
# What each arm was given, and whether those two things are the same thing.
#
# Written before either arm runs and checked before either arm runs, because a
# comparison between two different trees is not a comparison and finding that
# out afterwards costs both arms.
provenance_now() {
    python3 "$ROOT/dev/bench-summary.py" --provenance "$OUT/provenance.json" \
        --project "$PROJECT" --workspace "$WORKSPACE_A" --repository "$ROOT" \
        --import-mark "$IMPORT_MARK" --task "$TASK" --symbol "$SYMBOL" \
        --marker "$MARKER" --model "$MODEL" --turns "$TURNS" > /dev/null
}

parity_gate() {
    # Only when both arms are in play. One arm is not a comparison and has
    # nothing to be comparable to.
    [[ "$ARMS" == *A* && "$ARMS" == *B* ]] || return 0
    if ! python3 "$ROOT/dev/bench-summary.py" --check-parity "$OUT/provenance.json" \
            > "$OUT/parity.json" 2>&1; then
        say
        say "the two arms were not given the same tree, so nothing was run:"
        sed 's/^/    /' "$OUT/parity.json"
        say
        say "Re-import the project into the machine and try again:"
        say
        say "    make -C image agent PROJECT=$PROJECT"
        exit 1
    fi
    say "the two arms were staged from the same tree"
}

preflight_b() {
    local report="$OUT/preflight-b.json"
    rm -f "$report" "$OUT/preflight-b.verdict.json"
    say "arm B: asking the machine whether it is there (this costs nothing)"
    if [ -n "${THALYX_BENCH_PREFLIGHT_CMD:-}" ]; then
        # shellcheck disable=SC2086
        $THALYX_BENCH_PREFLIGHT_CMD > "$report" 2> "$OUT/preflight-b.err" || true
    elif [ ! -S "$SOCKET" ]; then
        # A necessary condition, kept — and now no longer mistaken for a
        # sufficient one. Written into the report rather than exiting here, so
        # that a dead machine and an absent one come out of the same place.
        printf '{"ready":false,"because":["there is no socket at %s — is the machine up? `make -C image run-agent`"]}\n' \
            "$SOCKET" > "$report"
    else
        "$MCP" --connect "$SOCKET" --preflight \
            --wait "${THALYX_BENCH_PREFLIGHT_WAIT:-20}" \
            > "$report" 2> "$OUT/preflight-b.err" || true
    fi
    # `|| true`, and it is not laziness. The verdict *exits non-zero when the
    # machine is not ready* — that is its whole job — and this file runs under
    # `set -e`, so without it a dead arm B killed the script here, silently,
    # before the block below could say why. The fifth entry in
    # `Estrategia-de-Pruebas.md`: the instrument includes the harness, and the
    # self-test that caught this was written before the code it caught.
    python3 "$ROOT/dev/bench-summary.py" --preflight-verdict "$report" \
        --project "$PROJECT" > "$OUT/preflight-b.verdict.json" 2>&1 || true
}

# ── the three prompts, which are one prompt ──────────────────────────────────
#
# Identical wording between arms, with nothing in any of them naming a tool. A
# prompt that said "use thalyx_symbol" would be measuring whether the model can
# follow an instruction, and the question is whether it reaches for the
# primitive on the strength of the tool description alone.
prompt_for() {
    local task="$1" symbol="$2" marker="$3"
    case "$task" in
        read)
            printf '%s' "Find where the symbol \`$symbol\` is defined, what depends on it, and \
which files I would have to look at to change it. Answer with the definition site, the \
list of dependents, and the list of files to review."
            ;;
        change)
            printf '%s' "Add a short doc comment (a Rust /// line) above the definition of \`$symbol\` \
and above every function in the same file, then tell me which files you changed. Do not \
change any behaviour."
            ;;
        reversible)
            # Five steps and no sixth. Every one of them is something the
            # question needs: find the definition, find the dependents, change
            # all of them the same mechanical way, find out what that touched,
            # and put it back exactly. Dropping any one leaves a task that a
            # single well-aimed edit finishes, which is the task that just came
            # back level.
            #
            # "do not build or test anything" is a control and not a handicap:
            # arm B has no shell, so a `cargo build` in arm A would be an
            # expense in one column with no counterpart in the other, and the
            # thing being compared is finding, changing and undoing.
            printf '%s' "The identifier \`$symbol\` is used in more than one file of this project. \
Do all of the following, in order, and then answer.

1. Find where \`$symbol\` is defined.
2. Find every other place in the project that refers to it.
3. Rename it everywhere: \`$symbol\` becomes \`$marker\`, at its definition and at every \
place that refers to it. Change nothing else — no formatting, no comments, no other \
identifier — and do not build or test anything.
4. Check what you have changed, and note which files it touched and how many places in each.
5. Put the project back exactly as you found it, byte for byte: no file left differing \
from its original content, no file added and no file removed.

Answer with the definition site, the list of files you changed, how many places you \
changed in each, and a last line saying whether the project is back to its original state."
            ;;
        *) return 1 ;;
    esac
}

# ── the self-test ────────────────────────────────────────────────────────────
#
# What can be checked about this harness without spending a run, which is
# everything except what the agent does. Two kinds of claim:
#
#   1. The prompt is one string, shared, and names nothing. Checked by reading
#      *this file*, so it stays true when somebody edits the prompt.
#   2. That `tree_hash` and `walk` here reach the one implementation of what a
#      tree is, which lives in `dev/bench-summary.py` and is exercised in full
#      by `dev/bench-summary.py --self-test` — permissions, symlinks, entry
#      type, existence, contents, and the witness. Repeating that battery here
#      would be a second set of tests over the same code, which is how two
#      implementations get written in the first place. What is checked here is
#      the wiring: a baseline that does not move, and one control the *old*
#      pipeline let through, so a revert to it fails loudly.
#   3. That the two halves fit: `--restored-b` becomes arm B's after-walk, the
#      summary reads the pair, and an arm nobody looked at is NOT PROVEN in
#      each of the two ways it can be — and a non-zero exit for whichever of
#      them was demanded.
self_test() {
    local trouble=0
    ok()   { printf '  ok      %s\n' "$*"; }
    bad()  { printf '  FAILED  %s\n' "$*"; trouble=$((trouble + 1)); }

    # ── the prompt is one prompt ──
    local invocations
    invocations=$(grep -c 'claude -p "\$PROMPT"' "${BASH_SOURCE[0]}" || true)
    if [ "$invocations" = 1 ]; then
        ok "there is exactly one place the agent is prompted, so both arms get the same words"
    else
        bad "the prompt is passed in $invocations places; it has to be one, or the arms differ"
    fi

    local reversible
    reversible=$(prompt_for reversible Widget WidgetRenamed)
    for forbidden in thalyx Thalyx THALYX mcp MCP Bash Grep Glob "Read(" Edit Write attempt snapshot; do
        case "$reversible" in
            *"$forbidden"*) bad "the prompt says \"$forbidden\", which names a tool or the system under test" ;;
        esac
    done
    ok "the prompt names no tool, no MCP server and not Thalyx"

    for needed in "Widget" "WidgetRenamed" "byte for byte" "defined" "refers to it"; do
        case "$reversible" in
            *"$needed"*) ;;
            *) bad "the prompt does not ask for \"$needed\"" ;;
        esac
    done
    ok "the prompt asks to find the definition, find the dependents, rename, and restore exactly"

    if [ "$(prompt_for reversible Widget WidgetRenamed)" = "$reversible" ]; then
        ok "the same task and symbol produce the same prompt every time"
    else
        bad "the prompt is not deterministic"
    fi

    if prompt_for nonsense Widget WidgetRenamed >/dev/null 2>&1; then
        bad "an unknown task produced a prompt instead of refusing"
    else
        ok "an unknown task refuses rather than inventing a prompt"
    fi

    # ── a relative --out is not a fact about where the caller was standing ──
    #
    # The failure this reproduces: `--out target/bench-external-agent-3`, and
    # arm A answering `Settings file not found.` because `run_arm` had `cd`ed
    # into the staged workspace before handing `claude` a `--settings` that was
    # still relative to somewhere else. Every derived path had the same fault;
    # the settings file is only the one that says so out loud.
    #
    # Checked from a directory that is neither the checkout nor the target, so
    # that a normalisation which happened to resolve against `$ROOT` would fail
    # here rather than pass by coincidence.
    local relative_from; relative_from=$(mktemp -d)
    (
        cd "$relative_from"
        mkdir -p "here/there"
        OUT="here/there"
        WORKSPACE_A="here/../elsewhere"
        SOCKET="s.sock"
        IMPORT_MARK="m.json"
        PROJECT=""
        EXPECT=""
        RESTORED_B=""
        normalise_paths
        printf '%s\n%s\n%s\n' "$OUT" "$WORKSPACE_A" "$SOCKET"
    ) > "$relative_from/answers"
    local wanted; wanted=$(cd "$relative_from" && pwd -P)
    local n=0
    while read -r line; do
        n=$((n + 1))
        case "$line" in
            /*) ;;
            *) bad "normalise_paths left \`$line\` relative" ;;
        esac
        case "$line" in
            "$wanted"/*) ;;
            *) bad "\`$line\` did not resolve against the directory the caller was in" ;;
        esac
    done < "$relative_from/answers"
    [ "$n" = 3 ] || bad "normalise_paths answered $n paths and should have answered 3"
    if [ "$(sed -n 2p "$relative_from/answers")" = "$wanted/elsewhere" ]; then
        ok "a relative --out and its siblings become absolute, from wherever they were typed"
    else
        bad "normalise_paths did not normalise \`..\` out of a path"
    fi
    # The same argument twice from two directories is the same path, which is
    # the property the settings file needed and did not have.
    local twice_a twice_b
    twice_a=$( cd "$relative_from"       && OUT="here/there" normalise_paths_out )
    twice_b=$( cd "$relative_from/here"  && OUT="../here/there" normalise_paths_out )
    if [ "$twice_a" = "$twice_b" ] && [ -n "$twice_a" ]; then
        ok "the same output directory named from two places is one path"
    else
        bad "the same output directory named from two places came out as $twice_a and $twice_b"
    fi
    rm -rf "$relative_from"

    # ── the wiring to the one implementation ──
    local work
    work=$(mktemp -d)
    mkdir -p "$work/t/src" "$work/t/.git" "$work/t/target"
    printf 'one\n'  > "$work/t/src/a.txt"
    printf 'two\n'  > "$work/t/src/b.txt"
    printf 'index\n' > "$work/t/.git/index"
    printf 'junk\n'  > "$work/t/target/o"

    local baseline; baseline=$(tree_hash "$work/t")
    [ -n "$baseline" ] || bad "tree_hash produced nothing at all"

    touch "$work/t/src/a.txt"
    [ "$(tree_hash "$work/t")" = "$baseline" ] \
        && ok "an untouched tree hashes the same, and a newer mtime is not a change" \
        || bad "the hash moved without anything the task asks about moving"

    printf 'ONE\n' > "$work/t/src/a.txt"
    [ "$(tree_hash "$work/t")" != "$baseline" ] \
        && ok "a changed byte is not a restored tree" || bad "a changed byte hashed as unchanged"
    printf 'one\n' > "$work/t/src/a.txt"
    [ "$(tree_hash "$work/t")" = "$baseline" ] \
        && ok "putting the byte back is a restored tree" || bad "an actual restore did not hash equal"

    # The control that the old contents-only pipeline passed, kept here rather
    # than only in the Python: if somebody ever puts `find -type f | xargs
    # sha256sum` back in this file, this is the line that says so.
    rm -f "$work/t/src/b.txt"
    ln -s /etc/passwd "$work/t/src/b.txt"
    [ "$(tree_hash "$work/t")" != "$baseline" ] \
        && ok "a file replaced by a symlink out of the workspace is not a restored tree" \
        || bad "a symlink out of the workspace hashed as a restored file"
    rm -f "$work/t/src/b.txt"; printf 'two\n' > "$work/t/src/b.txt"

    printf 'rewritten by git status\n' > "$work/t/.git/index"
    printf 'a fresh build\n' > "$work/t/target/o"
    mkdir -p "$work/t/node_modules"; printf 'x\n' > "$work/t/node_modules/p"
    [ "$(tree_hash "$work/t")" = "$baseline" ] \
        && ok ".git, target and node_modules are outside the question" \
        || bad "something outside the workspace's content moved the hash"

    # The machinery boundary, wired the same way as the exclusions above: the
    # socket QEMU opens for the agent channel lives under `image/build`, and on
    # 2026-08-29 it was the *only* difference arm B's restore check reported.
    # The full battery — a real file changed beside it, an unlisted file, a
    # mode, a symlink, a byte — is in `dev/bench-summary.py --self-test`, where
    # the one implementation is. What is checked here is that this file reaches
    # it, and the control beside it so a boundary that swallowed the workspace
    # would not look like a boundary that works.
    mkdir -p "$work/t/image/build"
    printf 'a kernel\n' > "$work/t/image/build/bzImage"
    baseline=$(tree_hash "$work/t")
    printf 'a socket, as far as this test needs one\n' > "$work/t/image/build/agent.sock"
    [ "$(tree_hash "$work/t")" = "$baseline" ] \
        && ok "what make -C image builds is machinery, not the workspace" \
        || bad "the agent channel under image/build moved the workspace's hash"
    printf 'CHANGED\n' > "$work/t/src/a.txt"
    [ "$(tree_hash "$work/t")" != "$baseline" ] \
        && ok "a real file changed beside the machinery still fails the restore" \
        || bad "a real change hid behind the machinery boundary"
    printf 'one\n' > "$work/t/src/a.txt"
    rm -rf "$work/t/image"

    # And that `walk` writes all four, because each of them is something the
    # summary silently does without if the file is missing.
    walk "$work/t" "$work/walked"
    if [ -s "$work/walked" ] && [ -s "$work/walked.manifest" ] \
            && [ -s "$work/walked.mtimes" ] && [ -s "$work/walked.setaside" ]; then
        ok "a walk leaves the digest, the manifest, the mtimes and the set-aside report"
    else
        bad "a walk did not leave all four of the digest, manifest, mtimes and set-aside"
    fi

    # ── the two halves fit together ──
    #
    # No agent runs here. What is checked is the wiring between this file and
    # `bench-summary.py`: that `--restored-b` turns a directory into arm B's
    # after-hash, that the summary reads the pair, and that an arm nobody hashed
    # comes back NOT PROVEN and fails when somebody demanded it. The stream
    # standing in for a run is the captured real session, copied — the same
    # bytes the parser's own self-test reads.
    local bench="$work/bench" sample="$ROOT/dev/samples/claude-stream-json.ndjson"
    mkdir -p "$bench" "$work/project"
    printf 'a project\n' > "$work/project/f.txt"
    if [ -f "$sample" ]; then
        cp "$sample" "$bench/armB.ndjson"
        walk "$work/project" "$bench/armB.before"

        # Not walked yet: NOT PROVEN, and a non-zero exit when demanded.
        if THALYX_REQUIRE_RESTORE_CHECK=1 "${BASH_SOURCE[0]}" \
                --project "$work/project" --symbol Widget --task reversible \
                --arms none --out "$bench" > "$work/log" 2>&1; then
            bad "an arm whose tree nobody hashed passed while the check was demanded"
        else
            ok "an unhashed arm is NOT PROVEN, and demanding the check makes it a failure"
        fi

        # Restored: the exported tree is the tree it started from.
        cp -a "$work/project" "$work/exported"
        "${BASH_SOURCE[0]}" --project "$work/project" --symbol Widget --task reversible \
            --arms none --out "$bench" --restored-b "$work/exported" > "$work/log" 2>&1
        if grep -q '"restored": true' "$bench/summary.json"; then
            ok "an exported tree that matches is reported as restored"
        else
            bad "a matching exported tree was not reported as restored"
        fi

        # …and, with the tree restored and nothing in the stream having changed
        # anything, the verdict must still not be a pass. This is the whole
        # audit finding in one assertion: the captured session is a single
        # `Read`, so an oracle that read the verdict off the digest would call
        # it done.
        if grep -q '"passed": true' "$bench/summary.json"; then
            bad "a run that only read scored a pass because its tree came back"
        else
            ok "a restored tree is not a pass on its own"
        fi

        # The witness, in both of its states, because they are different facts
        # and the summary has to keep them apart. `cp -a` preserves mtimes, so
        # the exported tree here is a workspace nothing ever wrote to: the
        # witness saw nothing, which is a **false** and not an absence.
        if grep -q '"intermediate_state": false' "$bench/summary.json"; then
            ok "a workspace nothing wrote to is witnessed as unchanged, not left unknown"
        else
            bad "a workspace nothing wrote to did not come back as an unchanged witness"
        fi

        # Losing the mtimes is no longer losing the witness, and that is the
        # whole of the 2026-08-29 repair: the tools' own answers are in the
        # stream, and no restore reaches back into it. An arm whose walks are
        # gone is still witnessed by what its tools said.
        rm -f "$bench/armB.after.mtimes" "$bench/armB.before.mtimes"
        "${BASH_SOURCE[0]}" --project "$work/project" --symbol Widget --task reversible \
            --arms none --out "$bench" --restored-b "$work/exported" > "$work/log" 2>&1
        if grep -q '"intermediate_state": "not_proven"' "$bench/summary.json"; then
            bad "an arm with a stream on disk lost its witness when the mtimes went"
        else
            ok "losing the mtimes does not lose the witness: the stream still has the tools' answers"
        fi

        # And absence, which is what an arm measured by the older
        # `--output-format json` looks like: a hashed tree, no per-tool detail,
        # and therefore no witness at all. NOT PROVEN, with its own switch,
        # because an arm can have a perfect tree and nothing that saw it change.
        mv "$bench/armB.ndjson" "$bench/armB.ndjson.kept"
        printf '{"is_error":false,"num_turns":2,"duration_ms":1,"total_cost_usd":0.1,"result":"done"}\n' \
            > "$bench/armB.json"
        if THALYX_REQUIRE_MUTATION_WITNESS=1 "${BASH_SOURCE[0]}" \
                --project "$work/project" --symbol Widget --task reversible \
                --arms none --out "$bench" > "$work/log" 2>&1; then
            bad "an arm nobody witnessed passed while the witness was demanded"
        else
            grep -q 'NOT PROVEN' "$work/log" \
                && ok "an unwitnessed arm is NOT PROVEN, and its own switch makes it a failure" \
                || bad "an unwitnessed arm failed without saying it was NOT PROVEN"
        fi
        rm -f "$bench/armB.json"
        mv "$bench/armB.ndjson.kept" "$bench/armB.ndjson"

        # ── reading a run that is already over ──
        #
        # No agent, no project walked again: `--regrade` reads what is in the
        # output directory and writes a second summary beside the first. The
        # first must survive, because a run whose instrument turned out to be
        # wrong is worth more with both readings kept.
        cp "$bench/summary.json" "$work/summary-before-regrade.json"
        walk "$work/project" "$bench/armB.before"
        "${BASH_SOURCE[0]}" --task reversible --symbol Widget --out "$bench" \
            --regrade > "$work/log" 2>&1 || true
        if [ -s "$bench/summary-regraded.json" ] \
                && cmp -s "$bench/summary.json" "$work/summary-before-regrade.json"; then
            ok "a regrade writes its own summary and leaves the original untouched"
        else
            bad "a regrade did not write summary-regraded.json, or wrote over summary.json"
        fi
        if grep -q '"claude_was_not_called": true' "$bench/summary-regraded.json" \
                && grep -q '"the_grader_changed_after_the_run": true' "$bench/summary-regraded.json"; then
            ok "the regraded summary says on its face that no agent was run for it"
        else
            bad "the regraded summary does not say where its numbers came from"
        fi
        # Into a file and then grepped, never piped into `grep -q`: this file
        # runs under `pipefail` and `grep -q` closes the pipe on its first
        # match, so the pipeline reports the SIGPIPE and a passing check reads
        # as a failure. That is the fifth entry in `Estrategia-de-Pruebas.md`'s
        # list of times the instrument was the thing that was wrong.
        "${BASH_SOURCE[0]}" --task reversible --symbol Widget --out "$bench" \
            --forensics > "$work/forensics" 2>&1 || true
        if grep -q 'arm B' "$work/forensics"; then
            ok "the forensic table can be read out of a run that is over"
        else
            bad "the forensic table said nothing about a stream that is on disk"
        fi

        # And the control, without which "restored" and "never looked" are the
        # same answer: a tree that did not come back must not say it did.
        printf 'left behind\n' > "$work/exported/leftover.txt"
        "${BASH_SOURCE[0]}" --project "$work/project" --symbol Widget --task reversible \
            --arms none --out "$bench" --restored-b "$work/exported" > "$work/log" 2>&1
        if grep -q '"restored": false' "$bench/summary.json"; then
            ok "an exported tree with a file left in it is reported as not restored"
        else
            bad "a tree that did not come back was reported as restored"
        fi
    else
        printf '  NOT PROVEN  the captured session is missing, so the two halves were not fitted\n'
        trouble=$((trouble + 1))
    fi

    # ── the anchoring of arm A, and the preflight of arm B ──
    #
    # Every case here is the run of 2026-08-29 taken apart. It was given
    # `--project /tmp/bench-thalyx`, arm A ran `cd /home/cesarmanzocode/thalyx`,
    # and then arm B produced nothing at all — and the harness had no question
    # whose answer would have been any of that.
    #
    # No agent runs and no machine is needed: what is checked is this file's own
    # decisions, with the probe replaced by a command that prints a canned
    # answer. `bench-summary.py --self-test` checks the classification itself.

    # The default workspace is not inside this checkout, which is the fault.
    case "${TMPDIR:-/tmp}/thalyx-bench-arm-a" in
        "$ROOT"/*) bad "the default workspace is inside $ROOT, which is the 2026-08-29 fault" ;;
        *) ok "arm A's workspace defaults to somewhere outside this checkout" ;;
    esac

    local nest
    nest=$(mktemp -d)
    mkdir -p "$nest/clean/w" "$nest/repo/target/bench/w"
    printf 'read this before anything else\n' > "$nest/repo/CLAUDE.md"
    ancestry_check "$nest/clean/w" >/dev/null 2>&1 \
        && ok "a workspace with nothing above it passes the ancestry check" \
        || bad "a workspace with nothing above it was refused"
    ancestry_check "$nest/repo/target/bench/w" >/dev/null 2>&1 \
        && bad "a workspace under a checkout with a CLAUDE.md was allowed" \
        || ok "a workspace under a CLAUDE.md is refused: that is what put arm A in ~/thalyx"
    rm -f "$nest/repo/CLAUDE.md"; mkdir -p "$nest/repo/.git"
    ancestry_check "$nest/repo/target/bench/w" >/dev/null 2>&1 \
        && bad "a workspace inside another repository's .git range was allowed" \
        || ok "a workspace under somebody else's .git is refused too"

    # A `claude` that must never be called, and says so if it is.
    #
    # The two checks below are the only ones in this file whose runs are
    # supposed to stop *before* an agent, and until 2026-08-29 they were also
    # the only ones with no stand-in on the PATH — so they leaned on a real
    # `claude` being installed for the run to get as far as the refusal they
    # were about. `dev/verify.sh` runs under `sudo`, root's PATH has no
    # `claude`, and both of them failed on Cesar's machine with the harness
    # saying `no claude on this host`: rule 5 exactly, the instrument answering
    # a question nobody asked it.
    #
    # It also makes the claim direct rather than inferred. "Arm A was never
    # paid for" was being read off an `armA.ndjson` that is empty for several
    # reasons; this file exists only if something actually started an agent.
    local nobin="$nest/nobin" started="$nest/agent-was-started"
    mkdir -p "$nobin"
    cat > "$nobin/claude" <<STANDIN
#!/usr/bin/env bash
printf 'called at %s\n' "\$PWD" >> "$started"
STANDIN
    chmod +x "$nobin/claude"

    # And that the run refuses rather than staging there. The project is real,
    # the workspace is under a CLAUDE.md, and no agent may be started.
    mkdir -p "$nest/project"; printf 'x\n' > "$nest/project/f.txt"
    printf 'read this\n' > "$nest/repo/CLAUDE.md"
    if PATH="$nobin:$PATH" THALYX_BENCH_WORKSPACE="$nest/repo/target/bench/w" \
            "${BASH_SOURCE[0]}" \
            --project "$nest/project" --symbol Widget --task reversible \
            --arms A --out "$nest/out" > "$nest/log" 2>&1; then
        bad "the harness staged arm A under a CLAUDE.md and ran it"
    else
        if grep -q 'CLAUDE.md is above the workspace' "$nest/log" && [ ! -e "$started" ]; then
            ok "the run refuses, by name, before starting an agent anywhere"
        else
            bad "the run failed without saying the workspace's ancestry was why"
            sed 's/^/      /' "$nest/log"
        fi
    fi

    # ── arm B, checked before arm A is paid for ──
    #
    # The probe is replaced, so a dead machine, a stale one and a healthy one
    # can all be exercised in a container with no KVM. What is being checked is
    # this file's ordering: that a machine which is not READY stops the run
    # *before* `claude` is called for arm A.
    mkdir -p "$nest/out2"
    local dead='{"ready":false,"because":["the socket is there and the machine never said hello"]}'
    if PATH="$nobin:$PATH" THALYX_BENCH_PREFLIGHT_CMD="printf %s $dead" \
            THALYX_BENCH_WORKSPACE="$nest/clean/w" "${BASH_SOURCE[0]}" \
            --project "$nest/project" --symbol Widget --task reversible \
            --arms AB --out "$nest/out2" > "$nest/log2" 2>&1; then
        bad "a machine that never said hello did not stop the run"
    else
        if grep -q 'arm B is NOT READY' "$nest/log2" && [ ! -e "$started" ] \
                && [ ! -s "$nest/out2/armA.ndjson" ]; then
            ok "a dead arm B stops the run before arm A is run at all"
        else
            bad "a dead arm B was found out after arm A had already been paid for"
            sed 's/^/      /' "$nest/log2"
        fi
    fi

    # The control, one field apart: a machine that answers and is holding this
    # project gets past the preflight. Without it, a preflight that refused
    # everything would look exactly like one that works.
    local top alive
    top=$(python3 -c "
import json,sys,pathlib
sys.path.insert(0, '$ROOT/dev')
print(json.dumps(sorted(p.name for p in pathlib.Path('$nest/project').iterdir())))")
    alive="{\"ready\":true,\"thalyx\":\"0.1.0\",\"workspace\":\"/home/project\",\"tools_offered\":11,\"top_level\":$top}"
    printf '%s' "$alive" > "$nest/alive.json"
    python3 "$ROOT/dev/bench-summary.py" --preflight-verdict "$nest/alive.json" \
        --project "$nest/project" > "$nest/verdict.json" 2>&1 \
        && ok "a machine that answered and holds this project is READY" \
        || { bad "a healthy machine was refused"; cat "$nest/verdict.json"; }

    # ── and the whole thing, end to end, with a stand-in for the agent ──
    #
    # No API call and no money: `claude` is replaced on the PATH by a script
    # that prints a stream of the shape Claude Code prints. What is being
    # checked is this file's behaviour around it — that a run whose agent
    # wandered is stopped, said to be INVALID, and stopped *before* arm B.
    #
    # The stand-in models the property under test and nothing else, which is
    # rule 8: it prints a `system init` with a working directory and one
    # `tool_use` block, because those are the two things the anchoring check
    # reads. A fake that also had to be an agent would be a different system.
    local bin="$nest/bin"
    mkdir -p "$bin" "$nest/out3" "$nest/clean/w3"
    cat > "$bin/claude" <<'STANDIN'
#!/usr/bin/env bash
# Prints where it was started and one tool call, in Claude Code's own shapes.
printf '{"type":"system","subtype":"init","cwd":"%s","session_id":"stand-in"}\n' "$PWD"
printf '%s\n' "$THALYX_SELFTEST_CALL"
printf '{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":1,"total_cost_usd":0.0,"result":"done"}\n'
STANDIN
    chmod +x "$bin/claude"

    local wandering='{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cd '"$nest"'/repo && grep -rn Widget ."}}]}}'
    local staying='{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Grep","input":{"pattern":"Widget"}}]}}'
    local ready='{"ready":true,"thalyx":"0.1.0","workspace":"/home/project","tools_offered":11,"top_level":["f.txt"]}'

    # Arm B's half of the provenance, written with the same program
    # `project-stage` writes it with, so the parity gate below is passed for the
    # reason a real import would pass it rather than because this handed it a
    # match it made up.
    python3 "$ROOT/dev/bench-summary.py" --import-stamp "$nest/project" \
        --workspace /home/project > "$nest/import-good.json"

    if PATH="$bin:$PATH" THALYX_SELFTEST_CALL="$wandering" \
            THALYX_BENCH_PREFLIGHT_CMD="printf %s $ready" \
            THALYX_BENCH_IMPORT_MARK="$nest/import-good.json" \
            THALYX_BENCH_WORKSPACE="$nest/clean/w3" "${BASH_SOURCE[0]}" \
            --project "$nest/project" --symbol Widget --task reversible \
            --arms AB --out "$nest/out3" > "$nest/log3" 2>&1; then
        bad "an arm A that worked in another tree was graded instead of refused"
    else
        if grep -q 'INVALID: arm A did not work in the copy it was given' "$nest/log3" \
                && [ ! -s "$nest/out3/armB.ndjson" ]; then
            ok "an arm A that reached outside its workspace is INVALID, and arm B is \
never paid for"
        else
            bad "an arm A that reached outside its workspace was not caught before arm B"
            sed 's/^/      /' "$nest/log3"
        fi
    fi

    # And the other half of the same claim, which is a different fact: an agent
    # whose *working directory* was not the workspace. Every path it named could
    # be relative and innocent-looking and it would still have been reading
    # somebody else's tree — which is exactly the shape of 2026-08-29. The
    # stand-in is told to report a cwd it was not started in.
    rm -rf "$nest/out3b"; mkdir -p "$nest/out3b"
    cat > "$bin/claude-elsewhere" <<'STANDIN'
#!/usr/bin/env bash
printf '{"type":"system","subtype":"init","cwd":"%s","session_id":"stand-in"}\n' \
    "$THALYX_SELFTEST_CWD"
printf '%s\n' "$THALYX_SELFTEST_CALL"
printf '{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":1,"total_cost_usd":0.0,"result":"done"}\n'
STANDIN
    chmod +x "$bin/claude-elsewhere"
    cp "$bin/claude-elsewhere" "$bin/claude"
    if PATH="$bin:$PATH" THALYX_SELFTEST_CALL="$staying" \
            THALYX_SELFTEST_CWD="$nest/repo" \
            THALYX_BENCH_PREFLIGHT_CMD="printf %s $ready" \
            THALYX_BENCH_IMPORT_MARK="$nest/import-good.json" \
            THALYX_BENCH_WORKSPACE="$nest/clean/w3" "${BASH_SOURCE[0]}" \
            --project "$nest/project" --symbol Widget --task reversible \
            --arms AB --out "$nest/out3b" > "$nest/log3b" 2>&1; then
        bad "an arm A that started somewhere else was graded instead of refused"
    else
        if grep -q 'it started in' "$nest/log3b" && [ ! -s "$nest/out3b/armB.ndjson" ]; then
            ok "an arm A whose working directory was not the workspace is INVALID, \
even with every path it named relative and innocent"
        else
            bad "an arm A that started in another tree was not caught"
            sed 's/^/      /' "$nest/log3b"
        fi
    fi
    # Back to the honest stand-in for the control below.
    cat > "$bin/claude" <<'STANDIN'
#!/usr/bin/env bash
printf '{"type":"system","subtype":"init","cwd":"%s","session_id":"stand-in"}\n' "$PWD"
printf '%s\n' "$THALYX_SELFTEST_CALL"
printf '{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":1,"total_cost_usd":0.0,"result":"done"}\n'
STANDIN
    chmod +x "$bin/claude"

    # The control. Same stand-in, same everything, one tool call that stays put:
    # the run must get past the anchoring check and go on to arm B. Without it a
    # check that refused every run would look exactly like one that works.
    rm -rf "$nest/out4"; mkdir -p "$nest/out4"
    PATH="$bin:$PATH" THALYX_SELFTEST_CALL="$staying" \
        THALYX_BENCH_PREFLIGHT_CMD="printf %s $ready" \
        THALYX_BENCH_WORKSPACE="$nest/clean/w3" "${BASH_SOURCE[0]}" \
        --project "$nest/project" --symbol Widget --task reversible \
        --arms A --out "$nest/out4" > "$nest/log4" 2>&1 || true
    grep -q 'arm A: stayed inside' "$nest/log4" \
        && ok "an arm A that stayed inside its workspace is not refused" \
        || { bad "an arm A that did nothing wrong was refused"; sed 's/^/      /' "$nest/log4"; }

    # And that the two arms being staged from different trees stops the run
    # before either of them is called. The import stamp is written by hand here
    # because no machine is being staged: what is under test is the gate.
    rm -rf "$nest/out5"; mkdir -p "$nest/out5"
    printf '{"imported_from":"/tmp/some-other-project","input_manifest":"beef",\
"workspace":"/home/other"}\n' > "$nest/import.json"
    if PATH="$bin:$PATH" THALYX_SELFTEST_CALL="$staying" \
            THALYX_BENCH_PREFLIGHT_CMD="printf %s $ready" \
            THALYX_BENCH_IMPORT_MARK="$nest/import.json" \
            THALYX_BENCH_WORKSPACE="$nest/clean/w3" "${BASH_SOURCE[0]}" \
            --project "$nest/project" --symbol Widget --task reversible \
            --arms AB --out "$nest/out5" > "$nest/log5" 2>&1; then
        bad "two arms staged from different trees were compared anyway"
    else
        grep -q 'the two arms were not given the same tree' "$nest/log5" \
            && ok "two arms staged from different trees stop the run before either is called" \
            || { bad "different input trees did not stop the run for that reason"; \
                 sed 's/^/      /' "$nest/log5"; }
    fi

    rm -rf "$nest"

    rm -rf "$work"

    printf '\n'
    [ "$trouble" = 0 ] && { printf '  PROVEN\n'; return 0; }
    printf '  %s FAILED\n' "$trouble"
    return 1
}

if [ -n "$SELFTEST" ]; then
    self_test
    exit $?
fi

# Neither of these runs an agent, and neither needs a project on disk: they read
# what a finished run left in `--out`. `--forensics` is the table that answers
# "did those six `Edit` calls do anything" without a second run, which on
# 2026-08-29 was a question three different summaries could not tell apart.
if [ -n "$FORENSICS" ] || [ -n "$REGRADE" ]; then
    [ -d "$OUT" ] || { say "no run to read at $OUT"; exit 1; }
    [ -n "$MARKER" ] || [ -z "$SYMBOL" ] || MARKER="${SYMBOL}Renamed"
    READ_ARGS=(--out "$OUT" --task "$TASK" --symbol "$SYMBOL" --model "$MODEL" --turns "$TURNS")
    [ -n "$EXPECT" ] && READ_ARGS+=(--expect-file "$EXPECT")
    [ "$TASK" = reversible ] && READ_ARGS+=(--marker "$MARKER")
    if [ -n "$FORENSICS" ]; then
        python3 "$ROOT/dev/bench-summary.py" "${READ_ARGS[@]}" --forensics
        exit $?
    fi
    python3 "$ROOT/dev/bench-summary.py" "${READ_ARGS[@]}" --regrade
    STATUS=$?
    say
    say "regraded: $OUT/summary-regraded.json"
    say "original: $OUT/summary.json  (not written to)"
    exit $STATUS
fi

[ -n "$PROJECT" ] || { say "which project: --project <dir>"; exit 1; }
[ -d "$PROJECT" ] || { say "$PROJECT is not a directory"; exit 1; }
[ -n "$SYMBOL" ] || { say "which symbol the task is about: --symbol <Name>"; exit 1; }
[ -z "$ARMS" ] || command -v claude >/dev/null 2>&1 || { say "no claude on this host"; exit 1; }

[ -n "$MARKER" ] || MARKER="${SYMBOL}Renamed"

PROMPT="$(prompt_for "$TASK" "$SYMBOL" "$MARKER")" \
    || { say "--task is read, change or reversible"; exit 1; }

if [[ "$ARMS" == *B* ]]; then
    MCP="$ROOT/target/release/thalyx-mcp"
    [ -x "$MCP" ] || ( cd "$ROOT" && cargo build --release -p thalyx-mcp >/dev/null )
fi

mkdir -p "$OUT"

# ── arm B is checked before arm A is paid for ────────────────────────────────
#
# Order matters and it is the whole point of this block being here rather than
# beside arm B. The run of 2026-08-29 spent arm A in full and *then* discovered
# that arm B was not there. Nothing about arm B's readiness depends on arm A
# having run, so there is no reason to find out in that order — and every reason
# not to.
if [[ "$ARMS" == *B* ]]; then
    preflight_b
    if ! python3 -c "
import json, sys
print(json.load(open(sys.argv[1]))['ready'])" "$OUT/preflight-b.verdict.json" 2>/dev/null | grep -qx True; then
        say
        say "arm B is NOT READY, so nothing was run and nothing was paid for:"
        sed 's/^/    /' "$OUT/preflight-b.verdict.json"
        [ -s "$OUT/preflight-b.err" ] && sed 's/^/    /' "$OUT/preflight-b.err"
        say
        say "The machine comes up with:"
        say
        say "    make -C image agent PROJECT=$PROJECT"
        exit 1
    fi
    say "arm B: READY"
fi

run_arm() {
    local arm="$1" cwd="$2"; shift 2
    local began ended
    say "arm $arm: running (model $MODEL, at most $TURNS turns)"
    # Anything left from a previous run goes first. The summary falls back to
    # the older single-object shape when a stream is empty, and a stale file
    # from last week is exactly the input that would make a failed run look
    # like a successful one.
    rm -f "$OUT/arm$arm.ndjson" "$OUT/arm$arm.json"
    began=$(date +%s)
    # `stream-json` and not `json`, so that every tool the model called is on
    # disk. `--verbose` is what the CLI requires to emit the stream in `-p`
    # mode, and the whole stream is kept: a summary can be recomputed from it
    # later, and a run whose numbers are argued about is a run nobody can
    # re-read if only the summary survived.
    ( cd "$cwd" && timeout 1800 claude -p "$PROMPT" \
        --permission-mode acceptEdits \
        --max-turns "$TURNS" --output-format stream-json --verbose --model "$MODEL" \
        "$@" < /dev/null ) > "$OUT/arm$arm.ndjson" 2> "$OUT/arm$arm.err" || true
    ended=$(date +%s)
    # The wall time the summary reports is the agent's own. This one is the
    # whole invocation including process start, and it is printed rather than
    # recorded so that the two are never confused for each other.
    say "arm $arm: $((ended - began))s wall, $(wc -l < "$OUT/arm$arm.ndjson") stream events"
}

# ── arm A: an ordinary Linux copy, ordinary tools, and nowhere else ──────────
#
# The copy is staged **outside this checkout**. Everything above the workspace
# is checked first, the process is started physically inside it, a `PreToolUse`
# hook refuses any call that names a path out of it, and afterwards the stream's
# own `system init` event is read back to say where the agent actually was.
# Four things, because the one that was there — passing the directory as the
# working directory — is exactly what was there on 2026-08-29 and it was not
# enough.
if [[ "$ARMS" == *A* ]]; then
    rm -rf "$WORKSPACE_A"
    mkdir -p "$WORKSPACE_A"
    if ! ancestry_check "$WORKSPACE_A"; then
        say
        say "arm A's workspace has project context above it, so an agent started there"
        say "would be reading somebody else's instructions about somebody else's tree."
        say "That is the fault of 2026-08-29. Stage it somewhere with nothing above it:"
        say
        say "    $0 --project $PROJECT --workspace /tmp/thalyx-bench-arm-a …"
        exit 1
    fi
    tar -C "$PROJECT" --exclude=./target --exclude=./node_modules -cf - . \
        | tar -C "$WORKSPACE_A" -xf -
    walk "$WORKSPACE_A" "$OUT/armA.before"
    provenance_now
    parity_gate

    # The guard, live. Exit 2 is Claude Code's "blocked, and this is why"; the
    # classification is `bench-summary.py`'s, so the call refused during the run
    # and the call reported after it are decided by one program rather than two
    # that agree until they do not.
    #
    # **It is the second line of defence and never the first.** A hook is
    # something the CLI has to honour, and a run in `-p` mode silently ignores a
    # settings file it will not parse — so a harness that trusted this would be
    # trusting a thing it cannot see fail. What decides the verdict is
    # `--scope-check` below, read out of the stream the run itself wrote, which
    # needs nothing to have worked. This exists so that a call which would leave
    # the workspace is *stopped* as well as *counted*.
    #
    # No `matcher`, which is how a hook says every tool.
    BREACH="$OUT/armA.breach.jsonl"
    rm -f "$BREACH"
    cat > "$OUT/armA.settings.json" <<JSON
{"hooks": {"PreToolUse": [{"hooks": [{"type": "command",
 "command": "python3 $ROOT/dev/bench-summary.py --scope-guard --workspace $WORKSPACE_A --home $HOME --breach-file $BREACH"}]}]}}
JSON

    run_arm A "$WORKSPACE_A" \
        --allowedTools "Read" "Edit" "Write" "Grep" "Glob" "Bash" \
        --settings "$OUT/armA.settings.json" \
        --strict-mcp-config
    walk "$WORKSPACE_A" "$OUT/armA.after"

    # ── did arm A stay where it was put ──
    #
    # Here, and not only in the summary, because here is the last moment at
    # which the answer still saves money: an arm A that worked in the wrong
    # tree makes the comparison void, and arm B has not been paid for yet.
    if ! python3 "$ROOT/dev/bench-summary.py" --scope-check "$OUT" --arm A \
            > "$OUT/scope-A.json" 2> "$OUT/scope-A.err"; then
        say
        say "INVALID: arm A did not work in the copy it was given."
        sed 's/^/    /' "$OUT/scope-A.err" 2>/dev/null || true
        python3 - "$OUT/scope-A.json" <<'REPORT' || true
import json, sys
try:
    report = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(0)
if report.get("cwd_is_the_workspace") is False:
    print(f"    it started in {report.get('cwd_reported')!r}, not "
          f"{report.get('workspace')!r}")
for entry in report.get("paths_outside_the_workspace", [])[:10]:
    print(f"    {entry['tool']} {entry['field']}={entry['path']!r}")
REPORT
        say
        say "Nothing was run for arm B, so nothing more was spent. The full report is at"
        say "$OUT/scope-A.json."
        exit 1
    fi
    say "arm A: stayed inside $WORKSPACE_A"
fi

# ── arm B: the same bytes, inside a Thalyx machine ───────────────────────────
#
# The workspace is *not* copied here: it is already inside the VM, put there by
# `make -C image agent PROJECT=…`. What this checks is that it is the same
# project — a comparison between two different trees is not a comparison.
if [[ "$ARMS" == *B* ]]; then
    [ -S "$SOCKET" ] || { say "no agent channel at $SOCKET — is the machine up?"; exit 1; }
    cat > "$OUT/mcp.json" <<JSON
{"mcpServers":{"thalyx":{"type":"stdio","command":"$MCP",
 "args":["--connect","$SOCKET","--metrics","$OUT/armB.metrics.json"]}}}
JSON
    rm -f "$OUT/armB.metrics.json"
    # What arm B started from, hashed on this host.
    #
    # Not the machine's copy — that is inside the VM and unreachable from here —
    # but `$PROJECT`, which is what `image/Makefile` tarred onto the store with
    # the same exclusions `tree_hash` applies. The assumption is written down
    # because it can be wrong: if the workspace on the store is not `$PROJECT`
    # (a stale store from an earlier project, a `store-stage` that was never
    # re-run), this hash is of the wrong tree and the summary will say the tree
    # did not come back. That is the cautious direction — rule 9 — and
    # `armB.before` is on disk so the mistake is findable rather than silent.
    walk "$PROJECT" "$OUT/armB.before"
    # An empty directory, so that nothing on this host is reachable even by
    # accident. Everything arm B can see is inside the machine.
    rm -rf "$OUT/b"; mkdir -p "$OUT/b"
    ancestry_check "$OUT/b" || say "note: arm B's empty cwd has context above it; it has \
no file tools, so nothing there is reachable — but the two arms' cwds are not alike"
    run_arm B "$OUT/b" \
        --mcp-config "$OUT/mcp.json" --strict-mcp-config \
        --allowedTools "mcp__thalyx" \
        --disallowedTools "Read" "Edit" "Write" "Grep" "Glob" "Bash" "NotebookEdit" "WebFetch" "WebSearch" "Task"
fi

# ── arm B, restored: the bytes off the store, read by this host ──────────────
#
# The only honest way to answer "did arm B put everything back" is to look at
# the bytes, and the bytes are inside a Btrfs image that QEMU has open
# read-write. So this cannot happen during the run — mounting a live filesystem
# is how a store gets corrupted — and it is a second pass, with the machine
# down, over the same `--out` directory:
#
#     sudo make -C image agent-export INTO=/tmp/armB-after
#     dev/bench-external-agent.sh --project … --symbol … --task reversible \
#         --arms none --restored-b /tmp/armB-after
#
# Until then the summary reports the restore as `not_proven`, never as a pass.
if [ -n "$RESTORED_B" ]; then
    [ -d "$RESTORED_B" ] || { say "$RESTORED_B is not a directory"; exit 1; }
    walk "$RESTORED_B" "$OUT/armB.after"
    say "arm B: hashed the exported workspace at $RESTORED_B"
fi

# ── the summary ──────────────────────────────────────────────────────────────
#
# A separate program, because a parser for somebody else's output that lives
# inside a shell script is a parser nobody can test without running the thing it
# parses. `dev/bench-summary.py --self-test` checks it against a captured real
# session, in a second, for free.
SUMMARY_ARGS=(--out "$OUT" --task "$TASK" --symbol "$SYMBOL" --model "$MODEL" --turns "$TURNS")
[ -n "$EXPECT" ] && SUMMARY_ARGS+=(--expect-file "$EXPECT")
# The marker is what tells a run that really changed everything and put it back
# from a run that did nothing at all — both of which leave the tree hashing
# equal. Only the `reversible` task has one; for the others the field is absent
# rather than zero.
[ "$TASK" = reversible ] && SUMMARY_ARGS+=(--marker "$MARKER")
# Rule 3: one environment variable per requirement, and the requirement here is
# "somebody looked at arm B's bytes afterwards".
[ "${THALYX_REQUIRE_RESTORE_CHECK:-}" = 1 ] && SUMMARY_ARGS+=(--require-restore-check)
# And the other requirement, which is a different one and therefore a different
# variable: that something outside the agent saw the workspace change. An arm
# can have a perfectly hashed tree and no witness at all — that is precisely the
# agent that did nothing — so one variable for both would mean the only way to
# demand either is to demand the other.
[ "${THALYX_REQUIRE_MUTATION_WITNESS:-}" = 1 ] && SUMMARY_ARGS+=(--require-mutation-witness)
# Not `set -e`'s business: the summary exits non-zero when a requirement the
# caller *demanded* came back NOT PROVEN, and the paths and the next command
# below are exactly what somebody in that situation needs to read.
SUMMARISED=0
python3 "$ROOT/dev/bench-summary.py" "${SUMMARY_ARGS[@]}" || SUMMARISED=$?

say
say "streams:  $OUT/armA.ndjson  $OUT/armB.ndjson"
say "summary:  $OUT/summary.json"

if [ "$TASK" = reversible ]; then
    say
    say "If the grader changes after this run, read it again without paying for another:"
    say
    say "    $0 --task $TASK --symbol $SYMBOL --out $OUT --regrade"
    say "    $0 --task $TASK --symbol $SYMBOL --out $OUT --forensics"
fi

if [ "$TASK" = reversible ] && [ ! -s "$OUT/armB.after" ]; then
    say
    say "arm B's restore is NOT PROVEN: nothing on this host has looked at its bytes yet."
    say "Shut the machine down, then:"
    say
    say "    sudo make -C image agent-export INTO=$OUT/b-export"
    say "    $0 --project $PROJECT --symbol $SYMBOL --task reversible \\"
    say "        --arms none --out $OUT --restored-b $OUT/b-export"
fi

say
say "arm A worked in: $WORKSPACE_A"
say
say "One run is an anecdote. What this proves is that the comparison can be run."
exit $SUMMARISED
