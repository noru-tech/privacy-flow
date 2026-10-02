#!/usr/bin/env python3
"""Break one analysis semantic in the working tree, for the `conformance catches` CI job.

A corpus that a broken analyser also passes proves nothing. Each break below disables one
thing the corpus is meant to test; CI builds the broken binary and requires the corpus to fail.
Never commit the result.
"""

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]

BREAKS = {
    # Field sensitivity: a field read sees every field.
    "field-sensitivity": ("src/engine/worklist.rs", "if t == TOP || t == named(k) {", "if true {"),
    # Interprocedural flow: calls to local functions bind no arguments.
    "summaries": ("src/facts.rs", "self.bind_args(site, f, bound, args);", "let _ = (site, bound, args);"),
    # Instances: every instance carries every construction's data again.
    "instances": ("src/facts.rs", "if class.ctor_this.is_none() {", "if true {"),
    # Sanitisers: predicates no longer remove anything.
    "sanitisers": ("src/facts.rs", "if s.def.language == lang && s.def.methods.iter().any(|m| m == name) {", "if false {"),
    # Stores: a field write loses which field it was.
    "stores": ("src/engine/worklist.rs", "EdgeKind::Store(k) => Some(named(k)),", "EdgeKind::Store(_k) => Some(TOP),"),
}


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in BREAKS:
        print(f"usage: break_engine.py {{{','.join(sorted(BREAKS))}}}", file=sys.stderr)
        return 2
    path, old, new = BREAKS[sys.argv[1]]
    p = ROOT / path
    text = p.read_text()
    if old not in text:
        print(f"{path}: anchor not found; update BREAKS for {sys.argv[1]}", file=sys.stderr)
        return 1
    p.write_text(text.replace(old, new, 1))
    print(f"broke {sys.argv[1]} in {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
