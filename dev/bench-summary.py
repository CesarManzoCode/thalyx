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
import json
import pathlib
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
    a `sed` command inside `Bash` when the agent felt like it. All three are the
    agent claiming to have written the new name somewhere.
    """
    row = {}
    per_tool = {}
    calls = 0
    returned = 0
    sent = 0
    results_seen = 0
    writes = 0
    naming = 0

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

        elif kind == "user":
            content = event.get("message", {}).get("content")
            for block in content if isinstance(content, list) else []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    returned += text_length(block.get("content"))
                    results_seen += 1

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


def reversible_verdict(row, marker_given, graded):
    """Whether this arm did the reversible task, from three separate instruments.

    The one thing this must never do is read the verdict off the tree hash
    alone. `restored` is true for an agent that changed everything and put it
    back, and equally true for an agent that answered "no" and stopped — so
    `really_changed`, which comes from the agent's own stream rather than from
    the filesystem, is what tells those two apart.

    A component that is unknown makes `passed` **absent**. Not false: a run
    whose restore nobody has checked yet has not failed, and printing `false`
    for it would be the same lie in the other direction.
    """
    verdict = {}

    if marker_given and "tool_calls_naming_the_new_name" in row:
        verdict["really_changed"] = row["tool_calls_naming_the_new_name"] > 0
    if "mutating_tool_calls" in row:
        verdict["mutating_tool_calls"] = row["mutating_tool_calls"]

    if "tree_unchanged" in row:
        verdict["restored"] = row["tree_unchanged"]
        verdict["restore_check"] = "proven"
    else:
        verdict["restore_check"] = "not_proven"
        verdict["restore_check_because"] = NO_TREE_AFTER

    needed = [("really_changed", verdict.get("really_changed") if marker_given else True),
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
    """That the reversible verdict cannot be earned by doing nothing.

    Ordinary logic over this file's own data, so no captured sample is involved
    and none is owed. What it is guarding is the one mistake the task invites:
    the tree hashes equal both for an agent that changed everything and put it
    back and for an agent that never touched anything, and only the first is a
    pass.
    """
    cases = [
        ("did the work and put it back",
         {"tool_calls_naming_the_new_name": 6, "mutating_tool_calls": 6,
          "tree_unchanged": True, "task_success": True},
         True, {"passed": True}),
        ("did nothing at all, so the tree is perfect",
         {"tool_calls_naming_the_new_name": 0, "mutating_tool_calls": 0,
          "tree_unchanged": True, "task_success": True},
         True, {"passed": False, "really_changed": False}),
        ("did the work and left it changed",
         {"tool_calls_naming_the_new_name": 6, "mutating_tool_calls": 6,
          "tree_unchanged": False, "task_success": True},
         True, {"passed": False, "restored": False}),
        ("did the work, put it back, and named the wrong files",
         {"tool_calls_naming_the_new_name": 6, "mutating_tool_calls": 6,
          "tree_unchanged": True, "task_success": False},
         True, {"passed": False}),
        ("nobody has hashed its tree yet",
         {"tool_calls_naming_the_new_name": 6, "mutating_tool_calls": 6,
          "task_success": True},
         True, {"restore_check": "not_proven"}),
    ]

    trouble = 0
    for about, row, graded, wanted in cases:
        verdict = reversible_verdict(row, marker_given=True, graded=graded)
        for field, want in wanted.items():
            got = verdict.get(field)
            if got == want:
                print(f"  ok      {about}: {field} is {got!r}")
            else:
                print(f"  FAILED  {about}: {field} expected {want!r}, got {got!r}")
                trouble += 1

    # The unchecked arm must be undecided, never a failure. A run whose bytes
    # nobody looked at has not lost.
    undecided = reversible_verdict(
        {"tool_calls_naming_the_new_name": 6, "mutating_tool_calls": 6, "task_success": True},
        marker_given=True, graded=True)
    if "passed" not in undecided and "undecided_because" in undecided:
        print("  ok      an arm nobody hashed is undecided, not failed")
    else:
        print(f"  FAILED  an unchecked arm was given a verdict: {undecided!r}")
        trouble += 1

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
    parser.add_argument("--self-test", action="store_true")
    given = parser.parse_args()

    if given.self_test:
        sys.exit(self_test())

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
    unproven = [row["arm"] for row in summary["arms"]
                if row.get("reversible", {}).get("restore_check") == "not_proven"]
    if unproven:
        print(f"\n  NOT PROVEN  arm {', arm '.join(unproven)}: {NO_TREE_AFTER}", file=sys.stderr)
        if given.require_restore_check:
            sys.exit(1)


if __name__ == "__main__":
    main()
