# The machine.
#
#   make            everything below, in order
#   make kernel     fetch and build Linux with our configuration
#   make binary     build thalyx, statically linked
#   make image      pack the root filesystem
#   make run        boot it in QEMU
#   make count      list what is really inside the image
#
# ─────────────────────────────────────────────────────────────────────────────
# WHAT HAS RUN AND WHAT HAS NOT
#
# `make image` and `make count` are `thalyx dev image`, which is Rust with tests
# and has been exercised.
#
# The kernel side has now been run once, but not from here and not as this file
# runs it: 6.12.101 was configured and built by hand in the development
# container from a GitHub mirror, because kernel.org is not reachable from it.
# What that proves is the configuration — every option survived, and the
# resulting vmlinux carries .BTF. What it does not prove is GCC 15, since the
# container has GCC 13, so the C23 problem this version bump exists to fix
# cannot reproduce there either way.
#
# QEMU has still never run. `make run` has no evidence behind it at all.
#
# Stated here rather than in a footnote because the project's own rules say not
# to stack unverified work and stay quiet about it.
# ─────────────────────────────────────────────────────────────────────────────

ROOT    := $(CURDIR)/..
BUILD   := $(CURDIR)/build
TARGET  := x86_64-unknown-linux-musl

# 6.12.9 does not build with GCC 15, which is what Fedora 43 ships. GCC 15
# defaults to C23, where `bool`, `true` and `false` are keywords, and
# arch/x86/boot/compressed/ was the one directory that never passed `-std=`.
#
# It cannot be fixed from here: that Makefile opens with `KBUILD_CFLAGS :=`,
# which throws away everything from above, so KCFLAGS never reaches it. Upstream
# added the missing `-std=gnu11` in January 2025 and it landed in the stable
# series at 6.12.14 — checked tag by tag, not assumed.
#
# 6.12 is the LTS line this image was decreed on; .101 is its head.
KVERSION := 6.12.101
KSOURCE  := $(BUILD)/linux-$(KVERSION)
KTARBALL := $(BUILD)/linux-$(KVERSION).tar.xz
KURL     := https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-$(KVERSION).tar.xz
BZIMAGE  := $(BUILD)/bzImage

THALYX    := $(ROOT)/target/$(TARGET)/release/thalyx
INITRAMFS := $(BUILD)/initramfs.cpio
DISK      := $(BUILD)/store.qcow2
MEMORY    := 2048

.PHONY: all kernel binary image run count clean toolchain config-check

all: kernel binary image
	@echo
	@echo "  make run"

$(BUILD):
	mkdir -p $(BUILD)

# ───────────────────────────────────────────────────────────────── the kernel

$(KTARBALL): | $(BUILD)
	curl -fL -o $@ $(KURL)

$(KSOURCE): $(KTARBALL)
	tar -C $(BUILD) -xf $<
	touch $@

# Built from our configuration rather than a distribution's.
#
# `allnoconfig` starts from nothing and turns on only what thalyx.config names,
# which is the opposite of taking a distro kernel and switching things off. What
# is in this kernel is what somebody decided to put there.
$(BZIMAGE): $(KSOURCE) thalyx.config | $(BUILD)
	cp thalyx.config $(KSOURCE)/.config-fragment
	$(MAKE) -C $(KSOURCE) allnoconfig
	cat $(KSOURCE)/.config-fragment >> $(KSOURCE)/.config
	$(MAKE) -C $(KSOURCE) olddefconfig
	@$(MAKE) --no-print-directory config-check
	$(MAKE) -C $(KSOURCE) -j$$(nproc) bzImage
	cp $(KSOURCE)/arch/x86/boot/bzImage $@

# Asking for an option is not having it.
#
# `olddefconfig` drops, without a word, every option whose dependencies are not
# met — the line is simply not in the .config afterwards. Nine of thalyx.config's
# options were being lost that way, including BPF_LSM and DEBUG_INFO_BTF: the
# kernel would have built, booted, and mounted everything, and the only sign
# would have been thalyx-lsm failing to attach, three steps later, with the
# blame landing on the loader.
#
# So this fails the build instead of the boot. It compares every line asked for
# against the line that came out, which also catches an option renamed or
# retired between kernel versions — the failure a version bump actually causes.
config-check:
	@test -r $(KSOURCE)/.config || { \
	    echo "  no .config to check at $(KSOURCE) — the kernel was never"; \
	    echo "  configured. That is a different failure from an option being"; \
	    echo "  dropped, and saying so is the point."; \
	    exit 1; \
	}
	@missing=$$(grep -v '^#' thalyx.config | grep '=' | while read -r want; do \
	        grep -qxF "$$want" $(KSOURCE)/.config || echo "    $$want"; \
	    done); \
	if [ -n "$$missing" ]; then \
	    echo; \
	    echo "  These were asked for and are not in the kernel's .config:"; \
	    echo "$$missing"; \
	    echo; \
	    echo "  olddefconfig discarded them: something they depend on is off,"; \
	    echo "  or the symbol no longer exists in $(KVERSION). Each one is a"; \
	    echo "  line thalyx.config should have had next to it."; \
	    echo; \
	    exit 1; \
	fi; \
	echo "  config: every option in thalyx.config survived olddefconfig"

kernel: $(BZIMAGE)

# ───────────────────────────────────────────────────────────────── the program

toolchain:
	rustup target add $(TARGET)

# Statically linked, so the image needs no dynamic loader and no libc on disk.
#
# musl is compiled in, not shipped. Nothing in the image is a program somebody
# could invoke, because there is nobody to invoke it.
$(THALYX): toolchain
	cd $(ROOT) && cargo build --release --target $(TARGET) -p thalyx-cli

binary: $(THALYX)
	@file $(THALYX) | grep -q 'statically linked' \
	    || { echo "NOT STATIC — the image would need a libc on disk"; exit 1; }
	@echo "  static: $$(du -h $(THALYX) | cut -f1)"

# ─────────────────────────────────────────────────────────────────── the image

$(INITRAMFS): $(THALYX) | $(BUILD)
	$(THALYX) dev image --binary $(THALYX) --out $@

image: $(INITRAMFS)

count: $(INITRAMFS)
	@$(THALYX) dev image --list $(INITRAMFS)

# ─────────────────────────────────────────────────────────────────── the store
#
# Persistent state lives on its own disk, so the root can be a tmpfs that keeps
# nothing. The three subvolumes are the ones Core-Nucleo decrees; PID 1 mounts
# them, and does not create them, because a machine that silently made itself a
# store on every boot could never tell you it had lost one.

$(DISK): | $(BUILD)
	qemu-img create -f qcow2 $@ 8G

# ────────────────────────────────────────────────────────────────────── boot
#
# No bootloader and no partition table: QEMU is handed the kernel and the
# archive. `lsm=` puts bpf in the LSM order, which thalyx-lsm needs and which no
# distribution enables by default.

run: $(BZIMAGE) $(INITRAMFS) $(DISK)
	qemu-system-x86_64 \
	    -m $(MEMORY) \
	    -smp 2 \
	    -kernel $(BZIMAGE) \
	    -initrd $(INITRAMFS) \
	    -drive file=$(DISK),if=virtio,format=qcow2 \
	    -nographic \
	    -append "console=ttyS0 lsm=capability,bpf panic=-1"

clean:
	rm -rf $(BUILD)
