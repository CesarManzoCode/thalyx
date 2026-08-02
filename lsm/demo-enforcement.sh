#!/bin/bash
# Prove that the permission primitive actually denies something.
#
# Until this passes, "permissions are enforced in the kernel" is a claim.
#
#   1. Creates one cgroup and puts a policy in the map for it: filesystem
#      allowed, network denied.
#   2. Turns enforcement on.
#   3. Opens a TCP connection from inside that cgroup, and from outside it.
#
# Inside must fail with errno 1, EPERM — the kernel refusing because Thalyx
# said so. Outside must fail with errno 111, ECONNREFUSED — the connection
# reaching the network stack and being refused there. Two different failures
# is the point: it shows the denial is targeted rather than blanket. Looking
# only at the inside case, a policy that broke everything would look identical
# to one that worked.
#
# It connects to 127.0.0.1:9, a port nothing listens on, so the test needs no
# internet and always reaches the same place.
#
# The connection is made from Python rather than curl. curl reports both
# failures with the same wording, and under -s reports nothing at all; Python
# surfaces the raw errno, which is the only thing here that distinguishes a
# denial from a refusal.
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
KEY_HEX=""

green() { printf '\033[32m%s\033[0m\n' "$1"; }
red()   { printf '\033[31m%s\033[0m\n' "$1"; }

# Report the errno of a connection attempt, and nothing else.
PROBE='
import socket, sys
s = socket.socket()
s.settimeout(3)
try:
    s.connect((sys.argv[1], int(sys.argv[2])))
    print("errno=0 connected")
except OSError as e:
    print(f"errno={e.errno} {e.strerror}")
'

connect_inside() {
    sudo sh -c "echo \$\$ > $CGROUP/cgroup.procs; exec python3 -c '$PROBE' $TARGET $PORT" 2>&1
}

connect_outside() {
    python3 -c "$PROBE" "$TARGET" "$PORT" 2>&1
}

cleanup() {
    echo
    echo "==> cleaning up"
    if [ -n "$KEY_HEX" ]; then
        sudo bpftool map delete pinned "$PINDIR/maps/thalyx_policy" \
            key hex $KEY_HEX 2>/dev/null && echo "    policy entry removed"
    fi
    sudo bpftool map update pinned "$PINDIR/maps/thalyx_enforcing" \
        key 0 0 0 0 value 0 0 0 0 2>/dev/null && echo "    back to observe mode"
    sudo rmdir "$CGROUP" 2>/dev/null && echo "    cgroup removed"
}

if ! sudo test -d "$PINDIR/lsm"; then
    red "thalyx-lsm is not attached. Run 'make load' first."
    exit 1
fi

# ---------------------------------------------------------------- the cgroup

echo "==> creating a cgroup for the test"
sudo mkdir -p "$CGROUP" || { red "could not create $CGROUP"; exit 1; }
trap cleanup EXIT

# The cgroup id the kernel reports through bpf_get_current_cgroup_id() is the
# inode number of the cgroup directory.
CGID=$(stat -c %i "$CGROUP")
echo "    cgroup id $CGID"

# Confirm a process placed there really lands in it. If this disagrees, the
# policy would be written against an id nothing runs under, and every result
# after it would be meaningless while looking like a failed denial.
LANDED=$(sudo sh -c "echo \$\$ > $CGROUP/cgroup.procs; cat /proc/self/cgroup" 2>&1 | head -1)
case "$LANDED" in
    *thalyx-demo*) echo "    a process placed there confirms it: $LANDED" ;;
    *) red "    a process moved into the cgroup does not report being in it:"
       red "    $LANDED"
       exit 1 ;;
esac

# bpftool takes keys and values as little-endian byte sequences.
u64_hex() {
    printf '%016x' "$1" | sed 's/\(..\)/\1 /g' | awk '{for(i=NF;i>0;i--) printf "%s ", $i}'
}
u32_hex() {
    printf '%08x' "$1" | sed 's/\(..\)/\1 /g' | awk '{for(i=NF;i>0;i--) printf "%s ", $i}'
}

KEY_HEX=$(u64_hex "$CGID")

# struct policy { __u32 allowed; __u32 flags; __u64 expires_ns; }
#
# allowed = FS_READ | FS_WRITE = 6. Network is deliberately absent: the module
# may touch files and may not touch the network, which is the shape of a real
# grant. Denying the filesystem too would stop the test process from starting.
VALUE_HEX="$(u32_hex 6)$(u32_hex 0)$(u64_hex 0)"

echo
echo "==> writing the policy: filesystem allowed, network denied"
sudo bpftool map update pinned "$PINDIR/maps/thalyx_policy" \
    key hex $KEY_HEX value hex $VALUE_HEX || { red "could not write the policy"; exit 1; }

# ---------------------------------------------------------------- baseline

echo
echo "==> enforcement OFF — the policy is in the map but not applied"
BEFORE=$(connect_inside)
echo "    inside:   $BEFORE"

# ---------------------------------------------------------------- enforcing

echo
echo "==> enforcement ON"
sudo bpftool map update pinned "$PINDIR/maps/thalyx_enforcing" \
    key 0 0 0 0 value 1 0 0 0 || { red "could not enable enforcement"; exit 1; }

INSIDE=$(connect_inside)
OUTSIDE=$(connect_outside)

echo "    inside:   $INSIDE"
echo "    outside:  $OUTSIDE"

# ---------------------------------------------------------------- verdict

echo
echo "=============================================="

denied_inside=0
allowed_outside=0
unaffected_before=0

case "$BEFORE"  in errno=111*) unaffected_before=1 ;; esac
case "$INSIDE"  in "errno=1 "*) denied_inside=1 ;; esac
case "$OUTSIDE" in errno=111*) allowed_outside=1 ;; esac

if [ "$denied_inside" -eq 1 ] && [ "$allowed_outside" -eq 1 ] && [ "$unaffected_before" -eq 1 ]; then
    green "ENFORCEMENT IS REAL."
    echo
    echo "Same cgroup, same policy, same destination. The only thing that"
    echo "changed was the enforcement flag, and the failure changed from"
    echo "ECONNREFUSED to EPERM. Outside the cgroup nothing changed at all."
    echo
    echo "That is the just-in-time permission primitive doing what it was"
    echo "decreed to do, in the kernel, on real hardware."
    exit 0
fi

red "NOT PROVEN"
echo
if [ "$unaffected_before" -eq 0 ]; then
    echo "Before enforcement was on, the connection did not fail the way an"
    echo "unfiltered one should (expected errno=111). Got: $BEFORE"
    echo "Something other than Thalyx is affecting this connection, so nothing"
    echo "measured afterwards would mean anything."
fi
if [ "$denied_inside" -eq 0 ]; then
    echo "With enforcement on, the connection inside the cgroup was not denied."
    echo "Got: $INSIDE  (expected errno=1)"
    echo "Either the program is not attached to socket_connect, or the policy"
    echo "key does not match the cgroup id the kernel reports."
    echo "Check:  make status"
fi
if [ "$allowed_outside" -eq 0 ]; then
    echo "Outside the cgroup the connection did not behave as an unfiltered one."
    echo "Got: $OUTSIDE  (expected errno=111)"
    echo "That would mean the denial is not targeted, which is worse than a"
    echo "denial that fails to fire."
fi
exit 1
