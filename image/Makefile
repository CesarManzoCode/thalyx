# The machine.
#
#   make                  everything below, in order
#   make kernel           fetch and build Linux with our configuration
#   make binary           build thalyx, statically linked
#   make image            pack the root filesystem
#   make store-stage      build what goes on the store disk
#   sudo make store       format the disk and copy that onto it
#   make run              boot it in QEMU
#   make count            list what is really inside the image
#
# `store` is the one that needs root, and the only one. It will not build
# anything: see the note above it.
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
MEMORY    := 2048

# The store disk. Raw and not qcow2: a raw image can be formatted and mounted
# with mkfs.btrfs and a loop device, which every Linux has, while qcow2 needs
# `qemu-nbd`, a kernel module and a device to attach it to. The disk is sparse,
# so eight gigabytes of it occupy what is written and nothing more.
STORE     := $(BUILD)/store.img
STORESIZE := 8G
STAGE     := $(BUILD)/store-stage
STAMP     := $(BUILD)/store-stage.done
MNT       := $(BUILD)/store-mnt
STOREDEV  := /dev/vda

# The first module. It is not in the image and must not be: the image carries
# the kernel and one program. A module lives on the store, which is the whole
# distinction between what Thalyx *is* and what has been installed on it.
GREETER   := $(ROOT)/target/$(TARGET)/release/greeter

.PHONY: all kernel binary image run count clean toolchain config-check \
        store store-stage greeter

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
# nothing. The three subvolumes are the ones Journal-y-Snapshots decrees; PID 1
# mounts them, and does not create them, because a machine that silently made
# itself a store on every boot could never tell you it had lost one.
#
# So they are made here, once, on a machine that has btrfs-progs. The image does
# not have btrfs-progs and cannot: it has one program in it. That is the same
# shape as the bpftool problem and it has the same answer — the work moves to
# build time rather than a second binary moving into the image.
#
# Two targets, two invocations, and `store` does not build the stage.
#
#     make -C image store-stage        # you
#     sudo make -C image store         # root
#
# It was one command, with `store` depending on `store-stage`, and that was
# wrong twice over. It failed immediately — `sudo` resets PATH, so rustup and
# cargo, which live under the user's home, are simply not there — and the
# failure is the smaller problem. Had it worked, `sudo make store` would have
# run the entire Rust build as root: every build script of every dependency,
# executing with privilege, writing root-owned files into the target directory
# that the next ordinary `cargo build` could not replace.
#
# So the privilege boundary is the target boundary. `store-stage` builds and
# needs nothing; `store` is mkfs, mount, three subvolumes and a copy, and it
# refuses rather than building anything itself.

# The module, built for the machine rather than for the host that builds it.
# Static for the same reason `thalyx` is: there is no libc on the store either.
$(GREETER): toolchain
	cd $(ROOT) && cargo build --release --target $(TARGET) -p dev-thalyx-greeter

greeter: $(GREETER)
	@file $(GREETER) | grep -q 'statically linked' \
	    || { echo "NOT STATIC — the module could not run on the machine"; exit 1; }
	@echo "  static: $$(du -h $(GREETER) | cut -f1)"

# What goes on the disk, built where anyone can look at it.
#
# The manifest grants one file, by the path it will have *inside the machine* —
# /opt/thalyx/data/greeter/notes.txt, which is the `modules` subvolume. Not the
# directory around it: the module reads whatever it was granted when nobody
# names a file, and a grant naming a directory would send it to read a directory
# and report the failure as a permissions problem.
store-stage: binary greeter
	rm -rf $(STAGE) $(STAMP)
	mkdir -p $(STAGE)/system $(STAGE)/modules/greeter $(STAGE)/user
	mkdir -p $(BUILD)/pack/bin
	cp $(GREETER) $(BUILD)/pack/bin/greeter
	$(THALYX) dev keygen --out $(BUILD)/store-signing.key
	printf '%s\n' \
	    'Thalyx es el sistema operativo. El kernel de Linux es un componente' \
	    'que Thalyx gestiona, no el anfitrion sobre el que descansa.' \
	    > $(STAGE)/modules/greeter/notes.txt
	printf '%s\n' \
	    'format_version = 1' \
	    'id             = "dev.thalyx.greeter"' \
	    'name           = "Greeter"' \
	    'version        = "1.0.0"' \
	    'description    = "The first module written against Thalyx internal API"' \
	    'license        = "GPL-3.0-or-later"' \
	    'publisher_key  = "ed25519:0000000000000000000000000000000000000000000000000000000000000000"' \
	    'distribution   = "prebuilt"' \
	    '' \
	    '[artifact]' \
	    'hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
	    'size = 0' \
	    '' \
	    '[requires]' \
	    'thalyx = ">=0.1.0"' \
	    '' \
	    '[[permissions]]' \
	    'resource = "/opt/thalyx/data/greeter/notes.txt"' \
	    'action   = "read"' \
	    'type     = "persistent"' \
	    '' \
	    '[entrypoints]' \
	    'run = "bin/greeter"' \
	    > $(BUILD)/greeter-manifest.toml
	$(THALYX) dev pack $(BUILD)/pack --manifest $(BUILD)/greeter-manifest.toml \
	    --key $(BUILD)/store-signing.key --out $(BUILD)/greeter.thmod
	THALYX_ROOT=$(STAGE)/system $(THALYX) module install $(BUILD)/greeter.thmod --yes
	@THALYX_ROOT=$(STAGE)/system $(THALYX) module list | grep -q dev.thalyx.greeter \
	    || { echo "the module did not install into the stage"; exit 1; }
	@# Written last, and only on success. `store` looks for this and not for
	@# the directory: an interrupted stage leaves the directory sitting there
	@# looking finished, and a disk built from it would be missing whatever
	@# had not happened yet.
	@date -u +%Y-%m-%dT%H:%M:%SZ > $(STAMP)
	@echo
	@echo "  staged: $(STAGE)"
	@echo "  now:    sudo make -C image store"

