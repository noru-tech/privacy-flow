# ADR 0002: Worklist engine with function summaries; Datalog engine kept as the oracle

Status: accepted, 2026-10-02.

## Context

The spec recommends emitting IR facts as relations and running the flow analysis as a Datalog
program (with a Rust crate such as `ascent` or `crepe`), for declarative rules and deterministic
fixpoints, and asks for a comparison against a hand-written worklist over a def-use graph on a
benchmark before committing, with the numbers recorded here.

What the analysis must produce shaped the comparison: not only which sources reach which sinks,
but one shortest witness path per (source, sink, category), with every hop cited, through
function calls (context-sensitive for parameters, so that a helper called with an email address
and with an ID does not mix them), within a call-depth bound that is reported where it bites.

## What was built

Both engines, over the same facts, with one normative semantics written out in
[`src/engine/mod.rs`](../../src/engine/mod.rs):

- **Worklist** ([`src/engine/worklist.rs`](../../src/engine/worklist.rs)): function summaries
  (per formal parameter, field token and category class: reached return tokens, hits and shared
  variables) computed by a dependency-driven worklist to a fixpoint, then a Dijkstra search from
  every seed over states `(variable, token, depth)`, splicing summaries at call sites. The search
  keeps parent pointers, so the witness path falls out of it.
- **Datalog** ([`src/engine/datalog.rs`](../../src/engine/datalog.rs)): the same summaries and
  reachability as an `ascent` 0.8 program (relations `fr`, `sum`, `fr_app`, `tr`, `tr_app`,
  `reach`), with the depth-bound rules in a second stratum because they negate reachability. It
  computes which seeds reach which hits; it does not reconstruct paths.

## Measurements

Apple M3 Pro, release build, `piiflow` at the commit that added this ADR.

| Input | Worklist engine | Datalog engine |
| --- | --- | --- |
| documenso (220,640 lines of TypeScript, 2,001 files, 1,448 flows) | 118 ms | about 920 ms (1,036 ms for both engines together) |
| peak memory of the whole scan on documenso | 358 MB | 838 MB (both engines together) |
| synthetic service, 400 modules (criterion, engine alone) | 6.6 ms | 7.9 ms |
| fixture tree (criterion, whole scan) | 14.4 ms | — |

Both engines report identical reachability on every fixture, every conformance vector, the whole
fixture tree, and documenso.

## Decision

`scan` uses the worklist engine. The Datalog engine is kept as an independent oracle:

- the fixture suite runs both and fails on any difference;
- `scan --engine datalog` (hidden) runs both on any input and exits 3 if they disagree;
- the criterion bench reports both.

## Why

- **Witness paths need the search anyway.** The output's central promise is a cited chain for
  every finding. With Datalog the paths would come from a second pass over the same graph, with
  summaries spliced in, so the worklist would have to exist regardless; Datalog would only add a
  second implementation of reachability on the critical path.
- **Cost on real code.** On documenso the Datalog program took about eight times as long and more
  than twice the memory, because it materialises reachability at every depth for every summary
  start and every seed. On small, regular inputs the two are close.
- **Determinism is not at stake.** The worklist engine is deterministic by construction (ordered
  maps, ties broken on `(distance, depth, variable, token)`, seeds run in parallel but collected
  in order), which the determinism tests check.
- **"Rules are data" does not need Datalog.** The catalogue extends the analysis without Rust
  changes because the facts builder only asks the catalogue what a call is; the engines never
  see a vendor name. A rule that needs a new kind of fact still needs Rust in either design.

What Datalog gives — a declarative statement of the semantics that is easy to compare against —
is exactly what an oracle is for, so it stays in that role.

## Consequences

- The semantics are written once in prose and twice in code; any change to one engine must be
  made in the other, or the fixture suite fails.
- The `ascent` dependency stays in the binary (it is pure Rust and small); removing it later is a
  build-time decision, not a design one.
- If a future rule needs facts only a declarative query can express conveniently (for example,
  joining flow facts across repositories), Datalog is ready; that join is Noru's side of the
  open/closed line in any case.
