#!/bin/sh
# Check whether this machine can develop and run Thalyx's BPF LSM.
#
# Every check prints what it looked at and what to do about it, because a
# preflight that only says "FAIL" makes you go read its source.
#
# Exit status: 0 if everything required passed, 1 otherwise.

set -u

ok=0
warn=0
fail=0

pass()  { printf '  \033[32mok\033[0m    %s\n' "$1"; ok=$((ok + 1)); }
note()  { printf '  \033[33mwarn\033[0m  %s\n' "$1"; warn=$((warn + 1)); }
bad()   { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; fail=$((fail + 1)); }
hint()  { printf '        → %s\n' "$1"; }

echo
echo "Thalyx development preflight"
echo "============================"
echo

# ---------------------------------------------------------------- CPU and KVM

echo "Virtualisation"

if grep -qE '^flags.*\b(svm|vmx)\b' /proc/cpuinfo; then
    pass "CPU reports hardware virtualisation support"
else
    bad "CPU does not report svm (AMD) or vmx (Intel)"
    hint "On AMD boards this is usually 'SVM Mode' in the BIOS, under"
    hint "Advanced → CPU Configuration. It ships disabled on many A320 boards."
fi

if [ -e /dev/kvm ]; then
    if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
        pass "/dev/kvm is present and usable by this user"
    else
        bad "/dev/kvm exists but this user cannot use it"
        hint "sudo usermod -aG kvm \$USER   (then log out and back in)"
    fi
else
    bad "/dev/kvm does not exist"
    hint "Enable SVM in the BIOS, then check the kvm_amd module is loaded:"
    hint "  sudo modprobe kvm_amd && dmesg | grep -i kvm"
fi

cores=$(nproc 2>/dev/null || echo 1)
if [ "$cores" -ge 4 ]; then
    pass "$cores CPU threads available"
else
    note "$cores CPU threads — builds will be slow but will work"
fi

echo

# ---------------------------------------------------------------- Memory, disk

echo "Resources"

total_kb=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
total_gb=$((total_kb / 1024 / 1024))
if [ "$total_gb" -ge 8 ]; then
    pass "${total_gb} GB RAM (a 6 GB guest leaves room for the host)"
else
    note "${total_gb} GB RAM — use a smaller guest, 2 GB should still boot"
fi

free_gb=$(df -BG --output=avail . 2>/dev/null | tail -1 | tr -dc '0-9')
free_gb=${free_gb:-0}
if [ "$free_gb" -ge 40 ]; then
    pass "${free_gb} GB free on this filesystem"
elif [ "$free_gb" -ge 20 ]; then
    note "${free_gb} GB free — enough for the VM, tight if you later build a kernel"
    hint "A kernel tree with debug symbols is roughly 25 GB on its own."
else
    bad "${free_gb} GB free — not enough for a development VM"
    hint "Budget about 20 GB: the guest image grows as it is used."
fi

echo

# ---------------------------------------------------------------- Host tooling

echo "Host tooling"

for tool in qemu-system-x86_64 qemu-img; do
    if command -v "$tool" >/dev/null 2>&1; then
        pass "$tool found"
    else
        bad "$tool not found"
        hint "Debian/Ubuntu: sudo apt install qemu-system-x86 qemu-utils"
        hint "Arch:           sudo pacman -S qemu-full"
        hint "Fedora:         sudo dnf install qemu-kvm qemu-img"
    fi
done

for tool in curl ssh; do
    if command -v "$tool" >/dev/null 2>&1; then
        pass "$tool found"
    else
        bad "$tool not found"
    fi
done

if command -v cloud-localds >/dev/null 2>&1 || command -v genisoimage >/dev/null 2>&1 \
   || command -v xorriso >/dev/null 2>&1; then
    pass "an ISO builder is available for the cloud-init seed"
else
    bad "no ISO builder found (cloud-localds, genisoimage or xorriso)"
    hint "Debian/Ubuntu: sudo apt install cloud-image-utils"
    hint "Arch:           sudo pacman -S cdrtools"
fi

if command -v cargo >/dev/null 2>&1; then
    pass "cargo found ($(cargo --version 2>/dev/null | cut -d' ' -f2))"
else
    bad "cargo not found"
    hint "https://rustup.rs"
fi

echo

# ------------------------------------------------------- Host kernel (optional)

echo "Host kernel (informational — the LSM runs in the guest, not here)"

host_kernel=$(uname -r)
pass "running $host_kernel"

if [ -r /sys/kernel/security/lsm ]; then
    active=$(cat /sys/kernel/security/lsm 2>/dev/null)
    if echo "$active" | grep -q bpf; then
        pass "host has BPF LSM active ($active)"
    else
        note "host LSM order does not include bpf ($active)"
        hint "Irrelevant for development: the guest is configured separately."
    fi
else
    note "securityfs not mounted, cannot read the host LSM order"
fi

echo
echo "============================"
printf '%d passed, %d warnings, %d failures\n' "$ok" "$warn" "$fail"
echo

if [ "$fail" -gt 0 ]; then
    echo "Fix the failures above, then run this again."
    echo "Nothing here needs the BIOS except SVM, and nothing needs a reboot"
    echo "except enabling it."
    exit 1
fi

echo "Ready. Next: make -C dev vm"
exit 0
