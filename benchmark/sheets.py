#!/usr/bin/env python3
"""Draw the labelling samples, blind them, and write label templates (PROTOCOL.md §5–§7).

    python3 benchmark/sheets.py [--cache .benchmark-cache] [--allow-without-privado]

Inputs: corpus.json; each application's piiflow document (run.py); Privado's mapped flows in
benchmark/results/privado/<app>.jsonl; candidate sites (candidates.py).

Outputs:
  benchmark/sheets/<app>.items.jsonl   the items: locations, categories, sink class; no code
  benchmark/sheets/key/<app>.json      item → tool, native ids, strata, stratum sizes (reviewers
                                       do not open this until labelling is done)
  <cache>/sheets/<app>.md              the rendered sheet, with each hop's line of code read
                                       from the fetched application (not committed)
  benchmark/labels/{R1,R2}/<app>.yml   empty label templates, written only if absent

Everything is deterministic: the same inputs give the same samples, ids and order.
"""
import argparse
import hashlib
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
SALT = "privacy-flow-benchmark-v1"

K_PER_RULE = 8
K_INFO = 10
K_GAPS = 10
K_PRIVADO_PER_TYPE = 8
K_SITES = 30
K_SITES_LOG = 12
RULES = ["PF001", "PF002", "PF003", "PF004", "PF005", "PF006"]

# Coarse sink classes shared by both tools' items, so the class does not reveal the tool.
COARSE = {"log": "log", "llm": "llm", "http": "http", "browser_storage": "browser_storage",
          "error_tracking": "third_party", "analytics": "third_party", "messaging": "third_party",
          "third_party": "third_party", "other": "other"}


def h(*parts):
    return hashlib.sha256("\x1f".join((SALT, *parts)).encode()).hexdigest()


def collapse(hops):
    """Drop hops on the same line as the hop before them, keeping the source and the sink.

    Applied to both tools' paths alike: one tool repeats a line many times where the other
    does not, which would otherwise make the path's length reveal the tool, and a reviewer reads
    lines, not columns.
    """
    if len(hops) <= 2:
        return hops
    out = [hops[0]]
    for hop in hops[1:-1]:
        if (hop["path"], hop["line"]) != (out[-1]["path"], out[-1]["line"]):
            out.append(hop)
    out.append(hops[-1])
    return out


def take(items, k, app, key):
    return sorted(items, key=lambda x: h(app, key(x)))[:k]


def piiflow_items(app, doc):
    sinks = {s["id"]: s for s in doc["sinks"]}
    flows = {f["id"]: f for f in doc["flows"]}
    findings = [f for f in doc["findings"] if f["rule_id"] != "PFC01" and f.get("flow")]
    strata = {}
    for rule in RULES:
        strata[f"piiflow:{rule}"] = [f for f in findings if f["rule_id"] == rule and f["severity"] != "info"]
    strata["piiflow:info"] = [f for f in findings if f["severity"] == "info"]
    population = {s: len(v) for s, v in strata.items()}
    chosen = {}
    for stratum, members in strata.items():
        k = K_INFO if stratum == "piiflow:info" else K_PER_RULE
        for f in take(members, k, app, lambda f: f["id"]):
            flow = flows[f["flow"]]
            item_id = "F" + h(app, "piiflow", flow["id"])[:10]
            entry = chosen.setdefault(item_id, {
                "item": {
                    "id": item_id, "kind": "flow",
                    "hops": collapse([{"path": p["path"], "line": p["line"], "column": p["column"]} for p in flow["path"]]),
                    # piiflow's `unknown` (a maybe-personal name) would reveal the tool; both
                    # tools' unspecific categories render as Fideslang's generic `user`.
                    "categories": ["user" if flow["category"] == "unknown" else flow["category"]],
                    "sink_class": COARSE.get(sinks[flow["sink"]]["class"], "other"),
                },
                "key": {"tool": "piiflow", "flow": flow["id"], "findings": [], "strata": []},
            })
            entry["key"]["findings"].append({"id": f["id"], "rule": f["rule_id"], "severity": f["severity"]})
            entry["key"]["strata"].append(stratum)
    gaps = doc["coverage"]["gaps"]
    population["piiflow:gaps"] = len(gaps)
    for g in take(gaps, K_GAPS, app, lambda g: g["id"]):
        item_id = "G" + h(app, "piiflow", g["id"])[:10]
        chosen[item_id] = {
            "item": {"id": item_id, "kind": "gap", "gap_kind": g["kind"], "detail": g["detail"],
                     "locations": [{"path": l["path"], "line": l.get("line"), "column": l.get("column")} for l in g["locations"][:5]],
                     "reached_by_sources": len(g["sources"])},
            "key": {"tool": "piiflow", "gap": g["id"], "strata": ["piiflow:gaps"]},
        }
    return chosen, population


