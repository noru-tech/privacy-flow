#!/usr/bin/env python3
"""Score the labels (PROTOCOL.md §5 and §7).

    python3 benchmark/score.py --agreement     # Cohen's kappa between R1 and R2, before discussion
    python3 benchmark/score.py                 # results from labels/final/ to results/scores.{json,md}

Precision: per tool, per piiflow rule (pooled sample proportion with a 95 % Wilson interval,
and a stratum-weighted estimate), for piiflow's maybe-personal (info) findings, and per Privado
flow type. Gap usefulness: the share of sampled coverage gaps that hide a real flow. Recall: the
share of sampled candidate sites that are sinks receiving personal data and that each tool
finds (a flow whose sink is on that line) or, for piiflow, flags (a flow or a coverage gap on
that line). `unsure` labels leave the denominators and are counted.
"""
import argparse
import json
import math
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
CATEGORY_RULES = {"PF003", "PF004", "PF006"}
RULES = ["PF001", "PF002", "PF003", "PF004", "PF005", "PF006"]


def wilson(k, n, z=1.959964):
    if n == 0:
        return None
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    w = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return {"k": k, "n": n, "p": round(p, 4), "lo": round(max(0.0, c - w), 4), "hi": round(min(1.0, c + w), 4)}


def kappa(pairs):
    """Cohen's kappa over (a, b) label pairs; None when undefined."""
    if not pairs:
        return None
    n = len(pairs)
    po = sum(a == b for a, b in pairs) / n
    cats = {x for p in pairs for x in p}
    pe = sum((sum(a == c for a, _ in pairs) / n) * (sum(b == c for _, b in pairs) / n) for c in cats)
    return {"n": n, "agreement": round(po, 4), "kappa": round((po - pe) / (1 - pe), 4) if pe < 1 else None}


def norm(v):
    if isinstance(v, bool):
        return "yes" if v else "no"
    return str(v).strip().lower() if v is not None else None


def load_labels(directory, app):
    try:
        import yaml
    except ImportError:
        sys.exit("score.py needs PyYAML to read labels (pip install pyyaml)")
    path = directory / f"{app}.yml"
    if not path.exists():
        return None
    doc = yaml.safe_load(path.read_text()) or {}
    return doc.get("items") or {}, doc.get("added_sites") or []


def apps(corpus):
    return [e for e in corpus["repositories"]]


def agreement(args, corpus):
    fields = {"flow": "verdict", "gap": "hides_flow", "site": "personal"}
    pairs = {k: [] for k in fields}
    missing = []
    for entry in apps(corpus):
        app = entry["name"]
        items = {json.loads(l)["id"]: json.loads(l) for l in (HERE / "sheets" / f"{app}.items.jsonl").read_text().splitlines()}
        a, b = load_labels(args.labels / "R1", app), load_labels(args.labels / "R2", app)
        if a is None or b is None:
            missing.append(app)
            continue
        for item_id, it in items.items():
            field = fields[it["kind"]]
            x, y = norm((a[0].get(item_id) or {}).get(field)), norm((b[0].get(item_id) or {}).get(field))
            if it["kind"] == "site":
                # A non-sink is its own answer for the recall question.
                x = "not_sink" if norm((a[0].get(item_id) or {}).get("is_sink")) == "no" else x
                y = "not_sink" if norm((b[0].get(item_id) or {}).get("is_sink")) == "no" else y
            if x is not None and y is not None:
                pairs[it["kind"]].append((x, y))
    out = {kind: kappa(p) for kind, p in pairs.items()}
    out["applications_missing_labels"] = missing
    print(json.dumps(out, indent=2, sort_keys=True))
    return out


