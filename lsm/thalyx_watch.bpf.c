// SPDX-License-Identifier: GPL-2.0
//
// thalyx-watch — filesystem mutation events for the semantic index.
//
// Separate from thalyx_lsm.bpf.c on purpose. These hooks deny nothing: they
// report what changed so the graph can reprocess it. Enforcement must not
// depend on them, and they must not depend on enforcement.
//
// Splitting them also means a hook this kernel does not expose stops the
// watcher without stopping the enforcement. That matters because which
// filesystem hooks exist varies by kernel configuration — `make hooks` shows
// what this machine actually offers.
//
// They **never block**. Re-parsing inside a hook would make every write in the
// system wait for a parser, and a stalled parser would stall the filesystem.
// The event is queued and a worker picks it up, which is why the index is
// eventually consistent with a known lag rather than instantly exact.
//
// See vault/04-Flujo-Canonico/Coherencia-Doble-Ruta.md.

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wmissing-declarations"
#include "vmlinux.h"
#pragma clang diagnostic pop

#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

char LICENSE[] SEC("license") = "GPL";

#define THALYX_CREATED 0
#define THALYX_REMOVED 1
#define THALYX_RENAMED 2

struct mutation {
    __u64 cgroup_id;
    __u32 pid;
    __u32 kind;
    char  comm[16];
};

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 1024 * 1024);
} thalyx_mutations SEC(".maps");

static __always_inline void report(__u32 kind)
{
    struct mutation *event = bpf_ringbuf_reserve(&thalyx_mutations, sizeof(*event), 0);
    if (!event)
        return;   /* ring full: the worker marks the index stale rather than
                     pretending it saw everything */

    event->cgroup_id = bpf_get_current_cgroup_id();
    event->pid = bpf_get_current_pid_tgid() >> 32;
    event->kind = kind;
    bpf_get_current_comm(&event->comm, sizeof(event->comm));
    bpf_ringbuf_submit(event, 0);
}

/* inode_* rather than path_*: the path family only exists when
   CONFIG_SECURITY_PATH is built in, which depends on which LSMs the
   distribution ships. The inode hooks are part of the core LSM set and are
   always present. */

SEC("lsm/inode_unlink")
int BPF_PROG(thalyx_inode_unlink, struct inode *dir, struct dentry *dentry, int ret)
{
    if (ret == 0)
        report(THALYX_REMOVED);
    return ret;
}

SEC("lsm/inode_create")
int BPF_PROG(thalyx_inode_create, struct inode *dir, struct dentry *dentry,
             umode_t mode, int ret)
{
    if (ret == 0)
        report(THALYX_CREATED);
    return ret;
}

SEC("lsm/inode_rename")
int BPF_PROG(thalyx_inode_rename, struct inode *old_dir, struct dentry *old_dentry,
             struct inode *new_dir, struct dentry *new_dentry, int ret)
{
    if (ret == 0)
        report(THALYX_RENAMED);
    return ret;
}
