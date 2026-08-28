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
"""

import argparse
import json
import pathlib
import sys

# Which tools do the thing, per arm. Named rather than guessed at from the tool
# name, because `Bash` can run `grep` and there is no honest way to know from
# the stream whether it did — so `Bash` counts as a tool call and as nothing
# else, and the raw per-tool table below is what tells you the rest.
FILE_READERS = {"Read", "NotebookRead", "mcp__thalyx__thalyx_read"}
TEXT_SEARCHERS = {"Grep", "mcp__thalyx__thalyx_find"}
INDEX_QUESTIONS = {"mcp__thalyx__thalyx_symbol", "mcp__thalyx__thalyx_dependencies"}


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


def read_stream(path):
    """The whole of one arm, from its stream."""
    row = {}
    per_tool = {}
    calls = 0
    returned = 0
    sent = 0
    results_seen = 0

    for event in events(path):
        kind = event.get("type")

        if kind == "assistant":
            for block in event.get("message", {}).get("content", []) or []:
                if not isinstance(block, dict) or block.get("type") != "tool_use":
                    continue
                name = block.get("name") or "<unnamed>"
                per_tool[name] = per_tool.get(name, 0) + 1
                calls += 1
                sent += len(json.dumps(block.get("input", {})))

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


def arm(out, name, expectations):
    row = {"arm": name}

    stream = out / f"arm{name}.ndjson"
    plain = out / f"arm{name}.json"
    if stream.exists() and stream.stat().st_size:
        row.update(read_stream(stream))
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
    if "index_questions" in row:
        print("  FAILED  index_questions was invented for a session that had no index")
        trouble += 1
    else:
        print("  ok      index_questions is absent, not zero, where it cannot exist")

    print()
    print("  PROVEN" if not trouble else f"  {trouble} FAILED")
    return 1 if trouble else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=pathlib.Path)
    parser.add_argument("--task", default="")
    parser.add_argument("--symbol", default="")
    parser.add_argument("--model", default="")
    parser.add_argument("--turns", default="")
    parser.add_argument("--expect-file", type=pathlib.Path)
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
        "arms": [row for row in (arm(given.out, "A", expectations),
                                 arm(given.out, "B", expectations)) if row],
        "note": "One run of one task. This is a harness, not a result.",
    }
    (given.out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
