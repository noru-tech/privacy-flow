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
  records. Those are human (or Noru) judgements ([open/closed line](NOTES.md)).
- **It calls no model and makes no network call**, and sends no telemetry. It can write a review
  queue (`--format review`) for a human or an agent to work through; it never reads answers back
  from it.
- **It does not certify compliance.** A clean result is a statement about the scanned scope, with
  its gaps listed. The [control mapping](docs/control-mapping.md) is pending review by a privacy
  lawyer or Noru's compliance lead.

## Languages

- **Supported:** TypeScript (`.ts`, `.tsx`, `.mts`, `.cts`), JavaScript (`.js`, `.jsx`, `.mjs`,
  `.cjs`) and Python (`.py`).
- **Not analysed, reported as `unsupported_language` gaps:** Go, Java, Kotlin, Scala, Ruby, PHP,
  C#, F#, Rust, Swift, Objective-C, Elixir, Erlang, Clojure, Dart, and the script blocks of Vue,
  Svelte and Astro single-file components. Go and Java/Kotlin are next on the [roadmap](ROADMAP.md).
- **Silent:** files in other languages (C, C++, shell, SQL, Jupyter notebooks) are ignored without
  a gap; Python stub files (`.pyi`) and TypeScript declaration files (`.d.ts`) are not code.
- **Parsing:** tree-sitter-typescript 0.23 rejects a bare `&` in JSX text (`<p>Terms & conditions</p>`).
  The file is analysed as far as it parses and reported as a `parse_error` gap; so is any file with
  a syntax error. Files over 2 MiB are skipped and reported.

## Frameworks

Request input is a source only in frameworks whose request objects are modelled
([`catalogue/sources.yml`](catalogue/sources.yml)):

- **Supported:** Express (handler `req.body`, `query`, `params`, `cookies`, `headers`, `ip`),
  Fastify, Next.js App Router route handlers and Pages Router API routes, `next/headers`, FastAPI
  (Pydantic-typed bodies are narrowed to their classified fields), Flask (`flask.request`),
  Django and Django REST framework views in `views` modules (the `request` parameter).
- **Not supported, reported as `unsupported_framework` gaps** (calls into them fold into that gap):
  Koa, Hapi, NestJS (`@Body()` and friends), Hono, Elysia, Remix, SvelteKit, Nuxt and h3, Restify,
  AdonisJS, tRPC, ts-rest servers, GraphQL servers (Apollo, Yoga, Mercurius, TypeGraphQL, Graphene,
  Strawberry, Ariadne), Slack Bolt, aiohttp servers, Tornado, Sanic, Falcon, Pyramid, Bottle,
  Quart, Litestar, and Starlette used directly ([`catalogue/frameworks.yml`](catalogue/frameworks.yml)).
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
- **Field sensitivity into calls is one level deep.** Within a function, through module-level
  objects and closures, and back out of calls, every literal is an allocation site whose fields
  stay apart at any depth, and writes through an alias are seen (`p = o; p.a = email; log(o.a)`):
  `r = wrap(); log(r.data.id)`, where `wrap` returns `{ data: { email, id } }`, is not reported
  ([ADR 0007](docs/adr/0007-allocation-sites.md), [ADR 0009](docs/adr/0009-call-context-sites.md)).
  An object passed *into* a call, or one that comes from an import or a call that may also reach a
  library, keeps one level of fields: `f({ user })` read as `input.user.id` inside `f` is reported. Instance fields keep two levels
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
- **Inheritance:** methods inherited from a base class, and `super.method()`, are not resolved;
  passing personal data to one is a `dynamic_call` gap. Getters and setters are field reads and
  writes, not calls.
- **Exceptions:** a value thrown is not connected to the `catch` parameter. **Silent:**
  `try { throw new Error(email) } catch (e) { log(e) }` is missed.
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

## Sources and sinks

- **Sources are names.** A field, dict key or parameter is personal data when the
  [classification table](vendor/classification/classification.json), a catalogue or config entry,
  or an ingested data map says so; names the table marks only *maybe* personal are reported at
  `info` for review. **Silent:** personal data held under a name the table does not know (`foo`)
  is not a source until it is declared under `fields`; a local variable named `email` assigned from
  an unknown call is not a source.
- **Data maps** apply by field name, and to typed objects by collection name (`User` ↔ `users`).
- **Sinks are the catalogue's** ([`catalogue/sinks.yml`](catalogue/sinks.yml)). An SDK it does
  not list is a gap, not a sink. Loggers injected without a type are recognised by receiver name
  (`logger`, `log`) and marked `heuristic`.
- **Outbound HTTP:** a host is known when the URL is a literal, a template or concatenation that
  starts with one, or a constant holding one; a client's base URL (axios instances, `httpx.Client`)
  is never followed, so calls on such clients have a dynamic host.
- **Browser storage** (`localStorage`, `sessionStorage`, `document.cookie`, js-cookie) is reported
  as flows, not egress; no rule fires on it in v0.1. `window.parent.postMessage` and
  `window.open` are gaps, not sinks.
- **Not sinks:** writing to the system's own databases, caches, queues on its own infrastructure,
  and files (data at rest is privacy-datamap's domain); rendering data into the application's own
  HTML or JSON responses.
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
- **Not yet done:** the labelled real-world benchmark with two reviewers, the comparison with
  Privado, and the Zenodo DOI (milestone M6, [benchmark](docs/benchmark.md)); the review of the
  control mapping.
