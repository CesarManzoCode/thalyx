# The machine.
#
#   make                  everything below, in order
#   make kernel           fetch and build Linux with our configuration
#   make lsm              build the kernel side, which goes inside the binary
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
# QEMU has run, several times, from 2026-08-03. Each of those boots found a
# kernel option that no check here could see — BPF_LSM with BTF, then
# SECURITY_NETWORK, then FUNCTION_TRACER — which is why `hook-check` exists and
# why `dev/verify.sh` now boots the image itself instead of leaving it to
# somebody with the patience to rebuild a kernel three times.
#
# `boot` is what that stage runs, and it builds nothing: see the note above it.
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

# The digest of the kernel tarball, pinned.
#
# Thalyx compiles its own kernel, so this file is not a dependency — it becomes
# the most privileged half of the machine. It used to be fetched over HTTPS and
# built, and TLS is the wrong instrument for the question: it says who served
# the bytes, not what the bytes were. A CDN that served something else, or a
# `KVERSION` bumped without anybody looking, would produce a kernel nobody
# checked, and the image would boot and say nothing.
#
# `UNPINNED` fails the build on purpose. Filling it in is a one-time act by a
# human who verified the digest against kernel.org's **signed** list, which is
# the only step that establishes anything — a digest taken from the same
# download it is meant to check proves only that the file did not change
# between two reads of it:
#
#     curl -O https://cdn.kernel.org/pub/linux/kernel/v6.x/sha256sums.asc
#     gpg --locate-keys torvalds@kernel.org gregkh@kernel.org
#     gpg --verify sha256sums.asc
#     grep 'linux-6.12.101.tar.xz$$' sha256sums.asc
#
# then put the digest here. `make -C image pin-kernel` prints these steps.
KSHA256  := UNPINNED

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
        store store-stage greeter lsm doctor hook-check boot pin-kernel

all: doctor kernel binary image hook-check
	@echo
	@echo "  make run"

# ────────────────────────────────────────────────────────── the prerequisites
#
# Everything this needs, checked before anything is downloaded or compiled.
#
# `vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md` ends Phase 1 when a
# person outside the project boots this following only the README, with no help.
# Step 1 is booting the image. The thing that stops that person is never a hard
# problem: it is a missing package, discovered one at a time, each one after
# whatever ran before it succeeded. Downloading a kernel, extracting it,
# configuring it and compiling it is tens of minutes, and finding out at the end
# that `bc` is not installed costs all of them — twice, because the next missing
# tool is found the same way.
#
# So the failures are collected rather than hit, and reported together with the
# one line that fixes all of them. On a machine that has everything it says so
# and gets out of the way.
#
# ## Why the tool and not the package
#
# `dpkg -l` would name what is installed on a Debian derivative, and would be
# wrong twice: it is the wrong question — the property is whether the tool runs,
# not whether a package manager believes it is present — and it is one distro's
# question, on a project that refuses to belong to a distro. `command -v` and a
# compile probe ask the machine.
#
# The apt line is printed because Linux Mint is what the first outside person
# has, and a person who is told "you are missing libelf" and not which package
# carries it has been told nothing they can act on. It is help, not the check.
#
# ## What it deliberately does not check
#
# Whether the network reaches kernel.org, and whether rustup can fetch the musl
# target. Both need the network to find out, both fail with their own clear
# message, and a check that tried would be slow every time to save nothing.
#
# The package list is passed to `echo` unquoted so the shell splits it into
# words. It used to end in `| xargs`, which does the same and quietly needs
# /bin/echo to exist — an odd thing for the target whose whole job is finding
# out what does not exist to depend on without saying so.

# $(1) command  $(2) the apt package that carries it  $(3) what stops without it
define need
	@command -v $(1) > /dev/null 2>&1 || { \
	    echo "    $(1) — $(3)" >> $(BUILD)/doctor.missing; \
	    echo "$(2)" >> $(BUILD)/doctor.packages; \
	}
endef

