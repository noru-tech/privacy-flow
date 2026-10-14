#!/usr/bin/env python3
"""Smoke-test the C# analysis on dotnet/eShop at a pinned commit.

    python3 .github/scripts/smoke_eshop.py target/release/piiflow [--checkout DIR] [--report FILE]

Fetches the repository at the commit `tests/smoke/eshop.expected.json` names (unless --checkout
gives an existing one), scans it twice (one thread and eight), and checks:

- both runs exit 0 or 4 and write the same bytes (determinism), within the time budget;
- the findings the expectations require are there, with the required categories, and the ones
  they forbid are not;
- the required flows are there;
- coverage gaps are of the allowed kinds and within the limits, and no C# or Razor file is an
  unsupported language.

The fetch happens here, in the test, never in piiflow, which makes no network calls. Standard
library only.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPECTED = ROOT / "tests" / "smoke" / "eshop.expected.json"
SEVERITY = {"info": 0, "warning": 1, "medium": 2, "high": 3}


def fetch(repository: str, commit: str, into: pathlib.Path) -> None:
    def git(*args: str) -> None:
        subprocess.run(["git", "-C", str(into), *args], check=True, capture_output=True)

    into.mkdir(parents=True, exist_ok=True)
    git("init", "-q")
    git("remote", "add", "origin", repository)
    for attempt in range(3):
        try:
            git("fetch", "-q", "--depth", "1", "origin", commit)
            break
        except subprocess.CalledProcessError:
            if attempt == 2:
                raise
            time.sleep(5 * (attempt + 1))
    git("checkout", "-q", "FETCH_HEAD")


def scan(binary: str, checkout: pathlib.Path, threads: int) -> tuple[int, bytes, float]:
    start = time.monotonic()
    proc = subprocess.run(
        [binary, "--threads", str(threads), "scan", "-q", "-f", "json", str(checkout)],
        capture_output=True,
    )
    return proc.returncode, proc.stdout, time.monotonic() - start


def findings_at(doc: dict, rule: str, path: str, line: int | None) -> list[dict]:
    out = []
    for f in doc["findings"]:
        loc = f.get("location") or {}
        if f["rule_id"] == rule and loc.get("path") == path and (line is None or loc.get("line") == line):
            out.append(f)
    return out


def check(doc: dict, want: dict) -> list[str]:
    problems = []
    for m in want["must"]:
        hits = findings_at(doc, m["rule"], m["path"], m.get("line"))
        where = f'{m["rule"]} at {m["path"]}:{m.get("line", "*")}'
        if not hits:
            problems.append(f"missing {where}")
            continue
        cats = {c for f in hits for c in f["categories"]}
        lacking = [c for c in m.get("categories", []) if c not in cats]
        if lacking:
            problems.append(f"{where} lacks categories {lacking} (has {sorted(cats)})")
    for m in want["must_not"]:
        floor = SEVERITY[m.get("min_severity", "info")]
        hits = [
            f
            for f in findings_at(doc, m["rule"], m["path"], m.get("line"))
            if SEVERITY[f["severity"]] >= floor
        ]
        if hits:
            problems.append(f'unexpected {m["rule"]} at {m["path"]}:{m.get("line", "*")}: {hits[0]["message"]}')
    sources = {s["id"]: s for s in doc["sources"]}
    sinks = {s["id"]: s for s in doc["sinks"]}
    for m in want["must_flows"]:
        found = False
        for fl in doc["flows"]:
            src, snk = sources[fl["source"]]["location"], sinks[fl["sink"]]["location"]
            if (
                (src["path"], src["line"]) == (m["source"]["path"], m["source"]["line"])
                and (snk["path"], snk["line"]) == (m["sink"]["path"], m["sink"]["line"])
                and m.get("category", fl["category"]) == fl["category"]
            ):
                found = True
                break
        if not found:
            problems.append(
                f'missing flow {m["source"]["path"]}:{m["source"]["line"]} -> '
                f'{m["sink"]["path"]}:{m["sink"]["line"]} {m.get("category", "")}'
            )
    gaps = doc["coverage"]["gaps"]
    if len(gaps) > want["max_gaps"]:
        problems.append(f'{len(gaps)} gaps, more than {want["max_gaps"]}')
    parse_errors = 0
    for g in gaps:
        if g["kind"] not in want["allowed_gap_kinds"]:
            problems.append(f'gap of kind {g["kind"]}: {g["detail"]}')
        if g["kind"] == "parse_error":
            parse_errors += 1
        if g["kind"] == "unsupported_language" and any(
            loc["path"].endswith((".cs", ".razor")) for loc in g["locations"]
        ):
            problems.append(f'C# reported as unsupported: {g["detail"]}')
    if parse_errors > want["max_parse_error_files"]:
        problems.append(f"{parse_errors} files with parse errors, more than {want['max_parse_error_files']}")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("binary")
    parser.add_argument("--checkout", type=pathlib.Path, help="an existing checkout at the pinned commit")
    parser.add_argument("--report", type=pathlib.Path, help="write the scan's JSON document here")
    args = parser.parse_args()
    want = json.loads(EXPECTED.read_text())
    with tempfile.TemporaryDirectory() as tmp:
        checkout = args.checkout
        if checkout is None:
            checkout = pathlib.Path(tmp) / "eshop"
            fetch(want["repository"], want["commit"], checkout)
        head = subprocess.run(
            ["git", "-C", str(checkout), "rev-parse", "HEAD"], capture_output=True, text=True
        ).stdout.strip()
        if head and head != want["commit"]:
            print(f"checkout is at {head}, expected {want['commit']}", file=sys.stderr)
            return 1
        code1, out1, secs1 = scan(args.binary, checkout, 1)
        code8, out8, secs8 = scan(args.binary, checkout, 8)
    problems = []
    for code in (code1, code8):
        if code not in (0, 4):
            problems.append(f"piiflow exited {code}")
    if out1 != out8:
        problems.append("one thread and eight threads gave different documents")
    if max(secs1, secs8) > want["max_seconds"]:
        problems.append(f"took {max(secs1, secs8):.1f} s, more than {want['max_seconds']} s")
    if args.report:
        args.report.write_bytes(out1)
    if not problems:
        doc = json.loads(out1)
        problems = check(doc, want)
        s = doc["summary"]
        print(f"eShop @ {want['commit'][:12]}: {s['findings']} findings, {s['flows']} flows, "
              f"{s['gaps']} gaps; {secs1:.1f} s (1 thread), {secs8:.1f} s (8 threads)")
    for p in problems:
        print(f"::error::{p}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
