#!/usr/bin/env bash
#
# Put the engine module and its weights on the store stage.
#
# Called by `image/Makefile`'s `engine-stage`, and a script rather than a recipe
# because it is a page of shell with a manifest in the middle of it: a Makefile
# recipe of this size is one long line with tabs in it, which nobody can read
# and nobody can run by hand while something is going wrong.
#
# Three things go onto the stage, and the third is the one that makes the
# machine usable rather than merely equipped:
#
#   1. the weights, on the `modules` subvolume, at the path the machine sees;
#   2. the engine, packed into a signed .thmod and **installed**;
#   3. the agent's settings — tier, weights, and the module that runs them.
#
# Installed, where the greeter is deliberately left uninstalled. The greeter is
# step 2 of `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md` — a person
# installing a signed module — and a machine that booted with it already there
# would make that step unperformable. The engine is the opposite requirement:
# Cesar decreed on 2026-08-28 that the machine boots **able to be spoken to**,
# and an engine the human has to install first is a handful of commands standing
# between the power button and the first sentence. The bundle stays in the
# repository too, so it can be reinstalled by hand.
#
# Doing nothing when there is no engine or no model is correct and is not
# silent. A store with no agent is a supported machine —
# `vault/01-Filosofia/Principio-Doble-Ruta.md` is what makes that true — but a
# machine that has no agent because MODEL was misspelled looks exactly like one
# built that way on purpose, so this says which piece is missing and what fixes
# it.

set -uo pipefail

STAGE="$1"; BUILD="$2"; THALYX="$3"; ENGINE="$4"; MODEL="$5"
ENGINE_ID="$6"; TIER="$7"; MODELS_DIR="$8"; RUN_DIR="$9"

if [ -z "$MODEL" ] || [ ! -f "$MODEL" ] || [ ! -x "$ENGINE" ]; then
    echo
    echo "  no agent on this store, and here is exactly why:"
    if [ -x "$ENGINE" ]; then
        echo "    engine  $ENGINE"
    else
        echo "    engine  MISSING — make -C image engine"
    fi
    if [ -z "$MODEL" ]; then
        echo "    model   MISSING — make -C image store-stage MODEL=<file.gguf>"
    elif [ ! -f "$MODEL" ]; then
        echo "    model   $MODEL is not a file"
    else
        echo "    model   $MODEL"
    fi
    echo
    echo "  The machine still boots and every verb still works. It will say it"
    echo "  has no model when you talk to it in your own words."
    exit 0
fi

set -e
echo "  engine: $ENGINE"
echo "  model:  $MODEL"

# The ceiling follows the tier, because the weights are what fills it. A tier is
# chosen precisely to say how big a model this machine runs, so a fixed 4 GiB
# meant `ENGINE_TIER=media` produced a store whose engine was killed partway
# through loading its own weights — and the only fix was editing this file by
# hand before every build. Roughly three times the weights, which is the mmapped
# file plus its context, and page cache is charged to the module's cgroup.
case "$TIER" in
    ligera)          MEMORY="4GiB"  ;;
    media)           MEMORY="8GiB"  ;;
    alta)            MEMORY="16GiB" ;;
    maxima|máxima)   MEMORY="32GiB" ;;
    *)
        echo "  '$TIER' is not a tier. They are: ligera, media, alta, maxima" >&2
        exit 1
        ;;
esac
echo "  memory: $MEMORY ceiling for tier $TIER"

rm -rf "$BUILD/engine-pack"
mkdir -p "$BUILD/engine-pack/bin" "$STAGE/modules/engine/models" "$STAGE/modules/engine/run"
cp "$ENGINE" "$BUILD/engine-pack/bin/llama-completion"
cp "$MODEL" "$STAGE/modules/engine/models/model.gguf"

# The grants are directories rather than the two files, and that is deliberate.
# `$RUN_DIR` holds one throwaway directory per inference — the prompt and the
# grammar for that answer, named after that answer's marker — so there is no
# fixed file to name. `$MODELS_DIR` is a directory so that changing the weights
# is copying a file onto the store, which is what Cesar asked for: swapping the
# model must not mean rebuilding Thalyx.
cat > "$BUILD/engine-manifest.toml" <<TOML
format_version = 1
id             = "$ENGINE_ID"
name           = "llama.cpp"
version        = "1.0.0"
description    = "The inference engine: llama-completion, static, one process per answer"
license        = "MIT"
publisher_key  = "ed25519:0000000000000000000000000000000000000000000000000000000000000000"
distribution   = "prebuilt"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 0

[requires]
thalyx = ">=0.1.0"

[[permissions]]
resource = "$MODELS_DIR"
action   = "read"
type     = "persistent"

[[permissions]]
resource = "$RUN_DIR"
action   = "read"
type     = "persistent"

# The tier's ceiling and not the 1 GiB floor. \`module_standard\` charges page
# cache to the module's cgroup, and the weights are mmapped: the smallest tier
# is about 1.1 GB before any context at all, so the floor would kill the engine
# partway through loading. This is the ceiling a human approves once, at
# install, and it is enforced by \`memory.max\` rather than by the LSM — see
# \`Confinement::establish\`.
[[permissions]]
resource = "memory"
action   = "$MEMORY"
type     = "persistent"

[entrypoints]
run = "bin/llama-completion"
TOML

"$THALYX" dev pack "$BUILD/engine-pack" \
    --manifest "$BUILD/engine-manifest.toml" \
    --key "$BUILD/store-signing.key" \
    --out "$BUILD/engine.thmod"

mkdir -p "$STAGE/system/repo"
cp "$BUILD/engine.thmod" "$STAGE/system/repo/"

export THALYX_ROOT="$STAGE/system"
"$THALYX" module install "$BUILD/engine.thmod" --yes > /dev/null

# The path recorded is the one inside the machine, and the bytes measured are
# the ones on the stage. `--reading` exists for exactly this: the file is not
# yet where it will be, and a size recorded without reading anything would be
# the one thing `config.rs` refuses to do.
"$THALYX" agent model use "$TIER" \
    --weights "$MODELS_DIR/model.gguf" \
    --reading "$STAGE/modules/engine/models/model.gguf" \
    --module "$ENGINE_ID" > /dev/null

# Checked rather than assumed. An install that half-failed leaves a store that
# boots and cannot answer, and the person who finds that out is whoever booted
# it.
"$THALYX" module list | grep -q "$ENGINE_ID" \
    || { echo "  the engine did not install onto the stage"; exit 1; }

echo "  agent:  $TIER ▪ $ENGINE_ID, installed and configured"
