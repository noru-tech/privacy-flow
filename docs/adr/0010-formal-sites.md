# ADR 0010: Formal sites: objects passed into a call keep their fields apart

Status: accepted, 2026-10-03. The parameter side of ADR 0009.

## Context

After ADR 0009, objects returned from calls kept their fields apart in the caller, but objects
passed into calls did not: `show({ owner: { id, email } })` read as `input.owner.id` inside
`show` reported the email address, because the binding carried the argument's one-level field
tokens and reading `input.owner` collapsed them. On the private monorepos this was the shape of
most of the remaining long false chains, such as a recipient's name reported as reaching a log of
`recipient.email` through `sendToRecipient({ recipient, ... })`.

## Decision

- **Formal sites.** A parameter every binding of which passes one argument to it (positionally or
  by keyword, never a rest parameter, `...xs` or `**kw`) holds a *formal site*, keyed by
  (function, parameter, path of fields below it), up to three fields deep. A formal site stands for
  whatever any call passes there; its fields mirror the arguments' sites' fields.
- **Formal sites' fields are formal parameters.** Each field variable of a formal site (and its
  rest) is appended to the function's formal parameters, and each call site binds it from the
  corresponding field variable of the argument's site. Summaries are computed per formal as
  before, so context sensitivity comes from the existing machinery: a function called with an
  object holding an email address and with one holding an ID keeps the calls apart. The engines
  are unchanged.
- **One mapping for both directions.** The points-to pass maps sites across a call either out of
  it (returns, to call-site clones, with exits) or into it (arguments, to formal sites, with
  bindings); fields, rests and nesting are mirrored the same way, and a rest a site gets later is
  read by every read already made of the site.

A parameter with any inexact binding, with no binding, or classified as personal data by its
name keeps its previous behaviour: it is unknown, and its field reads keep their `Load` edges.

## Measurements

Against ADR 0009's build:

| | before | after |
| --- | --- | --- |
| documenso `8a41a3bf`: findings | 523 | 519 |
| time, peak memory | 2.1 s, 410 MB | 2.6 s, 430 MB |
| private monorepo A: flows | 37,616 | 37,582 (40 removed, 6 added) |
| time, peak memory | 12.8 s, 3.5 GB | 16.3 s, 2.9 GB |
| private monorepo B: flows | 5,977 | 5,471 (509 removed, 3 added) |
| time, peak memory | 3.4 s, 810 MB | 4.6 s, 960 MB |

On monorepo B, the removed flows sampled are categories that do not reach the sink: a recipient's
name or device ID reported as reaching a log of `recipient.email`. The flows of the email address
itself to the same sinks are all still reported. The added flows are short and plausible (a
document ID sent to the project's own API) or long chains through generic data structures.

## Consequences

- Conformance vectors `ts-param-nested`, `ts-param-context` and `py-param-dict`, and fixture
  `ts/param-objects`. `ts-param-context` passes on the previous build too; it guards against a
  context-insensitive implementation. CI breaks the pass (`param-sites`).
- **Cost:** every formal site field is a further formal parameter with its own summaries, which
  is most of the added time. Summarising formal-site parameters only for the tokens their
  function reads is the obvious next optimisation.
- **Not covered:** rest parameters and spread arguments; parameters classified by name; objects
  more than three fields below a parameter.
