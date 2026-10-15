# The catalogue

Sources, sinks, sanitisers, propagators, frameworks and dispatchers are data, in YAML under
[`catalogue/`](../catalogue/), compiled into the binary and versioned together (the `version`
key of each file, and a SHA-256 over all of them that every output document records). Adding an
SDK is a change to these files, not to Rust; a project can add entries of the same shape in
[`.privacy-flow.yml`](configuration.md). `piiflow rules` lists everything loaded.

## API paths

The catalogue identifies a call by its **API path**: what the called value is, traced back to an
import, a global or a type annotation.

| Code | API path of the call |
| --- | --- |
| `import pino from 'pino'; const log = pino(); log.info(x)` | `pino:().info` |
| `import OpenAI from 'openai'; new OpenAI().chat.completions.create(…)` | `openai:().chat.completions.create` |
| `import { PostHog } from 'posthog-node'; new PostHog(k).capture(…)` | `posthog-node:PostHog().capture` |
| `import * as Sentry from '@sentry/node'; Sentry.setUser(u)` | `@sentry/node:setUser` |
| `function f(log: Logger)` with `import type { Logger } from 'pino'`, then `log.info(x)` | `pino:Logger.info` |
| `prisma.$transaction(async (tx) => tx.user.create(…))` | `@prisma/client:PrismaClient().$transaction.<cb0>.user.create` |
| `console.log(x)`, `fetch(url)` (JavaScript globals) | `console.log`, `fetch` |
| Python `import logging; logging.getLogger(__name__).info(x)` | `logging.getLogger().info` |
| Python `from openai import OpenAI; OpenAI().chat.completions.create(…)` | `openai.OpenAI().chat.completions.create` |
| Python `def f(client: WebClient)` from `slack_sdk`, then `client.chat_postMessage(…)` | `slack_sdk.WebClient.chat_postMessage` |
| Python `print(x)` | `builtins.print` |
| C# `ILogger<OrdersApi> logger` (injected), then `logger.LogInformation(…)` | `Microsoft.Extensions.Logging.ILogger.LogInformation` |
| C# `new HttpClient().PostAsJsonAsync(…)` (an extension method of `HttpClient`) | `System.Net.Http.HttpClient().PostAsJsonAsync` |
| C# `Console.WriteLine(x)`, `string.Join(…)` | `System.Console.WriteLine`, `System.String.Join` |
| C# `[FromBody]` on a parameter | `Microsoft.AspNetCore.Mvc.FromBodyAttribute()` |