def privado_items(app, rows):
    strata = {}
    for r in rows:
        strata.setdefault(f"privado:{r['flow_type']}", []).append(r)
    population = {s: len(v) for s, v in strata.items()}
    chosen = {}
    for stratum, members in sorted(strata.items()):
        for r in take(members, K_PRIVADO_PER_TYPE, app, lambda r: r["native_id"]):
            item_id = "F" + h(app, "privado", r["native_id"])[:10]
            chosen[item_id] = {
                "item": {"id": item_id, "kind": "flow", "hops": collapse(r["hops"]), "categories": r["categories"],
                         "sink_class": COARSE.get(r["sink_class"], "other")},
                "key": {"tool": "privado", "native_id": r["native_id"], "strata": [stratum]},
            }
    return chosen, population


def site_items(app, sites):
    logs = [s for s in sites if s["class"] == "log"]
    rest = [s for s in sites if s["class"] != "log"]
    sid = lambda s: f"{s['path']}:{s['line']}:{s['column']}"
    n_log = min(len(logs), max(K_SITES_LOG, K_SITES - len(rest)))
    picked = take(logs, n_log, app, sid) + take(rest, K_SITES - n_log, app, sid)
    chosen = {}
    for s in picked:
        item_id = "S" + h(app, "site", sid(s))[:10]
        chosen[item_id] = {
            "item": {"id": item_id, "kind": "site", "path": s["path"], "line": s["line"], "column": s["column"],
                     "callee": s["callee"], "class": s["class"]},
            "key": {"strata": ["sites:log" if s["class"] == "log" else "sites:other"]},
        }
    return chosen, {"sites:log": len(logs), "sites:other": len(rest)}


def code_line(root, path, line, cache):
    if line is None:
        return ""
    lines = cache.get(path)
    if lines is None:
        try:
            lines = (root / path).read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            lines = []
        cache[path] = lines
    text = lines[line - 1].strip() if 0 < line <= len(lines) else ""
    return text if len(text) <= 160 else text[:159] + "…"


def render(app, entry, items, root):
    files = {}
    out = [f"# {app}: labelling sheet\n",
           f"{entry['repository']} at `{entry['commit']}`, scope `{entry['scope']}`. Paths are relative to the scope.",
           "Label each item in your own `benchmark/labels/<you>/" + app + ".yml`; see benchmark/PROTOCOL.md §3.\n"]
    for it in items:
        if it["kind"] == "flow":
            out.append(f"## {it['id']} — flow to a {it['sink_class']} sink, categories: {', '.join(it['categories'])}\n")
            n = len(it["hops"])
            for i, hop in enumerate(it["hops"]):
                role = "source" if i == 0 else "sink" if i == n - 1 else "step"
                out.append(f"{i + 1}. {role:6} `{hop['path']}:{hop['line']}:{hop['column']}`")
                out.append(f"   ```\n   {code_line(root, hop['path'], hop['line'], files)}\n   ```")
        elif it["kind"] == "gap":
            out.append(f"## {it['id']} — coverage gap ({it['gap_kind']}), reached by {it['reached_by_sources']} source(s)\n")
            out.append(f"Unresolved: `{it['detail']}`\n")
            for loc in it["locations"]:
                out.append(f"- `{loc['path']}:{loc['line']}`")
                out.append(f"  ```\n  {code_line(root, loc['path'], loc['line'], files)}\n  ```")
        else:
            out.append(f"## {it['id']} — call site ({it['class']} candidate)\n")
            out.append(f"`{it['path']}:{it['line']}:{it['column']}`\n```\n{code_line(root, it['path'], it['line'], files)}\n```")
        out.append("")
    return "\n".join(out) + "\n"


