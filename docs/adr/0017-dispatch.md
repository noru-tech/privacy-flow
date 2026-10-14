# ADR 0017: Dispatchers: library calls that reach the project's handlers

Status: accepted, 2026-10-14.

## Context

A mediator (MediatR in most .NET services) decouples a caller from its handlers:
`mediator.Send(command)` runs the `IRequestHandler<TCommand, TResult>` registered for the
command's type, and every `IPipelineBehavior<TRequest, TResponse>` around it, without any code
calling them by name. The call is into a library; a handler is a class no call reaches.

eShop shows why it matters. Its ordering API builds a `CreateOrderCommand` holding the buyer's
address and card number and sends it through MediatR. `OrdersApi.cs` is careful not to log the
request ("don't log the request as it has CC number"), but the pipeline's `LoggingBehavior`,
`ValidatorBehavior` and `TransactionBehavior` log every command whole, with `{@Command}`. Without
the mediator, those handlers had no callers and the card number never reached the log; with a
plain "the library call propagates", it reached the call's result and nothing else.

## Decision

A new catalogue file, `catalogue/dispatchers.yml`, and one generic rule in `src/facts.rs`. An
entry names the dispatching calls (API path globs) and its targets: methods `method` of the
project's classes that implement or extend `implements`, receiving the argument as parameter
`param`. With `type_arg`, a target is reached only when the dispatched value's class is the
interface's type argument at that index, or extends it:

```yaml
- id: cs.mediatr.request
  language: csharp
  calls: ["MediatR.{IMediator,ISender,Mediator}{,()}**.{Send,CreateStream}"]
  arg: 0
  targets:
    - { implements: "MediatR.{IRequestHandler,IStreamRequestHandler}", method: Handle, param: 0, type_arg: 0 }
    - { implements: "MediatR.{IPipelineBehavior,IStreamPipelineBehavior}", method: Handle, param: 0, type_arg: 0 }
```

- **Matching a call.** One of the call's API paths matches `calls`, or one of its local targets,
  named by its class's qualified name (`Shop.Events.Bus.Publish`), does: a project can declare
  its own bus in `.privacy-flow.yml`.
- **The type filter.** A type argument that is one of the handler class's own type parameters
  (`LoggingBehavior<TRequest, TResponse>`) accepts anything: an open generic behavior runs for
  every request. A project class accepts values whose provenance is an instance of it or of a
  subclass. A value of no known class reaches every candidate.
- **The binding** is a call site and an argument binding, exactly as a direct call to the handler
  makes; the handler's return value is the call's result. Nothing else is new, so the Datalog
  engine, which reads the same sites and bindings, agrees with the worklist engine without a
  change.
- **The call is explained**: no coverage gap for it.

Candidates (classes, their receiving methods and type arguments) are computed once per target.

## Alternatives

- **Model MediatR in the analysis.** One framework in Rust, against principle 6 (rules are
  data); every other mediator or bus would need the same.
- **Treat the handlers as entry points with tainted parameters.** Every handler's parameter
  would be a source of unknown category, losing the categories the command carries, and the
  pipeline behaviors would log "unknown".
- **No type filter.** Every handler would receive every command; on eShop, the cancel handler
  would log the order's card number.

## Measurements

eShop at `dc7ea499`: the three pipeline behaviors report the card number, address and user name
of the order command (`LoggingBehavior.cs:9`, `ValidatorBehavior.cs:30`,
`TransactionBehavior.cs:41` and `:59`); `IdentifiedCommandHandler.cs:79` and the command
handlers receive only their own commands. On the 100k-line synthetic C# service, which sends a
request from each of its 1,587 modules, computing the candidates once per target instead of per
call took the facts phase from 2.0 s to 0.45 s.

## Consequences

- Entries for MediatR requests and notifications. MassTransit's in-process mediator is not
  covered: its consumers receive a `ConsumeContext<T>` whose `Message` holds the value, which
  needs a binding into a field.
- Fixture `cs/mediatr-dispatch`, conformance vector `cs-mediatr-dispatch`, and a `dispatch`
  break in the `conformance catches` CI job.
- A message broker's consumers (RabbitMQ, Service Bus) are not reached from the producer: the
  message leaves the process, which is the sink.
