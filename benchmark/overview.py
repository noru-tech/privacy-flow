#!/usr/bin/env python3
"""Summarise what each tool reported on the corpus, before any labels (README "Status").

    python3 benchmark/overview.py      # writes benchmark/results/overview.json

Counts only: flows, findings, coverage gaps, sink classes, where the two tools' sinks
coincide, how many candidate sink sites each tool reports a flow at, run times, and labelling
progress. None of it says whether a reported flow is real; that is what the labels are for.
Needs the fetched applications' piiflow documents (run.py) and candidates (candidates.py).
"""
import collections
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
CACHE = ROOT / ".benchmark-cache"
COARSE = {"log": "log", "llm": "llm", "http": "http", "browser_storage": "browser_storage",
          "error_tracking": "third_party", "analytics": "third_party", "messaging": "third_party",
          "third_party": "third_party", "other": "other"}
LABEL_FIELDS = {"F": "verdict", "G": "hides_flow", "S": "is_sink"}


def labelled(path):
    """Items with their primary field filled in, by a plain text scan (no YAML dependency)."""
    if not path.exists():
        return 0
    done, item = 0, None
    for line in path.read_text().splitlines():
        s = line.strip()
        if line.startswith("  ") and not line.startswith("    ") and s.endswith(":") and s[0] in "FGS":
            item = s[:-1]
        elif item and s.startswith(LABEL_FIELDS[item[0]] + ":"):
            value = s.split(":", 1)[1].split("#", 1)[0].strip()
            done += bool(value)
    return done


def main():
    corpus = json.loads((HERE / "corpus.json").read_text())
    summary = {a["name"]: a for a in json.loads((HERE / "results/summary.json").read_text())["applications"]}
    apps = []
    for entry in corpus["repositories"]:
        name = entry["name"]
        doc = json.loads((CACHE / "results" / name / "flows.json").read_text())
        sinks = {s["id"]: s for s in doc["sinks"]}
        pf_lines = collections.defaultdict(set)
        by_class = collections.Counter()
        maybe = 0
        for f in doc["flows"]:
            s = sinks[f["sink"]]
            cls = COARSE.get(s["class"], "other")
            by_class[cls] += 1
            pf_lines[(s["location"]["path"], s["location"]["line"])].add(cls)
            maybe += f["category"] == "unknown"
        findings = [f for f in doc["findings"] if f["rule_id"] != "PFC01"]
        gap_lines = {(l["path"], l.get("line")) for g in doc["coverage"]["gaps"] for l in g["locations"]}

        pv_run = HERE / "results/privado" / f"{name}.run.json"
        pv_flows = HERE / "results/privado" / f"{name}.jsonl"
        privado = {"status": "not run"}
        pv_lines = set()
        if pv_flows.exists():
            rows = [json.loads(l) for l in pv_flows.read_text().splitlines()]
            rows = [r for r in rows if "hops" in r]
            run = json.loads(pv_run.read_text())
            pv_lines = {(r["hops"][-1]["path"], r["hops"][-1]["line"]) for r in rows}
            privado = {"status": "ok", "flows": len(rows),
                       "by_class": dict(collections.Counter(COARSE.get(r["sink_class"], "other") for r in rows)),
                       "sink_lines": len(pv_lines), "wall_seconds": run["wall_seconds"]}
        elif pv_run.exists():
            run = json.loads(pv_run.read_text())
            privado = {"status": "failed", "reason": "out of memory (exit 137) in both attempts",
                       "wall_seconds": [a["wall_seconds"] for a in run["attempts"]]}

        sites = [json.loads(l) for l in (CACHE / "candidates" / f"{name}.jsonl").read_text().splitlines()]
        site_keys = {(s["path"], s["line"]) for s in sites}
        items = [json.loads(l) for l in (HERE / "sheets" / f"{name}.items.jsonl").read_text().splitlines()]
        key = json.loads((HERE / "sheets/key" / f"{name}.json").read_text())
        tools = collections.Counter(k.get("tool", "site") for k in key["items"].values())
        lines = sum(l["lines"] for l in summary[name]["languages"])
        apps.append({
            "name": name,
            "repository": entry["repository"],
            "language": entry["language"],
            "framework": entry["framework"],
            "lines": lines,
            "files": summary[name]["files"],
            "piiflow": {
                "flows": len(doc["flows"]),
                "flows_maybe_personal": maybe,
                "by_class": dict(by_class),
                "sink_lines": len(pf_lines),
                "findings": len(findings),
                "findings_by_severity": dict(collections.Counter(f["severity"] for f in findings)),
                "findings_by_rule": dict(collections.Counter(f["rule_id"] for f in findings if f["severity"] != "info")),
                "gaps": len(doc["coverage"]["gaps"]),
                "gaps_by_kind": dict(collections.Counter(g["kind"] for g in doc["coverage"]["gaps"])),
                "processors": doc["summary"]["processors"],
                "wall_seconds": summary[name]["wall_seconds"],
                "peak_rss_mib": summary[name]["peak_rss_mib"],
            },
            "privado": privado,
            "sink_lines_overlap": None if privado["status"] != "ok" else {
                "both": len(set(pf_lines) & pv_lines),
                "piiflow_only": len(set(pf_lines) - pv_lines),
                "privado_only": len(pv_lines - set(pf_lines)),
            },
            "candidate_sites": {
                "total": len(site_keys),
                "by_class": dict(collections.Counter(s["class"] for s in sites)),
                "with_piiflow_flow": len(site_keys & set(pf_lines)),
                "with_piiflow_flow_or_gap": len(site_keys & (set(pf_lines) | gap_lines)),
                "with_privado_flow": None if privado["status"] != "ok" else len(site_keys & pv_lines),
            },
            "sample": {
                "items": len(items),
                "flows_piiflow": sum(1 for i, k in key["items"].items() if i[0] == "F" and k.get("tool") == "piiflow"),
                "flows_privado": sum(1 for i, k in key["items"].items() if i[0] == "F" and k.get("tool") == "privado"),
                "gaps": sum(1 for i in items if i["kind"] == "gap"),
                "sites": sum(1 for i in items if i["kind"] == "site"),
                "labelled_r1": labelled(HERE / "labels/R1" / f"{name}.yml"),
                "labelled_r2": labelled(HERE / "labels/R2" / f"{name}.yml"),
            },
        })
    scores = HERE / "results/scores.json"
    out = {"generated_from": {"piiflow_commit": json.loads((HERE / "results/summary.json").read_text())["piiflow"]["commit"]},
           "applications": apps,
           # The labelled results, once score.py has run on final labels; absent until then.
           "scores": json.loads(scores.read_text()) if scores.exists() else None}
    (HERE / "results/overview.json").write_text(json.dumps(out, indent=1, sort_keys=True) + "\n")
    tot = lambda f: sum(f(a) for a in apps)
    print(f"{len(apps)} applications, {tot(lambda a: a['lines']):,} lines; piiflow {tot(lambda a: a['piiflow']['flows']):,} flows, "
          f"{tot(lambda a: a['piiflow']['gaps']):,} gaps; Privado {tot(lambda a: a['privado'].get('flows', 0)):,} flows; "
          f"{tot(lambda a: a['sample']['items'])} items per reviewer")


if __name__ == "__main__":
    main()
