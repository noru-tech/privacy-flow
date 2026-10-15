# ADR 0020: A finding is an exposure, and its flows are the evidence

Status: accepted, 2026-10-15.

## Context

Every rule fired once per flow, and a flow is one source, one sink and one category. A tester
running 0.4.0 on dotnet/eShop got 214 findings, 134 of them PF001, and read most of them as
duplicates:

- **One log call, one finding per field.** `services.Logger.LogWarning(…, request)` in
  `OrdersApi.cs` receives the whole `CreateOrderRequest`: seven findings, one each for the street,
  city, state, postal code, country, user name and card number. All seven are fixed by changing
  that one line.
- **The same fields at every log call they pass.** The request goes on through the identified
  command handler, MediatR's logging, validation and transaction behaviours and the command
  handler, each with its own log call, so the seven findings repeat at each.

The first is noise: one decision, seven findings, seven dispositions to record. The second is
not: each log call is a separate place where the data is written, and a separate fix.

The unit a person acts on is the place the data leaves, under one rule. The flows that reach it
are the reason for the finding, and they must stay: `explain`, SARIF code flows and the
review queue cite them.

## Decision

**One finding per exposure.** An exposure is a rule at a sink, and the key is

    (rule, sink ID, needs_review)

- **Every flow the rule fires on at that sink is evidence.** The finding lists their IDs in
  `flows`, in document order (sink, then source location), and carries all their categories.
  Its severity is the highest of its flows'; its location is the sink's; its processor is the
  sink's, which is the same for every flow there.
- **Flows that need review are a finding of their own.** A flow needs review when its source is
  a name that only may be personal, its category is `unknown`, its sink is matched by receiver
  name, or it passes through a library error. Those flows form a second finding at the same
  sink, so the review queue still lists them, and a decision about a card number is never
  mixed with one about a field called `Description`.
- **The ID** is the hash of the rule, the sink ID, `needs_review` and the sorted categories:
  `pf-` and 16 hex digits. Sink IDs carry no line numbers (ADR 0003), so neither does the
  finding's. It is computed after all flows are known, from sets, so it does not depend on the
  order the engine found them in or on the number of threads. A new flow with a category the
  finding already has keeps the ID; a new category changes it.
- **The message** names the categories, the first source in source order, and how many others
  there are: `user.account.username, …, user.financial.credit_card from
  `request.UserName (request: CreateOrderRequest)` (src/Ordering.API/Apis/OrdersApi.cs:120) and
  6 other sources reaches a log sink …`. A finding with one flow reads as before.
- **Coverage gaps are unchanged:** one PFC01 finding per gap, with the same ID as before.

**Dispositions follow the categories they were made about.** `scan` carries a disposition by
finding ID, as before. When the ID is new, it carries the disposition of the earlier findings of
the same rule that share a flow with the new one, when they all record the same disposition and
together had every category it has. An exposure that lost a category keeps its decision; one that
gained a category is open again, because the decision was not made about that category. The same
rule reads a `0.1` document, whose findings were one per flow: an exposure is decided when every
flow of it was, with the same disposition. Without that, upgrading would have dropped every
recorded decision.

**The document's schema version is 0.2.** `findings[].flow` (one ID or null) became
`findings[].flows` (a list, empty for a gap): a breaking change, so `schema_version` is bumped.
`validate` rejects a 0.1 document; `scan` and `diff` still read one for its dispositions.

**Outputs.** SARIF has one result per finding, with a code flow per flow (its message names the
flow ID and category), the sources as `relatedLocations` and the flow IDs in
`properties.flows`. `explain` prints every flow of the finding. The review queue lists a finding
under each name its sources have. `diff` counts a finding as changed when its exposure gained a
flow, or when its ID is new but the base had a finding of the same rule at the same sink; that
finding is not also counted as removed.

## Alternatives

Measured on dotnet/eShop at the commit `tests/smoke/eshop.expected.json` pins and on the
benchmark corpus (twelve applications at their pinned commits). Coverage-gap findings are the
same under every key: 27 on eShop, 2,460 on the corpus.

| Key | eShop findings | corpus findings |
| --- | --- | --- |
| One per flow (0.4.0) | 214 | 13,705 |
| (rule, sink, source location, review) | 102 | 11,777 |
| (rule, sink, source function, review) | 95 | 9,082 |
| (rule, sink, source function) | 90 | 8,258 |
| **(rule, sink, review): chosen** | **67** | **3,617** |
| (rule, sink) | 62 | 3,415 |

- **(rule, sink, source location or object).** Groups the fields of one typed object, which
  answers the address case, and nothing else: the corpus barely moves (11,777), because in
  TypeScript and Python each field read is its own source. A shared logging helper or MediatR
  behaviour still gives one finding per caller at the same line. Its ID would also need a source
  anchor without a line number, and typed objects have none in the document today.
- **(rule, sink, source function).** Fewer findings, same problem: Polar's log calls reached from
  hundreds of functions stay hundreds of findings (4,726 to 5,505 on Polar alone, against
  1,394).
- **(rule, sink) without the review split.** Five fewer findings on eShop, but a finding that
  mixes certain and maybe-personal data has no single severity or `needs_review`, and its
  maybe-personal names drop out of the review queue.
- **(rule, source object), across sinks.** One finding for the order request wherever it is
  logged. It hides that each log call is its own place to fix, and its location would be a
  source, not the line to change.
- **Keep one finding per flow and group in the outputs only.** Every consumer (`check`, the
  Action, SARIF uploads, dispositions) would still count and decide per flow.
- **IDs without categories, so that an exposure keeps its ID as it grows.** A decision about a
  log line that writes a user name would then also cover the card number added to it later.
- **Keep `flow` beside `flows`.** One of the flows would be singled out for no reason, and
  readers of `flow` would silently see a fraction of the evidence.

## Consequences

- eShop: 214 → 67 findings (PF001 134 → 28, PF003 12 → 3, PF006 41 → 9; PFC01 27). The order
  request's seven categories are one finding at each of the six log calls the smoke test names,
  and `tests/smoke/eshop.expected.json` now requires exactly one finding there, citing at least
  seven flows.
- The benchmark corpus: 13,705 → 3,617 findings; without coverage gaps, 11,245 → 1,157. Polar
  9,304 → 1,394, PrivateGPT 1,543 → 473, Ghost 1,276 → 705. Flows are unchanged (21,568).
- The ID of every finding about flows changes once (PFC01 IDs do not). Dispositions recorded against 0.4.0 documents are carried to the
  exposures that group their flows when all of an exposure's flows were decided alike.
- The benchmark's labelling sheets (`benchmark/sheets.py`) sample findings of a 0.1 document and
  label each one's flow. They read the protocol run's documents, which stay 0.1; sampling a 0.2
  document would need a protocol deviation that says how an exposure is labelled.
- The fixture markers (`// expect: PF001`) name a rule at a sink line, so the fixture test is
  unchanged; 17 goldens have fewer findings.
