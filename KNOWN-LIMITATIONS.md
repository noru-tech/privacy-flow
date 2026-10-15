# Known limitations

What `piiflow` does not do, what it cannot see, and where its evidence stops. Each item links to
the document that describes it in detail. Planned work is on the [roadmap](ROADMAP.md). Where a
limitation can hide a flow, the analysis reports a [coverage gap](docs/rules/PFC01.md) wherever it
can tell; the items marked **silent** below are the ones it cannot.

## What `piiflow` deliberately does not do

- **It does not find security vulnerabilities** (injection, SSRF, deserialisation). It is a
  privacy flow tool.
- **It does not read data.** It analyses code, not data at rest or in databases, and never sees a
  value; categories come from names, types, frameworks and data maps.
- **It does not observe the running system.** No runtime or dynamic analysis.
- **It does not decide purposes, legal bases or data subjects**, and does not produce Article 30
  records. Those are human (or Noru) judgements ([open/closed line](NOTES.md)). The
  [data map](docs/datamap.md) proposes data uses and subjects for review, and never a legal
  basis.
- **It calls no model and makes no network call**, and sends no telemetry. It can write a review
  queue (`--format review`) for a human or an agent to work through; it never reads answers back
  from it.
- **It does not certify compliance.** A clean result is a statement about the scanned scope, with
  its gaps listed. The [control mapping](docs/control-mapping.md) is pending review by a privacy
  lawyer or Noru's compliance lead.

## Languages

- **Supported:** TypeScript (`.ts`, `.tsx`, `.mts`, `.cts`), JavaScript (`.js`, `.jsx`, `.mjs`,
  `.cjs`), Python (`.py`), and C# (`.cs`, and Blazor components in `.razor`).
- **Not analysed, reported as `unsupported_language` gaps:** Go, Java, Kotlin, Scala, Ruby, PHP,
  F#, Rust, Swift, Objective-C, Elixir, Erlang, Clojure, Dart, the script blocks of Vue, Svelte and
  Astro single-file components, and Razor views and pages (`.cshtml`). Go and Java/Kotlin are next
  on the [roadmap](ROADMAP.md).
- **Silent:** files in other languages (C, C++, shell, SQL, Jupyter notebooks) are ignored without
  a gap; Python stub files (`.pyi`) and TypeScript declaration files (`.d.ts`) are not code.
- **Parsing:** tree-sitter-typescript 0.23 rejects a bare `&` in JSX text (`<p>Terms & conditions</p>`),
  and tree-sitter-c-sharp 0.23 a `#if` inside an expression or a base list. The file is analysed as
  far as it parses and reported as a `parse_error` gap; so is any file with a syntax error. Files
  over 2 MiB are skipped and reported.
- **C# defaults:** build output (`bin/`, `obj/`), designer and generated files (`*.Designer.cs`,
  `*.g.cs`), EF Core migrations and test projects (`*.Tests`, `*.UnitTests`, `*.FunctionalTests`,
  `*.IntegrationTests`) are excluded by default.

## Frameworks

Request input is a source only in frameworks whose request objects are modelled
([`catalogue/sources.yml`](catalogue/sources.yml)):

- **Supported:** Express (handler `req.body`, `query`, `params`, `cookies`, `headers`, `ip`),
  Fastify, Next.js App Router route handlers and Pages Router API routes, `next/headers`, FastAPI
  (Pydantic-typed bodies are narrowed to their classified fields), Flask (`flask.request`),
  Django and Django REST framework views in `views` modules (the `request` parameter), ASP.NET Core
  minimal APIs (`Map*` handlers; services, `[FromServices]`, DbContexts, the request context and
  interfaces are injected, not input; `[AsParameters]` binds each property; a `Guid` or a number is
  input only when its name says what it is), ASP.NET Core MVC controller actions, gRPC service
  methods (overrides taking a `ServerCallContext`), `HttpRequest` reads, claims
  (`FindFirst("sub")` is classified by the claim type), and Blazor components (`@bind`,
  `[SupplyParameterFromForm]`, `[SupplyParameterFromQuery]`, `[Parameter]` on routable pages).
- **Not supported, reported as `unsupported_framework` gaps** (calls into them fold into that gap):
  Koa, Hapi, NestJS (`@Body()` and friends), Hono, Elysia, Remix, SvelteKit, Nuxt and h3, Restify,
  AdonisJS, tRPC, ts-rest servers, GraphQL servers (Apollo, Yoga, Mercurius, TypeGraphQL, Graphene,
  Strawberry, Ariadne, HotChocolate, GraphQL.NET), Slack Bolt, aiohttp servers, Tornado, Sanic,
  Falcon, Pyramid, Bottle, Quart, Litestar, Starlette used directly, SignalR hubs, Azure Functions,
  Orleans grains, Razor Pages (`PageModel`), FastEndpoints, CoreWCF and ASP.NET on .NET Framework
  ([`catalogue/frameworks.yml`](catalogue/frameworks.yml)). A C# class is in a framework when it
  extends one of the framework's types or carries its attribute, since its imports are global and
  implicit.
