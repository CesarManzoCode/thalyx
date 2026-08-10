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
#include <bpf/bpf_core_read.h>

char LICENSE[] SEC("license") = "GPL";

#define THALYX_CREATED 0
#define THALYX_REMOVED 1
#define THALYX_RENAMED 2
#define THALYX_WRITTEN 3
#define THALYX_RETITLED 4   /* metadata: truncate, chmod, chown, utimes */

/* From include/linux/fs.h. Duplicated rather than derived because vmlinux.h
 * carries type layouts, not macros. */
#define MAY_WRITE 0x00000002

struct mutation {
    __u64 cgroup_id;
    __u32 pid;
    __u32 kind;
    char  comm[16];
};

/* `thalyx_mut_ring` and not `thalyx_mutations`, and the difference is fifteen
 * characters.
 *
 * BPF_OBJ_NAME_LEN is 16 including the terminator, so the kernel keeps the
 * first fifteen characters of a map's name and nothing more. `thalyx_mutations`
 * and `thalyx_mutation_count` both truncate to `thalyx_mutation`, so the kernel
 * held two maps of this object under one name — and anything that identified a
 * map by asking the kernel got whichever came first.
 *
 * That is the state Cesar's machine of 2026-08-10 was in: the counter pinned,
 * the ring not, and a report that could only say "nothing is pinned there".
 * Whether the collision is what stopped the pin was never established; what is
 * established is that two maps sharing one kernel name makes the question
 * unanswerable, and a name nobody can ask about is worse than a long one.
 */
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 1024 * 1024);
} thalyx_mut_ring SEC(".maps");

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
 * ## Why it is per-CPU
 *
 * It used to be a plain array, which is one `__sync_fetch_and_add` on one
 * cacheline contended by every core in the machine. That was affordable while
 * the hooks were three rare operations. It is not affordable now that the set
 * includes `file_permission`, which fires on every read and write on the
 * machine — the counter would have become a global lock in the write path.
 *
 * Per-CPU makes each increment local and uncontended. Userspace sums the
 * slots, and the sum is still monotonic: no single read is atomic across
 * CPUs, but any read lands between the true value when it started and the
 * true value when it finished, so a later read is never smaller than an
 * earlier one. That is the only property the freshness logic depends on.
 *
 * Never reset while loaded. A count that went backwards would look like
 * "fewer things changed", so userspace treats any decrease as a reload and
 * therefore as a gap in coverage.
 */
struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 1);
    __type(key, __u32);
    __type(value, __u64);
} thalyx_mutation_count SEC(".maps");

static __always_inline void count_mutation(void)
{
    __u32 slot = 0;
    __u64 *total = bpf_map_lookup_elem(&thalyx_mutation_count, &slot);
    if (total)
        /* No atomic: the slot belongs to this CPU and BPF programs are not
         * preemptible by other BPF programs on the same CPU. */
        *total += 1;
}

/* ## Attributing a mutation to the tree it happened in
 *
 * The count above is machine-wide. That is the safe direction — it costs walks
 * that were not needed, never a missed change — but it means the index's
 * shortcut only ever fires on a machine nobody is using. Everything below
 * exists to answer the tighter question: did anything change *in this tree*.
 *
 * Userspace puts the (device, inode) of each indexed tree root in here. The
 * program only ever increments keys that are already present; it never
 * inserts, so the map cannot grow from the write path and a busy machine
 * cannot fill it.
 */
struct root_key {
    __u64 ino;
    __u32 dev;
    __u32 pad;   /* explicit, and always zero: a hash key compares by bytes,
                    and a padding hole full of stack garbage would make the
                    same root hash differently on every call */
};

struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 256);
    __type(key, struct root_key);
    __type(value, __u64);
} thalyx_watched SEC(".maps");

/* Mutations that could not be attributed to any tree, and so have to be
 * counted against all of them.
 *
 * Only one thing lands here: a directory nested deeper than the walk below is
 * willing to climb. Everything else resolves — a match, or a walk that reaches
 * the root of its own filesystem, which settles the question rather than
 * leaving it open (see below).
 *
 * Userspace adds this to every tree's count. Over-counting a tree costs a walk
 * that was not needed; leaving it out would let a change go unseen.
 */
struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 1);
    __type(key, __u32);
    __type(value, __u64);
} thalyx_unattributed SEC(".maps");

/* How far up the tree to look before giving up and counting it against
 * everything. Deep enough that real source trees resolve; bounded because the
 * verifier requires it and because a corrupted parent chain must not spin. */
#define MAX_DEPTH 64

static __always_inline void count_unattributed(void)
{
    __u32 slot = 0;
    __u64 *total = bpf_map_lookup_elem(&thalyx_unattributed, &slot);
    if (total)
        *total += 1;
}

