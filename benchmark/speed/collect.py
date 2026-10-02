#!/usr/bin/env python3
"""Collect the speed benchmark's per-application results into results, a chart and the README.

    python3 benchmark/speed/collect.py <dir of <app>.json files> [--write]

Without --write, prints the README section. With --write, writes benchmark/speed/results.json,
benchmark/speed/chart.svg, and replaces the README's section between the speed markers. Every
number in the README comes from here.
"""
import json
import math
import pathlib
import statistics
import sys
from xml.sax.saxutils import escape

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
START, END = "<!-- speed:start -->", "<!-- speed:end -->"
NAMES = {"vercel-chatbot": "Vercel chatbot", "umami": "Umami", "taxonomy": "Taxonomy", "open-saas": "Open SaaS",
         "ghost": "Ghost", "hoppscotch": "Hoppscotch", "healthchecks": "Healthchecks", "redash": "Redash", "ctfd": "CTFd",
         "fastapi-template": "FastAPI template", "polar": "Polar", "private-gpt": "PrivateGPT"}


def load(src):
    corpus = json.loads((ROOT / "benchmark/corpus.json").read_text())
    summary = {a["name"]: a for a in json.loads((ROOT / "benchmark/results/summary.json").read_text())["applications"]}
    rows, machines = [], []
    for entry in corpus["repositories"]:
        f = pathlib.Path(src) / f"{entry['name']}.json"
        if not f.exists():
            continue
        r = json.loads(f.read_text())
        runs = r["piiflow"]["runs"]
        pv = r["privado"]
        machines.append(r["machine"])
        rows.append({
            "app": entry["name"], "repository": entry["repository"], "language": entry["language"],
            "lines": sum(l["lines"] for l in summary[entry["name"]]["languages"]),
            "piiflow_seconds": round(statistics.median(x["wall_seconds"] for x in runs), 2),
            "piiflow_runs": [x["wall_seconds"] for x in runs],
            "piiflow_peak_mib": max(x["peak_rss_mib"] or 0 for x in runs),
            "piiflow_identical_to_recorded_run": r["piiflow"]["identical_to_recorded_run"],
            "piiflow_version": r["piiflow"]["version"],
            "privado_seconds": pv["wall_seconds"], "privado_peak_mib": pv["peak_memory_mib"],
            "privado_finished": pv["finished"], "privado_out_of_memory": pv["out_of_memory"],
            "privado_memory_limit_gib": pv["memory_limit_gib"], "privado_image": pv["image"],
        })
    rows.sort(key=lambda r: r["lines"])
    return rows, machines


def fmt_s(s):
    if s >= 120:
        return f"{s / 60:.1f} min"
    return f"{s:.2f} s" if s < 10 else f"{s:.1f} s" if s < 100 else f"{s:,.0f} s"


def fmt_mib(m):
    return f"{m:,} MiB" if m < 1024 else f"{m / 1024:.1f} GiB"


def cpus_text(machines):
    """The CPU models the jobs ran on; GitHub assigns runners from a pool, so they can differ."""
    counts = {}
    for m in machines:
        counts[m.get("cpu") or "unknown CPU"] = counts.get(m.get("cpu") or "unknown CPU", 0) + 1
    if len(counts) == 1:
        return next(iter(counts))
    return ", ".join(f"{cpu} ({n} jobs)" for cpu, n in sorted(counts.items(), key=lambda kv: -kv[1]))


