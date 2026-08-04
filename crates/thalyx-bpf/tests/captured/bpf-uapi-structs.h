/* Captured verbatim from /usr/include/linux/bpf.h on 2026-08-03, Linux uapi.
 *
 * These are the two structures the kernel fills in for BPF_OBJ_GET_INFO_BY_FD,
 * and they are here for the same reason the enums beside them are: the offsets
 * were about to be written from memory, and the last constant written from
 * memory was wrong by one and cost a run on real hardware.
 *
 * `name` sits at byte 64 of `struct bpf_prog_info`. Nothing about that is
 * guessable, and getting it wrong does not fail: it reads sixteen bytes of
 * some other field and reports a program called whatever those bytes spell.
 * Thalyx would then say enforcement is not attached while it is, or the
 * reverse — which is the exact failure the vault keeps writing rules about.
 *
 * The test beside this walks the declarations and computes the offsets. The
 * two macro-defined lengths it needs, also verbatim from the same header:
 */

#define BPF_OBJ_NAME_LEN 16U
#define BPF_TAG_SIZE	8

struct bpf_prog_info {
	__u32 type;
	__u32 id;
	__u8  tag[BPF_TAG_SIZE];
	__u32 jited_prog_len;
	__u32 xlated_prog_len;
	__aligned_u64 jited_prog_insns;
	__aligned_u64 xlated_prog_insns;
	__u64 load_time;	/* ns since boottime */
	__u32 created_by_uid;
	__u32 nr_map_ids;
	__aligned_u64 map_ids;
	char name[BPF_OBJ_NAME_LEN];
	__u32 ifindex;
	__u32 gpl_compatible:1;
	__u32 :31; /* alignment pad */
	__u64 netns_dev;
	__u64 netns_ino;
	__u32 nr_jited_ksyms;
	__u32 nr_jited_func_lens;
	__aligned_u64 jited_ksyms;
	__aligned_u64 jited_func_lens;
	__u32 btf_id;
	__u32 func_info_rec_size;
	__aligned_u64 func_info;
	__u32 nr_func_info;
	__u32 nr_line_info;
	__aligned_u64 line_info;
	__aligned_u64 jited_line_info;
	__u32 nr_jited_line_info;
	__u32 line_info_rec_size;
	__u32 jited_line_info_rec_size;
	__u32 nr_prog_tags;
	__aligned_u64 prog_tags;
	__u64 run_time_ns;
	__u64 run_cnt;
	__u64 recursion_misses;
	__u32 verified_insns;
	__u32 attach_btf_obj_id;
	__u32 attach_btf_id;
} __attribute__((aligned(8)));
/* Only the prefix, and deliberately: everything past `prog_id` is a union of
 * one struct per link type, which is not a fixed layout and which Thalyx never
 * reads. The parser stops at the union, so trimming the rest is not a
 * shortening that breaks the counting — it is where the counting ends.
 */
struct bpf_link_info {
	__u32 type;
	__u32 id;
	__u32 prog_id;
};


/* And the arm of `union bpf_attr` that BPF_MAP_*_ELEM uses, captured verbatim
 * from include/uapi/linux/bpf.h at v6.12 on 2026-08-04.
 *
 * It is anonymous in the kernel's header, so the walker above cannot reach it
 * by name; the test beside it reads these lines directly. It is here for the
 * same reason as everything else in this file — `thalyx-syscall` declares a
 * `repr(C)` mirror of it, and `key` at the wrong offset hands the kernel a
 * pointer into the middle of a field instead of a cgroup id.
 *
 * The kernel would then write a permission against whatever those eight bytes
 * spell: a policy for a cgroup nobody asked about, and none for the module
 * that is about to run.
 */

	struct { /* anonymous struct used by BPF_MAP_*_ELEM commands */
		__u32		map_fd;
		__aligned_u64	key;
		union {
			__aligned_u64 value;
			__aligned_u64 next_key;
		};
		__u64		flags;
	};
