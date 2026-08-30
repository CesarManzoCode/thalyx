#!/usr/bin/env python3
"""What a benchmark run cost, read out of what the agent actually printed.

`dev/bench-external-agent.sh` calls this once per run. It is a separate file so
that it can be *tested without running an agent*: `--self-test` parses a real
captured session, committed beside it, and checks the numbers that session is
known to have. `Estrategia-de-Pruebas.md` rule 6 — a parser for somebody else's
output needs one captured real sample, verbatim, because a hand-written fixture
proves the parser matches its author's model of the format and nothing else.
This project has broken that rule twice, and the second time it accused
llama.cpp of ignoring a grammar it had just obeyed.

## The one rule about the numbers

**Nothing here is estimated.** A field the agent did not print is absent from
the summary, never zero and never guessed. That is rule 10: a failure to read is
not a failure to exist. It matters most for tokens and cost, which only exist
because Claude Code prints them — an arm that reported an invented token count
would make the whole comparison worthless in the direction nobody would check.

## What is comparable between the two arms and what is not

Read out of the stream for **both** arms, so they mean the same thing:

    turns, wall time, tokens, cost      the agent's own result event
    tool calls, and which tools         every tool_use block
    bytes handed back to the model      every tool_result block
    files read, text searches           the tools that do those things, by name

Read for arm B only, because there is nothing on Linux it would mean:

    the machine's own metrics           thalyx-mcp --metrics

Arm B is therefore measured twice — once from the agent's stream, once from the
adapter — and the two are kept apart rather than merged. Rule 5, the instrument
includes the harness: if those two disagree, one of them is wrong, and a summary
that had averaged them would hide it.

## The trap in the `reversible` task

That task ends with "put the project back exactly as you found it", and the
harness checks it by hashing the tree before and after. Which means **an agent
that did nothing at all scores a perfect restore.** A verdict read off the hash
would rank a refusal above every honest attempt, and it would rank it highest in
arm B, which is the direction this whole comparison must never be wrong in.

So the verdict is a conjunction and each part is read from a different place:

    really_changed    the new name appeared in a tool call — the agent's stream
    restored          the bytes came back — this host, hashing the tree
    task_success      the answer named the files the ground truth demands

`reversible.passed` is true only when all three are, and it is **absent** when
any of them is unknown rather than guessed at. The commonest unknown is arm B's
`restored`: its workspace lives inside the VM, so hashing it means exporting the
store with the machine down, and until somebody does that the summary says
`restore_check: not_proven`. `--require-restore-check` turns that into a
non-zero exit — rule 3, one switch per requirement, so demanding this one never
means demanding anything else.
"""

import argparse
import copy
import hashlib
import json
import os
import pathlib
import shutil
import socket
import stat
import sys
import tempfile

# Which tools do the thing, per arm. Named rather than guessed at from the tool
# name, because `Bash` can run `grep` and there is no honest way to know from
# the stream whether it did — so `Bash` counts as a tool call and as nothing
# else, and the raw per-tool table below is what tells you the rest.
FILE_READERS = {"Read", "NotebookRead", "mcp__thalyx__thalyx_read"}
TEXT_SEARCHERS = {"Grep", "mcp__thalyx__thalyx_find"}
INDEX_QUESTIONS = {"mcp__thalyx__thalyx_symbol", "mcp__thalyx__thalyx_dependencies"}

# Tools that cannot do anything *but* change the workspace. `Bash` is absent on
# purpose and for the same reason it is absent from TEXT_SEARCHERS: `sed -i` and
# `ls` arrive as the same tool name and there is no honest way to tell them
# apart from the stream. So this counts what is certain, and the marker count
# below — which looks inside the input of every call, whatever the tool — is
# what catches a rename done with `sed`.
WORKSPACE_WRITERS = {
    "Edit",
    "MultiEdit",
    "Write",
    "NotebookEdit",
    "mcp__thalyx__thalyx_edit",
    "mcp__thalyx__thalyx_file",
}

# Tools that cannot do anything *but* read. Named one by one, and the list is
# short on purpose: membership here is a claim that no argument to this tool can
# change a byte, and a tool nobody has checked belongs in neither list.
#
# **The reason this list exists at all is the 2026-08-29 forensics.** A `Bash`
# call whose command was `git checkout -- <file>` was printed by the forensic
# table as `write=False`. Nothing in the code claimed that meant "proven not to
# have written" — but `False` is what a reader reads, and a restore performed
# with the shell is precisely the call that most needed to be visible.
PROVEN_READERS = (
    FILE_READERS
    | TEXT_SEARCHERS
    | INDEX_QUESTIONS
    | {
        "Glob",
        "LS",
        "mcp__thalyx__thalyx_state",
        "mcp__thalyx__thalyx_list",
        "mcp__thalyx__thalyx_index",
        "mcp__thalyx__thalyx_changed",
    }
)


# ── what a tree is, for the purpose of "put it back exactly" ─────────────────
#
# The `reversible` task ends with "byte for byte: no file left differing from
# its original content, no file added and no file removed", and until
# 2026-08-28 the harness checked that claim with `find -type f | xargs
# sha256sum`. Which is contents and nothing else. `-type f` does not match a
# symlink at all, so an agent that replaced `src/lib.rs` with a symlink to
# `/etc/passwd` deleted a file *and* added a link and the digest moved only
# because the contents went missing; an agent that left a source file mode 777,
# or turned a directory into a file, restored the tree perfectly as far as that
# check could see.
#
# So the digest is over a manifest and not over contents: for every entry below
# the root, its **type**, its **permission bits**, its **content** if it is a
# regular file and its **target** if it is a symlink. That is the whole list in
# `Agentes-Externos.md`, and deliberately not a filesystem diff: owner, mtime,
# xattrs and inode numbers are all absent, because none of them is something the
# task asks the agent to preserve and each one would make arm A fail a restore
# it performed correctly.
#
# ── what is not the workspace, and why that list is not a taste ──────────────
#
# The benchmark measures a *project*. It does not measure the machinery that
# carries the project to the two arms, and on 2026-08-29 it did: arm B's only
# reported difference between the tree it started from and the tree that came
# back was
#
#     -s140000  0755  -  -  image/build/agent.sock
#
# which is the Unix socket **QEMU** opens so `thalyx-mcp` can talk to the
# machine. It exists on this host because the benchmark is running; it is not in
# the copy on the store because it did not exist when `project-stage` tarred the
# project in. No agent could have created it, no agent could have removed it,
# and a restore verdict that turns on it is a verdict about the harness.
#
# So the list below is one thing and not four: **the roots that belong to the
# machinery rather than to the project**, each with the reason it is machinery.
# It is a path prefix and not a basename — `image/build` and never `build`, so
# nothing called `build` anywhere else in a project falls through it — and it is
# the only place the exclusions are written down, read by the baseline walk and
# the final walk alike through `manifest()` and `mtimes()`, because two lists
# that agree by coincidence are how this harness got a `find -type f` pipeline
# and a summary that reasoned about a different tree.
#
# ## Why this cannot hide a modification the agent made
#
# Because nothing here is unmeasured — it is **set aside**, which is a different
# thing. `set_aside()` walks each of these roots and reports how many entries it
# holds and a digest of their types, modes and sizes, on both sides, into the
# summary. A change under `image/build` is therefore still on the record; what
# it stops doing is deciding `restored`. An exclusion you can read the effect of
# is not a hiding place, and the four cases in `manifest_self_test` are the
# proof: the socket may appear and the restore still holds, and a real file
# changed *beside* the socket, an unlisted file appearing anywhere, and a mode,
# a symlink or a byte moving all still fail.
#
# Contents under a set-aside root are deliberately **not** hashed. `image/build`
# holds the kernel image, the initramfs and the store disk, and the store disk
# is a file QEMU has open read-write while the run is happening: hashing it
# would be measuring the machine's own writes, slowly, and calling them the
# project's.
OUTSIDE_THE_WORKSPACE = (
    # Both arms carry it and it changes for reasons that are not the task: a
    # `git status` in arm A rewrites the index. Hashing it would fail arm A for
    # a restore it performed correctly, and only arm A.
    ".git",
    # What `image/Makefile`'s staging leaves out of the copy it puts on the
    # store, so a project that carries them is not the same tree in the two
    # arms.
    "target",
    "node_modules",
    # What `make -C image` builds: the kernel tree, the bzImage, the initramfs,
    # the store disk, and the socket QEMU opens for the agent channel. All of it
    # is the machinery that carries the benchmark, none of it is the project,
    # and `.gitignore` has said exactly that since before this benchmark
    # existed. This is the entry that was missing on 2026-08-29.
    "image/build",
)


def _outside(where):
    """Whether this path, relative to the tree root, is machinery and not project.

    A whole-segment prefix match: `image/build` excludes `image/build/agent.sock`
    and does not exclude `image/builder.rs`. Anything less than whole-segment
    would make the list a substring rule, which is the ad-hoc exclusion this
    replaced.
    """
    return where in OUTSIDE_THE_WORKSPACE or any(
        where.startswith(root + "/") for root in OUTSIDE_THE_WORKSPACE
    )


def _entries(root):
    """Every entry of the workspace below `root`, with the machinery pruned.

    One walk, used by `manifest()` and by `mtimes()`, so that the digest and the
    witness can never come to disagree about what the workspace is.
    """
    root = pathlib.Path(root)
    for here, directories, files in os.walk(root, followlinks=False):
        base = pathlib.Path(here).relative_to(root).as_posix()
        prefix = "" if base == "." else base + "/"
        directories[:] = [d for d in directories if not _outside(prefix + d)]
        for name in list(directories) + list(files):
            where = prefix + name
            if _outside(where):
                continue
            try:
                info = (pathlib.Path(here) / name).lstat()
            except OSError as why:
                yield where, pathlib.Path(here) / name, why
                continue
            yield where, pathlib.Path(here) / name, info


def set_aside(root):
    """What each machinery root holds, so that setting it aside hides nothing.

    Types, modes, sizes and paths — never contents, for the reason written
    above. Absent rather than empty for a root the tree does not have: rule 10,
    a root that is not there is not a root that is empty.
    """
    root = pathlib.Path(root)
    report = {}
    for machinery in OUTSIDE_THE_WORKSPACE:
        top = root / machinery
        if not top.exists() and not top.is_symlink():
            continue
        lines = []
        for here, directories, files in os.walk(top, followlinks=False):
            base = pathlib.PurePosixPath(pathlib.Path(here).relative_to(root).as_posix())
            for name in list(directories) + list(files):
                where = (base / name).as_posix()
                try:
                    info = (pathlib.Path(here) / name).lstat()
                except OSError as why:
                    lines.append(f"?\t-\t-\tunstattable:{why.errno}\t{where}")
                    continue
                kind = ("l" if stat.S_ISLNK(info.st_mode)
                        else "d" if stat.S_ISDIR(info.st_mode)
                        else "f" if stat.S_ISREG(info.st_mode)
                        else f"s{info.st_mode & 0o170000:o}")
                size = info.st_size if stat.S_ISREG(info.st_mode) else "-"
                lines.append(f"{kind}\t{stat.S_IMODE(info.st_mode):04o}\t{size}\t{where}")
        lines.sort()
        report[machinery] = {
            "entries": len(lines),
            "shape": hashlib.sha256("\n".join(lines).encode()).hexdigest(),
        }
    return report


def _digest_of_file(path):
    """The content hash of one regular file, or why it could not be read.

    Rule 10: a file that could not be read is written down as unreadable, never
    as empty. An unreadable file that hashed the same as an empty one would let
    a permission change pass as a restore.
    """
    hasher = hashlib.sha256()
    try:
        with open(path, "rb") as bytes_in:
            for chunk in iter(lambda: bytes_in.read(1 << 20), b""):
                hasher.update(chunk)
    except OSError as why:
        return f"unreadable:{why.errno}"
    return hasher.hexdigest()


def manifest(root):
    """Every entry of the workspace under `root`, as one sorted line each.

    Symlinks are never followed — `os.walk(followlinks=False)` and `lstat`
    throughout — because following them would take the harness outside the
    workspace, and because the question is what the link *is*, not what it
    points at.

    What the workspace is, and what is machinery, is `OUTSIDE_THE_WORKSPACE`
    above and nowhere else.
    """
    lines = []
    for where, path, info in _entries(root):
        if isinstance(info, OSError):
            lines.append(f"?\t-\t-\tunstattable:{info.errno}\t{where}")
            continue
        mode = stat.S_IMODE(info.st_mode)
        if stat.S_ISLNK(info.st_mode):
            try:
                target = os.readlink(path)
            except OSError as why:
                target = f"unreadable:{why.errno}"
            lines.append(f"l\t{mode:04o}\t-\t{target}\t{where}")
        elif stat.S_ISDIR(info.st_mode):
            lines.append(f"d\t{mode:04o}\t-\t-\t{where}")
        elif stat.S_ISREG(info.st_mode):
            lines.append(f"f\t{mode:04o}\t{info.st_size}\t{_digest_of_file(path)}\t{where}")
        else:
            # A fifo, a socket, a device node. Named by kind rather than
            # lumped in with "other", because an agent that left a fifo
            # where a file was has not restored the tree and the summary
            # should be able to say what it left.
            lines.append(f"s{info.st_mode & 0o170000:o}\t{mode:04o}\t-\t-\t{where}")

    lines.sort(key=lambda line: line.rsplit("\t", 1)[-1])
    return "\n".join(lines) + ("\n" if lines else "")


def manifest_digest(root):
    return hashlib.sha256(manifest(root).encode()).hexdigest()


# ── the answer key must not be inside the corpus ─────────────────────────────
#
# `--expect-file` is what the grader checks the final answer against. It is the
# answer key. On the compact run of 2026-08-30 that key was
# `dev/bench-expect/<name>.txt` — a file *of the checkout the benchmark uses as
# its corpus* — and arm B spent one whole MCP call reading it. Every number that
# run produced about how much work an arm did to find something is therefore a
# number about how much work it took to read the answer.
#
# Nothing about that defect is specific to the symbol, the six files or the task,
# so nothing here is: the guard hashes whatever was passed as `--expect-file` and
# looks for those bytes anywhere the agent could reach.
#
# ## Why the scope is wider than `_entries`
#
# `_entries` is what a *restore* is judged over, and it prunes `.git` and
# `image/build` because they move for reasons that are not the task. Neither of
# those is a reason an agent cannot read them: arm A's copy is `tar` minus
# `target` and `node_modules`, so `.git` is right there. A guard that reused the
# restore boundary would have declared a key in `.git/` safe.
#
# So the walk here prunes only what neither arm is given, and it errs the one
# way a guard is allowed to err — refusing a run that was fine costs a sentence,
# and allowing one that leaked costs both arms and the conclusion drawn from
# them. Rule 9.
NEVER_STAGED = ("target", "node_modules")


def _reachable(root):
    """Every regular file an arm could open, with only the never-staged pruned."""
    root = pathlib.Path(root)
    for here, directories, files in os.walk(root, followlinks=False):
        base = pathlib.Path(here).relative_to(root).as_posix()
        prefix = "" if base == "." else base + "/"
        directories[:] = [d for d in directories
                          if (prefix + d) not in NEVER_STAGED
                          and not any((prefix + d).startswith(r + "/") for r in NEVER_STAGED)]
        for name in files:
            where = prefix + name
            path = pathlib.Path(here) / name
            # Never followed, so a link cannot walk this guard out of the tree,
            # and a link is not the bytes anyway.
            if path.is_symlink():
                continue
            yield where, path


def answer_key_leak(expect, project):
    """Whether the file the grader answers from is also inside the corpus.

    Cheap on purpose — it runs before a cent is spent, so it must never be a
    reason not to run it. One hash of the key, then a size comparison per file
    and a hash only of the files whose size already matches.

    Fails closed in both directions rule 9 cares about. A key that cannot be
    read is not a key that did not leak: it is reported as `unreadable` and the
    caller stops, because the one thing this must never do is answer "no leak"
    for a question it could not ask.
    """
    report = {
        "expect_file": str(expect),
        "project": str(project),
        "leaked": False,
        "found": [],
        "because": "",
    }
    try:
        key = pathlib.Path(expect).read_bytes()
    except OSError as why:
        report["unreadable"] = f"{expect}: {why}"
        report["because"] = (
            f"the expect file could not be read ({why}), so whether it is inside "
            f"the corpus is not something this run knows"
        )
        return report
    report["expect_bytes"] = len(key)
    report["expect_digest"] = hashlib.sha256(key).hexdigest()
    if not key:
        # A zero-byte key carries no answer, so it cannot leak one — and every
        # empty file in the corpus would match it. Said out loud rather than
        # silently passed: an empty answer key is its own defect.
        report["because"] = "the expect file is empty, so it holds no answer to leak"
        return report

    try:
        project = pathlib.Path(project).resolve()
    except OSError as why:
        report["unreadable"] = f"{project}: {why}"
        report["because"] = f"the corpus could not be resolved ({why})"
        return report

    for where, path in _reachable(project):
        try:
            if path.stat().st_size != len(key):
                continue
            if path.read_bytes() != key:
                continue
        except OSError:
            # Unreadable *here* is not fail-open: a file this process cannot
            # read is one whose size already matched, and it is reported as a
            # candidate rather than dismissed.
            report["found"].append(f"{where} (same size, unreadable)")
            continue
        report["found"].append(where)

    if report["found"]:
        report["leaked"] = True
        report["because"] = (
            f"the answer key {expect} is byte-identical to "
            + ", ".join(report["found"][:8])
            + f" inside {project}. An agent that opens that file has been handed "
              "the answer, so neither arm measures the work of finding it"
        )
    else:
        report["because"] = "the answer key is nowhere inside the corpus"
    return report


def mtimes(root):
    """When each regular file of the workspace was last written, and last changed.

    Two stamps per file and a header saying which tree they were taken from,
    because they answer the same question with different weaknesses.

    Kept **out** of the manifest and in a file of its own, because neither is
    something the task asks anybody to restore — `git checkout -- .` puts every
    byte back and moves every stamp, and a digest that folded them in would fail
    arm A for doing the task correctly.

    ## mtime, and the hole in it

    `st_mtime` is *when the contents were last written*, and `utimensat` can set
    it to anything. An agent that restores from a `cp -a` copy, or unpacks a tar
    it made first, puts the contents back **and the mtimes back**, and this
    witness sees a workspace nothing ever happened to. That is not a theory: it
    is one of the two explanations for a run on 2026-08-29 that made six `Edit`
    calls, ended restored, and was witnessed as unchanged.

    ## ctime, which is the half an agent cannot put back

    `st_ctime` is when the *inode* last changed, and there is no syscall that
    sets it. Writing a file moves it; `utimensat` moves it; `chmod` moves it;
    restoring the old mtime moves it again. Short of raw surgery on an unmounted
    filesystem, a file that was written between the two walks has a ctime that
    says so, whatever the agent did afterwards to tidy up. It is the one witness
    on this host that a correct restore cannot erase.

    ## Why the header, and why ctime is not always usable

    `cp -a` preserves mtime and cannot preserve ctime, so **every** file in a
    copied tree has a fresh one. Arm B's two walks are of two different trees —
    this host's `$PROJECT` before, and an export off the store afterwards, which
    is a `cp -a` — so its ctimes would say every file was touched, which is a
    false positive of the loudest possible kind. The header records which tree
    was walked, and `files_touched` uses the ctimes only when the two walks were
    of the same one. Rule 5: the instrument includes the harness, and an
    instrument that only works in one arm has to know which arm it is in.
    """
    lines = [f"# root\t{pathlib.Path(root).resolve().as_posix()}"]
    body = []
    for where, _path, info in _entries(root):
        if isinstance(info, OSError) or not stat.S_ISREG(info.st_mode):
            continue
        body.append(f"{info.st_mtime_ns}\t{info.st_ctime_ns}\t{where}")
    body.sort(key=lambda line: line.split("\t", 2)[2])
    return "\n".join(lines + body) + "\n"


def workspace_lines(text):
    """The manifest lines that are the project, with the machinery dropped.

    The manifest **file** is the primary record and the digest beside it is a
    summary of it, so a walk taken under an older exclusion list can still be
    re-read under the current one — which is the whole reason the restore check
    of a run that is already over could be corrected without spending another
    run. Nothing is recomputed from the filesystem here: the bytes were hashed
    when the walk happened and those hashes are in the line.
    """
    kept, dropped = [], 0
    for line in text.splitlines():
        if not line:
            continue
        where = line.rsplit("\t", 1)[-1]
        if _outside(where):
            dropped += 1
            continue
        kept.append(line)
    return kept, dropped


def files_touched(before_text, after_text):
    """How many files the filesystem says were written between the two walks.

    A file counts as touched when it existed before and a stamp moved, or when
    it did not exist before and does now. A file that vanished counts too:
    something removed it. All three are writes the workspace saw, and none of
    them can be produced by an agent that only read.

    Which stamps count is decided by the headers, not by hope — see `mtimes`.
    The two walks being of the same tree is what makes `ctime` usable, and it is
    the only stamp an agent that put everything back cannot have put back too.
    """
    def read(text):
        root, found = None, {}
        for line in text.splitlines():
            if line.startswith("# root\t"):
                root = line.split("\t", 1)[1]
                continue
            if "\t" not in line:
                continue
            parts = line.split("\t")
            if len(parts) == 2:
                # A walk taken before this file wrote two stamps. Rule 10: what
                # it did not record is absent, not zero.
                when, where = parts[0], parts[1]
                changed = None
            else:
                when, changed, where = parts[0], parts[1], "\t".join(parts[2:])
            # The same boundary as the manifest, and for the same reason: a
            # socket QEMU opened is not the workspace holding another state.
            if _outside(where):
                continue
            found[where] = (when, changed)
        return root, found

    was_root, before = read(before_text)
    now_root, after = read(after_text)
    # `cp -a` gives a whole tree fresh ctimes, so comparing them across two
    # different trees would report every file as touched. Arm B is exactly that
    # shape and this is the line that keeps it honest.
    same_tree = was_root is not None and was_root == now_root

    touched = 0
    for where, (when, changed) in after.items():
        if where not in before:
            touched += 1
            continue
        was_when, was_changed = before[where]
        if was_when != when:
            touched += 1
        elif same_tree and changed is not None and was_changed is not None \
                and was_changed != changed:
            touched += 1
    for where in before:
        if where not in after:
            touched += 1
    return touched


