# Benchmark protocol (M6)

How precision and recall of `piiflow` (and, for comparison, Privado's open-source scanner) are
measured on real applications. This file is fixed before any labelling starts; a change after
that is a deviation and is recorded under [Deviations](#deviations) with its date and reason.

## 1. Corpus

[`corpus.json`](corpus.json): 12 applications, 6 TypeScript and 6 Python, each pinned by commit
SHA and fetched by [`fetch.py`](fetch.py) into `.benchmark-cache/` (never committed, never
vendored). Selection criteria, fixed before `piiflow` was run on any candidate, and every
candidate considered with the reason it was or was not chosen, are in [SELECTION.md](SELECTION.md).

**Scope.** Each application is analysed from the root of its deployable unit (the directory with
its `package.json` or `pyproject.toml`), as a team would run the tool. The only configuration is
`exclude` for code in another language's frontend or theme assets, listed in `corpus.json`. No
sinks, sources, sanitisers or processors are declared: the benchmark measures the tool as
shipped.

## 2. Held out, and frozen

- None of these applications was used while developing `piiflow`. documenso and Netflix Dispatch,
  which were, are not in the corpus.
- **Freeze.** The `piiflow` commit under test is recorded in `results/summary.json` by
  [`run.py`](run.py). From that commit until the labels are final, no change to the engine,
  catalogue or rules may be informed by what the tool reports on these applications. A crash or
  a failure to run is the only exception, fixed and recorded as a deviation.
- Improvements made after the labels are final are measured as a separate run against the same
  labels; new flows such a run reports are labelled with the same procedure and reported apart.

## 3. Units and labels

Three kinds of item, each labelled by **two reviewers independently**:

| Kind | What it is | Label |
| --- | --- | --- |
| `flow` | A flow a tool reports: a source location, a sink call, categories, and the cited path | `verdict`: `tp`, `fp_path` (data cannot travel this way), `fp_not_personal` (it can, but the source is not personal data), `unsure`; `category_ok`: whether at least one reported category is right |
| `gap` | A `piiflow` coverage gap (PFC01) | `hides_flow`: `yes` (personal data does reach a sink through the unresolved construct), `no`, `unsure` |
| `site` | A sink call site from the independent enumeration (§4) | `personal`: `yes` / `no` / `unsure`; if `yes`, the categories and the location of one source |

Every `tp`, `yes` and `fp_*` label carries a `citation`: a `file:line` in the application, and for
`fp_path` the place where the reported path breaks. Definitions follow the rule pages: personal
data is anything the [classification table](../vendor/classification/classification.json) or
Fideslang treats as `user.*`; credentials count. A log line that prints only an identifier the
application generated (a UUID primary key) is personal data (`user.unique_id`).

A **finding** is correct when its flow is `tp` and, for a rule that depends on the category
(PF003 special categories, PF004 children's data, PF006 credentials), `category_ok` is true.

## 4. Independent sink sites (for recall)

Recall cannot be measured from what a tool reports. [`candidates.py`](candidates.py) enumerates
call sites by syntax alone: calls whose callee name matches broad patterns for logging, HTTP,
messaging, analytics, error tracking, LLM and storage APIs (`log`, `info`, `warn`, `error`,
`debug`, `print`, `send`, `post`, `request`, `fetch`, `track`, `capture`, `identify`, `create`,
`complete`, `put_object`, …), **without reading `piiflow`'s catalogue**. Reviewers also add any
sink they meet while labelling that the enumeration missed (`added_by_reviewer: true`); added
sites are reported separately so the sampled estimate stays unbiased.

A tool **finds** a site when it reports a flow whose sink is on that line. `piiflow` **flags** a
site when it either finds it or reports a coverage gap located on that line (the PFC01 promise:
not seen, but not claimed clean).

## 5. Sampling

Labelling every reported flow is not feasible (Dispatch alone has 2,549). Samples are drawn
deterministically by [`sheets.py`](sheets.py): items are ordered by
`sha256("privacy-flow-benchmark-v1" ‖ repository ‖ item id)` and the first *k* of each stratum are
taken.

| Stratum (per application) | k |
| --- | --- |
| `piiflow` findings at `high`, `medium` or `warning`, per rule PF001–PF006 | 8 |
| `piiflow` findings at `info` (maybe-personal names) | 10 |
| `piiflow` coverage gaps | 10 |
| Privado flows, per sink type (leakages, third parties, internal APIs) | 8 |
| Candidate sink sites | 30 |

Per-rule precision is reported both as the pooled sample proportion with a 95 % Wilson interval
and as a stratum-weighted estimate (each application's sample proportion weighted by its number
of findings for that rule). Recall is the proportion of sampled `personal: yes` sites a tool finds
(and, for `piiflow`, flags), with a Wilson interval, pooled and per language.

## 6. Blinding

[`sheets.py`](sheets.py) renders every `flow` item the same way whichever tool reported it: the
hop locations, each with its line of code read from the fetched application, and the reported
categories mapped to Fideslang. Items get opaque IDs and are shuffled together; the key that maps
items to tools is written to `sheets/key/` and reviewers do not open it until §7 is done.

## 7. Procedure

1. `python3 benchmark/fetch.py && python3 benchmark/run.py` — analyse, record the freeze commit,
   timings and output digests (byte-identical on rerun).
2. Privado's run (§8) on GitHub Actions; its outputs are mapped by `privado/map.py`.
3. `python3 benchmark/sheets.py` — candidate sites, samples, rendered items and empty label files
   `labels/R1/<app>.yml` and `labels/R2/<app>.yml`.
4. Each reviewer labels alone, without seeing the other's labels or the key.
5. `python3 benchmark/score.py --agreement` — Cohen's κ per item kind, before discussion.
6. Disagreements are resolved by discussion into `labels/final/<app>.yml`, each with a one-line
   reason. `unsure` that remains after discussion is excluded from the denominators and counted.
7. `python3 benchmark/score.py` — results to `results/scores.json` and `results/scores.md`.
8. Publish: corpus, protocol, labels (both reviewers' and final), key, scripts and results, archived
   on Zenodo for a DOI.

Reviewers are recorded as `R1` and `R2` in the label files; their names and roles appear in the
write-up with their consent.

## 8. Comparison with Privado

Privado's open-source engine (`privado-core`, LGPL-3.0) is run as a fixed image digest with
networking disabled, through [`.github/workflows/benchmark-privado.yml`](../.github/workflows/benchmark-privado.yml),
on the same commits and scopes. Every judgement call is written down here before the run:

- **Engine, not CLI.** `privado-cli` sends telemetry to Privado on every run regardless of its
  metrics setting, so the image is invoked directly (`core scan`) with `--network none`,
  `--skip-upload`, `--skip-download-dependencies`, `--offline-mode` and
  `PRIVADO_METRICS_ENABLED=false`.
- **Version.** Image `public.ecr.aws/privado/privado@sha256:349fdd5a01c01acb4b17c9db0df782c8ad2fb4d87b5940247720fc3cac8e7f15`
  (built 2024-11-04; the engine's last release is v1.1.206, 2024-08-28) with the rules repository
  `Privado-Inc/privado` at tag `v1.3.91` mounted, the newest rules, so LLM SDKs added since the
  image was built are known to it.
- **Language.** One language per application (`-fl javascript` or `-fl python`), matching
  `corpus.json`. Privado describes JavaScript/TypeScript support as beta; that is reported.
- **Exclusions.** Privado's default exclusions (tests, specs, mocks) are kept; they match
  `piiflow`'s defaults closely. `corpus.json` excludes are passed to it as well.
- **Deduplication** stays at Privado's default; path limits stay at their defaults.
- **Mapping.** `privado/map.py` turns each `dataFlow` path into (source location, sink location,
  categories, sink class): `leakages` → log; `third_parties` with an SDK id → third party, and
  LLM when the id names an LLM provider; `third_parties` API and `internal_apis` → HTTP;
  `storages` are not sinks for either tool and are dropped (and counted). Categories go through
  the crosswalk in `privado/crosswalk.json` (Privado data element → Fideslang), one line each,
  reviewed before the run.
- **Matching for recall** is by sink line only, the same rule as for `piiflow`.
- **Timing** is not compared: the image is amd64-only and runs on a different machine.

## Deviations

None yet.
