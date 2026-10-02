# Benchmark

What is measured today, how, and what is not measured yet. Numbers are from `piiflow` 0.1.0
(pre-release) on an Apple M3 Pro, release build, unless a row says otherwise. Reproduce any of
them with the commands given; nothing here depends on the network except cloning the repositories.

## Accuracy

### Rule fixtures

[`tests/fixtures/`](../tests/fixtures/): 52 programs, a failing and a passing case for every rule in
TypeScript and Python plus behaviour cases (frameworks, cross-file summaries, instance isolation,
typed injection, monorepo resolution, coverage gaps). Expected findings are written as markers on
the sink lines (`// expect: PF001`), and the test requires the reported set of (rule, file, line)
to equal the markers exactly.

| | Result |
| --- | --- |
| Fixtures | 52 |
| Expected findings | 41 |
| Reported findings that match | 41 (no missing, no unexpected) |
| Datalog oracle agrees with the worklist engine | 52 of 52 |

Being written alongside the code, fixtures show the rules do what they say; they are not a
measure of accuracy on code nobody wrote for the tool.

### Conformance corpus

[`conformance/`](../conformance/README.md): 32 small programs with hand-written expected flows,
designed to be scored by any analyser through an external-runner contract.

```bash
python3 conformance/run.py --verifier "piiflow -q scan --walk -f facts"
```

| | Result |
| --- | --- |
| Required vectors | 30, all pass |
| Known-limitation vectors | 2, both fail as documented (flow-insensitivity) |
| Flow-level precision / recall on required vectors | 1.00 / 1.00 |
| Deliberately broken analysers the corpus rejects | 5 of 5 (field sensitivity, interprocedural binding, instance isolation, sanitisers, field stores) |

The corpus is an answer key for specific behaviours, not a sample of real code.

### Real-world corpus (labelled precision and recall): not yet

The spec asks for permissively licensed open-source applications, referenced by commit SHA and
never vendored, with known flows labelled by hand by two reviewers, and precision and recall per
rule; and, where fair and reproducible, Privado's open-source scanner run on the same corpus with
both numbers published with the method. That is milestone M6 and has not been done. The two
repositories below are candidates for it; the numbers are what `piiflow` reports, unlabelled.

| Repository | Commit | Licence | Language | Files | Lines |
| --- | --- | --- | --- | --- | --- |
| [documenso/documenso](https://github.com/documenso/documenso) | `8a41a3bf618d9e46e2e1c0f437aa0488d91b85de` | AGPL-3.0 | TypeScript | 2,001 | 220,640 |
| [Netflix/dispatch](https://github.com/Netflix/dispatch) | `dd2837e82a0bf5565b1b4b4b91ea30b7262d4061` | Apache-2.0 | Python (+ Vue/JS) | 795 | 96,603 |

documenso's licence is AGPL-3.0, which allows analysis and publishing results; M6 will prefer
permissively licensed repositories (MIT, Apache-2.0, BSD) for the labelled corpus, as the spec
says.

| | documenso | Dispatch |
| --- | --- | --- |
| Flows | 1,448 | 2,549 |
| Findings at `high` / `medium` / `warning` | 35 / 31 / 84 | 38 / 560 / 231 |
| Findings at `info` (review queue: maybe-personal names) | 607 | 1,832 |
| Coverage gaps | 76 | 112 |
| Processors reached | Amazon S3, Amazon SES, Azure Storage, Inngest, PostHog, Stripe | Slack, api.opsgenie.com, graph.microsoft.com |

Informal review while developing (one reviewer, not a labelled measurement): Dispatch's
medium-severity PF001 findings are mostly participant email addresses in debug and info log
lines, which are true positives as the rule defines them. Several documenso PF002 findings sent
document file names (maybe-personal, `info`) to object storage. Remaining false positives seen were
mostly imprecision through generic propagation (an unknown library assumed to pass its arguments
to its result) and the one-level field sensitivity of plain objects.

#### Protocol for M6

1. Pick 8 to 12 permissively licensed applications, half TypeScript and half Python, that use at
   least one sink class each; record each by repository and commit SHA in
   `benchmark/corpus.json` (code is never vendored).
2. Two reviewers independently label every flow `piiflow` reports and every flow they find by
   reading the code's sink calls (true positive, false positive, missed), with a citation per
   label; disagreements are resolved by discussion and the agreement rate is published.
3. Publish per-rule precision and recall with confidence intervals, the labels, and the scripts.
4. Run Privado's open-source scanner (pinned version and configuration) on the same commits, map
   its data-flow output to (source, sink, category) as fairly as its output allows, and publish its
   numbers next to ours, with the mapping and every judgement call written down.
5. Archive the corpus definition, labels and results on Zenodo for a DOI, as `acc`'s conformance
   corpus is.

## Performance

| Input | Lines | Wall time | Peak memory |
| --- | --- | --- | --- |
| Synthetic TypeScript service ([`benches/synthetic.py`](../benches/synthetic.py)) | 98,073 | 1.0–1.5 s | 360–500 MB |
| Synthetic TypeScript service | 392,292 | 5.9 s | 1.5 GB |
| documenso (TypeScript monorepo) | 220,640 | 1.6–2.2 s | 360 MB |
| Dispatch (Python) | 96,603 | 1.3–1.8 s | 250 MB |
| Fixture tree (criterion) | — | 14.4 ms | — |

Per phase on documenso: lowering 123 ms, program and provenance 893 ms, facts 229 ms, engine
118 ms, report 60 ms (`piiflow scan --timings`). The spec's target, a 100k-line TypeScript service
in under five seconds on a current laptop, is met with room to spare; memory grows with the
program's size (about 3.5 KB per line on the synthetic service), which is roughly with file count
for files of similar size.

CI enforces a coarse budget on the 100k-line synthetic service (10 s, 1,500 MiB on a shared
runner) with [`.github/scripts/perf_budget.py`](../.github/scripts/perf_budget.py), which catches an
accidental quadratic rather than a few percent. `cargo bench` runs the criterion benchmarks,
including the engine comparison in [ADR 0002](adr/0002-flow-engine.md).

## Determinism

Every fixture is analysed with 1 and 8 threads, twice each, and compared byte for byte; every
fixture's output is compared with a committed golden; CI runs both on Linux and macOS.
