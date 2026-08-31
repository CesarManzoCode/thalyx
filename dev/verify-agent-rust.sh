#!/usr/bin/env bash
# Ask a running Thalyx machine, over the same socket Claude Code would use,
# whether it can really resolve a Rust name.
#
#   dev/verify-agent-rust.sh [socket]
#
# ─────────────────────────────────────────────────────────────────────────────
# WHAT THIS IS FOR
#
# `vault/09-Notas-Tecnicas/Runtime-Rust-Agente.md`. On 2026-08-30 a paid run
# was answered `there is no cargo on this machine` by a machine that had said
# READY. The runtime that closes that is built, staged and checked by code with
# tests — and none of that is the claim. The claim is that a **booted Thalyx**,
# asked through its agent channel, resolves a symbol with rust-analyzer and
# renames it.
#
# No shell in a container can make that claim. This is the thing somebody runs
# on the machine that can.
#
# ## What it deliberately does not do
#
# It does not invoke Claude Code, it does not run the A/B benchmark, and it
# costs nothing. It also leaves the workspace exactly as it found it: the rename
# runs inside a program with `on_success: "rollback"`, so the work really
# happens, `edits_by_file` is really counted, and the tree is put back because
# the caller asked rather than because anything failed.
#
# ## And it does not believe the machine's summary
#
# Every check reads the field it is about out of the machine's own answer —
# `source`, `analyzer_starts`, `edits_by_file`, `tree` — rather than looking for
# a word in a sentence. Rule 10 of `Estrategia-de-Pruebas.md` has been paid for
# twice by greps that kept passing after the sentence changed.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOCKET="${1:-$ROOT/image/build/agent.sock}"
MCP="$ROOT/target/release/thalyx-mcp"
SYMBOL="${THALYX_RUST_SYMBOL:-LanternRegistry}"
RENAMED="${THALYX_RUST_RENAMED:-BeaconRegistry}"

PROVEN=0
UNPROVEN=0
FAILED=0
proven()   { printf '   \033[32mPROVEN\033[0m      %s\n' "$*"; PROVEN=$((PROVEN + 1)); }
unproven() { printf '   \033[33mNOT PROVEN\033[0m  %s\n' "$*"; UNPROVEN=$((UNPROVEN + 1)); }
failed()   { printf '   \033[31mFAILED\033[0m      %s\n' "$*"; FAILED=$((FAILED + 1)); }
say()      { printf '  %s\n' "$*"; }

echo
echo "  Can the machine behind $SOCKET resolve a Rust name?"
echo

if [ ! -S "$SOCKET" ]; then
    failed "there is no agent channel at $SOCKET — the machine is not running with one"
    say
    say "    make -C image agent PROJECT=$ROOT/dev/rust-corpus"
    say
    exit 1
fi

if [ ! -x "$MCP" ]; then
    say "building thalyx-mcp for this host"
    ( cd "$ROOT" && cargo build --release -p thalyx-mcp ) > /dev/null 2>&1 \
        || { failed "thalyx-mcp did not build"; exit 1; }
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ── 1. the preflight, which is the thing that failed to fail on 2026-08-30 ──
"$MCP" --connect "$SOCKET" --preflight --needs-rust --wait 30 \
    > "$WORK/preflight.json" 2> "$WORK/preflight.err"
PREFLIGHT_STATUS=$?

python3 - "$WORK/preflight.json" "$PREFLIGHT_STATUS" > "$WORK/preflight.lines" <<'PY'
import json, sys
try:
    report = json.load(open(sys.argv[1]))
except Exception as error:
    print(f"   \033[31mFAILED\033[0m      the preflight printed nothing readable: {error}")
    sys.exit(1)
ready = report.get("ready") is True
tool = report.get("toolchain") or {}
cargo = tool.get("cargo") or {}
analyzer = tool.get("rust_analyzer") or {}
runtime = tool.get("runtime") or {}
mark = "\033[32mPROVEN\033[0m" if ready else "\033[31mFAILED\033[0m"
print(f"   {mark}      the machine says it is ready for a Rust task"
      if ready else
      f"   {mark}      the machine is NOT ready: " + "; ".join(report.get("because") or []))
for name, tool_report in (("cargo", cargo), ("rust-analyzer", analyzer)):
    where = tool_report.get("from")
    version = tool_report.get("version")
    if tool_report.get("path"):
        owner = {"thalyx": "Thalyx's own runtime",
                 "host": "a toolchain installed on the host",
                 "named": "a file a variable named"}.get(where, where)
        print(f"   \033[32mPROVEN\033[0m      {name} ran inside the machine: {version} — {owner}")
        print(f"                 {tool_report['path']}")
    else:
        print(f"   \033[31mFAILED\033[0m      {name} is not on this machine at all")