# The disk itself. Needs root, for the loop device and for the mount.
#
# `current` is a relative symlink and nothing else on the store records an
# absolute path of its own, which is what lets the stage be built at one path
# and mounted at another. If that stops being true, this copy is where it will
# show, and it will show as a module that installed and cannot be found.
store:
	@test -f $(STAMP) || { \
	    echo "  nothing staged yet. Build it first, as yourself:"; \
	    echo; \
	    echo "      make -C image store-stage"; \
	    echo; \
	    echo "  This target does not do it for you on purpose. It runs as"; \
	    echo "  root, and a Rust build under root executes every dependency's"; \
	    echo "  build script with privilege and leaves root-owned files in"; \
	    echo "  the target directory."; \
	    exit 1; \
	}
	@test "$$(id -u)" = 0 || { \
	    echo "  make store needs root: it formats a disk and mounts it."; \
	    echo "  sudo make -C image store"; \
	    exit 1; \
	}
	@command -v mkfs.btrfs > /dev/null || { \
	    echo "  no mkfs.btrfs. Thalyx requires Btrfs; install btrfs-progs."; \
	    exit 1; \
	}
	rm -f $(STORE)
	truncate -s $(STORESIZE) $(STORE)
	mkfs.btrfs -q -L thalyx-store $(STORE)
	mkdir -p $(MNT)
	mount -o loop $(STORE) $(MNT)
	btrfs subvolume create $(MNT)/system
	btrfs subvolume create $(MNT)/modules
	btrfs subvolume create $(MNT)/user
	cp -a $(STAGE)/system/. $(MNT)/system/
	cp -a $(STAGE)/modules/. $(MNT)/modules/
	cp -a $(STAGE)/user/. $(MNT)/user/
	@# `cp -a` keeps the ownership of the stage, which is whoever ran
	@# `store-stage` — a uid that does not exist inside the machine. A store
	@# built by Thalyx itself is root's, because installing is something
	@# Thalyx does as Thalyx, and this makes the built one indistinguishable
	@# from that. Nothing here becomes writable by a module: the uids handed
	@# out start at 700000 and this is 0.
	chown -R 0:0 $(MNT)/system $(MNT)/modules $(MNT)/user
	btrfs subvolume list $(MNT)
	umount $(MNT)
	@echo
	@echo "  store: $(STORE) — three subvolumes, one module installed"

# ────────────────────────────────────────────────────────────────────── boot
#
# No bootloader and no partition table: QEMU is handed the kernel and the
# archive. `lsm=` puts bpf in the LSM order, which thalyx-lsm needs and which no
# distribution enables by default. `thalyx.store=` names the disk rather than
# leaving PID 1 to probe for it — a probe finds the wrong disk exactly once, and
# the failure is Thalyx writing its store onto somebody else's filesystem.
#
# The store is a prerequisite that is deliberately not built here: `make store`
# needs root, and a `run` that quietly asked for a password would be a `run`
# nobody could put in a script.
#
# But it refuses rather than warning. It used to print a note and boot anyway,
# and the note scrolled past above a kernel's worth of output — so the machine
# came up saying it had no disk, which reads as the store being broken when
# what happened is that nobody made one. Booting with no store is a real thing
# to want; it is `make run STORELESS=1`, and then the machine's own message is
# the answer rather than a mystery.
#
# The disk is checked for readability, not just existence. `sudo make store`
# leaves it owned by root, and QEMU started as an ordinary user would fail to
# open it with an error about the file rather than about the permission.

run: $(BZIMAGE) $(INITRAMFS)
	@if [ -z "$(STORELESS)" ] && [ ! -f $(STORE) ]; then \
	    echo "  no store disk at $(STORE), so nothing would survive this boot."; \
	    echo; \
	    echo "      make -C image store-stage"; \
	    echo "      sudo make -C image store"; \
	    echo; \
	    echo "  To boot without one on purpose:  make -C image run STORELESS=1"; \
	    exit 1; \
	fi
	@if [ -f $(STORE) ] && [ ! -r $(STORE) ]; then \
	    echo "  $(STORE) is there and this user cannot read it."; \
	    echo "  sudo make store leaves it owned by root. Either run QEMU with"; \
	    echo "  sudo, or: sudo chown $$(id -un) $(STORE)"; \
	    exit 1; \
	fi
	qemu-system-x86_64 \
	    -m $(MEMORY) \
	    -smp 2 \
	    -kernel $(BZIMAGE) \
	    -initrd $(INITRAMFS) \
	    $$(test -z "$(STORELESS)" && test -r $(STORE) \
	        && echo "-drive file=$(STORE),if=virtio,format=raw") \
	    -nographic \
	    -append "console=ttyS0 lsm=capability,bpf panic=-1 thalyx.store=$(STOREDEV)"

clean:
	rm -rf $(BUILD)
