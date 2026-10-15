# The data map: `piiflow datamap`

`piiflow datamap` writes and maintains a full Fides data map of a repository,
`.fides/datamap.yml`. It contains the repository's systems, what they store, where they send
personal data, what they receive, and a proposed privacy declaration for each purpose. Nothing
in it is made up, and nothing is decided for you:

- **Systems, datasets, egress and ingress come from the code.** A `#:` comment above each element
  says where.
- **Data uses and data subjects are proposals.** Each one is listed for review until a person
  decides, and no legal basis is proposed.
- **Your edits stay.** A re-run merges the code's new picture into the file instead of
  overwriting it.

```console
$ piiflow datamap
wrote ./.fides/datamap.yml and its lock: 11 system(s) in the repository (detected), 4 dataset(s), 11 declaration(s) to review; …
$ git add .fides/datamap.yml .fides/datamap.lock.json
```

It runs offline, with no configuration, on TypeScript, JavaScript, Python and C#. The output is
Fideslang 3.1.3, the version of the vendored taxonomy, and loads with the Fides CLI's own model.

## What is in it

### Systems

There is one system per deployable unit of the repository
([`catalogue/topology/systems.yml`](../catalogue/topology/systems.yml)):

| Language | Deployable unit |
| --- | --- |
| C# | A web or worker SDK project, an executable, or an Azure Functions worker |
| JavaScript | A package that depends on a server framework or an application framework (Next.js, Vite, …), or has a `start` script |
| Python | A Django project (`manage.py`), or a module that creates a FastAPI, Flask, Starlette, Celery, Litestar or aiohttp application |
| Any | A directory with a Dockerfile |

These are never systems: .NET Aspire app hosts, test projects, and units none of whose files are
analysed.

A file belongs to the nearest unit above it. A system's code is its own files plus those of the
libraries it references (project references, compiled-in files, local imports). A flow belongs to
every system whose code holds all of its hops.

A system's key comes from its project or package name (`Ordering.API` becomes `ordering_api`).
With no deployable unit, the repository is one system, named by `system:` in
`.privacy-flow.yml`, or `this_system`.

To set the systems yourself, declare them; their files are claimed first:

```yaml
systems:
  - fides_key: orders_api
    name: Orders API
    paths: ["services/orders/**"]
    hosts: [orders.internal.example.com]
```

### Egress and ingress

- **To processors.** A flow to a processor (PostHog, Stripe, an LLM provider, a literal host) is
  an egress DataFlow to a system of type "Third Party".
- **To other systems.** A request sent through a registered client reaches the system its base
  address names ([`catalogue/topology/clients.yml`](../catalogue/topology/clients.yml)). The
  client can be a typed `AddHttpClient<T>`, `axios.create({ baseURL })`, or an `httpx.Client` or
  `aiohttp.ClientSession` with `base_url`. The address resolves to a system by the host names
  systems are reached by:
  - .NET Aspire `AddProject`/`AddDotnetProject` names;
  - docker-compose services (by their build context);
  - `systems: hosts:`.
- **To a destination the code does not name.** It gets a placeholder system,
  `unresolved_<class>_<file>`. Replace the key in your system's egress with the receiving
  system's, or declare the host; the placeholder then goes.
- **Ingress.** Each system's ingress mirrors the edges into it, plus `user`: the categories its
  request handlers receive.

### Datasets

Datasets come from ORM models ([`catalogue/topology/stores.yml`](../catalogue/topology/stores.yml)):

| Store | What is read |
| --- | --- |
| Entity Framework Core | Contexts and their `DbSet<T>`; configuration classes and `OnModelCreating` (`ToTable`, `HasColumnName`, mapped private fields, shadow properties, `Ignore`, owned types as nested fields); `[Table]`, `[Column]`, `[NotMapped]`; ASP.NET Core Identity's tables |
| Django | `models.Model` subclasses, `Meta.db_table`, abstract bases, `*Field()` columns, foreign keys as `<name>_id`, `AbstractUser`'s fields |
| SQLAlchemy | Declarative models, `__tablename__`, `Column`/`mapped_column`, `Mapped[...]` |
| Prisma | `model` and `type` blocks, `@map`, `@@map` |

Fields are classified by column name, then member name, with the same table flows use. A field
that is only maybe personal (`name`, `description`) has no category and is listed under the
dataset's `meta.privacy_flow.needs_review`. The system that registers a context, or uses a
schema's client, references the dataset.

### Privacy declarations

Each system's evidence (its egress, its user input, its datasets, the systems it receives from)
is grouped by a proposed data use
([`catalogue/topology/purposes.yml`](../catalogue/topology/purposes.yml)). Each group becomes one
declaration:

```yaml
    meta:
      privacy_flow:
        needs_review:
          - "Analytics for Reporting"
    privacy_declarations:
      #: PROPOSED analytics.reporting with data subjects customer, from the code: review it
      #: purposes.yml#by_class.analytics: product analytics
      - name: "Analytics for Reporting"
        data_use: "analytics.reporting"
        data_subjects:
          - "customer"
        data_categories:
          - "user.contact.email"
        egress:
          - "posthog"
```

To accept a declaration, review it, change its data use or data subjects if needed, and remove
its name from `needs_review`. Its data use and subjects are yours from then on. If its evidence
changes (a new category, a new egress), it goes back under review. Logs give no declaration:
logging is not a purpose. Legal bases, retention and the rest are yours to add; piiflow keeps
them.