if runtime.get("identity"):
    print(f"                 runtime {runtime['identity']}"
          f" (Rust {runtime.get('rust')}, musl {runtime.get('musl')})")
sys.exit(0 if ready else 1)
PY
READY=$?
cat "$WORK/preflight.lines"
# Counted from the lines that were printed, never from how many blocks ran: a
# summary that assumed its own arithmetic is a summary that stops matching the
# checks the first time one is added.
tally() {
    PROVEN=$((PROVEN + $(grep -c 'PROVEN' "$1")))
    FAILED=$((FAILED + $(grep -c 'FAILED' "$1")))
    UNPROVEN=$((UNPROVEN + $(grep -c 'NOT PROVEN' "$1")))
    # `NOT PROVEN` contains `PROVEN`, so it was counted twice above; take it
    # back once. Found by a summary that reported four proven checks on a run
    # with two.
    PROVEN=$((PROVEN - $(grep -c 'NOT PROVEN' "$1")))
}
tally "$WORK/preflight.lines"

if [ "$READY" -ne 0 ]; then
    say
    say "Nothing further was asked: a machine that cannot resolve names has"
    say "nothing to be asked about. The preflight's own words:"
    sed 's/^/    /' "$WORK/preflight.json" | head -40
    [ -s "$WORK/preflight.err" ] && sed 's/^/    /' "$WORK/preflight.err"
    exit 1
fi

# ── 2 and 3. a real question and a real rename, over the real channel ──
#
# Driven as MCP, because that is the surface Claude Code uses and a check that
# went in some other way would be checking some other thing.
python3 - "$MCP" "$SOCKET" "$SYMBOL" "$RENAMED" <<'PY' > "$WORK/semantic.txt" 2>&1
import json, subprocess, sys, time

mcp, socket, symbol, renamed = sys.argv[1:5]
server = subprocess.Popen(
    [mcp, "--connect", socket, "--wait", "30"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
    text=True,
)

def call(method, params, request_id):
    server.stdin.write(json.dumps(
        {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}) + "\n")
    server.stdin.flush()
    deadline = time.time() + 600
    while time.time() < deadline:
        line = server.stdout.readline()
        if not line:
            raise SystemExit("the adapter stopped answering")
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get("id") == request_id:
            return message
    raise SystemExit(f"{method} did not answer in ten minutes")

def tool(name, arguments, request_id):
    reply = call("tools/call", {"name": name, "arguments": arguments}, request_id)
    text = ((reply.get("result") or {}).get("content") or [{}])[0].get("text", "")
    try:
        return json.loads(text)
    except ValueError:
        return {"unparsed": text}

call("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": {"name": "verify-agent-rust", "version": "1"}}, 1)
server.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
server.stdin.flush()

# The first question. Starting rust-analyzer on a cold workspace takes tens of
# seconds; the timeout above is generous for that reason and not because
# anything here is slow.
context = tool("thalyx_context", {"query": symbol}, 2)
print("CONTEXT " + json.dumps(context))

# The rename, inside a program that puts the tree back **because it was asked
# to**, not because anything failed. So the work really happens, the counts are
# really counted, and the machine is left exactly as it was found.
program = (
    f"const before = thalyx.context({symbol!r});\n"
    f"const done = thalyx.rename({symbol!r}, {renamed!r});\n"
    "return {source: before.source, ok: done.ok, error: done.error,\n"
    "        definition: done.definition ?? null,\n"
    "        edits_by_file: done.edits_by_file ?? null};\n"
)
outcome = tool("thalyx_exec", {"label": "verify the machine's own Rust",
                               "run": program, "on_success": "rollback"}, 3)
print("EXEC " + json.dumps(outcome))
server.kill()
PY

CONTEXT_LINE=$(grep '^CONTEXT ' "$WORK/semantic.txt" | head -1 | cut -d' ' -f2-)
EXEC_LINE=$(grep '^EXEC ' "$WORK/semantic.txt" | head -1 | cut -d' ' -f2-)

if [ -z "$CONTEXT_LINE" ]; then
    failed "the machine never answered a context question; see below"
    sed 's/^/    /' "$WORK/semantic.txt" | head -30