doctor: | $(BUILD)
	@rm -f $(BUILD)/doctor.missing $(BUILD)/doctor.packages $(BUILD)/doctor.unchecked
	@touch $(BUILD)/doctor.missing $(BUILD)/doctor.packages $(BUILD)/doctor.unchecked
	$(call need,curl,curl,fetching the kernel source)
	$(call need,tar,tar,unpacking it)
	$(call need,xz,xz-utils,unpacking it — the tarball is .tar.xz)
	$(call need,gcc,build-essential,compiling the kernel)
	$(call need,ld,build-essential,linking it)
	$(call need,readelf,build-essential,checking that thalyx needs no libc)
	$(call need,bc,bc,the kernel's version arithmetic)
	$(call need,flex,flex,the kernel's configuration parser)
	$(call need,bison,bison,the same parser)
	$(call need,file,file,describing the binary that goes in the image)
	$(call need,clang,clang,the BPF programs — gcc cannot target BPF)
	$(call need,qemu-system-x86_64,qemu-system-x86,booting the machine)
	@# Not `command -v`, and the difference is not pedantry. mkfs.btrfs lives
	@# in /usr/sbin, which Debian and its derivatives stopped putting on an
	@# ordinary user's PATH — so `command -v` reports it missing on a machine
	@# that has it, and the person is sent to install what they already have.
	@# What has to be true is that *root* can run it, because `make store` is
	@# the target that uses it and that target runs under sudo.
	@command -v mkfs.btrfs > /dev/null 2>&1 \
	    || [ -x /usr/sbin/mkfs.btrfs ] || [ -x /sbin/mkfs.btrfs ] || { \
	    echo "    mkfs.btrfs — the store disk; Thalyx requires Btrfs" \
	        >> $(BUILD)/doctor.missing; \
	    echo "btrfs-progs" >> $(BUILD)/doctor.packages; \
	}
	@# pahole gets its own paragraph because its absence is the nastiest
	@# failure in this whole build. CONFIG_DEBUG_INFO_BTF depends on pahole
	@# being present, and Kconfig drops an option whose dependencies are not
	@# met *without saying so*. The kernel would build, boot, and mount
	@# everything, and the only symptom would be thalyx-lsm failing to attach
	@# several steps later — with the blame landing on the loader, which had
	@# nothing to do with it. config-check catches it now, but it catches it
	@# after the download and the configure. This catches it before.
	$(call need,pahole,dwarves,BTF — without it thalyx-lsm can never attach)
	@# Headers are not commands, so they are checked by compiling something
	@# that needs them. libelf is objtool's, and objtool is built because
	@# x86_64 unwinds with ORC.
	@if command -v gcc > /dev/null 2>&1; then \
	    echo '#include <libelf.h>' | gcc -E -xc - > /dev/null 2>&1 || { \
	        echo "    libelf.h — objtool, which every x86_64 kernel builds" \
	            >> $(BUILD)/doctor.missing; \
	        echo "libelf-dev" >> $(BUILD)/doctor.packages; \
	    }; \
	    echo '#include <openssl/bio.h>' | gcc -E -xc - > /dev/null 2>&1 || { \
	        echo "    openssl/bio.h — the kernel's own build tooling" \
	            >> $(BUILD)/doctor.missing; \
	        echo "libssl-dev" >> $(BUILD)/doctor.packages; \
	    }; \
	else \
	    echo "  gcc is not here, so the kernel's headers could not be checked" \
	        >> $(BUILD)/doctor.unchecked; \
	    echo "  at all. There may be more to install after this round." \
	        >> $(BUILD)/doctor.unchecked; \
	fi
	@# Rust is last and separate: it does not come from apt, and telling
	@# somebody to `apt install cargo` gets them a version that is older than
	@# this workspace needs and a rustup that is not there to add the musl
	@# target with.
	@command -v cargo > /dev/null 2>&1 && command -v rustup > /dev/null 2>&1 || \
	    echo "rust" >> $(BUILD)/doctor.packages
	@# The kernel digest, reported in the same round as everything else.
	@#
	@# It belongs here and not at the download for the reason this whole
	@# target exists: a person told "everything is here", who then loses the
	@# fetch and the configure before hitting a wall nobody warned them
	@# about, has been failed in exactly the way `doctor` was written to
	@# prevent. It is not an apt package, so it goes in its own marker and is
	@# explained separately — but it is found in the same run.
	@if [ "$(KSHA256)" = "UNPINNED" ]; then \
	    echo "    the kernel tarball's digest — nothing has verified what this" \
	        >> $(BUILD)/doctor.missing; \
	    echo "      build compiles into the most privileged half of the machine" \
	        >> $(BUILD)/doctor.missing; \
	    echo "kernel-pin" >> $(BUILD)/doctor.packages; \
	fi
	@if [ -s $(BUILD)/doctor.packages ]; then \
	    echo; \
	    echo "  Not here yet:"; \
	    echo; \
	    cat $(BUILD)/doctor.missing; \
	    if grep -qx rust $(BUILD)/doctor.packages; then \
	        echo "    cargo, rustup — building thalyx itself"; \
	    fi; \
	    echo; \
	    apt=$$(grep -vx rust $(BUILD)/doctor.packages | grep -vx kernel-pin | sort -u); \
	    if [ -n "$$apt" ]; then \
	        echo "  On Linux Mint, Ubuntu or Debian, all of them at once:"; \
	        echo; \
	        echo "      sudo apt install" $$apt; \
	        echo; \
	    fi; \
	    if grep -qx kernel-pin $(BUILD)/doctor.packages; then \
	        echo "  The kernel digest is not a package. Thalyx compiles its own"; \
	        echo "  kernel, so the tarball is not a dependency — it becomes the"; \
	        echo "  machine. TLS says who served the bytes, not what they were:"; \
	        echo; \
	        echo "      make -C image pin-kernel"; \
	        echo; \
	        echo "  prints the four commands and where to put the answer."; \
	        echo; \
	    fi; \
	    if grep -qx rust $(BUILD)/doctor.packages; then \
	        echo "  Rust does not come from apt. The version apt has is older"; \
	        echo "  than this needs, and brings no rustup to add the musl"; \
	        echo "  target with:"; \
	        echo; \
	        echo "      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"; \
	        echo; \
	    fi; \
	    if [ -s $(BUILD)/doctor.unchecked ]; then \
	        cat $(BUILD)/doctor.unchecked; \
	        echo; \
	    fi; \
	    echo "  Then run this again. Nothing was downloaded or built."; \
	    echo; \
	    exit 1; \
	fi
	@echo "  prerequisites: everything the build and the boot need is here"

$(BUILD):
	mkdir -p $(BUILD)

# How to establish the kernel digest, printed rather than performed.
#
# Deliberately not automated. A target that fetched the tarball and recorded
# its own hash would look like verification and be none: it would prove the
# file did not change between two reads of it, which nobody was ever worried
# about. What establishes anything is the signature on kernel.org's list, and
# checking a signature means a human deciding whose key to trust.
pin-kernel:
	@echo
	@echo "  Pinning the kernel tarball for $(KVERSION)."
	@echo
	@echo "  The digest has to come from kernel.org's SIGNED list, not from"
	@echo "  the tarball you are about to build. Four commands:"
	@echo
	@echo "    curl -O https://cdn.kernel.org/pub/linux/kernel/v6.x/sha256sums.asc"
	@echo "    gpg --locate-keys torvalds@kernel.org gregkh@kernel.org"
	@echo "    gpg --verify sha256sums.asc"
	@echo "    grep 'linux-$(KVERSION).tar.xz$$' sha256sums.asc"
	@echo
	@echo "  The third one must say 'Good signature'. If it does not, stop —"
	@echo "  an unsigned list of digests establishes nothing at all."
	@echo
	@echo "  Then set KSHA256 in image/Makefile to what the fourth prints."
	@echo "  Current value: $(KSHA256)"
	@echo

# ───────────────────────────────────────────────────────────────── the kernel

$(KTARBALL): | $(BUILD)
	@if [ "$(KSHA256)" = "UNPINNED" ]; then \
	    echo; \
	    echo "  the kernel tarball has no pinned digest."; \
	    echo; \
	    echo "  Thalyx compiles its own kernel, so this download becomes the"; \
	    echo "  most privileged half of the machine. Building it unverified"; \
	    echo "  would mean the image boots a kernel nobody checked, and says"; \
	    echo "  nothing about it — which is the failure with no symptom."; \
	    echo; \
	    echo "  Run 'make -C image pin-kernel' for the four commands that"; \
	    echo "  establish the digest, then set KSHA256 in this Makefile."; \
	    echo; \
	    exit 1; \
	fi
	@# Downloaded to a temporary name and only given the real one once the
	@# digest matches. A partial download that kept the final name would be
	@# treated as done by every later run, and `make` would never fetch it
	@# again.
	curl -fL -o $@.part $(KURL)
	@echo "$(KSHA256)  $@.part" | sha256sum -c - > /dev/null || { \
	    echo; \
	    echo "  the kernel tarball does not match the digest this image pins."; \
	    echo "  expected: $(KSHA256)"; \
	    echo "  got:      $$(sha256sum $@.part | cut -d' ' -f1)"; \
	    echo; \
	    echo "  Thalyx compiles its own kernel, so this file becomes the"; \
	    echo "  privileged half of the machine. A mismatch is refused rather"; \
	    echo "  than built: TLS says who served the bytes, not what they were."; \
	    rm -f $@.part; \
	    exit 1; \
	}
	mv $@.part $@
	@echo "  kernel tarball: sha256 matches the pinned digest"

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

# Does this kernel have the hooks the programs attach to?
#
# `config-check` above compares what thalyx.config asked for against what came
# out. It cannot see something nobody asked for, and on 2026-08-04 that was
# CONFIG_SECURITY_NETWORK: the kernel built clean, booted clean, mounted
# everything, and then said
#
#     no  thalyx-lsm  this kernel does not expose `bpf_lsm_socket_connect`
#
# `bpf_lsm_<hook>` symbols are generated from include/linux/lsm_hook_defs.h, and
# every socket hook there sits inside `#ifdef CONFIG_SECURITY_NETWORK`. Nothing
# was rejected; the symbol was simply never compiled, and the first thing that
# noticed was a machine with no shell.
#
# So the question is asked here, of the kernel that was just built, and the
# symbols come from **the object that will attach to them** rather than from a
# list kept alongside. A list beside the code disagrees with the code
# eventually, and the disagreement here is a kernel built without the hook the
# programs actually use.
#
# System.map and not `nm vmlinux`: it is what the build already writes, and it
# does not need binutils to have been built against this kernel's format.
#
# Needs the binary, so it is not part of `kernel` — it is a separate target,
# and `all` runs it once both exist.
hook-check:
	@test -r $(KSOURCE)/System.map || { \
	    echo "  no System.map at $(KSOURCE) — the kernel was never built."; \
	    echo "  That is a different failure from a hook being absent, and"; \
	    echo "  saying so is the point."; \
	    exit 1; \
	}
	@test -x $(THALYX) || { \
	    echo "  $(THALYX) is not built, so nothing can say which hooks the"; \
	    echo "  enforcement object attaches to:"; \
	    echo; \
	    echo "      make -C image binary"; \
	    exit 1; \
	}
	@wanted=$$($(THALYX) enforce hooks) || { \
	    echo "  the binary could not say what it attaches to:"; \
	    $(THALYX) enforce hooks; \
	    exit 1; \
	}; \
	missing=$$(for hook in $$wanted; do \
	        grep -qw "$$hook" $(KSOURCE)/System.map || echo "    $$hook"; \
	    done); \
	if [ -n "$$missing" ]; then \
	    echo; \
	    echo "  This kernel does not expose the hooks enforcement needs:"; \
	    echo "$$missing"; \
	    echo; \
	    echo "  The symbol was not rejected — it was never compiled, because"; \
	    echo "  the option that compiles that hook is off. Socket hooks need"; \
	    echo "  CONFIG_SECURITY_NETWORK; file hooks need CONFIG_SECURITY."; \
	    echo "  Add the line to thalyx.config and rebuild the kernel."; \
	    echo; \
	    echo "  Booting anyway gives a machine that mounts everything and"; \
	    echo "  enforces nothing, and says so only after it has booted."; \
	    echo; \
	    exit 1; \
	fi; \
	echo "  hooks: this kernel exposes every symbol enforcement attaches to"
	@# A hook existing and a program being able to attach to it are two
	@# claims, and the second one cost its own boot — 2026-08-04, the day
	@# after the first. With the symbols present, attaching returned EBUSY.
	@#
	@# BPF attaches to an LSM hook with a trampoline. `register_fentry` calls
	@# `register_ftrace_direct` when the function is ftrace-managed, and falls
	@# back to poking the text when it is not — where it expects the five-byte
	@# NOP that CONFIG_FUNCTION_TRACER puts at the top of every function, and
	@# returns EBUSY when the bytes are anything else. Which reads as somebody
	@# else holding the hook. Nobody was.
	@#
	@# So the artifact is asked, not the request: `register_ftrace_direct` is
	@# compiled only under CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS, so its
	@# presence in the map *is* the property. thalyx.config asks for that
	@# option too and config-check verifies it, but only for as long as
	@# somebody keeps the line — and a check defined by a list cannot see what
	@# is missing from the list. That is the rule this file learned twice.
	@grep -qw register_ftrace_direct $(KSOURCE)/System.map || { \
	    echo; \
	    echo "  This kernel exposes the hooks and cannot have anything"; \
	    echo "  attached to them: it has no BPF trampoline support."; \
	    echo; \
	    echo "  register_ftrace_direct is not in the kernel, which means"; \
	    echo "  CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS is off. It is three"; \
	    echo "  dependencies below CONFIG_FUNCTION_TRACER; that is the line"; \
	    echo "  thalyx.config needs."; \
	    echo; \
	    echo "  Booting anyway gives 'Resource busy' when it attaches, which"; \
	    echo "  reads as something else holding the hook. Nothing is."; \
	    echo; \
	    exit 1; \
	}
	@echo "  hooks: and it can have a trampoline attached to them"

# ───────────────────────────────────────────────────────────────── the program

toolchain:
	rustup target add $(TARGET)

# The kernel side, built before the binary because it goes *inside* it.
#
# `thalyx` carries thalyx_lsm.bpf.o as bytes and loads it with its own loader —
# no bpftool, and no second file in an image that holds one program. That is
# what `crates/thalyx-cli/build.rs` picks up, and it picks up nothing if this
# has not run.
#
# Building it needs clang, bpftool and a kernel that publishes BTF, on the
# machine doing the building. Those are not needed to *run* Thalyx and they are
# needed to make an image that can enforce anything. An image without
# enforcement is a real thing to want while debugging, and it is
# `make image WITHOUT_LSM=1` — said out loud rather than arrived at by a tool
# quietly being absent.
LSM_OBJECT := $(ROOT)/lsm/thalyx_lsm.bpf.o

$(LSM_OBJECT):
	$(MAKE) -C $(ROOT)/lsm thalyx_lsm.bpf.o

lsm: $(LSM_OBJECT)
	@echo "  kernel side: $$(du -h $(LSM_OBJECT) | cut -f1), to be embedded in the binary"

# Statically linked, so the image needs no dynamic loader and no libc on disk.
#
# musl is compiled in, not shipped. Nothing in the image is a program somebody
# could invoke, because there is nobody to invoke it.
$(THALYX): toolchain $(if $(WITHOUT_LSM),,$(LSM_OBJECT))
	@test -n "$(WITHOUT_LSM)" || test -f $(LSM_OBJECT) || { \
	    echo "  $(LSM_OBJECT) was not built, so the binary would carry no"; \
	    echo "  enforcement and the machine would say so at every boot."; \
	    echo "  To build an image without it on purpose:"; \
	    echo "      make -C image image WITHOUT_LSM=1"; \
	    exit 1; \
	}
	cd $(ROOT) && cargo build --release --target $(TARGET) -p thalyx-cli

# What "static" actually has to mean here: the program asks for no interpreter.
# That is a program header, and it is what the kernel reads when it execs the
# thing. So it is what gets checked.
#
# It used to be `file … | grep 'statically linked'`, and that refused a binary
# that was perfectly static. Rust links musl as static-pie, and file(1) calls
# that `static-pie linked` — a different phrase, describing the same absence of
# a loader. The check was written against a sentence somebody assumed file(1)
# prints rather than one it had been seen printing, which is the rule in
# Estrategia-de-Pruebas about invented fixtures, in a Makefile.
#
# The evidence that it was the instrument and not the binary: the image had
# already booted with that same binary as /init. A dynamically linked one would
# have given `No working init found` before Thalyx printed a word.
#
# file(1)'s answer is still printed, because it reads well and because a future
# reader deserves to see what it says next to what was decided from it.
STATIC_CHECK = \
	command -v readelf > /dev/null || { \
	    echo "  no readelf, so nothing here checked that $$WHAT needs no libc."; \
	    echo "  Install binutils. Refusing rather than passing unchecked:"; \
	    echo "  an image that needs a loader panics at boot with no clue why."; \
	    exit 1; \
	}; \
	if readelf -lW "$$BIN" | grep -q INTERP; then \
	    echo "  NOT STATIC — $$WHAT asks for a dynamic loader, and there is"; \
	    echo "  none on the machine:"; \
	    readelf -lW "$$BIN" | grep -A1 INTERP | sed 's/^/    /'; \
	    exit 1; \
	fi; \
	echo "  static: $$(du -h "$$BIN" | cut -f1)  $$(file -b "$$BIN" | cut -d, -f4-)"

binary: $(THALYX)
	@BIN="$(THALYX)"; WHAT="the image"; $(STATIC_CHECK)

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
	@BIN="$(GREETER)"; WHAT="the module"; $(STATIC_CHECK)

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
	@# Into the repository, and deliberately **not** installed.
	@#
	@# vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md step 2 is a person
	@# installing a signed module from a local repository. A machine that boots
	@# with it already installed makes that step unperformable — there would be
	@# nothing left to install, and the trusted path of step 3 would never be
	@# reached. So the disk carries the bundle, and the human installs it.
	mkdir -p $(STAGE)/system/repo
	cp $(BUILD)/greeter.thmod $(STAGE)/system/repo/
	@# Two checks, and they are different claims.
	@#
	@# That the bundle holds up: a signature that does not verify is refused
	@# inside the machine, where there is no shell to look at the file with, and
	@# the person following the README would have no way to tell whose fault it
	@# was. `dev inspect` prints INVALID rather than failing, so the wording is
	@# what has to be checked.
	@$(THALYX) dev inspect $(STAGE)/system/repo/greeter.thmod \
	    | grep -q 'signature    valid' \
	    || { echo "the bundle in the repository does not verify"; \
	         $(THALYX) dev inspect $(STAGE)/system/repo/greeter.thmod; exit 1; }
	@# And that nothing is installed. If a later change puts the module back on
	@# the disk, step 2 of the exit criterion silently stops being performable —
	@# the machine would boot with it already there and nothing to install.
	@if THALYX_ROOT=$(STAGE)/system $(THALYX) module list | grep -q dev.thalyx.greeter; then \
	    echo "the module is installed on the stage, and step 2 needs it not to be"; \
	    exit 1; \
	fi
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
	@# Two ownerships, and they are not the same question.
	@#
	@# Inside the filesystem: root, done above, because those are the ids the
	@# machine sees and Thalyx is uid 0 there. The file holding the filesystem
	@# is the host's business, and it belongs to whoever asked for it — `make
	@# run` needs to open it read-write, and a `run` that needed sudo would put
	@# QEMU, the whole guest, and every future `make` under root for no reason.
	@#
	@# Conflating the two gives you a store that either QEMU cannot open or the
	@# machine cannot own. Both have now happened.
	@if [ -n "$(SUDO_UID)" ]; then \
	    chown $(SUDO_UID):$(SUDO_GID) $(STORE); \
	    echo "  the disk file belongs to uid $(SUDO_UID); what is inside it is root's"; \
	fi
	@echo
	@echo "  store: $(STORE) — three subvolumes, one module in the repository"
	@echo "  now:   make -C image run"

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
# The disk is checked for *writability*, not existence and not readability.
# QEMU opens a drive read-write, because a store the machine cannot write to is
# not a store. Checking `-r` let a root-owned 0644 disk through and turned the
# refusal into QEMU's own "Permission denied", which names the file and not the
# reason — the same shape of mistake as grepping file(1) for a phrase: a proxy
# for the property instead of the property.

run: $(BZIMAGE) $(INITRAMFS)
	@$(MAKE) --no-print-directory boot

# The same boot, building nothing at all.
#
# `run` depends on the kernel and the image, and the binary's own rule depends
# on `toolchain`, which is .PHONY — so make always considers `thalyx` out of
# date and asking for `run` can start a cargo build. Under sudo, which is how
# `dev/verify.sh` boots the machine, that build runs as root: every
# dependency's build script executes with privilege and leaves root-owned files
# in target/ that the next ordinary `cargo build` cannot replace.
#
# That is the same failure `store` was split in two to avoid, and the same
# rule: the privilege boundary is the target boundary. This target boots what
# is already there and refuses to make anything.
#
# It is also what keeps one QEMU command line in the repository. A second copy
# in verify.sh would drift, and the drift would be the automated boot testing
# something other than the boot a person gets.
boot:
	@test -f $(BZIMAGE) && test -f $(INITRAMFS) || { \
	    echo "  nothing to boot: the kernel or the image is not built."; \
	    echo; \
	    echo "      make -C image"; \
	    echo; \
	    echo "  This target builds neither, on purpose — it is the one"; \
	    echo "  dev/verify.sh runs, and that runs as root."; \
	    exit 1; \
	}
	@if [ -z "$(STORELESS)" ] && [ ! -f $(STORE) ]; then \
	    echo "  no store disk at $(STORE), so nothing would survive this boot."; \
	    echo; \
	    echo "      make -C image store-stage"; \
	    echo "      sudo make -C image store"; \
	    echo; \
	    echo "  To boot without one on purpose:  make -C image run STORELESS=1"; \
	    exit 1; \
	fi
	@if [ -f $(STORE) ] && [ ! -w $(STORE) ]; then \
	    echo "  $(STORE) is there and this user cannot write to it."; \
	    echo "  QEMU opens the disk read-write: a store the machine cannot"; \
	    echo "  write to is not a store."; \
	    echo; \
	    echo "      sudo chown $$(id -un):$$(id -gn) $(STORE)"; \
	    echo; \
	    echo "  A sudo make store from before 2026-08-03 left it root's."; \
	    exit 1; \
	fi
	qemu-system-x86_64 \
	    -m $(MEMORY) \
	    -smp 2 \
	    -kernel $(BZIMAGE) \
	    -initrd $(INITRAMFS) \
	    $$(test -z "$(STORELESS)" && test -w $(STORE) \
	        && echo "-drive file=$(STORE),if=virtio,format=raw") \
	    -nographic \
	    -append "console=ttyS0 lsm=capability,bpf panic=-1 thalyx.store=$(STOREDEV)"

clean:
	rm -rf $(BUILD)
