#!/usr/bin/env python3
"""Write the benchmark report into the repository: benchmark/REPORT.md and its SVG charts.

    python3 benchmark/overview.py          # refresh the data first
    python3 benchmark/report/markdown.py

The same content as the report page (benchmark/report/page.html), as Markdown that GitHub
renders, with charts as SVG files under benchmark/report/charts/. Every number and name comes
from benchmark/results/overview.json (and the labelled scores inside it, once they exist).
"""
import json
import math
import pathlib
from xml.sax.saxutils import escape

HERE = pathlib.Path(__file__).resolve().parent
BENCH = HERE.parent
NAMES = {"vercel-chatbot": "Vercel chatbot", "umami": "Umami", "taxonomy": "Taxonomy", "open-saas": "Open SaaS",
         "ghost": "Ghost", "hoppscotch": "Hoppscotch", "healthchecks": "Healthchecks", "redash": "Redash", "ctfd": "CTFd",
         "fastapi-template": "FastAPI template", "polar": "Polar", "private-gpt": "PrivateGPT"}
WORDS = ["no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"]
INK, SUB, GRID, A, B, MID, BAD = "#121a1b", "#5f6b6a", "#e3e8e8", "#3a5355", "#9cb8b5", "#6f9391", "#b3261e"
FONT = "-apple-system, Segoe UI, Helvetica, Arial, sans-serif"


def count(n):
    return WORDS[n] if n < len(WORDS) else str(n)


def listing(xs):
    return xs[0] if len(xs) == 1 else ", ".join(xs[:-1]) + " and " + xs[-1]


def pct(p):
    return f"{round(100 * p)} %"


def svg(w, h, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" font-family="{FONT}">'
            f'<rect width="{w}" height="{h}" rx="10" fill="#ffffff"/>' + "".join(body) + "</svg>\n")


def text(x, y, s, size=12, fill=SUB, anchor="start", weight=None):
    w = f' font-weight="{weight}"' if weight else ""
    return f'<text x="{x:.1f}" y="{y:.1f}" font-size="{size}" fill="{fill}" text-anchor="{anchor}"{w}>{escape(str(s))}</text>'


def legend(items, y=44, x0=24):
    out, x = [], x0
    for color, label in items:
        out.append(f'<rect x="{x}" y="{y}" width="11" height="11" rx="2" fill="{color}"/>')
        out.append(text(x + 17, y + 10, label, 12.5))
        x += 17 + 7.2 * len(label) + 22
    return out


def chart_flows(apps):
    rows = sorted(apps, key=lambda a: a["lines"])
    W, L, R, rowh, top = 860, 190, 70, 36, 70
    H = top + len(rows) * rowh + 40
    x = lambda v: L + (W - L - R) * math.log10(max(v, 1)) / 4
    body = [text(24, 30, "Flows reported per application (log scale)", 17, INK, weight=600)] + legend([(A, "piiflow"), (B, "Privado")])
    for t in [1, 10, 100, 1000, 10000]:
        body.append(f'<line x1="{x(t):.1f}" x2="{x(t):.1f}" y1="{top - 6}" y2="{H - 34}" stroke="{GRID}"/>')
        body.append(text(x(t), H - 16, f"{t:,}", 12, anchor="middle"))
    for i, a in enumerate(rows):
        y = top + i * rowh
        body.append(text(L - 12, y + 12, NAMES[a["name"]], 13, INK, "end"))
        body.append(text(L - 12, y + 26, f"{'Python' if a['language'] == 'python' else 'TypeScript'} · {round(a['lines'] / 1000) or 1}k lines", 11, anchor="end"))
        n = a["piiflow"]["flows"]
        body.append(f'<rect x="{L}" y="{y + 2}" width="{max(x(n) - L, 2):.1f}" height="11" rx="2" fill="{A}"/>')
        body.append(text(L + max(x(n) - L, 2) + 6, y + 12, f"{n:,}", 11.5, INK))
        if a["privado"]["status"] == "ok":
            m = a["privado"]["flows"]
            w = max(x(m) - L, 2 if m else 0)
            body.append(f'<rect x="{L}" y="{y + 16}" width="{w:.1f}" height="11" rx="2" fill="{B}"/>')
            body.append(text(L + w + 6, y + 26, f"{m:,}", 11.5, INK))
        else:
            body.append(text(L + 2, y + 26, "Privado did not finish (out of memory)", 11.5, BAD))
    return svg(W, H, body)