def mutation_class(name, given):
    """What this one call did to the workspace, as far as the stream can say.

    Three answers and not two, which is the whole of the 2026-08-29 repair:

        "writes"    this call can do nothing else. `Edit`, `Write`, and the
                    Thalyx tools whose whole purpose is to change a file.
        "reads"     this call can do nothing else. `Read`, `Grep`, `Glob`, the
                    index questions, `thalyx_edit` with `action: "show"`.
        "unknown"   **the stream cannot tell.** `Bash` above all, and any tool
                    nobody has put in one of the two lists above.

    ## Why a third answer, and why `Bash` is not "reads"

    The forensic table of the failed run printed, for a call whose command was
    `git checkout -- <path>`:

        Bash  write=False

    That is a shell command which *restores files* — the single most important
    mutation in the whole `reversible` task — reported with the same word the
    table uses for a `Grep`. No line of code claimed `False` meant "proven not
    to have written"; it did not have to. A two-valued field has no way to say
    "I do not know", so the answer it gives for the thing it cannot see is the
    answer it gives for the thing it checked.

    **A tool name is a statement of intent, never evidence of an effect.** So
    intent and effect are separated: this function classifies the *call*, and
    the filesystem witness — the walks this harness takes on either side of the
    run, which no name can talk its way past — is the authority on whether a
    different state ever existed. `unknown` is counted, reported, and it makes
    a witness that saw nothing come back `not_proven` instead of `false`.

    Deciding it any other way would need a shell parser that recognised every
    mutating command in every language `Bash` can reach — `sed -i`, `>`, `git
    checkout`, `install`, `mv`, a python one-liner, `make`. That parser cannot
    be written correctly, and one that is nearly correct is worse than none:
    it would answer `False` with confidence for whatever it had not thought of,
    which is exactly the failure this is repairing.
    """
    if name in WORKSPACE_WRITERS:
        # `thalyx_edit` is the reason this is a function and not a set
        # membership: its `show` action returns numbered lines and writes
        # nothing, so counting it as a mutation would credit arm B with edits it
        # did not make — an error in the flattering direction.
        if name == "mcp__thalyx__thalyx_edit" and (given or {}).get("action") == "show":
            return "reads"
        return "writes"
    if name in PROVEN_READERS:
        return "reads"
    return "unknown"


def is_a_write(name, given):
    """Whether this call is one that can only have written.

    Kept as its own name because "counts as a mutation" and "is not known to
    have read" are different questions and the counters need the first. It is
    **not** the negation of anything: `is_a_write` coming back false says only
    that this call is not a certain write, and `mutation_class` is what says
    which of the two other things it is.
    """
    return mutation_class(name, given) == "writes"


# ── where a call was pointed, and whether that is inside the experiment ──────
#
# The 2026-08-29 run was not a comparison, and the reason was one line of shell.
# `--out` defaults to `$ROOT/target/bench-external-agent`, `$ROOT` is the
# checkout `dev/bench-external-agent.sh` lives in, and arm A's copy is made at
# `$OUT/a`. So the agent was started, physically, **inside Cesar's own working
# clone of this project** — and Claude Code collects `CLAUDE.md` from every
# ancestor of its working directory. Arm A therefore began the task holding this
# repository's own instructions, which open with "read this before anything
# else" and name `vault/06-Pendientes/Punto-Actual.md`, and it worked in
# `~/thalyx` because the context it had been handed was about `~/thalyx`.
#
# Anchoring is two halves and it needs both. The first is physical and lives in
# the harness: stage arm A's workspace where nothing above it belongs to
# anything, and refuse to start if an ancestor carries a `CLAUDE.md`, a
# `.claude/`, a `.mcp.json` or a `.git`. The second is this file: read back, out
# of the stream the run already wrote, **every path any call named**, and say
# whether any of them left the workspace. Telling the model where to work is not
# a control. Checking afterwards where it worked is.
#
# The same classification runs live as a `PreToolUse` hook (`--scope-guard`), so
# a call that would leave the workspace is refused rather than merely counted.
# One implementation, two callers — the mistake this project keeps making is
# two implementations that agree until they do not.

# Roots that hold programs and kernel interfaces and never project data. A path
# under one of these is outside the workspace and is *not* a breach: `/usr/bin/git`
# is how a shell command names the shell command. Reported under its own key, so
# the allowance is on the record rather than out of sight — the same discipline
# as the machinery boundary above.
NOT_PROJECT_DATA = ("/usr/", "/bin/", "/sbin/", "/lib/", "/lib64/", "/proc/", "/sys/", "/dev/")

# Where in a tool's input a path lives, when the tool has a schema rather than a
# command line. Everything else is caught by the sweep for values that look like
# absolute paths, which is why this list not being exhaustive is safe.
# `paths` and `to` are here because of what they cost. A tool that names its
# files in a **list** — `thalyx_edit`'s `paths`, and every batch shape after it —
# was invisible to this sweep: `paths` was not a field it knew, and the fallback
# below only looks at values that are strings, so a list of six absolute paths
# out of the workspace was six paths nobody checked. It was caught by a self-test
# whose control column disagreed with it, which is the only reason it is not
# still true. `to` is `thalyx_file`'s destination, missing for the same reason
# under a different name.
PATH_FIELDS = ("file_path", "file_paths", "path", "paths", "notebook_path", "file",
               "files", "dir", "directory", "cwd", "target", "source", "destination", "to")


def _unquote(token):
    for quote in ("'", '"'):
        if len(token) >= 2 and token.startswith(quote) and token.endswith(quote):
            return token[1:-1]
    return token.strip("'\"")


def paths_named(name, given):
    """Every path this one call pointed at, as the call itself spelled it.

    Two sources, because tools come in two shapes. A tool with a schema names
    its paths in named fields; `Bash` names them inside one string, and the only
    honest thing to do with that string is to look at every token in it. Both
    over-collect on purpose: a token that turns out not to be a path resolves to
    somewhere inside the workspace and is dropped by the classification, whereas
    a path this never looked at is a path nobody checked.

    `cd` is picked out by name and not by shape. It is the call that moved arm A
    into `~/thalyx`, its argument is very often relative, and a relative
    argument is exactly the one a sweep for `/`-leading tokens cannot see.
    """
    found = []
    given = given if isinstance(given, dict) else {}

    for field in PATH_FIELDS:
        value = given.get(field)
        if isinstance(value, str) and value.strip():
            found.append((field, value.strip()))
        elif isinstance(value, list):
            found.extend((field, one.strip()) for one in value
                         if isinstance(one, str) and one.strip())

    # Anything else in the input that is spelled like an absolute path or a home
    # reference. A tool added later with a field nobody listed above is covered
    # by this and not by a guess about its schema.
    for key, value in given.items():
        if key in PATH_FIELDS or not isinstance(value, str):
            continue
        text = value.strip()
        if key != "command" and (text.startswith("/") or text.startswith("~")):
            found.append((key, text))

    command = given.get("command")
    if isinstance(command, str):
        # Shell punctuation becomes whitespace so that `cd /a&&ls` is two words.
        broken = command
        for punctuation in ("&&", "||", ";", "|", "\n", "(", ")", "{", "}", "<", ">"):
            broken = broken.replace(punctuation, " ")
        words = [_unquote(word) for word in broken.split()]
        # A `cd`'s argument is reported once, as a `cd`. Without this the sweep
        # below sees the same absolute path again and the summary reports two
        # breaches where the command made one.
        spoken_for = set()
        for n, word in enumerate(words):
            if word in ("cd", "pushd", "chdir") and n + 1 < len(words):
                spoken_for.add(n + 1)
        for n, word in enumerate(words):
            if word in ("cd", "pushd", "chdir") and n + 1 < len(words):
                # Named as its own kind: a `cd` is not an operation on a file,
                # it is the thing that decides what every later path means.
                found.append(("cd", words[n + 1]))
            elif n in spoken_for:
                continue
            elif word.startswith("/") or word.startswith("~") or word.startswith("$HOME"):
                found.append(("command", word))
            elif word.startswith("../") or word == "..":
                found.append(("command", word))
    return found


def where_it_points(raw, workspace, home=None):
    """Which of four places a path a call named is in.

        "inside"            below the workspace, or relative and staying there
        "outside"           somewhere else entirely: this is the breach
        "not_project_data"  a program or a kernel interface — `/usr/bin/git`
        "unreadable"        a path this could not resolve at all

    A relative path is resolved against the workspace, which is only sound
    because the harness asserts, from the run's own `system init` event, that
    the agent's working directory *is* the workspace. Without that assertion
    this function would be assuming the very thing that went wrong.
    """
    text = (raw or "").strip()
    if not text:
        return "unreadable"
    if text.startswith("~"):
        if home:
            text = str(home) + text[1:]
        else:
            # A home reference on a host whose home nobody recorded. It cannot
            # be resolved and it cannot be waved through: rule 9, the cautious
            # answer.
            return "outside"
    if text.startswith("$HOME"):
        if not home:
            return "outside"
        text = str(home) + text[len("$HOME"):]
    if "$" in text:
        # A path built out of a variable this cannot expand. Not a breach on its
        # own — reported as what it is, so it can be looked at.
        return "unreadable"

    workspace = os.path.normpath(str(workspace))
    if text.startswith("/"):
        resolved = os.path.normpath(text)
    else:
        resolved = os.path.normpath(os.path.join(workspace, text))

    if resolved == workspace or resolved.startswith(workspace + os.sep):
        return "inside"
    if any(resolved.startswith(root) or resolved + "/" == root for root in NOT_PROJECT_DATA):
        return "not_project_data"
    return "outside"


# ── two arms, two boundaries, and they are not the same boundary ─────────────
#
# The regrade of 2026-08-29 found arm B reported `scope: VIOLATED` for a run in
# which nothing had gone wrong. The grader was comparing
#
#     /home/bench-thalyx                     the workspace, inside the machine
#     …/target/bench-external-agent-3/b      the directory `claude` was started in
#
# and calling them different, which they are: **they are not in the same
# namespace**, and no comparison between them means anything at all. Arm B's
# `claude` runs on this host with every file tool taken away from it; the empty
# directory it stands in is not a workspace it failed to stay inside, it is the
# floor of a room with nothing in it.
#
# So the two words are separated, permanently, and every check below says which
# one it is about:
#
#   host_control_cwd
#       where the `claude` process itself was started, on this host. Read from
#       the stream's own `system init` event.
#
#   guest_project_workspace
#       where the project the task is about actually is. For arm A a directory
#       on this host; for arm B a path inside the machine, reached only through
#       the socket.
#
# **Arm A's boundary is that those two are the same directory.** It has ordinary
# file tools and an ordinary shell, so the only thing that keeps it in the tree
# it was given is that it was started there and never left — which is exactly
# what failed on 2026-08-29, and why `cwd_is_the_workspace` is a conjunct of its
# verdict rather than a note beside it.
#
# **Arm B's boundary is the channel.** Its `host_control_cwd` is infrastructure
# and is deliberately somewhere with nothing in it; what makes the arm what it
# says it is:
#
#   1. it holds **no host tool that could read or write the project** — the run
#      takes `Read`, `Edit`, `Write`, `Grep`, `Glob`, `Bash` away, and a stream
#      with one of them in it is a run whose confinement did not happen;
#   2. every path its Thalyx tools **accepted** resolves under the guest
#      workspace;
#   3. every path those tools **answered with** does too — that is the machine's
#      own boundary, seen from outside it;
#   4. the preflight proved, before a cent was spent, that the channel was
#      pointed at the guest workspace holding this project.
#
# A path a Thalyx tool was *asked* for and **refused** is not a breach: it is
# the boundary working, which is the thing the experiment is about. So an
# outside path is only counted against arm B when its own `tool_result` came
# back without an error — rule 4, the difference between a denial and an
# operation that never happened.

# The boundary each arm is judged under. Named rather than inferred at each use.
HOST_WORKSPACE = "host_control_cwd_is_the_workspace"
GUEST_WORKSPACE = "host_control_cwd_is_infrastructure"

# Which model an arm is judged under when its provenance does not say.
#
# Not a guess: `dev/bench-external-agent.sh` writes `run_arm A` for the Linux
# copy on this host and `run_arm B` for the arm that only has the socket, in
# that file, hard-coded. Runs from before the provenance carried `boundary`
# still have to be gradeable — the whole point of keeping the streams — and this
# is the fact that lets them be.
BOUNDARY_BY_ARM = {"A": HOST_WORKSPACE, "B": GUEST_WORKSPACE}

# The tools that would let an arm reach the project without going through the
# channel. `Task` is here because a subagent is handed the parent's tools, and
# `NotebookEdit` because it writes files under another name.
HOST_FILE_TOOLS = frozenset({
    "Read", "Edit", "MultiEdit", "Write", "NotebookEdit", "NotebookRead",
    "Grep", "Glob", "Bash", "BashOutput", "KillShell", "Task",
})

# Where a Thalyx answer spells a path. The values are compared against the
# guest workspace, which is the only place the machine may name.
ANSWERED_PATH_KEYS = ("path", "paths", "writes_to", "not_written", "workspace",
                      "root", "to", "from", "name")


def is_a_thalyx_tool(name):
    """Whether this call went down the channel rather than to the host.

    By prefix and not by an exact list: the MCP server is mounted under a name
    the harness picks (`mcp__thalyx__…`), and a list of tool names here would
    be a second catalogue to keep in step with `crates/thalyx-mcp/src/tools.rs`.
    """
    return name.startswith("mcp__") and "thalyx" in name


def answered_paths(content):
    """Every path a tool's answer named, whatever depth it named it at.

    Thalyx answers are one JSON object per line. Anything that will not parse is
    reported as unread rather than treated as empty: rule 10, a failure to read
    is not a failure to exist.
    """
    found, unread = [], 0
    for line in (text_of(content) or "").splitlines():
        line = line.strip()
        if not line or not line.startswith("{"):
            continue
        try:
            answer = json.loads(line)
        except json.JSONDecodeError:
            unread += 1
            continue

        def walk(node, key=None):
            if isinstance(node, dict):
                for name, value in node.items():
                    walk(value, name)
            elif isinstance(node, list):
                for value in node:
                    walk(value, key)
            elif isinstance(node, str) and key in ANSWERED_PATH_KEYS and node.strip():
                found.append((key, node.strip()))

        walk(answer)
    return found, unread


def _calls_in(path):
    """Every `tool_use` in a stream, with the result that answered it.

    One pass for both, because a call's verdict depends on its answer and the
    answer arrives in a later event — the same pairing `read_stream` does, for
    the same reason, and deliberately not shared with it: that function counts
    and this one classifies, and a helper serving both would grow a flag saying
    which caller it had.
    """
    init_cwd = None
    made = []
    failed, answered = set(), set()
    results = {}
    for event in events(path):
        kind = event.get("type")
        if kind == "system" and event.get("subtype") == "init":
            if isinstance(event.get("cwd"), str):
                init_cwd = event["cwd"]
        elif kind == "assistant":
            for block in event.get("message", {}).get("content", []) or []:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    made.append((block.get("id"), block.get("name") or "<unnamed>",
                                 block.get("input")))
        elif kind == "user":
            content = event.get("message", {}).get("content")
            for block in content if isinstance(content, list) else []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    where = block.get("tool_use_id")
                    if where is None:
                        continue
                    answered.add(where)
                    results[where] = block.get("content")
                    if block.get("is_error") is True:
                        failed.add(where)
    return init_cwd, made, failed, answered, results


def guest_scope_report(path, workspace, preflight=None):
    """Whether arm B was the arm it says it was.

    Four questions, and not one of them is "did the process stay in its
    directory". That question has no answer here and asking it is what produced
    a false `VIOLATED`: the process's directory is on this host and the
    workspace is inside a machine.
    """
    report = {
        "boundary": GUEST_WORKSPACE,
        "guest_project_workspace": str(workspace),
    }
    init_cwd, made, failed, answered, results = _calls_in(path)
    if init_cwd is not None:
        report["host_control_cwd"] = init_cwd
    # Said out loud, because the whole fault was somebody comparing these two.
    report["host_control_cwd_is_not_compared"] = (
        "arm B's process runs on this host and its workspace is inside the "
        "machine; the two are different namespaces and a comparison between "
        "them is not a fact about either"
    )

    host_tools, refused, accepted, unreadable = [], [], [], []
    answered_outside, answers_unread = [], 0
    by_name = {}
    for call_id, name, given in made:
        by_name[name] = by_name.get(name, 0) + 1
        if name in HOST_FILE_TOOLS:
            # No path test at all. This arm is defined by not having the tool,
            # so having used it is the finding — whether the call succeeded
            # says something about the CLI, not about the experiment.
            host_tools.append({"tool": name, "input": given,
                               "answered_with_an_error": call_id in failed})
            continue
        if not is_a_thalyx_tool(name):
            # `ToolSearch`, `TodoWrite` and their kind: they name no path and
            # cannot reach the project. Counted so the record is complete.
            continue
        # The host's home means nothing inside the machine, so `~` is not
        # expanded here — it is unresolvable, which is the cautious answer.
        for field, raw in paths_named(name, given):
            verdict = where_it_points(raw, workspace, home=None)
            if verdict == "inside":
                continue
            entry = {"tool": name, "field": field, "path": raw}
            if verdict == "unreadable":
                unreadable.append(entry)
            elif call_id in failed:
                # The machine was asked and said no. That is the boundary
                # working; counting it as a breach would score Thalyx down for
                # the one thing it is being measured for.
                refused.append(entry)
            else:
                accepted.append(entry)
        found, unread = answered_paths(results.get(call_id))
        answers_unread += unread
        for field, raw in found:
            if where_it_points(raw, workspace, home=None) != "inside":
                answered_outside.append({"tool": name, "field": field, "path": raw})

    report["tools_used"] = dict(sorted(by_name.items()))
    report["host_file_tools_used"] = host_tools[:40]
    report["host_file_tools_used_count"] = len(host_tools)
    report["paths_the_machine_accepted_outside_the_workspace"] = accepted[:40]
    report["paths_the_machine_accepted_outside_the_workspace_count"] = len(accepted)
    report["paths_the_machine_refused"] = refused[:40]
    report["paths_this_could_not_resolve"] = unreadable[:40]
    report["paths_answered_outside_the_workspace"] = answered_outside[:40]
    report["paths_answered_outside_the_workspace_count"] = len(answered_outside)
    if answers_unread:
        report["answers_this_could_not_read"] = answers_unread

    # The fourth conjunct, and the only one that is not read out of the stream:
    # that the channel was pointed at the right tree before anything was paid
    # for. Absent evidence is NOT PROVEN and never a pass.
    report["preflight"] = preflight_evidence(preflight, workspace)

    breached = bool(host_tools) or bool(accepted) or bool(answered_outside)
    if breached:
        report["scope"] = "VIOLATED"
        why = []
        if host_tools:
            why.append(f"{len(host_tools)} call(s) to host file tools "
                       f"({', '.join(sorted({one['tool'] for one in host_tools}))}), which this "
                       "arm is defined by not having")
        if accepted:
            why.append(f"{len(accepted)} path(s) outside the guest workspace that the machine "
                       "accepted rather than refused")
        if answered_outside:
            why.append(f"{len(answered_outside)} path(s) outside the guest workspace in the "
                       "machine's own answers")
        report["scope_because"] = "; ".join(why)
    elif report["preflight"]["proven"] is False:
        # Evidence **against**, not missing evidence. Somebody probed the
        # channel before the run and what came back was a different workspace,
        # a different project, or a machine that said it was not ready — and a
        # run made over that channel is a run about a tree nobody asked for.
        report["scope"] = "VIOLATED"
        report["scope_because"] = report["preflight"]["because"]
    elif report["preflight"]["proven"] is None:
        report["scope"] = "not_proven"
        report["scope_because"] = report["preflight"]["because"]
    elif not made:
        report["scope"] = "not_proven"
        report["scope_because"] = ("the stream carries no tool call at all, so there is "
                                   "nothing in it that went through the channel")
    else:
        report["scope"] = "INTACT"
    return report


def boundary_of(side, arm):
    """Which boundary this arm is judged under, from its provenance or its name.

    The provenance is asked first so that a harness which grows a third arm says
    so in writing rather than being guessed at by a letter. `BOUNDARY_BY_ARM` is
    the fallback and it is a fact rather than a default — see its comment.
    """
    named = (side or {}).get("boundary")
    if named in (HOST_WORKSPACE, GUEST_WORKSPACE):
        return named
    return BOUNDARY_BY_ARM.get(arm, HOST_WORKSPACE)


def preflight_for(out, arm):
    """The verdict the preflight left for this arm, or nothing if none was written.

    Read here rather than inside the report so that the report is a function of
    what it is handed: a check that reaches for a file on its own is a check
    whose self-test needs a filesystem.
    """
    if out is None:
        return None
    where = pathlib.Path(out) / f"preflight-{arm.lower()}.verdict.json"
    if not where.exists() or not where.stat().st_size:
        return None
    try:
        return json.loads(where.read_text())
    except (OSError, json.JSONDecodeError):
        # Rule 10, and rule 9 with it: a verdict this could not read is not a
        # verdict that said yes.
        return {"unreadable": str(where)}


def preflight_evidence(preflight, workspace):
    """Whether somebody proved the channel reached this workspace, before the run.

    Its own function because the answer has three values and two of them are not
    failures. `proven: None` means nobody looked — which is what every run
    before this check existed will say, and saying it is the point.
    """
    if not isinstance(preflight, dict):
        return {"proven": None,
                "because": "no preflight verdict was written down for this arm, so nothing "
                           "says the channel was pointed at this workspace"}
    said = {"ready": preflight.get("ready"),
            "workspace": preflight.get("workspace"),
            "top_level_matches": preflight.get("top_level_matches")}
    trouble = []
    if preflight.get("ready") is not True:
        trouble.append("the preflight did not come back ready")
    if preflight.get("top_level_matches") is not True:
        trouble.append("the preflight never confirmed the machine was holding this project")
    seen = preflight.get("workspace")
    if not isinstance(seen, str) or not seen:
        trouble.append("the preflight did not say which workspace the machine answered for")
    elif os.path.normpath(seen) != os.path.normpath(str(workspace)):
        trouble.append(f"the preflight reached {seen!r} and the provenance says the workspace "
                       f"is {workspace!r}")
    said["proven"] = not trouble
    said["because"] = ("the preflight reached this workspace and found this project"
                       if not trouble else "; ".join(trouble))
    return said


