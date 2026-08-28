#!/usr/bin/env bash
# The same agent, the same task, two machines. Which one works better?
#
#   dev/bench-external-agent.sh --project <dir> [--socket <sock>]
#                               [--task read|change|reversible]
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

# A hash of every file in a tree, by content, in a stable order. Not a
# timestamp: the question the `change` and `reversible` tasks ask is whether the
# bytes came back, and mtimes never come back.
#
# The exclusions are not a taste. They are exactly what
# `image/Makefile`'s staging leaves out of the copy it puts on the store
# (`--exclude=./target --exclude=./node_modules`) plus `.git`, which both arms
# carry and which changes for reasons that are not the task — a `git status` in
# arm A rewrites the index. Hashing it would make arm A fail a restore it
# performed correctly, and only arm A, which is the one direction a comparison
# must never be wrong in.
tree_hash() {
    ( cd "$1" && find . -type f \
        -not -path './.git/*' -not -path './target/*' -not -path './node_modules/*' \
        -print0 \
        | LC_ALL=C sort -z \
        | xargs -0 sha256sum 2>/dev/null \
        | sha256sum | cut -d' ' -f1 )
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
#   2. `tree_hash` answers the question the `reversible` task rests on. Rule 4:
#      a check that something came back needs a baseline (the untouched tree,
#      which must hash the same twice) and a control (a tree that really did
#      change, which must not) — without the second, a hash function that
#      returned a constant would pass.
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

    # ── tree_hash answers the restore question ──
    local work
    work=$(mktemp -d)
    mkdir -p "$work/t/src" "$work/t/.git" "$work/t/target"
    printf 'one\n'  > "$work/t/src/a.txt"
    printf 'two\n'  > "$work/t/src/b.txt"
    printf 'index\n' > "$work/t/.git/index"
    printf 'junk\n'  > "$work/t/target/o"

    local baseline; baseline=$(tree_hash "$work/t")

    # Baseline: nothing happened, and the answer must not move. A hash that
    # folded in an mtime would fail here, which is why the file is touched.
    touch "$work/t/src/a.txt"
    [ "$(tree_hash "$work/t")" = "$baseline" ] \
        && ok "an untouched tree hashes the same, and a newer mtime is not a change" \
        || bad "the hash moved without a byte moving"

    # Control: three ways a tree fails to come back, each of which the
    # `reversible` task can produce and each of which must be caught.
    printf 'ONE\n' > "$work/t/src/a.txt"
    [ "$(tree_hash "$work/t")" != "$baseline" ] \
        && ok "a changed byte is not a restored tree" || bad "a changed byte hashed as unchanged"
    printf 'one\n' > "$work/t/src/a.txt"
    [ "$(tree_hash "$work/t")" = "$baseline" ] \
        && ok "putting the byte back is a restored tree" || bad "an actual restore did not hash equal"

    printf 'left over\n' > "$work/t/src/a.txt.bak"
    [ "$(tree_hash "$work/t")" != "$baseline" ] \
        && ok "a file left behind is not a restored tree" || bad "a new file hashed as unchanged"
    rm -f "$work/t/src/a.txt.bak"

    rm -f "$work/t/src/b.txt"
    [ "$(tree_hash "$work/t")" != "$baseline" ] \
        && ok "a deleted file is not a restored tree" || bad "a deletion hashed as unchanged"
    printf 'two\n' > "$work/t/src/b.txt"

    # And the exclusions, which are the part that could silently make arm A
    # fail a restore it did perform.
    printf 'rewritten by git status\n' > "$work/t/.git/index"
    printf 'a fresh build\n' > "$work/t/target/o"
    mkdir -p "$work/t/node_modules"; printf 'x\n' > "$work/t/node_modules/p"
    [ "$(tree_hash "$work/t")" = "$baseline" ] \
        && ok ".git, target and node_modules are outside the question" \
        || bad "something outside the workspace's content moved the hash"

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
        tree_hash "$work/project" > "$bench/armB.before"

        # Not hashed yet: NOT PROVEN, and a non-zero exit when demanded.
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

# ── arm A: an ordinary Linux copy, ordinary tools ────────────────────────────
if [[ "$ARMS" == *A* ]]; then
    rm -rf "$OUT/a"
    mkdir -p "$OUT/a"
    tar -C "$PROJECT" --exclude=./target --exclude=./node_modules -cf - . | tar -C "$OUT/a" -xf -
    tree_hash "$OUT/a" > "$OUT/armA.before"
    run_arm A "$OUT/a" \
        --allowedTools "Read" "Edit" "Write" "Grep" "Glob" "Bash" \
        --strict-mcp-config
    tree_hash "$OUT/a" > "$OUT/armA.after"
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
    tree_hash "$PROJECT" > "$OUT/armB.before"
    # An empty directory, so that nothing on this host is reachable even by
    # accident. Everything arm B can see is inside the machine.
    rm -rf "$OUT/b"; mkdir -p "$OUT/b"
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
    tree_hash "$RESTORED_B" > "$OUT/armB.after"
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
# Not `set -e`'s business: the summary exits non-zero when a requirement the
# caller *demanded* came back NOT PROVEN, and the paths and the next command
# below are exactly what somebody in that situation needs to read.
SUMMARISED=0
python3 "$ROOT/dev/bench-summary.py" "${SUMMARY_ARGS[@]}" || SUMMARISED=$?

say
say "streams:  $OUT/armA.ndjson  $OUT/armB.ndjson"
say "summary:  $OUT/summary.json"

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
say "One run is an anecdote. What this proves is that the comparison can be run."
exit $SUMMARISED