def chart(rows, machine, version, machines):
    W, L, R, rowh, top = 860, 190, 110, 40, 64
    H = top + len(rows) * rowh + 70
    # The axis runs past the longest bar so its label fits before the ratio column.
    lo, hi = 0.01, 10000.0
    x = lambda v: L + (W - L - R) * (math.log10(max(v, lo)) - math.log10(lo)) / (math.log10(hi) - math.log10(lo))
    ink, sub, grid, a, b, bad = "#121a1b", "#5f6b6a", "#e3e8e8", "#3a5355", "#9cb8b5", "#b3261e"
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="-apple-system, Segoe UI, Helvetica, Arial, sans-serif">',
           f'<rect width="{W}" height="{H}" rx="10" fill="#ffffff"/>',
           f'<text x="24" y="32" font-size="17" font-weight="600" fill="{ink}">Time to scan one application (log scale)</text>',
           f'<rect x="24" y="44" width="11" height="11" rx="2" fill="{a}"/><text x="41" y="54" font-size="12.5" fill="{sub}">piiflow {escape(version)}</text>',
           f'<rect x="150" y="44" width="11" height="11" rx="2" fill="{b}"/><text x="167" y="54" font-size="12.5" fill="{sub}">Privado (privado-core 1.1.175)</text>']
    for t in [0.01, 0.1, 1, 10, 100, 1000]:
        out.append(f'<line x1="{x(t):.1f}" x2="{x(t):.1f}" y1="{top - 4}" y2="{H - 46}" stroke="{grid}"/>')
        out.append(f'<text x="{x(t):.1f}" y="{H - 30}" font-size="12" fill="{sub}" text-anchor="middle">{t:g}</text>')
    out.append(f'<line x1="{W - R + 6}" x2="{W - R + 6}" y1="{top - 4}" y2="{H - 46}" stroke="{grid}"/>')
    out.append(f'<text x="{W - 24}" y="{top - 10}" font-size="11" fill="{sub}" text-anchor="end">piiflow is</text>')
    for i, r in enumerate(rows):
        y = top + i * rowh
        out.append(f'<text x="{L - 12}" y="{y + 14}" font-size="13" fill="{ink}" text-anchor="end">{escape(NAMES.get(r["app"], r["app"]))}</text>')
        out.append(f'<text x="{L - 12}" y="{y + 29}" font-size="11" fill="{sub}" text-anchor="end">{"Python" if r["language"] == "python" else "TypeScript"} · {round(r["lines"] / 1000) or 1}k lines</text>')
        pw = x(r["piiflow_seconds"]) - L
        out.append(f'<rect x="{L}" y="{y + 4}" width="{max(pw, 2):.1f}" height="12" rx="2" fill="{a}"/>')
        out.append(f'<text x="{L + max(pw, 2) + 6:.1f}" y="{y + 14}" font-size="11.5" fill="{ink}">{fmt_s(r["piiflow_seconds"])}</text>')
        vw = x(r["privado_seconds"]) - L
        if r["privado_finished"]:
            out.append(f'<rect x="{L}" y="{y + 19}" width="{max(vw, 2):.1f}" height="12" rx="2" fill="{b}"/>')
            out.append(f'<text x="{L + max(vw, 2) + 6:.1f}" y="{y + 29}" font-size="11.5" fill="{ink}">{fmt_s(r["privado_seconds"])}</text>')
            ratio = r["privado_seconds"] / r["piiflow_seconds"]
            out.append(f'<text x="{W - 24}" y="{y + 22}" font-size="13" font-weight="600" fill="{ink}" text-anchor="end">{ratio:,.0f}× faster</text>')
        else:
            why = "out of memory" if r["privado_out_of_memory"] else "did not finish"
            out.append(f'<rect x="{L}" y="{y + 19}" width="{max(vw, 2):.1f}" height="12" rx="2" fill="none" stroke="{bad}" stroke-dasharray="4 3"/>')
            out.append(f'<text x="{L + max(vw, 2) + 6:.1f}" y="{y + 29}" font-size="11.5" fill="{bad}">{why} after {fmt_s(r["privado_seconds"])}</text>')
    out.append(f'<text x="24" y="{H - 10}" font-size="11.5" fill="{sub}">Same GitHub-hosted runner for both tools ({machine.get("cpus")} vCPU, {machine.get("memory_gib")} GiB). piiflow: median of 3 runs. Privado: one run, 14 GiB limit.</text>')
    out.append("</svg>")
    return "\n".join(out) + "\n"


