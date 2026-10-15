# ADR 0020: Data of undetermined category is uncertain, and identifiers of things are not personal data

Status: accepted, 2026-10-15.

## Context

A tester ran 0.4.0 on dotnet/eShop and reported two things about classification.

- **Product identifiers read as personal data.** `[Parameter] public int ItemId` (a route
  parameter) and `[SupplyParameterFromQuery] int? BrandId` were reported as `unknown` sent to a
  host computed at run time, PF006 at `warning`. Six of eShop's 41 PF006 findings were item,
  brand and type IDs.
- **`unknown` meant two things.** A name the classification table marks *maybe* personal (`name`,
  `payload`) was an `info` finding for review (ADR 0004). Request input the analysis cannot name
  (`req.body`, a query parameter) kept the rule's severity, as PF001's page said. On eShop, 21 of
  52 findings whose only category was `unknown` were `medium` or `warning`: search text, a
  `returnUrl`, a `clientId`. The same word read as "to review" in one finding and as "personal
  data" in the next.

Request input was already kept to names for ASP.NET Core: a number or a GUID bound by MVC or a
minimal API is a source only when its name is classified (`classify_only_types`). Blazor
properties, FastAPI path parameters, `int[]` and string parameters, and reads by key
(`Request.Query["itemId"]`, `request.args.get("product_id")`) had no such rule.

## Decision

- **Uncertain findings.** A flow whose category is `unknown` is an uncertain finding under every
  rule: `info`, `needs_review: true`, and a message that starts `uncertain: data of undetermined
  category from …`. It is never dropped and never reported as personal data. This replaces
  PF001's "request input keeps the rule's severity", and extends ADR 0004's review queue to
  request input.
- **Identifiers of things.** `catalogue/classification.yml` lists entity words (`item`,
  `product`, `brand`, `catalog`, `category`, `type`, `sku`, `variant`, `order_line`, `line_item`
  …) and identifier suffixes (`id`, `ids`, `uuid`, `guid`, `sku`). A name is an identifier of a
  thing when its words, camelCase split, are entity words followed by a suffix: `item_id`,
  `catalogItemId`, `BrandIds`, `item_type_id`, `productid`. Such a name:
  - is never classified, by the table or the catalogue;
  - is not request input: a parameter, a bound property (`[Parameter]`,
    `[SupplyParameterFromQuery]`) or an `[AsParameters]` property under the name is not a source;
  - stops request input read through it: a flow of `unknown` request input whose path reads a
    field or key under such a name (`Request.Query["itemId"]`, `req.query.productId`,
    `args.get("product_id")`) carries only that identifier and is dropped, as a flow narrowed by
    a classified read is.

  A project's `fields` still classify the name. A project's `not_personal` names now also stop
  request input, the same declaration.
- **Person identifiers are unchanged.** `user_id`, `customer_id`, `buyer_id` and `id` on a
  person's record stay `user.unique_id.pseudonymous` (ADR 0015). One word that is not an entity
  keeps a name out: `user_type_id`, `blood_type_id` and `customer_item_id` are not matched, since a
  lookup ID on a person can encode an attribute of that person.
- **Orders, accounts, teams and tenants are not entities here.** They belong to people often
  enough (a customer's order, a sole trader's account) that an unclassified `order_id` stays
  uncertain, for review. `order.id` is not a source either way (ADR 0015).

## Alternatives

- **Keep request input at the rule's severity.** Logging a request body in full is the case PF001
  exists for. But a scalar query parameter is as often a search term, a page number or a return
  URL, and a finding that fails the build because the analysis could not tell is a claim the
  tool cannot support. A body's classified fields are still findings at full severity
  (`req.body.email`, a typed body); the whole body is in the review queue.
- **A severity of its own for uncertain findings.** A schema change for every consumer of the
  document and SARIF, for what `info` with `needs_review` already says.
- **Every `*_id` is not personal.** It would drop person identifiers under names the catalogue
  does not list (`owner_id`, `patient_id`), which ADR 0015 treats as personal data.
- **Decide by type.** Blazor properties could take `classify_only_types`, as MVC parameters do.
  It would not reach `int[]` or string identifiers, nor reads by key, and would drop every
  unnamed number (a page index) along with them. The name rule covers all of these; the type
  rule can follow on its own.

## Measurements

eShop at `dc7ea499`, against 0.4.0 (`ac10149`):

| | before | after |
| --- | --- | --- |
| Flows | 214 | 208 |
| PF001 | 134 (110 medium, 24 info) | 134 (100 medium, 34 info) |
| PF003 | 12 (4 medium, 8 info) | 12 (12 info) |
| PF006 | 41 (36 warning, 5 info) | 35 (29 warning, 6 info) |
| Findings whose only category is `unknown` | 58, of which 21 medium or warning | 52, all `info` |

The six flows gone are `ItemId` (twice), `BrandId`, `BrandIds`, `ItemTypeId` and `TypeIds` sent
to the catalog API. The fifteen that moved to `info` are search text, chat input and claims
lists sent to a model or a log, a `returnUrl`, a `clientId`, request headers and a webhook token.
The data map no longer draws `web_app → catalog_api`: edges come from flows (ADR 0018), and the
only flows on it were those IDs.

The benchmark corpus at its pinned commits, against the same build:

| Application | flows | findings that became uncertain (`info`) | R1 sampled sinks found |
| --- | --- | --- | --- |
| Polar | 18,398 → 18,391 | PF001 medium 822, PF006 warning 58, PF002 high 14 | 0 → 0 |
| Ghost | 836 | PF002 high 66 | 8 → 8 |
| PrivateGPT | 1,368 | PF001 medium 50 | 0 → 0 |
| Redash | 365 | PF001 medium 20, PF006 warning 18 | 13 → 13 |
| Vercel chatbot | 109 | PF003 medium 15, PF001 medium 1 | 9 → 9 |
| CTFd | 76 | PF001 medium 5, PF002 high 4, PF006 warning 2 | 3 → 3 |
| Open SaaS, Umami, Taxonomy, Healthchecks, Hoppscotch, fastapi-template | unchanged | none | unchanged |

All twelve: 59 → 59 of 114 sampled sinks found; recall counts flows, and only Polar's seven
change. Those read `subscription_update.product_id` from the request and go on through the
product looked up by it. 1,075 findings move to `info`, nearly all whole request objects:
Ghost's `req.query`, `req.cookies` and `req.headers` reaching Stripe through middleware, the
Vercel chatbot's `request.headers`, `request.url` and `request.cookies` reaching the model beside
`request.body`, Polar's `auth_subject` sent to Sentry. Time is unchanged.

## Consequences

- Fixtures `cs/entity-identifiers` and `py/entity-identifiers`. The fixture test requires every
  finding whose only category is `unknown` to be `info`, need review and say so. Goldens change
  in wording; `py/django-view`'s request body sent to an undeclared Sentry is PF002 `info`, no
  longer `high` (the processor is still in the egress records).
- eShop's smoke test forbids PF006 on the catalog clients' item, brand and type IDs and a medium
  PF001 on the search text, and no longer requires the `web_app → catalog_api` edge.
- **Not covered:** entity words in other languages; `id` alone in request input (`/items/{id}`),
  which stays uncertain; a claims list (`User.Claims`), which holds personal data but has no
  category and is uncertain until it gets one.