JavaScript paths are `module:path`; Python paths are fully dotted. C# paths are
namespace-qualified types and their members, with generic arguments stripped: the type comes
from the declaration (`ILogger<T> logger`), a constructor call (`new HttpClient()` gives
`HttpClient()`), or a static access; extension methods are members of the type they extend,
and attributes are constructions of the attribute type. A C# file names a type without its
namespace, so the analysis qualifies it by the `using` directives in scope: a type the catalogue
spells (a literal prefix of one of its C# patterns) is qualified exactly, which is why C# entries
spell their receiver types (`Microsoft.Extensions.Logging.{ILogger,ILoggerFactory}**.LogInformation`)
rather than start with a wildcard ([ADR 0016](adr/0016-csharp.md)). A call adds `()`. A callback
passed to an API gets the parameter path `<cbN>` (or `<fieldN>` for a function in a field of an
options object, such as a `render` prop). Imports of the project's own files, workspace packages
(`package.json` names) and `tsconfig.json` path aliases resolve to local functions instead, which
the analysis follows through function summaries.

How values get a path (provenance) is described in [the design](design.md#provenance): imports,
member access, calls, `new`, type annotations, callbacks, values stored in and read from
containers (`globalThis.cache.set(k, client)` … `cache.get(k)`), and object fields.

## Patterns

Entries match API paths with globs: `*` matches within one segment (no `.`), `**` matches any run
of segments, `**.` also matches none (`posthog-js:**.capture` matches `posthog-js:capture`), and
`{a,b}` alternates. Instance segments are usually written `Name{,()}` so that a constructed instance
(`Name()`) and an annotated one (`Name`) both match.

## Sinks (`catalogue/sinks.yml`)

```yaml
- id: js.posthog                # stable; findings and declarations refer to it
  language: javascript          # javascript (also TypeScript), python or csharp
  class: analytics              # log, error_tracking, analytics, messaging, llm, http,
                                # browser_storage, device_storage, third_party
  processor: PostHog            # the vendor that receives the data, if there is one
  calls: ["{posthog-node,posthog-js}:**.{capture,identify,alias}"]
  assigns: []                   # property writes that are sinks (`document.cookie`)
  receivers: []                 # heuristic: receiver names (`logger`) …
  methods: []                   # … and method names, for receivers of unknown origin
  args: all                     # or a list of positional indices
  keywords: []                  # Python keyword arguments to include when args is a list
  receiver: false               # the receiver is sent too (a message object's `send()`)
  host: { arg: 0, keyword: url, field: url, base_url: false }   # outbound HTTP only
  decorated: []                 # project functions with one of these attributes are this sink
```

With `receiver: true` the object the method is called on carries data out as well as the
arguments: `EmailMessage(subject, body, to=[user.email]).send()` sends what the message was built
with, though `send()` itself takes nothing. Builder methods that put data into such an object
(`msg.set_content(text)`, `notifier.add(url)`) are `args_to_receiver` propagators.

`host` says where an HTTP call's URL is: a positional `arg`, a Python `keyword`, or a `field` of an
options object. A literal URL (or a template or concatenation starting with one, followed through
constants) gives the host, which names the processor; a relative URL is the application's own
origin; anything else is `dynamic` ([PF006](rules/PF006.md)). With `base_url: true` a relative URL
resolves against a client's configured base URL, so it is dynamic too. `built_arg: N` follows
the URL argument to the call that built it and takes that call's argument `N`
(`SendAsync(new HttpRequestMessage(HttpMethod.Post, url))`), or its `field` when it was set in an
initializer (`RequestUri = …`); `wrappers` are calls the URL passes through (`new Uri(url)`).

`decorated` makes calls to the project's own functions a sink when they carry a matching
attribute: a `[LoggerMessage]` method, whose body a source generator writes, logs its arguments;
a Refit interface method is an HTTP call.

## Sources (`catalogue/sources.yml`)

Personal data enters a value in five ways. Three need no catalogue entry:

- **Field reads** of names the [classification table](../vendor/classification/classification.json)
  maps to a category (`user.email`, `data["phone_number"]`, `getattr(u, "email")`,
  `form.get("email")`, destructuring). Names the table marks `maybe_pii` are sources of category
  `unknown` that need review.
- **Parameters** whose own name is classified (`function send(email)`).
- **Typed objects**: a value annotated with a type whose declaration (an interface, type alias,
  class, Pydantic model, dataclass, ORM model, or a data-map collection of the same name) has
  classified fields carries those fields, each a source cited at the annotation with the field's
  declaration.

The catalogue adds request input from frameworks:

```yaml
- id: js.express.handler
  language: javascript
  kind: handler                       # handler | route_export | read
  framework: Express
  registrations: ["express:**.{get,post,put,patch,delete,all,use}"]   # calls taking the handler
  params:
    - index: 0                        # or `name: request`, or `all: true`
      fields: { body: unknown, query: unknown, ip: user.device.ip_address }
    - index: 1
      provenance: "express:Response"  # what the parameter is, so res.json() resolves
```

`handler` sources are functions passed to a registration call, decorated by a matching decorator
(FastAPI), or defined in matching files (`files`, Django views); or methods of classes that
extend one of `bases` or carry one of `class_decorators` (MVC controllers), or that take a
parameter of a `has_param_type` type (gRPC services), filtered by `members` (`require` and
`exclude` modifiers, `exclude_decorators`). `route_export` sources are named exports of matching
files (Next.js route handlers). `read` sources are reads and calls of API paths
(`flask.request.form`, `next/headers:cookies`); with `arg_category`, a literal argument decides
the category (`FindFirst("sub")` is a pseudonymous identifier, `FindFirst(ClaimTypes.Email)` an
email address). `property` sources are properties with one of `decorators` that a framework sets
from the request (Blazor's `[SupplyParameterFromForm]`).

A parameter's `whole`, `fields` and `methods` give categories; `typed: true` narrows an annotated
parameter to its type's classified fields (FastAPI with a Pydantic body); `by_name: true`
classifies by the parameter's own name. Injected dependencies are left alone by `skip_types`
(globs over the annotation and the declared type's path), `skip_names`, `skip_bases` (a class
extending a `DbContext`), `skip_interfaces` (any of the project's interfaces) and `skip_attrs`
(`[FromServices]`); `classify_only_types` keeps `whole` off a `Guid` or a number;
`expand_attrs` binds each property of a parameter's type on its own (`[AsParameters]`); `by_attr`
picks a rule by attribute (`[FromBody]`), ahead of `by_type` and the skips.

## Classification context (`catalogue/classification.yml`)

The [classification table](../vendor/classification/classification.json) is vendored verbatim
from privacy-datamap, and its exact names are meant to mean the same thing in every schema. A few
do not, and this file keeps them to the context where the table's category holds:

```yaml
- id: address-state
  names: [state, province]          # table names this entry limits
  objects: ["*address*", "*addr"]   # globs over the object the name is read from
  siblings: [city, postal_code, zip, country, street]   # names beside it that are context enough
```

An entry with a `category` adds names instead of limiting the table's: classified with that
category everywhere when it gives no context (`user_id`, `customer_id`), and only in context when
it does (`id` on a `user` or a `Customer`; [ADR 0015](adr/0015-identifiers.md)).

A name in `names` of an entry without a `category` is classified only when the object it is read from (`address.state`,
`billing_address["state"]`, `this.state` in a class named `Address`) matches one of `objects`,
or when another field of the same type, or another parameter of the same function, is one of
`siblings`. Otherwise it is not a source: `context.state` and `def set_state(state)` are
application state. A project that means `state` as an address everywhere says so under `fields`
in `.privacy-flow.yml`, which always applies.

Identifiers of things are declared apart, by the words that make them up
([ADR 0021](adr/0021-uncertain-findings-and-identifiers-of-things.md)):

```yaml
entity_identifiers:
  - id: entity-identifiers
    entities: [item, product, brand, catalog, category, type, order_line]   # words, or compounds
    suffixes: [id, ids, uuid]                                               # the last word
```

A name is an identifier of a thing when its words, camelCase split, are entity words followed by
a suffix (`item_id`, `catalogItemId`, `BrandIds`, `item_type_id`, `productid`). One other word
anywhere keeps it out: `user_type_id`, `blood_type_id` and `order_id` are not matched. Such a name
is never classified, and request input under it (a route parameter `ItemId`, `query["itemId"]`)
is not a source. A project's `fields` still classify it.

## Sanitisers (`catalogue/sanitisers.yml`)

```yaml
- id: js.password-hash
  language: javascript
  calls: ["{bcrypt,bcryptjs}:{hash,hashSync}", "argon2:hash"]
  methods: []                 # methods of plain values (`includes`)
  functions: []               # local functions, `path/to/file:name`
  removes: [user.authorization]   # category prefixes, or "*" for everything
```

The defaults are deliberately few: password hashing removes credentials, and predicates
(`includes`, `startsWith`, `len`, `isinstance`, `bcrypt.compare`) return a boolean or a number, not
the data. General-purpose hashing is not a sanitiser: a hashed email address is pseudonymised
data, which is still personal data (GDPR Recital 26). If your assessment says a construction is
anonymisation, declare it in `.privacy-flow.yml` with a citation to that assessment.

## Propagators (`catalogue/propagators.yml`)

A call the catalogue does not know is assumed to pass its arguments to its result, and personal
data reaching it is a [coverage gap](rules/PFC01.md), because it might be a sink. Propagators are
calls known not to be sinks:

```yaml
- id: js.databases
  language: javascript
  calls: ["{@prisma/client,drizzle-orm,mongoose,knex,pg,redis}:**"]
  receivers: [prisma, tx, db]   # any method on a receiver with these names and unknown origin
  flow: none                    # args_to_result | args_only | args_to_receiver | fluent |
                                # receiver_to_callback | callback_result | none
```

`args_only` passes the arguments but not the receiver: a call on a service client returns the
service's data, not the client's configuration (the API key it was built with). `args_to_result` is the default for string building, validation, formatting and framework
plumbing; `args_to_receiver` puts arguments into the receiver (`xs.push(x)`);
`fluent` does that and returns the receiver, so a chained call adds to it too
(`sb.Append(a).Append(b)`);
`receiver_to_callback` hands the receiver's elements to a callback's first parameter (`xs.map(f)`);
`callback_result` makes each callback argument's return value the result (`Task.Run(() => …)`);
`none` passes nothing (database clients: writing to the system's own store is data at rest, and a
query's result is the stored record, not its filter values).

Per language, `non_propagating_fields` lists field reads that are not the data (`length`, `Count`);
`value_methods` lists methods of plain values (strings) that do not make a value a record
(`email.toLowerCase()` keeps `email` an email address); `accessor_fields` lists fields that read
what an object holds rather than an attribute (`claim.Value`), which are not classified by name;
and `metadata_types` lists types whose values describe code (`System.Type`), whose fields are not
personal data.

`browser_storage` and `device_storage` (a native app's storage on the phone: .NET MAUI
`SecureStorage` and `Preferences`) are not external: their flows are reported, and no rule fires
on them.

## Presentation (`catalogue/presentation.yml`)

Calls that only present data in the app's own interface: a dialog, a navigation, a control's
bindable property, an animation ([ADR 0023](adr/0023-gap-impact.md)). A coverage gap at one is
still a gap, with impact `low`, and its PFC01 finding is reported at `info` at most.

```yaml
- id: cs.maui.dialogs
  language: csharp
  calls: ["{Microsoft.Maui.Controls.,}{Shell,Page}{,.Current}.{DisplayAlert,DisplayActionSheet}"]
  why: "shows a dialog on the device"     # the gap's impact reason quotes it
  citation: "Display pop-ups, https://learn.microsoft.com/dotnet/maui/user-interface/pop-ups"
```

The globs are matched against the gap's detail, the call's API path.

## Frameworks (`catalogue/frameworks.yml`)

Each web framework, by the modules that identify it, and whether its request input is modelled.
Importing an unsupported one is a coverage gap ([PFC01](rules/PFC01.md)), and calls into its
modules fold into that one gap. C# imports are global and implicit, so a framework also lists
`types`: a class that extends one of them, or carries one as an attribute, uses the framework
(a SignalR `Hub`).

## Dispatchers (`catalogue/dispatchers.yml`)

A library call that hands its argument to the project's own handlers ([ADR 0017](adr/0017-dispatch.md)):

```yaml
- id: cs.mediatr.request
  language: csharp
  calls: ["MediatR.{IMediator,ISender,Mediator}{,()}**.{Send,CreateStream}"]
  arg: 0                       # the positional argument dispatched
  targets:
    - implements: "MediatR.{IRequestHandler,IStreamRequestHandler}"   # classes implementing this
      method: Handle           # receive the argument in this method
      param: 0                 # as this parameter
      type_arg: 0              # when the value's class is this type argument (or extends it)
```

The argument is bound to each handler's parameter as a direct call would bind it, and the
handlers' return values are the call's result. A handler generic over that type argument (a
pipeline behavior) receives every value; a value of unknown class reaches every handler. A
project declares its own bus the same way in `.privacy-flow.yml`; `calls` may then name its own
method by its class's qualified name (`Shop.Events.Bus.Publish`).

## The data map's catalogue (`catalogue/topology/`)

What [`piiflow datamap`](datamap.md) needs besides flows is data too, versioned and digested apart
from the flow catalogue. `scan` uses `systems.yml` and `clients.yml` to say where a request
through a registered client goes ([ADR 0022](adr/0022-client-destinations.md)), so the flow
catalogue's digest covers those two as well:

| File | What it says |
| --- | --- |
| [`systems.yml`](../catalogue/topology/systems.yml) | What a deployable unit is (C# SDKs and output types, packages with a server framework or a `start` script, Django projects, Python app objects, Dockerfiles), what never is (app hosts, test projects), and orchestrator calls that name the projects they run (.NET Aspire) |
| [`clients.yml`](../catalogue/topology/clients.yml) | Client registrations and where their base address is (typed `AddHttpClient<T>`, named `AddHttpClient("name")` with `CreateClient("name")`, `axios.create`, `ky`, `httpx`, `aiohttp`); configuration reads a base address may come from (service discovery keys, connection strings); and orchestrator calls that set a configuration key to another resource's endpoint (Aspire `WithEnvironment`) |
| [`stores.yml`](../catalogue/topology/stores.yml) | How ORM models declare what is stored: Entity Framework Core, Django, SQLAlchemy, Prisma |
| [`purposes.yml`](../catalogue/topology/purposes.yml) | The data use proposed per sink class, processor and kind of evidence, with the reason; proposals, never decisions |

Every entry has a citation; every proposed data use and subject must be a key of the vendored
taxonomy, which loading checks.

## How a call is decided

For each call, in this order:

1. A local function or class: followed through its summary (or treated as a sanitiser if a
   sanitiser's `functions` names it, or a sink if a sink's `decorated` names its attribute); a
   dispatcher's handlers likewise.
2. Each API path of the callee: a **sink** (a hit), a **read source**, a **sanitiser**, or a
   **propagator**; then the receiver-name heuristic sinks; then, for a method on a JavaScript global
   the catalogue does not name, the plain-value methods.
3. A method on a value of unknown origin: heuristic sinks by receiver name, sanitiser and
   propagator methods by name, propagator receivers by name.
4. Otherwise, if nothing above explained the call: personal data reaching it is a coverage gap
   (`unresolved_callee`, `unresolved_method`, `unresolved_import` or `dynamic_call`), unless the
   path belongs to an unsupported framework, whose gap already covers it.

## Proposing an entry

See [CONTRIBUTING](../CONTRIBUTING.md#adding-an-sdk): one entry, a fixture that fails without
it, the vendor's documentation as the citation, and `piiflow rules` to check it loads.