- **Next.js server actions** (`'use server'`) are reported as `unsupported_construct` gaps: their
  arguments come from the client and are not modelled as request input.
- **Silent:** other kinds of input are not sources unless a field name says so: WebSocket messages,
  queue and job payloads, CLI arguments, environment variables, files read from disk, and
  responses from other services.
- **Silent:** Django views outside a `views` module, and Express or Fastify handlers registered
  through a wrapper the analysis cannot resolve to the framework, are not recognised as handlers.

## Analysis

- **Flow-insensitive within a function.** A variable overwritten before it reaches a sink still
  flows (`let v = user.email; v = 'x'; log(v)` is reported). Two conformance vectors record this as
  a known limitation.
- **Field sensitivity.** Within a function, through module-level objects and closures, out of
  calls and into them, every literal is an allocation site whose fields stay apart, and writes
  through an alias are seen (`p = o; p.a = email; log(o.a)`): `r = wrap(); log(r.data.id)` and
  `show({ owner: { id, email } })` read as `input.owner.id` are not reported
  ([ADR 0007](docs/adr/0007-allocation-sites.md), [ADR 0009](docs/adr/0009-call-context-sites.md),
  [ADR 0010](docs/adr/0010-formal-sites.md)). An object keeps one level of fields when it comes
  from an import, from a call that may also reach a library, through a rest parameter or a spread
  argument (`f(...xs)`), into a parameter whose name is classified as personal data, or more than
  three fields below a parameter. Instance fields keep two levels
  (`this.config.url` keeps `config`'s fields apart) ([ADR 0005](docs/adr/0005-instances-and-fields.md)).
  **Silent:** an object passed to a call that writes to it (`f(o)` where `f` sets `p.a = email`)
  does not carry that write back to the caller's reads of `o`. Array elements are not distinguished from
  each other, and a computed key (`obj[key]`) may read any field.
- **Instances:** each `new C(...)` carries what its constructor stored, through the constructor's
  summary. **Silent:** a method that mutates an instance after construction writes to the class's
  shared field variables, which an instance logged as a whole does not carry when the class has a
  constructor (`c = new C(); c.setEmail(e); log(c)` is missed).
- **Calls into the project's own code** are followed through summaries, context-sensitively for
  parameters. Calls through function values the analysis cannot resolve (a callback parameter with
  no resolvable caller, `handlers[name](x)`) are `dynamic_call` gaps when personal data is passed.
- **Inheritance** ([ADR 0011](docs/adr/0011-inheritance.md)): inherited methods, constructors
  and fields, `super`, and members of external base classes resolve, and a call on a value typed
  with a base class reaches every override. A template method (a base method that calls, on
  `this`, a method subclasses override) is analysed once per subclass that inherits it
  ([ADR 0013](docs/adr/0013-per-subclass-methods.md)), so the call reaches that subclass's own
  override; past a budget of 200,000 copied statements per scan, the call is a `dynamic_call`
  gap. **Silent:** a value a subclass stores in an inherited field is not seen by
  base class methods. A base class built by a function call (a mixin) is not resolved.
- **TypeScript interfaces** ([ADR 0012](docs/adr/0012-interfaces.md)): a call on a value typed
  with an interface reaches every local class that declares `implements` (directly, through an
  extending interface, or through a base class). Classes that match an interface structurally
  without `implements`, object literals typed with an interface, and object type aliases are not
  implementations; a call on such a value stays a gap when personal data reaches it. Reading a property or getter gives what it
  returns; setters are field writes.
- **Exceptions** ([ADR 0014](docs/adr/0014-exceptions.md)): a value thrown reaches the catch
  parameter, across calls; the error of a library call inside a `try` may carry its arguments,
  reported for review. Exception types are not distinguished (any `except` catches anything
  raised in its `try`), `finally` blocks are not followed, and a bare re-raise (`raise`) is not.
- **Libraries:** a call into a module the catalogue does not know is assumed to pass its arguments
  to its result, and personal data reaching it is an `unresolved_callee` gap. **Silent:** callbacks
  receive no data from the arguments of whatever later invokes them through an event system
  (`emitter.on('x', d => log(d))` after `emitter.emit('x', email)` is missed).
- **Containers:** a container obtained from another container (`subscribers.get(k).add(cb)`) has
  no location of its own, so what is added to it is not seen by a later read from the outer
  container, and calls on those values may not resolve ([ADR 0008](docs/adr/0008-container-identity.md)).
- **Provenance** (which function or API a call reaches) is flow- and context-insensitive and
  bounded: API paths stop growing at 16 segments, at most 16 API paths are kept per variable, at
  most 4,096 provenance values of any kind, and the fixpoint stops after 64 rounds (18 were needed
  for a 220k-line monorepo) ([ADR 0003](docs/adr/0003-provenance-and-resolution.md)).
- **Depth bound:** a witness path may cross at most `max_call_depth` call boundaries (default 32,
  at most 64), counting each entry into a callee and each return out of one. Where the bound cut a
  witness to something not reached any other way, a `depth_bound` gap is reported at the call site.
- **Dynamic code:** `eval`, `new Function` and dynamic `import()` with a computed specifier are not
  followed; personal data passed to them is a gap. `getattr` and `obj[key]` with a computed name read
  any field. **Silent:** monkey-patching (replacing a method at run time) is not seen.
- **TypeScript:** type assertions (`req.body as Signup`) do not narrow request input; types are read
  from annotations by name (interfaces, type aliases, classes), not checked.
- **Python:** `**kwargs` binds every unmatched keyword argument; metaclasses, descriptors and
  `__getattr__` are not modelled; the results of ORM queries are not typed (their fields are
  classified by name when read).
- **C#** ([ADR 0016](docs/adr/0016-csharp.md)): names resolve against the whole project, with
  global, project-file and implicit usings, partial classes and extension methods; there is no
  assembly metadata.
  - A type outside the project is known by name only. One the catalogue spells is qualified
    exactly; one only a catalogue wildcard covers (`Microsoft.EntityFrameworkCore.**`) is
    qualified by the first such `using` in scope, which may be the wrong namespace when several
    `using`s are covered by wildcards; one no `using` explains stays bare, and a call on it is
    an `unresolved_callee` gap. The return types of external calls are not known: their paths
    are `T.Method()`, matched by the catalogue's globs.
  - Every variable declared with a type that declares classified fields is a source of those
    fields (`Customer customer` with an `Email` property), as for TypeScript annotations; C#
    declares types almost everywhere, so typed seeds are common.
  - Overloads share a name: a call reaches every overload of the nearest class that defines it.
    `out` and `ref` arguments receive the call's result, so a method's return value carries what
    it writes to them (conformance vector `cs-out-param`).
  - **Silent:** a getter's body is followed for what it is, not for the data it reads
    (`public string Label => "card of " + Holder;` read as `card.Label` does not carry `Holder`;
    vector `cs-property-getter`); code that a source generator writes (other than `[LoggerMessage]`
    methods, which are log sinks, and gRPC message classes, which propagate) is not seen; `dynamic`
    is an object of unknown type.
  - **Dispatch** ([ADR 0017](docs/adr/0017-dispatch.md)): MediatR's `Send` and `Publish` reach
    the project's handlers and pipeline behaviors for the value's type; a value of unknown class
    reaches every handler. Other mediators and buses are followed only when a project declares
    them under `dispatchers`; an interface call on the project's own bus reaches its
    implementations as any interface call does. A message published to a broker leaves the
    process: its consumers are not reached from the producer.
  - Razor components are translated into C# for the analysis; markup expressions are rendered
    into the application's own page, which is not a sink. Child content templates
    (`RenderFragment` lambdas with `context`) read `context` as an unknown member.

## Sources and sinks

- **Values named like data:** a parameter or field read named `email` is an email address
  unless it is used as a record or a class (named fields read, non-string methods called, here or
  in a function it is passed to). **Silent:** the attribute inside such a record under a generic
  name (`channel.phone.value`) is not classified by the record's name.
- **Ghost's logger** (`@tryghost/logging`) is not a sink yet: on Ghost it reaches about 520
  medium findings, most of the sampled ones through long, imprecise chains. **Silent:** what
  Ghost logs through it.
- **Sources are names.** A field, dict key or parameter is personal data when the
  [classification table](vendor/classification/classification.json), a catalogue or config entry,
  or an ingested data map says so; names the table marks only *maybe* personal are reported at
  `info` for review. **Silent:** personal data held under a name the table does not know (`foo`)
  is not a source until it is declared under `fields`; a local variable named `email` assigned from
  an unknown call is not a source.
- **Identifiers** ([ADR 0015](docs/adr/0015-identifiers.md)): `user_id`, `customer_id` and the
  like, and `id` on a person's record, are pseudonymous identifiers. A flow of one to a log is
  reported but is not a PF001 finding. **Silent:** identifiers under other names (`sub`, an
  `owner` field holding an ID) are not sources unless declared under `fields`.
- **Context-dependent names:** `state` and `province` are an address's only on an object named
  like an address or beside other address fields
  ([`catalogue/classification.yml`](catalogue/classification.yml)). **Silent:** an address state
  read from an object named otherwise (`order.tax_breakdown[0]["state"]`) is not a source unless
  the project declares `state` under `fields`.
- **Data maps** apply by field name, and to typed objects by collection name (`User` ↔ `users`).
- **Sinks are the catalogue's** ([`catalogue/sinks.yml`](catalogue/sinks.yml)). An SDK it does
  not list is a gap, not a sink. Loggers injected without a type are recognised by receiver name
  (`logger`, `log`) and marked `heuristic`.
- **Mail without a named provider.** SMTP clients (`smtplib`, Nodemailer, Django's and Flask's
  mail, the NestJS mailer) send to whatever server is configured at run time, so their flows name
  no processor and no rule fires on them; they are reported as flows and egress facts.
- **A sink's result carries nothing.** **Silent:** a send call's response that echoes the
  recipient (an SMTP rejection that names the address) is not personal data when it is logged.
- **Outbound HTTP:** a host is known when the URL is a literal, a template or concatenation that
  starts with one, or a constant holding one. In `scan`, a client's base URL (axios instances,
  `httpx.Client`, typed `HttpClient`s) is not followed, so calls on such clients have a dynamic
  host (PF006); [`datamap`](docs/datamap.md) binds them to their registration.
