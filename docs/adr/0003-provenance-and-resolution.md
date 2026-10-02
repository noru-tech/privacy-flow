# ADR 0003: Resolve calls by provenance

Status: accepted, 2026-10-02.

## Context

Whether a call is a sink depends on what is being called: `client.capture(...)` is a PostHog sink
only if `client` is a PostHog client. Names alone are not enough (`capture` is a common method
name), and types are not available without a type checker. The analysis also has to follow calls
into the project's own functions, across files and packages.

## Decision

A flow-insensitive **provenance** fixpoint ([`src/program.rs`](../../src/program.rs)) assigns
each variable a small set of values: an **API path** into an external module or a global
(`posthog-node:PostHog().capture`), a local function, class or instance, a module or a package.
It is computed from imports, member access, calls (`()`), construction, type annotations,
callbacks handed to APIs, arguments bound to local parameters, returns (with their objects'
fields), field writes, and containers. The catalogue matches API paths with globs; local
functions are followed through summaries.

Imports resolve to scanned files first: relative paths; the nearest `tsconfig.json` (with
`extends`, relative or through a workspace package) and its `paths` and `baseUrl`; workspace
packages by their `package.json` name, entry and `exports`; Python packages by relative import
or by the shortest scanned path that ends with the module path. Anything else is external.

Bounds keep it finite: API paths stop growing at 16 segments, and at most 16 API paths are kept
per variable (the smallest), while functions, classes and modules are not capped (a component's
render callbacks from every call site all matter; the safety bound is 4,096).

## Why

- A provenance set is cheap (one pass per round over the statements, 18 rounds on a 220k-line
  monorepo) and explains itself: a gap's detail is the API path the analysis could not place.
- Type annotations are already in the code and say what an injected value is (`logger: Logger`,
  `client: WebClient`, `tx: Prisma.TransactionClient`); reading them as provenance resolves the
  dependency-injection patterns that name-based matching misses, without a type checker.
- Callback provenance (`<cbN>`) resolves code handed to libraries — Prisma transactions, Sentry
  scopes, render props — through the same catalogue patterns as everything else.

## Measurements that shaped it

On documenso (220k lines of TypeScript), coverage gaps went from 1,099 to 76 as these were added:
workspace and nested tsconfig resolution, type-annotation provenance, callback provenance, JSX
components as calls, argument-to-parameter binding, returned object fields, path-keyed
containers, and not reporting a gap when some target of a call is known.

## Alternatives

- **A type checker (TypeScript's, Pyright).** Precise, but a Node or Python runtime inside a static
  binary, and slow on large repositories.
- **Name heuristics only.** Used as a fallback for loggers by receiver name (marked `heuristic`),
  but too noisy as the main mechanism.

## Consequences

- Provenance is flow-insensitive and context-insensitive: a variable holding a PostHog client in
  one branch and a logger in another is both, and a call on it is checked against both.
- Values whose origin is invisible (a parameter with no annotation and no resolvable caller, an
  object from a dynamic import) have no provenance; personal data passed to their methods is a
  coverage gap, never silently dropped.
