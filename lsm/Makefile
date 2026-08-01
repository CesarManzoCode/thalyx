# Build and load thalyx-lsm.
#
# Run this inside the development guest, never on the host. A policy that
# denies the wrong thing locks the machine until reboot.
#
#   make            compile the BPF object
#   make load       attach it to the LSM hooks
#   make unload     detach it
#   make enforce    switch from observing to denying
#   make observe    log what would be denied, deny nothing
#   make status     what is attached and what it is doing
#
# `make load` deliberately lands in observe mode. A security policy should be
# measurable before it is binding: run the system, see what it *would* have
# denied, and only then turn it on.

CLANG   ?= clang
BPFTOOL ?= bpftool
ARCH    := $(shell uname -m | sed 's/x86_64/x86/;s/aarch64/arm64/')

OBJ     := thalyx_lsm.bpf.o
SRC     := thalyx_lsm.bpf.c
VMLINUX := vmlinux.h

BPFFS   := /sys/fs/bpf
PINDIR  := $(BPFFS)/thalyx

CFLAGS := -O2 -g -Wall -Werror -target bpf -D__TARGET_ARCH_$(ARCH) -I.

.PHONY: all load unload enforce observe status clean

all: $(OBJ)

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

$(OBJ): $(SRC) $(VMLINUX)
	$(CLANG) $(CFLAGS) -c $(SRC) -o $(OBJ)

load: $(OBJ)
	@grep -q bpf /sys/kernel/security/lsm || { \
	    echo "bpf is not in the active LSM order: $$(cat /sys/kernel/security/lsm)"; \
	    echo "The hooks exist but nothing can attach. Add bpf to lsm= and reboot."; \
	    exit 1; }
	@mountpoint -q $(BPFFS) || sudo mount -t bpf bpf $(BPFFS)
	sudo mkdir -p $(PINDIR)
	sudo $(BPFTOOL) prog loadall $(OBJ) $(PINDIR) autoattach
	@# Observe first. Enforcing from the start would mean the first run of an
	@# untested policy is also the one that can lock you out.
	@sudo $(BPFTOOL) map update pinned $(PINDIR)/thalyx_enforcing \
	    key 0 0 0 0 value 0 0 0 0 2>/dev/null || true
	@echo
	@echo "attached, observing. Nothing is denied yet."
	@echo "  make status    see what it would deny"
	@echo "  make enforce   start denying"

unload:
	sudo rm -rf $(PINDIR)
	@echo "detached"

enforce:
	sudo $(BPFTOOL) map update pinned $(PINDIR)/thalyx_enforcing \
	    key 0 0 0 0 value 1 0 0 0
	@echo "enforcing: denials are now real"

observe:
	sudo $(BPFTOOL) map update pinned $(PINDIR)/thalyx_enforcing \
	    key 0 0 0 0 value 0 0 0 0
	@echo "observing: denials are logged, not applied"

status:
	@echo "LSM order:  $$(cat /sys/kernel/security/lsm)"
	@if [ -d $(PINDIR) ]; then \
	    echo "attached:   yes"; \
	    printf 'mode:       '; \
	    if sudo $(BPFTOOL) map dump pinned $(PINDIR)/thalyx_enforcing 2>/dev/null \
	        | grep -q '"value": \["0x01"'; then echo enforcing; else echo observing; fi; \
	    echo; \
	    echo "policy entries:"; \
	    sudo $(BPFTOOL) map dump pinned $(PINDIR)/thalyx_policy 2>/dev/null || echo "  (none)"; \
	else \
	    echo "attached:   no"; \
	fi

clean:
	rm -f $(OBJ) $(VMLINUX)
