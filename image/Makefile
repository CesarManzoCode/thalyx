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
# and has been exercised. Everything else in this file — the kernel fetch, the
# kernel build, QEMU — has never run: the machine it was written on has no
# network route to kernel.org and no QEMU. The first `make kernel` is also the
# first time anyone finds out what is wrong with it.
#
# Stated here rather than in a footnote because the project's own rules say not
# to stack unverified work and stay quiet about it.
# ─────────────────────────────────────────────────────────────────────────────

ROOT    := $(CURDIR)/..
BUILD   := $(CURDIR)/build
TARGET  := x86_64-unknown-linux-musl

KVERSION := 6.12.9
KSOURCE  := $(BUILD)/linux-$(KVERSION)
KTARBALL := $(BUILD)/linux-$(KVERSION).tar.xz
KURL     := https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-$(KVERSION).tar.xz
BZIMAGE  := $(BUILD)/bzImage

THALYX    := $(ROOT)/target/$(TARGET)/release/thalyx
INITRAMFS := $(BUILD)/initramfs.cpio
DISK      := $(BUILD)/store.qcow2
MEMORY    := 2048

.PHONY: all kernel binary image run count clean toolchain

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
	$(MAKE) -C $(KSOURCE) -j$$(nproc) bzImage
	cp $(KSOURCE)/arch/x86/boot/bzImage $@

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
