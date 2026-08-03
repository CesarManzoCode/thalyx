#ifndef __VMLINUX_H__
#define __VMLINUX_H__
typedef unsigned char __u8;
typedef short unsigned int __u16;
typedef unsigned int __u32;
typedef long long unsigned int __u64;
typedef long long int __s64;
typedef _Bool bool;
typedef __u16 __be16;
typedef __u32 __be32;
typedef __u32 __wsum;
typedef int __s32;
typedef long int __kernel_long_t;
struct __sk_buff;
struct bpf_sock;
struct bpf_sock_addr;
struct bpf_sock_ops;
struct bpf_perf_event_data;
struct bpf_map;
struct sk_msg_md;
struct xdp_md;
struct sk_reuseport_md;
struct bpf_dynptr;
struct bpf_timer;
struct task_struct;
struct path;
struct btf_ptr;
struct bpf_spin_lock;
struct linux_binprm;
struct pt_regs;
struct seq_file;
struct tcphdr;
struct bpf_sk_lookup;
struct bpf_tcp_sock;
struct tcp_sock;
struct sock;
struct sockaddr;
struct sk_buff;
struct bpf_pidns_info;
struct bpf_sysctl;
struct bpf_redir_neigh;
struct bpf_func_info;
struct mptcp_sock;
struct bpf_dynptr_kern;
struct cgroup;
struct iphdr;
struct ipv6hdr;
struct udp6_sock;
struct unix_sock;
struct nf_conn;
struct bpf_ct_opts;
struct bpf_iter_num;
enum bpf_map_type { BPF_MAP_TYPE_UNSPEC = 0, BPF_MAP_TYPE_HASH = 1, BPF_MAP_TYPE_ARRAY = 2, BPF_MAP_TYPE_RINGBUF = 27 };
#pragma clang attribute push (__attribute__((preserve_access_index)), apply_to = record)
struct sockaddr { __u16 sa_family; char sa_data[14]; };
struct socket { int state; short type; unsigned long flags; };
struct file { unsigned int f_flags; unsigned int f_mode; };
#pragma clang attribute pop
#endif