- **Browser storage** (`localStorage`, `sessionStorage`, `document.cookie`, js-cookie) is reported
  as flows, not egress; no rule fires on it in v0.1. `window.parent.postMessage` and
  `window.open` are gaps, not sinks.
- **Not sinks:** writing to the system's own databases, caches, queues on its own infrastructure,
  and files; rendering data into the application's own HTML or JSON responses. What a system
  stores is described by its ORM models in the [data map](docs/datamap.md), not by flows.
- **Sanitisers:** general-purpose hashing is not one by default (pseudonymised data is personal
  data, [ADR 0006](docs/adr/0006-sanitisers-and-hashing.md)); a sanitiser removes categories, it
  does not transform them.

## Output and workflow

- **Dirty work trees** are scanned as they are and named by their HEAD commit; `subject.dirty`
  says so. `git ls-files` and a directory walk can disagree about untracked files; the method is
  recorded.
- **`diff`** analyses each revision with its own `.privacy-flow.yml`, needs both commits in the
  local repository (`fetch-depth: 0` in CI), and identifies findings by anchors without line
  numbers: renaming a function or changing the logged expression makes a finding new.
- **Finding IDs** survive edits elsewhere in a file, not edits to the flow's own source or sink
  expression; a disposition on a changed finding is reported as dropped by the next `scan`.
