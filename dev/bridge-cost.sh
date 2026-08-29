#!/usr/bin/env bash
# How much of an agent's wall clock is the bridge, measured without paying for a
# model.
#
#   dev/bridge-cost.sh [--calls N]
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
set -u

CALLS=40
while [ $# -gt 0 ]; do
    case "$1" in
        --calls) CALLS="$2"; shift 2 ;;
        *) echo "usage: $0 [--calls N]" >&2; exit 2 ;;
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
script() {
    echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}'
    local n=1
    while [ "$n" -le "$CALLS" ]; do
        echo "{\"jsonrpc\":\"2.0\",\"id\":$n,\"method\":\"tools/call\",\"params\":{\"name\":\"thalyx_state\",\"arguments\":{}}}"
        n=$((n + 1))
        echo "{\"jsonrpc\":\"2.0\",\"id\":$n,\"method\":\"tools/call\",\"params\":{\"name\":\"thalyx_list\",\"arguments\":{\"path\":\"src\"}}}"
        n=$((n + 1))
        echo "{\"jsonrpc\":\"2.0\",\"id\":$n,\"method\":\"tools/call\",\"params\":{\"name\":\"thalyx_symbol\",\"arguments\":{\"name\":\"SlotTable\"}}}"
        n=$((n + 1))
    done
}

began=$(date +%s.%N)
script | "$MCP" --connect "$SOCKET" --metrics "$METRICS" --wait 20 >/dev/null 2>"$WORK/stderr"
ended=$(date +%s.%N)

if [ ! -s "$METRICS" ]; then
    echo "no metrics were written; thalyx-mcp said:" >&2
    cat "$WORK/stderr" >&2
    exit 1
fi

python3 - "$METRICS" "$began" "$ended" <<'PY'
import json, sys

metrics = json.load(open(sys.argv[1]))
outside = float(sys.argv[3]) - float(sys.argv[2])

requests = metrics.get("machine_requests", 0)
machine = metrics.get("machine_seconds", 0.0)
wall = metrics.get("wall_seconds", 0.0)
calls = metrics.get("mcp_calls", 0)

if not requests:
    print("NOT PROVEN: no request reached the machine.")
    raise SystemExit(1)

adapter = max(wall - machine, 0.0)
print()
print(f"  {calls} tool calls, {requests} questions to the machine")
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
