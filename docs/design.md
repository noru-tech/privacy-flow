# Design

How `piiflow` turns a repository into cited flows. The decisions behind the major choices are
recorded as ADRs in [`adr/`](adr/README.md); this page describes the system as it is.

```text
files ──► lower (tree-sitter, per file, parallel) ──► IR per file
      ──► program (renumber in path order, resolve imports, provenance fixpoint)
      ──► facts (edges, seeds, call sites, sink and gap hits, from the catalogue)
      ──► engine (summaries bottom-up, then a shortest-path search per seed)
      ──► report (sources, sinks, flows, coverage, findings, egress, digest)
      ──► output (JSON, table, SARIF, Fides, in-toto, facts)
```

## 1. Files

Inside a Git work tree the tracked files are enumerated with `git ls-files`; elsewhere, by a sorted
directory walk (`scan --walk` forces it). `diff` reads a revision's tree from the object database
with `git ls-tree` and `git cat-file --batch`, without a checkout. Exclusions apply (tests,
fixtures, vendored and generated code by default), files over 2 MiB are skipped and reported, and
files in languages the analysis does not parse are reported as a coverage gap. The method is
recorded in the output.

## 2. Lowering

Each file is parsed with tree-sitter (TypeScript, TSX, JavaScript, Python grammars, pinned in
`Cargo.lock`) and lowered into a small shared IR ([`src/ir.rs`](../src/ir.rs),
[ADR 0001](adr/0001-shared-ir.md)). Every expression becomes a variable; statements move values:

| Statement | Meaning |
| --- | --- |
| `Copy dst ← src` | every field of `src` flows to the same field of `dst` |
| `Load dst ← obj.f` / `Store obj.f ← src` | field read and write (`f` absent for a computed key) |
| `ThisLoad` / `ThisStore` | `this.f` in a method, through the field's own variable |
| `Concat dst ← parts` | template literals, f-strings, `+`: everything collapses into `dst` |
| `Call dst ← callee(args)` | with the callee as a value, a method on a receiver, or dynamic |
| `Return`, `FuncRef`, `ClassRef`, `Global`, `Import`, `TypeRef`, `Lit` | the rest |

Scoping follows each language (block scope and hoisting in JavaScript; function scope with
`global`/`nonlocal` and invisible class scopes in Python). JSX elements that render a component are
calls with a props object; destructuring becomes loads; `x.get("email")` is also a load of
`email`. Parsing happens in parallel; each file's IR uses file-local numbers, so nothing depends on
the thread count.

## 3. Program and provenance

Files are assembled in sorted path order and renumbered. Imports resolve to scanned files:
relative paths, `tsconfig.json` path aliases (the nearest config above the importing file, with
`extends`), workspace packages (`package.json` names), and Python packages (relative imports, and
absolute ones by the shortest matching path); otherwise to an external module.

**Provenance** answers "what is this value?" so that calls can be resolved
([ADR 0003](adr/0003-provenance-and-resolution.md)). It is a flow-insensitive fixpoint over the
statements, with bounded path length (16 segments) and bounded sets (16 API paths per variable):

- an import gives `module:name` (an **API path**), a local function, class or module;
- member access extends a path; a call adds `()`; `new C()` of a local class gives an instance;
- a type annotation gives the annotated type's path or an instance of the local class
  (`logger: Logger` from `pino` is `pino:Logger`);
- a function passed to an external API gives its parameters `api.<cbN>`; a function in a field of
  an options or props object gives `api.<fieldN>`;
- arguments give a local function's parameters their provenance (so a callback resolves inside
  the callee), and a function's return carries its object's fields;
- containers: what goes in through `set`/`push` comes out of `get`/`pop`, keyed by the container's
  variable, field or API path; computed keys meet in the container's elements.

Provenance never carries personal data; it only decides which function or API a call reaches.

## 4. Facts

