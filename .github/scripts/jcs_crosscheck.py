#!/usr/bin/env python3
"""Recompute privacy-flow's canonical bytes and digests with an independent RFC 8785 library.

Every JSON document piiflow writes is the RFC 8785 (JCS) serialization of its value, and its
`digest` is SHA-256 over the JCS bytes of the document with `digest` empty and every finding's
`disposition` null (docs/output.md). This script checks both claims against the committed goldens
without any of piiflow's code: it parses each golden with Python's json module, serializes it with
the `rfc8785` package, and compares.

Usage: python jcs_crosscheck.py [GOLDENS_DIR]
"""

import hashlib
import json
import pathlib
import sys

import rfc8785


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "tests/goldens")
    errors = []
    checked = 0
    for path in sorted(root.glob("*.json")) + sorted(root.glob("*.sarif")):
        raw = path.read_bytes()
        value = json.loads(raw)
        if rfc8785.dumps(value) != raw:
            errors.append(f"{path.name}: bytes are not the RFC 8785 serialization of the document")
        checked += 1
        if path.suffix == ".json" and "digest" in value:
            doc = json.loads(raw)
            doc["digest"] = ""
            for f in doc["findings"]:
                f["disposition"] = None
            want = "sha256:" + hashlib.sha256(rfc8785.dumps(doc)).hexdigest()
            if want != value["digest"]:
                errors.append(f"{path.name}: digest {value['digest']} but the canonical bytes hash to {want}")
            checked += 1
    for e in errors:
        print(e, file=sys.stderr)
    print(f"{checked} checks, {len(errors)} failures")
    return 1 if errors or checked == 0 else 0


if __name__ == "__main__":
    sys.exit(main())