def section(rows, machine, version, date, machines):
    corpus = [e["name"] for e in json.loads((ROOT / "benchmark/corpus.json").read_text())["repositories"]]
    pending = [NAMES[n] for n in corpus if n not in {r["app"] for r in rows}]
    done = [r for r in rows if r["privado_finished"]]
    failed = [NAMES[r["app"]] for r in rows if not r["privado_finished"]]
    ratios = [r["privado_seconds"] / r["piiflow_seconds"] for r in done]
    identical = sum(r["piiflow_identical_to_recorded_run"] for r in rows)
    cpu_models = len({m.get("cpu") for m in machines})
    pf_t = [r["piiflow_seconds"] for r in rows]
    pv_t = [r["privado_seconds"] for r in done]
    lead = (f"**On the same machine, piiflow scanned each application in {fmt_s(min(pf_t))} to {fmt_s(max(pf_t))}, "
            f"using at most {fmt_mib(max(r['piiflow_peak_mib'] for r in rows))} of memory. Privado took "
            f"{fmt_s(min(pv_t))} to {fmt_s(max(pv_t))} and up to {fmt_mib(max(r['privado_peak_mib'] for r in done))}"
            + (f", and ran out of memory on {' and '.join(failed)}" if failed else "") + ".** "
            f"Half the applications ran more than {statistics.median(ratios):,.0f} times faster with piiflow.")
    lines = [START, "", "## How fast is it?", "", lead, "",
             "![Time to scan each application, piiflow and Privado](benchmark/speed/chart.svg)", "",
             "| Application | Lines of code | piiflow | Privado | piiflow memory | Privado memory |",
             "| --- | ---: | ---: | ---: | ---: | ---: |"]
    for r in rows:
        pv = fmt_s(r["privado_seconds"]) if r["privado_finished"] else ("out of memory" if r["privado_out_of_memory"] else "did not finish")
        lines.append(f"| [{NAMES[r['app']]}](https://github.com/{r['repository']}) | {r['lines']:,} | {fmt_s(r['piiflow_seconds'])} | {pv} | "
                     f"{fmt_mib(r['piiflow_peak_mib'])} | {fmt_mib(r['privado_peak_mib'])} |")
    for name in pending:
        lines.append(f"| {name} | | still running | still running | | |")
    lines += ["",
              f"**How this was measured** ({date}). The {len(corpus)} open-source applications of the "
              f"[benchmark corpus](benchmark/SELECTION.md); each one on its own GitHub-hosted runner "
              f"({machine.get('cpus')} vCPU, {machine.get('memory_gib')} GiB; {cpu_models} different CPU model{'s' if cpu_models != 1 else ''} "
              "across jobs, listed in [results.json](benchmark/speed/results.json)), with both tools run one after the other on it. "
              f"piiflow {version} is the released Linux binary, verified by its attestation; the median of 3 runs is shown, and its "
              f"output matched the recorded macOS run byte for byte on {'every' if identical == len(rows) else f'{identical} of {len(rows)}'} application. "
              "Privado is privado-core 1.1.175 from its pinned image with its newest rules, offline, one run, 14 GiB limit. "
              "This measures speed and memory only; whether each tool's findings are *right* is measured by the "
              "[labelled benchmark](benchmark/REPORT.md), which is in progress. "
              "[Method and how to rerun it](benchmark/speed/README.md).", "", END]
    return "\n".join(lines)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    write = "--write" in sys.argv
    if len(args) != 1:
        sys.exit(__doc__)
    rows, machines = load(args[0])
    if not rows:
        sys.exit("no results found")
    machine = machines[0]
    version = rows[0]["piiflow_version"].replace("piiflow ", "")
    import datetime
    date = datetime.date.today().isoformat()
    text = section(rows, machine, version, date, machines)
    if not write:
        print(text)
        return
    (HERE / "results.json").write_text(json.dumps({"measured_on": date, "machines": machines, "applications": rows}, indent=2, sort_keys=True) + "\n")
    (HERE / "chart.svg").write_text(chart(rows, machine, version, machines))
    readme = (ROOT / "README.md").read_text()
    if START not in readme:
        sys.exit("README.md has no speed markers")
    head, rest = readme.split(START, 1)
    _, tail = rest.split(END, 1)
    (ROOT / "README.md").write_text(head + text + tail)
    print(f"wrote results.json, chart.svg and the README section ({len(rows)} applications)")


if __name__ == "__main__":
    main()
