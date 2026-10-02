# Exit codes

`piiflow` exits with one of five stable codes. They are part of the public interface: a code's
meaning never changes, and a new condition gets a new code rather than reusing one. The codes are
defined once, in `Exit` in [`src/lib.rs`](../src/lib.rs), and a unit test pins each value.

| Code | Meaning | Returned by |
| --- | --- | --- |
| [0](#0-success) | Success, or the policy threshold passed | every command |
| [1](#1-policy-threshold-exceeded) | Policy threshold exceeded | `check` |
| [2](#2-invalid-command-line-arguments) | Invalid command-line arguments | every command |
| [3](#3-invalid-input) | Invalid input: configuration, document or file | every command that reads one |
| [4](#4-coverage-incomplete) | Coverage incomplete: a clean result cannot be claimed (takes precedence over 1) | `scan`, `diff`, `check` |

Findings never change the exit code of `scan` or `diff`: they write their output and exit 0, or
4 when coverage is incomplete. Only `check` enforces the policy threshold. The
[GitHub Action](github-action.md) treats 0, 1 and 4 as verdicts (it still uploads SARIF and writes
the job summary) and any other code as a failure of the run itself.

## 0: Success

The command did what it was asked: a document was written, a document validated, or `check`
found no finding at or above the policy's `fail_on` severity (default `medium`) without a
disposition, and no open coverage gap.

```console
$ piiflow -q scan tests/fixtures/ts/pf002-pass > /dev/null; echo $?
0
```

## 1: Policy threshold exceeded

`check` found at least one finding (other than a coverage gap) whose severity is at or above
`fail_on` and that no disposition suppresses on the `--as-of` date. Each finding names its rule;
see the [rule pages](rules/README.md) for what it means and how to fix it or record a disposition.

```console
$ piiflow -q scan tests/fixtures/ts/pf002-fail -o flows.json
$ piiflow check flows.json > /dev/null; echo $?
1
```

## 2: Invalid command-line arguments

The command line could not be used: an unknown subcommand or flag, a bad value (`--format nope`),
a path that is not a directory, a malformed revision range (`diff HEAD`), a revision that is not a
commit, `--as-of` that is not a calendar date, `check` on a document with non-open dispositions but
no `--as-of`, `--threads 0`, or an `in-toto` output requested for a tree that is not a Git commit.
The message says which.

```console
$ piiflow diff HEAD; echo $?
error: expected BASE..HEAD or BASE.., got "HEAD"
2
```

## 3: Invalid input

An input could not be used: `.privacy-flow.yml` is not valid YAML, does not conform to
[its schema](../schemas/config.schema.json) (a missing citation, an unknown key, `PFC01`
disabled), or names a category that is not in the vendored Fideslang taxonomy; a data map is
missing or malformed; or a flow-facts document does not validate (schema, digest, findings that
do not follow from its flows, a malformed disposition). No output is written for invalid input.

```console
$ printf 'version: 1\nprocessors:\n  - name: X\n' > .privacy-flow.yml
$ piiflow scan . ; echo $?
error: .privacy-flow.yml does not conform to its schema:
  /processors/0: "citation" is a required property
3
```

## 4: Coverage incomplete

The analysis could not see everywhere personal data might go: personal data reached a call it
cannot resolve, the depth bound cut a witness short, the code imports a web framework whose
request input is not modelled, a file did not parse, or the repository has source files in a
language that is not analysed. The output is still written, with every gap in `coverage.gaps`
and a PFC01 finding for each, but a clean result cannot be claimed.

`scan` and `diff` exit 4 whenever there is a gap. `check` exits 4 while any PFC01 finding has no
disposition, whatever `--fail-on` says, and 4 takes precedence over 1: an incomplete analysis is
never reported as a plain pass or failure.

```console
$ piiflow -q scan tests/fixtures/ts/unsupported-framework > /dev/null; echo $?
4
```

Resolve the gap (teach the catalogue the SDK, raise `max_call_depth`), or review it and record a
disposition on its PFC01 finding ([PFC01](rules/PFC01.md)). See
[known limitations](../KNOWN-LIMITATIONS.md).

---

[Rules](rules/README.md) · [Configuration](configuration.md) · [README](../README.md#output-formats-and-exit-codes)
