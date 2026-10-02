# Notes

Working notes for `privacy-flow`: decisions, measured numbers and milestone status. Newest
milestone status is at the bottom. Decisions are never rewritten; a reversal gets a new entry.

## 2026-10-02 — Decision: where the open/closed line sits

Decided before any code was written.

**Open (this repository, MIT):** parsing, the intermediate representation, flow analysis, the
source/sink/sanitiser/propagator catalogue, the rules, and every output format (table, canonical
JSON flow facts, SARIF, Fides egress, in-toto). Anyone can run `piiflow` on their own code with
no Noru account and get every finding, every cited path and every coverage gap.

**Closed (Noru):** purpose and legal-basis inference, RoPA assembly, cross-repository topology
(joining flow facts from many repositories into one processing map), and anything that turns flow
facts into Article 30 records.

The test applied to anything new: if it answers *where does the data go, and how do we know*, it
is open. If it answers *why is that allowed, and what does the record of processing say*, it is
Noru. A flow fact is evidence; a lawful-basis judgement about it is not something this binary
makes (see principle 3 and "Out of scope" in the spec).

Consequence for the scope sections: none. The spec's scope already stops at flow facts.

## 2026-10-02 — Name check

Checked on 2026-10-02:

| Name | Where | Result |
| --- | --- | --- |
| `privacy-flow` | crates.io | free (404) |
| `privacy-flow` | `noru-tech/tap` | no formula |
| `pflow` | crates.io | **taken**: `pflow` 0.3.0, Petri-net modelling (pflow-xyz/pflow-rs) |
| `pflow` | PyPI | **taken**: `pflow` and `pflow-cli` |
| `pflow` | GitHub | **collision**: `spinje/pflow`, an actively developed CLI that installs a `pflow` command for building AI-agent automations, pushed the day before this check; also `LumaPictures/pflow` |
| `pflow` | Homebrew core and `noru-tech/tap` | no formula |
| `PrivacyFlow` | GitHub | `HKUDS/PrivacyFlow` (unrelated, agent secrets) — similar name, different package |
| `piiflow` | crates.io | free (404) |
| `privflow` | crates.io | free (404) |

`spinje/pflow` is aimed at teams building on LLM APIs, the same teams this tool is for, so a
`pflow` binary would collide on exactly the machines that matter.

**Decision:** the package is `privacy-flow` everywhere (crate, repository, GitHub Action),
and the binary is `piiflow`. (Corrected 2026-10-02: the Homebrew formula is `piiflow`, so the
install command names what it installs, `brew install noru-tech/tap/piiflow`; see the release
entry below.) This follows the `acc` convention: one
package name everywhere, a short binary name. The spec's `pflow` is read as `piiflow` throughout
this repository.

## 2026-10-02 — Repository

Local at `~/code/privacy-flow`, with a private GitHub repository `noru-tech/privacy-flow` created
from it. Nothing is published to crates.io or the tap until a release is cut deliberately.

## 2026-10-02 — M0 status: skeleton, CI, release dry run, ADRs

**Works.** Cargo package `privacy-flow` (binary `piiflow`, Rust 2024, MSRV 1.90). CI
(`.github/workflows/ci.yml`): fmt and clippy, tests on Linux and macOS, a cross-OS byte comparison
of the whole fixture tree's output, MSRV, cargo-deny, typos, an independent RFC 8785 cross-check of
every golden (Python `rfc8785`, hash-pinned), the conformance corpus, a matrix that breaks the
engine five ways and requires the corpus to fail, and a performance budget. CodeQL (Rust, Python,
Actions; fixtures ignored), OpenSSF Scorecard and Dependabot as in `acc`. Every action is pinned by
SHA, `persist-credentials: false`, top-level `contents: read`.

Release pipeline: cargo-dist 0.33.0, Homebrew installer only (no `curl | sh`), formula `piiflow`
in `noru-tech/homebrew-tap`, GitHub artifact attestations, CycloneDX SBOM, checksums, and a
crates.io job by trusted publishing. Dry run on 2026-10-02: `dist plan` lists four targets
(aarch64/x86_64 macOS, aarch64/x86_64 Linux musl), the formula, the SBOM and the source archive;
`dist build` for aarch64-apple-darwin produced a 2.5 MB archive whose `piiflow --version` runs.
`conformance-release.yml` attests `conformance/CORPUS-DIGESTS.txt` on each tag.

