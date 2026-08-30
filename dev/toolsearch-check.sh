#!/usr/bin/env bash
# Does `alwaysLoad` actually stop Claude Code from spending a turn on tool search?
#
#   dev/toolsearch-check.sh
#
# The compact surface's whole claim is that three tools cost less than fourteen.
# The run of 2026-08-30 spent its first inference on a `ToolSearch` before it
# could call any of the three — so the surface was small and the model still
# paid to find it. `alwaysLoad: true` in the MCP config is what stops that, and
# this is the measurement that says so.
#
# ## Why this is a script and not a note
#
# The claim is about a CLI this repository does not own, does not vendor and
# cannot pin. `alwaysLoad` arrived in a version, its meaning can change in the
# next one, and a sentence in a comment saying "measured on 2026-08-30" ages
# into a sentence about a program nobody has run since. This can be re-run.
#
# ## What it measures, and against what
#
# Rule 4. A run that shows no `ToolSearch` proves nothing on its own — a model
# that never needed the tool would look identical. So there are two arms over
# the *same* stub server and the *same* prompt, differing in one field:
#
#   control:   no alwaysLoad   →  expect ToolSearch before the tool
#   treatment: alwaysLoad      →  expect the tool, with no ToolSearch
#
# It is a stub and not `thalyx-mcp`, on purpose. What is under test is the
# client's loading behaviour, and a stub needs no machine, no socket and no
# Btrfs — so this answers the same question in this container that it answers on
# the hardware. The stub advertises the same three names for one reason only:
# so that the sequence printed here reads like the sequence in a real run.
#
# **It costs two small inferences.** Nothing else here does, which is why it is
# not wired into `verify.sh` or into the harness self-test; the self-test checks
# that the config the run writes still carries the flag this measured.
set -euo pipefail

command -v claude >/dev/null 2>&1 || { echo "  no claude on this host"; exit 1; }
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "  claude $(claude --version)"

cat > "$WORK/stub.py" <<'STUB'
import json, sys
TOOLS = [
    {"name": n, "description": "stands in for the real one",
     "inputSchema": {"type": "object", "properties": {}}}
    for n in ("thalyx_context", "thalyx_exec", "thalyx_evidence")
]
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if "id" not in message:
        continue
    method = message.get("method")
    if method == "initialize":
        result = {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}},
                  "serverInfo": {"name": "stub", "version": "0"},
                  "instructions": "A stub standing in for a Thalyx machine."}
    elif method == "tools/list":
        result = {"tools": TOOLS}
    else:
        result = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": result}) + "\n")
    sys.stdout.flush()
STUB

# One prompt for both arms, for the same reason the benchmark has one prompt.
PROMPT="Call the thalyx_exec tool once with no arguments, then stop."

arm() {
    local name="$1" always="$2"
    if [ "$always" = yes ]; then
        printf '{"mcpServers":{"thalyx":{"type":"stdio","command":"python3","alwaysLoad":true,"args":["%s"]}}}\n' \
            "$WORK/stub.py" > "$WORK/$name.json"
    else
        printf '{"mcpServers":{"thalyx":{"type":"stdio","command":"python3","args":["%s"]}}}\n' \
            "$WORK/stub.py" > "$WORK/$name.json"
    fi
    ( cd "$WORK" && timeout 300 claude -p "$PROMPT" \
        --mcp-config "$WORK/$name.json" --strict-mcp-config \
        --allowedTools "mcp__thalyx" \
        --output-format stream-json --verbose --max-turns 6 \
        < /dev/null ) > "$WORK/$name.ndjson" 2> "$WORK/$name.err" || true
}

echo "  running the control (no alwaysLoad) …"
arm control no
echo "  running the treatment (alwaysLoad) …"
arm always yes

python3 - "$WORK/control.ndjson" "$WORK/always.ndjson" <<'READ'
import json, sys

def calls(path):
    """Every tool the model asked for, in order."""
    asked = []
    for line in open(path):
        try:
            event = json.loads(line)
        except ValueError:
            continue
        content = (event.get("message") or {}).get("content")
        if not isinstance(content, list):
            continue
        for block in content:
            if isinstance(block, dict) and block.get("type") == "tool_use":
                asked.append(block.get("name"))
    return asked

def searched_first(asked):
    """Whether a tool search came before the first thalyx call.

    `None` when no thalyx call happened at all, which is neither answer: a model
    that never reached the tool tells us nothing about how the tool was loaded.
    """
    for name in asked:
        if name == "ToolSearch":
            return True
        if name and "thalyx" in name:
            return False
    return None

control, always = calls(sys.argv[1]), calls(sys.argv[2])
print()
print(f"  control  (no alwaysLoad): {control}")
print(f"  treatment (alwaysLoad)  : {always}")
print()

trouble = []
if searched_first(control) is None:
    trouble.append("the control never called the tool, so it measured nothing")
elif not searched_first(control):
    # Not a failure of Thalyx. It means the client stopped deferring by default,
    # and the flag this exists to justify may no longer be buying anything.
    trouble.append("the control reached the tool WITHOUT a tool search: this build "
                   "does not defer MCP tools by default, so alwaysLoad is not what "
                   "is being measured any more")
if searched_first(always) is None:
    trouble.append("the treatment never called the tool, so it measured nothing")
elif searched_first(always):
    trouble.append("alwaysLoad did NOT stop the tool search: this build ignores the "
                   "flag, or the flag has changed meaning")

if trouble:
    print("  NOT PROVEN / FAILED")
    for why in trouble:
        print(f"    {why}")
    sys.exit(1)
print("  PROVEN  alwaysLoad removes the tool search that stands before the first call")
READ
