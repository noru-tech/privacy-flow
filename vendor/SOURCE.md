# Vendored data — do not edit here

These files are **verbatim copies** from
[`noru-tech/noru-grc-engineering`](https://github.com/noru-tech/noru-grc-engineering) at commit
`530301fd2ebe562e51889eafb74179c8b1a50f87` (2026-09-14). Reusing them, rather than writing new
ones, is deliberate: privacy-datamap (what a repository stores) and privacy-flow (where it sends
it) must agree on what `email` means and which categories are special.

| File | Copied from | SHA-256 |
| --- | --- | --- |
| `taxonomy/data_categories.json` | `contract/lib/taxonomy/data_categories.json` | `f1be7bbd1d19b8f2ed0f777156ec844e8a5a7144563f3afae7548d5c32c24af1` |
| `taxonomy/data_subjects.json` | `contract/lib/taxonomy/data_subjects.json` | `18b9758032e5c6264fd8842aa71630e67b81c253d8064860e0a6e09113fd477c` |
| `taxonomy/data_uses.json` | `contract/lib/taxonomy/data_uses.json` | `557d57cca1eac975ca45dda7a344541ec6c59c13e70e83192d140760e0a8e7bb` |
| `taxonomy/special_categories.json` | `contract/lib/taxonomy/special_categories.json` | `ac5316dbbaf7605f26a3e4750df97f2e341572ebe8ea419a39557dc40c534e71` |
| `classification/classification.json` | `plugins/privacy-datamap/references/classification.json` | `4f1caa4a3e8d076d9ba9e6f6b9d067a9c5888d85f7af4ef76f19c33815650333` |

`tests/vendor.rs` pins these digests, so an accidental edit fails the build. To refresh, copy the
files from a newer commit of noru-grc-engineering, update the commit and digests here and in the
test, and read the diff: a category that disappears upstream changes what existing flow facts
mean.

## Licences

The Fideslang taxonomy is © Ethyca, Inc., **CC BY 4.0**; these JSON files are a modified
redistribution (reformatted to JSON, reduced to `fides_key`, `name` and `description`). Snapshot
of the upstream taxonomy directory at commit `21eb1746904d` (2024-11-04), release 3.1.3. See
[`NOTICE`](../NOTICE). The classification table and the special-category selection are Noru's
own and MIT-licensed.
