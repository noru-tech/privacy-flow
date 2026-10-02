# ADR 0009: Allocation sites across calls, cloned per call site and returned through slots

Status: accepted, 2026-10-03. Extends ADR 0007 across calls.

## Context

ADR 0007 made literals allocation sites within a function and stopped at calls, because a
context-insensitive extension would let a callee's return value hold its callers' data. Across a
call, an object kept one level of field tokens: `wrap()` returning `{ data: { email, id } }` read
as `r.data.id` in the caller reported the email address. On documenso, after ADR 0008, about 11 of
the 23 medium PF001 findings went through such a returned object, most of them
`return { ...envelope, user: { id, name, email } }` read as `envelope.user.id` or
`document.envelopeId` in a caller.

## Decision

- **Clones per call site.** A call that reaches only local functions (no API, class, unresolved or
  dynamic target) holds, at that call site, a clone of each site its callee returns, keyed by
  (call site, original literal). A clone's fields mirror the callee site's fields through the
  call; a site returned through several functions is cloned at each call, and recursion closes on
  itself. At most 32 clones per call site, beyond which the result is unknown.
- **Return slots.** The engines' fixed "a return value exits to the call's result" becomes a table:
  a function's **return slots** are its return value and the field variables of the sites it
  returns, and each slot has an **exit** at each call site. Summaries record the slot reached;
  applying a summary at a call site continues at that slot's exit there, so a helper called with
  an email address and with an ID returns each to its own call site. Seed-mode search follows a
  slot to every call site, as it did the return value. Both engines implement this
  (`src/engine/mod.rs`), and the fixture suite checks that they agree.
- **A function's own slots never stop its summary,** shared or not. A field variable shared with
  another function would otherwise turn a context-specific return into global state.
- **Closures do not share.** A heap variable used from a function nested in its owner is not
  shared, as the locals a closure captures are not.
- **Spreads are rests.** What spreads into a literal (`{ ...record, owner }`, `**d`) is the
  literal's rest, a variable of its own that each spread copies into: a read of field `k` reads
  the site's own `k` and `k` of the rest. A clone has a rest of its own, and the literal's rest is
  one more return slot. A spread no longer makes the whole literal unknown.
- **`catch` and `finally`** on a JavaScript promise hold what the promise holds, for allocation
  sites; their callbacks carry nothing, as in the data model.

Everything else is unchanged: a call result that may also come from a library is unknown, and
its field reads keep their `Load` edges.

## Measurements

| | before (ADR 0008) | after |
| --- | --- | --- |
| documenso `8a41a3bf`: findings | 549 | 523 |
| medium PF001 | 23 | 14 |
| high PF002 | 29 | 27 |
| time, peak memory | 1.8 s, 390 MB | 1.9 s, 390 MB |
| private monorepo A: flows | 39,044 | 37,616 (1,429 removed, 1 added) |
| time, peak memory | 9.6 s, 3.3 GB | 12.0 s, 3.7 GB |
| private monorepo B: flows | 6,011 | 5,977 (35 removed, 1 added) |

Every removed documenso finding sampled, and every high-severity one, went through a returned object
read in a caller: an email address reported as reaching a log of `envelope.user.id`, or PostHog
through `result.userId` while the address was in `result.user`. The flow added on monorepo B is a
real one (task IDs, classified by the project's data map, sent in a query string); the one on A is a
110-hop chain through generic scanner data structures.

## Found on the way

- A first version added 127 flows on monorepo A, all unrealisable: data entered a function at one
  call site and left at another. A closure inside the function read a clone's field, which made
  the field variable shared, and a summary stops at shared variables, handing the data to every
  caller. Hence the last two rules above.
- Witness paths now cite a spread before a read through it (source, `...user`, `copy.email`),
  without the alias assignment between them: one golden changed for that reason.

## Consequences

- Conformance vectors `ts-returned-nested`, `ts-returned-context`, `ts-returned-spread` and
  `py-returned-dict`, and fixture `ts/returned-objects` (the documenso shapes: a two-level spread,
  `.catch()`, a closure over a call result). `ts-returned-context` passes on the previous build
  too; it guards against a context-insensitive implementation. CI breaks the pass
  (`call-context`).
- **Not covered:** objects passed *into* a call still collapse one level
  (`f({ user })` read as `input.user.id` inside `f`), which is the parameter-side twin of this
  decision; and a returned object whose callee is also reached through a library or a dynamic
  call stays unknown.