def chart_overlap(shared, totals):
    rows = sorted(shared, key=lambda a: a["lines"]) + [None]
    W, L, R, rowh, top = 860, 190, 24, 30, 70
    H = top + len(rows) * rowh + 20
    body = [text(24, 30, "Where both tools report personal data arriving", 17, INK, weight=600)] + legend([(A, "Both tools"), (MID, "Only piiflow"), (B, "Only Privado")])
    for i, a in enumerate(rows):
        y = top + i * rowh + (10 if a is None else 0)
        o = totals if a is None else a["sink_lines_overlap"]
        total = o["both"] + o["piiflow_only"] + o["privado_only"]
        if a is None:
            body.append(f'<line x1="24" x2="{W - 24}" y1="{y - 6}" y2="{y - 6}" stroke="#c9d3d2"/>')
        body.append(text(L - 12, y + 15, f"All {count(len(shared))}" if a is None else NAMES[a["name"]], 13, INK, "end", 600 if a is None else None))
        cx = L
        for key, color, tcolor in [("both", A, "#ffffff"), ("piiflow_only", MID, "#ffffff"), ("privado_only", B, INK)]:
            v = o[key]
            if not v:
                continue
            w = (W - L - R) * v / total
            body.append(f'<rect x="{cx:.1f}" y="{y + 2}" width="{max(w - 1, 1):.1f}" height="20" fill="{color}"/>')
            if w > 24:
                body.append(text(cx + w / 2, y + 16, f"{v:,}", 11.5, tcolor, "middle"))
            cx += w
    return svg(W, H, body)


def chart_classes(shared):
    classes = [("log", "Logs", "#2c4446"), ("http", "Other servers (HTTP)", "#557a78"), ("third_party", "Third-party SDKs", "#86a8a5"),
               ("llm", "LLM providers", "#b4c9c7"), ("browser_storage", "Browser storage", "#dbe5e4")]
    W, L, R, H = 860, 110, 24, 190
    body = [text(24, 30, "Where each tool's flows end", 17, INK, weight=600)]
    x = 24
    for _, label, color in classes:
        body.append(f'<rect x="{x}" y="44" width="11" height="11" rx="2" fill="{color}" stroke="#d5dddc"/>')
        body.append(text(x + 17, 54, label, 12.5))
        x += 17 + 7.2 * len(label) + 22
    for i, tool in enumerate(["piiflow", "privado"]):
        t = {}
        for a in shared:
            for k, v in (a[tool].get("by_class") or {}).items():
                t[k] = t.get(k, 0) + v
        total = sum(t.values())
        y = 80 + i * 52
        body.append(text(L - 12, y + 16, "piiflow" if tool == "piiflow" else "Privado", 13, INK, "end"))
        body.append(text(L - 12, y + 31, f"{total:,} flows", 11, anchor="end"))
        cx = L
        for j, (key, _, color) in enumerate(classes):
            v = t.get(key, 0)
            if not v:
                continue
            w = (W - L - R) * v / total
            body.append(f'<rect x="{cx:.1f}" y="{y}" width="{max(w - 1, 1):.1f}" height="26" fill="{color}"/>')
            if w > 34:
                body.append(text(cx + w / 2, y + 17, f"{round(100 * v / total)} %", 11, "#ffffff" if j < 2 else INK, "middle"))
            cx += w
    return svg(W, H, body)


