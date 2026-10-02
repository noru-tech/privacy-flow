## What

<!-- One or two sentences on the change. -->

## Why

<!-- The problem, the rule, or the issue it relates to. -->

## Checks

- [ ] `cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings && cargo test`
- [ ] Analysis changes: a fixture (failing and passing case) or a conformance vector covers it
- [ ] Output changes: goldens regenerated deliberately (`UPDATE_GOLDENS=1 cargo test --test determinism`) and the diff reviewed
- [ ] Rule, schema or catalogue changes: docs/rules/, the schema, docs/catalogue.md and CHANGELOG.md updated together
- [ ] No real personal data, secrets or private code in fixtures or examples
