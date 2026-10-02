#!/usr/bin/env python3
"""Write or check conformance/CORPUS-DIGESTS.txt and conformance/MANIFEST.json.

CORPUS-DIGESTS.txt lists the SHA-256 of every file in the corpus, one `<digest>  <path>` per
line in path order; the release workflow attests it, so a published corpus can be verified.
MANIFEST.json lists the vectors with their language, status and expected flow count.
"""

import hashlib
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
VECTORS = HERE / "vectors"


def digests():
    lines = []
    for p in sorted(VECTORS.rglob("*")):
        if p.is_file():
            rel = p.relative_to(HERE).as_posix()
            lines.append(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {rel}")
    return "\n".join(lines) + "\n"


def manifest():
    vectors = []
    for v in sorted(p for p in VECTORS.iterdir() if (p / "vector.json").is_file()):
        meta = json.loads((v / "vector.json").read_text())
        expected = [l for l in (v / "expected.jsonl").read_text().splitlines() if l.strip()]
        vectors.append({"name": v.name, "language": meta["language"], "status": meta["status"],
                        "description": meta["description"], "expected_flows": len(expected)})
    return json.dumps({"corpus": "privacy-flow conformance", "version": "0.1", "vectors": vectors},
                      indent=2, sort_keys=True) + "\n"


def main():
    want = {"CORPUS-DIGESTS.txt": digests(), "MANIFEST.json": manifest()}
    if "--check" in sys.argv:
        bad = [name for name, text in want.items() if (HERE / name).read_text() != text]
        if bad:
            print(f"out of date: {', '.join(bad)} (run python3 conformance/digests.py)", file=sys.stderr)
            return 1
        print("corpus digests and manifest match the files on disk")
        return 0
    for name, text in want.items():
        (HERE / name).write_text(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
