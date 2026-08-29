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
# The exclusions are the same three as before and for the same reason: they are
# what `image/Makefile` leaves out of the copy it puts on the store, plus
# `.git`, which both arms carry and which changes for reasons that are not the
# task.
EXCLUDED_TOP = {".git", "target", "node_modules"}


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
    """Every entry under `root`, as one sorted line each.

    Symlinks are never followed — `os.walk(followlinks=False)` and `lstat`
    throughout — because following them would take the harness outside the
    workspace, and because the question is what the link *is*, not what it
    points at.
    """
    root = pathlib.Path(root)
    lines = []

    for here, directories, files in os.walk(root, followlinks=False):
        relative_here = pathlib.PurePosixPath(pathlib.Path(here).relative_to(root).as_posix())
        if str(relative_here) == ".":
            directories[:] = [d for d in directories if d not in EXCLUDED_TOP]
            files = [f for f in files if f not in EXCLUDED_TOP]

        for name in list(directories) + list(files):
            path = pathlib.Path(here) / name
            where = (relative_here / name).as_posix() if str(relative_here) != "." else name
            try:
                info = path.lstat()
            except OSError as why:
                lines.append(f"?	-	-	unstattable:{why.errno}	{where}")
                continue
            mode = stat.S_IMODE(info.st_mode)
            if stat.S_ISLNK(info.st_mode):
                try:
                    target = os.readlink(path)
                except OSError as why:
                    target = f"unreadable:{why.errno}"
                lines.append(f"l	{mode:04o}	-	{target}	{where}")
            elif stat.S_ISDIR(info.st_mode):
                lines.append(f"d	{mode:04o}	-	-	{where}")
            elif stat.S_ISREG(info.st_mode):
                lines.append(f"f	{mode:04o}	{info.st_size}	{_digest_of_file(path)}	{where}")
            else:
                # A fifo, a socket, a device node. Named by kind rather than
                # lumped in with "other", because an agent that left a fifo
                # where a file was has not restored the tree and the summary
                # should be able to say what it left.
                lines.append(f"s{info.st_mode & 0o170000:o}	{mode:04o}	-	-	{where}")

    lines.sort(key=lambda line: line.rsplit("\t", 1)[-1])
    return "\n".join(lines) + ("\n" if lines else "")


def manifest_digest(root):
    return hashlib.sha256(manifest(root).encode()).hexdigest()


def mtimes(root):
    """When each regular file under `root` was last written, as sorted lines.

    Kept **out** of the manifest and in a file of its own, because an mtime is
    not something the task asks anybody to restore — `git checkout -- .` puts
    every byte back and moves every mtime, and a digest that folded them in
    would fail arm A for doing the task correctly.

    It is here for the opposite question, which nothing else on this host can
    answer: *did the workspace ever hold something other than what it started
    with?* An agent that renamed a symbol in six files and put them back leaves
    six files whose contents match and whose mtimes moved. That is evidence
    from the filesystem rather than from the agent's own account of itself,
    which is the one thing the `reversible` verdict may not be read off.
    """
    root = pathlib.Path(root)
    lines = []
    for here, directories, files in os.walk(root, followlinks=False):
        relative_here = pathlib.Path(here).relative_to(root).as_posix()
        if relative_here == ".":
            directories[:] = [d for d in directories if d not in EXCLUDED_TOP]
            files = [f for f in files if f not in EXCLUDED_TOP]
        for name in files:
            path = pathlib.Path(here) / name
            where = f"{relative_here}/{name}" if relative_here != "." else name
            try:
                info = path.lstat()
            except OSError:
                continue
            if stat.S_ISREG(info.st_mode):
                lines.append(f"{info.st_mtime_ns}\t{where}")
    lines.sort(key=lambda line: line.split("\t", 1)[1])
    return "\n".join(lines) + ("\n" if lines else "")


def files_touched(before_text, after_text):
    """How many files the filesystem says were written between the two walks.

    A file counts as touched when it existed before and its mtime moved, or
    when it did not exist before and does now. A file that vanished counts too:
    something removed it. All three are writes the workspace saw, and none of
    them can be produced by an agent that only read.
    """
    def by_path(text):
        found = {}
        for line in text.splitlines():
            if "\t" not in line:
                continue
            when, where = line.split("\t", 1)
            found[where] = when
        return found

    before, after = by_path(before_text), by_path(after_text)
    touched = 0
    for where, when in after.items():
        if before.get(where) != when:
            touched += 1
    for where in before:
        if where not in after:
            touched += 1
    return touched


