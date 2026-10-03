#!/usr/bin/env python3
"""Write labels/final/<app>.yml from R1's and R2's labels (PROTOCOL.md §7, step 6, as amended).

    python3 benchmark/labelling/finalise.py

The protocol settles disagreements by discussion. These final labels were made without one (see
PROTOCOL.md, Deviations, 2026-10-03): an item both reviewers answered the same way takes that
answer; an item they answered differently, or that one of them left blank, is `unsure` with the
reason, so score.py leaves it out of every denominator and counts it. An application is
finalised only when both reviewers labelled every one of its items; the others get no final
file and no scores.

What counts as the same answer is what score.py reads: a flow's `verdict` (and `category_ok`
when both say `tp`), a gap's `hides_flow`, a site's `is_sink` and, for a sink, `personal`.
"""
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
BENCH = HERE.parent


def norm(v):
    if isinstance(v, bool):
        return "yes" if v else "no"
    return str(v).strip().lower() if v is not None else None


def answer(kind, label):
    """The parts of a label score.py reads, normalised; None if the item is unanswered."""
    if kind == "flow":
        v = norm(label.get("verdict"))
        if v is None:
            return None
        return (v, norm(label.get("category_ok")) if v == "tp" else None)
    if kind == "gap":
        v = norm(label.get("hides_flow"))
        return None if v is None else (v,)
    s = norm(label.get("is_sink"))
    if s is None:
        return None
    return (s, norm(label.get("personal")) if s == "yes" else None)


def unsure(kind, reason):
    field = {"flow": "verdict", "gap": "hides_flow", "site": "personal"}[kind]
    out = {field: "unsure", "note": reason}
    if kind == "site":
        out["is_sink"] = "unsure"
    return out


def finalise(app, yaml):
    items = [json.loads(l) for l in (BENCH / "sheets" / f"{app}.items.jsonl").read_text().splitlines()]
    labels = []
    for r in ("R1", "R2"):
        path = BENCH / "labels" / r / f"{app}.yml"
        doc = (yaml.safe_load(path.read_text()) or {}) if path.exists() else {}
        labels.append(doc.get("items") or {})
    answered = [sum(1 for it in items if answer(it["kind"], l.get(it["id"]) or {}) is not None) for l in labels]
    if min(answered) < len(items):
        return None, answered, len(items)
    final, agreed, differ = {}, 0, 0
    for it in items:
        a, b = (l[it["id"]] for l in labels)
        if answer(it["kind"], a) == answer(it["kind"], b):
            final[it["id"]] = {k: v for k, v in a.items() if k != "note"}
            agreed += 1
        else:
            final[it["id"]] = unsure(it["kind"], "R1 and R2 disagree; not settled by discussion")
            differ += 1
    return {"agreed": agreed, "differ": differ, "items": final}, answered, len(items)


def main():
    try:
        import yaml
    except ImportError:
        sys.exit("finalise.py needs PyYAML (pip install pyyaml)")
    corpus = json.loads((BENCH / "corpus.json").read_text())
    out_dir = BENCH / "labels" / "final"
    out_dir.mkdir(parents=True, exist_ok=True)
    for entry in corpus["repositories"]:
        app = entry["name"]
        result, answered, total = finalise(app, yaml)
        target = out_dir / f"{app}.yml"
        if result is None:
            if target.exists():
                target.unlink()
            print(f"{app}: not finalised (R1 {answered[0]}/{total}, R2 {answered[1]}/{total} answered)")
            continue
        header = ("# Final labels for {app}, from R1 and R2 by benchmark/labelling/finalise.py: {a} items both\n"
                  "# answered the same way, {d} they did not (unsure, not settled by discussion). See PROTOCOL.md.\n")
        body = yaml.safe_dump({"items": result["items"]}, sort_keys=True, allow_unicode=True, width=100)
        target.write_text(header.format(app=app, a=result["agreed"], d=result["differ"]) + body)
        print(f"{app}: {result['agreed']} agreed, {result['differ']} unsure")


if __name__ == "__main__":
    main()
