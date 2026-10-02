#!/usr/bin/env python3
"""Fail on a large performance regression.

Two cases, each scanned with the release binary:

- the 100k-line synthetic TypeScript service (benches/synthetic.py), against wall time and
  peak memory;
- wide summaries (benches/wide_summaries.py), the shape that made the engine quadratic on
  polarsource/polar, against the engine phase's time (about 0.45 s fixed, 16 s quadratic, on
  an Apple M3 Pro).

The budgets are deliberately loose (shared CI runners vary by 2x or more); they catch an
accidental quadratic, not a few percent. Measured numbers are in NOTES.md.

    python3 .github/scripts/perf_budget.py target/release/piiflow
"""

import os
import pathlib
import re
import resource
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
BUDGET_SECONDS = float(os.environ.get("PIIFLOW_BUDGET_SECONDS", "10"))
BUDGET_MIB = float(os.environ.get("PIIFLOW_BUDGET_MIB", "1500"))
BUDGET_WIDE_ENGINE_SECONDS = float(os.environ.get("PIIFLOW_BUDGET_WIDE_ENGINE_SECONDS", "4"))


def wide_summaries(binary):
    """The engine phase on wide summaries; returns an error message or None."""
    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run([sys.executable, str(ROOT / "benches/wide_summaries.py"), tmp], check=True)
        proc = subprocess.run([binary, "scan", tmp, "--walk", "--timings", "-o", os.devnull], capture_output=True, text=True)
    if proc.returncode not in (0, 4):
        return f"wide summaries: piiflow exited {proc.returncode}\n{proc.stderr}"
    m = re.search(r"engine (\d+) ms", proc.stderr)
    if not m:
        return "wide summaries: no engine timing in the output"
    engine = int(m.group(1)) / 1000
    print(f"wide summaries: engine {engine:.2f} s")
    if engine > BUDGET_WIDE_ENGINE_SECONDS:
        return f"wide summaries: engine took {engine:.2f} s, over the {BUDGET_WIDE_ENGINE_SECONDS:.0f} s budget"
    return None


def main():
    binary = sys.argv[1] if len(sys.argv) > 1 else str(ROOT / "target/release/piiflow")
    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run([sys.executable, str(ROOT / "benches/synthetic.py"), tmp, "100000"], check=True)
        start = time.monotonic()
        proc = subprocess.run([binary, "scan", tmp, "--walk", "--timings", "-o", os.devnull], capture_output=True, text=True)
        wall = time.monotonic() - start
    if proc.returncode not in (0, 4):
        print(proc.stderr, file=sys.stderr)
        return 1
    peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    # ru_maxrss is KiB on Linux and bytes on macOS.
    peak_mib = peak / 1024 if sys.platform != "darwin" else peak / (1024 * 1024)
    timings = next((l for l in proc.stderr.splitlines() if l.startswith("timings:")), "")
    print(f"100k-line synthetic service: {wall:.2f} s wall, {peak_mib:.0f} MiB peak")
    print(timings)
    ok = True
    if wall > BUDGET_SECONDS:
        print(f"::error::scan took {wall:.2f} s, over the {BUDGET_SECONDS:.0f} s budget", file=sys.stderr)
        ok = False
    if peak_mib > BUDGET_MIB:
        print(f"::error::peak memory {peak_mib:.0f} MiB, over the {BUDGET_MIB:.0f} MiB budget", file=sys.stderr)
        ok = False
    error = wide_summaries(binary)
    if error:
        print(f"::error::{error}", file=sys.stderr)
        ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