def is_a_write(name, given):
    """Whether this one call changed the workspace, as far as the stream shows.

    `thalyx_edit` is the reason this is a function and not a set membership: its
    `show` action returns numbered lines and writes nothing, so counting it as a
    mutation would credit arm B with edits it did not make — an error in the
    flattering direction, which is the one that must not happen.
    """
    if name not in WORKSPACE_WRITERS:
        return False
    if name == "mcp__thalyx__thalyx_edit":
        return (given or {}).get("action") != "show"
    return True


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
    naming = 0
    # Pass one collects the calls; the results that decide them arrive in later
    # events, so nothing about success can be settled inside the loop.
    made = []
    failed_ids = set()
    answered_ids = set()

    for event in events(path):
        kind = event.get("type")

        if kind == "assistant":
            for block in event.get("message", {}).get("content", []) or []:
                if not isinstance(block, dict) or block.get("type") != "tool_use":
                    continue
                name = block.get("name") or "<unnamed>"
                given = block.get("input", {})
                per_tool[name] = per_tool.get(name, 0) + 1
                calls += 1
                serialised = json.dumps(given)
                sent += len(serialised)
                if is_a_write(name, given):
                    writes += 1
                if marker and marker in serialised:
                    naming += 1
                    made.append((block.get("id"), name, given))

        elif kind == "user":
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
        row["mutating_tool_calls"] = writes
        # Absent, not zero, where no marker was given: a task with no rename in
        # it has no new name to have been named, and a `0` would read as an
        # agent that never wrote one.
        if marker:
            row["tool_calls_naming_the_new_name"] = naming
            confirmed = wrong = read_only = unanswered = 0
            for where, name, given in made:
                if not is_a_write(name, given):
                    read_only += 1
                elif where in failed_ids:
                    wrong += 1
                elif where in answered_ids:
                    confirmed += 1
                else:
                    unanswered += 1
            row["mutations_naming_the_new_name"] = confirmed
            row["failed_calls_naming_the_new_name"] = wrong
            row["read_only_calls_naming_the_new_name"] = read_only
            # Counted apart from the failures because it is a different fact: a
            # call the stream never carried an answer for. It is not evidence
            # of a mutation and it is not evidence of a failure either.
            row["unanswered_calls_naming_the_new_name"] = unanswered

    if marker and isinstance(row.get("answer"), str):
        row["new_name_in_answer"] = marker in row["answer"]

    return row


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
    """Whether this arm did the reversible task, from five separate instruments.

    The one thing this must never do is read the verdict off the tree digest
    alone. A restored digest is what an agent that changed everything and put it
    back leaves, and equally what an agent that answered "no" and stopped
    leaves — so the digest can only ever be one conjunct of five.

    Until 2026-08-28 the second conjunct was "the new name appeared in some tool
    call", which is a sentence the agent writes and not a thing the workspace
    saw. A `Grep` for the new name satisfied it. A failed `Edit` satisfied it.
    An agent that read a file, said it had renamed everything and stopped
    satisfied it. What is required now, and each part comes from somewhere
    else:

      A  `mutation_attempted`  a call that can only mutate, that names the new
                               name, and whose *result* came back without an
                               error. Read from the stream, but from the tool's
                               answer rather than from the model's request.

      B  `intermediate_state`  the workspace really held something else for a
                               while. Read from outside the agent entirely:
                               either the filesystem said so (files whose mtime
                               moved between the walk before the run and the
                               walk after it) or `thalyx-mcp --metrics` counted
                               mutations of its own. Unknown → `not_proven`,
                               and `--require-mutation-witness` makes that a
                               non-zero exit.

      C  `completed_normally`  the run's own `result` event did not say
                               `is_error`. An agent that died at its turn limit
                               has not done the task, whatever its last message
                               claimed.

      D  `task_success`        the final answer named every file the ground
                               truth demands.

      E  `restored`            the manifest digest came back — contents, entry
                               type, permission bits and symlink targets, hashed
                               on this host.

    A component that is unknown makes `passed` **absent**. Not false: a run
    whose restore nobody has checked yet has not failed, and printing `false`
    for it would be the same lie in the other direction.

    The one place A is allowed to be satisfied without a mutating tool call is
    arm A's shell. `sed -i` arrives as `Bash` and the stream cannot tell it from
    `ls`, so a rename done that way would score `mutation_attempted: false` and
    the comparison would punish arm A for using the tool it was given. B covers
    it exactly: `sed -i` moves mtimes and `ls` does not. So a call that named
    the new name, came back without an error, and left the filesystem changed
    counts — and none of the three false positives above can produce that,
    because none of them changes a file.
    """
    verdict = {}

    if "mutating_tool_calls" in row:
        verdict["mutating_tool_calls"] = row["mutating_tool_calls"]
    for field in ("mutations_naming_the_new_name",
                  "failed_calls_naming_the_new_name",
                  "read_only_calls_naming_the_new_name"):
        if field in row:
            verdict[field] = row[field]

    # ── B, first, because A leans on it for the shell case ──
    witness = None
    if "files_touched_on_disk" in row:
        verdict["files_touched_on_disk"] = row["files_touched_on_disk"]
        witness = row["files_touched_on_disk"] > 0
        verdict["intermediate_state_from"] = "the filesystem, by mtime"
    mutations = (row.get("thalyx") or {}).get("mutations")
    if isinstance(mutations, int):
        verdict["thalyx_mutations"] = mutations
        if witness:
            verdict["intermediate_state_from"] = "the filesystem and the adapter, agreeing"
        elif witness is None:
            verdict["intermediate_state_from"] = "the adapter's own count"
        # An adapter that counted zero is a witness that saw nothing, not an
        # absent witness. Leaving it `None` would turn arm B's laziest possible
        # run — never call a mutating tool — into `not_proven` rather than a
        # loss, which is the exact direction this comparison must not be wrong in.
        witness = bool(witness) or mutations > 0
    if witness is None:
        verdict["intermediate_state"] = "not_proven"
        verdict["intermediate_state_because"] = NO_WITNESS
    else:
        verdict["intermediate_state"] = witness

    # ── A ──
    if marker_given and "mutations_naming_the_new_name" in row:
        by_a_mutating_tool = row["mutations_naming_the_new_name"] > 0
        # The shell case. `answered` is every call that named the new name and
        # did not come back an error; on its own that is a `Grep`, which is why
        # it only counts alongside a filesystem that moved.
        answered = (row.get("tool_calls_naming_the_new_name", 0)
                    - row.get("failed_calls_naming_the_new_name", 0)
                    - row.get("unanswered_calls_naming_the_new_name", 0))
        by_the_shell = answered > 0 and witness is True
        verdict["mutation_attempted"] = by_a_mutating_tool or by_the_shell
        verdict["really_changed"] = verdict["mutation_attempted"] and witness is True

    # ── C ──
    if "is_error" in row and row["is_error"] is not None:
        verdict["completed_normally"] = row["is_error"] is False

    # ── E ──
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


