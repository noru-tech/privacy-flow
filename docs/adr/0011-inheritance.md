# ADR 0011: Inheritance, virtual calls on typed values, and properties

Status: accepted, 2026-10-03.

## Context

Classes had no bases. A method a subclass inherits did not resolve on its instances, `super.m()`
and `super().m()` were dynamic calls, a field a base constructor set was a different variable
from the same field read in a subclass, and a call on a value typed with a base class reached
only the base's (often abstract) method. Reading a property (`self.transport`, a getter) gave the
function, not what it returns. On the benchmark corpus these were most of the `dynamic_call`
gaps (227 on Polar, 73 on PrivateGPT, 24 on Healthchecks) and a group of missed flows: every
Healthchecks notification goes through `self.transport.notify(...)`, a property typed
`Transport` with about thirty subclasses that override `notify` and inherit helpers.

## Decision

- **Bases are values.** The lowering records each class's base expressions (`extends Base`,
  `class C(Base, Mixin)`; not `implements`, not keyword arguments), and their provenance gives
  the bases: a local class, or an API path for an external one.
- **Lookup walks the lineage.** A member of a class or instance is the first definition in the
  class and its local ancestors, breadth-first. Constructors are inherited the same way. When
  no local class in the lineage defines it, the member is the external base's
  (`class Analytics extends PostHog` → `posthog-node:PostHog().capture`).
- **`super`** in a method is its class's bases: `super.m()` and `super().m()` are the bases'
  `m`, and `super(...)` is their constructor.
- **Fields go down the hierarchy.** What a class stores in `this.f` flows to `this.f` in its
  subclasses (a base `__init__` sets `self.channel`, a subclass method reads it), for data and
  provenance. Not up: with a field linked both ways, every subclass sees what each of its
  siblings stores. Redash's query runners all keep their configuration in one inherited field.
- **Virtual calls on typed values.** A call on an instance held anywhere but `this`/`self` (a
  parameter, a field, a property typed with the base class) also reaches every override in a
  local subclass: any of them can be the receiver.
- **Not on `this`.** A call on `this`/`self` follows only the class's own lineage. A base
  method has one summary for every subclass, so dispatching `this.run()` to every override hands
  each subclass's data to all the others. Where subclasses override the method, the call is a
  `dynamic_call` gap ("overridden in N subclasses") when personal data reaches it.
- **Properties.** Reading a Python `@property` (or `cached_property`) or a JavaScript getter
  gives the provenance of what it returns, so its return annotation types the value.

## Alternatives

- **Analyse inherited methods per subclass** (clone a base method's summary for each receiver
  class). Precise for template methods (`this.deliver()` in a base class reaching the subclass's
  `deliver`), and the direction for a later version; it multiplies summaries by hierarchy size.
- **Dispatch on `this` too.** Tried first: Redash went from 242 flows to 4,741, nearly all of
  them one query runner's credentials reaching another runner's logs through
  `BaseQueryRunner._run_query_internal` calling `self.run_query`.
- **Link fields both ways.** Tried first; same failure through shared base fields.

## Measurements

Against `36c2ea4`, the benchmark corpus at its pinned commits:

| Application | flows | gaps | `dynamic_call` | `unresolved_*` |
| --- | --- | --- | --- | --- |
| fastapi-template | 3 → 3 | 9 → 4 | 3 → 0 | 6 → 4 |
| Healthchecks | 5 → 6 | 58 → 28 | 24 → 6 | 34 → 22 |
| CTFd | 39 → 39 | 75 → 76 | 17 → 3 | 58 → 73 |
| Redash | 242 → 307 | 139 → 150 | 21 → 12 | 118 → 138 |
| PrivateGPT | 761 → 2,462 | 266 → 232 | 73 → 30 | 193 → 202 |
| Polar | 10,472 → 11,518 | 916 → 784 | 227 → 64 | 687 → 718 |
| Ghost, Hoppscotch, Umami, Taxonomy, open-saas, Vercel chatbot | unchanged | ±2 | ±1 | ±3 |

Time is unchanged within noise (Polar 13.4 s → 13.3 s, PrivateGPT 2.6 s → 2.9 s). Some dynamic
calls become `unresolved_callee` gaps: a method inherited from an external class the catalogue
does not know now has a path that names it. Most added flows on PrivateGPT start from
`context.state`, which the classification table reads as an address's state, and reach every
chat interceptor through `interceptor.intercept(context)`; the dispatch is real, the source is
not. On Polar they go through `benefit_strategy.grant(...)` to each benefit strategy.

## Consequences

- Conformance vectors `ts-inherited-method`, `ts-super-call`, `ts-inherited-field`,
  `ts-typed-dispatch`, `ts-getter`, `py-inherited-method`, `py-super-call`,
  `py-inherited-field`, `py-virtual-dispatch` and `py-property`; `ts-virtual-dispatch` (a
  template method) is a known limitation. Fixtures `ts/inheritance` and `py/inheritance`. CI
  breaks base resolution (`inheritance`).
- **Silent:** a value a subclass stores in an inherited field is not seen by base class methods
  that read it.
- **Not covered:** TypeScript interfaces (`implements`): a value typed with an interface does not
  reach the classes that implement it; personal data passed to its methods stays a gap.
  Mixins built by functions (`class C extends withLogging(Base)`) have no local base.