def scope_report(path, workspace, home=None, repository=None,
                 boundary=HOST_WORKSPACE, preflight=None):
    """Whether one arm stayed inside the boundary that arm actually has.

    A dispatcher and not a check: the two arms are confined by different things,
    and one function that tried to judge both under one rule is the function
    that reported arm B as having strayed out of a directory it was never in.
    """
    if boundary == GUEST_WORKSPACE:
        return guest_scope_report(path, workspace, preflight=preflight)
    return host_scope_report(path, workspace, home=home, repository=repository)


def host_scope_report(path, workspace, home=None, repository=None):
    """Where the agent actually was, and everywhere its calls actually pointed.

    Reads only the stream the run already wrote, so it costs nothing and can be
    pointed at a run that is long over — which is how the failed run of
    2026-08-29 can be graded for this without being paid for again.

    `cwd_reported` comes from Claude Code's own `system init` event, which
    carries the working directory the process started in. That is the field that
    would have said, in the first line of the first stream, that arm A was not
    where `--project` put it.
    """
    report = {"boundary": HOST_WORKSPACE, "workspace": str(workspace),
              # The same two names the guest report uses. For this arm they are
              # one directory, and saying so is what makes the two reports
              # readable side by side instead of in two vocabularies.
              "guest_project_workspace": str(workspace)}
    outside, unreadable, allowed = [], [], 0

    for event in events(path):
        if event.get("type") == "system" and event.get("subtype") == "init":
            if isinstance(event.get("cwd"), str):
                report["cwd_reported"] = event["cwd"]
                report["host_control_cwd"] = event["cwd"]
            continue
        if event.get("type") != "assistant":
            continue
        for block in event.get("message", {}).get("content", []) or []:
            if not isinstance(block, dict) or block.get("type") != "tool_use":
                continue
            name = block.get("name") or "<unnamed>"
            for field, raw in paths_named(name, block.get("input")):
                verdict = where_it_points(raw, workspace, home)
                if verdict == "inside":
                    continue
                if verdict == "not_project_data":
                    allowed += 1
                    continue
                entry = {"tool": name, "field": field, "path": raw}
                (unreadable if verdict == "unreadable" else outside).append(entry)

    if "cwd_reported" in report:
        report["cwd_is_the_workspace"] = (
            os.path.normpath(report["cwd_reported"]) == os.path.normpath(str(workspace))
        )
    report["paths_outside_the_workspace"] = outside[:40]
    report["paths_outside_the_workspace_count"] = len(outside)
    report["paths_this_could_not_resolve"] = unreadable[:40]
    report["paths_under_program_roots"] = allowed
    if repository:
        # The specific breach that happened, named as itself: the original
        # checkout the harness was launched from. A run that touched *that* is
        # not a run that wandered, it is a run measuring the wrong tree.
        where = os.path.normpath(str(repository))
        report["paths_in_the_original_checkout"] = [
            entry for entry in outside
            if os.path.normpath(os.path.join(str(workspace), entry["path"])) == where
            or entry["path"].startswith(where)
        ][:40]

    intact = not outside
    if "cwd_is_the_workspace" in report:
        intact = intact and report["cwd_is_the_workspace"]
        report["scope"] = "INTACT" if intact else "VIOLATED"
    else:
        # No `init` event: an older stream, or one that was cut off before the
        # CLI said where it was. The paths can still be judged; where the agent
        # started cannot, and that is NOT PROVEN rather than either answer.
        report["scope"] = "VIOLATED" if outside else "not_proven"
        report["scope_because"] = (
            "the stream carries no `system init` event, so nothing in it says which "
            "directory the agent started in"
        )
    return report


def events(path):
    """Every JSON object in an NDJSON stream, skipping what will not parse.

    A truncated last line is the ordinary way a killed run ends, and losing the
    whole summary to it would lose exactly the run worth looking at.
    """
    for line in path.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            yield json.loads(line)
        except json.JSONDecodeError:
            continue


def scope_guard(hook, workspace, home=None):
    """Should this one call, about to be made, be allowed to happen?

    The live half of `scope_report`, wired as a `PreToolUse` hook so a call that
    would leave the workspace is **refused** rather than counted afterwards.
    Same classification, same function, one implementation — a guard that
    reasoned about paths its own summary would not is two instruments that agree
    until the day they matter.

    Returns `(allowed, why)`. The caller turns that into a hook exit code: this
    knows about paths and nothing about Claude Code's protocol.

    ## Why refusing is a control and not a handicap

    Arm B cannot reach outside its workspace at all. Its tools take paths
    relative to a workspace root inside the machine, and Thalyx refuses a path
    that resolves out of it — that is the boundary the whole experiment is about.
    Arm A, on an ordinary Linux copy with an ordinary shell, could reach the
    entire host, and on 2026-08-29 it did. Confining arm A to the same tree is
    what makes the two columns answer the same question. It costs arm A nothing
    it needs: everything the task asks about is in the tree it was given.
    """
    name = hook.get("tool_name") or ""
    given = hook.get("tool_input")
    for field, raw in paths_named(name, given):
        verdict = where_it_points(raw, workspace, home)
        if verdict == "outside":
            return False, (
                f"`{raw}` is outside this run's workspace ({workspace}). Everything "
                f"this task is about is inside it; work there, with paths relative to "
                f"it. This is the benchmark's boundary, not the model's."
            )
    return True, ""


