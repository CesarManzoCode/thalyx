#!/usr/bin/env bash
#
# The Rust runtime the agent inside Thalyx programs with — built, not borrowed.
#
#   dev/build-rust-runtime.sh [output-directory]
#
# Leaves a finished artifact at <output>/<identity>/ and prints the path on the
# last line. `make -C image store-stage RUST=1` copies that onto the store; see
# `image/Makefile`.
#
# ─────────────────────────────────────────────────────────────────────────────
# WHY THIS EXISTS
#
# On 2026-08-30 a paid benchmark run watched Claude pick exactly the right
# primitive inside Thalyx —
#
#     const def = thalyx.context('…');
#     const r1  = thalyx.rename('…', '…');
#
# — and get back `source: index`, `analyzer_starts: 0`, and
#
#     rename: { ok: false, error: unresolved,
#               message: "there is no `cargo` on this machine" }
#
# The machine had been told it could resolve names and it could not. Everything
# after that in the transcript is a consequence.
#
# The first fix attempted was to copy the host's `~/.rustup` into the store.
# Cesar stopped it, and he was right: `Filosofia-Fundacional.md` says Thalyx is
# the whole system, and a Thalyx whose programming face only works because
# Fedora happens to have rustup installed is a Thalyx that borrows its most
# important capability from the machine it claims to replace. Move the disk to
# another x86_64 box and the semantic provider would vanish.
#
# So: **the host provides nothing at agent runtime.** Not cargo, not rustc, not
# rust-analyzer, not the standard library, not the dynamic loader. This script
# builds all of it from artifacts that are named, versioned and digest-checked,
# and after it has run the host is out of the picture.
#
# ─────────────────────────────────────────────────────────────────────────────
# WHY THE musl TOOLCHAIN AND NOT THE ORDINARY ONE
#
# Measured on 2026-08-31, both ways, before anything was written:
#
#   x86_64-unknown-linux-gnu   needs glibc's loader, libc, libm, libdl, librt,
#                              libpthread, libgcc_s and libz from the host, and
#                              carries a separate 191 MB libLLVM.so.
#
#   x86_64-unknown-linux-musl  needs exactly two files that Rust does not ship:
#                              musl's loader and libgcc_s.so.1. LLVM is linked
#                              inside librustc_driver, so there is no libLLVM.
#
# Rust publishes host tools for `x86_64-unknown-linux-musl` officially, with a
# sha256 for every file in its own channel manifest. Two missing files is a
# problem a person can close; most of a distribution is not. That is the whole
# reason for the choice — see `vault/09-Notas-Tecnicas/Runtime-Rust-Agente.md`.
#
# ─────────────────────────────────────────────────────────────────────────────
# WHERE THE TWO MISSING FILES COME FROM, AND WHY NEITHER IS COPIED
#
# **The loader** is musl's `libc.so`, compiled here from musl's own release
# tarball with the digest below. The same arrangement the Linux kernel already
# has in `image/Makefile`: a pinned tarball, checked before it is believed,
# built by us. It is about a megabyte and it builds in seconds.
#
# **libgcc_s.so.1** is linked here out of `libunwind.a`, which Rust ships inside
# `rust-std`'s `self-contained/` directory — so it comes from the same
# digest-checked artifact as the compiler and introduces nothing new.
#
# That it is enough is a measurement, not a hope. Of the 883 undefined symbols
# across cargo, rustc, rust-analyzer, the proc-macro server and
# librustc_driver, everything resolves against musl and librustc_driver except
# 29, and of those 29 the only ones that are not weak are the fifteen
# `_Unwind_*` — every one of which `libunwind.a` defines. The rest are the
# transactional-memory stubs, `__register_frame_info`, and the two `pidfd_*`
# functions Rust's std weak-links for newer musl.
#
# It was then run rather than argued: a chroot holding this artifact, `/proc`
# and three device nodes — no shell, no /usr, no /lib64, nothing else —
# answered `cargo --version`, `cargo metadata`, `rustc --emit=metadata`, and a
# full rust-analyzer session that resolved a definition and returned four
# rename edits.
#
# ─────────────────────────────────────────────────────────────────────────────
# WHAT IS DELIBERATELY NOT IN IT
#
#   share/, man pages, docs        never runs, and it is 13 MB of the rustc
#                                  component alone
#   lib/rustlib/<triple>/bin/      rust-lld, wasm-component-ld, rust-objcopy —
#                                  174 MB of *linkers*. The semantic provider
#                                  never links: `cargo metadata` and
#                                  rust-analyzer's analysis do not. Leaving them
#                                  out is also the honest thing, because
#                                  `rust-lld` wants libgcc's integer builtins,
#                                  which the unwinder-only libgcc_s above does
#                                  not have — shipping a linker that cannot
#                                  start is worse than shipping none.
#   every other target             one host, one target
#   rustdoc, rustfmt, clippy       not what a semantic provider is for
#
# So this artifact **resolves and renames**; it does not build. When something
# inside Thalyx needs to compile a proc-macro or a build script, that is the
# next known cause, and it gets its own change with its own evidence.
#
# ─────────────────────────────────────────────────────────────────────────────
# THE PINS
#
# Every URL below has a digest, and a digest that does not match stops the
# build. A tarball fetched over TLS proves who served the bytes, not what the
# bytes were — the same reasoning the kernel pin carries in `image/Makefile`.
#
# The Rust digests are not ours: they are the values in Rust's own channel
# manifest, `https://static.rust-lang.org/dist/channel-rust-1.90.0.toml`, which
# is how rustup itself decides whether a download is the file it asked for.
#
#   dev/rust-runtime-pins.sh prints the commands that re-derive them.
#
# musl 1.2.4 and not 1.2.5 for one reason: 1.2.4 is the one that was built and
# run end to end under the toolchain above on 2026-08-31. Rule 12 —
# `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md` — the thing that gets
# verified has to be the thing that ships. Bumping it is one line here and the
# same physical test again.

