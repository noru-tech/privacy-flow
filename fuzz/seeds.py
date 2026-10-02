#!/usr/bin/env python3
"""Build the seed corpus for each fuzz target from the test fixtures and the demo.

    python3 fuzz/seeds.py                write fuzz/corpus/<target>/ (for `cargo fuzz run`)
    python3 fuzz/seeds.py --zip DIR      write DIR/<target>_seed_corpus.zip (for ClusterFuzzLite)

Each seed uses the target's input layout: see the comment at the top of each file in
fuzz/fuzz_targets/. Standard library only.
"""

from __future__ import annotations

import argparse
import hashlib
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TREES = [ROOT / "tests" / "fixtures", ROOT / "examples"]

# Must match the tables in fuzz_targets/lower.rs and fuzz_targets/scan.rs.
LOWER_EXT = {".ts": 0, ".tsx": 1, ".js": 2, ".py": 3}
SCAN_EXT = {".ts": 0, ".tsx": 2, ".js": 3, ".py": 4}
SCAN_PACKAGE_JSON, SCAN_CONFIG, SCAN_MAX_FILES = 6, 7, 8

DATAMAPS = [
    b"\x00dataset:\n  - fides_key: app\n    collections:\n      - name: users\n        fields:\n"
    b"          - name: email\n            data_categories: [user.contact.email]\n"
    b"          - name: address\n            fields:\n              - name: city\n"
    b"                data_categories: [user.contact.address.city]\n",
    b'\x01{"dataset": [{"fides_key": "app", "collections": [{"name": "users", "fields": '
    b'[{"name": "phone", "data_categories": ["user.contact.phone_number"]}]}]}]}',
]


def scan_seed(directory: Path) -> bytes | None:
    chunks = []
    for path in sorted(p for p in directory.rglob("*") if p.is_file()):
        if path.name == ".privacy-flow.yml":
            pick = SCAN_CONFIG
        elif path.name == "package.json":
            pick = SCAN_PACKAGE_JSON
        elif path.suffix in SCAN_EXT:
            pick = SCAN_EXT[path.suffix]
        else:
            continue
        body = path.read_bytes()
        if b"\x00" not in body:
            chunks.append(bytes([pick]) + body)
    return b"\x00".join(chunks[:SCAN_MAX_FILES]) if chunks else None


def seeds() -> dict[str, list[bytes]]:
    out: dict[str, list[bytes]] = {"lower": [], "scan": [], "config": [], "datamap": list(DATAMAPS)}
    for tree in TREES:
        for path in sorted(p for p in tree.rglob("*") if p.is_file()):
            if path.suffix in LOWER_EXT:
                out["lower"].append(bytes([LOWER_EXT[path.suffix]]) + path.read_bytes())
            elif path.name == ".privacy-flow.yml":
                out["config"].append(path.read_bytes())
        # One scan seed per fixture: the directories that hold a fixture.json, and the demo.
        for marker in sorted(tree.rglob("fixture.json")):
            if seed := scan_seed(marker.parent):
                out["scan"].append(seed)
    if seed := scan_seed(ROOT / "examples" / "demo"):
        out["scan"].append(seed)
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--zip", metavar="DIR", type=Path, help="write <target>_seed_corpus.zip files here")
    args = parser.parse_args()
    for target, items in seeds().items():
        names = {hashlib.sha1(item).hexdigest(): item for item in items}
        if args.zip:
            args.zip.mkdir(parents=True, exist_ok=True)
            with zipfile.ZipFile(args.zip / f"{target}_seed_corpus.zip", "w") as zf:
                for name, item in sorted(names.items()):
                    zf.writestr(name, item)
        else:
            directory = ROOT / "fuzz" / "corpus" / target
            directory.mkdir(parents=True, exist_ok=True)
            for name, item in names.items():
                (directory / name).write_bytes(item)
        print(f"{target}: {len(names)} seeds")
    return 0


if __name__ == "__main__":
    sys.exit(main())
