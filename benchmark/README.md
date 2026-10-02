# Real-world benchmark (M6)

Precision and recall of `piiflow` on twelve open-source applications it was not developed
against, labelled by two reviewers, with Privado's open-source scanner run on the same commits.

- [PROTOCOL.md](PROTOCOL.md): units, labels, sampling, blinding, scoring, the Privado judgement
  calls, and deviations. Fixed before labelling.
- [SELECTION.md](SELECTION.md): the criteria and every candidate considered.
- [report/](report/): the readable report page, rebuilt from the results with `python3 benchmark/overview.py && python3 benchmark/report/build.py out.html`.
- [REVIEWERS.md](REVIEWERS.md): how R1 and R2 label, in the labelling desk (`labelling/`).
- [corpus.json](corpus.json): the applications, pinned by commit, with scope and exclusions.
- [results/summary.json](results/summary.json): what `piiflow` reported on each application, the
  commit under test, document digests, wall time and peak memory.

## Status

| Step | State |
| --- | --- |
| Corpus fixed, protocol written | done (2026-10-02) |
| `piiflow` run recorded | done: commit `9a78b08`, byte-identical on repeat |
| Privado run | done for 10 of 12; `polar` and `redash` exceed a standard runner's memory even with swap (recorded as failures) |
| Sheets drawn | done: 752 items per reviewer (313 blinded flows, 114 gaps, 325 sites) |
| Labelling by R1 and R2 | not started; the labelling desk is live |
| Scores, write-up, Zenodo DOI | after labelling |

## Reproduce

```bash
python3 benchmark/fetch.py              # the only step that uses the network
cargo build --release
python3 benchmark/run.py --repeat       # byte-identical digests on every run and machine
python3 benchmark/candidates.py
python3 benchmark/sheets.py             # needs benchmark/results/privado/*.jsonl
python3 benchmark/score.py --agreement
python3 benchmark/score.py
python3 benchmark/overview.py && python3 benchmark/report/build.py report.html
```

`python3 -m unittest benchmark/test_benchmark.py` checks the sampling and statistics. Fetched
applications and full `piiflow` documents stay in `.benchmark-cache/` and are never committed;
the committed items carry locations and categories only, and each sheet is rendered locally with
the code read from the fetched application.