set -euo pipefail

RUST_VERSION="1.90.0"
RUST_DIST_DATE="2025-09-18"
TARGET="x86_64-unknown-linux-musl"
DIST="https://static.rust-lang.org/dist/$RUST_DIST_DATE"

# component:filename:sha256 — the sha256 of the .tar.xz, from Rust's manifest.
COMPONENTS=(
  "cargo:cargo-$RUST_VERSION-$TARGET.tar.xz:dddd1ee3da59440d5aa4d149ebb5fbbe0d7252dd94e171d5f2b071d7354f9b3a"
  "rustc:rustc-$RUST_VERSION-$TARGET.tar.xz:993cb26cee9525b1553d82b8fc2b6ddffd50a0a561cd896e3daa3a9f8ae65949"
  "rust-std:rust-std-$RUST_VERSION-$TARGET.tar.xz:38490d575786f4688e83b357baeb022d8dde0ace2cb8c1357e060c76644fc56a"
  "rust-analyzer:rust-analyzer-$RUST_VERSION-$TARGET.tar.xz:cc5d529f84710b8f4439bd457d7cdc432f0dc616203a5d51af895d5ded8ed691"
  "rust-src:rust-src-$RUST_VERSION.tar.xz:cde088d57064d151b2236f4619aea4a8207e0709eb3035ddc6617d609ab7d453"
)

MUSL_VERSION="1.2.4"
MUSL_URL="https://musl.libc.org/releases/musl-$MUSL_VERSION.tar.gz"
MUSL_SHA256="7a35eae33d5372a7c0da1188de798726f68825513b7ae3ebe97aaaa52114f039"

IDENTITY="rust-$RUST_VERSION-$TARGET"
# Where the artifact lands inside the machine. Spelled the same way in
# `crates/thalyx-rust/src/runtime.rs`, which is what discovery looks at: two
# places computing this is two answers to where the toolchain is.
INSIDE="/opt/thalyx/toolchains/rust/$IDENTITY"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-${TMPDIR:-/tmp}/thalyx-rust-runtime}"
CACHE="${THALYX_RUST_DIST_CACHE:-$OUT/dist}"
WORK="$OUT/work"
ARTIFACT="$OUT/$IDENTITY"

