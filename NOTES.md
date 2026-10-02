# Notes

Working notes for `privacy-flow`: decisions, measured numbers and milestone status. Newest
milestone status is at the bottom. Decisions are never rewritten; a reversal gets a new entry.

## 2026-10-02 — Decision: where the open/closed line sits

Decided by Bip before any code was written. The suggested split is adopted unchanged.

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

`spinje/pflow` targets the same audience Noru sells to (teams building on LLM APIs), so a
`pflow` binary would collide on exactly the machines that matter.

**Decision (Bip):** the package is `privacy-flow` everywhere (crate, repository, GitHub Action),
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
| Byte-identical output across thread counts, Linux and macOS | done locally (1 and 8 threads, goldens); the cross-OS job runs once the repository is on GitHub |
| A scan with coverage gaps never exits 0 | done: exit 4 takes precedence over 1; principle test |
| `explain` shows a complete cited chain for every finding | done: a CLI test explains every finding of every fixture and checks one source line per hop |
| Release archives verify with `gh attestation verify` as the README documents | **pending**: needs a tagged release |
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
3. Let CI pass once on GitHub, including the cross-OS comparison.
4. Tag `v0.1.0`, then run the README's `gh attestation verify` commands against the release.

**Open questions.**

- The control mapping (docs/control-mapping.md) needs review by a privacy lawyer or Noru's
  compliance lead before it is cited anywhere outside this repository.
- M6: the labelled real-world benchmark with two reviewers, the Privado comparison, and the Zenodo
  DOI. The protocol is in docs/benchmark.md.
- Hashing as a "pseudonymised" category transform rather than a removal (ADR 0006) needs a
  vocabulary Fideslang does not have.
- Allocation-site abstraction for plain objects would remove the main remaining source of false
  flows seen on documenso (ADR 0005).
