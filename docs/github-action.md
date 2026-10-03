# GitHub Action

Report the personal-data flows a pull request introduces, as code-scanning alerts with the full
hop chain, and gate merges on them.

```yaml
# .github/workflows/privacy-flow.yml
on:
  pull_request:

permissions:
  contents: read
  security-events: write   # upload SARIF to code scanning

jobs:
  privacy-flow:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0     # the base commit must be in the checkout
      - uses: noru-tech/privacy-flow@v0.2.0
```

Pin both actions to a commit SHA in production workflows, as this repository does.

## What it does

1. **Installs** the `piiflow` release named by `version`: downloads the archive for the runner
   platform, checks its SHA-256, and (by default) verifies its GitHub artifact attestation with
   `gh attestation verify` before running it.
2. **Analyses the changes**: `piiflow diff BASE..HEAD` analyses both revisions from the Git object
   database (no checkout of the base) and keeps only the findings the head introduces or changes:
   a new flow, a flow that now carries a new category, a coverage gap at a new location. Each
   revision is analysed with its own `.privacy-flow.yml`; dispositions come from `dispositions`
   (default `.privacy-flow/flows.json`).
3. **Checks policy**: `piiflow check` with `--as-of` set to today in UTC (computed by the action:
   the binary never reads the clock) renders the result in `format` to `output`, and the job
   summary shows a table.
4. **Uploads SARIF** to code scanning (category `privacy-flow`), each alert with a code flow
   through every hop, linked to its rule page.
5. **Maps the exit code** to the step: 0 passes; 1 (threshold exceeded) and 4 (coverage incomplete)
   fail the step unless `fail-on-findings: false`; anything else fails the run.

## Inputs

| Input | Default | |
| --- | --- | --- |
| `version` | `0.2.0` | Release to install |
| `path` | `.` | Directory to analyse |
| `base` | the pull request's base SHA | Base revision |
| `head` | the pull request's head SHA, or the pushed SHA | Head revision |
| `dispositions` | `.privacy-flow/flows.json` if present | Recorded decisions to honour |
| `as-of` | today (UTC) | Date dispositions are evaluated on |
| `format` | `sarif` | `sarif`, `json`, `table`, `fides`, `in-toto` or `facts` |
| `output` | `privacy-flow.sarif` | Rendered output path |
| `upload-sarif` | `true` | Upload to code scanning when the format is SARIF |
| `fail-on-findings` | `true` | `false` runs in advisory mode |
| `verify-attestation` | `true` | Verify the release archive's attestation |

Outputs: `exit-code`, `output`, and `document` (the JSON flow-facts document of the diff).

## Recording a decision

Commit `.privacy-flow/flows.json` (from `piiflow scan`) with dispositions on the findings you
accept. The action honours them by finding ID, which does not change when lines move. See
[dispositions](output.md#dispositions).

## Without pull requests

To scan a whole repository on a schedule, run the CLI directly:

```yaml
- run: piiflow scan . -o privacy-flow.sarif || test $? -eq 4
```