say()  { printf '  %s\n' "$*" >&2; }
die()  { printf '\n  %s\n\n' "$*" >&2; exit 1; }

for tool in curl tar sha256sum cc make ld; do
    command -v "$tool" > /dev/null \
        || die "no $tool. Building the loader needs a C compiler and binutils; \
the same ones image/Makefile already needs for the kernel."
done

mkdir -p "$CACHE" "$WORK"

# Fetch and verify. `THALYX_RUST_DIST_CACHE` lets a machine that already has the
# bytes skip the download — the digest is checked either way, so a cached file
# that is not the file is caught exactly like a bad download.
fetch() {
    local url="$1" name="$2" want="$3" path="$CACHE/$2"
    if [ ! -s "$path" ]; then
        say "fetching $name"
        curl -fsSL --retry 3 -o "$path.part" "$url" \
            || die "could not fetch $url"
        mv "$path.part" "$path"
    fi
    local got
    got="$(sha256sum "$path" | cut -d' ' -f1)"
    [ "$got" = "$want" ] || die "$name is not the file it should be.
      expected $want
      got      $got
  Nothing was used. If the pin is out of date, re-derive it — dev/rust-runtime-pins.sh
  prints how — rather than editing the digest to match whatever arrived."
    say "verified $name"
}

say ""
say "Rust runtime for the agent — $IDENTITY"
say ""

for entry in "${COMPONENTS[@]}"; do
    IFS=: read -r _component file digest <<< "$entry"
    fetch "$DIST/$file" "$file" "$digest"
done
fetch "$MUSL_URL" "musl-$MUSL_VERSION.tar.gz" "$MUSL_SHA256"

rm -rf "$ARTIFACT" "$WORK"
mkdir -p "$ARTIFACT/bin" "$ARTIFACT/lib/rustlib" "$ARTIFACT/libexec" "$WORK"

unpack() {
    local file="$1"
    say "unpacking $file"
    tar -C "$WORK" -xf "$CACHE/$file"
}
for entry in "${COMPONENTS[@]}"; do
    IFS=: read -r _component file _digest <<< "$entry"
    unpack "$file"
done

C="$WORK/cargo-$RUST_VERSION-$TARGET/cargo"
R="$WORK/rustc-$RUST_VERSION-$TARGET/rustc"
S="$WORK/rust-std-$RUST_VERSION-$TARGET/rust-std-$TARGET"
A="$WORK/rust-analyzer-$RUST_VERSION-$TARGET/rust-analyzer-preview"
SRC="$WORK/rust-src-$RUST_VERSION/rust-src"

# The selection, spelled as paths rather than as an exclusion list. An
# exclusion list is a claim about everything you did not think of; this is a
# claim about what is there.
cp "$C/bin/cargo"                            "$ARTIFACT/bin/cargo"
cp "$R/bin/rustc"                            "$ARTIFACT/bin/rustc"
cp "$A/bin/rust-analyzer"                    "$ARTIFACT/bin/rust-analyzer"
cp "$R/libexec/rust-analyzer-proc-macro-srv" "$ARTIFACT/libexec/"
cp "$R"/lib/librustc_driver-*.so             "$ARTIFACT/lib/"
mkdir -p "$ARTIFACT/lib/rustlib/$TARGET"
cp -a "$S/lib/rustlib/$TARGET/lib"           "$ARTIFACT/lib/rustlib/$TARGET/lib"
cp -a "$SRC/lib/rustlib/src"                 "$ARTIFACT/lib/rustlib/src"

