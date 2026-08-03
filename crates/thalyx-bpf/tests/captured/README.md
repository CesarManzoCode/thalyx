# Captured, not written

`thalyx_lsm.bpf.o` is clang's real output for `lsm/thalyx_lsm.bpf.c`, kept here
because `Estrategia-de-Pruebas.md` says a parser for another tool's format needs
one captured sample verbatim. A fixture written by hand proves the parser
matches its author's idea of ELF and BTF, which is exactly the thing that has
already gone wrong in this project more than once.

Produced on 2026-08-03 with:

    make -C lsm thalyx_lsm.bpf.o

and copied here verbatim. **It is the object that gets loaded**, byte for byte.

That was not always true. Until `lsm/vmlinux.h` was written by hand, this
fixture was compiled against a stub that lived beside it, because the
development container has no kernel BTF for `bpftool` to generate a real
`vmlinux.h` from — so the object the tests read and the object the machine
loaded were two different files built from two different headers, and nothing
compared them. Now there is one header, so there can be one object.

Regenerate it the same way whenever the C or the header changes. An object left
behind is a test that keeps passing about a program nobody runs.

## `kernelish.btf`

A stand-in for a running kernel's BTF, and also clang's real output — extracted
from `kernelish.c` with `llvm-objcopy --dump-section=.BTF=`. Its `struct file`
puts `f_flags` at byte **20**, behind three fields the object's own header does
not have; its `struct sockaddr` keeps `sa_family` at **0**.

That pair is the whole point. One field moved and one did not, so a relocation
pass that did nothing at all would pass on `sa_family` and fail on `f_flags` —
which is the baseline-and-control shape `Estrategia-de-Pruebas` asks for, made
out of two fields instead of two machines.