def score(args, corpus):
    cache = pathlib.Path(args.cache)
    rule_obs = {r: [] for r in RULES}          # (app, correct)
    info_obs, gap_obs = [], []
    tool_obs = {"piiflow": [], "privado": []}  # flow items: (app, stratum, tp)
    privado_type_obs = {}
    recall_obs = []                            # (language, piiflow_found, piiflow_flagged, privado_found)
    populations, unsure, added = {}, {"flow": 0, "gap": 0, "site": 0}, 0
    have_privado = False
    for entry in apps(corpus):
        app = entry["name"]
        labels = load_labels(args.labels / "final", app)
        if labels is None:
            sys.exit(f"{app}: no final labels in {args.labels / 'final'}")
        labels, extra = labels
        added += len(extra)
        key = json.loads((HERE / "sheets/key" / f"{app}.json").read_text())
        populations[app] = key["population"]
        items = {json.loads(l)["id"]: json.loads(l) for l in (HERE / "sheets" / f"{app}.items.jsonl").read_text().splitlines()}
        doc = json.loads((cache / "results" / app / "flows.json").read_text())
        sinks = {s["id"]: s for s in doc["sinks"]}
        pf_sink_lines = {(sinks[f["sink"]]["location"]["path"], sinks[f["sink"]]["location"]["line"]) for f in doc["flows"]}
        pf_gap_lines = {(l["path"], l.get("line")) for g in doc["coverage"]["gaps"] for l in g["locations"]}
        pv_path = HERE / "results/privado" / f"{app}.jsonl"
        pv_sink_lines = set()
        if pv_path.exists():
            have_privado = True
            for line in pv_path.read_text().splitlines():
                r = json.loads(line)
                if "hops" in r:
                    pv_sink_lines.add((r["hops"][-1]["path"], r["hops"][-1]["line"]))
        for item_id, k in key["items"].items():
            it, lab = items[item_id], labels.get(item_id) or {}
            if it["kind"] == "flow":
                verdict = norm(lab.get("verdict"))
                if verdict in (None, "unsure"):
                    unsure["flow"] += 1
                    continue
                tp = verdict == "tp"
                cat_ok = norm(lab.get("category_ok")) == "yes"
                tool_obs[k["tool"]].append((app, k["strata"][0], tp))
                if k["tool"] == "privado":
                    privado_type_obs.setdefault(k["strata"][0], []).append((app, tp))
                for f in k.get("findings", []):
                    correct = tp and (cat_ok or f["rule"] not in CATEGORY_RULES)
                    if f["severity"] == "info":
                        info_obs.append((app, correct))
                    elif f["rule"] in rule_obs:
                        rule_obs[f["rule"]].append((app, correct))
            elif it["kind"] == "gap":
                v = norm(lab.get("hides_flow"))
                if v in (None, "unsure"):
                    unsure["gap"] += 1
                    continue
                gap_obs.append((app, v == "yes"))
            else:
                if norm(lab.get("is_sink")) != "yes":
                    continue
                v = norm(lab.get("personal"))
                if v in (None, "unsure"):
                    unsure["site"] += 1
                    continue
                if v != "yes":
                    continue
                at = (it["path"], it["line"])
                found = at in pf_sink_lines
                recall_obs.append((entry["language"], found, found or at in pf_gap_lines, at in pv_sink_lines))

    def pooled(obs):
        return wilson(sum(1 for _, c in obs if c), len(obs))

    def weighted(obs, stratum):
        num = den = 0.0
        for app in {a for a, _ in obs}:
            mine = [c for a, c in obs if a == app]
            n_pop = populations[app].get(stratum, 0)
            num += n_pop * sum(mine) / len(mine)
            den += n_pop
        return round(num / den, 4) if den else None

    result = {
        "precision": {
            "piiflow_flows": wilson(sum(t for *_, t in tool_obs["piiflow"]), len(tool_obs["piiflow"])),
            "privado_flows": wilson(sum(t for *_, t in tool_obs["privado"]), len(tool_obs["privado"])) if have_privado else None,
            "piiflow_rules": {r: {"pooled": pooled(o), "weighted": weighted(o, f"piiflow:{r}")} for r, o in rule_obs.items()},
            "piiflow_info": {"pooled": pooled(info_obs), "weighted": weighted(info_obs, "piiflow:info")},
            "privado_by_type": {t: {"pooled": pooled(o), "weighted": weighted(o, t)} for t, o in sorted(privado_type_obs.items())},
        },
        "gaps_hiding_a_flow": pooled(gap_obs),
        "recall": {
            scope: {
                "piiflow_found": wilson(sum(f for _, f, _, _ in obs), len(obs)),
                "piiflow_flagged": wilson(sum(g for _, _, g, _ in obs), len(obs)),
                "privado_found": wilson(sum(p for *_, p in obs), len(obs)) if have_privado else None,
            }
            for scope, obs in {
                "all": recall_obs,
                "typescript": [o for o in recall_obs if o[0] == "typescript"],
                "python": [o for o in recall_obs if o[0] == "python"],
            }.items()
        },
        "unsure_excluded": unsure,
        "reviewer_added_sites": added,
    }
    out = HERE / "results"
    out.mkdir(exist_ok=True)
    (out / "scores.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    (out / "scores.md").write_text(markdown(result))
    print(markdown(result))
    return result


def fmt(w):
    if not w:
        return "—"
    return f"{w['p']:.2f} [{w['lo']:.2f}, {w['hi']:.2f}] (n={w['n']})"


def markdown(r):
    p = r["precision"]
    lines = ["# Benchmark results", "", "Proportions with 95 % Wilson intervals; see PROTOCOL.md.", "",
             "## Precision", "", "| | Pooled | Weighted |", "| --- | --- | --- |",
             f"| piiflow, all sampled flows | {fmt(p['piiflow_flows'])} | |",
             f"| Privado, all sampled flows | {fmt(p['privado_flows'])} | |"]
    for rule, v in p["piiflow_rules"].items():
        lines.append(f"| piiflow {rule} | {fmt(v['pooled'])} | {v['weighted'] if v['weighted'] is not None else '—'} |")
    v = p["piiflow_info"]
    lines.append(f"| piiflow maybe-personal (info) | {fmt(v['pooled'])} | {v['weighted'] if v['weighted'] is not None else '—'} |")
    for t, v in p["privado_by_type"].items():
        lines.append(f"| {t} | {fmt(v['pooled'])} | {v['weighted'] if v['weighted'] is not None else '—'} |")
    lines += ["", "## Recall on sampled sink sites that receive personal data", "",
              "| Scope | piiflow found | piiflow found or flagged | Privado found |", "| --- | --- | --- | --- |"]
    for scope, v in r["recall"].items():
        lines.append(f"| {scope} | {fmt(v['piiflow_found'])} | {fmt(v['piiflow_flagged'])} | {fmt(v['privado_found'])} |")
    lines += ["", f"Coverage gaps that hide a real flow: {fmt(r['gaps_hiding_a_flow'])}.", "",
              f"`unsure` labels excluded: {r['unsure_excluded']}. Sites added by reviewers: {r['reviewer_added_sites']}.", ""]
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--agreement", action="store_true")
    ap.add_argument("--labels", type=pathlib.Path, default=HERE / "labels")
    ap.add_argument("--cache", default=str(ROOT / ".benchmark-cache"))
    args = ap.parse_args()
    corpus = json.loads((HERE / "corpus.json").read_text())
    if args.agreement:
        agreement(args, corpus)
    else:
        score(args, corpus)


if __name__ == "__main__":
    main()
