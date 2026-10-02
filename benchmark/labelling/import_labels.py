#!/usr/bin/env python3
"""Turn a labelling-app export into the label files score.py reads.

    python3 benchmark/labelling/import_labels.py <export.json> [--force] [--labels-dir DIR]

Writes benchmark/labels/<R1|R2>/<app>.yml for every application in the export, in the same
shape as the templates sheets.py writes. Every value is checked against PROTOCOL.md §3; an
unknown item id or value stops the import before anything is written. An existing file with
more labels filled in than the export would replace is kept unless --force is given, so an
older export cannot overwrite newer work.
"""
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
BENCH = HERE.parent
FORMAT = "privacy-flow-labels/1"
ALLOWED = {
    "flow": {"verdict": {"tp", "fp_path", "fp_not_personal", "unsure"}, "category_ok": {"yes", "no"}},
    "gap": {"hides_flow": {"yes", "no", "unsure"}},
    "site": {"is_sink": {"yes", "no"}, "personal": {"yes", "no", "unsure"}},
}
PRIMARY = {"flow": "verdict", "gap": "hides_flow", "site": "is_sink"}


def scalar(v):
    if v is None or v == "":
        return ""
    return json.dumps(v, ensure_ascii=False)


def filled_in(path):
    """Primary fields filled in an existing label file (plain text scan)."""
    if not path.exists():
        return 0
    n = 0
    for line in path.read_text().splitlines():
        s = line.strip()
        for field in PRIMARY.values():
            if s.startswith(field + ":") and s.split(":", 1)[1].split("#", 1)[0].strip():
                n += 1
    return n


def render(app, items, labels, added):
    out = [f"# Labels for {app}, imported from the labelling app. See benchmark/PROTOCOL.md §3.", "items:"]
    for it in items:
        lab = labels.get(it["id"], {})
        out.append(f"  {it['id']}:")
        if it["kind"] == "flow":
            fields = ["verdict", "category_ok", "citation", "note"]
        elif it["kind"] == "gap":
            fields = ["hides_flow", "citation", "note"]
        else:
            fields = ["is_sink", "personal", "categories", "source", "note"]
        for f in fields:
            if f == "categories":
                out.append(f"    categories: {json.dumps(lab.get('categories') or [], ensure_ascii=False)}")
            else:
                v = scalar(lab.get(f))
                out.append(f"    {f}:{' ' + v if v else ''}")
    out.append("added_sites:" + ("" if added else " []"))
    for s in added:
        out.append(f"  - {json.dumps(s, ensure_ascii=False, sort_keys=True)}")
    return "\n".join(out) + "\n"


def main():
    argv = sys.argv[1:]
    labels_dir = BENCH / "labels"
    if "--labels-dir" in argv:
        i = argv.index("--labels-dir")
        labels_dir = pathlib.Path(argv[i + 1])
        del argv[i:i + 2]
    args = [a for a in argv if not a.startswith("--")]
    force = "--force" in argv
    if len(args) != 1:
        sys.exit(__doc__)
    export = json.loads(pathlib.Path(args[0]).read_text())
    if export.get("format") != FORMAT:
        sys.exit(f"not a labelling export (format {export.get('format')!r}, expected {FORMAT!r})")
    reviewer = export.get("reviewer")
    if reviewer not in ("R1", "R2"):
        sys.exit(f"the export names reviewer {reviewer!r}; expected R1 or R2")

    plans, errors = [], []
    for app, body in sorted(export.get("apps", {}).items()):
        sheet = BENCH / "sheets" / f"{app}.items.jsonl"
        if not sheet.exists():
            errors.append(f"{app}: not an application in the corpus")
            continue
        items = [json.loads(l) for l in sheet.read_text().splitlines()]
        kinds = {it["id"]: it["kind"] for it in items}
        labels = body.get("items", {})
        for item_id, lab in labels.items():
            kind = kinds.get(item_id)
            if kind is None:
                errors.append(f"{app}: unknown item {item_id}")
                continue
            for field, allowed in ALLOWED[kind].items():
                v = lab.get(field)
                if v not in (None, "") and v not in allowed:
                    errors.append(f"{app} {item_id}: {field} {v!r} is not one of {sorted(allowed)}")
        target = labels_dir / reviewer / f"{app}.yml"
        new = sum(1 for i, lab in labels.items() if lab.get(PRIMARY[kinds.get(i, "flow")]))
        old = filled_in(target)
        if old > new and not force:
            errors.append(f"{app}: {target} has {old} labels filled in, the export only {new}; rerun with --force to replace it")
        plans.append((target, render(app, items, labels, body.get("added_sites") or []), new, len(items)))
    if errors:
        sys.exit("nothing written:\n  " + "\n  ".join(errors))
    for target, text, new, total in plans:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
        print(f"{target}: {new} of {total} labelled")


if __name__ == "__main__":
    main()
