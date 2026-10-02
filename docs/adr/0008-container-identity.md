# ADR 0008: A container is a location, not the path of a constructor call

Status: accepted, 2026-10-02. Narrows how containers work under ADR 0003.

## Context

Provenance follows values through containers: what goes in through `set`/`push` comes out of
`get`/`pop`. A container was identified by its variable, or by any API path its variable carried,
so that `globalThis.cache.set(k, client)` in one function and `globalThis.cache.get(k)` in another
meet. `new Map()` carries the API path `Map()`, which every `new Map()` in the program shares, so
every Map was one container: a read from any Map returned everything written to any Map.

On documenso this made the Prisma client (`remember('prisma', () => new PrismaClient())`, cached in
a Map) also "a PDF document", "a fetch response" and "a zod schema". Each of those paths was then
decided against the catalogue: the Prisma path correctly as a database call that passes nothing
from its arguments to its result, the others as propagators that do. A query's filter
(`findFirst({ where: { email } })`) then reached its result, and logging `envelope.id` reported the
email address. 2,198 of 2,243 Prisma calls carried such paths. The catalogue entry for the
built-in containers made it worse: `Map()**` matched any call on anything taken out of a Map.

The merged container also hid two gaps in how containers worked, which code relied on without
knowing it: a container in an instance field (`this.handlers.set(...)` in one method,
`this.handlers.get(...)` in another) had no identity of its own, and a `for…of` loop variable
carried the collection's provenance instead of its elements'.

## Decision

- **An API path ending in a call (`Map()`, `client.cache()`) does not identify a container.** It is
  a new value at each call. Named locations (`globalThis.cache`, `__prisma_remember`) still do.
- **An instance field is a container's location.** A write through `this.f.set(...)` goes to the
  field's own variable (ADR 0005), which every method's `this.f` reads.
- **A container keyed by its variable keeps the variables put into it**, as path-keyed containers
  already did, so a read sees their fields too (an object literal with an `analyze` method).
- **Loop variables are elements.** `for (x of xs)` and Python `for x in xs` (and comprehensions) emit
  a provenance-only `Elements` statement: `x` gets what `xs` holds. `values()` and `entries()` are
  views of the same elements. Data flow is unchanged: a container's data is its elements'.
- **The built-in containers' catalogue entry covers their own methods only**
  (`{Map,Set,WeakMap,WeakSet}().*`). A method on an element is decided by the element's provenance.

## Consequences

- documenso `8a41a3bf`: findings 757 → 549, flows 1,448 → 1,251, medium PF001 29 → 23, high PF002
  32 → 29, high PF005 3 → 1, coverage gaps 76 → 67; 2.0 s → 1.8 s. Every removed flow goes into a
  database call and comes back out of its result. The flows that reach a sink from the stored
  record (`user.email` read from `findUnique`, then sent to Stripe) are still reported, from that
  read. Of the removed gaps, each was reached only through such a round trip.
- On two private TypeScript monorepos: one goes from 36.6 s to 9.6 s with 2 flows removed and 248
  added, the added ones through registry lookups that now resolve to the registered handlers; on
  the other, 13 false flows are removed (a handler's typed `id` reported as reaching decrypted
  credentials) and one is added.
- Fixtures `ts/orm-query-results` (the documenso shape) and `ts/registries` pin this. Each of the
  first two decisions alone leaves `ts/orm-query-results` failing.
- **Not covered:** a container obtained from another container (`subscribers.get(k).add(cb)`) is a
  temporary with no location of its own, so what is added to it is not seen by a later read. The
  merged `Set()` container used to hide this. Two calls in 117,000 on the monorepo lost a
  resolution this way. Listed in KNOWN-LIMITATIONS.md.
