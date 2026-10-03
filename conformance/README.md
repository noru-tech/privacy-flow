# Conformance corpus

A set of small programs, each with the flows a correct analyser reports for it. It is the answer
key `piiflow` is held to in CI, and it is written so that any other tool can be scored against
the same key.

| Path | What |
| --- | --- |
| [`vectors/`](vectors/) | One directory per vector: the program, `expected.jsonl`, `vector.json` |
| [`run.py`](run.py) | The runner (Python standard library only) |
| [`MANIFEST.json`](MANIFEST.json) | Every vector with its language, status and expected flow count |
| [`CORPUS-DIGESTS.txt`](CORPUS-DIGESTS.txt) | SHA-256 of every corpus file; attested at each release |
| [`digests.py`](digests.py) | Regenerates (or `--check`s) the two files above |
| [`action.yml`](action.yml) | A composite GitHub Action that runs the corpus against any verifier |

## What a vector tests

Each vector isolates one behaviour: field sensitivity, nested objects, writes through an alias,
whole-object logging, spreads, string
building, helpers that return or sink their argument, context-sensitivity, cross-file imports,
default exports, class fields, instance isolation, closures, array callbacks, predicates,
computed keys, request input (Express, Flask), keyword arguments, comprehensions. The ground
truth is deliberately uncontroversial so that a tool's own catalogue does not decide the
outcome:

- **Sources** are reads of fields the classification table names exactly (`email`,
  `phone_number`, `password`), with their Fideslang category, plus request input of category
  `unknown`. No vector depends on names the table only marks as maybe-personal, or on whether an
  identifier (`user.id`) is personal data: where a vector needs a field beside a personal one
  that is not personal, it is `plan`.
- **Sinks** are `console.*`, `print` and Python `logging`.
- A flow is identified by its source location, its sink location and its category, compared by
  path and line: `(source path, source line, sink path, sink line, category)`.

`vector.json` gives the language and a status. `required` vectors must pass. `known-limitation`
vectors record correct behaviour that the reference implementation does not reach yet (for
example, flow-insensitivity: a variable overwritten before it is logged); each is listed in
[KNOWN-LIMITATIONS.md](../KNOWN-LIMITATIONS.md). They are reported, never required.

## The external-runner contract

The runner calls the verifier once per vector:

```text
<verifier command> <vector directory>
```

and reads one JSON object per line from standard output, each a flow:

```json
{"source": {"path": "a.ts", "line": 2}, "sink": {"path": "a.ts", "line": 3}, "category": "user.contact.email"}
```

Paths are relative to the vector directory; other members are ignored. The exit status must be 0
or 4 (4 means the verifier's coverage was incomplete, which is not an error here); anything else
is an error. A vector passes when the reported set equals `expected.jsonl` exactly.

`piiflow` meets the contract with its `facts` output format:

```bash
python3 conformance/run.py --verifier "piiflow -q scan --walk -f facts"
```

(`--walk` enumerates by directory walk, because the vectors are inside this repository's Git work
tree.) Another analyser needs only a wrapper that prints the same lines.

The runner prints totals and, over required vectors, flow-level precision and recall; `--report
FILE` writes every vector's outcome with its missing and unexpected flows.

## Keeping the corpus honest

- A corpus that a broken analyser also passes proves nothing. CI breaks one semantic at a time
  (field sensitivity, interprocedural binding, instance isolation, sanitisers, field stores,
  allocation sites;
  [`.github/scripts/break_engine.py`](../.github/scripts/break_engine.py)) and requires the
  corpus to fail for each.
- `python3 conformance/digests.py --check` runs in CI: the digest list and the manifest must match
  the files.
- The expected flows were written by hand from the programs, then checked against `piiflow`.
  The corpus is an answer key, not a proof; a disagreement between it and an independent
  implementation is the most useful report this directory can receive.

## Adding a vector

Create `vectors/<language>-<behaviour>/` with the program, `vector.json` (`description`,
`language`, `status`) and `expected.jsonl` (sorted, one canonical JSON line per flow), then run
`python3 conformance/digests.py` and commit all of it.