def _commit(root):
    """What commit a tree is of, when it is a checkout and `git` is here.

    Absent rather than empty when it is neither, and never invented: rule 10. A
    `+dirty` suffix because a working tree with uncommitted changes is not the
    commit it claims to be, and two arms staged from it at different moments are
    not necessarily the same tree.
    """
    import subprocess  # only where a stamp is written, never on the reading path

    try:
        head = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"],
                              capture_output=True, text=True, timeout=20)
        if head.returncode != 0:
            return None
        state = subprocess.run(["git", "-C", str(root), "status", "--porcelain"],
                               capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        return None
    commit = head.stdout.strip()
    return commit + ("+dirty" if state.stdout.strip() else "")


def project_top_level(root):
    """The names at the root of a project, with the machinery pruned.

    What the preflight compares against the machine's own `list .`, and the
    cheapest evidence there is that the copy on the store is the copy that was
    handed to `--project`. It is not a hash — nothing on this host can hash a
    tree inside a live Btrfs image — and it does not pretend to be: it is the
    check that catches the failure that actually happens, which is a store left
    over from another project or from a `project-stage` nobody re-ran.
    """
    return sorted(
        entry.name for entry in pathlib.Path(root).iterdir()
        if not _outside(entry.name)
    )


def preflight_verdict(report, project=None):
    """Whether arm B is ready to be paid for, decided outside the probe.

    The probe (`thalyx-mcp --preflight`) talks to the machine; this decides what
    its answer means. They are apart on purpose: the decision can then be tested
    against a dead machine, a machine holding the wrong project and a healthy
    one, all three for free and none of them needing a VM — which is the only
    way this check itself gets checked in a container with no KVM.
    """
    verdict = {"ready": False, "because": []}
    if not isinstance(report, dict):
        verdict["because"].append("the probe printed nothing this could read")
        return verdict

    verdict["thalyx"] = report.get("thalyx")
    verdict["workspace"] = report.get("workspace")
    verdict["tools_offered"] = report.get("tools_offered")

    if report.get("ready") is not True:
        said = report.get("because") or ["the probe did not say it was ready"]
        verdict["because"].extend(said if isinstance(said, list) else [str(said)])

    if project is not None:
        here = project_top_level(project)
        there = sorted(name for name in (report.get("top_level") or []) if not _outside(name))
        verdict["project_top_level"] = here
        verdict["machine_top_level"] = there
        if not there:
            verdict["because"].append("the machine's workspace root listed nothing")
        elif here != there:
            missing = sorted(set(here) - set(there))
            extra = sorted(set(there) - set(here))
            verdict["because"].append(
                "the machine is holding a different tree from --project: "
                f"missing {missing or 'nothing'}, unexpected {extra or 'nothing'}. "
                "The store was staged from another project, or `project-stage` was "
                "never re-run"
            )
        else:
            verdict["top_level_matches"] = True

    verdict["ready"] = not verdict["because"]
    return verdict


def parity_verdict(provenance):
    """Whether the two arms were given the same thing to work on.

    Four facts, and the claim is only that they are **comparable** — never that
    the trees are byte-identical, which nothing on this host can check while the
    store is inside a live image. What it does check is every way they have
    actually diverged: a different source commit, a different input digest, a
    different exclusion list, or an arm whose provenance nobody wrote down.
    """
    verdict = {"comparable": False, "because": []}
    arms = (provenance or {}).get("arms") or {}
    a, b = arms.get("A"), arms.get("B")
    verdict["source_commit"] = (provenance or {}).get("source_commit")
    verdict["exclusions"] = (provenance or {}).get("exclusions")

    for name, side in (("A", a), ("B", b)):
        if not side:
            verdict["because"].append(f"arm {name} has no provenance recorded")
    if verdict["because"]:
        return verdict

    verdict["input_manifest"] = {"A": a.get("input_manifest"), "B": b.get("input_manifest")}
    verdict["imported_from"] = {"A": a.get("imported_from"), "B": b.get("imported_from")}
    verdict["effective_cwd"] = {"A": a.get("effective_cwd"), "B": b.get("effective_cwd")}

    if not a.get("input_manifest") or not b.get("input_manifest"):
        verdict["because"].append("one of the arms has no input manifest digest")
    elif a["input_manifest"] != b["input_manifest"]:
        verdict["because"].append(
            "the two arms were given different trees: arm A's copy and the tree "
            "arm B was imported from do not hash the same"
        )
    if a.get("imported_from") != b.get("imported_from"):
        verdict["because"].append(
            f"the arms were staged from different places: arm A from "
            f"{a.get('imported_from')!r}, arm B from {b.get('imported_from')!r}"
        )
    verdict["comparable"] = not verdict["because"]
    return verdict


def text_length(content):
    """How much text a tool handed back, whatever shape it came in."""
    if isinstance(content, str):
        return len(content)
    if isinstance(content, list):
        total = 0
        for block in content:
            if isinstance(block, dict) and isinstance(block.get("text"), str):
                total += len(block["text"])
            elif isinstance(block, str):
                total += len(block)
        return total
    return 0


def read_stream(path, marker=None):
    """The whole of one arm, from its stream.

    `marker` is the new name the `reversible` task renames the symbol to. Given
    one, every tool call is also checked for it — in the *whole* serialised
    input, not a chosen field, because the same rename arrives as `Edit` with a
    `new_string` in one arm, as `thalyx_edit` with a `text` in the other, and as
    a `sed` command inside `Bash` when the agent felt like it.

    ## Why a call that names the marker is not evidence that anything changed

    Until 2026-08-28 it was counted as exactly that, and three different runs
    that changed nothing would have scored as runs that changed everything:

      - `Grep {"pattern": "WidgetRenamed"}` names the new name and is a read.
      - `Edit {"new_string": "WidgetRenamed"}` names it and comes back
        `is_error: true` because `old_string` matched nothing — the workspace
        never held the new name for an instant.
      - `thalyx_edit {"action": "show"}` was already excluded, which is the same
        mistake caught once in one place and left everywhere else.

    So each call is now paired with its `tool_result` by `tool_use_id`, and the
    marker is counted in three separate buckets: named in a call that can only
    mutate **and whose result came back without an error**, named in a call that
    failed, and named in a call that was not a mutation at all. Only the first
    is evidence, and the other two are kept because a run that produced nothing
    but those is a run whose summary should say why it lost.

    A call with no result in the stream — the ordinary shape of a run killed at
    its turn limit mid-call — counts as **not** a successful mutation. Rule 9:
    the cautious answer, never the fast one.
    """
    row = {}
    per_tool = {}
    calls = 0
    returned = 0
    sent = 0
    results_seen = 0
    writes = 0
    unknown = 0
    unknown_tools = {}
    naming = 0
    # Pass one collects the calls; the results that decide them arrive in later
    # events, so nothing about success can be settled inside the loop.
    made = []
    written = []
    failed_ids = set()
    answered_ids = set()
    # What the agent's own `num_turns` is a count of, counted here so the two
    # can be compared instead of assumed equal. See `TURNS_MEAN`.
    assistant_ids = []
    assistant_ids_with_a_tool_use = set()
    calls_per_message = {}
    user_messages = 0

    for event in events(path):
        kind = event.get("type")

        if kind == "assistant":
            # One API response can arrive as several `assistant` events — the
            # captured session's thinking block and its tool call carry the same
            # `message.id` — so an event is not a message and counting events
            # would count the model's responses twice.
            said = (event.get("message") or {}).get("id")
            if said is not None and said not in assistant_ids:
                assistant_ids.append(said)
            for block in event.get("message", {}).get("content", []) or []:
                if not isinstance(block, dict) or block.get("type") != "tool_use":
                    continue
                name = block.get("name") or "<unnamed>"
                given = block.get("input", {})
                per_tool[name] = per_tool.get(name, 0) + 1
                calls += 1
                if said is not None:
                    assistant_ids_with_a_tool_use.add(said)
                    calls_per_message[said] = calls_per_message.get(said, 0) + 1
                serialised = json.dumps(given)
                sent += len(serialised)
                how = mutation_class(name, given)
                if how == "writes":
                    writes += 1
                    written.append(block.get("id"))
                elif how == "unknown":
                    # Not `not a write`. A call whose effect the stream cannot
                    # see is its own fact and it is kept as one.
                    unknown += 1
                    unknown_tools[name] = unknown_tools.get(name, 0) + 1
                if marker and marker in serialised:
                    naming += 1
                    made.append((block.get("id"), name, given))

        elif kind == "user":
            user_messages += 1
            content = event.get("message", {}).get("content")
            for block in content if isinstance(content, list) else []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    returned += text_length(block.get("content"))
                    results_seen += 1
                    where = block.get("tool_use_id")
                    if where is not None:
                        answered_ids.add(where)
                        if block.get("is_error") is True:
                            failed_ids.add(where)

        elif kind == "result":
            # The last one wins: a stream only has one, but a file that was
            # appended to twice should report the run that finished.
            row["is_error"] = event.get("is_error")
            row["stop_reason"] = event.get("stop_reason")
            for field, into in (
                ("num_turns", "turns"),
                ("duration_ms", "wall_ms"),
                ("duration_api_ms", "api_ms"),
                ("total_cost_usd", "cost_usd"),
            ):
                if event.get(field) is not None:
                    row[into] = event[field]
            denials = event.get("permission_denials")
            if isinstance(denials, list):
                row["permission_denials"] = len(denials)
            usage = event.get("usage") or {}
            for field in (
                "input_tokens",
                "output_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
            ):
                if field in usage:
                    row[field] = usage[field]
            if isinstance(event.get("result"), str):
                row["answer"] = event["result"]

    if assistant_ids:
        # Three counts of three different things, because `turns` is one number
        # and the run was given a limit on another. See `TURNS_MEAN` for what
        # each one is and which of them `--max-turns` bounds.
        row["assistant_messages"] = len(assistant_ids)
        row["assistant_messages_with_a_tool_use"] = len(assistant_ids_with_a_tool_use)
        row["most_tool_calls_in_one_message"] = max(calls_per_message.values(), default=0)
    if user_messages:
        row["user_messages_in_the_stream"] = user_messages

    if calls or results_seen:
        row["tool_calls"] = calls
        row["tools_used"] = dict(sorted(per_tool.items()))
        row["bytes_returned_to_model"] = returned
        row["bytes_sent_by_model"] = sent
        row["files_read"] = sum(n for t, n in per_tool.items() if t in FILE_READERS)
        row["text_searches"] = sum(n for t, n in per_tool.items() if t in TEXT_SEARCHERS)
        index = sum(n for t, n in per_tool.items() if t in INDEX_QUESTIONS)
        # Only where such a thing can exist. Arm A has no index, and a zero
        # there would read as "it had one and did not use it".
        if index or any(t.startswith("mcp__thalyx__") for t in per_tool):
            row["index_questions"] = index
        # Four facts and not one, kept apart on purpose: what the model asked
        # for, what a tool said it did, what an instrument outside the agent
        # saw, and what the tree looked like at the end. `mutating_tool_calls`
        # is only the first of them — a call the model *made*, counted from its
        # request, before anything answered it. Reading it as "six edits
        # happened" is what made a run with six `Edit`s and a failure in each
        # of them indistinguishable from a run that did the work.
        row["mutating_tool_calls"] = writes
        # The count that keeps `mutating_tool_calls` honest. Zero certain writes
        # and eleven `Bash` calls is not a run that wrote nothing; it is a run
        # nobody can say that about from the stream, and the difference has to
        # be on the face of the summary rather than in a comment.
        row["calls_of_unknown_mutation"] = unknown
        if unknown_tools:
            row["tools_of_unknown_mutation"] = dict(sorted(unknown_tools.items()))
        confirmed = sum(1 for where in written if where in answered_ids and where not in failed_ids)
        row["mutating_tool_calls_confirmed"] = confirmed
        row["mutating_tool_calls_that_failed"] = sum(1 for where in written if where in failed_ids)
        row["mutating_tool_calls_never_answered"] = sum(
            1 for where in written if where not in answered_ids
        )
        # Absent, not zero, where no marker was given: a task with no rename in
        # it has no new name to have been named, and a `0` would read as an
        # agent that never wrote one.
        if marker:
            row["tool_calls_naming_the_new_name"] = naming
            confirmed = wrong = read_only = unclear = unclear_ok = unanswered = 0
            for where, name, given in made:
                how = mutation_class(name, given)
                if how == "reads":
                    read_only += 1
                elif how == "unknown":
                    # A `Bash` that named the new name. It is not a read and it
                    # is not a proven write: `grep WidgetRenamed` and
                    # `sed -i s/Widget/WidgetRenamed/` are the same tool name.
                    unclear += 1
                    if where in answered_ids and where not in failed_ids:
                        unclear_ok += 1
                elif where in failed_ids:
                    wrong += 1
                elif where in answered_ids:
                    confirmed += 1
                else:
                    unanswered += 1
            row["mutations_naming_the_new_name"] = confirmed
            row["failed_calls_naming_the_new_name"] = wrong
            row["read_only_calls_naming_the_new_name"] = read_only
            row["unknown_calls_naming_the_new_name"] = unclear
            # The half of that which the tool answered without an error, which
            # is the only half the shell case can be built out of. Counted here
            # rather than subtracted later: the old arithmetic —
            # `named - failed - unanswered` — quietly included read-only calls,
            # and a `Grep` for the new name is not a rename.
            row["answered_unknown_calls_naming_the_new_name"] = unclear_ok
            # Counted apart from the failures because it is a different fact: a
            # call the stream never carried an answer for. It is not evidence
            # of a mutation and it is not evidence of a failure either.
            row["unanswered_calls_naming_the_new_name"] = unanswered

    if marker and isinstance(row.get("answer"), str):
        row["new_name_in_answer"] = marker in row["answer"]

    return row


# ── what `turns` is a count of, and what `--max-turns` bounds ────────────────
#
# They are not the same number, and on 2026-08-29 a run reported `turns: 37`
# under `--max-turns 30` and nobody could say whether it had been cut short.
#
# `turns` is copied verbatim out of the agent's own `result` event
# (`num_turns`) and this parser invents nothing about it. What Claude Code puts
# there is a count of the **user messages in the conversation**: the CLI's
# result builder increments it once per message of type `user`, and in `-p` mode
# those are the first prompt plus one per batch of tool results. Read off the
# captured session in `dev/samples/`, which is one `Read`: one user message in
# the stream, `num_turns: 2`.
#
#     turns  ==  user messages in the stream + 1   (the prompt, never echoed)
#
# `--max-turns` bounds a **different** counter: the agentic loop's own
# `turnCount`, which goes up once per round trip to the API — that is, once per
# assistant message that asked for tools and had to be answered. The loop stops
# when `turnCount + 1 > max_turns`.
#
#     bounded by --max-turns  ==  assistant_messages_with_a_tool_use
#
# The two are equal only while the model asks for exactly one tool at a time.
# The moment it asks for two in one message, one round trip produces two tool
# results, `turns` gains two and the bounded counter gains one — so `turns` can
# and does exceed `--max-turns` without the limit ever having been near.
#
# None of that is inferred from the numbers: it is the flag's own description
# ("Maximum number of agentic turns in non-interactive mode", hidden from
# `--help`) and the CLI's own counters, read out of the Claude Code bundle
# (2.1.251) on 2026-08-29. What this file does about it is refuse to have one
# ambiguous number: `turns` stays exactly as the agent printed it, the three
# counts this parser can make itself sit beside it, and `turns_mean` says so in
# the summary. `turn_limit_was_reached` is the only honest answer to "was it cut
# short", and it comes from the run's own stop reason, never from comparing two
# numbers that count different things.
TURNS_MEAN = (
    "`turns` is the agent's own `num_turns`: the user messages in the "
    "conversation, which is the prompt plus one per batch of tool results. "
    "`--max-turns` bounds a different counter — the API round trips, which is "
    "`assistant_messages_with_a_tool_use`. `turns` exceeding `max_turns` is "
    "therefore not a run that overran its limit; it is a run that asked for "
    "more than one tool in some message."
)


def judged(answer, expectations):
    """Whether the answer contains everything the task's ground truth demands.

    Substrings and not a model grading a model. It is crude on purpose: the
    expectations file is written by hand from the corpus, so a pass means the
    agent named every file it had to name, and a fail is readable — the summary
    says which strings were missing.
    """
    missing = [want for want in expectations if want not in answer]
    return {"task_success": not missing, "missing_from_answer": missing}


# What is missing when nobody has hashed an arm's tree after the run. Spelled
# once, printed as it is, and it names the command rather than the problem —
# `Primer-Arranque.md`'s rule everywhere else in this project: a refusal that
# does not carry its remedy is a refusal somebody has to go and research.
NO_TREE_AFTER = (
    "nothing on this host hashed the tree after the run. For arm B that means "
    "shutting the machine down, `sudo make -C image agent-export INTO=<dir>`, "
    "and re-running with `--arms none --restored-b <dir>`"
)


NO_WITNESS = (
    "nothing outside the agent saw the workspace change. That needs either the "
    "pair of mtime walks this harness writes beside each arm's manifest, or an "
    "arm whose adapter counted mutations of its own (`thalyx-mcp --metrics`)"
)


def reversible_verdict(row, marker_given, graded):
    """Whether this arm did the reversible task, from instruments that differ.

    The one thing this must never do is read the verdict off the tree digest
    alone. A restored digest is what an agent that changed everything and put it
    back leaves, and equally what an agent that answered "no" and stopped
    leaves — so the digest can only ever be one conjunct.

    ## The four facts, which are four and not one

    The audit of 2026-08-29 found them collapsed into two, and that is what
    turned a run into a false negative. They are:

      1. **asked**       the model requested a call that can only mutate.
                         `mutating_tool_calls`, counted off the request. Six of
                         these is not six edits: an `Edit` whose `old_string`
                         matched nothing is one of them too.
      2. **confirmed**   the *tool* answered without an error.
                         `mutating_tool_calls_confirmed`. This is the tool's
                         own word and not the model's: it is written by Claude
                         Code after the write returned, and no wording in the
                         final answer can produce it.
      3. **witnessed**   something outside the agent saw the workspace hold
                         another state. Three sources, below.
      4. **restored**    the manifest digest came back.

    ## The three witnesses to (3), and why there had to be more than one

    Until 2026-08-29 there was one and a half: the mtimes, plus the adapter for
    arm B. The mtime witness answers *is the workspace different now from how it
    started*, and the task's last step is **put it back** — so the very agents
    the task is trying to distinguish are the ones that erase it. `git checkout
    -- .` moves mtimes and is seen; a `cp -a` of a copy the agent made first, or
    a `tar` it unpacked, restores them and is not. A witness that a correct
    answer can switch off is not a witness.

      the filesystem, by mtime      `files_touched_on_disk > 0`. Weakest: an
                                    agent can restore it. Broadest: it is the
                                    only one that can see `sed -i` inside
                                    `Bash`, which the stream cannot tell from
                                    `ls`.
      the adapter's own count       `thalyx-mcp --metrics` mutations. Arm B
                                    only, and the only one available there
                                    during a run — its workspace is inside the
                                    machine and this host cannot walk it.
      a mutating tool's own result  `mutating_tool_calls_confirmed > 0`. The
                                    strongest, because it is the one an agent
                                    cannot undo: `Edit` cannot answer without
                                    an error unless it wrote, and a later
                                    restore does not reach back into the
                                    stream. It is also the only witness that
                                    works on a run that is already over.

    Any one of them seeing something is enough — they are three views of one
    fact, not three conditions. `false` requires every witness that exists to
    have seen nothing; `not_proven` is when there is no witness at all, and
    `--require-mutation-witness` makes that a non-zero exit.

    ## What each witness can be wrong about

      false positive, mtime         a file written by something other than the
                                    agent. Controlled by (1) and (2): a
                                    `really_changed` still needs a call that
                                    named the new name and did not fail.
      false positive, tool result   a `Write` that wrote back the same bytes
                                    answers without an error and changed
                                    nothing. Narrow, and it cannot reach
                                    `really_changed` unless it also carried the
                                    new name — which a `Write` of the original
                                    bytes does not.
      false negative, mtime         the restore that put the mtimes back. This
                                    is the one that cost a run.
      false negative, tool result   `sed -i` inside `Bash`, which arrives as a
                                    tool name that could equally have been
                                    `ls`. Covered by the mtime witness, which
                                    is why the weak one is kept.

    A component that is unknown makes `passed` **absent**. Not false: a run
    whose restore nobody has checked yet has not failed, and printing `false`
    for it would be the same lie in the other direction.
    """
    verdict = {}

    # ── (1) asked ──
    for field in ("mutating_tool_calls",
                  "mutating_tool_calls_confirmed",
                  "mutating_tool_calls_that_failed",
                  "mutating_tool_calls_never_answered",
                  "mutations_naming_the_new_name",
                  "failed_calls_naming_the_new_name",
                  "read_only_calls_naming_the_new_name"):
        if field in row:
            verdict[field] = row[field]
    if "mutating_tool_calls" in row:
        verdict["mutation_requested"] = row["mutating_tool_calls"] > 0

    # ── (2) confirmed ──
    if "mutating_tool_calls_confirmed" in row:
        verdict["mutation_tool_confirmed"] = row["mutating_tool_calls_confirmed"] > 0

    # ── (3) witnessed, from whichever instruments exist ──
    seen = {}
    if "files_touched_on_disk" in row:
        verdict["files_touched_on_disk"] = row["files_touched_on_disk"]
        seen["the filesystem, by mtime"] = row["files_touched_on_disk"] > 0
    mutations = (row.get("thalyx") or {}).get("mutations")
    if isinstance(mutations, int):
        verdict["thalyx_mutations"] = mutations
        # An adapter that counted zero is a witness that saw nothing, not an
        # absent witness. Leaving it out would turn arm B's laziest possible
        # run — never call a mutating tool — into `not_proven` rather than a
        # loss, which is the exact direction this comparison must not be wrong in.
        seen["the adapter's own count"] = mutations > 0
    if "mutating_tool_calls_confirmed" in row:
        seen["a mutating tool's own result"] = row["mutating_tool_calls_confirmed"] > 0

    witness = None
    if seen:
        verdict["intermediate_state_witnesses"] = dict(sorted(seen.items()))
        witness = any(seen.values())
        agreeing = sorted(about for about, saw in seen.items() if saw)
        verdict["intermediate_state_from"] = ", ".join(agreeing) if agreeing else None
        # Rule 5: when two instruments disagree one of them is wrong, and the
        # summary that averaged them would hide which. They are not averaged —
        # any one of them seeing something is enough, because they answer
        # *was there ever another state* and a `false` from the mtimes means
        # "not visible at the end", never "never happened" — but the
        # disagreement is written down, because it is the shape of a run worth
        # reading twice.
        if len(set(seen.values())) > 1:
            verdict["witnesses_disagree"] = (
                "saw it: " + ", ".join(agreeing) + " — saw nothing: "
                + ", ".join(sorted(about for about, saw in seen.items() if not saw))
                + ". The mtimes are the one an agent can put back, so this is the "
                  "ordinary shape of a change that was made and then undone."
            )
    unknown_calls = row.get("calls_of_unknown_mutation", 0)
    if witness is False and unknown_calls:
        # The rule the failed run is named after. Every witness saw nothing, and
        # the stream carries calls whose effect no witness can see — a `Bash`
        # that may have been `ls` and may have been `git checkout -- .`. Saying
        # `false` there would be the summary asserting, from a tool name, that
        # nothing was written. It does not know that, so it says so.
        witness = None
        verdict["intermediate_state"] = "not_proven"
        verdict["intermediate_state_because"] = (
            f"{NO_WITNESS}. And it cannot be called a run that wrote nothing either: "
            f"{unknown_calls} call(s) in this stream are of tools whose effect the "
            f"stream cannot see"
        )
    elif witness is None:
        verdict["intermediate_state"] = "not_proven"
        verdict["intermediate_state_because"] = NO_WITNESS
    else:
        verdict["intermediate_state"] = witness
    if unknown_calls:
        verdict["calls_of_unknown_mutation"] = unknown_calls

    # ── the new name, which is what makes it *this* task's change ──
    if marker_given and "mutations_naming_the_new_name" in row:
        by_a_mutating_tool = row["mutations_naming_the_new_name"] > 0
        # The shell case: a call whose effect the stream cannot see — a `Bash` —
        # that named the new name and that the tool answered without an error.
        # On its own that is equally a `grep WidgetRenamed`, which is why it
        # only counts alongside a filesystem that moved.
        answered = row.get("answered_unknown_calls_naming_the_new_name", 0)
        by_the_shell = answered > 0 and seen.get("the filesystem, by mtime") is True
        verdict["mutation_attempted"] = by_a_mutating_tool or by_the_shell
        verdict["really_changed"] = verdict["mutation_attempted"] and witness is True

    # ── completed ──
    if "is_error" in row and row["is_error"] is not None:
        verdict["completed_normally"] = row["is_error"] is False

    # ── (4) restored ──
    if "tree_unchanged" in row:
        verdict["restored"] = row["tree_unchanged"]
        verdict["restore_check"] = "proven"
    else:
        verdict["restore_check"] = "not_proven"
        verdict["restore_check_because"] = NO_TREE_AFTER

    needed = [("really_changed", verdict.get("really_changed") if marker_given else True),
              ("intermediate_state", witness),
              ("completed_normally", verdict.get("completed_normally")),
              ("restored", verdict.get("restored"))]
    if graded:
        needed.append(("task_success", row.get("task_success")))

    unknown = [about for about, value in needed if value is None]
    if unknown:
        verdict["undecided_because"] = f"not known: {', '.join(unknown)}"
    else:
        verdict["passed"] = all(value is True for _, value in needed)
    return verdict


def arm(out, name, expectations, marker=None, task="", provenance=None):
    row = {"arm": name}

    stream = out / f"arm{name}.ndjson"
    plain = out / f"arm{name}.json"
    if stream.exists() and stream.stat().st_size:
        row.update(read_stream(stream, marker))
    elif plain.exists() and plain.stat().st_size:
        # The older shape, `--output-format json`: one object, no per-tool
        # detail. Read for what it does carry rather than refused, and the
        # absent fields stay absent.
        try:
            answer = json.loads(plain.read_text())
        except json.JSONDecodeError:
            return {"arm": name, "unreadable": str(plain)}
        for field, into in (
            ("is_error", "is_error"),
            ("num_turns", "turns"),
            ("duration_ms", "wall_ms"),
            ("total_cost_usd", "cost_usd"),
            ("result", "answer"),
        ):
            if answer.get(field) is not None:
                row[into] = answer[field]
        for field in ("input_tokens", "output_tokens", "cache_read_input_tokens",
                      "cache_creation_input_tokens"):
            if field in (answer.get("usage") or {}):
                row[field] = answer["usage"][field]
    else:
        return None

    # ── where this arm actually worked ──
    #
    # Read out of the stream the run already wrote, so a run that is over can be
    # graded for it without being paid for again — which is precisely what the
    # run of 2026-08-29 needs.
    side = ((provenance or {}).get("arms") or {}).get(name) or {}
    workspace = side.get("effective_cwd")
    if workspace and stream.exists() and stream.stat().st_size:
        row["scope"] = scope_report(
            stream, workspace,
            home=(provenance or {}).get("home"),
            repository=(provenance or {}).get("repository"),
            boundary=boundary_of(side, name),
            preflight=preflight_for(out, name),
        )

    metrics = out / f"arm{name}.metrics.json"
    if metrics.exists():
        try:
            row["thalyx"] = json.loads(metrics.read_text())
        except json.JSONDecodeError:
            row["thalyx"] = {"unreadable": str(metrics)}

    row.update(work_between_inferences(row))

    # ── did the tree come back ──
    #
    # From the **manifests** and not from the digest files beside them,
    # whenever the manifests exist. The digest is one line that says nothing
    # about what moved, and — the reason this order matters — it was computed
    # under whatever `OUTSIDE_THE_WORKSPACE` said on the day of the walk. The
    # manifest is the record; re-reading it under today's boundary is how a run
    # graded with a wrong boundary can be graded again without being re-run.
    manifests = [out / f"arm{name}.{side}.manifest" for side in ("before", "after")]
    digests = [out / f"arm{name}.{side}" for side in ("before", "after")]
    if all(part.exists() and part.stat().st_size for part in manifests):
        was, dropped_before = workspace_lines(manifests[0].read_text())
        now, dropped_after = workspace_lines(manifests[1].read_text())
        row["tree_unchanged"] = was == now
        row["restore_check_read_from"] = "the manifests, under this file's current boundary"
        # Rule 4's baseline, turned into a number. Two walks of the same tree
        # share nearly every path even when every byte moved; two walks of
        # *different* trees do not, and that is a harness mistake wearing a
        # failed restore's clothes — `armB.before` is a hash of `$PROJECT`, on
        # the assumption that the store carries the same project, and a stale
        # store is exactly how that assumption breaks.
        here = {line.rsplit("\t", 1)[-1] for line in was}
        there = {line.rsplit("\t", 1)[-1] for line in now}
        if here or there:
            row["paths_shared_between_the_walks"] = round(
                len(here & there) / max(len(here | there), 1), 4)
        if dropped_before or dropped_after:
            # Not silent. A boundary whose effect nobody can see is a hiding
            # place; a boundary whose effect is printed beside the verdict is a
            # boundary.
            row["machinery_lines_set_aside"] = {"before": dropped_before, "after": dropped_after}
        if row["tree_unchanged"] is False:
            gone, arrived = set(was), set(now)
            row["tree_differences"] = sorted(
                [f"-{line}" for line in gone - arrived] + [f"+{line}" for line in arrived - gone]
            )[:40]
    elif all(part.exists() for part in digests):
        row["tree_unchanged"] = digests[0].read_text() == digests[1].read_text()
        row["restore_check_read_from"] = (
            "the digest files, which were computed under whatever boundary was "
            "in force when the walk was taken — the manifests are missing"
        )

    # What each machinery root held on either side, so that setting it aside is
    # on the record rather than out of sight. Absent when the walks predate
    # this, which is not the same fact as empty.
    aside = {}
    for side in ("before", "after"):
        where = out / f"arm{name}.{side}.setaside"
        if where.exists() and where.stat().st_size:
            try:
                aside[side] = json.loads(where.read_text())
            except json.JSONDecodeError:
                aside[side] = {"unreadable": str(where)}
    if aside:
        row["machinery_set_aside"] = aside

    # The external witness. Two walks of the same tree, taken by this host
    # before the agent started and after it stopped, compared on mtime alone.
    # Absent rather than zero when either walk is missing: nobody looked is not
    # the same fact as nobody wrote.
    walks = [out / f"arm{name}.{side}.mtimes" for side in ("before", "after")]
    if all(walk.exists() for walk in walks):
        row["files_touched_on_disk"] = files_touched(walks[0].read_text(), walks[1].read_text())

    if expectations and isinstance(row.get("answer"), str):
        row.update(judged(row["answer"], expectations))

    # The full answer is on disk; carrying it in the summary makes the summary
    # unreadable and says nothing the file does not.
    if isinstance(row.get("answer"), str):
        row["answer_chars"] = len(row["answer"])
        del row["answer"]

    if task == "reversible":
        row["reversible"] = reversible_verdict(row, bool(marker), bool(expectations))

    return row


def work_between_inferences(row):
    """How much deterministic machine work happened per model round trip.

    `vault/09-Notas-Tecnicas/Trabajo-Entre-Inferencias.md`. Every other number
    in this file counts what the *agent* did: turns, tool calls, tokens, cost.
    None of them can see the quantity the current hypothesis is about, because
    two runs of one tool call each look identical whether that call did one
    thing or thirty — and doing thirty is the whole bet.

    Read out of `thalyx-mcp --metrics`, which reads it out of the machine's own
    answer. It exists for arm B and for no other arm, and that asymmetry is not
    a thumb on the scale: arm A's Bash calls do many things too, and this cannot
    see inside them. What it can honestly say is what Thalyx counted, which is
    why the fields are named after Thalyx and not after "work".

    Absent, never zero, when no program ran. "This run used no programs" and
    "this run's programs did nothing" are different facts, and a summary that
    printed 0 for the first would report the mechanism as having failed when it
    was simply not reached.
    """
    programs = ((row.get("thalyx") or {}).get("programs")) or {}
    if not isinstance(programs, dict) or not programs.get("run"):
        return {}

    ran = programs["run"]
    operations = programs.get("machine_operations") or 0
    out = {
        "thalyx_programs": ran,
        "thalyx_machine_operations": operations,
        "thalyx_operations_per_program": round(operations / ran, 2),
        "thalyx_programs_committed": programs.get("committed"),
        "thalyx_programs_rolled_back": programs.get("rolled_back"),
    }

    # What the machine produced and did not send back, as a ratio to what it
    # did send back. This is the compression claim, and it is the one number
    # that says whether the small answers are small because there was nothing
    # to say or because the rest stayed inside.
    internal = programs.get("internal_bytes") or 0
    returned = row.get("bytes_returned") or (row.get("thalyx") or {}).get("bytes_returned")
    out["thalyx_internal_bytes"] = internal
    if isinstance(returned, int) and returned > 0:
        out["thalyx_internal_bytes_per_returned_byte"] = round(internal / returned, 2)
    return out


def transcript(path):
    """Every call in one arm's stream, whole: what was asked and what answered.

    `forensics` above is a *table* — one row per mutating call, with the answer
    excerpted — and excerpting is the right thing for the question it asks
    ("did those six Edit calls do anything"). It is the wrong thing for the
    other question, the one that came up on 2026-08-30: **what exactly did the
    first semantic rename say, and why did the model make three more calls
    afterwards.** That is answered only by reading the whole of each request and
    the whole of each answer, and no summary can stand in for it.

    So this prints them untruncated, in order, and interprets nothing. It reads
    a file that is already on disk, costs nothing, and can be pointed at a run
    that is long over.

    ## What it cannot recover, which is worth knowing before you look
    -
    A `thalyx_exec` answer carries what the program chose to `return`. The
    answers to the calls the program made *inside* the machine — what
    `context(...)` resolved, what `rename(...)` reported, whether it said
    `rust-analyzer` or `index` — are in the machine's evidence, under the
    `evidence` id in that answer, and they were never in the stream at all. If
    the program did not return them, this will not show them and nothing on the
    host can: `evidencia <id>` inside the machine is the only place they exist.
    That is the compression working as designed, and it is also the reason the
    handle is in every answer including the ones that went well.
    """
    calls, results = [], {}
    for event in events(path):
        kind = event.get("type")
        if kind == "assistant":
            for block in event.get("message", {}).get("content", []) or []:
                if not isinstance(block, dict):
                    continue
                if block.get("type") == "tool_use":
                    calls.append((block.get("id"), block.get("name") or "<unnamed>",
                                  block.get("input", {})))
                elif block.get("type") == "text" and (block.get("text") or "").strip():
                    # What the model said between calls. Often the whole of the
                    # answer to "why did it do that next".
                    calls.append((None, "<said>", block.get("text")))
        elif kind == "user":
            content = event.get("message", {}).get("content")
            for block in content if isinstance(content, list) else []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    results[block.get("tool_use_id")] = block

    lines = []
    number = 0
    for where, name, given in calls:
        if name == "<said>":
            lines.append("")
            lines.append("  ── the model said ─────────────────────────────────────────")
            for line in str(given).splitlines():
                lines.append(f"    {line}")
            continue
        number += 1
        lines.append("")
        lines.append(f"  ══ call {number}: {name} " + "═" * max(0, 48 - len(name)))
        lines.append("  ── asked ──")
        for key, value in (given if isinstance(given, dict) else {"input": given}).items():
            if isinstance(value, str) and "\n" in value:
                lines.append(f"    {key}:")
                for line in value.splitlines():
                    lines.append(f"      {line}")
            else:
                lines.append(f"    {key}: {value if isinstance(value, str) else json.dumps(value)}")
        answer = results.get(where)
        if answer is None:
            # Rule 10: nothing answered is a different fact from an empty answer.
            lines.append("  ── answered ── NOTHING. No tool_result carries this id.")
            continue
        text = answer.get("content")
        if isinstance(text, list):
            text = "".join(part.get("text", "") for part in text if isinstance(part, dict))
        text = text if isinstance(text, str) else json.dumps(text)
        flag = "  (isError)" if answer.get("is_error") else ""
        lines.append(f"  ── answered ── {len(text)} bytes{flag}")
        # Pretty-printed when it is JSON, because a Thalyx answer is an object
        # and a one-line object is what makes a run unreadable afterwards.
        try:
            text = json.dumps(json.loads(text), indent=2)
        except ValueError:
            pass
        for line in text.splitlines():
            lines.append(f"    {line}")
    return lines


def forensics(path, marker=None):
    """Every call that could have changed something, with what answered it.

    The table nobody had on 2026-08-29, when a run reported six `Edit` calls,
    a restored tree and no change witnessed, and the three explanations —
    six edits undone, six edits that failed, six edits nobody answered — were
    indistinguishable in the summary and distinguishable in the stream all
    along. It reads only what is already on disk, so it costs nothing and can
    be pointed at a run that is long over.

    The result excerpt is the **tool's** text, never the model's. That is the
    whole point of printing it: `Edit` cannot answer "The file … has been
    updated" without having written, and no wording in the final answer can
    produce that line.
    """
    calls, results = [], {}
    for event in events(path):
        kind = event.get("type")
        if kind == "assistant":
            for block in event.get("message", {}).get("content", []) or []:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    calls.append((block.get("id"), block.get("name") or "<unnamed>",
                                  block.get("input", {})))
        elif kind == "user":
            content = event.get("message", {}).get("content")
            for block in content if isinstance(content, list) else []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    results[block.get("tool_use_id")] = block

    rows = []
    for where, name, given in calls:
        serialised = json.dumps(given)
        # Named `did` and not `how`, because `how` is the result state four
        # lines down and the two shadowed each other into one wrong column.
        did = mutation_class(name, given)
        # `reads` is the only class that can be left out, because it is the only
        # one this file claims to know about. A call of unknown effect is
        # printed whether or not it mentions the new name — it is the row that
        # was missing on 2026-08-29, when a `git checkout -- <path>` scrolled
        # past as `write=False`.
        if did == "reads" and not (marker and marker in serialised):
            continue
        answer = results.get(where)
        if answer is None:
            how = "never answered"
            said = ""
        elif answer.get("is_error") is True:
            how = "ERROR"
            said = text_of(answer.get("content"))
        else:
            how = "ok"
            said = text_of(answer.get("content"))
        rows.append({
            "tool": name,
            # Three values, spelled out. There is deliberately no boolean here:
            # a reader who sees `False` reads "it did not write", and for `Bash`
            # that sentence is not one this file is entitled to.
            "mutation": did,
            "names_the_new_name": bool(marker and marker in serialised),
            "result": how,
            "asked": serialised[:200],
            "answered": said[:200].replace("\n", " / "),
        })
    return rows


def text_of(content):
    """The text a tool result carried, whatever shape it came in."""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = []
        for block in content:
            if isinstance(block, dict) and isinstance(block.get("text"), str):
                parts.append(block["text"])
            elif isinstance(block, str):
                parts.append(block)
        return "\n".join(parts)
    return ""


# ── regrading a run that is already over ─────────────────────────────────────
#
# A grader corrected after the run is worth nothing unless it can be pointed at
# the run it was corrected for, and pointing it there must not be able to turn
# "we never measured this" into "it passed". So a regrade says which of the
# three it is, per arm, and never averages them:
#
#   VALID       every conjunct of the verdict is known from evidence that was
#               written down during the run. The verdict stands, pass or fail.
#   NOT PROVEN  some conjunct is unknown because the evidence for it was never
#               written. Not a failure and not a pass — the thing that would
#               have decided it does not exist.
#   INVALID     the evidence that is there cannot be trusted: the run did not
#               finish, or the two walks are of trees so different that the
#               baseline is plainly of something else.
def regrade_status(row):
    verdict = row.get("reversible") or {}

    # Asked first, because it is the question that decides whether the rest of
    # the numbers are about the experiment at all. A run whose arm worked
    # somewhere else has a restore verdict, and that verdict is about a tree
    # nobody was measuring.
    scope = row.get("scope") or {}
    if scope.get("scope") == "VIOLATED":
        return {"status": "INVALID",
                "because": "the arm did not stay inside the boundary it has: "
                           + (scope.get("scope_because") or "see `scope` in this row"),
                "boundary": scope.get("boundary")}
    if scope and scope.get("scope") != "INTACT":
        return {"status": "NOT PROVEN",
                "because": "nothing on disk says whether the arm stayed inside the boundary "
                           "it has: " + (scope.get("scope_because") or "no reason recorded"),
                "boundary": scope.get("boundary")}

    if row.get("is_error") is True:
        return {"status": "INVALID",
                "because": "the run's own result event says it ended in an error, "
                           "so nothing after the failure is a measurement of the task"}

    if row.get("tool_calls") and "turns" not in row:
        return {"status": "INVALID",
                "because": "the stream carries tool calls but no result event, so the "
                           "run's own totals were never printed and the end of the run "
                           "is not on disk"}

    shared = row.get("paths_shared_between_the_walks")
    if isinstance(shared, float) and shared < 0.5:
        return {"status": "INVALID",
                "because": f"the two walks share only {shared:.0%} of their paths, which is "
                           "a baseline taken of a different tree, not a restore that failed"}

    if "undecided_because" in verdict:
        return {"status": "NOT PROVEN", "because": verdict["undecided_because"]}

    if "passed" not in verdict:
        return {"status": "NOT PROVEN",
                "because": "the verdict has no decision in it, and this file will not "
                           "invent one"}

    return {"status": "VALID",
            "boundary": scope.get("boundary"),
            "because": "every conjunct was decided from evidence written down during the "
                       "run: " + ", ".join(
                           f"{about}={verdict.get(about)!r}" for about in
                           ("really_changed", "intermediate_state", "completed_normally",
                            "restored", "passed") if about in verdict)}


# Which fields mean the same thing in both arms, so a difference between them
# is a difference and not a units mistake. Every one of them is read out of the
# agent's own stream in both arms — that is the whole reason the stream is kept.
COMPARABLE = (
    ("cost_usd", "cost"),
    ("wall_ms", "wall time"),
    ("api_ms", "API time"),
    ("turns", "turns, as the agent counts them"),
    ("tool_calls", "tool calls"),
    ("mutating_tool_calls", "calls that can only mutate"),
    ("bytes_returned_to_model", "bytes handed back to the model"),
    ("bytes_sent_by_model", "bytes of tool input the model wrote"),
    ("output_tokens", "output tokens"),
    ("input_tokens", "input tokens"),
    ("files_read", "file reads"),
    ("text_searches", "text searches"),
    ("answer_chars", "characters of final answer"),
)


def observed_deltas(arms):
    """B against A, for this one observation and for nothing else.

    Descriptive only, and named so: no winner, no aggregate, no verdict. One
    run of one task is an anecdote, `Agentes-Externos.md` says so, and a summary
    that printed "Thalyx wins" off a single pair of numbers would be exactly the
    confident wrongness `Estrategia-de-Pruebas.md` exists against.
    """
    by_name = {row.get("arm"): row for row in arms}
    a, b = by_name.get("A"), by_name.get("B")
    if not a or not b:
        return None
    deltas = {}
    for field, about in COMPARABLE:
        if field not in a or field not in b:
            continue
        was, now = a[field], b[field]
        if not isinstance(was, (int, float)) or not isinstance(now, (int, float)):
            continue
        entry = {"about": about, "A": was, "B": now, "B_minus_A": now - was}
        if was:
            entry["B_over_A"] = round(now / was, 4)
            entry["percent"] = round((now - was) / was * 100, 1)
        deltas[field] = entry
    return deltas


def self_test():
    """Parse the captured session and check what that session is known to be."""
    sample = pathlib.Path(__file__).parent / "samples" / "claude-stream-json.ndjson"
    if not sample.exists():
        print(f"  the captured sample is missing: {sample}")
        return 1

    row = read_stream(sample)
    # Every one of these was read off the real session by hand before this
    # parser existed: two turns, one Read, a cost, and an answer that says what
    # the file contained.
    checks = [
        ("the session's turn count", row.get("turns"), 2),
        ("that it did not fail", row.get("is_error"), False),
        ("the tools it called", row.get("tools_used"), {"Read": 1}),
        ("the calls it made", row.get("tool_calls"), 1),
        ("the files it read", row.get("files_read"), 1),
        ("the searches it ran", row.get("text_searches"), 0),
    ]
    trouble = 0
    for about, got, want in checks:
        if got != want:
            print(f"  FAILED  {about}: expected {want!r}, parsed {got!r}")
            trouble += 1
        else:
            print(f"  ok      {about}: {got!r}")

    for about, field in (
        ("a cost", "cost_usd"),
        ("output tokens", "output_tokens"),
        ("wall time", "wall_ms"),
        ("bytes handed back", "bytes_returned_to_model"),
    ):
        if row.get(field) is None:
            print(f"  FAILED  {about} was printed by the agent and is not in the summary")
            trouble += 1
        else:
            print(f"  ok      {about}: {row[field]}")

    # And the other half of rule 10: a field the agent never printed must not
    # appear. `index_questions` is meaningless for a session with no Thalyx.
    for about, field in (
        ("index_questions", "index_questions"),
        ("the new name in a tool call", "tool_calls_naming_the_new_name"),
        ("the new name in the answer", "new_name_in_answer"),
    ):
        if field in row:
            print(f"  FAILED  {about} was invented for a session that had no such thing")
            trouble += 1
        else:
            print(f"  ok      {about} is absent, not zero, where it cannot exist")

    if row.get("mutating_tool_calls") == 0:
        print("  ok      a session that only read counts zero writes")
    else:
        print(f"  FAILED  a read-only session counted {row.get('mutating_tool_calls')!r} writes")
        trouble += 1

    # ── what `turns` counts, pinned to the captured session ──
    #
    # `TURNS_MEAN` says `turns` is the user messages plus the prompt, and that
    # `--max-turns` bounds the API round trips instead. Both identities hold on
    # this session, which is one `Read`: one assistant message that asked for a
    # tool, one that answered in text, one tool result. If a future Claude Code
    # changes what it puts in `num_turns`, this is the line that says so
    # instead of a benchmark quietly reporting a number nobody can name.
    for about, got, want in (
        ("the assistant messages, counted by id and not by event",
         row.get("assistant_messages"), 2),
        ("the round trips --max-turns bounds",
         row.get("assistant_messages_with_a_tool_use"), 1),
        ("the tool results the conversation carried",
         row.get("user_messages_in_the_stream"), 1),
        ("no message asked for more than one tool at a time",
         row.get("most_tool_calls_in_one_message"), 1),
    ):
        if got == want:
            print(f"  ok      {about}: {got!r}")
        else:
            print(f"  FAILED  {about}: expected {want!r}, parsed {got!r}")
            trouble += 1

    if row.get("turns") == row.get("user_messages_in_the_stream", 0) + 1:
        print("  ok      `turns` is the user messages plus the prompt, as TURNS_MEAN says")
    else:
        print(f"  FAILED  `turns` ({row.get('turns')!r}) is not the user messages plus the "
              f"prompt ({row.get('user_messages_in_the_stream')!r} + 1); TURNS_MEAN is stale")
        trouble += 1

    trouble += counting_self_test(sample)
    trouble += between_inferences_self_test()
    trouble += manifest_self_test()
    trouble += verdict_self_test()
    trouble += anchoring_self_test(sample)
    trouble += regrade_self_test(sample)

    print()
    print("  PROVEN" if not trouble else f"  {trouble} FAILED")
    return 1 if trouble else 0


def between_inferences_self_test():
    """That two runs of one call each do not look the same when one did thirty
    things.

    The quantity the current hypothesis is about, and the one every other number
    in this file is blind to. If this ever passes with the two runs equal, the
    summary has stopped being able to see the mechanism at all — which would be
    worse than the mechanism not working, because nobody would know.
    """
    print()
    print("  work between inferences")
    trouble = 0

    busy = work_between_inferences({
        "bytes_returned": 1_000,
        "thalyx": {"programs": {
            "run": 2, "machine_operations": 60, "internal_bytes": 90_000,
            "committed": 1, "rolled_back": 1,
        }},
    })
    if busy.get("thalyx_operations_per_program") == 30.0:
        print("  ok      a run whose programs did sixty things says so")
    else:
        print(f"  FAILED  the ratio is {busy!r}")
        trouble += 1

    if busy.get("thalyx_internal_bytes_per_returned_byte") == 90.0:
        print("  ok      what stayed inside the machine is counted against what left it")
    else:
        print(f"  FAILED  the compression ratio is {busy!r}")
        trouble += 1

    # Absent and not zero. `0` would say the programs did nothing; nothing says
    # no program ran, and the two are different facts about a run.
    quiet = work_between_inferences({"thalyx": {"mutations": 3}})
    if quiet == {}:
        print("  ok      a run that used no programs reports no ratio rather than zero")
    else:
        print(f"  FAILED  a run with no programs reported {quiet!r}")
        trouble += 1

    # An arm with no adapter at all — arm A — must not acquire these fields.
    if work_between_inferences({"arm": "A", "turns": 4}) == {}:
        print("  ok      an arm with no Thalyx adapter is given no Thalyx numbers")
    else:
        print("  FAILED  arm A was given numbers only arm B's adapter can produce")
        trouble += 1

    return trouble


def anchoring_self_test(sample):
    """That an arm which worked somewhere else cannot be graded as if it had not.

    Every case here is the run of 2026-08-29 taken apart. That run was given
    `--project /tmp/bench-thalyx` and arm A ran `cd /home/cesarmanzocode/thalyx`,
    and nothing in the harness or the summary noticed, because nothing in either
    of them ever asked. These are the questions that were not being asked.

    The streams are built out of the captured session's own `system init` and
    `assistant` envelopes — rule 6 again: the format comes from Claude Code, and
    only the contents come from this file.
    """
    trouble = 0

    def ok(about, got, want):
        nonlocal trouble
        if got == want:
            print(f"  ok      {about}: {got!r}")
        else:
            print(f"  FAILED  {about}: expected {want!r}, got {got!r}")
            trouble += 1

    init = envelope = None
    for event in events(sample):
        if event.get("type") == "system" and event.get("subtype") == "init" and init is None:
            init = event
        elif event.get("type") == "assistant" and envelope is None:
            for block in event.get("message", {}).get("content", []) or []:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    envelope = event
                    break
    if init is None or envelope is None:
        print("  FAILED  the captured session has no init event or no tool call, so the "
              "anchoring test has no real envelope to build on")
        return 1

    with tempfile.TemporaryDirectory() as work:
        work = pathlib.Path(work)
        home = work / "home"
        repository = home / "thalyx"
        workspace = work / "bench" / "a"
        for where in (home, repository, workspace):
            where.mkdir(parents=True, exist_ok=True)

        def stream(name, cwd, calls):
            path = work / name
            with path.open("w") as into:
                said = copy.deepcopy(init)
                said["cwd"] = str(cwd)
                into.write(json.dumps(said) + "\n")
                for tool, given in calls:
                    event = copy.deepcopy(envelope)
                    for block in event["message"]["content"]:
                        if block.get("type") == "tool_use":
                            block["name"] = tool
                            block["input"] = given
                    into.write(json.dumps(event) + "\n")
            return path

        # ── 1. the working directory ──
        #
        # The first line of the first stream says where the agent started. On
        # 2026-08-29 nothing read it.
        good = stream("good.ndjson", workspace, [
            ("Read", {"file_path": "src/a.rs"}),
            ("Grep", {"pattern": "Widget", "path": "crates"}),
            ("Bash", {"command": "/usr/bin/git checkout -- src/a.rs"}),
        ])
        report = scope_report(good, workspace, home=home, repository=repository)
        ok("an arm that started in its workspace and stayed there", report["scope"], "INTACT")
        ok("and `/usr/bin/git` is a program, not a path out of the project",
           report["paths_under_program_roots"], 1)

        strayed = stream("strayed.ndjson", repository, [("Read", {"file_path": "src/a.rs"})])
        report = scope_report(strayed, workspace, home=home, repository=repository)
        ok("an arm that started in the original checkout is VIOLATED",
           report["scope"], "VIOLATED")
        ok("and the summary can say where it actually started",
           report["cwd_is_the_workspace"], False)

        # ── 2. an absolute path out of the project ──
        #
        # Literally the call the forensics found: `cd /home/…/thalyx`. The `cd`
        # is picked out by name because its argument is what decides where every
        # later relative path lands.
        wandered = stream("wandered.ndjson", workspace, [
            ("Bash", {"command": f"cd {repository} && grep -rn Widget crates/"}),
        ])
        report = scope_report(wandered, workspace, home=home, repository=repository)
        ok("a `cd` into the original checkout is a violation",
           report["scope"], "VIOLATED")
        ok("and it is named as the original checkout and not as a stray path",
           len(report["paths_in_the_original_checkout"]), 1)

        reading = stream("reading.ndjson", workspace, [
            ("Read", {"file_path": f"{repository}/vault/06-Pendientes/Punto-Actual.md"}),
        ])
        ok("an absolute read out of the workspace is a violation",
           scope_report(reading, workspace, home=home)["scope"], "VIOLATED")

        climbing = stream("climbing.ndjson", workspace, [
            ("Bash", {"command": "cd ../.. && ls"}),
        ])
        ok("and so is climbing out of it with a relative path",
           scope_report(climbing, workspace, home=home)["scope"], "VIOLATED")

        tilde = stream("tilde.ndjson", workspace, [("Read", {"file_path": "~/thalyx/CLAUDE.md"})])
        ok("a path spelled with a `~` is resolved rather than waved through",
           scope_report(tilde, workspace, home=home)["scope"], "VIOLATED")

        # The control, without which a guard that refused everything would look
        # exactly like a guard that works.
        ordinary = stream("ordinary.ndjson", workspace, [
            ("Edit", {"file_path": str(workspace / "src" / "a.rs"),
                      "old_string": "Widget", "new_string": "WidgetRenamed"}),
            ("Bash", {"command": "cd crates/thalyx-cli && ls -la"}),
            ("Glob", {"pattern": "**/*.rs"}),
        ])
        ok("ordinary work inside the workspace, named absolutely or relatively, is not "
           "a violation", scope_report(ordinary, workspace, home=home)["scope"], "INTACT")

        # ── 3. the same rule, live ──
        for about, call, want in (
            ("the guard refuses a `cd` out of the workspace",
             {"tool_name": "Bash", "tool_input": {"command": f"cd {repository}"}}, False),
            ("the guard refuses a read of the original checkout",
             {"tool_name": "Read", "tool_input": {"file_path": f"{repository}/CLAUDE.md"}}, False),
            ("the guard allows an edit inside the workspace",
             {"tool_name": "Edit", "tool_input": {"file_path": "src/a.rs"}}, True),
            ("the guard allows a shell command that names a program",
             {"tool_name": "Bash", "tool_input": {"command": "/usr/bin/git status"}}, True),
        ):
            ok(about, scope_guard(call, workspace, home)[0], want)

        # ── 3b. arm B, whose boundary is the channel and not a directory ──
        #
        # The regrade of 2026-08-29 reported this arm VIOLATED because the
        # grader compared `/home/bench-thalyx` — a path inside the machine —
        # with the empty host directory the `claude` process was started in.
        # These are the shapes that fault had, and the shapes it must not
        # start reporting as clean either.
        guest = "/home/bench-thalyx"
        infrastructure = work / "bench" / "b"
        infrastructure.mkdir(parents=True, exist_ok=True)
        healthy = {"ready": True, "workspace": guest, "top_level_matches": True}

        def channel(name, calls, cwd=infrastructure):
            """A stream in arm B's shape: an init, calls, and the answers to them.

            The answers matter here in a way they do not for arm A. A path the
            machine **refused** is the boundary working; a path it **accepted**
            is the boundary gone. Without the `tool_result` those two are the
            same line in the stream.
            """
            path = work / name
            with path.open("w") as into:
                said = copy.deepcopy(init)
                said["cwd"] = str(cwd)
                into.write(json.dumps(said) + "\n")
                for n, (tool, given, answer, failed) in enumerate(calls):
                    call_id = f"toolu_bench_{n}"
                    event = copy.deepcopy(envelope)
                    for block in event["message"]["content"]:
                        if block.get("type") == "tool_use":
                            block["name"] = tool
                            block["input"] = given
                            block["id"] = call_id
                    into.write(json.dumps(event) + "\n")
                    into.write(json.dumps({
                        "type": "user",
                        "message": {"role": "user", "content": [{
                            "type": "tool_result", "tool_use_id": call_id,
                            "is_error": failed,
                            "content": [{"type": "text", "text": answer}],
                        }]},
                    }) + "\n")
            return path

        working = channel("guest-good.ndjson", [
            ("ToolSearch", {"query": "thalyx"}, "found 10 tools", False),
            ("mcp__thalyx__thalyx_symbol", {"name": "Widget"},
             '{"op":"symbol","ok":true,"defined_in":{"path":"crates/w/src/lib.rs"}}', False),
            ("mcp__thalyx__thalyx_edit",
             {"action": "substitute", "old": "Widget", "new": "WidgetRenamed",
              "paths": ["crates/w/src/lib.rs", "crates/w/src/use.rs"]},
             '{"op":"edit","ok":true,"changed":[{"path":"crates/w/src/lib.rs",'
             '"replacements":3},{"path":"crates/w/src/use.rs","replacements":1}]}', False),
        ])
        report = scope_report(working, guest, home=home, repository=repository,
                              boundary=GUEST_WORKSPACE, preflight=healthy)
        ok("arm B working only through the channel is INTACT, however far its host "
           "directory is from the workspace", report["scope"], "INTACT")
        ok("and the host directory it stood in is recorded rather than compared",
           report["host_control_cwd"], str(infrastructure))

        # The exact shape of the run this fix exists for: a host cwd that is a
        # child of `--out`, a guest workspace with no relation to it, and
        # nothing wrong.
        under_out = channel("guest-under-out.ndjson", [
            ("mcp__thalyx__thalyx_state", {}, '{"op":"where","ok":true,"path":"' + guest + '"}',
             False),
        ], cwd=work / "bench" / "b")
        ok("the 2026-08-29 shape — host cwd under --out, workspace inside the machine — "
           "is not a violation",
           scope_report(under_out, guest, boundary=GUEST_WORKSPACE,
                        preflight=healthy)["scope"], "INTACT")
        # And the control beside it, without which the line above would pass for
        # a grader that had simply stopped checking arm B at all.
        ok("the same stream judged under arm A's boundary is still VIOLATED, so the "
           "fix is a different rule and not a rule switched off",
           scope_report(under_out, guest, boundary=HOST_WORKSPACE)["scope"], "VIOLATED")

        reaching = channel("guest-host-tool.ndjson", [
            ("mcp__thalyx__thalyx_state", {}, '{"op":"where","ok":true}', False),
            ("Read", {"file_path": "crates/w/src/lib.rs"}, "1  pub struct Widget;", False),
        ])
        report = scope_report(reaching, guest, boundary=GUEST_WORKSPACE, preflight=healthy)
        ok("an arm B that reached the project with a host file tool is VIOLATED",
           report["scope"], "VIOLATED")
        ok("and the tool is named", report["host_file_tools_used"][0]["tool"], "Read")

        shelled = channel("guest-bash.ndjson", [
            ("Bash", {"command": "ls crates"}, "permission denied", True),
        ])
        ok("and a host `Bash` is a violation even when it came back an error: the arm "
           "is defined by not having the tool",
           scope_report(shelled, guest, boundary=GUEST_WORKSPACE,
                        preflight=healthy)["scope"], "VIOLATED")

        escaping = channel("guest-escape.ndjson", [
            ("mcp__thalyx__thalyx_edit",
             {"action": "substitute", "old": "a", "new": "b", "paths": ["/etc/passwd"]},
             '{"op":"edit","ok":false,"error":"outside","wrote":false}', True),
        ])
        report = scope_report(escaping, guest, boundary=GUEST_WORKSPACE, preflight=healthy)
        ok("a path outside the workspace that the machine refused is the boundary "
           "working, not a breach", report["scope"], "INTACT")
        ok("and it is on the record as refused", len(report["paths_the_machine_refused"]), 1)

        accepted = channel("guest-accepted.ndjson", [
            ("mcp__thalyx__thalyx_edit",
             {"action": "substitute", "old": "a", "new": "b", "paths": ["/etc/hosts"]},
             '{"op":"edit","ok":true,"changed":[{"path":"/etc/hosts","replacements":1}]}',
             False),
        ])
        ok("the same path *accepted* is a breach, and that is the whole difference",
           scope_report(accepted, guest, boundary=GUEST_WORKSPACE,
                        preflight=healthy)["scope"], "VIOLATED")

        answering = channel("guest-answer.ndjson", [
            ("mcp__thalyx__thalyx_find", {"pattern": "*.rs"},
             '{"op":"find","ok":true,"entries":[{"path":"crates/w/src/lib.rs"},'
             '{"path":"/home/somebody-else/secrets.rs"}]}', False),
        ])
        report = scope_report(answering, guest, boundary=GUEST_WORKSPACE, preflight=healthy)
        ok("a machine that answered with a path outside its own workspace is a breach "
           "of the boundary, seen from outside it", report["scope"], "VIOLATED")
        ok("and the path is named",
           report["paths_answered_outside_the_workspace"][0]["path"],
           "/home/somebody-else/secrets.rs")

        ok("an arm B whose preflight nobody wrote down is NOT PROVEN, not a pass",
           scope_report(working, guest, boundary=GUEST_WORKSPACE,
                        preflight=None)["scope"], "not_proven")
        ok("an arm B whose preflight reached a different workspace is VIOLATED: that is "
           "evidence against, not evidence missing",
           scope_report(working, guest, boundary=GUEST_WORKSPACE,
                        preflight={"ready": True, "workspace": "/home/somewhere-else",
                                   "top_level_matches": True})["scope"], "VIOLATED")
        ok("and so is one whose machine was holding a different project",
           scope_report(working, guest, boundary=GUEST_WORKSPACE,
                        preflight={"ready": True, "workspace": guest,
                                   "top_level_matches": False})["scope"], "VIOLATED")

        # Which rule an arm is judged under, when the provenance predates the
        # question. Every run before today is in that position.
        ok("an old run's arm A is still judged as a host workspace",
           boundary_of({}, "A"), HOST_WORKSPACE)
        ok("and its arm B as a channel", boundary_of({}, "B"), GUEST_WORKSPACE)
        ok("and a provenance that says so out loud wins over the letter",
           boundary_of({"boundary": GUEST_WORKSPACE}, "A"), GUEST_WORKSPACE)

        # ── 4. arm B, before arm A is paid for ──
        (workspace / "Cargo.toml").write_text("[workspace]\n")
        (workspace / "crates").mkdir(exist_ok=True)
        (workspace / "target").mkdir(exist_ok=True)   # machinery: pruned on both sides

        dead = preflight_verdict(None, workspace)
        ok("a probe that printed nothing is not a machine to pay for", dead["ready"], False)

        silent = preflight_verdict({"ready": False, "because": [
            "the socket at /tmp/agent.sock is there and the machine never said hello"
        ]}, workspace)
        ok("a socket with nothing behind it is not READY", silent["ready"], False)

        stale = preflight_verdict({
            "ready": True, "thalyx": "0.1.0", "workspace": "/workspace",
            "tools_offered": 11, "top_level": ["README.md", "src"],
        }, workspace)
        ok("a machine holding a different tree is not READY", stale["ready"], False)

        alive = preflight_verdict({
            "ready": True, "thalyx": "0.1.0", "workspace": "/workspace",
            "tools_offered": 11, "top_level": ["Cargo.toml", "crates", "target"],
        }, workspace)
        ok("a machine that answered and is holding this project is READY",
           alive["ready"], True)

        # ── 5. the two arms were given the same thing ──
        same = {
            "source_commit": "abc123", "exclusions": list(OUTSIDE_THE_WORKSPACE),
            "arms": {
                "A": {"input_manifest": "d34d", "imported_from": "/tmp/bench-thalyx",
                      "effective_cwd": str(workspace)},
                "B": {"input_manifest": "d34d", "imported_from": "/tmp/bench-thalyx",
                      "effective_cwd": "/workspace"},
            },
        }
        ok("two arms staged from the same tree are comparable",
           parity_verdict(same)["comparable"], True)

        different = copy.deepcopy(same)
        different["arms"]["B"]["input_manifest"] = "beef"
        ok("two arms whose input trees hash differently are not",
           parity_verdict(different)["comparable"], False)

        elsewhere = copy.deepcopy(same)
        elsewhere["arms"]["B"]["imported_from"] = "/tmp/some-other-project"
        ok("nor are two arms staged from different places",
           parity_verdict(elsewhere)["comparable"], False)

        missing = {"source_commit": "abc123", "arms": {"A": same["arms"]["A"]}}
        ok("nor is a run where one arm's provenance was never written down",
           parity_verdict(missing)["comparable"], False)

    return trouble


def regrade_self_test(sample):
    """That a run already over can be read again, and cannot be read into a pass.

    End to end over an `--out` directory of the shape a real run leaves, with no
    agent anywhere: the streams are the captured session's envelopes, the walks
    are real walks of real trees. What it proves is the three things a regrade
    must never do — invent a witness, let the machinery decide the restore, or
    turn missing evidence into a verdict.
    """
    trouble = 0

    def envelopes():
        got = [None, None]
        for event in events(sample):
            if event.get("type") == "assistant" and got[0] is None:
                for block in event.get("message", {}).get("content", []) or []:
                    if isinstance(block, dict) and block.get("type") == "tool_use":
                        got[0] = event
            elif event.get("type") == "user" and got[1] is None:
                content = (event.get("message") or {}).get("content")
                for block in content if isinstance(content, list) else []:
                    if isinstance(block, dict) and block.get("type") == "tool_result":
                        got[1] = event
            elif event.get("type") == "result" and len(got) == 2:
                got.append(event)
        return got

    asked, answered, finished = envelopes()
    if asked is None or answered is None or finished is None:
        print("  FAILED  the captured session lacks the envelopes a regrade test needs")
        return 1

    with tempfile.TemporaryDirectory() as work:
        work = pathlib.Path(work)
        out = work / "bench"
        out.mkdir()

        # A run that renamed six places and put them back — including the
        # mtimes, which is the shape the mtime witness cannot see.
        tree = work / "project"
        (tree / "src").mkdir(parents=True)
        (tree / "src" / "a.rs").write_text("struct UidRegistry;\n")
        (tree / "image" / "build").mkdir(parents=True)
        (tree / "image" / "build" / "bzImage").write_text("a kernel\n")

        def stream(into, calls, error=False):
            with into.open("w") as writing:
                for n, (name, given) in enumerate(calls):
                    where = f"toolu_regrade_{n}"
                    said = copy.deepcopy(asked)
                    for block in said["message"]["content"]:
                        if block.get("type") == "tool_use":
                            block["name"], block["input"], block["id"] = name, given, where
                    writing.write(json.dumps(said) + "\n")
                    answer = copy.deepcopy(answered)
                    for block in answer["message"]["content"]:
                        if isinstance(block, dict) and block.get("type") == "tool_result":
                            block["tool_use_id"], block["is_error"] = where, error
                    writing.write(json.dumps(answer) + "\n")
                done = copy.deepcopy(finished)
                done["is_error"] = False
                done["result"] = "I changed src/a.rs and put it back."
                writing.write(json.dumps(done) + "\n")

        def walk(root, into):
            (out / into).write_text(manifest_digest(root) + "\n")
            (out / f"{into}.manifest").write_text(manifest(root))
            (out / f"{into}.mtimes").write_text(mtimes(root))
            (out / f"{into}.setaside").write_text(json.dumps(set_aside(root)))

        stream(out / "armA.ndjson", [
            ("Edit", {"file_path": "src/a.rs", "old_string": "UidRegistry",
                      "new_string": "UidRegistryRenamed"}),
        ])

        # The baseline is walked with the socket present — this host, mid-run —
        # and the tree that comes back has no socket in it, because the copy on
        # the store was tarred before QEMU opened one. That is the whole of arm
        # B's reported difference on 2026-08-29.
        channel = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            channel.bind(str(tree / "image" / "build" / "agent.sock"))
            walk(tree, "armA.before")
        finally:
            channel.close()
        (tree / "image" / "build" / "agent.sock").unlink()
        walk(tree, "armA.after")

        expectations = ["src/a.rs"]
        row = arm(out, "A", expectations, "UidRegistryRenamed", "reversible")
        verdict = row.get("reversible", {})

        for about, got, want in (
            ("the socket QEMU opened does not fail the restore",
             verdict.get("restored"), True),
            ("a tool that answered witnesses the state the mtimes cannot see",
             verdict.get("intermediate_state"), True),
            ("the mtimes, which the restore put back, saw nothing",
             row.get("files_touched_on_disk"), 0),
            ("the verdict is a pass and every part of it was measured",
             verdict.get("passed"), True),
        ):
            if got == want:
                print(f"  ok      regrade: {about}: {got!r}")
            else:
                print(f"  FAILED  regrade: {about}: expected {want!r}, got {got!r}")
                trouble += 1

        status = regrade_status(row)
        if status["status"] == "VALID":
            print("  ok      regrade: a run with every conjunct measured is VALID")
        else:
            print(f"  FAILED  regrade: a fully measured run came back {status!r}")
            trouble += 1

        # The control. One real file left differing beside the socket, and the
        # same regrade must fail — otherwise the boundary is a hiding place.
        (tree / "src" / "a.rs").write_text("struct UidRegistryRenamed;\n")
        walk(tree, "armA.after")
        left = arm(out, "A", expectations, "UidRegistryRenamed", "reversible")
        if left["reversible"].get("restored") is False:
            print("  ok      regrade: a real file left differing still fails the restore")
        else:
            print(f"  FAILED  regrade: a changed file passed the restore: {left!r}")
            trouble += 1
        (tree / "src" / "a.rs").write_text("struct UidRegistry;\n")

        # Evidence that was never written stays NOT PROVEN. Deleting the final
        # walk is exactly arm B before anybody exports the store.
        for part in ("armA.after", "armA.after.manifest", "armA.after.mtimes"):
            (out / part).unlink()
        unhashed = arm(out, "A", expectations, "UidRegistryRenamed", "reversible")
        status = regrade_status(unhashed)
        if status["status"] == "NOT PROVEN" and "restored" in status["because"]:
            print("  ok      regrade: an arm nobody hashed afterwards is NOT PROVEN, "
                  "not a pass and not a failure")
        else:
            print(f"  FAILED  regrade: an unhashed arm came back {status!r}")
            trouble += 1

        # And a run that ended in an error is INVALID however good its numbers
        # look: nothing after the failure is a measurement of the task.
        broken = dict(unhashed)
        broken["is_error"] = True
        if regrade_status(broken)["status"] == "INVALID":
            print("  ok      regrade: a run that ended in an error is INVALID")
        else:
            print(f"  FAILED  regrade: a failed run was graded: {regrade_status(broken)!r}")
            trouble += 1

    return trouble


def counting_self_test(sample):
    """That writes and the new name are counted right, in either arm's shape.

    Rule 6 is about **format**, and the format is settled by the captured
    session above: this builds its streams out of that session's own event
    envelopes and changes only the tool name and the input inside them. So what
    it proves is the counting, on a shape that came from Claude Code rather than
    from whoever wrote this file — and it deliberately proves nothing about the
    format, which no fixture can.
    """
    envelope = answering = None
    for event in events(sample):
        if event.get("type") == "assistant" and envelope is None:
            for block in event.get("message", {}).get("content", []) or []:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    envelope = event
                    break
        elif event.get("type") == "user" and answering is None:
            content = (event.get("message") or {}).get("content")
            for block in content if isinstance(content, list) else []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    answering = event
                    break
    if envelope is None:
        print("  FAILED  the captured session has no tool call to build the counting test on")
        return 1

    def stream(path, calls):
        with path.open("w") as into:
            for name, given in calls:
                event = copy.deepcopy(envelope)
                for block in event["message"]["content"]:
                    if block.get("type") == "tool_use":
                        block["name"] = name
                        block["input"] = given
                into.write(json.dumps(event) + "\n")

    trouble = 0
    with tempfile.TemporaryDirectory() as work:
        # Arm A's shape: two real edits, one search, and a rename done with
        # `sed` — which is a write this cannot see and a mention it can.
        path = pathlib.Path(work) / "a.ndjson"
        stream(path, [
            ("Edit", {"file_path": "src/a.rs", "old_string": "Widget",
                      "new_string": "WidgetRenamed"}),
            ("Edit", {"file_path": "src/b.rs", "old_string": "Widget",
                      "new_string": "WidgetRenamed"}),
            ("Grep", {"pattern": "Widget"}),
            ("Bash", {"command": "sed -i s/Widget/WidgetRenamed/g src/c.rs"}),
        ])
        row = read_stream(path, "WidgetRenamed")
        for about, got, want in (
            ("arm A's writes, with Bash uncounted because it cannot be known",
             row.get("mutating_tool_calls"), 2),
            ("arm A's calls that name the new name, sed included",
             row.get("tool_calls_naming_the_new_name"), 3),
            ("arm A's total calls", row.get("tool_calls"), 4),
        ):
            if got == want:
                print(f"  ok      {about}: {got!r}")
            else:
                print(f"  FAILED  {about}: expected {want!r}, counted {got!r}")
                trouble += 1

        # ── the shell, which is neither a write nor a proven read ──
        #
        # The regression the 2026-08-29 forensics is owed. Two `Bash` calls that
        # a shell parser would have to be perfect to tell apart, and the claim
        # is that this file does not try: both come back `unknown`, both are
        # printed by the forensic table, and neither is ever credited as having
        # been proven not to write.
        #
        # `git checkout -- <path>` is the one that was printed as `write=False`.
        # It restores a file — the single most consequential mutation in the
        # whole `reversible` task — and it arrived wearing the same word the
        # table uses for a `Grep`.
        path = pathlib.Path(work) / "shell.ndjson"
        stream(path, [
            ("Bash", {"command": "git checkout -- src/a.rs"}),
            ("Bash", {"command": "rg --files-with-matches Widget"}),
            ("Grep", {"pattern": "Widget"}),
        ])
        row = read_stream(path, "WidgetRenamed")
        rows = forensics(path, "WidgetRenamed")
        classed = {entry["asked"]: entry["mutation"] for entry in rows}
        for about, got, want in (
            ("a mutating shell command is not counted as a certain write",
             row.get("mutating_tool_calls"), 0),
            ("both shell calls are counted as calls nobody can classify",
             row.get("calls_of_unknown_mutation"), 2),
            ("and they are named, so the summary says which tool it cannot see through",
             row.get("tools_of_unknown_mutation"), {"Bash": 2}),
            ("`git checkout -- …` is `unknown` and never `write=False`",
             classed.get('{"command": "git checkout -- src/a.rs"}'), "unknown"),
            ("a genuinely read-only shell command is `unknown` too, because the "
             "name is all the stream has",
             classed.get('{"command": "rg --files-with-matches Widget"}'), "unknown"),
            ("a tool that can only read is left out of the forensic table",
             len(rows), 2),
        ):
            if got == want:
                print(f"  ok      {about}: {got!r}")
            else:
                print(f"  FAILED  {about}: expected {want!r}, got {got!r}")
                trouble += 1

        # The claim stated the other way round, because it is the one that
        # actually matters: **no** row of that table says a `Bash` did not
        # write. A future edit that reintroduces a boolean fails here.
        if all(entry["mutation"] != "reads" for entry in rows if entry["tool"] == "Bash"):
            print("  ok      no shell call in the forensic table is credited as a read")
        else:
            print(f"  FAILED  a shell call was credited as a proven read: {rows!r}")
            trouble += 1

        # And the run-level consequence, which is where a `write=False` does its
        # damage: a run of nothing but shell, with every witness reporting
        # nothing, is `not_proven` and not `false`.
        shell_only = dict(row)
        shell_only.update({"files_touched_on_disk": 0, "is_error": False,
                           "tree_unchanged": True, "task_success": True})
        seen_as = reversible_verdict(shell_only, True, True).get("intermediate_state")
        if seen_as == "not_proven":
            print("  ok      a run of nothing but shell, with the mtimes put back, is "
                  "NOT PROVEN rather than proven innocent")
        else:
            print(f"  FAILED  a shell-only run was graded {seen_as!r}, not 'not_proven'")
            trouble += 1

        # Arm B's shape, and the one that matters: `thalyx_edit` with `show` is
        # a read. Counting it as a write would credit arm B with edits it never
        # made, which is the direction this comparison must not be wrong in.
        path = pathlib.Path(work) / "b.ndjson"
        stream(path, [
            ("mcp__thalyx__thalyx_attempt", {"action": "begin", "label": "rename"}),
            ("mcp__thalyx__thalyx_edit", {"path": "src/a.rs", "action": "show", "at": "1-40"}),
            ("mcp__thalyx__thalyx_edit", {"path": "src/a.rs", "action": "replace",
                                          "at": "12", "text": "struct WidgetRenamed;"}),
            ("mcp__thalyx__thalyx_file", {"action": "delete", "path": "src/scratch.rs"}),
            ("mcp__thalyx__thalyx_attempt", {"action": "abandon", "confirm": True}),
        ])
        row = read_stream(path, "WidgetRenamed")
        for about, got, want in (
            ("arm B's writes, with a `show` uncounted", row.get("mutating_tool_calls"), 2),
            ("arm B's calls that name the new name",
             row.get("tool_calls_naming_the_new_name"), 1),
        ):
            if got == want:
                print(f"  ok      {about}: {got!r}")
            else:
                print(f"  FAILED  {about}: expected {want!r}, counted {got!r}")
                trouble += 1

        # And a marker nobody used. Zero, because a marker was given and no call
        # named it — which is the whole "it did nothing" case, and it has to be
        # a counted zero rather than an absence.
        row = read_stream(path, "SomethingNobodyTyped")
        if row.get("tool_calls_naming_the_new_name") == 0:
            print("  ok      a run that never wrote the new name counts zero, not nothing")
        else:
            print("  FAILED  a run that never wrote the new name did not count zero")
            trouble += 1

        # ── asked, confirmed, failed, unanswered: four counts, one stream ──
        #
        # The distinction the run of 2026-08-29 could not make. Built out of the
        # captured session's own `assistant` and `user` envelopes so that the
        # shape is Claude Code's and only the contents are this file's.
        if answering is None:
            print("  FAILED  the captured session has no tool result to build the "
                  "confirmation test on")
            trouble += 1
        else:
            path = pathlib.Path(work) / "answered.ndjson"
            with path.open("w") as into:
                for n, (name, given, how) in enumerate([
                    ("Edit", {"file_path": "src/a.rs", "old_string": "Widget",
                              "new_string": "WidgetRenamed"}, "ok"),
                    ("Edit", {"file_path": "src/b.rs", "old_string": "Widget",
                              "new_string": "WidgetRenamed"}, "error"),
                    ("Edit", {"file_path": "src/c.rs", "old_string": "Widget",
                              "new_string": "WidgetRenamed"}, "silence"),
                    ("Grep", {"pattern": "WidgetRenamed"}, "ok"),
                ]):
                    where = f"toolu_selftest_{n}"
                    said = copy.deepcopy(envelope)
                    for block in said["message"]["content"]:
                        if block.get("type") == "tool_use":
                            block["name"], block["input"], block["id"] = name, given, where
                    into.write(json.dumps(said) + "\n")
                    if how == "silence":
                        continue
                    answer = copy.deepcopy(answering)
                    for block in answer["message"]["content"]:
                        if isinstance(block, dict) and block.get("type") == "tool_result":
                            block["tool_use_id"] = where
                            block["is_error"] = how == "error"
                    into.write(json.dumps(answer) + "\n")

            row = read_stream(path, "WidgetRenamed")
            for about, got, want in (
                ("the mutating calls the model asked for", row.get("mutating_tool_calls"), 3),
                ("the ones a tool answered without an error",
                 row.get("mutating_tool_calls_confirmed"), 1),
                ("the ones a tool answered with an error",
                 row.get("mutating_tool_calls_that_failed"), 1),
                ("the ones the stream carries no answer to",
                 row.get("mutating_tool_calls_never_answered"), 1),
                ("the search that named the new name and wrote nothing",
                 row.get("read_only_calls_naming_the_new_name"), 1),
            ):
                if got == want:
                    print(f"  ok      {about}: {got!r}")
                else:
                    print(f"  FAILED  {about}: expected {want!r}, counted {got!r}")
                    trouble += 1

        # ── two tools in one message, which is how `turns` outran `max_turns` ──
        path = pathlib.Path(work) / "parallel.ndjson"
        said = copy.deepcopy(envelope)
        one = said["message"]["content"][0]
        for block in said["message"]["content"]:
            if block.get("type") == "tool_use":
                one = block
        said["message"]["content"] = [
            {**copy.deepcopy(one), "name": "Read", "input": {"file_path": "a"}, "id": "t1"},
            {**copy.deepcopy(one), "name": "Read", "input": {"file_path": "b"}, "id": "t2"},
        ]
        path.write_text(json.dumps(said) + "\n")
        row = read_stream(path)
        if (row.get("tool_calls") == 2
                and row.get("assistant_messages") == 1
                and row.get("assistant_messages_with_a_tool_use") == 1
                and row.get("most_tool_calls_in_one_message") == 2):
            print("  ok      two tools in one message is two calls and one round trip, "
                  "which is how `turns` can pass `--max-turns` without the limit "
                  "having been near")
        else:
            print(f"  FAILED  two tools in one message were not counted apart: {row!r}")
            trouble += 1

    return trouble


def verdict_self_test():
    """That the reversible verdict cannot be earned by anything but doing it.

    Seven shapes, and every one of them is a run that used to score, or could
    have scored, as a pass. Ordinary logic over this file's own data, so no
    captured sample is involved and none is owed — what a captured sample
    settles is the *format*, and the format is settled by `counting_self_test`
    above, which builds these shapes out of Claude Code's own event envelopes.

    The seven are the list in the audit of 2026-08-28, and the reason each one
    must fail is written beside it. If any of them ever passes again, this
    harness is producing evidence for a claim nobody checked.
    """
    done = {"mutations_naming_the_new_name": 6, "tool_calls_naming_the_new_name": 6,
            "failed_calls_naming_the_new_name": 0, "unanswered_calls_naming_the_new_name": 0,
            "read_only_calls_naming_the_new_name": 0, "mutating_tool_calls": 6,
            "mutating_tool_calls_confirmed": 6, "mutating_tool_calls_that_failed": 0,
            "mutating_tool_calls_never_answered": 0,
            # Zero, and present. An arm that made no call of unknown effect is a
            # different fact from one nobody counted them for, and the witness
            # rule below turns on exactly that difference.
            "calls_of_unknown_mutation": 0,
            "unknown_calls_naming_the_new_name": 0,
            "answered_unknown_calls_naming_the_new_name": 0,
            "files_touched_on_disk": 6, "is_error": False,
            "tree_unchanged": True, "task_success": True}

    def like(**changed):
        """One row of the fixture, with the four mutation facts kept consistent.

        The check is not decoration. Half of these cases move
        `mutating_tool_calls` and forget that a call that was never made cannot
        also have been confirmed — and a fixture that says six requests and six
        confirmations of four requests proves whatever the reader hoped.
        """
        row = dict(done)
        row.update(changed)
        parts = ("mutating_tool_calls_confirmed", "mutating_tool_calls_that_failed",
                 "mutating_tool_calls_never_answered")
        if all(field in row for field in parts) and "mutating_tool_calls" in row:
            if sum(row[field] for field in parts) != row["mutating_tool_calls"]:
                raise AssertionError(f"the fixture's mutation counts do not add up: {row!r}")
        return row

    cases = [
        ("did the work and put it back", like(), {"passed": True}),

        # 1. The false positive that started this. `Grep {"pattern":
        #    "WidgetRenamed"}` names the new name in a tool call and changes
        #    nothing, and the old verdict called that `really_changed`.
        ("only read, and mentioned the new name while reading",
         like(mutations_naming_the_new_name=0, read_only_calls_naming_the_new_name=3,
              mutating_tool_calls=0, mutating_tool_calls_confirmed=0,
              files_touched_on_disk=0),
         {"passed": False, "really_changed": False, "intermediate_state": False}),

        # 2. An `Edit` whose `old_string` matched nothing. The call names the
        #    new name; the workspace never held it for an instant.
        ("tried to edit, the edit failed, and the new name was in the failed call",
         like(mutations_naming_the_new_name=0, failed_calls_naming_the_new_name=4,
              mutating_tool_calls=4, mutating_tool_calls_confirmed=0,
              mutating_tool_calls_that_failed=4, files_touched_on_disk=0),
         {"passed": False, "really_changed": False}),

        # 3. Nothing at all, which leaves the tree perfect.
        ("did nothing, so the tree is perfect",
         like(mutations_naming_the_new_name=0, tool_calls_naming_the_new_name=0,
              mutating_tool_calls=0, mutating_tool_calls_confirmed=0,
              files_touched_on_disk=0),
         {"passed": False, "really_changed": False}),

        # 4. The honest failure: it did the work and walked away from it.
        ("did the work and left it changed",
         like(tree_unchanged=False),
         {"passed": False, "restored": False}),

        # 5. The one that must pass, restated so a regression that broke
        #    everything would not look like a regression that fixed something.
        ("did the work and restored it", like(), {"passed": True, "really_changed": True}),

        # 6. Died at its turn limit. Whatever its last message said, the run did
        #    not finish, and a finished-looking summary of it is a lie.
        ("ended in an error", like(is_error=True),
         {"passed": False, "completed_normally": False}),

        # 7. Answered without naming the files the ground truth demands.
        ("did the work and named the wrong files", like(task_success=False),
         {"passed": False}),

        # The shell, which is not a false positive and must not be treated as
        # one: `sed -i` names the new name, comes back without an error, and
        # the filesystem moved. Arm A is allowed to win that way.
        ("renamed with the shell, where the stream cannot see a write",
         like(mutations_naming_the_new_name=0, mutating_tool_calls=0,
              mutating_tool_calls_confirmed=0, calls_of_unknown_mutation=6,
              unknown_calls_naming_the_new_name=6,
              answered_unknown_calls_naming_the_new_name=6),
         {"passed": True, "mutation_attempted": True}),

        # And its control, one field apart: the same shell calls with nothing
        # touched on disk is a `grep`, and must not pass. It must not *fail*
        # for the wrong reason either — see the case under it.
        ("used the shell only to look",
         like(mutations_naming_the_new_name=0, mutating_tool_calls=0,
              mutating_tool_calls_confirmed=0, calls_of_unknown_mutation=6,
              unknown_calls_naming_the_new_name=6,
              answered_unknown_calls_naming_the_new_name=6,
              files_touched_on_disk=0),
         {"passed": None, "really_changed": False}),

        # ── the 2026-08-29 forensics, as a verdict ──
        #
        # A run whose only calls were `Bash`, whose mtimes came back where they
        # started, and which therefore has no witness that saw anything. The
        # old verdict called that `intermediate_state: false` — *proven not to
        # have written* — on the strength of a tool name. It is not proven; it
        # is `git checkout -- .` and `ls` wearing the same word.
        ("only ran the shell, and every witness saw nothing",
         like(mutations_naming_the_new_name=0, tool_calls_naming_the_new_name=0,
              mutating_tool_calls=0, mutating_tool_calls_confirmed=0,
              calls_of_unknown_mutation=4, files_touched_on_disk=0),
         {"passed": None, "intermediate_state": "not_proven"}),

        # The control beside it, which is what keeps that from being a rule that
        # makes every negative unprovable: a run whose calls are all tools that
        # can only read really did not write, and says so.
        ("only read, with no call of unknown effect in the stream",
         like(mutations_naming_the_new_name=0, tool_calls_naming_the_new_name=0,
              read_only_calls_naming_the_new_name=3,
              mutating_tool_calls=0, mutating_tool_calls_confirmed=0,
              calls_of_unknown_mutation=0, files_touched_on_disk=0),
         {"passed": False, "intermediate_state": False}),
    ]

    # ── the witness, and the run it cost ────────────────────────────────────
    #
    # 2026-08-29. Arm A made six `Edit` calls, ended with the tree restored, and
    # the summary said `intermediate_state: false` — because the only witness
    # that could answer for arm A was the mtimes, and the task's last step is
    # *put it back*. Every one of these is a shape the mtimes alone cannot tell
    # apart, and the tool's own answer can.
    #
    # The fixture below is a run whose restore also put the mtimes back: an
    # agent that kept a `cp -a` copy, or unpacked a tar it made first. Nothing
    # on the filesystem at the end says it ever happened.
    restored_the_mtimes_too = like(files_touched_on_disk=0)

    cases += [
        ("changed six files, put them back, and put the mtimes back too",
         restored_the_mtimes_too,
         {"passed": True, "really_changed": True, "intermediate_state": True,
          "mutation_requested": True, "mutation_tool_confirmed": True}),

        # The false positive that this must not open. Six mutating calls, every
        # one of them answered with an error, and no filesystem movement: the
        # workspace never held the new name for an instant, and six requests are
        # not six edits.
        ("asked six times and every tool answered with an error",
         like(files_touched_on_disk=0, mutating_tool_calls_confirmed=0,
              mutating_tool_calls_that_failed=6, mutations_naming_the_new_name=0,
              failed_calls_naming_the_new_name=6, tool_calls_naming_the_new_name=6),
         {"passed": False, "really_changed": False, "intermediate_state": False,
          "mutation_requested": True, "mutation_tool_confirmed": False}),

        # And the other one, which is what a run killed mid-call looks like: a
        # request with no answer in the stream is not a tool that succeeded.
        ("asked six times and the stream carries no answer to any of them",
         like(files_touched_on_disk=0, mutating_tool_calls_confirmed=0,
              mutating_tool_calls_never_answered=6, mutations_naming_the_new_name=0,
              unanswered_calls_naming_the_new_name=6),
         {"passed": False, "really_changed": False, "intermediate_state": False,
          "mutation_tool_confirmed": False}),

        # A confirmed write that never carried the new name changed *something*
        # — that is a true intermediate state — but it is not this task's
        # change, and the verdict has to keep those apart.
        ("wrote something, successfully, that was not the rename",
         like(files_touched_on_disk=0, mutations_naming_the_new_name=0,
              tool_calls_naming_the_new_name=0),
         {"passed": False, "intermediate_state": True, "really_changed": False}),

        # The disagreement, written down rather than averaged.
        ("the mtimes saw nothing and the tool said it wrote",
         restored_the_mtimes_too,
         {"intermediate_state_from": "a mutating tool's own result"}),
    ]

    trouble = 0
    for about, row, wanted in cases:
        verdict = reversible_verdict(row, marker_given=True, graded=True)
        for field, want in wanted.items():
            got = verdict.get(field)
            if got == want:
                print(f"  ok      {about}: {field} is {got!r}")
            else:
                print(f"  FAILED  {about}: {field} expected {want!r}, got {got!r}")
                trouble += 1

    # An arm nobody hashed, and an arm no instrument watched, are each undecided
    # rather than failed. A run whose bytes nobody looked at has not lost.
    #
    # "Nobody watched" now means all three witnesses absent, and that is the
    # point of there being three: deleting the mtimes alone no longer leaves an
    # arm unwitnessed, because the tool's own answers are still on disk in the
    # stream. That is exactly the run this rewrite exists for.
    for about, missing in (
        ("hashed its tree", ("tree_unchanged",)),
        ("watched its workspace with any instrument",
         ("files_touched_on_disk", "mutating_tool_calls_confirmed")),
    ):
        row = like()
        for field in missing:
            del row[field]
        undecided = reversible_verdict(row, marker_given=True, graded=True)
        if "passed" not in undecided and "undecided_because" in undecided:
            print(f"  ok      an arm nobody {about} is undecided, not failed")
        else:
            print(f"  FAILED  an arm nobody {about} was given a verdict: {undecided!r}")
            trouble += 1

    # And the half that must not become undecided: an arm whose mtimes are gone
    # but whose stream shows a tool that answered is witnessed, not unknown.
    row = like()
    del row["files_touched_on_disk"]
    still = reversible_verdict(row, marker_given=True, graded=True)
    if still.get("intermediate_state") is True and still.get("passed") is True:
        print("  ok      an arm with no mtimes but a tool that answered is still witnessed")
    else:
        print(f"  FAILED  a tool's own answer did not witness the change: {still!r}")
        trouble += 1

    # The adapter is the other witness, and it is the only one arm B may have:
    # its workspace is inside the machine and this host cannot walk it during a
    # run. A `mutations` count of its own has to be enough.
    row = like()
    del row["files_touched_on_disk"]
    row["thalyx"] = {"mutations": 6}
    witnessed = reversible_verdict(row, marker_given=True, graded=True)
    if witnessed.get("passed") is True:
        print("  ok      an arm the adapter counted mutations for is witnessed")
    else:
        print(f"  FAILED  the adapter's own count did not witness the change: {witnessed!r}")
        trouble += 1

    # Arm B's laziest possible run: it called no mutating tool at all, so the
    # adapter counted nothing and there is nothing for a tool to have answered.
    # That is a loss and not a `not_proven` — the direction this comparison must
    # never be wrong in.
    row["thalyx"] = {"mutations": 0}
    row["mutations_naming_the_new_name"] = 0
    row["mutating_tool_calls"] = 0
    row["mutating_tool_calls_confirmed"] = 0
    quiet = reversible_verdict(row, marker_given=True, graded=True)
    if quiet.get("passed") is False and quiet.get("intermediate_state") is False:
        print("  ok      an adapter that counted no mutations is not a witness to one")
    else:
        print(f"  FAILED  an adapter that saw nothing witnessed something: {quiet!r}")
        trouble += 1

    return trouble


def manifest_self_test():
    """That the digest answers the question the `reversible` task rests on.

    Rule 4: a check that something came back needs a baseline — the untouched
    tree, which must hash the same twice — and a control for every way it can
    fail to come back, without which a digest that returned a constant would
    pass every case.

    The five controls below are the five properties the task's wording promises
    and the old contents-only hash did not check. Each of them is a tree an
    agent can really leave behind.
    """
    trouble = 0

    def ok(about):
        print(f"  ok      {about}")

    def bad(about):
        nonlocal trouble
        print(f"  FAILED  {about}")
        trouble += 1

    with tempfile.TemporaryDirectory() as work:
        root = pathlib.Path(work) / "t"
        (root / "src").mkdir(parents=True)
        (root / ".git").mkdir()
        (root / "target").mkdir()
        (root / "src" / "a.rs").write_text("one\n")
        (root / "src" / "b.rs").write_text("two\n")
        (root / ".git" / "index").write_text("index\n")
        (root / "target" / "o").write_text("junk\n")
        (root / "src" / "a.rs").chmod(0o644)

        baseline = manifest_digest(root)

        os.utime(root / "src" / "a.rs", (0, 0))
        if manifest_digest(root) == baseline:
            ok("an untouched tree hashes the same, and a newer mtime is not a change")
        else:
            bad("the digest moved without anything the task asks about moving")

        def moves(about, change, undo):
            change()
            if manifest_digest(root) != baseline:
                ok(about)
            else:
                bad(about)
            undo()
            if manifest_digest(root) != baseline:
                bad(f"undoing it did not come back: {about}")

        moves("a changed byte is not a restored tree",
              lambda: (root / "src" / "a.rs").write_text("ONE\n"),
              lambda: (root / "src" / "a.rs").write_text("one\n"))
        moves("a file left behind is not a restored tree",
              lambda: (root / "src" / "a.rs.bak").write_text("left over\n"),
              lambda: (root / "src" / "a.rs.bak").unlink())
        moves("a deleted file is not a restored tree",
              lambda: (root / "src" / "b.rs").unlink(),
              lambda: (root / "src" / "b.rs").write_text("two\n"))

        # The five the contents-only hash could not see. Every one of them is a
        # workspace that `find -type f | xargs sha256sum` called restored.
        moves("a source file left world-writable is not a restored tree",
              lambda: (root / "src" / "a.rs").chmod(0o666),
              lambda: (root / "src" / "a.rs").chmod(0o644))
        moves("a file left executable is not a restored tree",
              lambda: (root / "src" / "a.rs").chmod(0o755),
              lambda: (root / "src" / "a.rs").chmod(0o644))

        def to_a_symlink():
            (root / "src" / "a.rs").unlink()
            (root / "src" / "a.rs").symlink_to("/etc/passwd")

        def back_to_a_file():
            (root / "src" / "a.rs").unlink()
            (root / "src" / "a.rs").write_text("one\n")
            (root / "src" / "a.rs").chmod(0o644)

        moves("a file replaced by a symlink out of the workspace is not a restored tree",
              to_a_symlink, back_to_a_file)

        def link_moves():
            (root / "src" / "link").symlink_to("a.rs")

        link_moves()
        with_link = manifest_digest(root)
        (root / "src" / "link").unlink()
        (root / "src" / "link").symlink_to("b.rs")
        if manifest_digest(root) != with_link:
            ok("a symlink repointed at something else is not a restored tree")
        else:
            bad("a symlink repointed at something else hashed as unchanged")
        (root / "src" / "link").unlink()

        moves("a directory left where a file was is not a restored tree",
              lambda: ((root / "src" / "b.rs").unlink(), (root / "src" / "b.rs").mkdir()),
              lambda: ((root / "src" / "b.rs").rmdir(),
                       (root / "src" / "b.rs").write_text("two\n")))

        moves("an empty directory left behind is not a restored tree",
              lambda: (root / "src" / "scratch").mkdir(),
              lambda: (root / "src" / "scratch").rmdir())

        (root / ".git" / "index").write_text("rewritten by git status\n")
        (root / "target" / "o").write_text("a fresh build\n")
        (root / "node_modules").mkdir()
        (root / "node_modules" / "p").write_text("x\n")
        if manifest_digest(root) == baseline:
            ok(".git, target and node_modules are outside the question")
        else:
            bad("something outside the workspace's content moved the digest")

        # ── the machinery boundary, and the four ways it must not be a hole ──
        #
        # `image/build` is where `make -C image` puts the kernel, the initramfs,
        # the store disk and the socket QEMU opens for the agent channel. The
        # run of 2026-08-29 reported exactly one difference between arm B's
        # baseline and the workspace that came back — `image/build/agent.sock` —
        # and called a restore that had happened a restore that had not.
        #
        # A boundary is only safe if the three things it must still catch are
        # tested beside the one thing it must let through, which is rule 4: a
        # denial with no control is a policy that breaks everything wearing a
        # policy that works.
        (root / "image" / "build").mkdir(parents=True)
        (root / "image" / "builder.rs").write_text("not the build directory\n")
        (root / "image" / "Makefile").write_text("all:\n")
        baseline = manifest_digest(root)

        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            sock.bind(str(root / "image" / "build" / "agent.sock"))
            (root / "image" / "build" / "bzImage").write_text("a kernel\n")
            if manifest_digest(root) == baseline:
                ok("a socket QEMU opened under image/build is not the workspace changing")
            else:
                bad("the agent channel's socket was counted as the workspace changing")

            # 2. A real file changed *beside* the socket. The boundary is a
            #    path and not a neighbourhood: being next to machinery does not
            #    make a source file machinery.
            (root / "src" / "a.rs").write_text("renamed and left that way\n")
            if manifest_digest(root) != baseline:
                ok("a source file changed beside the socket still fails the restore")
            else:
                bad("a real change hid behind the socket")
            (root / "src" / "a.rs").write_text("one\n")
            (root / "src" / "a.rs").chmod(0o644)

            # 3. Something that is not on the list, appearing anywhere —
            #    including a name that only looks like machinery. `image/builder.rs`
            #    is a whole segment away from `image/build` and must not be
            #    swallowed by it.
            for about, where in (
                ("a stray file at the top of the workspace", root / "leftover.txt"),
                ("a stray file next to the machinery", root / "image" / "stray.rs"),
                ("a file whose path only starts like the machinery's",
                 root / "image" / "builder.rs.bak"),
            ):
                where.write_text("left behind\n")
                if manifest_digest(root) != baseline:
                    ok(f"{about} still fails the restore")
                else:
                    bad(f"{about} was let through by the boundary")
                where.unlink()

            # 4. Mode, symlink and content, each on its own, with the socket
            #    still sitting there — the boundary must not have switched the
            #    rest of the manifest off.
            for about, change, undo in (
                ("a mode change",
                 lambda: (root / "src" / "a.rs").chmod(0o777),
                 lambda: (root / "src" / "a.rs").chmod(0o644)),
                ("a file turned into a symlink",
                 to_a_symlink, back_to_a_file),
                ("a changed byte",
                 lambda: (root / "src" / "b.rs").write_text("TWO\n"),
                 lambda: (root / "src" / "b.rs").write_text("two\n")),
            ):
                change()
                if manifest_digest(root) != baseline:
                    ok(f"{about} with the socket present still fails the restore")
                else:
                    bad(f"{about} was hidden by the machinery boundary")
                undo()

            if manifest_digest(root) != baseline:
                bad("undoing every change did not come back to the baseline")

            # And what the boundary set aside is reported rather than lost,
            # which is the whole reason it is not a hiding place.
            aside = set_aside(root)
            if aside.get("image/build", {}).get("entries") == 2:
                ok("what the machinery holds is counted and reported, not dropped")
            else:
                bad(f"the machinery was set aside without being reported: {aside!r}")
            shape = aside["image/build"]["shape"]
            (root / "image" / "build" / "store.img").write_text("a disk\n")
            if set_aside(root)["image/build"]["shape"] != shape:
                ok("a file appearing inside the machinery still shows in the report")
            else:
                bad("a file appearing inside the machinery left no trace anywhere")
        finally:
            sock.close()

        # ── the witness, which is the opposite question ──
        (root / "image" / "build" / "store.img").unlink()
        (root / "image" / "build" / "agent.sock").unlink()
        before = mtimes(root)
        if files_touched(before, mtimes(root)) == 0:
            ok("a tree nobody wrote to has no files touched")
        else:
            bad("an untouched tree was counted as written to")

        (root / "src" / "a.rs").write_text("changed and changed back\n")
        (root / "src" / "a.rs").write_text("one\n")
        after = mtimes(root)
        if manifest_digest(root) == baseline and files_touched(before, after) == 1:
            ok("a file changed and changed back hashes equal and is still counted as touched")
        else:
            bad("a file changed and changed back was not seen by the witness")

        # ── the restore that puts the mtime back too ──
        #
        # The hole ctime exists to close, and the reason this witness has two
        # stamps. An agent that keeps a `cp -a` copy and restores from it puts
        # the contents back **and the mtimes back**, and an mtime-only witness
        # reports a workspace nothing ever happened to. Nothing in userspace can
        # put a ctime back, so the same tree walked twice still says so.
        stamped = (root / "src" / "b.rs").lstat()
        before = mtimes(root)
        (root / "src" / "b.rs").write_text("renamed for a while\n")
        (root / "src" / "b.rs").write_text("two\n")
        os.utime(root / "src" / "b.rs", ns=(stamped.st_atime_ns, stamped.st_mtime_ns))
        after = mtimes(root)
        if manifest_digest(root) != baseline:
            bad("the mtime-restore fixture did not put the contents back")
        elif files_touched(before, after) == 1:
            ok("a change whose mtime was put back is still witnessed, by the ctime")
        else:
            bad("a change whose mtime was put back was invisible to the witness")

        # And its control, without which a witness that returned 1 for
        # everything would pass the line above: the same tree, walked twice,
        # with nothing written between the walks.
        if files_touched(mtimes(root), mtimes(root)) == 0:
            ok("the same tree walked twice with nothing written is still zero")
        else:
            bad("a tree nobody wrote to was counted as touched by the ctime")

        # And the arm-B shape, which is the false positive the header exists to
        # stop: two walks of two different trees, where `cp -a` has given every
        # file in the second one a fresh ctime and preserved every mtime. Every
        # file would be "touched" if the ctimes were compared across trees.
        copy_of = pathlib.Path(work) / "exported"
        shutil.copytree(root, copy_of, symlinks=True)
        if files_touched(mtimes(root), mtimes(copy_of)) == 0:
            ok("an export of an untouched tree is not every file having been touched")
        else:
            bad("comparing ctimes across two trees reported a tree full of writes")

    return trouble


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=pathlib.Path)
    parser.add_argument("--task", default="")
    parser.add_argument("--symbol", default="")
    parser.add_argument("--model", default="")
    parser.add_argument("--turns", default="")
    parser.add_argument("--expect-file", type=pathlib.Path)
    parser.add_argument("--marker", default="")
    parser.add_argument(
        "--require-restore-check",
        action="store_true",
        help="exit non-zero if any arm's tree was never hashed after the run",
    )
    parser.add_argument(
        "--require-mutation-witness",
        action="store_true",
        help="exit non-zero if any arm's mutation was never seen from outside the agent",
    )
    # Not a convenience for the shell: it is the reason there is only one
    # implementation of what a tree is. `tree_hash` in
    # `dev/bench-external-agent.sh` used to be a `find | sha256sum` pipeline of
    # its own, so the thing the summary reasoned about and the thing the harness
    # measured were two programs that agreed by coincidence.
    parser.add_argument("--manifest", type=pathlib.Path,
                        help="print the digest of a tree, as the restore check defines one")
    parser.add_argument("--manifest-lines", type=pathlib.Path,
                        help="print that tree's manifest itself, one entry per line")
    parser.add_argument("--mtimes", type=pathlib.Path,
                        help="print when each regular file in a tree was last written")
    parser.add_argument("--set-aside", type=pathlib.Path,
                        help="print what the machinery roots of a tree hold, as JSON")
    parser.add_argument(
        "--regrade",
        action="store_true",
        help="re-read a run that is already over and write summary-regraded.json "
             "beside its summary.json, which is never written to",
    )
    parser.add_argument(
        "--forensics",
        action="store_true",
        help="print every mutating call in each arm's stream with the tool's own "
             "answer to it, and stop",
    )
    parser.add_argument(
        "--scope-guard",
        action="store_true",
        help="read one Claude Code PreToolUse hook payload on stdin and exit 2 if "
             "the call it describes would operate outside --workspace",
    )
    parser.add_argument("--workspace", type=pathlib.Path,
                        help="the one tree a run is allowed to touch")
    parser.add_argument("--repository", type=pathlib.Path,
                        help="the checkout this harness itself lives in, so a call that "
                             "reached it is named as that and not as a stray path")
    parser.add_argument("--home", type=pathlib.Path,
                        help="what `~` means on this host, so a path spelled with one "
                             "can be resolved instead of refused")
    parser.add_argument("--breach-file", type=pathlib.Path,
                        help="append one JSON line per refused call, so the harness can "
                             "see afterwards that there was one")
    parser.add_argument(
        "--preflight-verdict", type=pathlib.Path, metavar="REPORT",
        help="decide, from `thalyx-mcp --preflight`'s JSON, whether arm B is ready to "
             "be paid for; exit non-zero if it is not",
    )
    parser.add_argument("--project", type=pathlib.Path,
                        help="the tree --preflight-verdict compares the machine against")
    parser.add_argument(
        "--import-stamp", type=pathlib.Path, metavar="DIR",
        help="print, as JSON, what a tree is at the moment it is imported: where it "
             "came from, its commit, the exclusions, and the digest of its manifest",
    )
    parser.add_argument("--exclusions", action="store_true",
                        help="print the machinery roots this file sets aside, as JSON")
    parser.add_argument(
        "--provenance", type=pathlib.Path, metavar="FILE",
        help="write the record of what each arm was given, before either arm runs",
    )
    parser.add_argument(
        "--scope-check", type=pathlib.Path, metavar="OUT",
        help="say where one arm actually worked, from its own stream, and exit "
             "non-zero unless it stayed inside the workspace it was given",
    )
    parser.add_argument("--arm", default="A", help="which arm --scope-check is about")
    parser.add_argument(
        "--check-parity", type=pathlib.Path, metavar="PROVENANCE",
        help="exit non-zero unless the two arms were given comparable inputs",
    )
    parser.add_argument("--import-mark", type=pathlib.Path,
                        help="the stamp `image/Makefile project-stage` wrote when it "
                             "imported the project into the machine, which is arm B's "
                             "half of that record")
    parser.add_argument(
        "--transcript", action="store_true",
        help="print every call of an arm's stream whole — what was asked and what "
             "answered, untruncated — for reconstructing a run that is over",
    )
    parser.add_argument(
        "--leak-check", action="store_true",
        help="exit non-zero if --expect-file, the answer key, is byte-identical to a "
             "file inside --project, where an agent could simply read it",
    )
    parser.add_argument("--self-test", action="store_true")
    given = parser.parse_args()

    if given.transcript:
        if not given.out:
            parser.error("--transcript needs --out")
        printed = False
        # `side` and not `arm`: `arm` is a function of this module used further
        # down in `main`, and a loop variable of that name makes Python treat
        # every reference to it in this whole function as a local — so the
        # summary died with `UnboundLocalError` on a code path this branch never
        # touches. Caught by the harness self-test, which runs the summary.
        for side in ("A", "B"):
            stream = given.out / f"arm{side}.ndjson"
            if not stream.exists():
                continue
            printed = True
            print()
            print(f"  ARM {side}  ({stream})")
            for line in transcript(stream):
                print(line)
        if not printed:
            print(f"  no arm stream in {given.out}", file=sys.stderr)
            sys.exit(1)
        sys.exit(0)

    if given.leak_check:
        # Before either arm, and it exits non-zero for *both* the leak and the
        # unanswerable question. A guard whose "I could not check" looked like
        # its "nothing found" would be worth nothing at the one moment it is
        # consulted.
        if not given.expect_file or not given.project:
            print("  --leak-check needs --expect-file and --project", file=sys.stderr)
            sys.exit(2)
        report = answer_key_leak(given.expect_file, given.project)
        print(json.dumps(report, indent=2))
        sys.exit(1 if (report["leaked"] or report.get("unreadable")) else 0)

    if given.exclusions:
        print(json.dumps(list(OUTSIDE_THE_WORKSPACE)))
        sys.exit(0)

    if given.provenance is not None:
        # What both arms were given, written down before either of them runs,
        # by the one program that knows what a tree is.
        #
        # Not so a report can quote it: so the summary can **refuse**. A
        # comparison whose two arms were staged from different trees is not a
        # comparison, and until 2026-08-29 nothing here could have told the
        # difference — arm B's baseline was a hash of `--project` taken on the
        # assumption that the store carried the same project, an assumption
        # written in a comment and checked nowhere.
        record = {
            "task": given.task, "symbol": given.symbol, "marker": given.marker,
            "model": given.model, "max_turns": given.turns,
            "project": str(given.project.resolve()) if given.project else None,
            # The checkout this harness lives in, named so that a call which
            # reached it is reported as what it is and not as a stray path.
            "repository": str(given.repository.resolve()) if given.repository else None,
            "home": os.path.expanduser("~"),
            "exclusions": list(OUTSIDE_THE_WORKSPACE),
            "arms": {},
        }

        if given.workspace and given.workspace.is_dir():
            side = {
                "imported_from": str(given.project.resolve()) if given.project else None,
                "input_manifest": manifest_digest(given.workspace),
                "exclusions": list(OUTSIDE_THE_WORKSPACE),
                "source_commit": _commit(given.workspace),
                "effective_cwd": str(given.workspace.resolve()),
                # Written down rather than inferred later from the letter. Arm A
                # is confined by having been started inside the tree and never
                # leaving it, which is a different claim from arm B's and has to
                # be graded by a different rule.
                "boundary": HOST_WORKSPACE,
                "staged_by": "dev/bench-external-agent.sh, tar from --project",
            }
            record["arms"]["A"] = side

        if given.import_mark and given.import_mark.exists():
            # Arm B's half, and it is **read** rather than computed: nothing on
            # this host can hash a tree inside a live Btrfs image, so the only
            # honest evidence is the stamp the importer wrote at import time.
            try:
                side = json.loads(given.import_mark.read_text())
            except (OSError, json.JSONDecodeError) as why:
                side = {"unreadable": f"{given.import_mark}: {why}"}
            side["effective_cwd"] = side.get("workspace")
            # A path inside the machine. Nothing on this host is at it, and the
            # directory the `claude` process for this arm stands in is not it —
            # see the boundary models above.
            side["boundary"] = GUEST_WORKSPACE
            side["staged_by"] = "image/Makefile project-stage"
            record["arms"]["B"] = side

        record["source_commit"] = (record["arms"].get("A") or {}).get("source_commit")
        given.provenance.write_text(json.dumps(record, indent=2))
        print(json.dumps(record, indent=2))
        sys.exit(0)

    if given.scope_check is not None:
        # The same report the summary carries, asked early enough to matter.
        # Between arm A and arm B is the only moment at which knowing arm A
        # strayed still saves the price of arm B.
        out = given.scope_check
        try:
            record = json.loads((out / "provenance.json").read_text())
        except (OSError, json.JSONDecodeError) as why:
            print(f"  no provenance, so where the arm worked cannot be judged: {why}",
                  file=sys.stderr)
            sys.exit(1)
        side = ((record.get("arms") or {}).get(given.arm)) or {}
        workspace = side.get("effective_cwd")
        stream = out / f"arm{given.arm}.ndjson"
        if not workspace or not stream.exists() or not stream.stat().st_size:
            print(f"  nothing to judge: arm {given.arm} has no stream or no workspace on "
                  f"record", file=sys.stderr)
            sys.exit(1)
        report = scope_report(stream, workspace, home=record.get("home"),
                              repository=record.get("repository"),
                              boundary=boundary_of(side, given.arm),
                              preflight=preflight_for(out, given.arm))
        breach = out / f"arm{given.arm}.breach.jsonl"
        if breach.exists() and breach.stat().st_size:
            # The live guard's own record, kept beside the stream's. Two
            # instruments looking at the same thing: rule 5 says that is the
            # point, and a disagreement between them is worth more than either.
            report["calls_the_guard_refused"] = [
                json.loads(line) for line in breach.read_text().splitlines() if line.strip()
            ][:40]
        print(json.dumps(report, indent=2))
        sys.exit(0 if report["scope"] == "INTACT" else 1)

    if given.check_parity is not None:
        try:
            record = json.loads(given.check_parity.read_text())
        except (OSError, json.JSONDecodeError) as why:
            print(f"  no provenance to check: {why}", file=sys.stderr)
            sys.exit(1)
        verdict = parity_verdict(record)
        print(json.dumps(verdict, indent=2))
        sys.exit(0 if verdict["comparable"] else 1)

    if given.import_stamp is not None:
        # Written by whoever is doing the importing, at the moment of the
        # import, and never worked out afterwards by the thing that wants the
        # answer. `image/Makefile`'s `project-stage` writes one for arm B and
        # `bench-external-agent.sh` writes one for arm A, both through this, so
        # the two stamps are comparable because they are the same program.
        where = given.import_stamp
        if not where.is_dir():
            print(f"  not a directory: {where}", file=sys.stderr)
            sys.exit(1)
        print(json.dumps({
            "imported_from": str(where.resolve()),
            "input_manifest": manifest_digest(where),
            "exclusions": list(OUTSIDE_THE_WORKSPACE),
            "source_commit": _commit(where),
            "workspace": str(given.workspace) if given.workspace else None,
        }, indent=2))
        sys.exit(0)

    if given.self_test:
        sys.exit(self_test())

    # ── the live guard ──
    #
    # Exit 0 lets the call through, exit 2 is Claude Code's "blocked, and this
    # is why" — the stderr goes to the model. Anything else would be a hook that
    # failed, which the CLI treats as a warning and lets the call proceed, so
    # every path through here ends in one of those two.
    if given.scope_guard:
        if not given.workspace:
            parser.error("--scope-guard needs --workspace")
        try:
            hook = json.loads(sys.stdin.read() or "{}")
        except json.JSONDecodeError:
            # Rule 9. A payload this could not read is not a call it may wave
            # through: it is a guard that has stopped working, and the run it
            # was guarding must not continue as though it had not.
            print("the scope guard could not read its own hook payload", file=sys.stderr)
            sys.exit(2)
        allowed, why = scope_guard(hook, given.workspace, given.home)
        if allowed:
            sys.exit(0)
        if given.breach_file:
            with given.breach_file.open("a") as into:
                into.write(json.dumps({
                    "tool": hook.get("tool_name"),
                    "input": hook.get("tool_input"),
                    "why": why,
                }) + "\n")
        print(why, file=sys.stderr)
        sys.exit(2)

    # ── arm B, before anything is paid for ──
    if given.preflight_verdict is not None:
        try:
            report = json.loads(given.preflight_verdict.read_text())
        except (OSError, json.JSONDecodeError) as why:
            report = None
            print(f"  the preflight probe left nothing readable: {why}", file=sys.stderr)
        verdict = preflight_verdict(report, given.project)
        print(json.dumps(verdict, indent=2))
        sys.exit(0 if verdict["ready"] else 1)

    for where, what in ((given.manifest, manifest_digest),
                        (given.manifest_lines, manifest),
                        (given.mtimes, mtimes)):
        if where is not None:
            if not where.is_dir():
                print(f"  not a directory: {where}", file=sys.stderr)
                sys.exit(1)
            sys.stdout.write(what(where))
            if what is manifest_digest:
                sys.stdout.write("\n")
            sys.exit(0)

    if given.set_aside is not None:
        if not given.set_aside.is_dir():
            print(f"  not a directory: {given.set_aside}", file=sys.stderr)
            sys.exit(1)
        print(json.dumps(set_aside(given.set_aside), indent=2))
        sys.exit(0)

    if given.forensics:
        if not given.out:
            parser.error("--forensics needs --out")
        for name in ("A", "B"):
            stream = given.out / f"arm{name}.ndjson"
            print(f"\n  ── arm {name}: {stream} ──")
            if not stream.exists() or not stream.stat().st_size:
                print("  no stream on disk")
                continue
            rows = forensics(stream, given.marker or None)
            if not rows:
                print("  every call in this stream is of a tool that can only read, and "
                      "none named the new name")
            elif not any(entry["mutation"] == "writes" for entry in rows):
                print("  nothing here is a certain write. `mutation=unknown` means the "
                      "stream cannot say — it is not `did not write`")
            for n, entry in enumerate(rows, 1):
                print(f"  {n:>3}  {entry['tool']}  mutation={entry['mutation']}  "
                      f"names_new={entry['names_the_new_name']}  -> {entry['result']}")
                print(f"       asked:    {entry['asked']}")
                print(f"       answered: {entry['answered']}")
        sys.exit(0)

    if not given.out:
        parser.error("--out is required")

    expectations = []
    if given.expect_file and given.expect_file.exists():
        expectations = [
            line.strip()
            for line in given.expect_file.read_text().splitlines()
            if line.strip() and not line.startswith("#")
        ]

    # Written by the harness before either arm ran, and read here rather than
    # re-derived: what the two arms were given, where each of them was put, and
    # which commit it all came from. A summary that worked this out for itself
    # afterwards would be the harness grading its own homework.
    provenance = None
    where = given.out / "provenance.json"
    if where.exists() and where.stat().st_size:
        try:
            provenance = json.loads(where.read_text())
        except json.JSONDecodeError:
            provenance = {"unreadable": str(where)}

    arms = [row for row in (
        arm(given.out, "A", expectations, given.marker, given.task, provenance),
        arm(given.out, "B", expectations, given.marker, given.task, provenance)) if row]
    summary = {
        "task": given.task,
        "symbol": given.symbol,
        "model": given.model,
        "max_turns": given.turns,
        "turns_mean": TURNS_MEAN,
        "workspace_boundary": {
            "outside_the_workspace": list(OUTSIDE_THE_WORKSPACE),
            "why": "the machinery that carries the benchmark is not the project it "
                   "measures; what each of these roots holds is reported per arm under "
                   "`machinery_set_aside` rather than hidden",
        },
        "graded_against": expectations or None,
        "provenance": provenance,
        # Absent rather than invented when the harness wrote no provenance: a
        # run whose inputs nobody recorded is a run nobody can say the arms of
        # were comparable, and that is not the same as saying they were not.
        "workspace_parity": parity_verdict(provenance) if provenance else None,
        "arms": arms,
        "note": "One run of one task. This is a harness, not a result.",
    }
    if given.marker:
        summary["renamed_to"] = given.marker

    deltas = observed_deltas(arms)
    if deltas:
        summary["observed_deltas"] = {
            "reading": "B against A, in this one observation. Not a result, not an "
                       "average, and not a winner: one run of one task with two arms "
                       "that differ in more ways than the one under test.",
            "fields": deltas,
        }

    if given.regrade:
        # Never `summary.json`. The original grader's output is the record of
        # what was believed at the time, and a regrade that wrote over it would
        # destroy the only evidence that the instrument was wrong — which is the
        # thing most worth keeping.
        for row in arms:
            row["regrade"] = regrade_status(row)
        present, missing = [], []
        for what, where in (
            ("arm A's stream", "armA.ndjson"),
            ("arm B's stream", "armB.ndjson"),
            ("arm A's baseline manifest", "armA.before.manifest"),
            ("arm A's final manifest", "armA.after.manifest"),
            ("arm A's baseline mtimes", "armA.before.mtimes"),
            ("arm A's final mtimes", "armA.after.mtimes"),
            ("arm B's baseline manifest", "armB.before.manifest"),
            ("arm B's final manifest", "armB.after.manifest"),
            ("arm B's baseline mtimes", "armB.before.mtimes"),
            ("arm B's final mtimes", "armB.after.mtimes"),
            ("arm B's adapter metrics", "armB.metrics.json"),
        ):
            (present if (given.out / where).exists() and (given.out / where).stat().st_size
             else missing).append(f"{what} ({where})")
        summary["regrade"] = {
            "this_is": "the original run, read again by a corrected grader",
            "no_agent_was_run": True,
            "claude_was_not_called": True,
            "the_run_itself": "unchanged; every stream, walk and metrics file was read "
                              "and none was written to",
            "the_grader_changed_after_the_run": True,
            "what_changed_in_the_grader": [
                "`image/build` is machinery and not workspace, so a socket QEMU opened "
                "during the run no longer counts as the workspace failing to come back",
                "the restore check is read from the manifest files rather than from the "
                "digests beside them, so a walk taken under the old boundary can be "
                "re-read under the new one without walking anything again",
                "a mutating tool call the tool itself answered without an error is a "
                "third witness to the workspace having held another state, alongside the "
                "mtimes and the adapter — the mtime witness alone cannot see a change an "
                "agent restored",
                "asked, confirmed, witnessed and restored are four fields and not two",
                "each arm is judged under the boundary that arm actually has: arm A's "
                "is that the process started in the workspace and never left it, arm "
                "B's is that it holds no host file tool and everything it touched went "
                "through the channel into the guest workspace. Comparing arm B's host "
                "working directory with a path inside the machine — two namespaces — "
                "is what reported a clean arm B as VIOLATED",
                "a list of paths is scanned. `paths` was not a field this file knew and "
                "its fallback sweep only looked at string values, so every file named in "
                "a list was a file nobody checked",
                "the scope verdict is a conjunct of the regrade: an arm outside its "
                "boundary is INVALID and an arm whose boundary nobody can check is NOT "
                "PROVEN, rather than both being graded on their numbers",
            ],
            "evidence_reused": present,
            "evidence_missing": missing or None,
            "original_summary": "summary.json, untouched",
        }
        into = given.out / "summary-regraded.json"
    else:
        into = given.out / "summary.json"

    into.write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))

    if given.regrade:
        # The three-line answer, on stderr, beside the JSON rather than inside
        # it. A regrade whose result can only be found by reading two hundred
        # lines of nesting is a regrade somebody will summarise from memory.
        print("\n  the regrade, one line per arm:", file=sys.stderr)
        for row in summary["arms"]:
            status = row.get("regrade") or {}
            scope = (row.get("scope") or {}).get("scope", "not recorded")
            print(f"    arm {row['arm']}: {status.get('status', 'unknown')} "
                  f"(scope {scope}, boundary {(row.get('scope') or {}).get('boundary')})",
                  file=sys.stderr)
            print(f"      {status.get('because', '')}", file=sys.stderr)

    # Rule 3, the half that makes a skip mean something: an arm whose bytes
    # nobody looked at is NOT PROVEN, it says so out loud, and the one variable
    # that demands *this* requirement makes it a failure.
    trouble = 0
    unproven = [row["arm"] for row in summary["arms"]
                if row.get("reversible", {}).get("restore_check") == "not_proven"]
    if unproven:
        print(f"\n  NOT PROVEN  arm {', arm '.join(unproven)}: {NO_TREE_AFTER}", file=sys.stderr)
        if given.require_restore_check:
            trouble = 1

    unwitnessed = [row["arm"] for row in summary["arms"]
                   if row.get("reversible", {}).get("intermediate_state") == "not_proven"]
    if unwitnessed:
        print(f"\n  NOT PROVEN  arm {', arm '.join(unwitnessed)}: {NO_WITNESS}", file=sys.stderr)
        if given.require_mutation_witness:
            trouble = 1

    # ── and the two that are never optional ──
    #
    # These have no `--require-…` switch and that is deliberate. The others
    # guard claims a run can honestly fail to have measured; these two say the
    # run was not the experiment it says it was, and a comparison between an arm
    # that worked somewhere else and an arm that worked where it was put is not
    # a comparison anybody should be able to opt into.
    strayed = [row["arm"] for row in summary["arms"]
               if row.get("scope", {}).get("scope") == "VIOLATED"]
    if strayed:
        for row in summary["arms"]:
            scope = row.get("scope") or {}
            if scope.get("scope") != "VIOLATED":
                continue
            print(f"\n  INVALID  arm {row['arm']} operated outside its workspace "
                  f"({scope.get('guest_project_workspace') or scope.get('workspace')})",
                  file=sys.stderr)
            if scope.get("scope_because"):
                print(f"           {scope['scope_because']}", file=sys.stderr)
            if scope.get("cwd_is_the_workspace") is False:
                print(f"           it started in {scope.get('cwd_reported')!r}",
                      file=sys.stderr)
            for one in scope.get("host_file_tools_used", [])[:10]:
                print(f"           {one['tool']} — a host tool this arm should not have",
                      file=sys.stderr)
            for key in ("paths_outside_the_workspace",
                        "paths_the_machine_accepted_outside_the_workspace",
                        "paths_answered_outside_the_workspace"):
                for entry in scope.get(key, [])[:10]:
                    print(f"           {entry['tool']} {entry['field']}={entry['path']!r}",
                          file=sys.stderr)
        trouble = 1

    parity = summary.get("workspace_parity")
    if parity and not parity.get("comparable"):
        print("\n  INVALID  the two arms were not given comparable inputs:", file=sys.stderr)
        for why in parity.get("because", []):
            print(f"           {why}", file=sys.stderr)
        trouble = 1

    if trouble:
        sys.exit(1)


if __name__ == "__main__":
    main()
