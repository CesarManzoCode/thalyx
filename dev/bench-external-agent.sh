#!/usr/bin/env bash
# The same agent, the same task, two machines. Which one works better?
#
#   dev/bench-external-agent.sh --project <dir> [--socket <sock>] [--task read|change]
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
        *) echo "  unknown argument: $1"; exit 1 ;;
    esac
done

say() { printf '  %s\n' "$*"; }

[ -n "$PROJECT" ] || { say "which project: --project <dir>"; exit 1; }
[ -d "$PROJECT" ] || { say "$PROJECT is not a directory"; exit 1; }
command -v claude >/dev/null 2>&1 || { say "no claude on this host"; exit 1; }
[ -n "$SYMBOL" ] || { say "which symbol the task is about: --symbol <Name>"; exit 1; }

MCP="$ROOT/target/release/thalyx-mcp"
[ -x "$MCP" ] || ( cd "$ROOT" && cargo build --release -p thalyx-mcp >/dev/null )

mkdir -p "$OUT"

# ── the two prompts, which are one prompt ────────────────────────────────────
#
# Identical wording between arms, with nothing in either naming a tool. A prompt
# that said "use thalyx_symbol" would be measuring whether the model can follow
# an instruction, and the question is whether it reaches for the primitive on the
# strength of the tool description alone.
case "$TASK" in
    read)
        PROMPT="Find where the symbol \`$SYMBOL\` is defined, what depends on it, and \
which files I would have to look at to change it. Answer with the definition site, the \
list of dependents, and the list of files to review."
        ;;
    change)
        PROMPT="Add a short doc comment (a Rust /// line) above the definition of \`$SYMBOL\` \
and above every function in the same file, then tell me which files you changed. Do not \
change any behaviour."
        ;;
    *) say "--task is read or change"; exit 1 ;;
esac

# A hash of every file in a tree, by content, in a stable order. Not a
# timestamp: the question the `change` task asks is whether the bytes came back,
# and mtimes never come back.
tree_hash() {
    ( cd "$1" && find . -type f -not -path './.git/*' -not -path './target/*' -print0 \
        | LC_ALL=C sort -z \
        | xargs -0 sha256sum 2>/dev/null \
        | sha256sum | cut -d' ' -f1 )
}

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
    # An empty directory, so that nothing on this host is reachable even by
    # accident. Everything arm B can see is inside the machine.
    rm -rf "$OUT/b"; mkdir -p "$OUT/b"
    run_arm B "$OUT/b" \
        --mcp-config "$OUT/mcp.json" --strict-mcp-config \
        --allowedTools "mcp__thalyx" \
        --disallowedTools "Read" "Edit" "Write" "Grep" "Glob" "Bash" "NotebookEdit" "WebFetch" "WebSearch" "Task"
fi

# ── the summary ──────────────────────────────────────────────────────────────
#
# A separate program, because a parser for somebody else's output that lives
# inside a shell script is a parser nobody can test without running the thing it
# parses. `dev/bench-summary.py --self-test` checks it against a captured real
# session, in a second, for free.
SUMMARY_ARGS=(--out "$OUT" --task "$TASK" --symbol "$SYMBOL" --model "$MODEL" --turns "$TURNS")
[ -n "$EXPECT" ] && SUMMARY_ARGS+=(--expect-file "$EXPECT")
python3 "$ROOT/dev/bench-summary.py" "${SUMMARY_ARGS[@]}"

say
say "streams:  $OUT/armA.ndjson  $OUT/armB.ndjson"
say "summary:  $OUT/summary.json"
say
say "One run is an anecdote. What this proves is that the comparison can be run."