def arm(out, name, expectations, marker=None, task=""):
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

    metrics = out / f"arm{name}.metrics.json"
    if metrics.exists():
        try:
            row["thalyx"] = json.loads(metrics.read_text())
        except json.JSONDecodeError:
            row["thalyx"] = {"unreadable": str(metrics)}

    before, after = out / f"arm{name}.before", out / f"arm{name}.after"
    if before.exists() and after.exists():
        row["tree_unchanged"] = before.read_text() == after.read_text()
        # What actually differs, when something does. The digests are one line
        # each and say nothing; the manifests beside them say which entry moved,
        # and a restore that failed is worth reading rather than re-running.
        detail = []
        for side in ("before", "after"):
            lines = out / f"arm{name}.{side}.manifest"
            detail.append(lines.read_text().splitlines() if lines.exists() else None)
        if row["tree_unchanged"] is False and all(part is not None for part in detail):
            was, now = set(detail[0]), set(detail[1])
            row["tree_differences"] = sorted(
                [f"-{line}" for line in was - now] + [f"+{line}" for line in now - was]
            )[:40]

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

    trouble += counting_self_test(sample)
    trouble += manifest_self_test()
    trouble += verdict_self_test()

    print()
    print("  PROVEN" if not trouble else f"  {trouble} FAILED")
    return 1 if trouble else 0


