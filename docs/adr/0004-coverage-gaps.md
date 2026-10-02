# ADR 0004: Unknown is a finding — coverage gaps, PFC01 and exit 4

Status: accepted, 2026-10-02.

## Context

Principle 4 of the spec: when the analysis cannot resolve a callee, a host, a dynamic property or
an unsupported framework, it says so, never rounds an unresolved path to "no flow", and an empty
result with coverage gaps cannot exit as clean. A static analyser always has blind spots; the
question is whether they are visible.

## Decision

- Every blind spot is a **gap** in `coverage.gaps`, with a kind, a detail, its locations, and the
  sources whose data reached it. Gaps that depend on data (an unresolved call, a dynamic call, an
  unresolved import, the depth bound) are recorded only where personal data actually reaches them;
  gaps that exist whatever the data (an unsupported language or framework, a parse error, a
  `'use server'` module) are always recorded.
- An unknown call is treated as propagating (its result carries its arguments) **and** reported:
  over-approximating the flow keeps later sinks visible, and the gap says why the call itself was
  not judged.
- Every gap is a **PFC01** finding, so it can carry a disposition like any other. PFC01 cannot be
  disabled in the policy; config that tries is rejected (exit 3).
- **Exit codes** follow `acc`: `scan` and `diff` exit 4 when any gap exists; `check` exits 4 while
  any PFC01 finding has no disposition on the `--as-of` date, and 4 takes precedence over 1.
- Gaps are aggregated (one per API path, with repeated chain segments collapsed, or per language or
  framework), so that one disposition covers one decision, not one call site.
- A call is a gap only if nothing explains it: a local target, a catalogue match on any of its
  API paths, a heuristic sink, a known plain-value method. Calls into an unsupported framework fold
  into that framework's gap.
- The depth bound is a gap only where it cut a witness to something not reached any other way
  (otherwise every recursive helper would be a gap); the default bound is 32.

## Alternatives

- **Exit 0 with a warning.** Silent in CI, which is where it matters; rejected by the principle.
- **Treat unknown calls as sinks.** Every unknown library call with personal data would become a
  PF002-like finding with an invented processor: louder and less honest than a gap.
- **Treat unknown calls as non-propagating.** Loses every flow through an unmodelled helper
  library, silently.
- **Exit 1 for gaps.** Conflates "this code does something we flag" with "we could not see": the
  first needs a fix, the second needs a review or a catalogue entry.

## Consequences

- Real codebases start with gaps (76 on documenso, 112 on Netflix Dispatch after the catalogue
  work, mostly unknown libraries). Each is either a catalogue entry to add (often upstream, which
  is how the catalogue grows) or a recorded decision.
- Names the classification table marks only *maybe* personal are a different kind of unknown —
  the category, not the destination — so they are findings at `info` severity with
  `needs_review: true` (the review queue), resolved with `not_personal` in config.
