# ADR 0022: `scan` resolves requests through registered clients

Status: accepted, 2026-10-15. Changes ADR 0018's "only `datamap` runs it": `scan` now runs the
client bindings.

## Context

A tester ran 0.4.0 on dotnet/eShop and asked for internal service calls to be told apart from
transfers to processors. `scan` reported 41 PF006 findings ("a host computed at run time"). Each
was a request through an `HttpClient` whose host is not in the call.

- **28 had a known host.** `piiflow datamap` already bound them to their registrations
  (ADR 0018). For example, `AddHttpClient<OrderingService>(o => o.BaseAddress =
  new("https+http://ordering-api"))` resolves through the Aspire app host to `ordering_api`.
  The data map drew those edges, but `scan` did not run the bindings, so the same requests stayed
  PF006 findings.
- **Two client shapes were not bound at all:**
  - named clients (`AddHttpClient("billing", …)` with `IHttpClientFactory.CreateClient("billing")`);
  - base addresses read from configuration, which .NET Aspire fills: service discovery
    (`services:billing:https:0`) and `WithEnvironment("Billing__Url", billing.GetEndpoint(…))`.

  NOTES.md listed both as not done.

## Decision

**`scan` runs the bindings.** After the report is built, `analyze` runs the first half of the
topology pass: systems, the names that reach them, and the client bindings
(`topology::destinations`). Attribution, datasets and edges stay in `datamap`. The topology
catalogue's `clients.yml` and `systems.yml` now shape `scan` output, so the flow catalogue's
digest covers them too.

**A sink records where its requests go.** An HTTP sink with a dynamic host gains an optional
`destination`:

| Bindings of the sink, in any system | `destination` |
| --- | --- |
| all name systems of the repository (the sending system too) | `internal`, the system keys |
| all name one host outside it | `processor`, that host |
| none, or both kinds, or two processors | absent |

`via` lists the registrations. The host itself stays `dynamic`: it says what the call does, and
the data map's binding reads it.

**Rules.**

- **PF006** fires only when no destination is recorded.
- **Internal.** An internal request is not external: no PF002, PF004 or PF005, and no egress
  record. The flow is still reported, and the data map draws it as an edge between systems.
- **Processor.** A request to a processor names that processor, as a literal host would. PF002
  fires if it is not declared, and a declaration's `hosts` match it.

**More clients bound** (`catalogue/topology/clients.yml`).

- **Named clients:** `client: named`. A registration's `name_arg` literal names the client. A
  request is bound when its client comes from a `creates` call (`CreateClient`) with the same
  name.
- **Configuration chained on the registration:** `chained: ["**.ConfigureHttpClient"]`, through
  up to four calls on its result. The base address may be stored on any parameter of the lambda
  (`(sp, c) => c.BaseAddress = …`).
- **Base addresses from configuration** (`configuration:`): an indexer read on a configuration
  object, `GetValue`, or `GetConnectionString`. Keys are compared as .NET compares them: case
  folded, `__` read as `:`.
  - `services:<name>:…` and `ConnectionStrings:<name>` name the resource.
  - Any other key resolves through an orchestrator `environment:` call that sets it, on the
    sending project's resource, to another resource's endpoint. For example,
    `.WithEnvironment("Billing__Url", billing.GetEndpoint("https"))` on the `shop` project means
    `shop` reaches `billing` by `Billing:Url`. A key two calls set to different systems names
    neither.
- **Unresolved bases leave a note:**
  - a local or unknown literal host gives `unknown_host`;
  - a key no orchestrator sets gives `unresolved_configuration`.

  The request stays PF006.

## Alternatives

- **A new host kind (`internal`) in place of `destination`.**
  - Against: the host says what the call's arguments show. The data map binds only dynamic hosts,
    and per system. One sink can be internal in one system and unbound in another.
- **Run the whole topology pass in `scan`.**
  - Against: attribution, datasets and declarations do not change a finding.
  - Against: `scan` output would carry a `topology` section that `datamap` owns.
- **Leave `scan` alone and point users to the data map.**
  - Against: the tester's complaint stays. A PF006 finding would claim the host is unknown while
    the data map names it.
- **`AddHttpClient<TClient, TImplementation>`.**
  - Against, for now: the C# lowering records a type argument on a call's result only when
    there is one (`GetRequiredService<T>()`). Recording both would type the builder's result as
    the client, and every chained call would then resolve on it.
  - Kept for later: a side table of type arguments, as members have.

## Measurements

eShop at `dc7ea499` (Apple M3 Pro):

| | Before | After |
| --- | --- | --- |
| PF006 | 41 | 13 |
| Internal requests (sinks) | — | 4: `web_app → ordering_api`, `web_app → catalog_api` (2), `webhook_client → webhooks_api` |
| `scan`, one thread | 0.58 s | 0.59 s |
| `scan` of the 100k-line synthetic C# service, one thread: report step | 373–436 ms | 408–415 ms |
| Data map | | unchanged |

The 13 that remain are hosts the code does not hold:

- 9 in the mobile app's `RequestProvider`, which sends to endpoints the user sets;
- 3 in the hybrid app's catalog client, whose base address is a development address
  (`localhost:11632`) that names no system;
- 1 in the webhook sender, which posts to a subscriber's URL.

## Consequences

- `tests/fixtures/cs/client-destinations`: named clients, an environment URL, service discovery,
  a chained configuration to a processor, and an unregistered name.
- `fixture.json` can state `destinations`.
- Fixtures that changed:
  - `py/multi-service` and `ts/multi-service` reach compose services, so they are internal and
    PF006 no longer fires;
  - `cs/multi-service` binds its client to a system and to a processor, so it keeps PF006.
- The eShop smoke test checks the destinations, and the data map expectation names exactly the
  destinations that stay unresolved.
- **Not covered:**
  - two type arguments (above);
  - clients registered as plain DI services (`AddSingleton(new HttpClient { BaseAddress = … })`);
  - `GetSection("A")["B"]` keys;
  - configuration read through a project's own helper (eShop's `GetRequiredValue`);
  - base addresses from fields of other classes (the hybrid app's `MauiProgram.MobileBffHost`).