ADRs 0001 (shared IR) and 0002 (flow engine, with measured numbers), then 0003 to 0006 as the
analysis grew.

**First run on GitHub (2026-10-02).** It found two mistakes, both fixed: the SARIF goldens were
never committed (`.gitignore` excluded `*.sarif`), and the engine-break jobs inherited
`-D warnings`, so a deliberate break failed to compile instead of failing the corpus. CodeQL and
Scorecard cannot run on a private repository without GitHub Code Security (CodeQL's upload is
refused; Scorecard's token cannot read the repository), so both are skipped while the repository
is private. They start working when it is made public, or when Code Security is enabled.

**Not verified yet.** Release
attestations cannot be verified until a tag is pushed.

## 2026-10-02 — M1 status: TypeScript, log sinks, PF001, `explain`, SARIF

**Works.** TypeScript/JavaScript lowering (TSX included) to the shared IR; console, pino, winston,
bunyan, loglevel, the NestJS logger and injected loggers (by receiver name, marked `heuristic`);
PF001 with failing and passing fixtures; `explain` prints the cited hop chain with each hop's
source line; SARIF 2.1.0 with one code flow per finding.

## 2026-10-02 — M2 status: third-party and LLM sinks, PF002 to PF005, processors

**Works.** Sentry, Segment, Mixpanel, PostHog, Amplitude, SendGrid, Postmark, Twilio, OpenAI,
Anthropic, Gemini, Mistral, LangChain, Vercel AI, fetch, axios, browser storage, plus Stripe, S3,
Azure Storage, GCS, Algolia, Intercom, HubSpot, Slack, Inngest and Trigger.dev as third-party
processors. PF002 to PF006 with fixtures. Processor declarations in `.privacy-flow.yml`; outbound
HTTP hosts from literals, templates and constants (base URLs of client instances included).

## 2026-10-02 — M3 status: Python and framework sources

**Works.** Python lowering, including `self` fields, `__init__` as constructor, annotations as
types and `**kwargs`; requests and httpx; Python SDKs of the sinks above; FastAPI (Pydantic bodies
narrowed to classified fields), Flask, Django and DRF views; Express, Fastify and Next.js on the
TypeScript side. Unsupported frameworks (NestJS, Koa, Hono, tRPC, GraphQL servers and others) are
`unsupported_framework` gaps.

## 2026-10-02 — M4 status: interprocedural summaries, `diff`, GitHub Action

**Works.** Cross-file summaries per (function, formal, field, category class); resolution through
relative imports, nested tsconfig `paths` with `extends`, workspace packages and Python packages;
per-instance constructor summaries and per-field `this` variables (ADR 0005); a depth bound of 32
call boundaries, reported as a gap where it cuts. `diff BASE..HEAD` analyses each revision with its
own configuration and reports only new or changed findings. The composite action installs a
verified release, runs `diff` and `check`, uploads SARIF and writes a job summary.

**Measured** (Apple M3 Pro, release build; details in docs/benchmark.md):

| Input | Lines | Time | Memory | Flows | Gaps |
| --- | --- | --- | --- | --- | --- |
| documenso `8a41a3bf` (TS monorepo) | 220,640 | 1.6–2.2 s | 360 MB | 1,448 | 76 (from 1,099 before M4's resolution work) |
| Netflix Dispatch `dd2837e8` (Python) | 96,603 | 1.3–1.8 s | 250 MB | 2,549 | 112 |
| Synthetic TS service | 98,073 | 1.0–1.5 s | 360–500 MB | | |
| Synthetic TS service | 392,292 | 5.9 s | 1.5 GB | | |

Engine comparison on documenso: worklist 118 ms, Datalog about 920 ms and 480 MB more. Depth bound
on documenso: 8 left 451 depth gaps, 16 left 56, 24 left 2, 32 left 0.

## 2026-10-02 — M5 status: Fides, in-toto, dispositions, v0.1.0 readiness

**Works.** Fides egress declarations (`-f fides`) and an in-toto Statement v1 (`-f intoto`) over
the canonical JSON; dispositions with `acc`'s semantics (`open`, `accepted`, `remediated`,
`false_positive`; decision and expiry dates, a required rationale; an expired disposition stops
suppressing its finding; PFC01 cannot be disabled); `check --as-of` for reproducible enforcement; facts (JSON lines) and a review queue
(Markdown) for a human or agent to work through.

Acceptance criteria for v0.1.0, checked 2026-10-02:

| Criterion | State |
| --- | --- |
| Every rule has failing and passing fixtures in both languages, run by CI | done: 52 fixtures, 41 expected findings, all matched; Datalog oracle agrees on 52 of 52 |
| Byte-identical output across thread counts, Linux and macOS | done: 1 and 8 threads, committed goldens, and CI's byte comparison of Linux and macOS output passed on 2026-10-02 (commit `8bd779a`) |
| A scan with coverage gaps never exits 0 | done: exit 4 takes precedence over 1; principle test |
| `explain` shows a complete cited chain for every finding | done: a CLI test explains every finding of every fixture and checks one source line per hop |
| Release archives verify with `gh attestation verify` as the README documents | done: v0.1.0, see the release entry |
| KNOWN-LIMITATIONS.md lists every unsupported framework, construct and depth bound | done |
| The benchmark write-up exists | done for fixtures, conformance and performance; the labelled corpus is M6 |

Conformance: 30 of 30 required vectors pass, 2 known limitations fail as documented, flow-level
precision and recall 1.00 on the required vectors, and all 5 deliberate engine breaks are caught.
The canonical JSON matches an independent RFC 8785 implementation on 106 checks.

**v0.1.0 is not tagged.** Tagging publishes to crates.io and the tap, which is a deliberate
decision, not a build step. Before the first tag:

1. Add `HOMEBREW_TAP_TOKEN` (fine-grained, contents write on `noru-tech/homebrew-tap` only) to the
   repository's secrets.
2. Configure crates.io trusted publishing for `privacy-flow` (repository
   `noru-tech/privacy-flow`, workflow `publish-crate.yml`); the first publish of a new crate may
   need a token, as `acc`'s did.
3. Tag `v0.1.0`, then run the README's `gh attestation verify` commands against the release.

**Open questions.**

- The control mapping (docs/control-mapping.md) needs review by a privacy lawyer or Noru's
  compliance lead before it is cited anywhere outside this repository.
- M6: the labelled real-world benchmark with two reviewers, the Privado comparison, and the Zenodo
  DOI. The protocol is in docs/benchmark.md.
- Hashing as a "pseudonymised" category transform rather than a removal (ADR 0006) needs a
  vocabulary Fideslang does not have.
- Allocation-site abstraction for plain objects would remove the main remaining source of false
  flows seen on documenso (ADR 0005).

## 2026-10-02 — M6 status: benchmark set up, run recorded, labelling not started

**Done.**

- Corpus of 12 held-out applications (6 TS, 6 Python; MIT, Apache-2.0 or BSD only), pinned by
  commit, with selection criteria fixed before any run and every rejected candidate listed
  (benchmark/SELECTION.md).
- Protocol fixed before labelling (benchmark/PROTOCOL.md): two independent reviewers; seeded,
  stratified samples (8 findings per rule per application, 10 maybe-personal findings, 10 gaps,
  30 candidate sink sites); recall measured on sink sites enumerated by syntax without piiflow's
  catalogue; piiflow and Privado flows blinded and shuffled together; Wilson intervals,
  stratum-weighted estimates, Cohen's κ before adjudication.
- piiflow run recorded at commit `9a78b08`: byte-identical on repeat, all 12 exit 4.
- Privado: the engine is LGPL-3.0 and unmaintained since 2024. Its CLI sends telemetry even with
  metrics disabled, so the protocol runs the image directly, by digest, with `--network none`
  (`.github/workflows/benchmark-privado.yml`). There is a crosswalk from its 113 data elements to
  Fideslang, and a mapper checked against privado-core's exporter model.
- Scripts for sampling, scoring and statistics, with tests in CI.

**Found on the way.** The engine was quadratic in depth-bound cuts: Polar took 130 s; it now
takes 12 s with byte-identical output (recorded as a protocol deviation). The CI performance
budget's synthetic service did not catch it, because the quadratic needs summaries that reach
thousands of sinks. The budget now has a second case shaped like that
(benches/wide_summaries.py): engine phase 0.44 s fixed, 16 s with the old code, budget 4 s;
checked against both builds.

**Privado run (2026-10-02).** Completed on 11 of 12 applications. The image reports
privado-core 1.1.175, not the last release. `polar` was killed for memory twice, the second time
with 14 GiB plus 21 GiB of swap, and is recorded as a Privado failure. `redash` failed the same
way twice, then finished at the protocol's settings during the speed benchmark and again in a
repeated protocol run; it rejoined the comparison before labelling (PROTOCOL.md, Deviations).

**Sheets drawn.** 776 items per reviewer: 337 flows (211 piiflow, 126 Privado, blinded and
shuffled), 114 gaps and 325 candidate sites (Redash's sheet redrawn with 24 Privado items when
Privado completed it). Two reviewers, R1 and R2, named in the write-up
with their consent (PROTOCOL.md §7); guide in benchmark/REVIEWERS.md.

**Labelling desk (2026-10-02).** A private claude.ai page, shared with the reviewers, built by
`benchmark/labelling/build.py`: one
item at a time with code context and GitHub links, keyboard answers, each reviewer's labels in their
own private store, export to JSON, imported by `benchmark/labelling/import_labels.py` (tested in
CI). It never sees the key. Full samples kept: 776 items per reviewer.

**Next.** Both reviewers label independently; then `score.py --agreement`, adjudication into
`labels/final/`, `score.py`, the write-up, and the Zenodo deposit (needs Noru's account).

## 2026-10-02 — v0.1.0 released

The repository was made public, after the internal wording in these notes was neutralised (the
full earlier text is kept outside the repository), and `v0.1.0` was tagged at `1029e7e`.

- GitHub release with four archives (macOS and Linux, arm64 and x86_64), checksums, the
  CycloneDX SBOM and the source archive. Verified as a user would: `gh attestation verify` on
  the aarch64 macOS archive passes with the release workflow at the tag as signer, the archive's
  digest is one of the attested subjects, `shasum -c` passes, and the binary scans and validates
  the demo.
- Homebrew: `piiflow.rb` in `noru-tech/homebrew-tap`, its four checksums equal to the release's.
- The conformance corpus's digest list is attested at the tag (`conformance-release.yml`).
- crates.io: the release's publish job failed as expected (the crate did not exist, and trusted
  publishing can only be configured for an existing crate). An owner published 0.1.0 from the
  tag by hand and added the two trusted publishers (release.yml and publish-crate.yml). The
  published crate holds only the anchored include list (161 kB); `cargo install privacy-flow
  --locked` builds it and scans the demo with the same output digest as the local build.
  Trusted publishing itself is first exercised by the next release.

## 2026-10-02 — Allocation sites for plain objects (ADR 0007)

**Done.** Every literal is an allocation site; a flow-insensitive points-to pass in the facts
builder (`src/facts/heap.rs`) gives each written field of a site its own variable and turns field
reads of variables that hold only known sites into copies from those variables. The engines are
unchanged. Nested fields stay apart within a function and through module-level objects and
closures, and writes through an alias are seen. Four new required conformance vectors and two
fixtures; the fixtures fail on the previous build (four false findings, two missed flows). A new
CI break, `allocation-sites`, checks that the corpus notices the pass is gone. The `stores` break
went unnoticed once field reads inside a function stopped using `Store` tokens; the
`ts-returned-object` vector covers it again with an object that crosses a return.

**Found on the way.** The first version did not make a field read of an unknown value unknown,
so `({ options = {} })` resolved `options.providerName` against the empty default only and lost a
real flow. On two private TypeScript monorepos it appeared to remove 2,730 flows, almost all of
them through that hole. With the fix, one false flow is removed and none is lost.

**Open.** Most false flows still seen on real code go through wrapper objects returned from one
function and read in another (`return { success: true, data: result }`). Allocation sites stop at
calls, because a context-insensitive extension would let a callee's return slot hold its callers'
data (ADR 0007). Doing this properly means sites cloned per call context.

**documenso (2026-10-02).** Output identical at `8a41a3bf` (1,448 flows, 29 medium PF001); 1.9 s to
2.0 s, 355 MB to 400 MB peak. Of the 29 medium findings, roughly 12 are true, 11 go through an
object returned across a call, and 6 through a Prisma query or create whose arguments the
library model passes to the result. The next precision work with measurable effect on documenso
is those two, not more intraprocedural sites.

