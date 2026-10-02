#!/usr/bin/env python3
"""Build the benchmark report page from benchmark/results/overview.json.

    python3 benchmark/overview.py                 # refresh the data first
    python3 benchmark/report/build.py <out.html> [--scores scores.json]

The page embeds the overview (and the labelled scores, when score.py has produced them) and
draws everything from it; no number is typed into the template. `--scores` overrides the scores
for a preview, for example from a known-answer run.
"""
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent


def main():
    args = sys.argv[1:]
    scores = None
    if "--scores" in args:
        i = args.index("--scores")
        scores = json.loads(pathlib.Path(args[i + 1]).read_text())
        del args[i:i + 2]
    if len(args) != 1:
        sys.exit(__doc__)
    data = json.loads((HERE.parent / "results/overview.json").read_text())
    if scores is not None:
        data["scores"] = scores
    payload = json.dumps(data, separators=(",", ":"), ensure_ascii=False).replace("</", "<\\/")
    page = (HERE / "page.html").read_text().replace("__DATA__", payload)
    pathlib.Path(args[0]).write_text(page)
    print(f"wrote {args[0]} ({len(page) / 1000:.0f} kB)")


if __name__ == "__main__":
    main()