def template(app, items):
    out = [f"# Labels for {app}. Fill every field; see benchmark/PROTOCOL.md §3. Do not open",
           "# benchmark/sheets/key/ until both reviewers are done.",
           "items:"]
    for it in items:
        out.append(f"  {it['id']}:")
        if it["kind"] == "flow":
            out += ["    verdict:        # tp | fp_path | fp_not_personal | unsure",
                    "    category_ok:    # yes | no",
                    "    citation:       # file:line (for fp_path: where the path breaks)",
                    "    note:"]
        elif it["kind"] == "gap":
            out += ["    hides_flow:     # yes | no | unsure",
                    "    citation:       # file:line of the sink reached, if yes",
                    "    note:"]
        else:
            out += ["    is_sink:        # yes | no",
                    "    personal:       # yes | no | unsure (only if is_sink is yes)",
                    "    categories: []  # Fideslang keys, if personal is yes",
                    "    source:         # file:line of one source, if personal is yes",
                    "    note:"]
    out += ["# Sinks you found while labelling that are not items above (reported separately):",
            "added_sites: []    # - {path: ..., line: ..., categories: [...], source: file:line}",
            ""]
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default=str(ROOT / ".benchmark-cache"))
    ap.add_argument("--allow-without-privado", action="store_true",
                    help="draw piiflow and site items only (for testing; sheets are then not final)")
    ap.add_argument("--labels-dir", default=str(HERE / "labels"))
    args = ap.parse_args()
    cache = pathlib.Path(args.cache)
    corpus = json.loads((HERE / "corpus.json").read_text())
    (HERE / "sheets/key").mkdir(parents=True, exist_ok=True)
    (cache / "sheets").mkdir(parents=True, exist_ok=True)
    for entry in corpus["repositories"]:
        app = entry["name"]
        doc = json.loads((cache / "results" / app / "flows.json").read_text())
        chosen, population = piiflow_items(app, doc)
        privado = HERE / "results" / "privado" / f"{app}.jsonl"
        if privado.exists():
            rows = [json.loads(l) for l in privado.read_text().splitlines() if l.strip()]
            rows = [r for r in rows if "summary" not in r]
            more, pop = privado_items(app, rows)
            chosen.update(more)
            population.update(pop)
        elif not args.allow_without_privado:
            sys.exit(f"{app}: {privado} is missing; run the Privado workflow first (PROTOCOL.md §7 step 2)")
        sites = [json.loads(l) for l in (cache / "candidates" / f"{app}.jsonl").read_text().splitlines()]
        more, pop = site_items(app, sites)
        chosen.update(more)
        population.update(pop)

        # Flows from both tools are shuffled together by their opaque ids; gaps and sites follow.
        order = {"flow": 0, "gap": 1, "site": 2}
        ids = sorted(chosen, key=lambda i: (order[chosen[i]["item"]["kind"]], h(app, "order", i)))
        items = [chosen[i]["item"] for i in ids]
        with open(HERE / "sheets" / f"{app}.items.jsonl", "w") as f:
            for it in items:
                f.write(json.dumps(it, sort_keys=True, ensure_ascii=False) + "\n")
        key = {"app": app, "population": dict(sorted(population.items())),
               "items": {i: chosen[i]["key"] for i in ids}}
        (HERE / "sheets/key" / f"{app}.json").write_text(json.dumps(key, indent=1, sort_keys=True) + "\n")
        root = cache / app / entry["scope"]
        (cache / "sheets" / f"{app}.md").write_text(render(app, entry, items, root))
        for reviewer in ("R1", "R2"):
            path = pathlib.Path(args.labels_dir) / reviewer / f"{app}.yml"
            if not path.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(template(app, items))
        kinds = {}
        for it in items:
            kinds[it["kind"]] = kinds.get(it["kind"], 0) + 1
        print(f"{app}: {len(items)} items {kinds}")


if __name__ == "__main__":
    main()
