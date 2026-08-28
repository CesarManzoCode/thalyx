#!/usr/bin/env bash
#
# What an inference engine needs to run as a Thalyx module, measured.
#
# Cesar decreed on 2026-08-28 that the agent arrives on the machine with the
# **engine as the first real module**. Before any of that is built, one question
# can kill it: a module runs under `module_standard`, and if a real engine needs
# something that filter denies, the decree is unbuildable and the cheapest time
# to know is now.
#
# So this measures rather than argues. It is `dev/foreign-agent-needs.sh` — the
# same comparison, against the same allowlist, read out of the same function
# body — pointed at an engine instead of at an agent. One comparison and not
# two: two would be two answers to one question, and the version of that this
# project has already paid for is in rule 5.
#
# ## What it needs, and what it does instead of guessing
#
# A real engine and a real model. The engine is llama.cpp's `llama-completion`,
# which is what `crates/thalyx-agent/src/llama.rs` already drives. The model is
# built here, by llama.cpp's own `gguf-py`, because the alternative was a file
# this script's author invented and rule 6 says a fixture proves the format
# matches your model of it.
#
# It is a *tiny* model on purpose — two layers, 64 dimensions. What is being
# asked is which syscalls an engine makes, and a two-layer model makes the same
# ones as a seventy-billion-parameter one: mmap the weights, start a thread per
# core, tokenize, run the graph. What it does **not** answer is the size
# question, and that is said again at the bottom rather than left implied.
#
# Nothing here is skipped in silence. Every missing piece prints NOT PROVEN and
# names what would fix it.

set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(dirname "$HERE")"
WORK="${TMPDIR:-/tmp}/thalyx-engine-needs.$$"
mkdir -p "$WORK"
trap 'rm -rf "$WORK"' EXIT

say() { printf '  %s\n' "$*"; }

ENGINE="${THALYX_ENGINE:-$(command -v llama-completion || true)}"
SOURCE="${THALYX_LLAMA_CPP:-}"

if [ -z "$ENGINE" ]; then
    say "NOT PROVEN  no inference engine here."
    say "            Set THALYX_ENGINE to a llama-completion, or build one:"
    say "                git clone --depth 1 https://github.com/ggml-org/llama.cpp"
    say "                cmake -B build -DCMAKE_BUILD_TYPE=Release -DLLAMA_CURL=OFF"
    say "                cmake --build build --target llama-completion"
    exit 0
fi

# `gguf-py` ships with llama.cpp. Found next to the engine when it was built in
# a checkout, which is the usual case and saves saying where twice.
if [ -z "$SOURCE" ]; then
    GUESS="$(cd "$(dirname "$ENGINE")/../.." 2>/dev/null && pwd)"
    [ -d "${GUESS:-}/gguf-py" ] && SOURCE="$GUESS"
fi
if [ -z "$SOURCE" ] || [ ! -d "$SOURCE/gguf-py" ]; then
    say "NOT PROVEN  llama.cpp's gguf-py is not here, so no model can be built."
    say "            Set THALYX_LLAMA_CPP to a llama.cpp checkout."
    exit 0
fi
if ! python3 -c "import numpy" > /dev/null 2>&1; then
    say "NOT PROVEN  numpy is not here, and gguf-py needs it to write a model."
    say "            pip install numpy"
    exit 0
fi

printf '\nthe model\n─────────\n'
if ! python3 "$HERE/tiny-model.py" "$SOURCE" "$WORK/tiny.gguf" 2>&1 | sed 's/^/  /'; then
    say "the model could not be written; nothing was measured"
    exit 1
fi

# Proven to load *before* it is used to measure. Rule 5: the instrument includes
# the harness, and an engine that died on the model would trace as an engine
# that needs almost nothing.
if ! "$ENGINE" -m "$WORK/tiny.gguf" -p hola -n 8 --no-warmup -t 2 > "$WORK/ran" 2>&1; then
    say "the engine would not run this model, so the trace below would be of a"
    say "program failing rather than of one working:"
    tail -3 "$WORK/ran" | sed 's/^/      /'
    exit 1
fi
say "the engine loaded it and generated 8 tokens"

THALYX_FOREIGN_AGENT="$ENGINE" \
THALYX_FOREIGN_AGENT_ARG="-m $WORK/tiny.gguf -p hola -n 8 --no-warmup -t 2" \
    "$HERE/foreign-agent-needs.sh"

printf '\nwhat this does not answer, and it is the expensive half\n'
printf '──────────────────────────────────────────────────────\n'
say "- **size.** This model is under a megabyte. \`module_standard\` caps a"
say "  module at 1 GiB (\`profile.rs\`, \`memory_max\`), mmapped weights are"
say "  reclaimable page cache but the KV cache and compute buffers are not,"
say "  and no manifest can ask for more. That number is the open question."
say "- **the libc.** This engine is linked against glibc and the one that would"
say "  ship is static musl. Rule 12: a build with another configuration is"
say "  another system, and startup is exactly where the two libcs differ."
say "- three paths it opened are outside a module's root — the two under"
say "  /sys/devices/system/cpu and /dev/tty. Whether it *needs* them is not"
say "  something a trace can say."