def counting_self_test(sample):
    """That writes and the new name are counted right, in either arm's shape.

    Rule 6 is about **format**, and the format is settled by the captured
    session above: this builds its streams out of that session's own event
    envelopes and changes only the tool name and the input inside them. So what
    it proves is the counting, on a shape that came from Claude Code rather than
    from whoever wrote this file — and it deliberately proves nothing about the
    format, which no fixture can.
    """
    envelope = None
    for event in events(sample):
        if event.get("type") != "assistant":
            continue
        for block in event.get("message", {}).get("content", []) or []:
            if isinstance(block, dict) and block.get("type") == "tool_use":
                envelope = event
                break
        if envelope:
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
            "files_touched_on_disk": 6, "is_error": False,
            "tree_unchanged": True, "task_success": True}

    def like(**changed):
        row = dict(done)
        row.update(changed)
        return row

    cases = [
        ("did the work and put it back", like(), {"passed": True}),

        # 1. The false positive that started this. `Grep {"pattern":
        #    "WidgetRenamed"}` names the new name in a tool call and changes
        #    nothing, and the old verdict called that `really_changed`.
        ("only read, and mentioned the new name while reading",
         like(mutations_naming_the_new_name=0, read_only_calls_naming_the_new_name=3,
              mutating_tool_calls=0, files_touched_on_disk=0),
         {"passed": False, "really_changed": False, "intermediate_state": False}),

        # 2. An `Edit` whose `old_string` matched nothing. The call names the
        #    new name; the workspace never held it for an instant.
        ("tried to edit, the edit failed, and the new name was in the failed call",
         like(mutations_naming_the_new_name=0, failed_calls_naming_the_new_name=4,
              mutating_tool_calls=4, files_touched_on_disk=0),
         {"passed": False, "really_changed": False}),

        # 3. Nothing at all, which leaves the tree perfect.
        ("did nothing, so the tree is perfect",
         like(mutations_naming_the_new_name=0, tool_calls_naming_the_new_name=0,
              mutating_tool_calls=0, files_touched_on_disk=0),
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
         like(mutations_naming_the_new_name=0, mutating_tool_calls=0),
         {"passed": True, "mutation_attempted": True}),

        # And its control, one field apart: the same shell calls with nothing
        # touched on disk is a `grep`, and must not pass.
        ("used the shell only to look",
         like(mutations_naming_the_new_name=0, mutating_tool_calls=0,
              files_touched_on_disk=0),
         {"passed": False, "really_changed": False}),
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

    # An arm nobody hashed, and an arm nobody witnessed, are each undecided
    # rather than failed. A run whose bytes nobody looked at has not lost.
    for about, missing in (("hashed its tree", "tree_unchanged"),
                           ("watched its workspace", "files_touched_on_disk")):
        row = like()
        del row[missing]
        undecided = reversible_verdict(row, marker_given=True, graded=True)
        if "passed" not in undecided and "undecided_because" in undecided:
            print(f"  ok      an arm nobody {about} is undecided, not failed")
        else:
            print(f"  FAILED  an arm nobody {about} was given a verdict: {undecided!r}")
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

    row["thalyx"] = {"mutations": 0}
    row["mutations_naming_the_new_name"] = 0
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

        # ── the witness, which is the opposite question ──
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
    parser.add_argument("--self-test", action="store_true")
    given = parser.parse_args()

    if given.self_test:
        sys.exit(self_test())

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

    if not given.out:
        parser.error("--out is required")

    expectations = []
    if given.expect_file and given.expect_file.exists():
        expectations = [
            line.strip()
            for line in given.expect_file.read_text().splitlines()
            if line.strip() and not line.startswith("#")
        ]

    summary = {
        "task": given.task,
        "symbol": given.symbol,
        "model": given.model,
        "max_turns": given.turns,
        "graded_against": expectations or None,
        "arms": [row for row in (arm(given.out, "A", expectations, given.marker, given.task),
                                 arm(given.out, "B", expectations, given.marker, given.task)) if row],
        "note": "One run of one task. This is a harness, not a result.",
    }
    if given.marker:
        summary["renamed_to"] = given.marker
    (given.out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))

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

    if trouble:
        sys.exit(1)


if __name__ == "__main__":
    main()
