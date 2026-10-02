# Security Policy

## Supported versions

This project is pre-1.0; security fixes are applied to the latest release on the `main` branch.

| Version | Supported |
| ------- | --------- |
| 0.1.x   | ✅        |

## Reporting a vulnerability

Please report security issues **privately** — do not open a public issue for an unfixed
vulnerability.

- Email: **security@noru.tech** with a subject line beginning `[SECURITY] privacy-flow`.
- Or use GitHub **private vulnerability reporting**:
  <https://github.com/noru-tech/privacy-flow/security/advisories/new> (Security →
  *Report a vulnerability*).

Please include: a description of the issue, the affected version or commit, reproduction steps or a
proof of concept, and the impact you foresee. Do not put proprietary source code or personal data
in a report unless it is essential to demonstrate the issue.

We aim to acknowledge reports within **5 business days** and to provide a remediation timeline after
triage. We will credit reporters who wish to be named once a fix is released.

## Scope and threat model

`piiflow` is a local command-line tool that reads source code and writes reports.

- **Repository contents are untrusted data.** Source files are parsed with tree-sitter and never
  executed, imported or evaluated. Configuration and data maps are parsed as data (`serde_json`,
  `serde-saphyr`); JSON Schema references are embedded, and remote schema resolution is not used.
- **No network.** The binary has no network dependency and opens no socket; a test enforces it.
  The only process it starts is `git` (`ls-files`, `rev-parse`, `status`, `ls-tree`, `cat-file`),
  with arguments it constructs; a revision passed to `diff` that starts with `-` is rejected.
- **Bounded work.** Files over 2 MiB are skipped; API paths, provenance sets and the provenance
  fixpoint are bounded; the call-depth bound is at most 64.
- **Outputs** contain file paths, function names and code excerpts of at most 80 characters per
  hop, never values of personal data; excerpts can contain whatever the code contains.
- **In scope:** crashes or hangs on crafted input, writes outside the requested output path, a way
  to make the binary reach the network or run a command, and anything that lets a repository change
  another repository's results.
- **Out of scope:** findings the analysis misses or reports wrongly (open a normal issue with a
  reproduction), and the GitHub Action's use of the runner's tools, which follows GitHub's model.
