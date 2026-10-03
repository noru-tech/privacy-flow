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

## 2026-10-02 — v0.1.1 released, archived on Zenodo

`v0.1.1` was tagged at `f9a30ac`. The analysis is unchanged from 0.1.0; the release carries the
speed benchmark, the benchmark report and the repository polish, and is the first release since the
repository was connected to Zenodo.

- GitHub release with the four archives, checksums, the CycloneDX SBOM and the source archive.
  `gh attestation verify` on the aarch64 macOS archive passes with the release workflow at the tag
  as signer, `shasum -c` passes, and the binary reports `piiflow 0.1.1`.
- Homebrew: `piiflow.rb` in `noru-tech/homebrew-tap` updated to 0.1.1 by the release.
- crates.io: 0.1.1 published by the release's publish job through trusted publishing, its first
  use.
- Zenodo: concept DOI 10.5281/zenodo.23104908 (resolves to the latest release), version DOI
  10.5281/zenodo.23104909 for 0.1.1. The concept DOI is in the README and CITATION.cff.

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

## 2026-10-02 — Database query results and container identity (ADR 0008)

**Found.** The 6 medium PF001 findings on documenso that went through a Prisma call were not the
query model: `js.databases` already passes nothing from a query's arguments to its result. The
Prisma client is cached in a `Map` by `remember()`, and every `new Map()` in the program was one
container for provenance (keyed by the path `Map()`), so the client was also a PDF document, a
fetch response and a zod schema, each decided against the catalogue as a propagator. 2,198 of
2,243 Prisma calls were affected. `{…,Map,…}()**` in `js.globals` also matched any call on a
Map's element.

**Done.** Containers are no longer keyed by paths ending in a call; instance-field containers and
loop variables (a provenance-only `Elements` statement) resolve to what they hold; the built-in
containers' catalogue entry covers their own methods only. Fixtures `ts/orm-query-results` and
`ts/registries`.

**Measured.** documenso: findings 757 → 549, medium PF001 29 → 23, gaps 76 → 67, 2.0 s → 1.8 s;
every removed flow goes through a database call. Two private monorepos: one 36.6 s → 9.6 s with
2 flows removed and 248 added (registry lookups now resolve); the other 13 false flows removed.

**On the way.** The first version (container keys only) appeared to remove 8,468 flows on one
monorepo; nearly all of that was lost resolution of registry lookups that had worked only
because every Map was merged. Comparing every call's resolved local targets between builds found
it (223 calls); after the instance-field, loop-variable and held-variable fixes, 2 remain (nested
containers, now in KNOWN-LIMITATIONS.md).

**Next.** Of documenso's 23 medium PF001 findings, roughly 11 go through an object returned from
one function and read in another, which needs allocation sites per call context (ADR 0007).

## 2026-10-03 — Allocation sites across calls (ADR 0009)

**Done.** Calls that reach only local functions hold per-call-site clones of the sites their
callee returns; the callee's site fields (and a literal's rest, what spreads into it) are extra
return slots, applied through summaries, so both engines generalise "return value exits to the
call result" to a slot/exit table. Spreads are rests; `.catch`/`.finally` pass a promise's sites
through. Four required vectors, fixture `ts/returned-objects`, CI break `call-context`.

**Measured.** documenso: findings 549 → 523, medium PF001 23 → 14, high PF002 29 → 27, same time
and memory. Monorepo A: 1,429 flows removed, 1 added, 9.6 s → 12.0 s, 3.3 → 3.7 GB. Monorepo B: 35
removed, 1 added (a real flow).

**On the way.** Each step was checked against the private monorepos, and each found something:
literals with an unknown spread were entirely unknown (the documenso shape), so spreads became
rests; a `.catch(() => null)` on the call made the result unknown; and 127 unrealisable flows came
from a closure reading a clone's field, which made it shared, so a summary handed it to every
caller. A function's own slots now never stop its summary, and closures do not share.

**Next.** The parameter side: objects passed into a call still collapse one level
(`f({ user })` read as `input.user.id`); the long false chains sampled on monorepo A go through
it.

## 2026-10-03 — Formal sites (ADR 0010)

**Done.** Parameters that every call passes exactly one argument to hold formal sites (up to three
fields deep) whose field variables are further formal parameters, bound from the arguments' site
fields at each call site; the points-to pass maps sites into calls as it maps them out of calls.
The engines are unchanged. Three required vectors, fixture `ts/param-objects`, CI break
`param-sites`.

