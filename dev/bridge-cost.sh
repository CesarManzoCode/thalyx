#!/usr/bin/env bash
# How much of an agent's wall clock is the bridge, measured without paying for a
# model.
#
#   dev/bridge-cost.sh [--calls N] [--surface legacy|compact]
#
# ## The question
#
# `vault/07-Adopcion-y-Fases/Evidencia-de-Agentes.md`: across the three real runs
# of the reversible benchmark, arm B's total wall clock exceeds the API time the
# agent reports by roughly six seconds, fairly steadily. Six seconds is not
# nothing when the whole task is under a minute, and nothing in the benchmark's
# own numbers says where it goes — the harness sees the model's clock and the
# machine's answers, and the path between them is unmeasured.
#
# So this measures that path, on this host, with no model and no API:
#
#     thalyx-mcp  →  UNIX socket  →  thalyx bridge serve  →  session  →  answer
#
# ## What it can and cannot separate
#
# It reports two numbers per request and it is honest about which is which:
#
#   machine_seconds   everything between the request leaving thalyx-mcp and the
#                     answer arriving — framing, the socket, and the verb's own
#                     work, together. Splitting those needs an instrument inside
#                     the machine; a number claiming to have split them from out
#                     here would be a guess with a measurement's name.
#
#   wall - machine    this adapter's own cost: reading a JSON-RPC line, composing
#                     the verbs, serialising the answer.
#
# **What is missing here is QEMU and virtio**, and that is the point of running
# it: the benchmark's machine is the same code with a virtio-serial port where
# this has a UNIX socket. Whatever this reports is the floor, and the difference
# between it and the same numbers on Cesar's machine is what the guest hop
# costs. `thalyx-mcp --metrics` writes these fields on every real run too, so
# the next benchmark answers the same question about itself for free.
#
# ## What it is not
#
# It is not a benchmark result and it is not a claim about a model's behaviour.
# It is one host, one process pair, and a fixed script of calls.
#
# ## Why it asks for the legacy surface
#
# On 2026-08-30 thalyx-mcp's default surface became three tools — `thalyx_context`,
# `thalyx_exec`, `thalyx_evidence` — and this script went on asking for
# `thalyx_state`, `thalyx_list` and `thalyx_symbol`. Nothing errored: an unknown
# tool is refused before it reaches the machine, so the run made **zero**
# requests, wrote a metrics file full of zeroes, and the stage that runs it
# reported NOT PROVEN with no number in it. A measurement that quietly stops
# measuring looks exactly like a machine that got slower and nobody would know
# which.
#
# The fix is not to put fourteen tools back on the default surface. What is
# under measurement here is the **wire** — framing, the socket, the machine's
# answer — and the three verbs that isolate the wire are the cheapest read-only
# ones there are: they do almost no work, so almost all of what is left is
# transport. Those three are on the legacy surface now, and the legacy surface
# exists precisely so a measurement can still reach them. So the script asks for
# it by name.
#
# `--surface compact` runs the same script against the default surface instead.
# It is the honest thing to point at the day somebody wants to know what an
# agent's actual tools cost — and it will refuse rather than report zeroes,
# because of the assertion below.
#
# ## And the assertion that makes it fail instead of go quiet
#
# Every call is counted and compared with the number sent. A tool the surface
# does not offer, a verb the machine has dropped, a socket that answered half
# the script: all of them now stop this with a message naming what did not
# arrive. Rule 5 — the instrument includes the harness, and this harness spent a
# day reporting a bridge it had never spoken to.
set -u

CALLS=40
SURFACE=legacy
while [ $# -gt 0 ]; do
    case "$1" in
        --calls) CALLS="$2"; shift 2 ;;
        --surface) SURFACE="$2"; shift 2 ;;
        *) echo "usage: $0 [--calls N] [--surface legacy|compact]" >&2; exit 2 ;;
    esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
THALYX="$ROOT/target/release/thalyx"
MCP="$ROOT/target/release/thalyx-mcp"

# The release profile, because it is the one the benchmark runs. A transport
# measured on a debug build measures the debug build.
for pair in "thalyx:thalyx-cli" "thalyx-mcp:thalyx-mcp"; do
    binary="$ROOT/target/release/${pair%%:*}"
    [ -x "$binary" ] || ( cd "$ROOT" && cargo build --release -p "${pair##*:}" >/dev/null ) || {
        echo "could not build ${pair##*:}" >&2
        exit 1
    }
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

STORE="$WORK/store"
TREE="$WORK/tree"
SOCKET="$WORK/agent.sock"
METRICS="$WORK/metrics.json"

mkdir -p "$TREE/src"
cat >"$TREE/src/slots.rs" <<'RUST'
/// Every slot this machine has given away.
pub struct SlotTable {
    next: u32,
}

impl SlotTable {
    pub fn new() -> SlotTable {
        SlotTable { next: 1 }
    }
}
RUST

"$THALYX" --root "$STORE" bridge --listen "$SOCKET" --workspace "$TREE" &
SERVER=$!
trap 'kill "$SERVER" 2>/dev/null; rm -rf "$WORK"' EXIT

# Waited for rather than slept on: a fixed sleep is either a slow script or a
# flaky one, and this is measuring time.
for _ in $(seq 1 200); do
    [ -S "$SOCKET" ] && break
    sleep 0.05
done
[ -S "$SOCKET" ] || { echo "the bridge never listened at $SOCKET" >&2; exit 1; }

