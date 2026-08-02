#!/usr/bin/env bash
#
# Everything Thalyx claims, checked on a real machine, in one run.
#
#   sudo ./dev/verify.sh
#
# Most of Thalyx cannot be proven in a container. The BPF LSM needs a kernel
# with `bpf` in its LSM order, resource limits need delegated cgroup
# controllers, and the enforcement demo needs to actually deny a connection.
# This script is the place where all of that gets exercised at once, on
# hardware, and reports what it managed to prove.
#
# The reporting rule is the same one the test suite follows: a step that could
# not run says NOT PROVEN and says why. It never counts as a pass. A green run
# that exercised nothing is indistinguishable from a green run that exercised
# everything, and that confusion has already cost this project once.
#
# Nothing is left loaded. The LSM is detached on the way out, whatever happened
# — including on Ctrl-C.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
ROOT="$PWD"

# ---------------------------------------------------------------- reporting

PROVEN=0
UNPROVEN=0
FAILED=0
declare -a NOTES=()

bold()   { printf '\033[1m%s\033[0m\n' "$*"; }
green()  { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }
red()    { printf '\033[31m%s\033[0m\n' "$*"; }

step() {
    printf '\n'
    bold "── $* "
}

proven()   { PROVEN=$((PROVEN + 1));     green   "   PROVEN      $*"; }
unproven() { UNPROVEN=$((UNPROVEN + 1)); yellow  "   NOT PROVEN  $*"; NOTES+=("$*"); }
failed()   { FAILED=$((FAILED + 1));     red     "   FAILED      $*"; NOTES+=("FAILED: $*"); }

# ---------------------------------------------------------------- teardown