def chart_gaps(apps):
    names = {"unresolved_callee": "Call into an unknown library", "unresolved_method": "Method on an object of unknown type",
             "dynamic_call": "Call through a function value", "depth_bound": "Call chain deeper than the limit",
             "unsupported_framework": "Unsupported framework", "unsupported_construct": "Unsupported construct"}
    t = {}
    for a in apps:
        for k, v in a["piiflow"]["gaps_by_kind"].items():
            t[k] = t.get(k, 0) + v
    rows = sorted(t.items(), key=lambda kv: -kv[1])
    W, L, rowh, top = 860, 280, 30, 56
    H = top + len(rows) * rowh + 16
    body = [text(24, 30, "Where piiflow could not see further, by kind", 17, INK, weight=600)]
    for i, (k, v) in enumerate(rows):
        y = top + i * rowh
        body.append(text(L - 12, y + 13, names.get(k, k), 13, INK, "end"))
        w = (W - L - 90) * v / rows[0][1]
        body.append(f'<rect x="{L}" y="{y + 3}" width="{max(w, 2):.1f}" height="12" rx="2" fill="{A}"/>')
        body.append(text(L + max(w, 2) + 6, y + 13, f"{v:,}", 11.5, INK))
    return svg(W, H, body)


def fmt_w(w):
    return "waiting for labels" if not w else f"**{pct(w['p'])}** ({pct(w['lo'])}–{pct(w['hi'])}, {w['n']:,} checked)"