# One JSON-RPC line per message. The mix is the read-only half of what an agent
# does — nothing here changes the tree, so the script can be repeated as many
# times as asked without the later calls measuring a different workspace from
# the earlier ones.
#
# On the compact surface the same three intentions are one `thalyx_exec`
# program, which is the point of that surface — and which is why it is the
# other column rather than a drop-in: an `exec` opens a reversible boundary, so
# on a host whose workspace is not a Btrfs subvolume it is refused before any of
# it happens, and the number would be the cost of a refusal wearing a
# measurement's name.
script() {
    echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}'
    local n=1
    while [ "$n" -le "$CALLS" ]; do
        if [ "$SURFACE" = compact ]; then
            echo "{\"jsonrpc\":\"2.0\",\"id\":$n,\"method\":\"tools/call\",\"params\":{\"name\":\"thalyx_exec\",\"arguments\":{\"label\":\"look\",\"run\":\"const s = thalyx.state(); const l = thalyx.list('src'); return l.entries.length;\"}}}"
            n=$((n + 1))
        else
            echo "{\"jsonrpc\":\"2.0\",\"id\":$n,\"method\":\"tools/call\",\"params\":{\"name\":\"thalyx_state\",\"arguments\":{}}}"
            n=$((n + 1))
            echo "{\"jsonrpc\":\"2.0\",\"id\":$n,\"method\":\"tools/call\",\"params\":{\"name\":\"thalyx_list\",\"arguments\":{\"path\":\"src\"}}}"
            n=$((n + 1))
            echo "{\"jsonrpc\":\"2.0\",\"id\":$n,\"method\":\"tools/call\",\"params\":{\"name\":\"thalyx_symbol\",\"arguments\":{\"name\":\"SlotTable\"}}}"
            n=$((n + 1))
        fi
    done
}

# How many `tools/call` lines the script above will send. Counted from the
# script itself rather than from arithmetic written twice, because the number
# this is compared against is the whole guard.
SENT="$(script | grep -c '"method":"tools/call"')"

began=$(date +%s.%N)
script | "$MCP" --surface "$SURFACE" --connect "$SOCKET" --metrics "$METRICS" --wait 20 \
    >"$WORK/answers" 2>"$WORK/stderr"
ended=$(date +%s.%N)

# What the answers actually said, which the metrics do not.
#
# A Thalyx refusal is a *value*: `{"ok": false, "error": …}` comes back as a
# successful tool call, and thalyx-mcp counts it as one — correctly, since from
# the adapter's side it is. That is fine for an agent and useless for a
# measurement: on this host `thalyx_exec` answers `not_a_subvolume` in a fifth of
# a millisecond, and a stage reading only the metrics would print that as the
# cost of running a program. Rule 4, and the control is right here in the reply.
REFUSED="$(grep -c '\\"ok\\":false' "$WORK/answers" || true)"

if [ ! -s "$METRICS" ]; then
    echo "no metrics were written; thalyx-mcp said:" >&2
    cat "$WORK/stderr" >&2
    exit 1
fi

python3 - "$METRICS" "$began" "$ended" "$SENT" "$SURFACE" "$REFUSED" "$WORK/answers" <<'PY'
import json, sys

metrics = json.load(open(sys.argv[1]))
outside = float(sys.argv[3]) - float(sys.argv[2])
sent = int(sys.argv[4])
surface = sys.argv[5]
refused = int(sys.argv[6])
answers = sys.argv[7]

requests = metrics.get("machine_requests", 0)
machine = metrics.get("machine_seconds", 0.0)
wall = metrics.get("wall_seconds", 0.0)
calls = metrics.get("mcp_calls", 0)

if not requests:
    print(f"FAILED: {sent} tool calls were sent on the {surface} surface and none of "
          f"them reached the machine. Tools offered: "
          f"{sorted(metrics.get('tools_used', {})) or 'none of the ones asked for'}.")
    raise SystemExit(1)

# The silent half of the same failure: some of them landed and some were
# refused before the wire, which would make every per-request number below an
# average over a denominator nobody chose.
if calls != sent:
    print(f"FAILED: {sent} tool calls were sent on the {surface} surface and the "
          f"adapter counted {calls}. A measurement over a mix that changed under it "
          f"is not a measurement.")
    raise SystemExit(1)

errors = metrics.get("errors", 0)
if errors:
    print(f"FAILED: {errors} of {calls} calls on the {surface} surface came back as "
          f"errors, so what is timed below is partly the cost of being refused.")
    raise SystemExit(1)

if refused:
    why = ""
    for line in open(answers):
        if '\\"ok\\":false' in line:
            try:
                inner = json.loads(json.loads(line)["result"]["content"][0]["text"])
            except Exception:
                break
            why = f' It said: {inner.get("error")} — {inner.get("message", "")}'
            break
    print(f"FAILED: {refused} of {calls} calls on the {surface} surface were answered "
          f"with a refusal, so the time below is the cost of being told no.{why}")
    raise SystemExit(1)

adapter = max(wall - machine, 0.0)
print()
print(f"  {calls} tool calls on the {surface} surface, "
      f"{requests} questions to the machine")
print()
print(f"  in the machine   {machine * 1000:9.2f} ms   {machine / requests * 1000:7.3f} ms/request")
print(f"  in the adapter   {adapter * 1000:9.2f} ms   {adapter / requests * 1000:7.3f} ms/request")
print(f"  thalyx-mcp wall  {wall * 1000:9.2f} ms")
print(f"  process wall     {outside * 1000:9.2f} ms   (includes starting both processes)")
print()
print("  No QEMU and no virtio in this path: it is the floor, and the same two")
print("  fields on a real run say what the guest hop adds to it.")
print()
PY
