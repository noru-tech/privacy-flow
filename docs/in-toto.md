# in-toto Statement

`piiflow scan --format in-toto` (or `-o flows.intoto.json`) writes an unsigned
[in-toto Statement v1](https://github.com/in-toto/attestation/blob/main/spec/v1/statement.md)
whose subject is the scanned commit and whose predicate is the [flow-facts document](output.md):

```json
{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [{ "name": "git:scanned-commit", "digest": { "gitCommit": "3f1c…" } }],
  "predicateType": "https://noru.tech/spec/privacy-flow/flows/v0.1",
  "predicate": { "$schema": "…/flows.schema.json", "schema_version": "0.1", "…": "…" }
}
```

It mirrors `acc`'s in-toto output: the bytes are RFC 8785, the Statement is unsigned, and signing
is left to the signer you already use for build provenance.

- **A commit is required.** Scan inside a Git work tree; outside one, the format exits 2 because
  there is nothing to name as the subject.
- **A dirty tree** is still scanned and named by its HEAD commit; `predicate.subject.dirty` says
  it was dirty. Sign Statements from clean checkouts.

## Signing and verifying

Sign with cosign or GitHub artifact attestations, as for any in-toto Statement. A verifier checks
the subject against the commit it cares about and runs:

```bash
piiflow validate flows.intoto.json
```

which checks that the Statement's type and predicate type are these, that its subject is exactly
the predicate's scanned commit, and then validates the predicate like any flow-facts document:
schema, digest, and findings that follow from the facts. `piiflow check flows.intoto.json` enforces
the policy on it.

`piiflow` does not verify signatures; verify the envelope first, then validate the Statement.