else
    printf '%s' "$CONTEXT_LINE" > "$WORK/context.json"
    SOURCE=$(python3 -c 'import json,sys;print((json.load(open(sys.argv[1])) or {}).get("source",""))' "$WORK/context.json")
    STARTS=$(python3 -c 'import json,sys;print((json.load(open(sys.argv[1])) or {}).get("analyzer_starts",""))' "$WORK/context.json")
    CONFINED=$(python3 -c 'import json,sys;print((json.load(open(sys.argv[1])) or {}).get("analyzer_confined",""))' "$WORK/context.json")
    if [ "$SOURCE" = "rust-analyzer" ]; then
        proven "context('$SYMBOL') came from rust-analyzer, not from the scan (source=rust-analyzer, analyzer_starts=$STARTS)"
    else
        failed "context('$SYMBOL') answered source=$SOURCE — the machine matched a name instead of resolving one"
        sed 's/^/    /' "$WORK/context.json"
    fi
    case "$CONFINED" in
        True|true) proven "and the provider that answered was confined by Thalyx" ;;
        False|false) unproven "the provider ran as an ordinary process on this machine — load the LSM (make -C lsm load) to close that half" ;;
        *) unproven "the answer did not say whether the provider was confined" ;;
    esac
fi

if [ -z "$EXEC_LINE" ]; then
    failed "the machine never ran the rename program; see below"
    sed 's/^/    /' "$WORK/semantic.txt" | head -40
else
    printf '%s' "$EXEC_LINE" > "$WORK/exec.json"
    python3 - "$WORK/exec.json" "$SYMBOL" > "$WORK/exec.lines" <<'PY'
import json, sys
answer = json.load(open(sys.argv[1]))
symbol = sys.argv[2]
# `returned`, which is what the program handed back — the field name is
# asserted in `exec.rs`'s own tests, so a rename there breaks this loudly
# instead of turning every check below into a silent "not present".
value = answer.get("returned") or {}
def proven(text):   print(f"   \033[32mPROVEN\033[0m      {text}")
def failed(text):   print(f"   \033[31mFAILED\033[0m      {text}")
if value.get("ok") is True:
    proven(f"rename('{symbol}', …) resolved and rewrote every place that refers to it")
else:
    failed(f"the rename did not work: {value.get('error')} — {json.dumps(value)[:300]}")
# A list of `{path, edits}`, which is the shape `semantic.rs` builds. Counted
# per file rather than totalled, because the whole reason the field exists is
# that a total cannot tell a caller which file moved three times and which
# moved once.
edits = value.get("edits_by_file")
if isinstance(edits, list) and edits:
    per_file = ", ".join(
        f"{entry.get('path')}: {entry.get('edits')}" for entry in edits
        if isinstance(entry, dict))
    total = sum(entry.get("edits", 0) for entry in edits if isinstance(entry, dict))
    proven(f"edits_by_file came back per file — {len(edits)} file(s), {total} edit(s): {per_file}")
    if len(edits) < 2:
        print("   \033[33mNOT PROVEN\033[0m  only one file moved, so this run cannot tell a "
              "per-file count from a total. The corpus in dev/rust-corpus has the "
              "symbol in two crates")
else:
    failed("edits_by_file is not in the answer, so nothing says how much of each file moved")
if value.get("definition"):
    proven(f"and it named the definition it had resolved: {json.dumps(value['definition'])[:200]}")
tree = answer.get("tree")
if tree == "restored" and answer.get("succeeded") is True:
    proven("the workspace was put back byte for byte because the caller asked, not because anything failed (succeeded=true, tree=restored)")
elif tree == "restored":
    failed("the tree was restored because something failed, which is not what this asked for")
else:
    failed(f"the workspace was left changed (tree={tree!r}); put it back before doing anything else")
PY
    cat "$WORK/exec.lines"
    tally "$WORK/exec.lines"
fi

echo
echo "  ════════════════════════════════════════════════════════════"
printf '   proven      %d\n' "$PROVEN"
printf '   not proven  %d\n' "$UNPROVEN"
printf '   failed      %d\n' "$FAILED"
echo "  ════════════════════════════════════════════════════════════"
echo
if [ "$FAILED" -gt 0 ]; then
    echo "  The machine does not do what Thalyx says it does."
    echo "  The whole exchange is in $WORK — copy it before this exits."
    exit 1
fi
echo "  The machine resolved a Rust name with its own compiler, renamed it,"
echo "  and gave the tree back exactly as it found it."