- **Outputs contain code excerpts** (at most 80 characters per hop), which can include whatever the
  code contains, such as a hard-coded address in a comment.
- **Not yet done:** the labels for Polar and PrivateGPT and the Zenodo DOI of the labelled
  benchmark (milestone M6, [benchmark](benchmark/README.md)); the review of the control mapping.

## The data map

[`piiflow datamap`](docs/datamap.md) ([ADR 0018](docs/adr/0018-systems-and-datasets.md),
[ADR 0019](docs/adr/0019-datamap-merge.md)):

- **Systems** are deployable units of one repository. Topology across repositories is not
  joined. The program is analysed as one, so a flow that crosses two services' own code (through
  dependency injection or an interface) is reported as a note, not an edge. A library's flow
  belongs to every system that links the library, used or not.
- **Edges between systems** are resolved through client registrations whose base address is a
  literal, and names an orchestrator (.NET Aspire), docker-compose or `systems: hosts:` gives.
  Not resolved:
  - gRPC clients and named `HttpClient`s;
  - Aspire environment URLs read from configuration;
  - message buses (an integration event's publisher and its consumers);
  - a slug match between a service name and a project name.
- **Datasets** come from Entity Framework Core, Django, SQLAlchemy and Prisma models.
  - Not read: TypeORM, SQLModel, Mongoose, Sequelize, Drizzle, raw SQL, migrations, and stores
    without models (Redis).
  - EF configuration is followed as far as `ApplyConfiguration`, `Entity<T>()` and local
    methods to depth 3. Value converters, TPH inheritance and configuration passed around are
    not modelled.
  - A type the catalogue does not spell is matched by its name (sets, configurations,
    attributes), since its namespace may be guessed wrong.
- **Proposals** are coarse: one data use per sink class or processor, `customer` as the data
  subject, nothing for logs. Every one needs review, and no legal basis is proposed.
- **Merging:**
  - hand-written comments are not kept (use `description`);
  - YAML anchors are expanded;
  - a renamed declaration is recognised only while its evidence is unchanged;
  - without the lock, nothing in the file is updated.
- **Feedback:** a map piiflow maintains is not read back as classification input; use `fields:`.
