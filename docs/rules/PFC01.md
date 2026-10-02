# PFC01: Coverage gap

**Where the analysis cannot see where data goes, it says so: every coverage gap is a PFC01 finding, and an open one means a clean result cannot be claimed.**

| | |
| --- | --- |
| Rule ID | `PFC01` |
| Default severity | `warning` (warning (SARIF)) |
| Fails the default policy (`fail_on: medium`) | never as a pass: an open gap makes `check` exit 4 |
| Fixtures | [`tests/fixtures/ts/pfc01-fail`](../../tests/fixtures/ts/pfc01-fail) (fails), [`tests/fixtures/ts/pfc01-pass`](../../tests/fixtures/ts/pfc01-pass) (passes), and the Python equivalents |

## Why it matters

An empty result is only meaningful if the analysis saw everything. A call into an SDK the
catalogue does not know, a callback whose target is computed at run time, a framework whose
request input is not modelled: each is a place where personal data might leave, unseen. PFC01
turns each into a finding with a location, so that it is reviewed rather than rounded to "no
flow". This is principle 4 of the project, enforced: `scan` exits 4 when coverage is incomplete,
and `check` exits 4 while any PFC01 finding has no disposition, whatever the threshold. PFC01
cannot be disabled in the policy.

## When it fires

Gap kinds (the gap's `kind`):

| Kind | Raised when |
| --- | --- |
| `unresolved_callee` | personal data is passed to a call into an external module (or global) the catalogue does not know, and nothing else explains the call |
| `unresolved_method` | personal data is passed to a method of an object whose origin the analysis cannot see |
| `dynamic_call` | personal data is passed to a computed callee (`handlers[name](x)`, a callback parameter) |
| `unresolved_import` | personal data is passed to something imported from a relative path that was not found |
| `depth_bound` | the call-depth bound (`max_call_depth`, default 32) cut a witness to something not reached any other way |
| `unsupported_framework` | the code imports a web framework whose request input is not modelled (Koa, NestJS, Hono, tRPC, aiohttp, Slack Bolt …; see `catalogue/frameworks.yml`) |
| `unsupported_language` | the repository has source files in a language the analysis does not parse (Go, Java, Ruby …) |
| `unsupported_construct` | a construct makes code remotely callable in a way no supported model covers (Next.js `'use server'`) |
| `parse_error` | a file did not parse completely, or was too large to analyse |

The first five are raised only when personal data actually reaches the gap; the last four exist
whatever the data. Gaps are aggregated: one gap per API path (repeated chains collapsed) or per
language or framework, listing every location.

## Controls

PFC01 is about the analysis, not about a control: it maps to none. It supports every other mapping by making their absence of findings meaningful.

These mappings are identifiers with Noru's own short gloss; no normative text from any standard is
quoted. They are a guide for control owners, not a compliance claim, and are pending review by a
privacy lawyer or Noru's compliance lead before the first release
([control mapping](../control-mapping.md)).

## Failing example

Fixture: [`tests/fixtures/ts/pfc01-fail/`](../../tests/fixtures/ts/pfc01-fail/). An email address is passed to `deliver` from an SDK the catalogue does not know.

```console
$ piiflow -q scan tests/fixtures/ts/pfc01-fail -f json | jq -r '.findings[] | "\(.rule_id) \(.severity) | \(.message)"'
PFC01 warning | coverage gap (unresolved_callee): acme-mailer:deliver
```

`piiflow explain <finding ID>` prints the full chain of hops from the source to the sink, with the
source line of each.

## Passing example

Fixture: [`tests/fixtures/ts/pfc01-pass/`](../../tests/fixtures/ts/pfc01-pass/). The project's `.privacy-flow.yml` declares `acme-mailer:deliver` as a messaging sink with a citation, and the vendor as a processor: the call is now a known sink, and the flow is an egress fact.

## How to fix

- Teach the catalogue: add the SDK as a sink, propagator or sanitiser in `.privacy-flow.yml` (with a
  citation), or contribute it to `catalogue/` (see [CONTRIBUTING](../../CONTRIBUTING.md#adding-an-sdk)).
- Raise `max_call_depth` for a `depth_bound` gap.
- If the gap is accepted (the unsupported framework is internal tooling, the dynamic call only
  reaches internal code), record a disposition with the reason. Its ID is the gap's, so it covers
  every location of that gap.

## Recording a disposition

A finding is never deleted. To record a human decision about it, edit only the finding's
`disposition` in `.privacy-flow/flows.json` (`accepted`, `remediated` or `false_positive`, with
`owner`, `decided_at`, `rationale`, and `remediated_at` or an optional `expires_at`), then run
`piiflow check .privacy-flow/flows.json --as-of YYYY-MM-DD`. The date is required because the
machine clock is never read. A disposition suppresses the finding from the threshold decision on
that date and leaves the finding, its flow and the summary intact; the next `scan` carries it over
by finding ID, which does not depend on line numbers. Exact rules:
[dispositions](../output.md#dispositions).

```json
{
  "status": "accepted",
  "owner": "privacy@example.com",
  "decided_at": "2026-10-01",
  "expires_at": "2026-12-31",
  "rationale": "Koa is used only by the internal admin tool, which receives no personal data (reviewed in SEC-98).",
  "remediated_at": null
}
```

To change how the rule is enforced rather than record a one-off decision, set its severity or
disable it under `policy.rules.PFC01` in [`.privacy-flow.yml`](../configuration.md#policy).

---

[All rules](README.md) · [Exit codes](../exit-codes.md) · [Control mapping](../control-mapping.md)
