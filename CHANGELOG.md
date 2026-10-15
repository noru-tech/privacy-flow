# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `piiflow datamap`: a full Fides data map of the repository (`.fides/datamap.yml`), maintained
  in place ([docs](docs/datamap.md), ADRs 0018 and 0019).
  - **Systems:** one per deployable service: C# web, worker and executable projects; packages with
    a server or app framework; Django and Python app objects; Dockerfiles. Or as declared under
    the new `systems:` key of `.privacy-flow.yml`.
  - **Flows between systems:** requests through registered clients (typed `AddHttpClient<T>`,
    `axios.create`, `httpx`, `aiohttp`) resolve to the system their base address names, by
    .NET Aspire project names, docker-compose services and `systems: hosts:`.
  - **Flows to processors and unnamed destinations,** and each system's input from users.
  - **Datasets:** from Entity Framework Core, Django, SQLAlchemy and Prisma models, with fields
    classified like flows.
  - **Privacy declarations, proposed:** a data use per group of evidence, from
    `catalogue/topology/purposes.yml`, always listed for review, never with a legal basis.
  - **Re-runs merge into the file** over the last generation (`.fides/datamap.lock.json`):
    edits, deletions and additions stay, untouched values follow the code.
  - **privacy-datamap:** decisions in its manifest win.
  - **CI:** `--check` exits 5, a new exit code, when the map is out of date. The GitHub Action
    gains `datamap-check`.