A declaration needs a name and a data use. piiflow never makes them up, whether the declaration
comes from a privacy-datamap manifest or from your edits. A declaration without them is moved out
of `privacy_declarations` into the system's `meta.privacy_flow.proposals`, and out of
`needs_review`:

```yaml
    meta:
      privacy_flow:
        proposals:
          #: INCOMPLETE: no data use, so not a Fideslang declaration; complete it and move it to privacy_declarations
          - name: "Send receipts"
            data_use: ""
            data_categories:
              - "user.contact.email"
    privacy_declarations: []
```

To keep it, give it a name and a data use and move it back into `privacy_declarations`.

## Re-runs: your edits stay

The lock, `.fides/datamap.lock.json`, holds what piiflow generated last. Commit it with the map.
A re-run compares three things: the lock, your file, and what the code shows now.

| You | The code | Result |
| --- | --- | --- |
| left a value as generated | changed it | the code's value |
| changed a value | changed it too | yours, with a `#: piiflow proposes …` comment |
| deleted an element | still shows it | stays deleted |
| left an element as generated | no longer shows it | removed |
| changed an element | no longer shows it | kept, and reported |
| added an element or a field (`legal_basis_for_processing`, `retention_period`, `dpo`, …) | — | kept |

- **Lists of keys** (`data_categories`, `data_subjects`, `dataset_references`) merge as sets.
- **A declaration you renamed** is recognised by its unchanged evidence.
- **Without the lock,** nothing in your file is updated, and a warning says so.
- **Comments:** piiflow's own `#:` comments are rewritten on every run; other comments are not
  kept (use a `description` field).

A map that piiflow maintains is not read back as classification input. To tell the analysis
that a field is personal, use `fields:` in `.privacy-flow.yml`.

## Decisions made in privacy-datamap

If the repository has a privacy-datamap manifest (`.noru/privacy-datamap.yml`, or
`--manifest`), the decisions signed there win over the proposals and over edits of the map. A
decision is a collection or declaration with an `interpretation` and no `needs_review`.

- **Collections.** A decided collection's field categories and `non_personal_fields` replace
  the generated ones of the collection of the same name.
- **Declarations.** A decided declaration replaces its system's proposal and leaves the review
  list. Systems match by key, or by privacy-datamap's root-path key (`apps/api` →
  `apps_api`).
- **What only the manifest has.** Its datasets and systems are included. Declarations of such a
  system that are not decided are listed in its `needs_review`. A declaration with no name or no
  data use is listed as a proposal (see [Privacy declarations](#privacy-declarations)).
- **Expired decisions** still apply, and are reported against `--as-of`.
- **Read-only.** piiflow never writes the manifest; `--no-manifest` ignores it.

piiflow writes `.fides/datamap.yml`, the same file privacy-datamap's export writes. Let one of
them own it. Since piiflow applies the manifest's decisions, it can be the one.

## In CI

```console
$ piiflow datamap --check
./.fides/datamap.yml is out of date (map): run `piiflow datamap` and commit the map and its lock
$ echo $?
5
```

`--check` writes nothing. It exits 5 when the map or its lock would change, and 5 takes
precedence over 4, so drift is enforced even with coverage gaps. Changes to `#:` lines (line
numbers that moved) and to line endings are not drift. The [GitHub Action](github-action.md)
runs it with `datamap-check: true`.

## Options

| Option | Default | |
| --- | --- | --- |
| `-o, --output FILE` | `.fides/datamap.yml` | The map to maintain |
| `--lock FILE` | `<map>.lock.json` beside it | The base of the merge |
| `--check` | | Write nothing; exit 5 on drift |
| `--dry-run` | | Write nothing; print the merged map |
| `--manifest FILE` / `--no-manifest` | `.noru/privacy-datamap.yml` if present | privacy-datamap decisions |
| `--as-of DATE` | | Report manifest decisions expired by this date |
| `--facts FILE` | | Also write the flow-facts document, with its [`topology`](output.md) |
| `--config`, `--walk` | | As for `scan` |

Exit codes: 0, 3 (a map, lock or manifest that cannot be read, or a merged map that would not
be valid Fideslang: nothing is written), 4
(coverage incomplete), 5 (`--check`: out of date). See [exit codes](exit-codes.md).

## Fides compatibility

- **Validation.** The merged map is checked offline against the Fideslang 3.1.3 rules that
  matter here:
  - required fields;
  - DataFlow types, and the key `user` with the type `user`;
  - declarations that name only their system's own DataFlows.

  A map that would break these rules is not written: piiflow exits 3 and names each problem,
  which is always in what you edited or added. Declarations without a name or a data use are
  moved to proposals first, so they never stop a run. CI loads every generated golden with
  `fideslang` itself.
- **Machine-readable markers.** These live under `meta.privacy_flow`: `needs_review` on systems
  and datasets, `proposals` (incomplete declarations) on systems, and `unresolved` on
  placeholder systems.
- **Merging into another data map.** The receiving systems are referenced by key, and the Fides
  server expects them to exist in the data map you push.

See [ADR 0018](adr/0018-systems-and-datasets.md) and
[ADR 0019](adr/0019-datamap-merge.md) for why it works this way, and
[KNOWN-LIMITATIONS](../KNOWN-LIMITATIONS.md#the-data-map) for what it does not do.
