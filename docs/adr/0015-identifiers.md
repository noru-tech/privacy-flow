# ADR 0015: Identifiers of people are sources, and logging one is not a finding

Status: accepted, 2026-10-03.

## Context

The classification table (shared with privacy-datamap) names no identifiers: `user.id`,
`user_id` and `customer_id` were not sources. Under the GDPR they are pseudonymous data, which is
personal data (Recital 26), and sending them to a processor is exactly what a record of
processing has to list: a user's ID as PostHog's `distinctId`, as Stripe customer metadata, in
an S3 object key, in an LLM prompt. Re-scoring the benchmark's site labels, these were among the
largest groups of missed sinks.

They are also what a team logs instead of contact data, which is the recommended practice:
Polar logs `customer_id` throughout. Treated like an email address, they added 9,149 medium log
findings on Polar alone.

## Decision

- **Identifiers are classified** as `user.unique_id.pseudonymous` by the catalogue
  (`catalogue/classification.yml`), not the table:
  - names that hold a person's identifier wherever they appear (`user_id`, `customer_id`,
    `member_id`, `subscriber_id`, `stripe_customer_id`, `distinct_id` …);
  - `id`, `uid` and `uuid` read from a person's record only: an object named like a person
    (`user.id`, `customer["id"]`, `owner.id`, `recipient.uid`) or a field of a class or type
    named so (`User.id`). `order.id` and `team.id` are not sources.
- **PF001 does not fire on an identifier alone.** The flow is reported in the facts; logging
  the identifier is not a finding. Every other rule treats it as personal data: PF002 when it
  reaches an undeclared processor, PF003 an LLM, PF006 a dynamic host.
- **Narrowing applies to request input only.** A flow of unknown category is dropped in favour
  of a classified read on its way only when its source is request input
  (`request.form["user_id"]`), as the rule always said; it had applied to any unknown source, so
  identifier reads on the way of a `Customer.name` flow hid that flow.
- The conformance corpus no longer uses `id` as the non-personal field beside a personal one
  (it uses `plan`), so it stays neutral on identifiers: whether `user.id` is a source is the
  catalogue's choice, not the answer key's.

## Alternatives

- **Every rule, at full severity.** Consistent with the law, but a log finding on every
  `log(user_id)` would bury the findings that matter.
- **Every rule, at `info`.** Nothing new fails a build, including identifiers sent to an
  undeclared processor, which should.
- **Classify identifiers in the table.** The table is privacy-datamap's and vendored verbatim;
  the change belongs there too, and can replace these entries once it lands.

## Measurements

The benchmark corpus at its pinned commits, against the build before (ADR 0014):

| Application | flows | new findings | R1 sampled sinks found |
| --- | --- | --- | --- |
| Polar | 8,034 → 18,593 | PF002 high +252, PF006 +470, info +70; PF001 medium −262 | 0 → 0 |
| Open SaaS | 9 → 23 | PF002 high +4, PF003 +1 | 2 → 8 |
| Ghost | 108 → 115 | PF002 high +5 | 3 → 3 |
| Redash | 351 → 370 | PF006 +4 | 11 → 11 |
| Hoppscotch | 152 → 189 | none | 4 → 4 |
| Vercel chatbot, Umami, Taxonomy, CTFd, PrivateGPT | +2 to +26 | PF002 +1 (Taxonomy), PF006 +1 (CTFd) | unchanged |

All twelve: 39 → 45 of 114 sampled sinks found. Polar's 262 fewer medium log findings are request
input (`auth_subject`, path parameters) whose flow carries only the identifier read on its way;
each sink still has the identifier flow. Time is unchanged.

## Consequences

- Fixture `py/identifiers`; the PF004 pass fixture declares Mixpanel, as the fail fixture does,
  since it sends a patient's ID.
- **Not covered:** identifiers under other names (`owner`, `assignee` holding an ID, `sub`), and
  `id` on objects named after a role the list does not have. A project adds its own under
  `fields` in `.privacy-flow.yml`.
