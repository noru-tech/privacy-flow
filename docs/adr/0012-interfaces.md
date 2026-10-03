# ADR 0012: TypeScript interfaces are supertypes of their implementations

Status: accepted, 2026-10-03.

## Context

[ADR 0011](0011-inheritance.md) made a call on a value typed with a base class reach every
subclass's override. TypeScript code more often types an injected dependency with an interface
(`constructor(private readonly mailer: Mailer)`, `Mailer` an interface that `SmtpMailer` and
`ConsoleMailer` implement). An interface is a type, not a value, so the annotation had no
provenance and personal data passed to `this.mailer.send(...)` was an `unresolved_method` gap.

## Decision

- An interface declaration is a class with no members of its own, bound to its name (hoisted
  and exported like a class), so annotations, `import type` and re-exports resolve to it. Its
  type declaration for typed objects is unchanged.
- `class C implements I` and `interface B extends A` make the class (or interface) a subtype:
  the interfaces are bases for dispatch and for the subclass relation, and never for member
  lookup. A call on a value typed `I` reaches the method of every local class that implements
  `I`, directly, through an interface that extends it, or through a base class.
- **Interfaces are open.** Implementations can come from anywhere (an adapter loaded at run
  time, another package), so the local implementations a call on an interface-typed value
  reaches do not explain its other targets: an external module the value may also come from is
  still a gap. Paths through built-in containers (`Map().get()`) say where the value was kept,
  not what it is, and are not.
- **A method no implementation defines** on a value known only as an interface instance is a
  method of a plain value, as before interfaces had provenance: `logger.info` on a local
  `Logger` interface is still a logger by name, `rows.push` still a mutator.
- `implements` of an external interface (`OnModuleInit` from `@nestjs/common`) adds nothing:
  only local interfaces are supertypes, and an external interface's path is never a fallback
  for a member the class does not define, which could otherwise match a catalogue sink the code
  never calls.

## Alternatives

- **Structural typing** (any class with a matching method set implements the interface, as the
  type checker sees it). Closer to TypeScript's semantics and catches classes that implement an
  interface without saying so, and object literals; it needs every method name of every class
  compared with every interface, and dispatches on names alone where classes share common
  method names (`send`, `get`). Declared `implements` is the common case in the dependency
  injection style this targets.

## Measurements

Against ADR 0011's build, the TypeScript and JavaScript applications of the benchmark corpus:
flows unchanged on all six; coverage gaps unchanged except Ghost, 609 → 612. Its three new gaps
are calls on a scheduler adapter typed with a local interface that extends
`@tryghost/adapter-base-scheduling`'s `SchedulerAdapter`, which the catalogue does not know; the
local implementation (an error-capturing wrapper) had hidden them. Time unchanged. These
applications inject dependencies by class (NestJS in Hoppscotch) or through untyped JavaScript
(Ghost), so the corpus has little of what this targets.

Two first attempts failed on it. Without the plain-value rule, Umami lost 11 flows
(`context.logger.info` on a local `Logger` interface stopped being a logger) and gained 5
dynamic-call gaps. Treating every run-time global path as a container, rather than the built-in
containers only, hid Ghost's `require()()` adapter: `require` is a global too.

## Consequences

- Conformance vector `ts-interface-dispatch` (interface, implementation and use in three files,
  through `import type`); fixtures `ts/interfaces` (an interface that extends another, an abstract
  class that implements one) and `ts/interface-open-world` (a value typed with an interface and
  loaded from an unknown module reaches the local implementation and is still a gap). The
  `inheritance` CI break also removes `implements`.
- **Not covered:** classes that implement an interface structurally without `implements`, object
  literals typed with an interface, and type aliases of object types. A call on such a value
  stays a gap when personal data reaches it.
