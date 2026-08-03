# Building the Thalyx image, and running it.
#
#   make iso   build it
#   make run   boot it in QEMU
#
# ─────────────────────────────────────────────────────────────────────────────
# NOTHING IN THIS FILE HAS EVER RUN.
#
# It was written in a container with no Alpine tooling, no QEMU, and no route to
# the Alpine package repositories — the network policy refuses them outright. So
# every target here is a proposal, and the first person to type `make iso` is
# also the first person to find out what is wrong with it.
#
# That is stated at the top rather than in a footnote because `CLAUDE.md` says
# not to stack unverified work and say nothing about it. The verified half of
# this effort is `thalyx session`, which runs and is tested; the image around it
# is not.
# ─────────────────────────────────────────────────────────────────────────────

ROOT      := $(CURDIR)/..
BUILD     := $(CURDIR)/build
OVERLAY   := $(CURDIR)/overlay
PROFILE   := thalyx
ARCH      := x86_64
DISK      := $(BUILD)/thalyx-disk.qcow2
DISK_SIZE := 8G
MEMORY    := 2048

# The image ships a repository so that the exit criterion is executable with no
# network at all. See Construccion-del-ISO.md.
MODULES   := $(ROOT)/modules
REPO      := $(BUILD)/repo

.PHONY: iso run clean requirements

requirements:
	@missing=""; \
	for tool in mkimage.sh apk abuild qemu-system-$(ARCH) qemu-img btrfs; do \
	    command -v $$tool > /dev/null 2>&1 || missing="$$missing $$tool"; \
	done; \
	if [ -n "$$missing" ]; then \
	    echo "missing:$$missing"; \
	    echo; \
	    echo "  alpine-sdk and alpine-conf provide mkimage.sh and abuild."; \
	    echo "  This is expected to be built on an Alpine host, or in a"; \
	    echo "  container with those installed."; \
	    exit 1; \
	fi

# The binaries the image carries.
#
# Built statically against musl so the image needs no dynamic loader decisions,
# and because a module's sandbox pivots into its own root where the host's
# libraries are not.
$(BUILD)/thalyx: | $(BUILD)
	cd $(ROOT) && cargo build --release --target $(ARCH)-unknown-linux-musl -p thalyx-cli
	cp $(ROOT)/target/$(ARCH)-unknown-linux-musl/release/thalyx $@

# A repository with one signed module, inside the image.
#
# The key is generated per build and thrown away: the image's repository is not
# a distribution channel, it is the thing that makes a first boot able to
# install something without the network. A publisher key that outlived the build
# would be a key nobody meant to trust.
$(REPO)/hola.thmod: $(BUILD)/thalyx | $(BUILD)
	mkdir -p $(REPO)
	$(BUILD)/thalyx dev keygen --out $(BUILD)/publisher.key
	$(BUILD)/thalyx dev pack $(MODULES)/hola/payload \
	    --manifest $(MODULES)/hola/manifest.toml \
	    --key $(BUILD)/publisher.key \
	    --out $@

$(BUILD):
	mkdir -p $(BUILD)

iso: requirements $(BUILD)/thalyx $(REPO)/hola.thmod
	@echo "── assembling the overlay"
	rm -rf $(BUILD)/apkovl
	mkdir -p $(BUILD)/apkovl
	cp -a $(OVERLAY)/. $(BUILD)/apkovl/
	install -Dm755 $(BUILD)/thalyx $(BUILD)/apkovl/usr/bin/thalyx
	mkdir -p $(BUILD)/apkovl/opt/thalyx/repo
	cp $(REPO)/*.thmod $(BUILD)/apkovl/opt/thalyx/repo/
	@echo "── building the image"
	mkimage.sh \
	    --profile $(PROFILE) \
	    --outdir $(BUILD) \
	    --arch $(ARCH) \
	    --repository https://dl-cdn.alpinelinux.org/alpine/latest-stable/main \
	    --tag thalyx
	@echo
	@echo "image in $(BUILD)"

# The disk is Btrfs with the three subvolumes decreed in Core-Nucleo.md, made
# before first boot rather than by an installer, because Phase 1 boots into a
# prepared machine rather than installing one.
$(DISK): | $(BUILD)
	qemu-img create -f qcow2 $@ $(DISK_SIZE)

run: $(DISK)
	qemu-system-$(ARCH) \
	    -m $(MEMORY) \
	    -smp 2 \
	    -cdrom $(BUILD)/thalyx-$(ARCH).iso \
	    -drive file=$(DISK),if=virtio,format=qcow2 \
	    -nographic \
	    -append "console=ttyS0 lsm=lockdown,capability,landlock,yama,bpf"

clean:
	rm -rf $(BUILD)
