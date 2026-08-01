#!/bin/bash
# Verify the guest can actually enforce a BPF LSM policy.
#
# The interesting check is not "is the kernel new enough" or "is the config
# set". Those can all look right while nothing can attach, because
# distributions ship CONFIG_BPF_LSM=y and then leave bpf out of the default
# LSM order. So the last check loads a real program onto a real hook and sees
# whether the kernel accepts it.

set -u

ok=0
fail=0
pass() { printf '  \033[32mok\033[0m    %s\n' "$1"; ok=$((ok + 1)); }
bad()  { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }
hint() { printf '        → %s\n' "$1"; }

echo
echo "Thalyx guest verification"
echo "========================="
echo

echo "Kernel"
kernel=$(uname -r)
major=$(echo "$kernel" | cut -d. -f1)
minor=$(echo "$kernel" | cut -d. -f2)
if [ "$major" -gt 5 ] || { [ "$major" -eq 5 ] && [ "$minor" -ge 7 ]; }; then
    pass "$kernel is new enough for BPF LSM (needs 5.7+)"
else
    bad "$kernel predates BPF LSM"
fi

if [ -r /sys/kernel/security/lsm ]; then
    order=$(cat /sys/kernel/security/lsm)
    if echo "$order" | grep -q bpf; then
        pass "bpf is in the active LSM order ($order)"
    else
        bad "bpf is NOT in the active LSM order ($order)"
        hint "The hooks exist but nothing can attach to them."
        hint "Check /etc/default/grub.d/99-thalyx.cfg, then: sudo update-grub && sudo reboot"
    fi
else
    bad "securityfs is not mounted"
fi

if [ -d /sys/fs/bpf ] && mountpoint -q /sys/fs/bpf; then
    pass "bpffs mounted at /sys/fs/bpf (pinned maps need it)"
else
    bad "bpffs not mounted"
    hint "sudo mount -t bpf bpf /sys/fs/bpf"
fi

if [ -f /sys/kernel/btf/vmlinux ]; then
    pass "kernel BTF present (CO-RE programs need it)"
else
    bad "no kernel BTF at /sys/kernel/btf/vmlinux"
fi

echo
echo "Toolchain"
for tool in clang bpftool cargo; do
    if command -v "$tool" >/dev/null 2>&1; then
        pass "$tool found"
    else
        bad "$tool not found"
    fi
done

echo
echo "Store"
if mountpoint -q /opt/thalyx; then
    fstype=$(stat -f -c %T /opt/thalyx)
    if [ "$fstype" = "btrfs" ]; then
        pass "/opt/thalyx is Btrfs"
        subvols=$(sudo btrfs subvolume list /opt/thalyx 2>/dev/null | wc -l)
        pass "$subvols subvolume(s) present"
    else
        bad "/opt/thalyx is $fstype, not Btrfs — snapshots and rollback will not work"
    fi
else
    bad "/opt/thalyx is not mounted"
    hint "sudo systemctl start thalyx-store.service"
fi

echo
echo "Live attach test"

# Everything above can pass while attaching still fails. This is the only
# check that proves the guest can do what Thalyx needs.
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

cat > "$workdir/probe.bpf.c" <<'PROBE'
#include <linux/bpf.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

char LICENSE[] SEC("license") = "GPL";

/* Attaches to a real LSM hook and allows everything. If the kernel accepts
   this, it will accept a policy. */
SEC("lsm/socket_connect")
int BPF_PROG(probe_socket_connect, struct socket *sock,
             struct sockaddr *address, int addrlen, int ret)
{
    return ret;
}
PROBE

if clang -O2 -g -target bpf -c "$workdir/probe.bpf.c" -o "$workdir/probe.bpf.o" 2>"$workdir/err"; then
    pass "a BPF LSM program compiles"
    if sudo bpftool prog load "$workdir/probe.bpf.o" /sys/fs/bpf/thalyx_probe 2>"$workdir/err2"; then
        pass "the kernel ACCEPTED an LSM program — enforcement is available"
        sudo rm -f /sys/fs/bpf/thalyx_probe
    else
        bad "the kernel refused to load an LSM program"
        hint "$(head -3 "$workdir/err2" | tr '\n' ' ')"
        hint "Almost always: bpf missing from the lsm= boot order."
    fi
else
    bad "could not compile a BPF LSM program"
    hint "$(head -3 "$workdir/err" | tr '\n' ' ')"
    hint "Missing headers? sudo apt install libbpf-dev linux-headers-\$(uname -r)"
fi

echo
echo "========================="
printf '%d passed, %d failures\n' "$ok" "$fail"
echo
[ "$fail" -eq 0 ] && echo "Guest is ready to enforce." || echo "Fix the failures above."
exit $((fail > 0))