/* Walk up from a dentry until a watched root is found.
 *
 * Three outcomes, and the middle one is what makes this worth doing:
 *
 * 1. **A watched root is an ancestor.** Count it against that tree.
 *
 * 2. **The walk reaches the root of its own filesystem without a match.**
 *    The file is *definitively outside* every watched tree on that filesystem:
 *    same superblock, every ancestor examined, no match. Nothing is counted.
 *
 *    This is the case that makes a busy machine quiet again. A browser cache,
 *    a log file, a pipe, a write to /tmp — each one climbs to its own root and
 *    stops, contributing nothing to any tree's count.
 *
 *    It rests on one assumption, and the assumption is checked rather than
 *    hoped for: a file reached through a *mount* inside a watched tree lives
 *    on a different superblock, and its walk would stop at that filesystem's
 *    root without ever seeing the watched dentry. So userspace refuses to
 *    scope a tree that has anything mounted underneath it — the check is one
 *    read of /proc/mounts at build time, and it turns an assumption into a
 *    precondition.
 *
 * 3. **The walk runs out of depth.** Genuinely unknown, so counted against
 *    everything.
 */
static __always_inline void attribute(struct dentry *dentry)
{
    struct dentry *cur = dentry;

    for (int i = 0; i < MAX_DEPTH; i++) {
        if (!cur)
            return;   /* nothing to attribute it to and nothing to conclude */

        struct inode *inode = BPF_CORE_READ(cur, d_inode);
        struct super_block *sb = BPF_CORE_READ(cur, d_sb);

        /* A negative dentry — the name being created does not exist yet — has
         * no inode. It cannot be a watched root either, so the walk simply
         * continues to its parent, which is the directory it is being created
         * in and is exactly what should be attributed. */
        if (inode && sb) {
            struct root_key key = {
                .ino = BPF_CORE_READ(inode, i_ino),
                .dev = BPF_CORE_READ(sb, s_dev),
                .pad = 0,
            };

            __u64 *count = bpf_map_lookup_elem(&thalyx_watched, &key);
            if (count) {
                __sync_fetch_and_add(count, 1);
                return;
            }
        }

        struct dentry *parent = BPF_CORE_READ(cur, d_parent);
        if (!parent || parent == cur)
            return;   /* the root of this filesystem: outcome 2, and settled */
        cur = parent;
    }

    count_unattributed();
}

/* Bump the counter, and queue the detail if there is room for it.
 *
 * `quiet` is for the hooks that fire constantly. Every write on the machine
 * passes through `file_permission`, and pushing a ring buffer record for each
 * one would drown every other event in the ring within milliseconds — the
 * detail for a rename would be lost to a log file being appended to. So the
 * hot hook contributes to the count, which is what freshness needs, and stays
 * out of the ring, which is what attribution needs.
 */
static __always_inline void report(__u32 kind, bool quiet, struct dentry *where)
{
    /* Counted first, and unconditionally. If the ring is full the detail is
     * lost, but the fact that something changed must not be: an index that
     * believed nothing had happened would be confidently wrong, which is the
     * one outcome this whole design refuses. */
    count_mutation();
    attribute(where);

    if (quiet)
        return;

    struct mutation *event = bpf_ringbuf_reserve(&thalyx_mut_ring, sizeof(*event), 0);
    if (!event)
        return;   /* ring full: the count still moved, so the worker falls back
                     to the full sweep rather than pretending it saw everything */

    event->cgroup_id = bpf_get_current_cgroup_id();
    event->pid = bpf_get_current_pid_tgid() >> 32;
    event->kind = kind;
    bpf_get_current_comm(&event->comm, sizeof(event->comm));
    bpf_ringbuf_submit(event, 0);
}

/* ## Why every hook counts before knowing whether the operation succeeds
 *
 * An LSM hook runs *before* the operation, so there is no outcome to wait for.
 * The `ret` argument is not "did it work" — it is what BPF programs already
 * attached to this same hook decided, and a non-zero value means one of them
 * denied it. Counting regardless over-counts a little: an operation that a
 * later check refuses, or that fails on ENOSPC, still moves the number.
 *
 * That is the safe direction. Over-counting costs a tree walk that was not
 * needed. Under-counting costs correctness, because the index would answer
 * "current" for a tree that had changed underneath it.
 *
 * `ret` is still returned untouched. A watcher that returned 0 would turn a
 * denial by another BPF LSM program on the same hook into an allow — a
 * program whose whole purpose is to deny nothing would start granting things.
 *
 * ## Why inode_* and not path_*
 *
 * The path family only exists when CONFIG_SECURITY_PATH is built in, which
 * depends on which LSMs the distribution ships. The inode hooks are part of
 * the core LSM set and are always present.
 */

/* --- things appearing, disappearing and moving --------------------------- */

SEC("lsm/inode_create")
int BPF_PROG(thalyx_create, struct inode *dir, struct dentry *dentry,
             umode_t mode, int ret)
{
    report(THALYX_CREATED, false, dentry);
    return ret;
}

