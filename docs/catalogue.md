# The catalogue

Sources, sinks, sanitisers, propagators and frameworks are data, in YAML under
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

JavaScript paths are `module:path`; Python paths are fully dotted. A call adds `()`. A callback
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
  language: javascript          # javascript (also TypeScript) or python
  class: analytics              # log, error_tracking, analytics, messaging, llm, http,
                                # browser_storage, third_party
  processor: PostHog            # the vendor that receives the data, if there is one
  calls: ["{posthog-node,posthog-js}:**.{capture,identify,alias}"]
  assigns: []                   # property writes that are sinks (`document.cookie`)
  receivers: []                 # heuristic: receiver names (`logger`) …
  methods: []                   # … and method names, for receivers of unknown origin
  args: all                     # or a list of positional indices
  keywords: []                  # Python keyword arguments to include when args is a list
  receiver: false               # the receiver is sent too (a message object's `send()`)
  host: { arg: 0, keyword: url, field: url, base_url: false }   # outbound HTTP only
```

With `receiver: true` the object the method is called on carries data out as well as the
arguments: `EmailMessage(subject, body, to=[user.email]).send()` sends what the message was built
with, though `send()` itself takes nothing. Builder methods that put data into such an object
(`msg.set_content(text)`, `notifier.add(url)`) are `args_to_receiver` propagators.

`host` says where an HTTP call's URL is: a positional `arg`, a Python `keyword`, or a `field` of an
options object. A literal URL (or a template or concatenation starting with one, followed through
constants) gives the host, which names the processor; a relative URL is the application's own
origin; anything else is `dynamic` ([PF006](rules/PF006.md)). With `base_url: true` a relative URL
resolves against a client's configured base URL, so it is dynamic too.

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
(FastAPI), or defined in matching files (`files`, Django views). `route_export` sources are named
exports of matching files (Next.js route handlers). `read` sources are reads and calls of API
paths (`flask.request.form`, `next/headers:cookies`). A parameter's `whole`, `fields` and `methods`
give categories; `typed: true` narrows an annotated parameter to its type's classified fields
(FastAPI with a Pydantic body); `by_name: true` classifies by the parameter's own name;
`skip_types` and `skip_names` leave injected dependencies alone.

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
  flow: none                    # args_to_result | args_to_receiver | receiver_to_callback | none
```

`args_to_result` is the default for string building, validation, formatting and framework
plumbing; `args_to_receiver` puts arguments into the receiver (`xs.push(x)`);
`receiver_to_callback` hands the receiver's elements to a callback's first parameter (`xs.map(f)`);
`none` passes nothing (database clients: writing to the system's own store is data at rest, and a
query's result is the stored record, not its filter values). `non_propagating_fields` lists field
reads that are not the data (`length`, `size`).

## Frameworks (`catalogue/frameworks.yml`)

Each web framework, by the modules that identify it, and whether its request input is modelled.
Importing an unsupported one is a coverage gap ([PFC01](rules/PFC01.md)), and calls into its
modules fold into that one gap.

## How a call is decided

For each call, in this order:

1. A local function or class: followed through its summary (or treated as a sanitiser if a
   sanitiser's `functions` names it).
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