- On dotnet/eShop: 11 services, 4 datasets (with the order's card number and address), and the
  web app's request carrying them resolved to the ordering API. The smoke test checks them.
- The flow-facts document can carry a `topology` section; only `datamap` writes it. `scan` and
  `diff` output is unchanged.

## [0.3.1] - 2026-10-14

The Fides fragment lists every destination personal data reaches, including those the code
does not name, and declared processors are named by their declaration. The analysis is
unchanged: for the same input, 0.3.1 reports the same flows and findings as 0.3.0.

### Changed

- Fides output: egress to a destination the code does not name (a host computed at run time, a
  model provider chosen by configuration) is a `DataFlow` with a placeholder key,
  `unresolved_<sink class>_<file>`, per sending file, with its categories and call sites. It was
  a closing comment, so a scan without `processors:` in `.privacy-flow.yml` wrote `egress: []`
  for services such as eShop.

### Fixed

- A processor declared by sink ID or host is named by its declaration in `egress` and the Fides
  output. A declaration matched by `sinks:` for a sink the catalogue gives no processor
  (`Microsoft.Extensions.AI`, the Vercel AI SDK) was labelled `?`, and without a `fides_key` it
  was left out of the Fides `egress`. A processor declared by `hosts:` is keyed by its name
  (`acme_crm`), not the host it matched.

## [0.3.0] - 2026-10-14

C# is the third language: ASP.NET Core services, Blazor components and the libraries around
them, analysed the same way as TypeScript and Python, offline. TypeScript and Python output is
unchanged apart from the catalogue digest.

### Added
- C# (`.cs`) and Blazor components (`.razor`), ADR 0016. Names resolve against the whole
  project: namespaces, `using` directives (global, static, aliases, `<Using>` items in the
  project file and the SDK's implicit usings from `ImplicitUsings`), nested and partial types,
  members inherited from base classes in other files, primary constructors, records,
  properties, attributes, extension methods, overloads, `out`/`ref`, patterns, LINQ query
  syntax and exceptions. External types are qualified by the namespaces the catalogue spells.
  Razor components are translated into C# with positions kept in the component.
- C# sources: ASP.NET Core minimal API handlers (services, `[FromServices]`, DbContexts, the
  request context and interfaces are injected; `[AsParameters]` binds each property;
  `[FromBody]` and friends are input), MVC controller actions, gRPC service methods,
  `HttpRequest` reads, the remote IP, claims (classified by the claim type: `sub` is a
  pseudonymous identifier), and Blazor `@bind`, `[SupplyParameterFromForm]`,
  `[SupplyParameterFromQuery]` and routable `[Parameter]`s.
- C# sinks: Microsoft.Extensions.Logging (including `[LoggerMessage]` methods), the console,
  `Debug`/`Trace`, Serilog, NLog, log4net, OpenTelemetry activity tags, Duende IdentityServer
  events, `HttpClient` (with the host of a request message), RestSharp, Refit, Sentry,
  Application Insights, Segment, Mixpanel, PostHog, RabbitMQ, MassTransit, Azure Service Bus,
  Event Hubs and Queues, Kafka, Amazon SNS, SQS and SES, SendGrid, MailKit, `SmtpClient`,
  Twilio, Microsoft.Extensions.AI, OpenAI, Azure OpenAI and Anthropic. Sanitisers for password
  hashing and predicates; propagators for the base class library, LINQ, `StringBuilder`,
  message builders, databases (EF Core, ASP.NET Identity, Redis, Dapper …) and framework
  plumbing.
- Dispatchers (ADR 0017, `catalogue/dispatchers.yml`): MediatR's `Send` and `Publish` reach the
  project's request and notification handlers and pipeline behaviors for the value's type. A
  project can declare its own bus under `dispatchers` in `.privacy-flow.yml`.
- Frameworks reported as unsupported for C#: SignalR, Azure Functions, Orleans, Razor Pages,
  FastEndpoints, HotChocolate and GraphQL.NET, CoreWCF, ASP.NET on .NET Framework; `.cshtml`
  files are an `unsupported_language` gap. A C# class uses a framework when it extends one of
  its types or carries its attribute.
- Catalogue fields: `decorated` sinks, `host.built_arg` and `host.wrappers`, `bases`,
  `class_decorators`, `members` and `has_param_type` for handlers, `kind: property` sources,
  `arg_category` for reads, `skip_bases`, `skip_interfaces`, `skip_attrs`, `expand_attrs`,
  `classify_only_types` and `by_attr` for parameters, flows `fluent` and `callback_result`,
  framework `types`, and per-language `value_methods`, `accessor_fields` and `metadata_types`.
  `skip_types` takes globs.
- Fixtures `cs/*` for every rule and the C# features; conformance vectors `cs-*`; a `dispatch`
  break in the conformance CI job; fuzz inputs for C#, Razor and project files; a C# variant of
  the synthetic benchmark in the performance budget; and a smoke test on dotnet/eShop at a
  pinned commit (`.github/scripts/smoke_eshop.py`, `tests/smoke/eshop.expected.json`), run in CI
  on changes to the analysis or the catalogue.

### Changed
- Language-specific branches outside the lowerings are a table (`LangTraits`); constructors are
  found by a flag rather than by name.

## [0.2.0] - 2026-10-03

What `piiflow` sees grows: email, messaging and push SDKs, inheritance and interfaces, exceptions,
CommonJS modules, and identifiers of people. Expect more flows and findings than 0.1.1 on the same
code, mostly PF002 for identifiers and emails sent to processors not yet declared, and fewer
maybe-personal findings where a name like `state` or `email` turned out to be something else.
`piiflow scan` reports dispositions it could not carry over to a changed finding, as before.

### Added
- Sinks: Stripe's billing portal sessions (JavaScript and Python), and the Polar (polar.sh) SDK
  (`@polar-sh/sdk`, `polar_sdk`).
- Identifiers (ADR 0015): `user_id`, `customer_id`, `member_id` and the like, and `id` on a
  person's record (`user.id`, `customer["id"]`, `User.id`), are `user.unique_id.pseudonymous`
  sources. They count as personal data for PF002 to PF006; an identifier alone reaching a log is
  reported as a flow but is not a PF001 finding. On Polar, 252 new PF002 findings (identifiers to
  undeclared processors).
- Exceptions (ADR 0014): a value thrown reaches the `catch`/`except` parameter, across calls and
  out of callbacks handed to libraries, and the error of a library call inside a `try` may carry
  the call's arguments (hop kind `error`, reported at `info` for review). Hop kinds `throw` and
  `error` are new in the flow facts.
- Registries: `PROVIDERS.get(kind)` on a dictionary literal is any of its entries (as
  `PROVIDERS[kind]` already was), and `PROVIDERS.get("smtp")` that entry, so a provider class
  picked from a table resolves. On CTFd, every email to its SMTP and Mailgun providers.
- Email, messaging and push sinks: Python `smtplib`, Flask-Mail, fastapi-mail, `emails`, Apprise,
  Resend, Mailjet, Brevo, Mailchimp Transactional, Vonage, python-telegram-bot, Amazon SNS,
  Firebase Cloud Messaging, Expo and Web Push; JavaScript `@nestjs-modules/mailer`, Wasp's
  `emailSender`, `@tryghost/nodemailer`, Amazon SNS, MailerSend, Mailchimp Transactional, Brevo,
  Mailjet, Vonage, Telegram (node-telegram-bot-api, Telegraf), Firebase Cloud Messaging, Expo,
  Web Push, Novu, Knock, Customer.io and Loops. Fixtures `py/messaging` and `ts/messaging`.
- Sinks take `receiver: true` when the object a method is called on is sent with it, and the
  message builders of the standard library's `email` package, Flask-Mail, `emails` and Apprise
  put their arguments into the message.
- Inheritance (ADR 0011): methods, constructors and fields inherited from local base classes,
  `super.m()` and `super().m()`, and members of external base classes
  (`class Analytics extends PostHog`) resolve. A call on a value typed with a base class (a
  parameter, a field, a property) reaches every subclass's override; a call on `this` follows the
  class's own method and is a `dynamic_call` gap where subclasses override it. Reading a Python
  property or a JavaScript getter gives what it returns. On Polar, `dynamic_call` gaps go from
  227 to 64; on Healthchecks, coverage gaps from 58 to 28.
- Template methods (ADR 0013): a base class method that calls, on `this`, a method subclasses
  override is analysed per subclass, so `this.deliver()` in `Notifier.notify` reaches each
  subclass's own `deliver` without one subclass's data reaching another's. These calls are no
  longer `dynamic_call` gaps.
- TypeScript interfaces (ADR 0012): a call on a value typed with an interface reaches the local
  classes that implement it, directly, through an extending interface or through a base class.
  Interfaces are open: an external module the value may also come from stays a gap.
- Fuzzing: cargo-fuzz targets for lowering, the whole pipeline (with a determinism check),
  configuration and data maps, run by ClusterFuzzLite on pull requests and weekly.

### Changed
- `state` and `province` are classified as an address's state only in an address context: on an
  object named like an address (`billing_address.state`), on a type or class with other address
  fields, or beside address parameters (`catalogue/classification.yml`). Elsewhere they are
  application state and not sources. On PrivateGPT, a chat engine's `context.state` made about
  half of all flows.
- Objects passed into calls keep their fields apart inside the callee, per call site:
  `show({ owner: { id, email } })` read as `input.owner.id` no longer reports the email address
  (ADR 0010).
- Objects returned from calls keep their fields apart in the caller, per call site: a helper's
  `return { ...record, user: { id, email } }` read as `r.user.id` no longer reports the email
  address. Spreads into a literal are its rest, not its identity. On documenso, medium PF001
  findings go from 23 to 14 (ADR 0009).
- Plain objects are allocation sites: within a function, and through module-level objects and
  closures, nested fields stay apart (`o = { a: { email, id } }; log(o.a.id)` is no longer
  reported), and a field written through one variable is seen through another that holds the
  same object (ADR 0007).

### Fixed
- `require('./m')` of a CommonJS module is what the module assigned to `module.exports` (a class,
  an object of exports), not only the module: `new (require('./mailer'))()` and
  `const { GhostMailer } = require('../mail')` resolve. On Ghost, whose server is CommonJS,
  unresolved-method gaps go from 396 to 156 and flows from 115 to 874; time 1.6 s to 4.0 s.
- A call on a service client (Stripe, AWS, Slack …) that is not a sink no longer returns the
  client's own configuration: `stripe.subscriptions.retrieve(id)` does not carry the secret key
  the client was built with (new propagator flow `args_only`).
- A value named like a personal attribute that is used as a record or a class (its named fields
  read, methods other than a string's called on it, here or in a function it is passed to) is
  not that attribute: a parameter or a field read named `email` that holds Ghost's Email model
  is not a source by its name. On PrivateGPT, 233 maybe-personal findings go; on Polar, 118.
- A flow of unknown category was dropped when a classified field read lay on its way, whatever
  its source; this "narrowing" now applies to request input only, as documented, so a
  maybe-personal field logged beside a classified one is no longer hidden.
- A call whose callee has two API paths that match the same sink (an aliased import and its
  original name) made two sinks at one place, and every flow into it was reported twice. The same
  anchor at the same place is now one source or sink. On Polar, 4,077 duplicate flows go.
- A Django `EmailMessage` (or `EmailMultiAlternatives`) built with personal data and sent with
  `send()` reported nothing: the data is in the message, and `send()` takes no arguments. On
  Healthchecks, 86 flows of addresses, names and phone numbers into its outgoing mail were missed.
- Every `new Map()` (and `Set`, `WeakMap`, `WeakSet`) was one container for provenance, so a
  value cached in a Map, such as a Prisma client, also resolved as whatever any other Map held.
  Database query filters then reached query results. On documenso, findings go from 757 to 549.
  Containers in instance fields and loop variables over containers now resolve to what they hold
  (ADR 0008).
- The catalogue's built-in containers match their own methods only, not calls on their elements.

## [0.1.1] - 2026-10-02

No change to the analysis: for the same input, 0.1.1 reports the same flows, findings and gaps as
0.1.0, and only the version recorded in the outputs differs.

### Added
- A speed and memory benchmark of piiflow and Privado on the twelve applications of the benchmark
  corpus, run on the same GitHub-hosted runners, with its method and results in the repository and
  a summary in the README.
- The benchmark report (`benchmark/REPORT.md`), tied to the released binary.
- A logo, a social preview image, a feature request form and a Discussions link in the issue
  chooser.
- Releases are archived on Zenodo from this release on.

### Changed
- The control-mapping notes no longer say the review is due before the first release; the mapping
  is still pending review.

## [0.1.0] - 2026-10-02

### Added
- `piiflow`, a deterministic, offline static analyser of personal-data flows in TypeScript,
  JavaScript and Python, built on tree-sitter, with a shared IR, provenance-based call resolution
  across files, workspace packages and tsconfig aliases, function summaries, and a worklist engine
  that reconstructs one shortest cited path per (source, sink, category). A Datalog engine
  (ascent) computes the same reachability and is the test oracle.
- Rules PF001 to PF006 and PFC01, each with a documentation page, failing and passing fixtures in
  both languages, and a control mapping pending review.
- The catalogue as YAML: sinks for logs, error tracking, analytics, messaging, LLM providers,
  outbound HTTP, browser storage and other processors; Express, Fastify, Next.js, FastAPI, Flask
  and Django request input; default sanitisers and propagators; supported and unsupported
  frameworks. Projects extend it in `.privacy-flow.yml`, validated by a published schema.
- Commands `scan`, `diff`, `explain`, `check`, `validate`, `rules`, `doctor`, `completions`,
  `manpage`; outputs table, RFC 8785 JSON with a digest, SARIF 2.1.0 with code flows, Fides egress
  declarations, in-toto Statement v1, facts (JSON lines) and a Markdown review queue.
- Dispositions with `acc`'s semantics, carried across scans by line-independent finding IDs.
- Coverage gaps for unresolved calls, dynamic calls, unresolved imports, the depth bound,
  unsupported languages, frameworks and constructs, and parse errors; exit 4 whenever a clean
  result cannot be claimed.
- Ingestion of Fides data maps and privacy-datamap derived facts.
- A conformance corpus of 32 vectors with an external-runner contract, a composite GitHub Action,
  criterion benchmarks, a CI performance budget, and a release pipeline with artifact
  attestations, checksums and an SBOM.

[Unreleased]: https://github.com/noru-tech/privacy-flow/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/noru-tech/privacy-flow/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/noru-tech/privacy-flow/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/noru-tech/privacy-flow/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/noru-tech/privacy-flow/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/noru-tech/privacy-flow/releases/tag/v0.1.0
