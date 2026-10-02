# Roadmap

The goal is one trustworthy answer to one question: where does personal data go in this code, and
how do we know? Everything below serves that question. Items are ordered by intent, not by date;
see the [issues](https://github.com/noru-tech/privacy-flow/issues) for status. Milestone status
reports are in [NOTES.md](NOTES.md).

## v0.1 (milestones M0 to M5)

TypeScript, JavaScript and Python; logs, error tracking, analytics, messaging, LLM providers,
outbound HTTP, browser storage and other processors as sinks; Express, Fastify, Next.js, FastAPI,
Flask and Django request input; summaries across files, workspace packages and tsconfig aliases;
rules PF001 to PF006 and PFC01; `scan`, `diff`, `explain`, `check`, `validate`, `rules`, `doctor`;
JSON, table, SARIF, Fides, in-toto, facts and review outputs; dispositions; the GitHub Action;
the conformance corpus; release pipeline with attestations.

## M6: real-world benchmark

Permissively licensed applications referenced by commit, flows labelled by two reviewers,
per-rule precision and recall, a fair comparison with Privado's open-source scanner, and a Zenodo
DOI for the corpus. See the protocol in [docs/benchmark.md](docs/benchmark.md#protocol-for-m6).

## After v0.1

- **Go**, then **Java and Kotlin**: one lowering each into the shared IR
  ([ADR 0001](docs/adr/0001-shared-ir.md)), with their logging, HTTP and SDK sinks in the catalogue
  and the common frameworks (net/http, gin, echo; Spring) as sources.
- **More frameworks:** NestJS (decorated controller parameters), Koa, Hono, tRPC procedures,
  GraphQL resolvers, Remix and SvelteKit loaders and actions, Next.js server actions, Starlette and
  aiohttp handlers; Vue and Svelte single-file component scripts.
- **Precision:** flow-sensitivity within a function (ordered statements in the IR),
  summarising formal-site parameters only for the tokens their function reads (the cost of
  [ADR 0010](docs/adr/0010-formal-sites.md)), inherited
  methods, exceptions to `catch`, event emitters.
- **Pseudonymisation as a category transform** rather than a removal, so a hashed identifier is
  reported as pseudonymised data, with a vocabulary agreed with privacy-datamap
  ([ADR 0006](docs/adr/0006-sanitisers-and-hashing.md)).
- **Catalogue growth:** more SDKs, contributed as YAML with a fixture each; a catalogue version
  policy for additions and changes.
- **Integrations (outside this repository):** privacy-datamap reads flow facts to fill processor
  and egress fields; ai-inventory reads PF003 facts. The flow-facts schema stays stable for them.

## Not planned

Runtime analysis, scanning data at rest, security vulnerability rules, any model call or network
access from the binary, and purpose or legal-basis inference ([NOTES.md](NOTES.md) records the
open/closed line).
