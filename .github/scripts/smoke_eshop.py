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
  unsupported language;
- `piiflow datamap` writes the same map with one thread and eight, with the systems, datasets
  and edges `tests/smoke/eshop.datamap.expected.json` requires, and a second run with
  `--check` finds it up to date.

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
EXPECTED_MAP = ROOT / "tests" / "smoke" / "eshop.datamap.expected.json"
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


def datamap(binary: str, checkout: pathlib.Path, threads: int, out: pathlib.Path, *extra: str) -> int:
    return subprocess.run(
        [binary, "--threads", str(threads), "-q", "datamap", "--no-manifest", str(checkout),
         "-o", str(out / "datamap.yml"), "--facts", str(out / "facts.json"), *extra],
        capture_output=True,
    ).returncode


def field_categories(fields: list[dict], prefix: str = "") -> dict[str, list[str]]:
    out = {}
    for f in fields:
        name = prefix + f["name"]
        out[name] = f["categories"]
        out.update(field_categories(f["fields"], name + "."))
    return out


def check_map(topo: dict, sinks: dict, want: dict) -> list[str]:
    problems = []
    systems = {s["key"]: s for s in topo["systems"]}
    if sorted(systems) != sorted(want["systems"]):
        problems.append(f"systems {sorted(systems)}, expected {sorted(want['systems'])}")
    for key, alias in want["aliases"].items():
        if alias not in systems.get(key, {}).get("aliases", []):
            problems.append(f"{key} is not reached as {alias}")
    datasets = {d["key"]: d for d in topo["datasets"]}
    for d in want["datasets"]:
        got = datasets.get(d["key"])
        if got is None:
            problems.append(f"missing dataset {d['key']}")
            continue
        if d["key"] not in systems.get(d["used_by"], {}).get("datasets", []):
            problems.append(f"{d['used_by']} does not use {d['key']}")
        if "database" in d and got["database"] != d["database"]:
            problems.append(f"{d['key']} names database {got['database']}, expected {d['database']}")
        collections = {c["name"]: c for c in got["collections"]}
        for cname, fields in d["fields"].items():
            if cname not in collections:
                problems.append(f"{d['key']} has no collection {cname}")
                continue
            have = field_categories(collections[cname]["fields"])
            for fname, cats in fields.items():
                if fname not in have:
                    problems.append(f"{d['key']}.{cname} has no field {fname}")
                elif not set(cats) <= set(have[fname]):
                    problems.append(f"{d['key']}.{cname}.{fname} has {have[fname]}, expected {cats}")
    edges = [(e["from"], e["to"].get("key"), set(e["categories"]), e) for e in topo["edges"]]
    for e in want["edges"]:
        hits = [x for x in edges if x[0] == e["from"] and x[1] == e["to"]]
        if not hits:
            problems.append(f"missing edge {e['from']} -> {e['to']}")
        elif not set(e.get("categories", [])) <= set().union(*(h[2] for h in hits)):
            problems.append(f"edge {e['from']} -> {e['to']} lacks {e.get('categories')}")
    for m in want["no_system_edges_from"]:
        for frm, to, _, e in edges:
            if to and any(sinks[s]["location"]["path"] == m["path"] for s in e["sinks"]):
                problems.append(f"unexpected system edge {frm} -> {to} from {m['path']}")
    return problems


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
        map_problems = []
        one, eight = pathlib.Path(tmp) / "one", pathlib.Path(tmp) / "eight"
        for d in (one, eight):
            d.mkdir()
        start = time.monotonic()
        codes = [datamap(args.binary, checkout, 1, one), datamap(args.binary, checkout, 8, eight)]
        map_secs = time.monotonic() - start
        if any(c not in (0, 4) for c in codes):
            map_problems.append(f"piiflow datamap exited {codes}")
        elif (one / "datamap.yml").read_bytes() != (eight / "datamap.yml").read_bytes():
            map_problems.append("datamap: one thread and eight threads gave different maps")
        else:
            again = datamap(args.binary, checkout, 1, one, "--check")
            if again not in (0, 4):
                map_problems.append(f"datamap --check after a write exited {again}")
            facts = json.loads((one / "facts.json").read_text())
            sinks = {s["id"]: s for s in facts["sinks"]}
            map_problems += check_map(facts["topology"], sinks, json.loads(EXPECTED_MAP.read_text()))
            t = facts["topology"]
            print(f"eShop data map: {len(t['systems'])} systems, {len(t['datasets'])} datasets, "
                  f"{len(t['edges'])} edges, {len(t['declarations'])} declarations; {map_secs:.1f} s for two runs")
    problems = list(map_problems)
    for code in (code1, code8):
        if code not in (0, 4):
            problems.append(f"piiflow exited {code}")
    if out1 != out8:
        problems.append("one thread and eight threads gave different documents")
    if max(secs1, secs8) > want["max_seconds"]:
        problems.append(f"took {max(secs1, secs8):.1f} s, more than {want['max_seconds']} s")
    if args.report:
        args.report.write_bytes(out1)
    if not [p for p in problems if p not in map_problems]:
        doc = json.loads(out1)
        problems += check(doc, want)
        s = doc["summary"]
        print(f"eShop @ {want['commit'][:12]}: {s['findings']} findings, {s['flows']} flows, "
              f"{s['gaps']} gaps; {secs1:.1f} s (1 thread), {secs8:.1f} s (8 threads)")
    for p in problems:
        print(f"::error::{p}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
