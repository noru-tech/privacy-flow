# ADR 0005: Per-field variables for `this`, and constructors that return their own instance

Status: accepted, 2026-10-02.

## Context

The first version modelled every instance of a class as one shared variable (the class's `this`),
flow-insensitively. On documenso this produced long cross-module flows that did not exist: an IP
address put into an error object on the server "reached" `console.error(error)` in unrelated
client dialogs, because every `AppError` instance shared one `this`. Separately, field sensitivity
was one level deep, so `this._options = { apiKey, endpoint }; fetch(this._options.endpoint)` sent
the API key to the URL.

## Decision

- **Per-field variables.** In methods, `this.f` is a variable of its own, one per (class, field),
  shared across methods (`ThisLoad`/`ThisStore` in the IR). Reading `this.config.url` reads the
  `config` field's variable and then its `url` field, which keeps the object's fields apart: two
  levels of field sensitivity for the pattern where it matters most. The whole instance (`log(this)`)
  still sees every field.
- **Constructor instances.** A constructor has its own `this`, a local it returns. `new C(args)`
  (Python `C(args)`) gets the constructor's summary at that call site, so an instance carries only
  what it was built with. Constructor writes also go to the per-field variables, so methods see
  them. Classes without a constructor keep the shared `this` for their field initialisers.
- Both `this` variables have "an instance of C" as provenance, so `this.method()` resolves.

## Alternatives

- **Allocation-site abstraction for all objects.** The general fix, and the direction for a later
  version; it changes every heap edge. This decision covers class instances, which were the source
  of the observed false flows, at a fraction of the cost.
- **Deeper field tokens (access paths of length 2 for every object).** Doubles the token space and
  complicates summaries' "a field the function does not name" token; the per-field variable gives
  the same precision for instance fields without it.

## Consequences

- Medium-severity PF001 findings on documenso went from 84 to 29 without losing true positives in
  the sample reviewed; the conformance vectors `ts-instance-isolation` and `py-instance-isolation`
  pin the behaviour, and CI breaks it deliberately to check the corpus notices.
- A method that mutates an instance after construction writes to the per-field variable, which an
  instance created elsewhere does not carry when logged as a whole: `c = new C(); c.setEmail(e);
  log(c)` is a missed flow. Listed in KNOWN-LIMITATIONS.md.
- Plain objects remain one level deep: `const o = { a: { email } }; log(o.a.id)` is reported.
