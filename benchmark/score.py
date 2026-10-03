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
    unlabelled = []
    # Head to head, on the applications both tools completed: (app, stratum, real flow?) per
    # sampled report, and (piiflow found, piiflow flagged, Privado found) per personal sink site.
    h2h_reports = {"piiflow": [], "piiflow_raised": [], "privado": []}
    h2h_sites = []
    # The same sites counted by the original rule (a flow's sink on the line), for comparison.
    recall_at_sink, h2h_at_sink = [], []
    for entry in apps(corpus):
        app = entry["name"]
        labels = load_labels(args.labels / "final", app)
        if labels is None:
            # Not labelled by both reviewers: no scores for it (PROTOCOL.md, Deviations).
            unlabelled.append(app)
            continue
        labels, extra = labels
        added += len(extra)
        key = json.loads((HERE / "sheets/key" / f"{app}.json").read_text())
        populations[app] = key["population"]
        items = {json.loads(l)["id"]: json.loads(l) for l in (HERE / "sheets" / f"{app}.items.jsonl").read_text().splitlines()}
        doc = json.loads((cache / "results" / app / "flows.json").read_text())
        sinks = {s["id"]: s for s in doc["sinks"]}
        pf_sink_lines = {(sinks[f["sink"]]["location"]["path"], sinks[f["sink"]]["location"]["line"]) for f in doc["flows"]}
        pf_gap_lines = {(l["path"], l.get("line")) for g in doc["coverage"]["gaps"] for l in g["locations"]}
        # A flow that enters a call of the project's own code on the site's line, and goes on to
        # a sink, finds that site too (PROTOCOL.md, Deviations, 2026-10-03): `sendmail(user.email)`
        # where the sink is inside `sendmail`.
        pf_through_lines = {(h["path"], h["line"]) for f in doc["flows"] for h in f["path"][1:-1]
                            if h["kind"] == "call" and (h.get("note") or "").startswith("into ")}
        pv_path = HERE / "results/privado" / f"{app}.jsonl"
        pv_sink_lines = set()
        pv_through_lines = set()
        pv_ran = pv_path.exists()
        if pv_ran:
            have_privado = True
            for line in pv_path.read_text().splitlines():
                r = json.loads(line)
                if "hops" in r:
                    pv_sink_lines.add((r["hops"][-1]["path"], r["hops"][-1]["line"]))
                    # Privado's hops have no kinds: any hop between source and sink counts, the
                    # more lenient reading of the same rule.
                    pv_through_lines.update((h["path"], h["line"]) for h in r["hops"][1:-1])
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
                if pv_ran and k["tool"] == "privado":
                    h2h_reports["privado"].append((app, k["strata"][0], tp))
                if pv_ran and k["tool"] == "piiflow":
                    for f in k.get("findings", []):
                        stratum = "piiflow:info" if f["severity"] == "info" else "piiflow:" + f["rule"]
                        h2h_reports["piiflow"].append((app, stratum, tp))
                        if f["severity"] != "info":
                            h2h_reports["piiflow_raised"].append((app, stratum, tp))
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
                sink = norm(lab.get("is_sink"))
                if sink == "unsure":
                    unsure["site"] += 1
                    continue
                if sink != "yes":
                    continue
                v = norm(lab.get("personal"))
                if v in (None, "unsure"):
                    unsure["site"] += 1
                    continue
                if v != "yes":
                    continue
                at = (it["path"], it["line"])
                at_sink = at in pf_sink_lines
                found = at_sink or at in pf_through_lines
                pv_at_sink = at in pv_sink_lines
                pv_found = pv_at_sink or at in pv_through_lines
                # Privado's recall counts only applications it completed (None elsewhere).
                recall_obs.append((entry["language"], found, found or at in pf_gap_lines, pv_found if pv_ran else None))
                recall_at_sink.append((entry["language"], at_sink, pv_at_sink if pv_ran else None))
                if pv_ran:
                    h2h_sites.append((found, found or at in pf_gap_lines, pv_found))
                    h2h_at_sink.append((at_sink, pv_at_sink))

    both_apps = sorted(a for a in populations if (HERE / "results/privado" / f"{a}.jsonl").exists())

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

    def stratified(obs, counts):
        """Share of a tool's reports that are real flows. Each (application, stratum) is weighted
        by how many reports it holds; the 95 % interval is normal, from the stratified variance
        with the finite-population correction. `counts(app)` names the strata that make up the
        tool's reports; strata with reports but no usable label are left out, and their share of
        all reports is given as `uncovered`."""
        cells = {}
        for app, stratum, ok in obs:
            cells.setdefault((app, stratum), []).append(ok)
        covered = sum(populations[a].get(st, 0) for a, st in cells)
        everything = sum(n for a in both_apps for st, n in populations[a].items() if counts(st))
        if not covered:
            return None
        est = var = 0.0
        for (app, stratum), xs in cells.items():
            n_pop, n = populations[app].get(stratum, 0), len(xs)
            p = sum(xs) / n
            w = n_pop / covered
            est += w * p
            if n > 1 and n_pop > 1:
                var += w * w * p * (1 - p) / (n - 1) * (1 - n / n_pop)
        half = 1.959964 * math.sqrt(var)
        return {"p": round(est, 4), "lo": round(max(0.0, est - half), 4), "hi": round(min(1.0, est + half), 4),
                "n": len(obs), "reports": covered, "uncovered": round(1 - covered / everything, 4) if everything else None}

    head_to_head = None
    if have_privado:
        n_sites = len(h2h_sites)
        head_to_head = {
            "applications": both_apps,
            "precision": {
                "piiflow": stratified(h2h_reports["piiflow"], lambda st: st.startswith("piiflow:PF") or st == "piiflow:info") if h2h_reports["piiflow"] else None,
                "piiflow_raised": stratified(h2h_reports["piiflow_raised"], lambda st: st.startswith("piiflow:PF")) if h2h_reports["piiflow_raised"] else None,
                "privado": stratified(h2h_reports["privado"], lambda st: st.startswith("privado:")) if h2h_reports["privado"] else None,
            },
            "recall": {
                "sites": n_sites,
                "piiflow_found": wilson(sum(f for f, _, _ in h2h_sites), n_sites),
                "piiflow_flagged": wilson(sum(g for _, g, _ in h2h_sites), n_sites),
                "privado_found": wilson(sum(p for _, _, p in h2h_sites), n_sites),
                # The original rule: only a flow's sink on the site's line.
                "piiflow_found_at_sink": wilson(sum(f for f, _ in h2h_at_sink), n_sites),
                "privado_found_at_sink": wilson(sum(p for _, p in h2h_at_sink), n_sites),
                "paired": {
                    "both": sum(1 for f, _, p in h2h_sites if f and p),
                    "piiflow_only": sum(1 for f, _, p in h2h_sites if f and not p),
                    "privado_only": sum(1 for f, _, p in h2h_sites if p and not f),
                    "neither": sum(1 for f, _, p in h2h_sites if not f and not p),
                },
            },
        }

    result = {
        "head_to_head": head_to_head,
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
                "privado_found": wilson(sum(1 for *_, p in obs if p), sum(1 for *_, p in obs if p is not None)) if have_privado else None,
            }
            for scope, obs in {
                "all": recall_obs,
                "typescript": [o for o in recall_obs if o[0] == "typescript"],
                "python": [o for o in recall_obs if o[0] == "python"],
            }.items()
        },
        "recall_at_sink": {
            "piiflow_found": wilson(sum(f for _, f, _ in recall_at_sink), len(recall_at_sink)),
            "privado_found": wilson(sum(1 for *_, p in recall_at_sink if p), sum(1 for *_, p in recall_at_sink if p is not None)) if have_privado else None,
        },
        "unsure_excluded": unsure,
        "reviewer_added_sites": added,
        "applications_without_final_labels": unlabelled,
    }
    out = args.out
    out.mkdir(parents=True, exist_ok=True)
    (out / "scores.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    (out / "scores.md").write_text(markdown(result))
    print(markdown(result))
    return result


def fmt(w):
    if not w:
        return "—"
    return f"{100 * w['p']:.0f} % [{100 * w['lo']:.0f}–{100 * w['hi']:.0f} %] (n={w['n']})"


def markdown(r):
    p = r["precision"]
    lines = ["# Benchmark results", ""]
    hh = r.get("head_to_head")
    if hh:
        pr, rc = hh["precision"], hh["recall"]
        pc = rc["paired"]
        lines += [f"## piiflow and Privado, head to head ({len(hh['applications'])} applications both completed)", "",
                  "| Question | piiflow | Privado |", "| --- | --- | --- |",
                  f"| Of the flows a tool reports, how many are real? | {fmt(pr['piiflow'])} | {fmt(pr['privado'])} |",
                  f"| ...counting only piiflow's findings (not its maybe-personal ones) | {fmt(pr['piiflow_raised'])} | |",
                  f"| Of the sinks that really receive personal data, how many does it find? | {fmt(rc['piiflow_found'])} | {fmt(rc['privado_found'])} |",
                  f"| ...or flags as a place it could not see | {fmt(rc['piiflow_flagged'])} | |",
                  f"| ...counting only a flow's sink on the line (the original rule) | {fmt(rc['piiflow_found_at_sink'])} | {fmt(rc['privado_found_at_sink'])} |", "",
                  "A sink is found when a flow ends on its line or enters a call on its line that leads to a sink "
                  "(PROTOCOL.md, Deviations, 2026-10-03).", "",
                  f"Of {rc['sites']} sampled sinks that receive personal data: both tools found {pc['both']}, only piiflow {pc['piiflow_only']}, "
                  f"only Privado {pc['privado_only']}, neither {pc['neither']}.", ""]
    lines += ["## Detail", "", "Proportions with 95 % Wilson intervals; see PROTOCOL.md.", "",
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
    if r.get("applications_without_final_labels"):
        lines += [f"Not scored (not labelled by both reviewers): {', '.join(r['applications_without_final_labels'])}.", ""]
    return "\n".join(lines)


def site_lines(doc):
    """(sink lines, lines a flow enters a call of the application on, gap lines) of a document."""
    sinks = {s["id"]: s for s in doc["sinks"]}
    at_sink = {(sinks[f["sink"]]["location"]["path"], sinks[f["sink"]]["location"]["line"]) for f in doc["flows"]}
    through = {(h["path"], h["line"]) for f in doc["flows"] for h in f["path"][1:-1]
               if h["kind"] == "call" and (h.get("note") or "").startswith("into ")}
    gaps = {(l["path"], l.get("line")) for g in doc["coverage"]["gaps"] for l in g["locations"]}
    return at_sink, through, gaps


def score_run(args, corpus):
    """Recall of a later run (`run.py --run NAME`) on the protocol's labelled sink sites.

    The sites were sampled without either tool, so they measure any version. The flow and gap
    samples were drawn from the protocol run's reports and say nothing about a later version's,
    so a later run has no precision until a sample of its own flows is labelled.
    """
    cache = pathlib.Path(args.cache) / "runs" / args.run
    if not cache.is_dir():
        sys.exit(f"no documents for {args.run} in {cache}; run benchmark/run.py --run {args.run}")
    obs, unlabelled = [], []
    for entry in apps(corpus):
        app = entry["name"]
        labels = load_labels(args.labels / "final", app)
        if labels is None:
            unlabelled.append(app)
            continue
        labels = labels[0]
        key = json.loads((HERE / "sheets/key" / f"{app}.json").read_text())
        items = {json.loads(l)["id"]: json.loads(l) for l in (HERE / "sheets" / f"{app}.items.jsonl").read_text().splitlines()}
        at_sink, through, gaps = site_lines(json.loads((cache / app / "flows.json").read_text()))
        pv_path = HERE / "results/privado" / f"{app}.jsonl"
        pv_sink, pv_through = set(), set()
        if pv_path.exists():
            for line in pv_path.read_text().splitlines():
                r = json.loads(line)
                if "hops" in r:
                    pv_sink.add((r["hops"][-1]["path"], r["hops"][-1]["line"]))
                    pv_through.update((h["path"], h["line"]) for h in r["hops"][1:-1])
        for item_id in key["items"]:
            it, lab = items[item_id], labels.get(item_id) or {}
            if it["kind"] != "site" or norm(lab.get("is_sink")) != "yes" or norm(lab.get("personal")) != "yes":
                continue
            at = (it["path"], it["line"])
            found = at in at_sink or at in through
            obs.append({"app": app, "language": entry["language"], "at_sink": at in at_sink, "found": found,
                        "flagged": found or at in gaps,
                        "privado": (at in pv_sink or at in pv_through) if pv_path.exists() else None})
    def rates(xs):
        pv = [x for x in xs if x["privado"] is not None]
        return {"sites": len(xs),
                "piiflow_found": wilson(sum(x["found"] for x in xs), len(xs)),
                "piiflow_flagged": wilson(sum(x["flagged"] for x in xs), len(xs)),
                "piiflow_found_at_sink": wilson(sum(x["at_sink"] for x in xs), len(xs)),
                "privado_found": wilson(sum(x["privado"] for x in pv), len(pv)) if pv else None}
    shared = [x for x in obs if x["privado"] is not None]
    result = {
        "run": args.run,
        "recall": {"all": rates(obs),
                   "typescript": rates([x for x in obs if x["language"] == "typescript"]),
                   "python": rates([x for x in obs if x["language"] == "python"])},
        "head_to_head": rates(shared),
        "precision": None,
        "applications_without_final_labels": unlabelled,
    }
    out = HERE / "results" / "runs" / args.run
    out.mkdir(parents=True, exist_ok=True)
    (out / "recall.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    lines = [f"# Recall of piiflow {args.run} on the labelled sink sites", "",
             "The protocol's sampled sink sites, labelled for 0.1.0, scored against this run's documents. "
             "Precision needs a sample of this run's own flows, not yet labelled.", "",
             "| Scope | sites | piiflow found | found or flagged | found, sink on the line only | Privado found |",
             "| --- | --- | --- | --- | --- | --- |"]
    for scope, r in list(result["recall"].items()) + [("head to head", result["head_to_head"])]:
        lines.append(f"| {scope} | {r['sites']} | {fmt(r['piiflow_found'])} | {fmt(r['piiflow_flagged'])} | "
                     f"{fmt(r['piiflow_found_at_sink'])} | {fmt(r['privado_found'])} |")
    if unlabelled:
        lines += ["", f"Not scored (not labelled by both reviewers): {', '.join(unlabelled)}."]
    (out / "recall.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    return result


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--agreement", action="store_true")
    ap.add_argument("--run", help="score the recall of a later run recorded by run.py --run")
    ap.add_argument("--labels", type=pathlib.Path, default=HERE / "labels")
    ap.add_argument("--cache", default=str(ROOT / ".benchmark-cache"))
    ap.add_argument("--out", type=pathlib.Path, default=HERE / "results", help="where scores.json and scores.md go")
    args = ap.parse_args()
    corpus = json.loads((HERE / "corpus.json").read_text())
    if args.agreement:
        agreement(args, corpus)
    elif args.run:
        score_run(args, corpus)
    else:
        score(args, corpus)


if __name__ == "__main__":
    main()
