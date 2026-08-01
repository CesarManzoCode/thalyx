# Development VM for Thalyx.
#
# The BPF LSM enforces policy in the kernel, so it cannot be developed against
# the host: a policy that denies the wrong thing locks the machine until reboot,
# and the whole system Thalyx builds on (Btrfs subvolumes, /opt/thalyx, module
# installs) needs a machine it is allowed to own.
#
# Everything here is reproducible from nothing: `make vm` downloads a cloud
# image, configures it, and boots it with BPF LSM enabled. No manual steps.
#
#   make preflight   check this machine can run any of it
#   make vm          create the guest (first run downloads ~700 MB)
#   make run         boot it
#   make ssh         shell into it
#   make sync        copy the Thalyx source into the guest
#   make check       verify BPF LSM is actually active in the guest
#   make clean       delete the guest, keep the downloaded image

SHELL := /bin/sh

VM_DIR      ?= $(CURDIR)/.vm
IMAGE_URL   ?= https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img
BASE_IMAGE  := $(VM_DIR)/base.img
DISK        := $(VM_DIR)/thalyx-dev.qcow2
SEED        := $(VM_DIR)/seed.iso
SSH_KEY     := $(VM_DIR)/id_ed25519

DISK_SIZE   ?= 20G
MEMORY      ?= 6144
CPUS        ?= 4
SSH_PORT    ?= 2222

SSH_OPTS := -i $(SSH_KEY) -p $(SSH_PORT) \
            -o StrictHostKeyChecking=no \
            -o UserKnownHostsFile=/dev/null \
            -o LogLevel=ERROR
SSH := ssh $(SSH_OPTS) thalyx@localhost

.PHONY: preflight vm run ssh sync check clean distclean help

help:
	@sed -n '1,/^SHELL/p' $(MAKEFILE_LIST) | sed 's/^# \{0,1\}//' | head -n -2

preflight:
	@sh $(CURDIR)/preflight.sh

# ------------------------------------------------------------------ guest setup

$(VM_DIR):
	mkdir -p $(VM_DIR)

$(SSH_KEY): | $(VM_DIR)
	ssh-keygen -t ed25519 -N '' -C thalyx-dev -f $(SSH_KEY)

$(BASE_IMAGE): | $(VM_DIR)
	@echo "==> downloading base image (once)"
	curl -L --progress-bar -o $(BASE_IMAGE).part $(IMAGE_URL)
	mv $(BASE_IMAGE).part $(BASE_IMAGE)

$(SEED): cloud-init.yaml $(SSH_KEY) | $(VM_DIR)
	@echo "==> building cloud-init seed"
	@sed 's|@SSH_PUBKEY@|$(shell cat $(SSH_KEY).pub)|' cloud-init.yaml > $(VM_DIR)/user-data
	@printf 'instance-id: thalyx-dev\nlocal-hostname: thalyx-dev\n' > $(VM_DIR)/meta-data
	@if command -v cloud-localds >/dev/null 2>&1; then \
	    cloud-localds $(SEED) $(VM_DIR)/user-data $(VM_DIR)/meta-data; \
	elif command -v xorriso >/dev/null 2>&1; then \
	    xorriso -as mkisofs -output $(SEED) -volid cidata -joliet -rock \
	        $(VM_DIR)/user-data $(VM_DIR)/meta-data >/dev/null 2>&1; \
	else \
	    genisoimage -output $(SEED) -volid cidata -joliet -rock \
	        $(VM_DIR)/user-data $(VM_DIR)/meta-data >/dev/null 2>&1; \
	fi

$(DISK): $(BASE_IMAGE) $(SEED)
	@echo "==> creating guest disk ($(DISK_SIZE), sparse)"
	qemu-img create -f qcow2 -F qcow2 -b $(BASE_IMAGE) $(DISK) $(DISK_SIZE)

vm: $(DISK)
	@echo
	@echo "Guest created. First boot installs the toolchain and takes a few minutes."
	@echo "  make run     boot it (Ctrl-A X to quit)"
	@echo "  make ssh     shell in, once it is up"

# ------------------------------------------------------------------ running it

# BPF LSM has to be in the lsm= list explicitly: distributions ship
# CONFIG_BPF_LSM=y but leave bpf out of the default order, so the hooks exist
# and nothing can attach to them. cloud-init sets this on the guest's own
# command line; the flags here are only what QEMU needs.
run: $(DISK)
	qemu-system-x86_64 \
	    -enable-kvm \
	    -cpu host \
	    -smp $(CPUS) \
	    -m $(MEMORY) \
	    -drive file=$(DISK),if=virtio,format=qcow2 \
	    -drive file=$(SEED),if=virtio,format=raw,readonly=on \
	    -netdev user,id=net0,hostfwd=tcp::$(SSH_PORT)-:22 \
	    -device virtio-net-pci,netdev=net0 \
	    -nographic

ssh:
	@$(SSH)

sync:
	@echo "==> copying source into the guest"
	@tar -C $(CURDIR)/.. \
	    --exclude=target --exclude=.git --exclude=dev/.vm \
	    -czf - . | $(SSH) 'rm -rf ~/thalyx && mkdir -p ~/thalyx && tar -C ~/thalyx -xzf -'
	@echo "==> done: ~/thalyx in the guest"

# The check that matters: not "is the kernel new enough" but "can a program
# actually attach to an LSM hook right now".
check:
	@$(SSH) 'bash -s' < verify-guest.sh

clean:
	rm -f $(DISK) $(SEED) $(VM_DIR)/user-data $(VM_DIR)/meta-data

distclean:
	rm -rf $(VM_DIR)
