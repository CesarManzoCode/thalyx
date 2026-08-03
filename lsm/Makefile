# Build and load thalyx-lsm.
#
#   make            compile both objects
#   make hooks      list which LSM hooks this kernel actually exposes
#   make load       attach enforcement (and the watcher, if its hooks exist)
#   make unload     detach everything
#   make enforce    switch from observing to denying
#   make observe    log what would be denied, deny nothing
#   make status     what is attached and what it is doing
#
# `make load` deliberately lands in observe mode. A security policy should be
# measurable before it is binding: run the system, see what it *would* have
# denied, and only then turn it on.
#
# Two objects, loaded independently:
#
#   thalyx_lsm    enforcement. Required.
#   thalyx_watch  filesystem events for the semantic index. Best effort — it
#                 denies nothing, and which filesystem hooks a kernel exposes
#                 depends on how it was configured, so it must not be able to
#                 stop enforcement from loading.

CLANG   ?= clang
BPFTOOL ?= bpftool
ARCH    := $(shell uname -m | sed 's/x86_64/x86/;s/aarch64/arm64/')

VMLINUX := vmlinux.h
OBJS    := thalyx_lsm.bpf.o thalyx_watch.bpf.o

BPFFS   := /sys/fs/bpf
PINDIR  := $(BPFFS)/thalyx
MAPDIR  := $(PINDIR)/maps

# -Werror applies to this project's code. vmlinux.h is machine-generated and
# suspends the relevant warning around its own include.
CFLAGS := -O2 -g -Wall -Werror -target bpf -D__TARGET_ARCH_$(ARCH) -I.

.PHONY: all hooks load unload enforce observe status demo diagnose clean

all: $(OBJS)

# Generated from the running kernel's own BTF, so the program compiles against
# the exact structure layouts it will run on. This is what makes one object
# file work across kernel versions (CO-RE) instead of needing a rebuild per
# kernel.
$(VMLINUX):
	@echo "==> generating vmlinux.h from the running kernel"
	@test -f /sys/kernel/btf/vmlinux || { \
	    echo "no kernel BTF at /sys/kernel/btf/vmlinux — this kernel cannot run CO-RE programs"; \
	    exit 1; }
	$(BPFTOOL) btf dump file /sys/kernel/btf/vmlinux format c > $(VMLINUX)

%.bpf.o: %.bpf.c $(VMLINUX)
	$(CLANG) $(CFLAGS) -c $< -o $@

# Which hooks this kernel exposes.
#
# A BPF LSM program can only attach where the kernel provides a bpf_lsm_<hook>
# symbol, and that set depends on how the kernel was configured. Asking first
# is faster than discovering it from a verifier error.
ENFORCE_HOOKS := socket_connect file_open
WATCH_HOOKS   := inode_create inode_unlink inode_rename inode_mkdir inode_rmdir \
                 inode_symlink inode_link inode_mknod inode_setattr file_permission

hooks:
	@echo "LSM order: $$(cat /sys/kernel/security/lsm 2>/dev/null || echo unknown)"
	@echo
	@echo "enforcement needs:"
	@for hook in $(ENFORCE_HOOKS); do \
	    if grep -q " bpf_lsm_$$hook$$" /proc/kallsyms 2>/dev/null; then \
	        printf '  \033[32mavailable\033[0m  %s\n' "$$hook"; \
	    else \
	        printf '  \033[31mmissing\033[0m    %s\n' "$$hook"; \
	    fi; \
	done
	@echo
	@# The watcher is all-or-nothing on purpose. One missing hook is one way a
	@# file can change without the counter moving, and a counter with a hole in
	@# it is worse than no counter: it would let the index answer "current"
	@# about a tree that had been rewritten underneath it.
	@echo "the watcher needs all of these, or it declines to load:"
	@for hook in $(WATCH_HOOKS); do \
	    if grep -q " bpf_lsm_$$hook$$" /proc/kallsyms 2>/dev/null; then \
	        printf '  \033[32mavailable\033[0m  %s\n' "$$hook"; \
	    else \
	        printf '  \033[31mmissing\033[0m    %s\n' "$$hook"; \
	    fi; \
	done
	@echo
	@echo "total attachable LSM hooks on this kernel: $$(grep -c ' bpf_lsm_' /proc/kallsyms 2>/dev/null || echo '?')"

