#!/usr/bin/env python3
"""Generate a TypeScript program whose summaries reach thousands of sinks, deterministically.

    python3 benches/wide_summaries.py /tmp/wide [leaves] [entry points] [chain]

The shape that made the engine quadratic on polarsource/polar: searches that reach many sinks
within the depth bound and the same sinks again beyond it. `hub` logs its argument through
`leaves` leaf functions; every link of a `chain`-long chain calls `hub` directly (each hit
found) and the next link (each hit cut by the bound, `max_call_depth: 2`). Checking each cut
against the hits found used to be a scan per cut. With the defaults, the engine phase took
about 14 s before the fix and under 0.5 s after it on an Apple M3 Pro; the performance budget
(.github/scripts/perf_budget.py) sits between the two.
"""

import pathlib
import sys


def main():
    out = pathlib.Path(sys.argv[1])
    leaves = int(sys.argv[2]) if len(sys.argv) > 2 else 12000
    entries = int(sys.argv[3]) if len(sys.argv) > 3 else 4
    chain = int(sys.argv[4]) if len(sys.argv) > 4 else 64
    out.mkdir(parents=True, exist_ok=True)
    # A direct `hub` call reaches each sink within two call boundaries; through the next link
    # it takes more, so every link's search both finds and cuts every hit.
    (out / ".privacy-flow.yml").write_text("version: 1\nmax_call_depth: 2\n")
    lines = ["import pino from 'pino';", "const logger = pino();", ""]
    for i in range(leaves):
        lines.append(f"export function leaf{i}(x: string) {{ logger.info(x); }}")
    lines.append("")
    lines.append("export function hub(x: string) {")
    lines += [f"  leaf{i}(x);" for i in range(leaves)]
    lines.append("}")
    (out / "hub.ts").write_text("\n".join(lines) + "\n")

    chain_src = ["import { hub } from './hub';", ""]
    for i in range(chain):
        callee = f"link{i + 1}" if i + 1 < chain else "hub"
        # Each link reaches every sink directly (found) and again through the rest of the
        # chain (cut once the chain passes the bound), in the same summary search.
        chain_src.append(f"export function link{i}(x: string) {{ hub(x); {callee}(x); }}")
    (out / "chain.ts").write_text("\n".join(chain_src) + "\n")

    entry_src = ["import { hub } from './hub';", "import { link0 } from './chain';", ""]
    for i in range(entries):
        entry_src += [
            f"export function entry{i}(user: {{ email: string }}) {{",
            "  hub(user.email);",
            "  link0(user.email);",
            "}",
        ]
    (out / "entries.ts").write_text("\n".join(entry_src) + "\n")


if __name__ == "__main__":
    main()
