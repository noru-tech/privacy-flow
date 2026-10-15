"""Load every data map golden with fideslang's own model (Fideslang 3.1.3).

    python3 .github/scripts/fides_crosscheck.py tests/goldens

`piiflow datamap` writes Fides data maps by hand and checks the rules it knows offline
(src/datamap/validate.rs); this checks them with Fides itself: required fields, key patterns, and
declarations that name only their system's DataFlows. Needs the packages in
fides-requirements.txt.
"""

import pathlib
import sys

import yaml
from fideslang.models import Taxonomy


def main() -> int:
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "tests/goldens")
    maps = sorted(root.glob("*.datamap.yml"))
    if not maps:
        print(f"no *.datamap.yml under {root}")
        return 1
    failed = 0
    for path in maps:
        try:
            t = Taxonomy.model_validate(yaml.safe_load(path.read_text()))
        except Exception as e:  # noqa: BLE001 - report every map's error
            failed += 1
            print(f"FAIL {path}: {e}")
            continue
        print(f"ok   {path}: {len(t.system)} system(s), {len(t.dataset)} dataset(s)")
    print(f"{len(maps) - failed} of {len(maps)} data maps load with fideslang")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
