#!/usr/bin/env python3
"""Run the privacy-flow conformance corpus against an analyser.

The contract (see README.md): for every vector directory, the runner calls

    <verifier command> <vector directory>

and reads one JSON object per line from its standard output, each a flow:

    {"source": {"path": "a.ts", "line": 2}, "sink": {"path": "a.ts", "line": 3}, "category": "..."}

Paths are relative to the vector directory; other members are ignored. The exit status must be
0 or 4 (4: coverage incomplete); anything else is an error. A vector passes when the reported
flows, compared as (source path, source line, sink path, sink line, category), are exactly its
expected.jsonl. Vectors with status "known-limitation" record correct behaviour that the
reference implementation does not reach yet: they are reported, not required.

Standard library only; no network access.
"""

import argparse
import json
import pathlib
import shlex
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent


def key(flow):
    return (
        flow["source"]["path"],
        int(flow["source"]["line"]),
        flow["sink"]["path"],
        int(flow["sink"]["line"]),
        flow["category"],
    )


def load_expected(vector):
    out = set()
    for line in (vector / "expected.jsonl").read_text().splitlines():
        if line.strip():
            out.add(key(json.loads(line)))
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--verifier", required=True, help="command to run; the vector directory is appended")
    ap.add_argument("--report", help="write a JSON report to this file")
    ap.add_argument("--vectors", default=str(HERE / "vectors"), help="vector directory")
    ap.add_argument("--timeout", type=int, default=120)
    args = ap.parse_args()

    base = shlex.split(args.verifier)
    results = []
    totals = {"pass": 0, "fail": 0, "error": 0, "known_limitation_fail": 0, "known_limitation_pass": 0}
    tp = fp = fn = 0
    for vector in sorted(p for p in pathlib.Path(args.vectors).iterdir() if (p / "vector.json").is_file()):
        meta = json.loads((vector / "vector.json").read_text())
        status = meta.get("status", "required")
        expected = load_expected(vector)
        entry = {"vector": vector.name, "status": status}
        try:
            proc = subprocess.run(base + [str(vector)], capture_output=True, text=True, timeout=args.timeout)
        except (OSError, subprocess.TimeoutExpired) as exc:
            entry.update(outcome="error", detail=str(exc))
            totals["error"] += 1
            results.append(entry)
            continue
        if proc.returncode not in (0, 4):
            entry.update(outcome="error", detail=f"exit {proc.returncode}: {proc.stderr.strip()[:500]}")
            totals["error"] += 1
            results.append(entry)
            continue
        try:
            reported = {key(json.loads(l)) for l in proc.stdout.splitlines() if l.strip()}
        except (ValueError, KeyError, TypeError) as exc:
            entry.update(outcome="error", detail=f"unparseable output: {exc}")
            totals["error"] += 1
            results.append(entry)
            continue
        missing = sorted(expected - reported)
        extra = sorted(reported - expected)
        ok = not missing and not extra
        if status == "known-limitation":
            outcome = "known_limitation_pass" if ok else "known_limitation_fail"
        else:
            outcome = "pass" if ok else "fail"
            tp += len(expected & reported)
            fp += len(extra)
            fn += len(missing)
        totals[outcome] += 1
        entry.update(outcome=outcome, missing=[list(m) for m in missing], unexpected=[list(e) for e in extra])
        results.append(entry)
        if outcome in ("fail", "error"):
            print(f"FAIL {vector.name}: missing {missing} unexpected {extra}", file=sys.stderr)

    precision = tp / (tp + fp) if tp + fp else 1.0
    recall = tp / (tp + fn) if tp + fn else 1.0
    report = {
        "totals": totals,
        "flows": {"true_positive": tp, "false_positive": fp, "false_negative": fn},
        "precision": round(precision, 4),
        "recall": round(recall, 4),
        "results": results,
    }
    if args.report:
        pathlib.Path(args.report).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"totals": totals, "precision": report["precision"], "recall": report["recall"]}, sort_keys=True))
    return 0 if totals["fail"] == 0 and totals["error"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
