#!/usr/bin/env python3
"""Turn an EXP-13 campaign's observations into the causal verdict.

The campaign (`cargo test ... --ignored exp13_campaign`) writes, under one
report directory, `<backend>-r<round>/<case>.json` for every arm it could run
each round, each carrying the case's observation and the platform trace of every
`exec` in it. This reads those, takes the cost that matters most — task
end-to-end latency, the transaction's own `whole_ns` — plus the per-phase totals
and the transport a run moved, and reports three differences per case and in
aggregate:

    K1 - L0   the total migration effect
    L1 - L0   the redesign effect, still on Linux
    K1 - L1   the kernel-specific contribution

with a bootstrap confidence interval over the paired rounds. It does not decide
the verdict for you; it lays the three effects side by side, which is the whole
point of not collapsing them into one number.

Usage:
    dev/exp13/analyze.py <report-dir> [--out <dir>] [--resamples 10000]
"""

from __future__ import annotations

import argparse
import json
import random
import re
import statistics
from pathlib import Path

ARMS = ["linux-current", "linux-managed", "thalyx-kernel-managed"]
SHORT = {"linux-current": "L0", "linux-managed": "L1", "thalyx-kernel-managed": "K1"}

ROUND_DIR = re.compile(r"^(?P<backend>.+)-r(?P<round>\d+)$")


def load(report: Path) -> dict:
    """observations[backend][round][case] = observation dict."""
    observations: dict = {arm: {} for arm in ARMS}
    for directory in sorted(report.glob("*-r*")):
        match = ROUND_DIR.match(directory.name)
        if not match:
            continue
        backend = match.group("backend")
        if backend not in ARMS:
            continue
        rnd = int(match.group("round"))
        for case_file in sorted(directory.glob("*.json")):
            try:
                blob = json.loads(case_file.read_text())
            except json.JSONDecodeError:
                continue
            observations[backend].setdefault(rnd, {})[case_file.stem] = blob
    return observations


def whole_ns(blob: dict) -> int | None:
    """The transaction's end-to-end nanoseconds, summed over the case's execs."""
    traces = blob.get("traces") or []
    total = 0
    seen = False
    for entry in traces:
        trace = entry.get("trace")
        if isinstance(trace, dict) and "whole_ns" in trace:
            total += int(trace["whole_ns"])
            seen = True
    return total if seen else None


def phase_ns(blob: dict) -> dict:
    """Per-phase nanoseconds, summed over the case's execs."""
    phases: dict = {}
    for entry in blob.get("traces") or []:
        trace = entry.get("trace")
        if not isinstance(trace, dict):
            continue
        for name, total in (trace.get("phases") or {}).items():
            phases[name] = phases.get(name, 0) + int(total.get("ns", 0))
    return phases


def transport(blob: dict) -> dict:
    """Calls and bytes a run moved between the client and its store, if any."""
    calls = sent = received = 0
    seen = False
    for request in blob.get("observation", {}).get("requests", []):
        evidence = request.get("evidence")
        if not isinstance(evidence, dict):
            continue
        state = (evidence.get("platform") or {}).get("state") or {}
        carried = state.get("transport")
        if isinstance(carried, dict):
            calls += int(carried.get("calls", 0))
            sent += int(carried.get("bytes_sent", 0))
            received += int(carried.get("bytes_received", 0))
            seen = True
    return {"calls": calls, "bytes_sent": sent, "bytes_received": received} if seen else {}


def paired(observations: dict, metric) -> dict:
    """metric(blob) -> number, gathered as series[arm][case] = [per round]."""
    series: dict = {arm: {} for arm in ARMS}
    for arm in ARMS:
        rounds = observations[arm]
        for rnd, cases in rounds.items():
            for case, blob in cases.items():
                value = metric(blob)
                if value is None:
                    continue
                series[arm].setdefault(case, {})[rnd] = value
    return series


def bootstrap_ci(diffs: list[float], resamples: int, rng: random.Random) -> tuple:
    """Median of paired differences, with a 95% bootstrap CI."""
    if not diffs:
        return (None, None, None)
    point = statistics.median(diffs)
    if len(diffs) == 1:
        return (point, point, point)
    medians = []
    n = len(diffs)
    for _ in range(resamples):
        sample = [diffs[rng.randrange(n)] for _ in range(n)]
        medians.append(statistics.median(sample))
    medians.sort()
    lo = medians[int(0.025 * resamples)]
    hi = medians[min(int(0.975 * resamples), resamples - 1)]
    return (point, lo, hi)


