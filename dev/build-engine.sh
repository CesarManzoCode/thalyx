#!/usr/bin/env bash
#
# The inference engine, built the way a Thalyx module has to be built.
#
# Cesar decreed on 2026-08-28 that the engine arrives on the machine as the
# **first real module**: not inside `/init`, not reimplemented in Rust, not part
# of the TCB. `vault/02-Arquitectura/Gamas-de-Modelo.md` already said the engine
# is invoked rather than linked; this is what makes it something the operating
# system carries rather than something a developer happens to have on `PATH`.
#
# ## Why static, and why that is the only thing checked here
#
# There is no libc inside the machine. `make -C image count` says the image is
# the kernel and one program, and a module's root filesystem binds `/usr`,
# `/lib` and the rest **only if they exist on the host** — inside Thalyx they do
# not. A dynamically linked engine would therefore fail at `execve` with a
# missing interpreter, on the machine, in front of whoever booted it.
#
# So the one thing this script refuses to finish without is an ELF with no
# INTERP and no NEEDED. It is checked rather than assumed because rule 12 was
# paid for three weeks ago by exactly this shape of mistake: a binary verified
# in one configuration and shipped in another.
#
# ## The flags, and which of them are not optional
#
#   -DLLAMA_CURL=OFF        no network in Thalyx, and libcurl is dynamic
#   -DLLAMA_OPENSSL=OFF     cpp-httplib finds the system OpenSSL, which ships
#                           only as .so — the static link fails outright on it
#   -DGGML_OPENMP=OFF       libgomp is likewise .so only on most distributions
#   -DGGML_NATIVE=OFF       the machine that builds the store is not necessarily
#                           the machine that boots it. `-march=native` in a
#                           module is an illegal instruction on somebody else's
#                           CPU, and it arrives as a module that dies with no
#                           message rather than as a build problem.
#   -DBUILD_SHARED_LIBS=OFF static ggml and llama, not .so beside the binary
#   -static                 the whole point
#
# The first three were each found by a failed link, in that order. They are
# written down so the next person does not find them again.
#
# ## What gets built, since 2026-08-28: `thalyx-engine`, not `llama-completion`
#
# `llama-completion` is one-shot by construction, so a machine that was asked
# two things paid for loading the weights twice — several seconds of a local
# model's whole cost, spent again on work the previous sentence had already
# done. `engine/thalyx-engine.cpp` is the same llama.cpp at the same tag with
# the same flags, shaped as a program that loads the GGUF once and then answers
# framed requests on a pipe. It is copied into the checkout's `tools/` and built
# by the same configure, rather than linked by hand against the static archives:
# ggml's backend link order is fiddly and moves between tags, and getting it
# wrong is a link failure on somebody else's machine.
#
#   dev/build-engine.sh [output-directory]
#
# Leaves the checkout and the build under the directory it is given, so a second
# run is incremental. Prints the path to the binary on the last line.

set -euo pipefail

# Pinned. An engine built from whatever `master` was that afternoon is an engine
# nobody can rebuild, and the flags above are true of this tag because this tag
# is what they were found against.
REF="${THALYX_LLAMA_CPP_REF:-b10665}"
REPO="${THALYX_LLAMA_CPP_URL:-https://github.com/ggml-org/llama.cpp}"

WORK="${1:-${TMPDIR:-/tmp}/thalyx-engine}"
SOURCE="$WORK/llama.cpp"
BUILD="$SOURCE/build"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

say() { printf '  %s\n' "$*" >&2; }

for tool in cmake git; do
    command -v "$tool" > /dev/null || {
        say "no $tool. The engine is a C++ program and needs a C++ toolchain."
        exit 1
    }
done

mkdir -p "$WORK"
if [ ! -d "$SOURCE/.git" ]; then
    say "fetching llama.cpp $REF into $SOURCE"
    git clone --depth 1 --branch "$REF" "$REPO" "$SOURCE" >&2
fi

# Thalyx's own program, into llama.cpp's tree. Copied every run rather than
# once, so editing the engine and rebuilding does what anyone would expect.
mkdir -p "$SOURCE/tools/thalyx-engine"
cp "$HERE/engine/thalyx-engine.cpp" "$HERE/engine/CMakeLists.txt" "$SOURCE/tools/thalyx-engine/"
grep -q 'add_subdirectory(thalyx-engine)' "$SOURCE/tools/CMakeLists.txt" \
    || printf '\nadd_subdirectory(thalyx-engine)\n' >> "$SOURCE/tools/CMakeLists.txt"

GENERATOR=()
command -v ninja > /dev/null && GENERATOR=(-G Ninja)

say "configuring"
cmake -B "$BUILD" -S "$SOURCE" "${GENERATOR[@]}" \
    -DCMAKE_BUILD_TYPE=Release \
    -DLLAMA_CURL=OFF \
    -DLLAMA_OPENSSL=OFF \
    -DGGML_OPENMP=OFF \
    -DGGML_NATIVE=OFF \
    -DBUILD_SHARED_LIBS=OFF \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_SERVER=OFF \
    -DCMAKE_EXE_LINKER_FLAGS="-static" >&2

say "building thalyx-engine — this takes a few minutes"
cmake --build "$BUILD" --target thalyx-engine -j "$(nproc)" >&2

ENGINE="$BUILD/bin/thalyx-engine"
[ -x "$ENGINE" ] || { say "the build finished and $ENGINE is not there"; exit 1; }

# The check that matters, and it is two questions rather than one. A program can
# have no INTERP and still carry a NEEDED — that is a shared object, not an
# executable — and `file` says "statically linked" for both.
if readelf -lW "$ENGINE" | grep -q INTERP; then
    say "FAILED  $ENGINE wants a dynamic loader:"
    readelf -lW "$ENGINE" | grep INTERP >&2
    say "        There is no loader inside Thalyx. The engine must be static."
    exit 1
fi
if readelf -dW "$ENGINE" 2>/dev/null | grep -q NEEDED; then
    say "FAILED  $ENGINE needs shared libraries:"
    readelf -dW "$ENGINE" | grep NEEDED >&2
    exit 1
fi

say "static, no interpreter, no shared libraries:"
say "$(file -b "$ENGINE")"
echo "$ENGINE"
