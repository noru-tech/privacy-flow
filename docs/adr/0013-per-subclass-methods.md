# ADR 0013: Template methods are analysed per subclass

Status: accepted, 2026-10-03.

## Context

[ADR 0011](0011-inheritance.md) left calls on `this`/`self` undispatched: a base class method has
one summary for every subclass, so sending `this.run()` to every override handed each subclass's
data to all the others (Redash went from 242 flows to 4,741). Template methods (a base method
that calls a hook its subclasses override, `notify()` calling `this.deliver()`) were reported as
`dynamic_call` gaps, "overridden in N subclasses", and the override's flows were missed.

## Decision

- **Template methods are copied into their subclasses.** After the provenance fixpoint, when the
  hierarchy is known, a method is a template if it calls, on `this`, a method that a subclass
  overrides, or another template method (to a fixpoint, so a helper that calls a hook makes its
  callers templates too). Each is copied, with the closures nested in it, into every subclass
  that inherits exactly that method, as that subclass's own method whose `this` is the
  subclass's. Its fields are the subclass's (created, and linked from the base's, where the
  subclass had none). Then the fixpoint runs again for the copies.
- In a copy, `this.deliver()` resolves through the subclass's lineage, to its own override; the
  original serves the base class itself. Neither is a gap.
- A copy's statements keep their file, position and text, so witness paths cite the base
  class's code. A copy is the same source or sink as its original: entries with the same anchor
  at the same place share one ID, and a flow found through several copies is reported once.
- **Budget:** at most 200,000 copied statements per scan; a template method whose copies would
  exceed what is left keeps its gaps.

## Alternatives

- **Receiver-sensitive summaries** in both engines (a summary per function and receiver class).
  Exact for every method, not only templates, at the cost of changing both engines and their
  equivalence; copying gives the same answer for the pattern that matters with no engine change.
- **Dispatch on `this` to every override.** Rejected in ADR 0011; the conformance vector
  `ts-template-isolation` now fails such an analyser.

## Measurements

Against ADR 0012's build, the benchmark corpus at its pinned commits:

| Application | flows | distinct flows | duplicates | "overridden" gaps |
| --- | --- | --- | --- | --- |
| Redash | 314 → 317 | 314 → 317 | 0 → 0 | 3 → 0 |
| PrivateGPT | 2,462 → 2,575 | 2,418 → 2,533 | 44 → 42 | 0 → 0 |
| Ghost | 105 → 92 | 85 → 85 | 20 → 7 | 3 → 0 |
| Polar | 11,521 → 7,444 | 7,124 → 7,124 | 4,397 → 320 | 3 → 0 |
| Healthchecks | 92 → 49 | 49 → 49 | 43 → 0 | 0 → 0 |
| Umami | 67 → 54 | 54 → 54 | 13 → 0 | 0 → 0 |
| CTFd | 43 → 41 | 41 → 41 | 2 → 0 | 0 → 0 |
| the other five | unchanged | | | |

"Distinct" counts flows by source and sink location and category; no distinct flow is lost on
any application. The drop in flows is duplicates going away: a call whose callee has two API
paths that match the same sink (`Message().send` and `EmailMultiAlternatives().send` for one
aliased Django message) made two sinks at one place, and every flow into it was reported twice.
The duplicates that remain have different sources at the same place (two fields of one typed
parameter). The added flows are template methods reaching an override: on PrivateGPT, the base
`TextReader.lazy_load_data` calling `self.lazy_document_load`, which `MarkItDownReader`
overrides. Time is unchanged.

## Consequences

- Conformance vector `ts-virtual-dispatch` is required (it was a known limitation), and
  `py-template-method` and `ts-template-isolation` are new; fixture `ts/inheritance` has no gap.
  CI breaks the copying (`per-subclass`).
- **Not covered:** a method that is not a template but reads a field a subclass writes still
  sees only what the base class stores ([ADR 0011](0011-inheritance.md)); copying every
  inherited method would cover it at a cost proportional to hierarchy size.
