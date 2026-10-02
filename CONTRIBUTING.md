# Contributing to privacy-flow

Thanks for your interest in improving `piiflow`! SDKs for the catalogue, framework models, bug
reports with a small reproduction, conformance vectors and docs are all welcome. Noru's
organization-wide [contributing guidelines](https://github.com/noru-tech/.github/blob/main/CONTRIBUTING.md)
apply here too.

## Ground rules

- Be respectful — see [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md).
- **Deterministic.** The same tracked files and configuration produce byte-identical output on
  every platform and thread count. Sort everything; never read the clock, the environment or the
  absolute path of the scan root into the output. The determinism tests and goldens enforce this.
- **Offline, no model.** No network crate, no telemetry, no update check, no LLM in the analysis
  path. A test fails if a dependency or the source could reach the network.
- **Unknown is reported as unknown.** Never round an unresolved path to "no flow": if the analysis
  cannot see something, it is a coverage gap ([ADR 0004](docs/adr/0004-coverage-gaps.md)).
- **Every claim is cited.** A flow carries every hop with `file:line:column`.
- **Rules are data.** Vendors, SDKs and frameworks belong in `catalogue/`, not in Rust. A test fails
  if `src/facts.rs` names an SDK.
- **Never recycle a rule identifier.** A retired `PF` code stays retired.
- Keep the dependency tree pure Rust so static musl builds keep working.

## Project layout

```
src/lower/        tree-sitter lowering, one module per language, into the shared IR (src/ir.rs)
src/program.rs    assembly in path order, import resolution, provenance fixpoint
src/resolve.rs    tsconfig paths and workspace packages
src/facts.rs      edges, seeds, call sites and hits from statements and the catalogue
src/engine/       worklist engine (paths) and Datalog engine (oracle); semantics in mod.rs
src/report.rs     flows, coverage, findings, egress, IDs, digest, dispositions
src/output/       table, SARIF, Fides, in-toto, review renderers; JSON and facts in mod.rs
src/cli.rs        the command line
catalogue/        sinks, sources, sanitisers, propagators, frameworks (YAML, compiled in)
schemas/          flow-facts and configuration JSON Schemas (draft 2020-12), compiled in
vendor/           Fideslang taxonomy and classification table, verbatim (see vendor/SOURCE.md)
tests/fixtures/   one directory per case, expectations as `expect:` markers on sink lines
tests/goldens/    canonical outputs of every fixture, compared byte for byte
conformance/      the conformance corpus and its runner
benches/          criterion benchmarks and the synthetic service generator
fuzz/             cargo-fuzz targets and their seed corpus, run in CI by .clusterfuzzlite/
docs/             design, ADRs, one page per rule, configuration, catalogue, output, action
```

## Adding an SDK

1. Add an entry to `catalogue/sinks.yml` (or `sources.yml`, `sanitisers.yml`, `propagators.yml`):
   a stable `id` (`js.vendor` or `py.vendor`), the `class`, the `processor`, and `calls` patterns
   matched against API paths ([docs/catalogue.md](docs/catalogue.md)). Write instance segments as
   `Client{,()}` so that annotated and constructed clients both match. Bump nothing: the catalogue
   digest changes by itself.
2. Add a fixture under `tests/fixtures/<ts|py>/<name>/` that uses the SDK the way its documentation
   does, with `// expect: PF00x` (or `# expect: …`) on the sink line, and `fixture.json`
   (`{"complete": true}` unless the fixture is about a gap).
3. Run `cargo test --test fixtures`, then `UPDATE_GOLDENS=1 cargo test --test determinism` and
   review the golden diff.
4. Put the vendor's documentation in the pull request as the citation. `piiflow rules` shows the
   entry; `piiflow scan` on the fixture shows it working.

A project can add the same entry to its own `.privacy-flow.yml` (with a `citation`) without
waiting for a release.

## Adding a rule

Rules live in `src/rules.rs` (metadata) and `derive` in `src/report.rs` (conditions, which must
read only the document). Every rule needs: a page under `docs/rules/` (the test suite checks), a
failing and a passing fixture in both languages, a row in the README and in
`docs/control-mapping.md`, and a `CHANGELOG.md` entry. Control mappings carry identifiers and a
short gloss only; never quote a standard.

## Changing the analysis

Both engines implement the semantics written in `src/engine/mod.rs`; change both, or the fixture
suite (which compares them on every fixture) fails. Run the conformance corpus
(`cargo test --test conformance`); if a change makes a `known-limitation` vector pass, promote it
and update KNOWN-LIMITATIONS.md. If the change is a design decision, add an ADR under `docs/adr/`.

## Tests

```bash
cargo test                                         # everything below
cargo test --test fixtures                         # rule and behaviour fixtures, Datalog oracle
cargo test --test conformance                      # the conformance corpus, in process
UPDATE_GOLDENS=1 cargo test --test determinism     # regenerate goldens after an intended change
python3 conformance/run.py --verifier "target/debug/piiflow -q scan --walk -f facts"
python3 conformance/digests.py                     # after changing a vector
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

### Fuzzing

The targets in `fuzz/fuzz_targets/` cover lowering one file, the whole pipeline on an in-memory
tree (which must also give the same digest twice), `.privacy-flow.yml` and Fides data maps. They
need a nightly toolchain and [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz):

```bash
python3 fuzz/seeds.py                              # seed corpus from the fixtures and the demo
cd fuzz && cargo +nightly fuzz run scan corpus/scan -- -max_total_time=300
```

The `fuzz` workflow runs every target on each pull request and weekly
for longer. A crash input belongs in a regression test or fixture alongside the fix.

`PIIFLOW_DEBUG_IR=1 piiflow scan …` prints every statement of the program with its provenance and
call targets to stderr, which is the fastest way to see why a call was or was not resolved.

## Commit messages and pull requests

Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `ci:`). Keep pull requests to one change;
include the fixture that shows it. Pull requests run this repository's own GitHub Action.
