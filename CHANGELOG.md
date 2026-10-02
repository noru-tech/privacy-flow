# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-10-02

No change to the analysis: for the same input, 0.1.1 reports the same flows, findings and gaps as
0.1.0, and only the version recorded in the outputs differs.

### Added
- A speed and memory benchmark of piiflow and Privado on the twelve applications of the benchmark
  corpus, run on the same GitHub-hosted runners, with its method and results in the repository and
  a summary in the README.
- The benchmark report (`benchmark/REPORT.md`), tied to the released binary.
- A logo, a social preview image, a feature request form and a Discussions link in the issue
  chooser.
- Releases are archived on Zenodo from this release on.

### Changed
- The control-mapping notes no longer say the review is due before the first release; the mapping
  is still pending review.

## [0.1.0] - 2026-10-02

### Added
- `piiflow`, a deterministic, offline static analyser of personal-data flows in TypeScript,
  JavaScript and Python, built on tree-sitter, with a shared IR, provenance-based call resolution
  across files, workspace packages and tsconfig aliases, function summaries, and a worklist engine
  that reconstructs one shortest cited path per (source, sink, category). A Datalog engine
  (ascent) computes the same reachability and is the test oracle.
- Rules PF001 to PF006 and PFC01, each with a documentation page, failing and passing fixtures in
  both languages, and a control mapping pending review.
- The catalogue as YAML: sinks for logs, error tracking, analytics, messaging, LLM providers,
  outbound HTTP, browser storage and other processors; Express, Fastify, Next.js, FastAPI, Flask
  and Django request input; default sanitisers and propagators; supported and unsupported
  frameworks. Projects extend it in `.privacy-flow.yml`, validated by a published schema.
- Commands `scan`, `diff`, `explain`, `check`, `validate`, `rules`, `doctor`, `completions`,
  `manpage`; outputs table, RFC 8785 JSON with a digest, SARIF 2.1.0 with code flows, Fides egress
  declarations, in-toto Statement v1, facts (JSON lines) and a Markdown review queue.
- Dispositions with `acc`'s semantics, carried across scans by line-independent finding IDs.
- Coverage gaps for unresolved calls, dynamic calls, unresolved imports, the depth bound,
  unsupported languages, frameworks and constructs, and parse errors; exit 4 whenever a clean
  result cannot be claimed.
- Ingestion of Fides data maps and privacy-datamap derived facts.
- A conformance corpus of 32 vectors with an external-runner contract, a composite GitHub Action,
  criterion benchmarks, a CI performance budget, and a release pipeline with artifact
  attestations, checksums and an SBOM.

[Unreleased]: https://github.com/noru-tech/privacy-flow/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/noru-tech/privacy-flow/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/noru-tech/privacy-flow/releases/tag/v0.1.0