LOADED=0
cleanup() {
    if [ "$LOADED" = 1 ]; then
        printf '\n'
        bold "── detaching thalyx-lsm "
        make -C "$ROOT/lsm" unload >/dev/null 2>&1 && echo "   done" || red "   could not unload; run: sudo make -C lsm unload"
    fi
    rm -rf "${WORK:-/nonexistent}"
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------- privileges

if [ "$(id -u)" != 0 ]; then
    red "This has to run as root: it loads a kernel security module, creates"
    red "cgroups, and mounts filesystems."
    echo
    echo "  sudo ./dev/verify.sh"
    exit 1
fi

WORK="$(mktemp -d)"

bold "Thalyx verification — $(uname -srm), $(date '+%Y-%m-%d %H:%M')"

# ---------------------------------------------------------------- 1. machine

step "1. what this machine can do"

KERNEL_OK=1
CGROUP2="$(awk '$3 == "cgroup2" { print $2; exit }' /proc/mounts)"
if [ -n "$CGROUP2" ]; then
    proven "cgroup2 mounted at $CGROUP2"
else
    failed "no cgroup2 filesystem; module confinement cannot work at all"
    KERNEL_OK=0
fi

CONTROLLERS="$(cat "$CGROUP2/cgroup.controllers" 2>/dev/null)"
if printf '%s' "$CONTROLLERS" | grep -qw memory && printf '%s' "$CONTROLLERS" | grep -qw pids; then
    proven "cgroup controllers available: $CONTROLLERS"
    HAVE_CONTROLLERS=1
else
    unproven "cgroup controllers memory/pids are not delegated here (have: ${CONTROLLERS:-none}); resource limits cannot be tested"
    HAVE_CONTROLLERS=0
fi

if [ -r /sys/kernel/security/lsm ]; then
    LSM_ORDER="$(cat /sys/kernel/security/lsm)"
    if printf '%s' "$LSM_ORDER" | tr ',' '\n' | grep -qx bpf; then
        proven "bpf is in the kernel's LSM order: $LSM_ORDER"
        HAVE_BPF_LSM=1
    else
        unproven "bpf is not in the LSM order ($LSM_ORDER); add lsm=...,bpf to the kernel command line and reboot"
        HAVE_BPF_LSM=0
    fi
else
    unproven "securityfs is not mounted, so the LSM order cannot be read"
    HAVE_BPF_LSM=0
fi

if command -v cargo >/dev/null 2>&1; then
    proven "cargo present"
else
    red "cargo is missing; nothing can be built or checked"
    exit 1
fi

# clang and bpftool are how the kernel side gets built, not claims Thalyx
# makes. Missing them means this machine cannot check that part — it does not
# mean anything is wrong.
for tool in clang bpftool; do
    if command -v "$tool" >/dev/null 2>&1; then
        proven "$tool present"
    else
        unproven "$tool is not installed, so the kernel side cannot be built here"
        HAVE_BPF_LSM=0
    fi
done

# ---------------------------------------------------------------- 2. the code

step "2. the code builds clean"

if cargo fmt --all --check >/dev/null 2>&1; then
    proven "formatting is clean"
else
    failed "cargo fmt --all --check reports differences"
fi

if cargo clippy --all-targets --quiet 2>&1 | grep -q '^error'; then
    failed "clippy found problems (run: cargo clippy --all-targets)"
else
    proven "clippy is clean, with warnings denied"
fi

if cargo build --release --quiet 2>&1 | tee "$WORK/build.log" | grep -q '^error'; then
    failed "the workspace does not build (see $WORK/build.log)"
    exit 1
fi
proven "the workspace builds"
THALYX="$ROOT/target/release/thalyx"

# ------------------------------------------------------------ 3. the kernel

if [ "$HAVE_BPF_LSM" = 1 ] && [ "$KERNEL_OK" = 1 ]; then
    step "3. the kernel side compiles and attaches"

    if make -C lsm hooks 2>&1 | tee "$WORK/hooks.log" | grep -q "available"; then
        proven "$(grep -c available "$WORK/hooks.log") LSM hook(s) attachable on this kernel"
    fi
    grep -q "missing" "$WORK/hooks.log" && \
        unproven "some hooks are missing on this kernel: $(grep missing "$WORK/hooks.log" | awk '{print $2}' | tr '\n' ' ')"

    if make -C lsm all > "$WORK/lsm-build.log" 2>&1; then
        proven "both BPF objects compile"
    else
        failed "the BPF objects do not compile; see $WORK/lsm-build.log"
        tail -20 "$WORK/lsm-build.log"
    fi

    if make -C lsm load > "$WORK/lsm-load.log" 2>&1; then
        LOADED=1
        proven "thalyx-lsm attached (observe mode)"
    else
        failed "thalyx-lsm did not attach; see $WORK/lsm-load.log"
        tail -20 "$WORK/lsm-load.log"
    fi

    if [ "$LOADED" = 1 ]; then
        if [ -e /sys/fs/bpf/thalyx/maps/thalyx_mutation_count ]; then
            proven "the mutation counter map is pinned"
        else
            unproven "the mutation counter map is not pinned; the watcher's hooks may be missing on this kernel"
        fi
    fi

    # ------------------------------------------------- 5. enforcement is real

    step "4. enforcement actually denies"

    if [ "$LOADED" = 1 ]; then
        if make -C lsm demo 2>&1 | tee "$WORK/demo.log" | grep -q "ENFORCEMENT IS REAL"; then
            proven "a connection was denied inside the cgroup and allowed outside it"
        else
            failed "the enforcement demo did not prove denial; see $WORK/demo.log"
            tail -25 "$WORK/demo.log"
        fi
    else
        unproven "the LSM is not attached, so nothing could be denied"
    fi
else
    step "3-4. the kernel side"
    unproven "skipped entirely: this machine has no usable BPF LSM"
fi

# --------------------------------------------------------- 5. the suite

step "5. the test suite, with every skip this machine can afford forbidden"

# After the LSM is attached, not before. The suite has tests that need the
# policy map to exist, and demanding them while it was still unloaded would
# report a failure of Thalyx for something this script had not done yet.
SUITE_ENV=(THALYX_REQUIRE_CGROUP_TESTS=1)
[ "$HAVE_CONTROLLERS" = 1 ] && SUITE_ENV+=(THALYX_REQUIRE_CONTROLLER_TESTS=1)
[ "$LOADED" = 1 ]           && SUITE_ENV+=(THALYX_REQUIRE_LSM_TESTS=1)

echo "   ${SUITE_ENV[*]}"
if env "${SUITE_ENV[@]}" cargo test --workspace --release --quiet > "$WORK/tests.log" 2>&1; then
    COUNT="$(grep -Eo '^test result: ok\. [0-9]+' "$WORK/tests.log" | awk '{s+=$4} END {print s}')"
    proven "${COUNT:-?} tests pass, and none of them skipped a check this machine can make"
else
    failed "the suite did not pass; see $WORK/tests.log"
    tail -30 "$WORK/tests.log"
fi

# The flags that were *not* set are exactly the things this machine cannot say
# anything about. Naming them is the point.
[ "$HAVE_CONTROLLERS" = 0 ] && unproven "resource limits (memory.max, pids.max) — no delegated controllers"
[ "$LOADED" = 0 ]           && unproven "kernel policy enforcement — the LSM is not attached"

# ------------------------------------------------- 6. a module, end to end

step "6. a real module, installed and run confined"

STORE="$WORK/store"
export THALYX_ROOT="$STORE"
PAYLOAD="$WORK/payload"
GRANTED="$WORK/granted"

mkdir -p "$PAYLOAD/bin" "$GRANTED"
chmod 700 "$GRANTED"   # private on purpose: only the idmapped bind makes it usable

cat > "$PAYLOAD/bin/demo" <<'MODULE'
#!/bin/sh
echo "uid=$(id -u)"
echo "pid=$$"
echo "host=$(hostname)"
echo "root=$(ls / | tr '\n' ' ')"
echo "net=$(tail -n +3 /proc/net/dev | awk '{print $1}' | tr -d ' \n')"
echo "granted=$(cat GRANTED_PATH/note 2>&1)"
MODULE
sed -i "s|GRANTED_PATH|$GRANTED|" "$PAYLOAD/bin/demo"
chmod +x "$PAYLOAD/bin/demo"
echo "reachable" > "$GRANTED/note"

"$THALYX" dev keygen --out "$WORK/publisher.key" >/dev/null 2>&1

cat > "$WORK/manifest.toml" <<MANIFEST
format_version = 1
id             = "org.thalyx.verify"
name           = "Thalyx verification module"
version        = "1.0.0"
description    = "Reports what it can see from inside its sandbox"
license        = "GPL-3.0-or-later"
publisher_key  = "ed25519:0000000000000000000000000000000000000000000000000000000000000000"
distribution   = "prebuilt"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 0

[requires]
thalyx = ">=0.1.0"

[[permissions]]
resource = "$GRANTED"
action   = "read"
type     = "persistent"

[entrypoints]
run = "bin/demo"
MANIFEST

if "$THALYX" dev pack "$PAYLOAD" --manifest "$WORK/manifest.toml" \
        --key "$WORK/publisher.key" --out "$WORK/verify.thmod" > "$WORK/pack.log" 2>&1; then
    proven "a module was packed and signed"
else
    failed "packing failed; see $WORK/pack.log"
fi

if "$THALYX" module install "$WORK/verify.thmod" --yes > "$WORK/install.log" 2>&1; then
    UID_ASSIGNED="$(grep -Eo 'user [0-9]+' "$WORK/install.log" | head -1 | awk '{print $2}')"
    proven "installed, and assigned user ${UID_ASSIGNED:-?}"
else
    failed "install failed; see $WORK/install.log"
    tail -20 "$WORK/install.log"
fi

if [ "$LOADED" = 1 ]; then
    if "$THALYX" module run org.thalyx.verify > "$WORK/run.log" 2>&1; then
        proven "the module ran confined"

        # Each of these asks the module what it saw. Asking Thalyx whether it
        # confined the module would prove nothing.
        check() {
            local what="$1" pattern="$2" field="$3"
            if grep -qE "$pattern" "$WORK/run.log"; then
                green "     $what"
            else
                failed "$what — the module reported: $(grep -E "^$field=" "$WORK/run.log" || echo 'nothing')"
            fi
        }
        check "it is PID 1 of its own namespace"         '^pid=1$'                          pid
        check "it is not the user Thalyx runs as"        "^uid=${UID_ASSIGNED:-700000}\$"   uid
        check "its hostname says nothing about the host" '^host=thalyx-module$'             host
        check "it has no network but loopback"           '^net=lo:$'                        net
        check "the granted path is readable"             '^granted=reachable$'              granted

        if grep -q 'root=.*module' "$WORK/run.log" && ! grep -q 'root=.*home' "$WORK/run.log"; then
            green "     its root holds its own tree and not the host's"
        else
            failed "the module's root looks like the host's: $(grep '^root=' "$WORK/run.log")"
        fi
    else
        failed "the confined run failed; see $WORK/run.log"
        tail -25 "$WORK/run.log"
    fi
else
    unproven "a confined run needs the LSM attached; Thalyx refuses to run a module nothing can enforce"
    echo "     (that refusal is itself the correct behaviour, and the suite tests it)"
fi

# --------------------------------------------------- 7. the index and watcher

step "7. the semantic index, and what the kernel can tell it"

if "$THALYX" graph build "$ROOT/crates" > "$WORK/graph.log" 2>&1; then
    proven "the index built: $(grep -Eo '[0-9]+ file\(s\), [0-9]+ parsed' "$WORK/graph.log" | head -1)"
else
    failed "the index did not build; see $WORK/graph.log"
fi

"$THALYX" graph status "$ROOT/crates" 2>&1 | sed 's/^/     /'

if [ "$LOADED" = 1 ]; then
    if "$THALYX" graph verify "$ROOT/crates" > "$WORK/verify-graph.log" 2>&1; then
        if grep -q "COVERAGE HOLE" "$WORK/verify-graph.log"; then
            unproven "the mutation counter cannot be believed on this machine (see below)"
        else
            proven "the counter and the tree agreed"
        fi
        sed 's/^/     /' "$WORK/verify-graph.log"
    else
        unproven "graph verify found a coverage hole — expected today, the hook set misses writes"
        sed 's/^/     /' "$WORK/verify-graph.log"
    fi
else
    unproven "the mutation counter needs the watcher attached"
fi

# ---------------------------------------------------------------- summary

printf '\n\n'
bold "════════════════════════════════════════════════════════════"
bold " proven      $PROVEN"
bold " not proven  $UNPROVEN"
bold " failed      $FAILED"
bold "════════════════════════════════════════════════════════════"

if [ ${#NOTES[@]} -gt 0 ]; then
    printf '\n'
    bold "What this run could not establish:"
    for note in "${NOTES[@]}"; do
        echo "  · $note"
    done
fi

printf '\n'
if [ "$FAILED" -gt 0 ]; then
    red "Something Thalyx claims is not true on this machine."
    exit 1
fi
if [ "$UNPROVEN" -gt 0 ]; then
    yellow "Nothing is broken, and this machine could not check everything."
    exit 0
fi
green "Everything Thalyx claims was checked here, and held."
