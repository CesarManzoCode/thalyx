// A stand-in for a kernel's BTF, produced by clang so it is a real BTF section
// and not one written by hand. `struct file` here puts f_flags at byte 24 and
// `struct sockaddr` keeps sa_family at 0 — which is exactly the pair the
// relocation test needs: one field that moved and one that did not.
typedef unsigned short __u16;
typedef unsigned int __u32;
typedef unsigned long long __u64;
struct file { __u64 f_lock; __u64 f_count; __u32 f_mode; __u32 f_flags; };
struct sockaddr { __u16 sa_family; char sa_data[14]; };
struct file *a_file;
struct sockaddr *an_address;