load: $(OBJS)
	@# Refuse to load over a live attachment rather than failing halfway with
	@# a message about pinning.
	@if sudo test -d $(PINDIR)/lsm; then \
	    echo "thalyx-lsm is already attached."; \
	    echo "  make status    see what it is doing"; \
	    echo "  make unload    detach it first, then load again"; \
	    exit 1; \
	fi
	@grep -q bpf /sys/kernel/security/lsm || { \
	    echo "bpf is not in the active LSM order: $$(cat /sys/kernel/security/lsm)"; \
	    echo "The hooks exist but nothing can attach. Add bpf to lsm= and reboot."; \
	    exit 1; }
	@mountpoint -q $(BPFFS) || sudo mount -t bpf bpf $(BPFFS)
	sudo mkdir -p $(PINDIR)
	@echo "==> loading enforcement"
	@# Maps get an explicit pin directory. Left implicit, libbpf places them
	@# under its own root while the loader places programs elsewhere, the two
	@# fight, and the load fails with a message that names neither.
	sudo $(BPFTOOL) prog loadall thalyx_lsm.bpf.o $(PINDIR)/lsm \
	    pinmaps $(MAPDIR) autoattach
	@# Observe first. Enforcing from the start would mean the first run of an
	@# untested policy is also the one that can lock you out.
	@sudo $(BPFTOOL) map update pinned $(MAPDIR)/thalyx_enforcing \
	    key 0 0 0 0 value 0 0 0 0 2>/dev/null || true
	@echo "==> loading the watcher (optional)"
	@sudo $(BPFTOOL) prog loadall thalyx_watch.bpf.o $(PINDIR)/watch \
	    pinmaps $(MAPDIR) autoattach \
	    && echo "    attached" \
	    || echo "    not attached — this kernel does not expose one of its hooks. \
Enforcement is unaffected; run 'make hooks' to see which."
	@echo
	@echo "attached, observing. Nothing is denied yet."
	@echo "  make status    see what it would deny"
	@echo "  make enforce   start denying"

unload:
	sudo rm -rf $(PINDIR)
	@# Also sweep stray pins at the bpffs root. An earlier build declared its
	@# maps LIBBPF_PIN_BY_NAME, which put them here rather than under $(PINDIR);
	@# one left behind makes every later load fail with an error that points at
	@# programs instead of at the map that is really in the way.
	@for stray in thalyx_policy thalyx_denials thalyx_enforcing thalyx_mutations \
	              thalyx_mutation_count thalyx_watched thalyx_unattributed; do \
	    if [ -e $(BPFFS)/$$stray ]; then \
	        sudo rm -f $(BPFFS)/$$stray && echo "  removed stray pin $$stray"; \
	    fi; \
	done
	@echo "detached"

enforce:
	sudo $(BPFTOOL) map update pinned $(MAPDIR)/thalyx_enforcing \
	    key 0 0 0 0 value 1 0 0 0
	@echo "enforcing: denials are now real"

observe:
	sudo $(BPFTOOL) map update pinned $(MAPDIR)/thalyx_enforcing \
	    key 0 0 0 0 value 0 0 0 0
	@echo "observing: denials are logged, not applied"

status:
	@echo "LSM order:  $$(cat /sys/kernel/security/lsm)"
	@# `sudo test`, not `test`: bpffs is mode 700 root-owned, so an unprivileged
	@# check reports "not attached" for something that is very much attached —
	@# a security tool reading as disarmed while it is armed.
	@if sudo test -d $(PINDIR)/lsm; then \
	    echo "enforcement: attached"; \
	    printf 'mode:        '; \
	    if sudo $(BPFTOOL) map dump pinned $(MAPDIR)/thalyx_enforcing 2>/dev/null \
	        | grep -q '"value": \["0x01"'; then echo enforcing; else echo observing; fi; \
	else \
	    echo "enforcement: not attached"; \
	fi
	@# Which of the watcher's programs are actually pinned. The load is
	@# all-or-nothing, so this is normally all or none — it is printed because
	@# "the watcher is attached" and "the watcher sees everything" are
	@# different claims, and only the second one lets the index skip a walk.
	@if sudo test -d $(PINDIR)/watch; then \
	    echo "watcher:     attached ($$(sudo ls $(PINDIR)/watch | wc -l)/$(words $(WATCH_HOOKS)) hooks)"; \
	    echo "             thalyx graph status  shows what it has counted"; \
	 else echo "watcher:     not attached"; fi
	@echo
	@# A pinned program that is not linked to a hook enforces nothing. The
	@# pins prove it loaded; only a link proves it is in the path.
	@printf 'live LSM links: '
	@sudo $(BPFTOOL) link list 2>/dev/null | grep -c 'prog_type lsm' || echo 0
	@echo
	@echo "policy entries:"
	@sudo $(BPFTOOL) map dump pinned $(MAPDIR)/thalyx_policy 2>/dev/null || echo "  (none)"

# What a failing load actually said.
#
# bpftool reports "failed to pin all programs" for several unrelated causes.
# libbpf's own debug output names the real one.
diagnose: $(OBJS)
	@echo "==> existing pins under $(BPFFS)"
	@sudo find $(BPFFS) -maxdepth 3 2>/dev/null | sed 's|^|    |' || echo "    (none)"
	@echo
	@echo "==> loading with libbpf debug output"
	-sudo $(BPFTOOL) -d prog loadall thalyx_lsm.bpf.o /sys/fs/bpf/thalyx-diagnose \
	    pinmaps /sys/fs/bpf/thalyx-diagnose-maps autoattach
	@echo
	@echo "==> cleaning up the diagnostic pins"
	@sudo rm -rf /sys/fs/bpf/thalyx-diagnose /sys/fs/bpf/thalyx-diagnose-maps

# Prove enforcement denies something. Safe on a working machine: the policy
# names one throwaway cgroup and everything else is untouched.
demo:
	@bash demo-enforcement.sh

clean:
	rm -f $(OBJS) $(VMLINUX)