# ── the loader, compiled here ────────────────────────────────────────────────
#
# `--prefix=/` and nothing installed: the one file wanted is `lib/libc.so`,
# which under musl *is* the dynamic linker as well as the C library. The
# binaries above ask the kernel for `/lib/ld-musl-x86_64.so.1`; PID 1 makes that
# name resolve to this file once the store is mounted — see
# `crates/thalyx-cli/src/store_disk.rs`.
say "building musl $MUSL_VERSION"
tar -C "$WORK" -xf "$CACHE/musl-$MUSL_VERSION.tar.gz"
(
    cd "$WORK/musl-$MUSL_VERSION"
    ./configure --prefix=/ --disable-static --enable-shared --disable-gcc-wrapper \
        > configure.log 2>&1 || { tail -20 configure.log >&2; die "musl configure failed"; }
    make -j"$(nproc 2>/dev/null || echo 2)" > build.log 2>&1 \
        || { tail -30 build.log >&2; die "musl did not build"; }
)
cp "$WORK/musl-$MUSL_VERSION/lib/libc.so" "$ARTIFACT/lib/libc.so"
chmod 755 "$ARTIFACT/lib/libc.so"
# Both names, because two different things ask for them: the kernel reads
# PT_INTERP and wants `ld-musl-x86_64.so.1`, and the loader resolves the
# `libc.so` in every DT_NEEDED. Relative, so the artifact can be built at one
# path and mounted at another.
ln -sf libc.so "$ARTIFACT/lib/ld-musl-x86_64.so.1"

# ── the unwinder, linked out of Rust's own ───────────────────────────────────
say "linking libgcc_s.so.1 from the toolchain's own libunwind.a"
UNWIND="$ARTIFACT/lib/rustlib/$TARGET/lib/self-contained/libunwind.a"
[ -f "$UNWIND" ] || die "no libunwind.a in rust-std's self-contained directory"
ld -shared -o "$ARTIFACT/lib/libgcc_s.so.1" \
   -soname libgcc_s.so.1 --eh-frame-hdr -z noexecstack \
   --whole-archive "$UNWIND" --no-whole-archive \
    || die "libgcc_s.so.1 did not link"

# ── does it run? ─────────────────────────────────────────────────────────────
#
# Asked, never assumed, and asked **through the artifact's own loader** —
# `lib/libc.so <program>` is how musl's ldso is invoked directly. That is what
# makes this checkable on a host that has no musl at all, which is every host
# this will ever be built on. A `-f` on the file would prove the copy worked;
# this proves the closure closed.
runs() {
    "$ARTIFACT/lib/libc.so" "$1" --version > /dev/null 2>&1
}
for program in bin/cargo bin/rustc bin/rust-analyzer; do
    runs "$ARTIFACT/$program" \
        || die "$program does not run under this artifact's own loader. The runtime is
  incomplete, and staging it would put a machine on the store that says it can
  resolve names and cannot. Nothing was staged."
done
say "cargo, rustc and rust-analyzer all answered --version through lib/libc.so"

# ── what it is, written down beside it ───────────────────────────────────────
#
# Read by `thalyx-rust`'s discovery, and by the preflight, so that a machine can
# say *which* toolchain it is using rather than that it has one.
{
    printf '{\n'
    printf '  "identity": "%s",\n'  "$IDENTITY"
    printf '  "rust": "%s",\n'      "$RUST_VERSION"
    printf '  "target": "%s",\n'    "$TARGET"
    printf '  "dist": "%s",\n'      "$DIST"
    printf '  "musl": "%s",\n'      "$MUSL_VERSION"
    printf '  "inside": "%s",\n'    "$INSIDE"
    printf '  "built": "%s",\n'     "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "components": {\n'
    last=$((${#COMPONENTS[@]} - 1)); i=0
    for entry in "${COMPONENTS[@]}"; do
        IFS=: read -r component file digest <<< "$entry"
        printf '    "%s": {"file": "%s", "sha256": "%s"}' "$component" "$file" "$digest"
        [ "$i" -lt "$last" ] && printf ','
        printf '\n'; i=$((i + 1))
    done
    printf '  },\n'
    printf '  "musl_sha256": "%s"\n' "$MUSL_SHA256"
    printf '}\n'
} > "$ARTIFACT/runtime.json"

say ""
say "  from:   $DIST  (Rust $RUST_VERSION, $TARGET)"
say "          $MUSL_URL"
say "  size:   $(du -sh "$ARTIFACT" | cut -f1)"
say "          bin      $(du -sh "$ARTIFACT/bin" | cut -f1)"
say "          lib      $(du -sh "$ARTIFACT/lib" | cut -f1)"
say "          libexec  $(du -sh "$ARTIFACT/libexec" | cut -f1)"
say "  inside: $INSIDE"
say ""
echo "$ARTIFACT"
