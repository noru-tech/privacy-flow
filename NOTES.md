# Notes

Working notes for `privacy-flow`: decisions, measured numbers and milestone status. Newest
milestone status is at the bottom. Decisions are never rewritten; a reversal gets a new entry.

## 2026-10-02 — Decision: where the open/closed line sits

Decided by Bip before any code was written. The suggested split is adopted unchanged.

**Open (this repository, MIT):** parsing, the intermediate representation, flow analysis, the
source/sink/sanitiser/propagator catalogue, the rules, and every output format (table, canonical
JSON flow facts, SARIF, Fides egress, in-toto). Anyone can run `piiflow` on their own code with
no Noru account and get every finding, every cited path and every coverage gap.

**Closed (Noru):** purpose and legal-basis inference, RoPA assembly, cross-repository topology
(joining flow facts from many repositories into one processing map), and anything that turns flow
facts into Article 30 records.

The test applied to anything new: if it answers *where does the data go, and how do we know*, it
is open. If it answers *why is that allowed, and what does the record of processing say*, it is
Noru. A flow fact is evidence; a lawful-basis judgement about it is not something this binary
makes (see principle 3 and "Out of scope" in the spec).

Consequence for the scope sections: none. The spec's scope already stops at flow facts.

## 2026-10-02 — Name check

Checked on 2026-10-02:

| Name | Where | Result |
| --- | --- | --- |
| `privacy-flow` | crates.io | free (404) |
| `privacy-flow` | `noru-tech/tap` | no formula |
| `pflow` | crates.io | **taken**: `pflow` 0.3.0, Petri-net modelling (pflow-xyz/pflow-rs) |
| `pflow` | PyPI | **taken**: `pflow` and `pflow-cli` |
| `pflow` | GitHub | **collision**: `spinje/pflow`, an actively developed CLI that installs a `pflow` command for building AI-agent automations, pushed the day before this check; also `LumaPictures/pflow` |
| `pflow` | Homebrew core and `noru-tech/tap` | no formula |
| `PrivacyFlow` | GitHub | `HKUDS/PrivacyFlow` (unrelated, agent secrets) — similar name, different package |
| `piiflow` | crates.io | free (404) |
| `privflow` | crates.io | free (404) |

`spinje/pflow` targets the same audience Noru sells to (teams building on LLM APIs), so a
`pflow` binary would collide on exactly the machines that matter.

**Decision (Bip):** the package is `privacy-flow` everywhere (crate, repository, Homebrew
formula name, GitHub Action), and the binary is `piiflow`. This follows the `acc` convention: one
package name everywhere, a short binary name. The spec's `pflow` is read as `piiflow` throughout
this repository.

## 2026-10-02 — Repository

Local at `~/code/privacy-flow`, with a private GitHub repository `noru-tech/privacy-flow` created
from it. Nothing is published to crates.io or the tap until a release is cut deliberately.
