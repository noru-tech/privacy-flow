# ADR 0019: A full data map, merged in place, with proposals as data

Status: accepted, 2026-10-15.

## Context

`piiflow datamap` writes a Fides data map: systems, datasets and flows (ADR 0018), and privacy
declarations. Three questions had no answer in the tool.

- **Declarations need a data use.** Fideslang requires one, and choosing it is a judgement. The
  open/closed line (NOTES.md, 2026-10-02) put purposes and legal bases on Noru's side.
- **A data map is kept by people.** They add legal bases, retention and owners, correct
  categories and delete what is wrong. A tool that overwrites the file on every run cannot be
  run twice.
- **CI needs to know when the map no longer matches the code.**

The owner chose one binary, no agent: piiflow proposes data uses from evidence, never decides
them, and maintains the file in place.

## Decision

**Proposals are data** (`catalogue/topology/purposes.yml`).

- **Where a proposal comes from.** A data use is proposed per sink class, per processor, and per
  kind of evidence (datasets, user input, internal and unresolved edges), with the reason. The
  default data subject is `customer`, and every key is checked against the vendored taxonomy.
- **One declaration per use.** A system's evidence is grouped by proposed data use, and each
  group is one declaration, named by the use's taxonomy name and linked to its egress, ingress
  and datasets.
- **Marked as proposals.** Every declaration is derived and needs review: it is listed under the
  system's `meta.privacy_flow.needs_review`, with a `#: PROPOSED … (basis)` comment.
- **Never proposed:** a legal basis. Logs give no declaration.

**A three-way merge with a generation lock.** `.fides/datamap.lock.json` (RFC 8785, committed)
holds what piiflow generated last. A run merges the new generation into the file over it:

- elements of lists are matched by identity: `fides_key`, `name`, a DataFlow's type and key;
- untouched values follow the code; changed values stay, with a comment saying what piiflow
  would propose;
- deleted elements stay deleted; elements the code no longer shows go unless changed;
  additions stay;
- lists of keys merge as sets;
- a declaration removed from `needs_review` is accepted: its data use and subjects are the
  person's, and it returns when its evidence changes;
- renamed declarations are recognised by their evidence;
- afterwards, declarations keep only references to their system's DataFlows, and placeholder
  systems nothing reaches go.

**`--check` and exit 5.** `--check` writes nothing and exits 5, a new code, when the map or the
lock would change. 5 takes precedence over 4: real repositories have gaps, and drift must still
fail. Changes to `#:` lines and line endings are not drift.

**privacy-datamap decisions win.** A manifest's interpreted, reviewed collections and
declarations override the generation, and override edits of the map too. The manifest is read,
never written.

**The map is not input to itself.** A data map piiflow maintains is not ingested for
classification: reading back its own output made a run without the lock differ from the next.
Decisions for the analysis go in `fields:`.

## Alternatives

- **Overwrite on every run.** Loses every human edit; the map could be generated once, then
  never again.
- **Keep the base in the YAML's `meta`.**
  - For: no second file.
  - Against: noise in every element, and it would be pushed to Fides.
- **Use the committed file at git HEAD as the base.** What was committed is what a person edited,
  not what piiflow generated: edits would read as the code's.
- **Edit the YAML in place, keeping comments.** No maintained Rust crate edits YAML at the CST
  level; hand-written comments are not kept, and a warning says so.
- **Propose nothing, or a placeholder data use.** Fides rejects an empty `data_use`, and a map
  with no declarations is not a data map.
- **Decide the data use.** That is the judgement the open/closed line keeps from this tool.

## Consequences

- `tests/datamap_merge.rs` covers re-runs, edits, deletions, removal, no lock, drift, line endings,
  unreadable input and manifest decisions; merge, manifest and YAML unit tests cover the rules.
- A CI job loads every data map golden with `fideslang` 3.1.3.
- **Exit codes.** 5 is added to the exit codes; the Action gains `datamap-check`.
- **NOTES.md:** a decision entry moves the open/closed line. *Proposing* uses from cited
  evidence, the topology within one repository, and maintaining the map are open; deciding
  purposes, legal bases, RoPA, and topology across repositories stay closed.
- **Limitations:** hand-written comments are not kept; proposals are coarse; logs are not
  declared; `customer` is the default subject; rename detection needs the evidence unchanged.