**Measured.** documenso 523 → 519 findings; monorepo A 40 flows removed, 6 added; monorepo B 509
removed (a recipient's name and device ID reported as reaching logs of `recipient.email`), 3 added.
Time +20–40%: each formal-site field is summarised as a parameter.

**Next.** Summarise formal-site parameters only for the tokens their function reads.

## 2026-10-03 — Email, messaging and push sinks

**Why.** Scoring the imported partial labels (R1, ten applications; provenance unconfirmed, so
for prioritising only) put piiflow's recall at 33 of 114 sampled personal sinks. Email and
messaging calls were the largest group of misses: libraries the catalogue did not list
(`smtplib`, Flask-Mail, `emails`, Apprise, `@nestjs-modules/mailer`, Wasp's `emailSender`, Expo),
and Django messages whose data is in the message object, not in `send()`'s arguments.

**Done.** Sinks take `receiver: true` (the receiver is sent too); Django, Flask-Mail, `emails`
and Apprise use it. Thirty-odd email, SMS, chat and push SDKs in both languages, each in fixture
`py/messaging` or `ts/messaging`. Propagators cover only the message constructors and builder
methods, so other calls into these SDKs stay gaps.

**Measured** (benchmark corpus, base `36c2ea4`). Messaging flows: Healthchecks 1 → 87 (every
outgoing email, previously silent), Ghost 1 → 6, Redash 0 → 7, Hoppscotch 0 → 6, CTFd 0 → 4,
fastapi-template 0 → 3, Polar 0 → 3, open-saas 0 → 1; gaps down by 1 or 2 in each. Against the R1
site labels: found 33 → 36 of 114; one site lost (fastapi-template logs the result of
`message.send`, which was a propagating gap and is now a sink whose result carries nothing).

**Next.** Most remaining messaging misses are wrappers reached through dynamic dispatch
(`self.transport.notify`, `this.mailer.send`, inherited `send`): inheritance and methods on typed
fields, not catalogue.

## 2026-10-03 — Inheritance (ADR 0011)

**Done.** Classes record their bases; lookup walks the lineage (methods, constructors, external
bases); `super` is the bases; fields flow from a class to its subclasses; a call on a typed value
dispatches to every override, a call on `this` does not and is a gap where it is overridden;
properties and getters give what they return. Ten required vectors and one known limitation,
fixtures `ts/inheritance` and `py/inheritance`, CI break `inheritance`.

**Measured** (against `36c2ea4`). `dynamic_call` gaps: Polar 227 → 64, PrivateGPT 73 → 30,
Healthchecks 24 → 6, CTFd 17 → 3. Flows: Redash 242 → 307, PrivateGPT 761 → 2,462 (mostly a
misclassified `context.state` reaching every chat interceptor), Polar 10,472 → 11,518. Time
unchanged. Dispatching on `this` as well, tried first, took Redash to 4,741 flows of one query
runner's credentials reaching the others' logs.

**Next.** Analyse inherited methods per subclass, so template methods resolve; TypeScript
interfaces to their implementing classes; `state` in the classification table.

## 2026-10-03 — TypeScript interfaces (ADR 0012)

**Done.** Interfaces are classes with no members, bound and exported by name; `implements` and
interface `extends` make subtypes for dispatch, never for lookup. A call through an interface
does not explain its external targets (open world); a method no implementation defines is a
plain value's method. Vector `ts-interface-dispatch`, fixtures `ts/interfaces` and
`ts/interface-open-world`.

**Measured** (against the inheritance build). The six TypeScript applications: flows unchanged;
Ghost gaps 609 → 612, three calls on an external scheduler adapter that a local wrapper had
hidden. The corpus injects by class or untyped JavaScript, so it has little of this pattern.

**Next.** Analyse inherited methods per subclass, so template methods resolve.

## 2026-10-03 — Template methods per subclass (ADR 0013)

**Done.** After the provenance fixpoint, template methods (calls on `this` to an overridden
method, or to another template) are copied into each subclass that inherits them, with the
subclass's `this` and fields, and the fixpoint runs again. Copies share their originals' source
and sink IDs; duplicate flows are dropped. Budget 200,000 copied statements. Vectors
`py-template-method`, `ts-template-isolation`, `ts-virtual-dispatch` (now required); CI break
`per-subclass`.

**Measured** (against the interfaces build). No distinct flow lost on any application.
"Overridden" gaps: Ghost, Redash and Polar 3 → 0 each. Added flows: Redash 3, PrivateGPT 115
(base readers calling an overridden loader). Duplicate flows removed: Polar 4,397 → 320,
Healthchecks 43 → 0, Umami 13 → 0, Ghost 20 → 7; these came from two API paths of one call
matching the same sink, a bug since v0.1. Time unchanged.

**Next.** The remaining duplicates are distinct sources at one place (fields of one typed
parameter); `state` in the classification table.

## 2026-10-03 — `state` is an address's only in context

**Why.** The vendored classification table maps `state` (and `province`) exactly to
`user.contact.address.state`, though the table's own rule is that an exact name means the same
thing in every schema. On PrivateGPT a chat engine's `context.state` made 1,295 of 2,462 flows;
on Redash `alert.state`; on the Vercel chatbot ProseMirror's editor state reached five gaps.

**Done.** `catalogue/classification.yml` keeps table names to a context: `state` and `province`
are classified only on an object named like an address, on a type or class with other address
fields, or beside address parameters. The table stays verbatim (it is privacy-datamap's); the
same change should be proposed upstream, after which the entry can go. Fixture
`py/contextual-names`.

**Measured** (against `main` at #14). Address-state flows: PrivateGPT 1,295 → 0 (flows 2,462 →
1,167, findings 2,682 → 1,371), Redash 2 → 0, Polar 621 → 507 (kept: `Address`, its tax
breakdown type, `billing_address.state`; dropped: `order.tax_breakdown[0]["state"]`, an ASGI
`scope["state"]`). No other category lost anywhere except Polar's OAuth callbacks, whose `state`
query parameter is now reported as request input instead of being narrowed into an address.

**Next.** Propose the table change in noru-grc-engineering.

## 2026-10-03 — M6: labelled results published

**Done.** Final labels from R1 and R2 for the ten applications both labelled in full
(`benchmark/labelling/finalise.py`): agreed items final, the 47 disagreements `unsure`, left out
and counted; Polar and PrivateGPT unscored. `score.py` counts unsure sites and skips unlabelled
applications. Deviation recorded; PROVENANCE.md records the owner's decision to publish.

**Results** (piiflow 0.1.0, ten applications, head to head with Privado): flows reported that
are real 72 % (65–79 %) against 22 % (2–41 %); sampled personal-data sinks found 31 % (23–40 %)
against 14 % (9–22 %), 38 % counting coverage gaps; gaps that hide a real flow 29 %. κ before
finalising 0.875 (flows), 0.875 (gaps), 0.897 (sites).

**Next.** Zenodo deposit of the labelled corpus (needs Noru's account); a recorded run of the
next release against the same site labels for recall, with a fresh precision sample.

## 2026-10-03 — Exceptions (ADR 0014) and registries

**Why.** Re-scoring the R1 site labels against main + #17 (36 of 114 found): the largest groups
of misses were wrapper calls (about 18) and caught errors logged or sent to Sentry (about 16).
Most wrapper misses were real; CTFd's came from a provider class picked from a dictionary with
`.get(kind)`, which lost what the entries were. The owner chose to report library-error echoes,
for review.

**Done.** Dictionary `.get`/`.pop` reads named entries. Functions have a throw slot (a return
slot whose exits are catch parameters or the caller's throw slot); `throw`/`raise` go to the
innermost catch; library calls in a `try` echo their arguments to the catch (hop `error`,
`info` and needs-review); callbacks handed to libraries throw through the call. Vectors
`ts-throw-catch`, `py-raise-across-calls`; fixtures `ts/exceptions`, `py/exceptions`.

**Measured.** Registries: CTFd 41 → 73 flows. Exceptions: flows through throws Polar 187,
PrivateGPT 79, Ghost 11, Redash 5; through echoes Polar 510, PrivateGPT 251, Hoppscotch 139,
Umami 78 (mostly `unknown`). R1 sites found 36 → 39. The remaining error-logging misses carry
identifiers or unmodelled request input, which no source names.

**Next.** Identifiers (user and customer IDs) as sources, which the remaining misses in both
groups need; then a new recorded benchmark run.

## 2026-10-03 — Identifiers as sources (ADR 0015)

**Done.** `catalogue/classification.yml` entries can add names with a category: person
identifiers everywhere, `id`/`uid`/`uuid` on person-like objects. PF001 does not fire on an
identifier alone (the owner's choice: logging an ID instead of contact data is good practice);
PF002 to PF006 do. Narrowing restricted to request input, as documented. Conformance vectors use
`plan` instead of `id` as the non-personal field, so the corpus is neutral on identifiers; both
this build and the previous pass all of them. Fixture `py/identifiers`.

**Measured** (against ADR 0014's build). R1 sampled sinks found 39 → 45 of 114 (Open SaaS 2 → 8).
Polar: PF002 high +252, PF006 +470; no new medium log findings. Time unchanged.

**Next.** The rest of the identifier names (`sub`, `owner` IDs) by config; the recorded run of
the next release.
