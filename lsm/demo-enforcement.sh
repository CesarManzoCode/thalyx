#!/bin/bash
# Prove that the permission primitive actually denies something.
#
# Until this passes, "permissions are enforced in the kernel" is a claim. What
# it does:
#
#   1. Creates one cgroup and puts a policy in the map for it: filesystem
#      allowed, network denied.
#   2. Turns enforcement on.
#   3. Tries to open a TCP connection from inside that cgroup, and from
#      outside it.
#
# Inside should fail with "Operation not permitted". Outside should fail with
# "Connection refused" — a different failure, from the network stack rather
# than from Thalyx, which is what shows the denial is targeted rather than
# blanket.
#
# It connects to 127.0.0.1:9, a port nothing listens on, so the test needs no
# internet and always reaches the same place.
#
# Safe to run on a working machine: the policy names exactly one cgroup, and
# every other process on the system has no entry, which the program treats as
# "not a Thalyx module" and lets through untouched. See the fail-open note in
# thalyx_lsm.bpf.c.

set -u

PINDIR=/sys/fs/bpf/thalyx
CGROUP=/sys/fs/cgroup/thalyx-demo
TARGET=127.0.0.1
PORT=9

green() { printf '\033[32m%s\033[0m\n' "$1"; }
red()   { printf '\033[31m%s\033[0m\n' "$1"; }

cleanup() {
    echo
    echo "==> cleaning up"
    sudo "$(command -v bpftool)" map delete pinned "$PINDIR/maps/thalyx_policy" \
        key hex $KEY_HEX 2>/dev/null && echo "    policy entry removed"
    sudo "$(command -v bpftool)" map update pinned "$PINDIR/maps/thalyx_enforcing" \
        key 0 0 0 0 value 0 0 0 0 2>/dev/null && echo "    back to observe mode"
    sudo rmdir "$CGROUP" 2>/dev/null && echo "    cgroup removed"
}

# `sudo test`, not `test`: bpffs is mode 700 and root-owned, so an
# unprivileged check reports "not attached" for something that is attached.
if ! sudo test -d "$PINDIR/lsm"; then
    red "thalyx-lsm is not attached. Run 'make load' first."
    exit 1
fi

# ---------------------------------------------------------------- the cgroup

echo "==> creating a cgroup for the test"
sudo mkdir -p "$CGROUP" || { red "could not create $CGROUP"; exit 1; }

# The cgroup id the kernel reports through bpf_get_current_cgroup_id() is the
# inode number of the cgroup directory.
CGID=$(stat -c %i "$CGROUP")
echo "    cgroup id $CGID"

# bpftool takes keys and values as little-endian byte sequences.
u64_hex() {
    printf '%016x' "$1" | sed 's/\(..\)/\1 /g' | awk '{for(i=NF;i>0;i--) printf "%s ", $i}'
}
u32_hex() {
    printf '%08x' "$1" | sed 's/\(..\)/\1 /g' | awk '{for(i=NF;i>0;i--) printf "%s ", $i}'
}

KEY_HEX=$(u64_hex "$CGID")
trap cleanup EXIT

# struct policy { __u32 allowed; __u32 flags; __u64 expires_ns; }
#
# allowed = FS_READ | FS_WRITE = 6. Network is deliberately absent: the module
# may touch files and may not touch the network, which is the shape of a real
# grant. Denying the filesystem too would stop the test process from starting
# at all.
VALUE_HEX="$(u32_hex 6)$(u32_hex 0)$(u64_hex 0)"

echo "==> writing the policy: filesystem allowed, network denied"
sudo bpftool map update pinned "$PINDIR/maps/thalyx_policy" \
    key hex $KEY_HEX value hex $VALUE_HEX || {
    red "could not write the policy"
    exit 1
}

# ---------------------------------------------------------------- observing

echo
echo "==> with enforcement OFF (observing)"
INSIDE_OBSERVING=$(sudo sh -c "echo \$\$ > $CGROUP/cgroup.procs; exec curl -s --max-time 3 http://$TARGET:$PORT/ 2>&1" 2>&1)
echo "    inside the cgroup:  ${INSIDE_OBSERVING:-<no output>}"

# ---------------------------------------------------------------- enforcing

echo
echo "==> turning enforcement ON"
sudo bpftool map update pinned "$PINDIR/maps/thalyx_enforcing" \
    key 0 0 0 0 value 1 0 0 0 || { red "could not enable enforcement"; exit 1; }

INSIDE=$(sudo sh -c "echo \$\$ > $CGROUP/cgroup.procs; exec curl -s --max-time 3 http://$TARGET:$PORT/ 2>&1" 2>&1)
OUTSIDE=$(curl -s --max-time 3 "http://$TARGET:$PORT/" 2>&1)

echo
echo "    inside the cgroup:  ${INSIDE:-<no output>}"
echo "    outside:            ${OUTSIDE:-<no output>}"

# ---------------------------------------------------------------- verdict

echo
echo "=============================================="

denied_inside=0
allowed_outside=0

case "$INSIDE" in
    *"not permitted"*|*"Operation not permitted"*) denied_inside=1 ;;
esac

case "$OUTSIDE" in
    *"refused"*|*"Connection refused"*) allowed_outside=1 ;;
esac

if [ "$denied_inside" -eq 1 ] && [ "$allowed_outside" -eq 1 ]; then
    green "ENFORCEMENT IS REAL."
    echo
    echo "Inside the cgroup the kernel refused the connection because Thalyx"
    echo "said so. Outside, the connection reached the network stack and was"
    echo "refused there instead — the denial is targeted, not blanket."
    echo
    echo "This is the just-in-time permission primitive doing what it was"
    echo "decreed to do, in the kernel, on real hardware."
    exit 0
fi

red "NOT PROVEN"
echo
if [ "$denied_inside" -eq 0 ]; then
    echo "The connection inside the cgroup was not denied. Either the program"
    echo "did not attach, or the policy key does not match the cgroup id the"
    echo "kernel reports. Check:  sudo bpftool link list"
fi
if [ "$allowed_outside" -eq 0 ]; then
    echo "The connection outside the cgroup did not behave as expected, which"
    echo "would mean the denial is not targeted. That is worse than a failed"
    echo "denial and should be investigated before enforcing anything."
fi
exit 1
