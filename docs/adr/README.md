# Architecture decision records

One record per major choice: the context, the decision, the alternatives and why they lost, and
what follows. Records are never rewritten; a reversal is a new record that supersedes the old one.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](0001-shared-ir.md) | Lower every language into one small IR | Accepted |
| [0002](0002-flow-engine.md) | Worklist engine with function summaries; Datalog engine kept as the oracle | Accepted |
| [0003](0003-provenance-and-resolution.md) | Resolve calls by provenance: API paths, local functions, types | Accepted |
| [0004](0004-coverage-gaps.md) | Unknown is a finding: coverage gaps, PFC01 and exit 4 | Accepted |
| [0005](0005-instances-and-fields.md) | Per-field variables for `this`, and constructors that return their own instance | Accepted |
| [0006](0006-sanitisers-and-hashing.md) | Hashing is not a default sanitiser | Accepted |
| [0007](0007-allocation-sites.md) | Allocation sites for plain objects, within what points-to can see | Accepted |
