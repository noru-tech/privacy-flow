# Configuration: `.privacy-flow.yml`

`piiflow` reads `.privacy-flow.yml` at the root of the scanned directory (or the file given with
`--config`). It is optional: without it the built-in catalogue and default policy apply. It is
validated against [`schemas/config.schema.json`](../schemas/config.schema.json) before anything
else happens; an invalid file stops the run with exit 3 and every schema error listed.

Every entry that adds knowledge (a source, sink, sanitiser, propagator, field, processor or
non-personal name) carries a `citation`: where the claim comes from (a DPA, a vendor record, a
ticket, a code review). The output document records the configuration's path and SHA-256, so a
finding's absence can always be traced back to the configuration that caused it.

```yaml
version: 1

system:
  fides_key: checkout_service
  name: Checkout service

exclude:
  - "scripts/**"
default_excludes: true

max_call_depth: 32

processors:
  - name: PostHog
    citation: "DPA signed 2026-03-01, vendor record VR-0042"
  - name: Acme CRM
    hosts: [crm.acme.example]
    citation: "Contract C-2026-19"

fields:
  - name: member_ref
    category: user.unique_id
    citation: "Data dictionary v3, entity Member"

not_personal:
  - name: payload
    citation: "Job payloads hold IDs only (reviewed in SEC-101)"

sanitisers:
  - id: acme.redact
    language: javascript
    functions: ["src/lib/redact.ts:redact"]
    removes: ["*"]
    citation: "Masks all but the domain; reviewed in SEC-142"

sinks:
  - id: acme.audit
    language: javascript
    class: third_party
    processor: Acme Audit
    calls: ["@acme/audit-sdk:**.record"]
    citation: "Audit vendor SDK reference, PR #12"

policy:
  fail_on: medium
  rules:
    PF001: { severity: high }
    PF006: { enabled: false }
```

## `version`

Always `1`.

## `system`

The system this repository is, for the [Fides output](fides.md): `fides_key` and an optional
`name`. Without it the Fides fragment uses `this_system` and says so.

## Files: `exclude` and `default_excludes`

`exclude` is a list of globs relative to the scan root (`*` within a directory, `**/` any number
of directories, `{a,b}` alternation). With `default_excludes: true` (the default) tests,
fixtures, mocks, seed data, vendored, generated and built code are left out as well:
`node_modules`, `vendor`, `third_party`, `dist`, `build`, `out`, `.next`, `coverage`, virtual
environments, `migrations`, `test`, `tests`, `__tests__`, `spec`, `*.test.*`, `*.spec.*`,
`test_*.py`, `*_test.py`, `conftest.py`, `fixtures`, `__mocks__`, `mocks`, `seed`, `seeds`,
`*.seed.*`, `e2e`, `cypress`, `playwright`, `*.stories.*`, `*.d.ts`, `*.min.js` (the full list is
`DEFAULT_EXCLUDES` in [`src/files.rs`](../src/files.rs)).

Inside a Git work tree the tracked files are enumerated with `git ls-files`; outside one, by a
directory walk (`scan --walk` forces the walk). The output records which (`subject.enumerated_by`).

## `datamap`

Field classifications from an existing data map flow into the analysis. By default `piiflow`
reads `.fides/datamap.yml` (a Fides data map, which privacy-datamap renders) and
`.noru/.cache/privacy-datamap.derived.json` (privacy-datamap's derived facts) when they exist. A
list of paths replaces the defaults; `false` turns ingestion off. A field's categories apply to
reads of that field name anywhere, and to objects typed with a type whose name matches the
collection (`User` ↔ `users`).

## `max_call_depth`

How many call boundaries (each entry into a function and each return out of one) a witness path
may cross. Default 32. Where the bound cuts a witness to something not reached any other way, a
`depth_bound` coverage gap is reported at that call site; the bound never silently turns into "no
flow".

## `processors`

The processors your organisation has a contract or vendor record for. A flow to a declared
processor is an egress fact (`declared: true`), not a [PF002](rules/PF002.md) finding. A
declaration matches a flow's sink by:

- `name`: the catalogue sink's `processor`, case-insensitive (`PostHog`, `OpenAI`, `Sentry`);
- `sinks`: catalogue sink ID globs (`js.posthog`, `py.*`);
- `hosts`: for outbound HTTP with a literal host, the host or a parent domain
  (`crm.acme.example` matches `api.crm.acme.example`).

`fides_key` overrides the key used for the processor in the Fides output.

## `fields`

Field names that are personal data of a Fideslang category, in addition to the vendored
[classification table](../vendor/classification/classification.json). `category` must be a key
of the vendored taxonomy or `unknown`. Names are matched as the table matches them: lowercased,
non-alphanumerics collapsed to `_`, and also with camelCase split (`memberRef` → `member_ref`).

## `not_personal`

Field names that are not personal data in this codebase, overriding the table. Typically names the
table marks *maybe* personal (`payload`, `title`, `name`), whose findings are otherwise reported at
`info` for review.

## `sources`, `sinks`, `sanitisers`, `propagators`

Catalogue entries with the same shape as the built-in [`catalogue/`](../catalogue/) files, plus a
required `citation`; [the catalogue reference](catalogue.md) explains every field. IDs must not
clash with built-in ones. Config entries are listed as `[config]` by `piiflow rules --config
.privacy-flow.yml`.

A sanitiser's `functions` names local functions as `path/to/file:functionName` (globs allowed in
both halves); a call to one replaces its arguments' categories with nothing for the categories it
`removes` (category prefixes, or `*` for everything including `unknown`). A team's own `redact()`
helper is the usual case.

## `policy`

- `fail_on`: the lowest severity that fails `piiflow check` (`info`, `warning`, `medium`,
  `high`; default `medium`).
- `rules`: per rule, `enabled` and `severity`. `PFC01` cannot be disabled: a coverage gap can never
  be turned into a clean result. Record a disposition on the gap instead.

The effective policy is embedded in the output document, so `check` and `validate` use the policy
the scan was made with.
