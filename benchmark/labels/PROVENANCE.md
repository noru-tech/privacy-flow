# Provenance of the imported labels

The labels in `R1/` and `R2/` were imported on 2026-10-03 with
`benchmark/labelling/import_labels.py` from two partial exports. This file records where they came
from and what is known about how they were made, so that whoever scores, adjudicates or publishes
them can judge that for themselves.

## What was imported

| File | SHA-256 | Reviewer field | `exported_at` | Items |
| --- | --- | --- | --- | --- |
| `labels-R1-partial-609.json` | `a01c206ca9ed8cae8f71cb76f50ea618ca9248378e8882e9bd6f02f1d2d97b42` | R1 | `2026-10-03T10:49:08.195Z` | 609 (10 applications complete) |
| `labels-R2-partial-641.json` | `6ed2672ba632bfcb2dcad2b79f63ed686bdb9525265a36ba6c97542059bfce86` | R2 | `2026-10-03T10:49:16.277438+00:00` | 641 (10 applications complete, Polar 32 of 81) |

Not yet labelled: Polar and PrivateGPT for R1; PrivateGPT and the rest of Polar for R2.

## What is known about how they were made

- The benchmark's owner states that both files were labelled by the two human reviewers in the
  labelling desk, in private browser windows, and downloaded to a scratch directory.
- The files were found in a local Codex working directory
  (`~/.codex/visualizations/2026/10/03/…/piiflow-review/continued-to-1-percent/`), next to a
  `README.txt` (SHA-256 `71118113f6ea40027648240cdb6aa62015dba1f53f39861656631327697f1a17`) and a
  `status.json` (SHA-256 `646328cfb0524f9dff62eaf17534916f2a8d5f3c7533b80c0c52a2587831ef95`) that
  describe them as checkpoints of a labelling run that "stopped when the usage tool reported 1%
  remaining", with "browser tabs … marked for preservation", and that add 26 (R1) and 12 (R2)
  labels to an earlier checkpoint of 583 and 629.
- The labelling desk writes `exported_at` with JavaScript's `toISOString()` (ending in `Z`); the R2
  file's timestamp has Python's `isoformat()` form, so the R2 file was written or rewritten outside
  the desk.
- Every R2 item and 576 of the 609 R1 items carry a note, and several notes on the same item are
  close paraphrases of each other across the two files.
- The desk stores labels server-side, per signed-in claude.ai account, not in the browser; it could
  not be checked whether the reviewers' desk stores hold these labels.

## Before these are used

`benchmark/PROTOCOL.md` requires two independent human reviewers. Until the provenance above is
confirmed, for example by each reviewer exporting from the desk directly and the exports matching
these files, agreement (κ) and precision computed from them should not be published as human
ground truth.