[`src/facts.rs`](../src/facts.rs) turns statements into the graph the engines run on. Nodes are
`(variable, field token)`, where a token is `TOP` (the whole value) or a named field. Edges come
from statements (copy, collapse, load, store, sanitise). Calls are decided by the catalogue
([the order](catalogue.md#how-a-call-is-decided)): a local target becomes a **call site** with
argument-to-parameter bindings; a sink becomes a **hit**; a sanitiser or propagator becomes edges;
anything unexplained becomes a **gap hit** and propagates conservatively. **Seeds** come from
classified field reads, classified parameter names, typed objects, framework request input and
read sources.

Classes ([ADR 0005](adr/0005-instances-and-fields.md)): each `this.field` used in methods is its
own shared variable, which gives two levels of field sensitivity on instances; a constructor has
its own `this`, returned through the constructor's summary, so an instance carries only what it
was built with.

Plain objects ([ADR 0007](adr/0007-allocation-sites.md)): every literal is an allocation site,
and a flow-insensitive points-to analysis inside the facts builder gives each written field of a
site its own variable. A field read of a variable that holds only known sites copies from those
variables, so nested fields stay apart; a read of a value from a parameter, a call or an import
keeps its `Load` edge.

## 5. Engines

Both engines implement the same normative semantics, written out in
[`src/engine/mod.rs`](../src/engine/mod.rs) ([ADR 0002](adr/0002-flow-engine.md)).

- **Summaries.** For each function with callers, for each formal parameter, token (whole, a field
  the function reads, or "a field it does not name") and category class (categories every
  sanitiser treats alike), the summary records which return tokens, hits and shared variables the
  parameter reaches, applying callees' summaries at nested call sites. Summaries stop at shared
  variables (module bindings, instance fields). They are computed by a dependency-driven worklist
  until nothing changes, which handles recursion.
- **Search.** From every seed, a shortest-path search over the edges, applying summaries at call
  sites, continuing from shared variables, and following a function's return to all its call sites
  (a return slot only ever holds data that originated inside the function, so this is never an
  unrealizable path). The first witness to each hit, shortest then shallowest, is kept.
- **Depth.** Entering a callee and returning from one each cross a call boundary; a witness may cross
  at most `max_call_depth` (default 32). Where the bound cut a witness to something not reached
  any other way, a `depth_bound` gap is reported at that call site.

This is context-sensitive for parameters (a helper called with an email address and with an ID
does not mix them), field-sensitive, and flow-insensitive within a function.

The worklist engine is the one `scan` uses; the Datalog engine (ascent) computes the same
reachability and is the test oracle: every fixture and the whole fixture tree must give identical
results from both, and `scan --engine datalog` runs both and fails on any disagreement.

## 6. Report

[`src/report.rs`](../src/report.rs) turns reach results into the [document](output.md):

- **flows**, one per (source, sink, category), each with its hop chain rebuilt from the witness
  (calls into callees are spliced in from their summaries);
- **subsumption**: a flow whose source is a hop of a longer flow to the same sink and category is
  dropped (a typed parameter and the read of that field further down the same path are one datum),
  and `unknown` request input is dropped where a classified field read on its way names it;
- **coverage**: static gaps, and reached gap hits aggregated per normalized detail;
- **findings, egress, summary**, derived by a pure function of the above, the policy and the
  declared processors, which is what `validate` re-runs;
- **IDs** from anchors without line numbers, and the **digest** over RFC 8785 bytes.

## 7. Determinism

Every collection is ordered or sorted before output, parallel work is collected in input order,
ties in the search break on `(distance, depth, variable, token)`, IDs do not depend on numbering,
and the clock is never read. Tests check byte-identical output across thread counts and against
goldens, which CI runs on Linux and macOS.

## 8. Performance

Measured numbers are in [NOTES.md](../NOTES.md) and [benchmark.md](benchmark.md): about 1 s for a
100k-line synthetic TypeScript service and 1.6 s for a 220k-line TypeScript monorepo on an
Apple M3 Pro. `benches/` has criterion benchmarks; CI enforces a coarse time and memory budget.
