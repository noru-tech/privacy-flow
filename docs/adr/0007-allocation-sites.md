# ADR 0007: Allocation sites for plain objects, within what points-to can see

Status: accepted, 2026-10-02.

## Context

Nodes are `(variable, field token)`, which keeps one level of an object's fields apart. A value
stored into a field is collapsed when the field is read: `o = { a: { email, id } }; log(o.a.id)`
reports the email address, because `(o, a)` holds the whole inner object and `o.a` reads it as
`TOP`. ADR 0005 fixed this for instance fields with a variable per (class, field) and named the
general fix, allocation-site abstraction, as the direction for a later version. Separately, the
analysis had no aliasing: `p = o; p.a = email; log(o.a)` was a missed flow, because the write
went to `(p, a)` and nothing flowed back to `o`.

## Decision

- **Sites.** Every literal (`{...}`, `[...]`, a Python dict or list) is an allocation site. Each
  named field of a site that something writes to gets a variable of its own, like `this.f` in
  ADR 0005.
- **Points-to, inside the facts builder.** A flow-insensitive, inclusion-based points-to analysis
  over the statements' copies, field writes and field reads finds which sites each variable may
  hold ([`src/facts/heap.rs`](../../src/facts/heap.rs)). A variable is **unknown** when its value
  can come from anywhere the analysis does not model: a parameter, a return slot, an instance
  field, a call result, an import, a global, a seed, string building, a catalogue flow, or a read
  of an unknown value's field. A variable holding more than 32 sites is unknown.
- **Edges.** A read `o.k` where `o` is not unknown becomes a `Copy` from each of `o`'s sites' `k`
  variables, which keeps the value's own field tokens, so nesting is kept apart at any depth. A
  read of an unknown variable keeps its `Load` edge, as before. A write `o.k = x` adds a `Copy`
  from `x` to the `k` variable of every site `o` may hold, and keeps its `Store` edge, so the whole
  object (`log(o)`) still carries every field. A site written with a computed key (`o[k] = x`) is
  dirty: reads of it keep the `Load` edge as well.
- **Shared.** A site's field variable is shared (summaries stop at it) when the site is in module
  code or the field is read or written from another function; otherwise it is local to the
  function, like any other temporary.
- **The engines do not change.** They read edges; neither knows about sites, so the worklist engine
  and the Datalog oracle agree by construction, and the fixture suite checks it.

## Why this boundary

Points-to stops at parameters and returns on purpose. Making a call's result point to the sites
its callee allocates, or a formal point to its callers' sites, would make one site's field
variables carry data from every caller to every caller. A function's return slot would then hold
data from outside the function, which is the property the engine relies on to follow returns to
every call site without creating unrealisable paths (`src/engine/mod.rs`). Across calls, objects
keep the summaries' one-level field tokens, which are context-sensitive.

Soundness relative to the previous analysis is the invariant the edges are built to keep: a read
stops using its `Load` edge only when every value the edge could carry also reaches it through a
site's field variable, with finer tokens.

## Alternatives

- **Interprocedural, context-insensitive points-to.** Removes more false flows through wrappers
  (`return { success: true, data: result }` read as `r.data` in the caller), but breaks the
  return-slot property above. Rejected for now; doing it right means cloning sites per call
  context, which is a design of its own.
- **Access paths of length two for every token.** The alternative ADR 0005 also rejected: it
  doubles the token space and complicates summaries' "a field the function does not name" token,
  and still stops at two levels.

## Consequences

- The cases in KNOWN-LIMITATIONS.md now work inside a function, and across functions through
  module-level objects and closures: `o = { a: { email } }; log(o.a.id)` is not reported, and
  `p = o; p.a = email; log(o.a)` is. Conformance vectors `ts-nested-fields`, `py-nested-fields`
  and `ts-alias-write`, and fixtures `ts/nested-objects` and `py/nested-dicts`, pin this behaviour.
  The fixtures fail on the previous build (four false findings, two missed). CI breaks the pass
  (`allocation-sites` in `break_engine.py`) and requires the corpus to fail.
- Field reads inside a function no longer exercise the `Load` and `Store` edge semantics, so the
  corpus's `stores` break went unnoticed. `ts-returned-object` restores it with an object that
  crosses a return, where sites stop.
- Measured on two TypeScript monorepos of about 3,000 source files each: no change in run time or
  peak memory. On one, a single false flow disappears: an API type's `name` field reported as
  reaching a log of `allData.organization.id`. On the other, nothing changes. Most of the false
  flows still seen on real code go through a wrapper object that is returned from one function and
  read in another, which this decision leaves alone (see Alternatives).
- On documenso `8a41a3bf` (the repository behind ADR 0005's numbers), the output is identical:
  1,448 flows, 757 findings, 29 medium PF001. Run time goes from 1.9 s to 2.0 s and peak memory
  from 355 MB to 400 MB, mostly in the engine, which now carries finer field tokens. A rough
  classification of the 29 medium findings by path shape (not a labelled review): about 12 are
  true (IP addresses and email addresses logged), about 11 go through an object returned from one
  function and read in another, and about 6 go through a Prisma call whose arguments the library
  model passes to its result (`findFirst({ where: { email } })`, then `envelope.id` logged). None
  involves nested literals within a function. The claim in NOTES.md that allocation sites would
  remove the main remaining source of false flows on documenso holds only for sites across calls
  and a finer library model, not for this decision.
- An object that escapes into a call and is mutated there (`f(o)` where `f` writes `p.a = email`)
  is still not seen by the caller's reads. It was not seen before either.