def main():
    D = json.loads((BENCH / "results/overview.json").read_text())
    apps, S, R = D["applications"], D.get("scores"), D.get("release")
    HH = S and S.get("head_to_head")
    shared = [a for a in apps if a["privado"]["status"] == "ok"]
    failed = [NAMES[a["name"]] for a in apps if a["privado"]["status"] != "ok"]
    by_lines = sorted(apps, key=lambda a: a["lines"])
    items = sum(a["sample"]["items"] for a in apps)
    labelled = sum(a["sample"]["labelled_r1"] + a["sample"]["labelled_r2"] for a in apps)
    pf_flows = sum(a["piiflow"]["flows"] for a in apps)
    pv_flows = sum(a["privado"]["flows"] for a in shared)
    totals = {k: sum(a["sink_lines_overlap"][k] for a in shared) for k in ("both", "piiflow_only", "privado_only")}
    union = sum(totals.values())
    maybe = sum(a["piiflow"]["flows_maybe_personal"] for a in apps)
    polar = next(a for a in apps if a["name"] == "polar")
    gaps = sum(a["piiflow"]["gaps"] for a in apps)

    charts = HERE / "charts"
    charts.mkdir(exist_ok=True)
    (charts / "flows.svg").write_text(chart_flows(apps))
    (charts / "overlap.svg").write_text(chart_overlap(shared, totals))
    (charts / "classes.svg").write_text(chart_classes(shared))
    (charts / "gaps.svg").write_text(chart_gaps(apps))

    L = ["# piiflow and Privado, on code neither was built for", "",
         "<!-- Generated by benchmark/report/markdown.py from benchmark/results/overview.json; do not edit by hand. -->", "",
         "Two static analysers that look for personal data leaving an application: into logs, third-party services, LLMs "
         f"and other servers. Both ran on the same {count(len(apps))} open-source applications. Two people now check, by hand and "
         "without knowing which tool said what, whether each reported flow is real and which real ones each tool missed.", ""]
    if HH:
        L += ["**Status: results are in.** Both reviewers labelled every item and settled their disagreements.", ""]
    else:
        L += [f"**Status: waiting for the labels.** {labelled:,} of {2 * items:,} labels done ({items:,} items, each labelled by both "
              "reviewers). Until then nothing here says which tool is more accurate; the counts below show what each tool reports.", ""]
    if R:
        L += [f"**Version tested: piiflow {R['tag'].lstrip('v')}**, released {R['date']}. "
              + (f"The released binary reproduces all {R['total']} applications' results byte for byte, so these are the numbers you get when you install it. "
                 if R["identical"] == R["total"] else f"The released binary reproduces {R['identical']} of {R['total']} applications' results byte for byte. ")
              + "Speed and memory are in the [README](../README.md#how-fast-is-it).", ""]

    L += ["## The comparison", "",
          f"Measured on the {count(len(shared))} applications both tools completed."
          + (f" Privado could not finish {listing(failed)}, so {'they are' if len(failed) > 1 else 'it is'} left out for both tools." if failed else ""), "",
          "| Question | piiflow | Privado |", "| --- | --- | --- |"]
    pr, rc = (HH or {}).get("precision") or {}, (HH or {}).get("recall") or {}
    L += [f"| Right when it reports a flow | {fmt_w(pr.get('piiflow'))} | {fmt_w(pr.get('privado'))} |",
          f"| Finds the real flows | {fmt_w(rc.get('piiflow_found'))} | {fmt_w(rc.get('privado_found'))} |",
          f"| Finds them, or flags the place it could not see | {fmt_w(rc.get('piiflow_flagged'))} | does not flag |",
          f"| Right, counting only its findings (not its “maybe personal” reports) | {fmt_w(pr.get('piiflow_raised'))} | no such split |", ""]
    if rc:
        pc = rc["paired"]
        L += [f"Of {sum(pc.values()):,} sampled places that really send personal data out: both tools found {pc['both']:,}, only piiflow "
              f"{pc['piiflow_only']:,}, only Privado {pc['privado_only']:,}, and neither {pc['neither']:,}.", ""]

    L += ["## How to read the numbers", "",
          "- **Right when it reports a flow.** Of the flows a tool reports, the share a reviewer confirms: the data can really get from "
          "where it is read to where it is sent, and it really is personal data. High means few false alarms.",
          "- **Finds the real flows.** Of the places that really send personal data out, the share where the tool reports a flow. The "
          "places are a random sample of every call that looks like logging, an API or an SDK, checked by hand. High means little is missed.",
          "- **Flags what it cannot see.** piiflow also reports places where its analysis stopped (an unknown library, a dynamic call). A "
          "real flow behind such a place is not found, but it is not hidden either: someone is told to look. Privado reports nothing comparable.",
          "- **The range after each number.** Every number comes from a sample, so it has a 95 % confidence range: the true value is very "
          "likely inside it. When the two tools' ranges overlap, the difference may be chance.", ""]

    L += ["## Is the comparison fair?", "", "In its favour:", "",
          "- **Same code for both.** The same applications at the same commits and the same folders. Each tool leaves out tests and "
          "generated code by its own default rules, which are close but not identical.",
          "- **Chosen before either tool ran.** Open-source applications under permissive licences, picked by written rules; every "
          "rejected candidate is listed with the reason ([SELECTION.md](SELECTION.md)). piiflow was not developed or tuned on any of them.",
          "- **Checked blind.** Each reviewer labels alone. Flows from both tools are shown in the same format and shuffled together, "
          "so a reviewer cannot tell which tool reported which.",
          "- **Privado at its best.** Its newest rules, run offline from a pinned image, without the telemetry its command-line tool sends.",
          "", "Its limits:", ""]
    if failed:
        L.append(f"- **Privado finished {count(len(shared))} of {count(len(apps))}.** It ran out of memory on {listing(failed)} every time, "
                 f"at the protocol's 14 GiB and again with swap added. Both tools are compared on the {count(len(shared))} it finished.")
    L += ["- **Privado calls its JavaScript and TypeScript support beta.** Half the applications are TypeScript. That is part of what "
          "is being measured; the full results are also broken down by language.",
          "- **Not compared:** how each tool names the kind of data (the two vocabularies only partly line up). Speed is compared "
          "separately, on one machine, in the [README](../README.md#how-fast-is-it).", "",
          "Everything is fixed in writing before labelling, with every later change recorded: [PROTOCOL.md](PROTOCOL.md).", ""]

    L += ["## What the tools reported", "",
          "Counts from the runs, before any checking. They show where each tool looks and how often they agree, not whether they are right.", "",
          f"- **They rarely point at the same place.** Of {union:,} places where at least one tool reports personal data arriving, they "
          f"agree on {totals['both']:,} ({pct(totals['both'] / union)}). At most of the rest one of them is wrong; the labels decide which.",
          f"- **piiflow reports far more.** {pf_flows:,} flows against Privado's {pv_flows:,}. {pct(polar['piiflow']['flows'] / pf_flows)} "
          "of piiflow's come from one large application, Polar, mostly log lines.",
          f"- **Many of piiflow's reports are tentative.** {pct(maybe / pf_flows)} start from a field whose name is only possibly personal, "
          "such as `name`. piiflow marks those for review rather than as findings, which is why the comparison shows it both ways.",
          f"- **piiflow says where it is blind.** {gaps:,} places across the applications where its analysis stopped. Every scan ends "
          "with “coverage incomplete” rather than a clean result.", "",
          "![Flows reported per application, piiflow and Privado](report/charts/flows.svg)", "",
          "![Where both tools report personal data arriving](report/charts/overlap.svg)", "",
          "![Where each tool's flows end](report/charts/classes.svg)", "",
          "![Where piiflow could not see further](report/charts/gaps.svg)", ""]

    L += ["## The applications", "",
          "| Application | Language | Framework | Lines | piiflow flows | Blind spots | Privado flows |",
          "| --- | --- | --- | ---: | ---: | ---: | ---: |"]
    for a in by_lines:
        fw = a["framework"].replace(" (unsupported)", " (not modelled by piiflow)").replace(" (not modelled)", " (not modelled by piiflow)")
        pv = f"{a['privado']['flows']:,}" if a["privado"]["status"] == "ok" else "did not finish"
        L.append(f"| [{NAMES[a['name']]}](https://github.com/{a['repository']}) | {'Python' if a['language'] == 'python' else 'TypeScript'} | "
                 f"{fw} | {a['lines']:,} | {a['piiflow']['flows']:,} | {a['piiflow']['gaps']:,} | {pv} |")
    L += ["", "## What the reviewers label", "",
          "Samples are drawn by a fixed rule, so anyone can draw the same ones and check the labels ([REVIEWERS.md](REVIEWERS.md)).", "",
          "| Application | piiflow flows | Privado flows | Blind spots | Call sites | Items | Reviewer 1 | Reviewer 2 |",
          "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"]
    for a in by_lines + [None]:
        s = ({k: sum(x["sample"][k] for x in apps) for k in apps[0]["sample"]} if a is None else a["sample"])
        name = "**Total**" if a is None else NAMES[a["name"]]
        L.append(f"| {name} | {s['flows_piiflow']:,} | {s['flows_privado']:,} | {s['gaps']:,} | {s['sites']:,} | {s['items']:,} | "
                 f"{s['labelled_r1']:,} of {s['items']:,} | {s['labelled_r2']:,} of {s['items']:,} |")
    L += ["", "---", "",
          f"Benchmark run recorded at piiflow commit `{D['generated_from']['piiflow_commit'][:7]}`"
          + (f", whose analysis code is identical to the {R['tag']} release" if R and R.get("analysis_identical") else "")
          + "; output byte-identical on every rerun. Privado: privado-core 1.1.175 from a pinned image, newest rules (v1.3.91), "
          "networking off. Regenerate with `python3 benchmark/overview.py && python3 benchmark/report/markdown.py`.", ""]
    (BENCH / "REPORT.md").write_text("\n".join(L))
    print(f"wrote benchmark/REPORT.md and {len(list(charts.glob('*.svg')))} charts")


if __name__ == "__main__":
    main()