SEC("lsm/inode_unlink")
int BPF_PROG(thalyx_unlink, struct inode *dir, struct dentry *dentry, int ret)
{
    report(THALYX_REMOVED, false, dentry);
    return ret;
}

SEC("lsm/inode_rename")
int BPF_PROG(thalyx_rename, struct inode *old_dir, struct dentry *old_dentry,
             struct inode *new_dir, struct dentry *new_dentry, int ret)
{
    /* The destination, not the source. A rename into a watched tree is a
     * change to that tree; one out of it changed the tree it left, and the
     * source dentry no longer describes where anything is. Counting the
     * destination misses the second case, so both are attributed. */
    report(THALYX_RENAMED, false, new_dentry);
    attribute(old_dentry);
    return ret;
}

/* `inode_create` is regular files only. A directory, a symlink, a hard link
 * and a device node each have their own hook, and every one of them was
 * missing from the first version of this program — so `mkdir` was invisible
 * to a watcher whose entire job is noticing that the tree changed shape. */

SEC("lsm/inode_mkdir")
int BPF_PROG(thalyx_mkdir, struct inode *dir, struct dentry *dentry,
             umode_t mode, int ret)
{
    report(THALYX_CREATED, false, dentry);
    return ret;
}

SEC("lsm/inode_rmdir")
int BPF_PROG(thalyx_rmdir, struct inode *dir, struct dentry *dentry, int ret)
{
    report(THALYX_REMOVED, false, dentry);
    return ret;
}

SEC("lsm/inode_symlink")
int BPF_PROG(thalyx_symlink, struct inode *dir, struct dentry *dentry,
             const char *old_name, int ret)
{
    report(THALYX_CREATED, false, dentry);
    return ret;
}

SEC("lsm/inode_link")
int BPF_PROG(thalyx_link, struct dentry *old_dentry, struct inode *dir,
             struct dentry *new_dentry, int ret)
{
    report(THALYX_CREATED, false, new_dentry);
    return ret;
}

SEC("lsm/inode_mknod")
int BPF_PROG(thalyx_mknod, struct inode *dir, struct dentry *dentry,
             umode_t mode, dev_t dev, int ret)
{
    report(THALYX_CREATED, false, dentry);
    return ret;
}

/* --- contents changing without the tree changing shape ------------------- */

/* The hole this program existed with until now.
 *
 * Editing a file in place creates nothing, removes nothing and renames
 * nothing. Every hook above stays silent while the contents the index parsed
 * are replaced. A counter with that hole can be read as "nothing changed"
 * about a tree that has been entirely rewritten, which is the exact failure
 * `Coherencia-Doble-Ruta` forbids — so the fast path could never be turned on,
 * and the counter could only ever be decoration.
 *
 * `file_permission` is called from `rw_verify_area`, on every read and every
 * write, with the access being asked for. Masking it to `MAY_WRITE` catches
 * every write through a descriptor **including one opened before this program
 * attached**, which `file_open` would have missed and which is precisely the
 * long-lived editor or database that makes a counter untrustworthy.
 *
 * It is the hot hook. That is why the counter is per-CPU and why this one does
 * not touch the ring buffer.
 */
SEC("lsm/file_permission")
int BPF_PROG(thalyx_write, struct file *file, int mask, int ret)
{
    if (mask & MAY_WRITE)
        report(THALYX_WRITTEN, true, BPF_CORE_READ(file, f_path.dentry));
    return ret;
}

/* Truncate, chmod, chown and utimes go through `notify_change`, not through
 * the write path, so `file_permission` never sees them. A truncate changes
 * what a file contains; a chmod changes whether the indexer can still read it.
 *
 * The signature of this hook has moved across kernel versions — it gained a
 * `user_namespace` argument and then an `mnt_idmap` one. This is the current
 * form. On a kernel too old for it the whole watcher declines to attach and
 * says so, which is the right outcome: a watcher missing a hook must not load
 * looking complete. Thalyx already requires idmapped mounts, so a kernel
 * without this signature cannot run the sandbox either.
 */
SEC("lsm/inode_setattr")
int BPF_PROG(thalyx_setattr, struct mnt_idmap *idmap, struct dentry *dentry,
             struct iattr *attr, int ret)
{
    report(THALYX_RETITLED, false, dentry);
    return ret;
}

/* ## What is still not covered, and is not pretended to be
 *
 * Extended attributes are deliberately absent. SELinux relabels files
 * constantly, and counting that would move the number all day for changes no
 * parser cares about — a counter that never stops moving allows no shortcut,
 * which is the same as no counter.
 *
 * A filesystem that another machine can write — NFS, SMB, a shared block
 * device — changes with no hook on this machine firing at all. No hook set can
 * close that, so `Trust::Counter` stays an explicit choice by the caller and
 * `Watcher::verify` stays the thing that has to agree first.
 */
