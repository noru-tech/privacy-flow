# Output: the flow-facts document

`piiflow scan` writes one JSON document (default `.privacy-flow/flows.json`) that every other
command and output format reads. Its schema is [`schemas/flows.schema.json`](../schemas/flows.schema.json),
version `0.1`; it is stable and versioned from v0.1 so that privacy-datamap, ai-inventory and Noru
can rely on it, and a breaking change bumps `schema_version`.

The document separates **facts** from **judgements derived from them**:

| Member | What it is |
| --- | --- |
| `tool`, `catalogue`, `config` | What produced it: tool version, catalogue version and digest, configuration path and digest |
| `subject` | What was scanned: how files were enumerated (`git`, `walk`, `git_revision`), the commit, whether the work tree was dirty, file and line counts per language |
| `analysis` | Engine, `max_call_depth`, data maps ingested |
| `policy`, `system`, `processors` | The effective policy and the declarations from `.privacy-flow.yml`, embedded so that `check` and `validate` need nothing else |
| `sources` | Every source that starts a reported flow: kind, category, `needs_review`, the name it was classified by and by what, function, location, text, and for typed objects where the field is declared |
| `sinks` | Every sink a reported flow ends at: catalogue ID, class, processor, `heuristic`, host (`literal`, `relative` or `dynamic`), function, location, text |
| `flows` | **Facts.** One per (source, sink, category): the call depth and the full `path` of hops, each with `kind` (`source`, `read`, `write`, `assign`, `concat`, `call`, `return`, `throw`, `error`, `sink`; `error` is a library call's error that may quote its arguments, and makes the finding one for review), `path`, `line`, `column` (Unicode code points, 1-based), the source text, the field a read or write touches, and a note |
| `coverage` | **Facts.** `complete`, and every gap with its kind, detail, locations and the sources whose data reached it |
| `findings` | **Derived.** One per rule a flow or gap triggers: rule, severity, message, the flow or gap it is about, categories, processor, `needs_review`, `heuristic`, location, and the human `disposition` |
| `egress` | **Derived.** Per processor (or dynamic host) and sink class: categories, whether declared, the citation, the flows |
| `summary` | **Derived.** Counts by rule and severity, processors reached |
| `cited_files` | SHA-256 of every file a flow or gap cites, so `explain` can tell when a file changed since the scan |
| `diff` | For `piiflow diff`: base, head, and how many findings were introduced, changed and removed |
| `digest` | `sha256:` over the canonical bytes of the document with `digest` empty and every `disposition` null |

`piiflow validate FILE` re-checks a document: schema, digest, and that `findings`, `egress` and
`summary` follow exactly from the facts, the embedded policy and the declared processors (for a
`diff` document, that every finding it keeps follows). A document edited anywhere except in a
disposition fails with exit 3.

## Bytes

Every JSON output is the [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) (JCS) serialization of
its value, with no trailing newline, and the digest is SHA-256 over exactly those bytes, so any JCS
implementation reproduces them. Every list is sorted; nothing depends on the clock, the machine, the
absolute path of the scan root, or the number of threads. CI compares the output of every fixture
across 1 and 8 threads and against goldens on Linux and macOS.

## IDs

IDs are stable across unrelated edits: they hash an **anchor** that contains no line numbers.

- A source: file, enclosing function, source kind, field or parameter name, category and source
  text, then an ordinal among identical anchors in source order: `src-` and 16 hex digits.
- A sink: file, enclosing function, catalogue sink ID, call text: `snk-…`.
- A flow: its source ID, sink ID and category: `flow-…`.
- A finding: its rule and its flow ID or gap ID: `pf-…`.
- A gap: its kind and normalized detail (an API path with repeated segments collapsed, a language,
  a framework, or a file for a parse error): `gap-…`.

Adding a line above a flow does not change its finding's ID, so dispositions survive ordinary
edits; changing the logged expression does.

## Dispositions

Findings are never deleted. A human decision goes in a finding's `disposition`:

```json
{
  "status": "accepted",
  "owner": "privacy@example.com",
  "decided_at": "2026-10-01",
  "expires_at": "2026-12-31",
  "rationale": "Covered by DPIA-2026-07; removal tracked in ENG-412.",
  "remediated_at": null
}
```

The rules follow `acc`'s exactly. Statuses are `open`, `accepted`, `remediated` and
`false_positive`. Every non-open disposition needs `owner`, `decided_at` and `rationale`;
`remediated` also needs `remediated_at` on or after `decided_at`; `expires_at` must not precede
`decided_at`; dates are ISO calendar dates.

`piiflow check FILE --as-of DATE` suppresses an `accepted` or `false_positive` finding from the
threshold decision from `decided_at` through `expires_at` inclusive, and a `remediated` one from
`remediated_at` on. `--as-of` is required whenever any disposition is not open: the machine clock
is never consulted. A suppressed finding stays in the document, its flow and the summary
unchanged; SARIF marks it as suppressed.

Dispositions are excluded from the digest, so recording one keeps the document valid. `scan`
carries them forward by finding ID from the previous document (the output file, `--dispositions
FILE`, or `.privacy-flow/flows.json`), and names any whose finding no longer exists. A PFC01
finding's disposition accepts that gap at every location it lists.

## Other formats

| `--format` | File name inferred from | What |
| --- | --- | --- |
| `json` | `*.json` | The document above |
| `table` | `*.txt` | A summary for terminals and CI logs; `info` findings counted, not listed |
| `sarif` | `*.sarif` | SARIF 2.1.0: one result per finding, `codeFlows` with every hop, `helpUri` to the rule page, `partialFingerprints` with the finding ID, dispositions as suppressions |
| `fides` | `*.yml`, `*.yaml` | System `egress` declarations to merge into a Fides data map ([fides.md](fides.md)) |
| `in-toto` | `*.intoto.json` | An unsigned in-toto Statement v1 over the scanned commit ([in-toto.md](in-toto.md)) |
| `facts` | `*.jsonl` | One canonical JSON line per flow: source and sink location, category, sink class, processor. The [conformance corpus](../conformance/README.md) reads it |

Without `--output` and `--format`, `scan` writes `.privacy-flow/flows.json`; with `--format` and
no `--output`, it prints to stdout. `check FILE --format F --output O` renders a checked document
without re-scanning, which is what the GitHub Action does.

## Personal data in the output

The document contains file paths, function names and short source excerpts (at most 80
characters per hop), never the values of personal data: the analysis is static and reads code, not
data. Excerpts can still contain whatever the code contains (a hard-coded email address in a
test, a comment). Treat outputs like the source they were made from.
