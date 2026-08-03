# Captured, not written

`thalyx_lsm.bpf.o` is clang's real output for `lsm/thalyx_lsm.bpf.c`, kept here
because `Estrategia-de-Pruebas.md` says a parser for another tool's format needs
one captured sample verbatim. A fixture written by hand proves the parser
matches its author's idea of ELF and BTF, which is exactly the thing that has
already gone wrong in this project more than once.

Produced on 2026-08-03 with:

    clang -O2 -g -Wall -target bpf -D__TARGET_ARCH_x86 -I. \
        -c lsm/thalyx_lsm.bpf.c -o thalyx_lsm.bpf.o

using clang 18.1.3 and `vmlinux-stub.h`, which is beside it. The stub exists
because the development container has no kernel BTF to generate a real
`vmlinux.h` from. **It does not make the object wrong**: CO-RE relocations are
resolved against the kernel's BTF at load time, so what the local header has to
get right is the field *names*, and `preserve_access_index` is what makes clang
emit the relocation rather than a fixed offset.

What this object is not: the one that gets loaded. That one is built by
`make -C lsm` against the real `vmlinux.h`, on a machine that has bpftool. This
one is for the parser to be checked against something clang actually wrote.

## `kernelish.btf`

A stand-in for a running kernel's BTF, and also clang's real output — extracted
from `kernelish.c` with `llvm-objcopy --dump-section=.BTF=`. Its `struct file`
puts `f_flags` at byte **20**, behind three fields the object's own header does
not have; its `struct sockaddr` keeps `sa_family` at **0**.

That pair is the whole point. One field moved and one did not, so a relocation
pass that did nothing at all would pass on `sa_family` and fail on `f_flags` —
which is the baseline-and-control shape `Estrategia-de-Pruebas` asks for, made
out of two fields instead of two machines.
