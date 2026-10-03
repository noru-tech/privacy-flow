# ADR 0014: Exceptions reach their catch, and library errors may echo their input

Status: accepted, 2026-10-03.

## Context

A value thrown was not connected to the `catch` (or `except`) parameter, a silent limitation:
`try { throw new Error(email) } catch (e) { log(e) }` was missed. Logging a caught error is also
the most common way personal data reaches logs and error tracking without anyone writing it
there: a database, HTTP or SDK call is given personal data, fails, and its error, which may
quote what it was given (Prisma validation errors list the invocation, axios errors carry the
request), is logged whole. Reviewers of the benchmark corpus counted about sixteen such sites
among the sampled sinks that receive personal data, the largest group piiflow missed.

## Decision

- **Throws are a return slot.** Every function has a throw slot. `throw x` / `raise x` goes to the
  innermost enclosing catch parameter in the same function, or to the throw slot. The slot exits
  at each call site to that call's catch parameter, or, outside a `try`, to the caller's own throw
  slot, so an uncaught exception propagates up the call chain. The engines are unchanged: return
  slots and their exits are already generic (ADR 0009).
- **A callback handed to a library throws through the library call**
  (`prisma.$transaction(async tx => ...)`): its throw slot reaches the catch around that call.
- **Library errors may echo their arguments.** A call inside a `try` body into a library (an API
  path that is not a language built-in) or one the analysis cannot resolve passes its arguments
  to the catch parameter. Calls into the project's own code do not: what they throw goes through
  their throw slots.
- **Echoes are for review.** A flow through an echo (hop kind `error`) is reported at `info`
  severity with `needs_review`, like a maybe-personal source: whether an error quotes its input
  depends on the library and the failure. Explicit throws (hop kind `throw`) are findings at
  their rule's severity.

## Alternatives

- **Explicit throws only.** Sound and quiet, but the labelled sites are almost all library errors.
- **Echo at full severity.** Would make every `catch (e) { log(e) }` after a database write a
  medium finding; reviewers would have to dismiss most of them.
- **Exception types** (`except ValueError` catching only what raises `ValueError`). Needs class
  hierarchies of exceptions, including the standard library's; flow-insensitive over-approximation
  costs little here.

## Measurements

The benchmark corpus at its pinned commits, against the build before (with ADR 0013 and the
`state` and registry fixes):

| Application | flows | through a throw | through an echo |
| --- | --- | --- | --- |
| Polar | → 8,034 | 187 | 510 |
| PrivateGPT | → 1,581 | 79 | 251 |
| Hoppscotch | 13 → 152 | 0 | 139 |
| Umami | 54 → 132 | 0 | 78 |
| Vercel chatbot | 49 → 94 | 0 | 41 |
| Redash | 315 → 351 | 5 | 31 |
| Ghost | 92 → 108 | 11 | 5 |
| Healthchecks | 49 → 55 | 1 | 6 |

Echo flows are mostly of category `unknown` (names only maybe personal). Against the R1 site
labels, three more sampled sinks are found (Hoppscotch's mailer and mock-server errors, a team
collection's transaction error). The other labelled error-logging sites carry data piiflow does
not classify as a source (user and customer identifiers, request input of an unmodelled
framework). Time is unchanged.

## Consequences

- Conformance vectors `ts-throw-catch` and `py-raise-across-calls`; fixtures `ts/exceptions` and
  `py/exceptions` (a throw, a throw two calls down, a Prisma and a `requests` echo, and a local
  call that does not echo). Hop kinds `throw` and `error` are added to the flow-facts schema.
- **Not covered:** exception types; `finally` blocks; errors of the project's own functions that
  wrap their input without throwing it (`return { error: input }`); a re-raise without a value
  (`raise`) is not followed.
