# ADR 0018: Systems and datasets within a repository

Status: accepted, 2026-10-15.

## Context

A Fides data map describes systems (what runs), datasets (what they store), the data flowing
between them and to processors, and why. Until now piiflow wrote only the egress of one system:
`scan -f fides`. That fragment had three gaps.

- **One system per repository.** eShop has eleven services in one repository.
- **No datasets.** What a repository stores was left to privacy-datamap.
- **No inter-service flows.** A request through a client whose base address is set at
  registration has a dynamic host, so on eShop the order request with its card number went to
  "an HTTP destination the code does not name". The code says where it goes: the registration
  names `https+http://ordering-api`, and the Aspire app host names the Ordering.API project
  `ordering-api`.

The owner chose a deterministic `piiflow datamap` that writes the full map with no agent, reading
an existing privacy-datamap manifest as input (ADR 0019 covers the map itself).

## Decision

**A topology pass after the analysis**, in `src/topology/`, writes an optional `topology` section
into the flow-facts document. Only `datamap` runs it, so `scan` and `diff` output is unchanged.
Its knowledge is data in `catalogue/topology/`, versioned and digested apart from the flow
catalogue.

- **Systems** (`systems.yml`). A deployable unit is matched by its manifest:
  - a C# SDK, `OutputType` or package;
  - `package.json` dependencies or scripts;
  - a marker file (`manage.py`, `Dockerfile`);
  - a module-level application object (`FastAPI()`).

  Orchestrators and test projects never count. A file belongs to the nearest unit above it,
  preferring its own language at each level. A system's closure is:
  - its own files;
  - the library units it references, transitively;
  - files compiled in from elsewhere;
  - local imports, never into another system's own files.

  `systems:` in `.privacy-flow.yml` claims files first. Keys are the snake case of the project
  identity, made distinct by parent directories; orchestrator names are aliases, never keys.
- **Attribution.** A flow belongs to every system whose closure holds all its hops: a library's
  flow belongs to each system that links the library. A flow that crosses two systems' own code
  (the program is analysed as one) is a note, not an edge.
- **Bindings** (`clients.yml`). A registration call sets a client's base address. For a typed
  client, the registered class is the call's type argument, and it receives an injected
  `HttpClient`; for `axios.create`, `httpx.Client` and the like, the requests go through the
  call's result.
  - **Binding.** An HTTP sink with a dynamic host is bound, per system, to the registrations in
    that system's closure. The receiver is chased through copies and fields to the registration
    (`src/values.rs`, shared with the facts builder).
  - **Resolving the host.** Its host resolves to a system by alias: Aspire `AddProject` and
    `AddDotnetProject` names, docker-compose services by build context, and `systems: hosts:`.
    Otherwise a public host is a processor, and a local or unknown one is a note.
  - **Facts and edges.** Bindings are facts; edges are derived from flows, sinks, processors,
    attribution and bindings, so `validate` re-derives them.
- **Datasets** (`stores.yml`). Four store kinds read lowered classes, through a side table the
  lowerings now keep (declared members with their types' exact head and type arguments, and
  outer classes); Prisma schemas are read by a line parser.
  - **Entity Framework:** contexts, sets, applied configuration classes, `OnModelCreating` and
    the local methods it calls, fluent mappings and attributes, Identity's tables.
  - **Django and SQLAlchemy:** model classes and their column attributes.
  - **Prisma:** `model` and `type` blocks, `@map` and `@@map`.

  Fields are classified with the flow classification table, by column then member name. A
  system references the datasets its closure registers, or whose schema client it imports.
- **User ingress.** The seeds of a system's recognised request handlers (`Facts.handler_funcs`)
  give its ingress from `user`.

## Alternatives

- **Analyse each system separately.**
  - For: exact attribution with no joined code.
  - Against: the shared libraries are analysed once per system, and edges between systems
    still need the bindings.
  - Kept for later, as an option.
- **Match service names to project names heuristically** (`ordering-api` ↔ `Ordering.API`).
  - For: it resolves more.
  - Against: it guesses; an orchestrator states the name.
  - Kept for later, flagged as heuristic.
- **Match catalogue types by full namespace only.**
  - Against: the C# lowering qualifies a name the catalogue does not spell by the first
    wildcard namespace in using order (ADR 0016), so eShop's `IEntityTypeConfiguration` becomes
    `MediatR.IEntityTypeConfiguration`.
  - Decision: sets, configurations and attributes match by type name. Fixing the qualification
    itself would change `scan` output.

## Measurements

eShop at `dc7ea499` (Apple M3 Pro):

| | |
| --- | --- |
| Systems | 11, matching the Aspire app host's projects |
| Datasets | 4 |
| Bound edges | `web_app → ordering_api` (card number, address), `web_app → catalog_api`, `webhook_client → webhooks_api` |
| Unresolved, correctly | the webhook sender, which posts to a subscriber's URL |
| Datamap time | 0.65 s against 0.64 s for `scan` (one thread); the topology pass takes 8 ms |

On the 100k-line synthetic C# service, `datamap` takes 1.95 s against 1.87 s for `scan`. Binding
first scanned every statement for each sink; indexing stores, field writes and type references
once took it from 312 ms to 2 ms on eShop.

## Consequences

- Fixtures `cs/multi-service`, `ts/multi-service`, `py/multi-service`, `ts/config-systems`,
  `py/fallback-single`, with topology and data map goldens (`tests/topology.rs`).
- The eShop smoke test checks the data map (`tests/smoke/eshop.datamap.expected.json`).
- **Not covered** (KNOWN-LIMITATIONS):
  - gRPC clients, named HttpClients and Aspire environment URLs;
  - message buses between services;
  - TypeORM, SQLModel, Mongoose, Sequelize and Drizzle;
  - raw SQL and migrations;
  - stores without models (Redis).
