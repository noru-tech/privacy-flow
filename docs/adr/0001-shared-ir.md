# ADR 0001: Lower every language into one small IR

Status: accepted, 2026-10-02.

## Context

v0.1 analyses TypeScript, JavaScript and Python, and Go and Java/Kotlin are on the roadmap. The
analysis (provenance, flow facts, engines, rules, outputs) should not know which language a
program was written in, so that a language is added by writing a lowering, not by touching the
analysis. The spec asks for parsing with tree-sitter through the Rust bindings, grammar versions
pinned in `Cargo.lock`.

## Decision

Each language has one lowering module (`src/lower/js.rs`, `src/lower/python.rs`) that walks the
tree-sitter syntax tree and emits a per-file `FileIr` ([`src/ir.rs`](../../src/ir.rs)):
variables, and statements that move values between them — copy, literal, field load and store
(with a computed-key form), concatenation, call (value, method or dynamic callee; positional,
keyword and spread arguments), return, function and class references, free globals, imports,
type references, and per-field instance loads and stores — plus declarations (functions with
parameters, classes with methods and constructor instance, type declarations with field names,
imports, exports, re-exports) and notes (parse errors, unsupported constructs).

Every expression is lowered to a variable; nested expressions become temporaries. Everything
language-specific stays in the lowering: scoping and hoisting rules, destructuring, decorators,
JSX, f-strings, comprehensions, `self` binding, `getattr`, `require`. The grammars are
tree-sitter 0.27 with tree-sitter-typescript 0.23.2, tree-sitter-javascript 0.25.0 and
tree-sitter-python 0.25.0, pinned in `Cargo.lock`.

Per-file numbering keeps lowering embarrassingly parallel (rayon); the program assembler
renumbers files in sorted path order, so results do not depend on the number of threads.

## Alternatives

- **One analysis per language over its syntax tree.** Simplest for one language, but every rule
  and engine change would be made twice, and the third language three times.
- **An existing code property graph (Joern, as Privado uses).** A JVM dependency, a large runtime,
  and a graph far richer than this analysis needs; it would rule out a single static binary.
- **A richer IR (SSA, control flow).** Needed for flow-sensitivity, which v0.1 does not have
  (the spec asks for flow-insensitive within a function). The IR can grow ordered blocks later
  without changing the statement kinds.

## Consequences

- Adding Go or Java means one new lowering module and its fixtures; the catalogue gains entries
  with `language: go`. The analysis needs no change unless the language has a construct the IR
  cannot express.
- The analysis is flow-insensitive: a value overwritten before it reaches a sink still flows
  (a `known-limitation` conformance vector records this).
- Citations come from the IR: every statement carries its position (1-based line, column in
  Unicode code points) and its source text, truncated to 80 characters.
