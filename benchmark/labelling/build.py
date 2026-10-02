#!/usr/bin/env python3
"""Build the labelling app's data: one JSON file per application, with code context.

    python3 benchmark/labelling/build.py <out dir>

Reads the committed items (benchmark/sheets/<app>.items.jsonl) and the fetched applications
(.benchmark-cache/, from fetch.py), and writes <out>/items/<app>.json plus <out>/apps.json. Each
hop, gap location and call site carries the lines around it and a link to that line on GitHub at
the pinned commit. Never reads benchmark/sheets/key/: the app must not know which tool reported
a flow. The output holds excerpts of the applications' code, so it is published with the app and
never committed.
"""
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
BENCH = HERE.parent
CACHE = BENCH.parent / ".benchmark-cache"
CONTEXT = {"flow": 2, "gap": 2, "site": 4}
MAX_LINE = 220


def main():
    out = pathlib.Path(sys.argv[1])
    (out / "items").mkdir(parents=True, exist_ok=True)
    corpus = json.loads((BENCH / "corpus.json").read_text())
    apps = []
    for entry in corpus["repositories"]:
        name = entry["name"]
        root = CACHE / name / entry["scope"]
        files = {}

        def lines_of(path):
            if path not in files:
                try:
                    files[path] = (root / path).read_text(encoding="utf-8", errors="replace").splitlines()
                except OSError:
                    files[path] = []
            return files[path]

        def where(path, line, ctx):
            src = lines_of(path)
            lo, hi = max(1, (line or 1) - ctx), min(len(src), (line or 1) + ctx)
            scope = "" if entry["scope"] == "." else entry["scope"].rstrip("/") + "/"
            return {
                "path": path, "line": line,
                "url": f"https://github.com/{entry['repository']}/blob/{entry['commit']}/{scope}{path}" + (f"#L{line}" if line else ""),
                "first": lo,
                "code": [l if len(l) <= MAX_LINE else l[: MAX_LINE - 1] + "…" for l in src[lo - 1: hi]],
            }

        items = []
        for raw in (BENCH / "sheets" / f"{name}.items.jsonl").read_text().splitlines():
            it = json.loads(raw)
            ctx = CONTEXT[it["kind"]]
            if it["kind"] == "flow":
                n = len(it["hops"])
                items.append({"id": it["id"], "kind": "flow", "categories": it["categories"], "sink_class": it["sink_class"],
                              "hops": [dict(where(h["path"], h["line"], ctx), column=h["column"],
                                            role="source" if i == 0 else "sink" if i == n - 1 else "step")
                                       for i, h in enumerate(it["hops"])]})
            elif it["kind"] == "gap":
                items.append({"id": it["id"], "kind": "gap", "gap_kind": it["gap_kind"], "detail": it["detail"],
                              "sources": it["reached_by_sources"],
                              "locations": [where(l["path"], l["line"], ctx) for l in it["locations"]]})
            else:
                items.append({"id": it["id"], "kind": "site", "callee": it["callee"], "class": it["class"],
                              "at": dict(where(it["path"], it["line"], ctx), column=it["column"])})
        (out / "items" / f"{name}.json").write_text(json.dumps({"app": name, "items": items}, separators=(",", ":"), ensure_ascii=False))
        apps.append({"name": name, "repository": entry["repository"], "commit": entry["commit"], "scope": entry["scope"],
                     "language": entry["language"], "items": len(items)})
    (out / "apps.json").write_text(json.dumps({"apps": apps}, indent=1))
    total = sum((out / "items" / f"{a['name']}.json").stat().st_size for a in apps)
    print(f"{len(apps)} applications, {sum(a['items'] for a in apps)} items, {total / 1e6:.1f} MB of item data")


if __name__ == "__main__":
    main()
