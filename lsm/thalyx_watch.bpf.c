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

/* A count of every mutation this program has seen, and nothing else.
 *
 * The ring buffer above carries the detail, and reading it needs a consumer
 * that mmaps the map and follows the ring protocol. This counter needs
 * `bpftool map dump`, which is already how everything else here is inspected.
 *
 * That difference decides what the index can do today. With the count alone
 * the index cannot know *what* changed — but it can know that **nothing** did,
 * and that is the answer to the expensive question. Checking freshness walks
 * the whole tree; a counter that has not moved since the last build means the
 * walk can be skipped entirely.
 *
 * Never reset while loaded. A count that went backwards would look like
 * "fewer things changed", so userspace treats any decrease as a reload and
 * therefore as a gap in coverage.
 */
struct {
    __uint(type, BPF_MAP_TYPE_ARRAY);
    __uint(max_entries, 1);
    __type(key, __u32);
    __type(value, __u64);
} thalyx_mutation_count SEC(".maps");

static __always_inline void count_mutation(void)
{
    __u32 slot = 0;
    __u64 *total = bpf_map_lookup_elem(&thalyx_mutation_count, &slot);
    if (total)
        __sync_fetch_and_add(total, 1);
}

static __always_inline void report(__u32 kind)
{
    /* Counted first, and unconditionally. If the ring is full the detail is
     * lost, but the fact that something changed must not be: an index that
     * believed nothing had happened would be confidently wrong, which is the
     * one outcome this whole design refuses. */
    count_mutation();

    struct mutation *event = bpf_ringbuf_reserve(&thalyx_mutations, sizeof(*event), 0);
    if (!event)
        return;   /* ring full: the count still moved, so the worker falls back
                     to the full sweep rather than pretending it saw everything */

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
