#!/usr/bin/env bash
# Point a programming agent at a running Thalyx machine.
#
#   dev/agent-connect.sh [socket]
#
# `vault/07-Adopcion-y-Fases/Agentes-Externos.md`. The machine has to be up —
# `make -C image run-agent` — and this connects to the channel QEMU is holding
# open for it.
#
# What it does, in this order and stopping at the first thing that is wrong:
#
#   1. builds `thalyx-mcp` for **this host**, not for the image. The image's
#      binary is a static musl build for the guest, and pointing an agent at it
#      is the kind of mistake that reads as "MCP is broken";
#   2. proves the machine is actually there and answering, by connecting and
#      reading its hello — before anything is registered, so that a machine that
#      is not up is one clear sentence rather than a tool that fails later;
#   3. registers the server with Claude Code, and writes a `.vscode/mcp.json`
#      beside it for the same server.
#
# There is exactly one server. VS Code, Claude Code and any other MCP client
# load the same binary and see the same tools — MCP is the boundary, and a
# second integration per client would be the thing the decree says not to build.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOCKET="${1:-$ROOT/image/build/agent.sock}"
MCP="$ROOT/target/release/thalyx-mcp"
METRICS="${THALYX_AGENT_METRICS:-}"

say() { printf '  %s\n' "$*"; }

if [ ! -S "$SOCKET" ]; then
    say "there is no agent channel at $SOCKET."
    say
    say "The machine has to be running, and running with the channel:"
    say
    say "    make -C image agent PROJECT=/path/to/your/project"
    say
    say "or, if the store already has a workspace on it:"
    say
    say "    make -C image run-agent"
    exit 1
fi

say "building thalyx-mcp for this host"
( cd "$ROOT" && cargo build --release -p thalyx-mcp >/dev/null )

# Asked before anything is registered. A client configured against a machine
# that is not answering fails on the model's first tool call, which reads as the
# model's mistake — and this is the one place that confusion can still be
# avoided cheaply.
say "asking the machine what it is"
if ! HELLO="$(printf '' | timeout 10 "$MCP" --connect "$SOCKET" 2>&1 >/dev/null)"; then
    printf '%s\n' "$HELLO"
    say
    say "The socket is there and the machine did not say hello. Either it is"
    say "still booting, or it came up without a workspace — its own boot report"
    say "says which, on the QEMU console."
    exit 1
fi
printf '%s\n' "$HELLO" | sed 's/^thalyx-mcp: /  /'

# ── Claude Code ──────────────────────────────────────────────────────────────
#
# `claude mcp add` writes into the user's own configuration, so it is `--scope
# local` — this is one experiment on one machine, and a server registered
# globally would follow the person into every other project they open.
if command -v claude >/dev/null 2>&1; then
    claude mcp remove thalyx --scope local >/dev/null 2>&1 || true
    if [ -n "$METRICS" ]; then
        claude mcp add thalyx --scope local -- "$MCP" --connect "$SOCKET" --metrics "$METRICS"
    else
        claude mcp add thalyx --scope local -- "$MCP" --connect "$SOCKET"
    fi
    say 'registered with Claude Code as thalyx'
else
    say 'no claude on this host, so nothing was registered with Claude Code.'
    say "The command, when there is one:"
    say
    say "    claude mcp add thalyx -- $MCP --connect $SOCKET"
fi

# ── VS Code, and anything else that speaks MCP ───────────────────────────────
#
# The same binary and the same arguments. Written next to the project rather
# than into the user's settings, so that opening this folder is what turns it
# on and closing it is what turns it off.
mkdir -p "$ROOT/.vscode"
cat > "$ROOT/.vscode/mcp.json" <<JSON
{
  "servers": {
    "thalyx": {
      "type": "stdio",
      "command": "$MCP",
      "args": ["--connect", "$SOCKET"]
    }
  }
}
JSON
say "wrote $ROOT/.vscode/mcp.json — VS Code loads the same server"

say
say "Now, in a terminal:"
say
say "    claude"
say
say 'and ask it something about the project. /mcp lists what it can see.'