def contrasts(series: dict, resamples: int) -> dict:
    """The three effects, per case and aggregated, over paired rounds."""
    rng = random.Random(0xE13)
    out: dict = {"per_case": {}, "aggregate": {}}
    pairs = [("K1", "L0"), ("L1", "L0"), ("K1", "L1")]
    by_short = {SHORT[arm]: series[arm] for arm in ARMS}
    all_cases = sorted(
        {case for arm in by_short.values() for case in arm}
    )
    aggregate = {f"{a}-{b}": [] for a, b in pairs}
    for case in all_cases:
        row = {}
        for a, b in pairs:
            left = by_short.get(a, {}).get(case, {})
            right = by_short.get(b, {}).get(case, {})
            rounds = sorted(set(left) & set(right))
            diffs = [float(left[r] - right[r]) for r in rounds]
            aggregate[f"{a}-{b}"].extend(diffs)
            point, lo, hi = bootstrap_ci(diffs, resamples, rng)
            row[f"{a}-{b}"] = {
                "rounds": len(diffs),
                "median": point,
                "ci95": [lo, hi],
            }
        # The arms' own medians, for context.
        row["medians"] = {
            short: (statistics.median(list(by_short[short][case].values()))
                    if case in by_short.get(short, {}) else None)
            for short in ("L0", "L1", "K1")
        }
        out["per_case"][case] = row
    for key, diffs in aggregate.items():
        point, lo, hi = bootstrap_ci(diffs, resamples, rng)
        out["aggregate"][key] = {"rounds": len(diffs), "median": point, "ci95": [lo, hi]}
    return out


def ms(ns: float | None) -> str:
    return "—" if ns is None else f"{ns / 1e6:+.3f}"


def summarise(report: Path, out: Path, resamples: int) -> None:
    observations = load(report)
    present = [SHORT[arm] for arm in ARMS if observations[arm]]
    rounds = max(
        (len(observations[arm]) for arm in ARMS if observations[arm]),
        default=0,
    )

    e2e = contrasts(paired(observations, whole_ns), resamples)

    # Per-phase and transport, aggregated, for the cost breakdown.
    phase_names = sorted(
        {name for arm in ARMS for cases in observations[arm].values()
         for blob in cases.values() for name in phase_ns(blob)}
    )
    phase_effects = {}
    for name in phase_names:
        s = paired(observations, lambda blob, n=name: phase_ns(blob).get(n))
        phase_effects[name] = contrasts(s, resamples)["aggregate"]

    transport_series = paired(observations, lambda blob: (transport(blob) or {}).get("calls"))
    transport_effect = contrasts(transport_series, resamples)["aggregate"]

    raw = {
        "report": str(report),
        "arms_present": present,
        "rounds": rounds,
        "resamples": resamples,
        "e2e_latency_ns": e2e,
        "phase_ns": phase_effects,
        "transport_calls": transport_effect,
    }
    out.mkdir(parents=True, exist_ok=True)
    (out / "exp13-results.json").write_text(json.dumps(raw, indent=2) + "\n")

    lines = []
    lines.append("# EXP-13 campaign results\n")
    lines.append(f"Report: `{report}` — arms present: {', '.join(present) or 'none'} — "
                 f"paired rounds: {rounds}, bootstrap resamples: {resamples}.\n")
    lines.append("All figures are milliseconds of task end-to-end latency "
                 "(`whole_ns`), median of paired per-round differences with a 95% "
                 "bootstrap CI. A negative K1−L1 means the kernel arm was faster.\n")
    lines.append("## Aggregate, all cases\n")
    lines.append("| effect | median (ms) | 95% CI (ms) | paired rounds×cases |")
    lines.append("|---|---|---|---|")
    for key in ("K1-L0", "L1-L0", "K1-L1"):
        cell = e2e["aggregate"].get(key, {})
        lo, hi = cell.get("ci95", [None, None])
        lines.append(f"| {key} | {ms(cell.get('median'))} | "
                     f"[{ms(lo)}, {ms(hi)}] | {cell.get('rounds', 0)} |")
    lines.append("\n## Per case\n")
    lines.append("| case | L0 | L1 | K1 | K1−L0 | L1−L0 | K1−L1 |")
    lines.append("|---|---|---|---|---|---|---|")
    for case, row in e2e["per_case"].items():
        med = row["medians"]
        lines.append(
            f"| {case} | {ms(med['L0'])} | {ms(med['L1'])} | {ms(med['K1'])} | "
            f"{ms(row['K1-L0']['median'])} | {ms(row['L1-L0']['median'])} | "
            f"{ms(row['K1-L1']['median'])} |"
        )
    lines.append("\n## Transport calls (managed arms), aggregate paired difference\n")
    lines.append("| effect | median calls | 95% CI |")
    lines.append("|---|---|---|")
    for key in ("K1-L1",):
        cell = transport_effect.get(key, {})
        lo, hi = cell.get("ci95", [None, None])
        lines.append(f"| {key} | {cell.get('median')} | [{lo}, {hi}] |")
    lines.append("")
    (out / "exp13-summary.md").write_text("\n".join(lines) + "\n")
    print(f"wrote {out/'exp13-results.json'} and {out/'exp13-summary.md'}")
    print("\n".join(lines[:16]))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="the campaign report directory")
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--resamples", type=int, default=10000)
    args = parser.parse_args()
    out = args.out or (args.report / "analysis")
    summarise(args.report, out, args.resamples)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
