# ADR 0006: Hashing is not a default sanitiser

Status: accepted, 2026-10-02.

## Context

The spec asks for "a small default list" of sanitisers — functions that remove or transform
personal data, such as hashing, masking or a team's own `redact()` — and for teams to declare
their own in config with a citation. Hashing is the most common candidate, and the most
contested: a SHA-256 of an email address is a stable identifier that links records about the
same person, and GDPR treats pseudonymised data as personal data (Recital 26). Treating every hash
as a sanitiser would remove the very flows a privacy review cares about ("we only send hashed
emails to the ad network").

## Decision

The default sanitisers are:

- **password hashing** (bcrypt, argon2, scrypt, PBKDF2; and their Python counterparts), which
  removes credential categories only: a password hash is not the password, but it is still
  derived from it;
- **predicates** (`includes`, `startsWith`, `indexOf`, `len`, `isinstance`, `bcrypt.compare` …),
  which remove everything: they return a boolean or a number, not the data.

General-purpose hashing, encoding and encryption are propagators, not sanitisers. A team whose
assessment concludes that a particular construction is anonymisation declares it in
`.privacy-flow.yml` under `sanitisers`, with the categories it removes and a citation to that
assessment. Declared sanitisers are listed with `[config]` by `piiflow rules`, and the
configuration's digest is in every output, so the decision stays visible.

## Alternatives

- **Hashes as sanitisers by default.** Fewer findings, and wrong for most of the cases they would
  hide.
- **Hashes as sanitisers that mark flows "pseudonymised".** A useful refinement for later
  (a category transform rather than a removal); it needs a vocabulary the Fideslang taxonomy does
  not have, so it is on the roadmap rather than invented here.

## Consequences

- A flow of a hashed email address to an analytics vendor is a PF002 finding until the vendor is
  declared, and stays in the egress facts afterwards, which is what an Article 30 record needs.
- Teams with a reviewed redaction or tokenisation helper get exact control, with a citation.
