# Rules

Every identifier `piiflow` reports has a stable page here. Identifiers are never reused; a retired
rule keeps its ID and its page.

| ID | Finding | Default severity | Maps to |
| --- | --- | --- | --- |
| [PF001](PF001.md) | Personal data reaches a log sink | medium | GDPR Art. 5(1)(c), Art. 32; ISO/IEC 27001 A.8.15 |
| [PF002](PF002.md) | Personal data reaches a third-party processor not declared in config | high | GDPR Art. 28, Art. 30 |
| [PF003](PF003.md) | Personal data reaches an LLM provider | medium | GDPR Art. 28, Art. 30; ISO/IEC 42001; EU AI Act Art. 10 |
| [PF004](PF004.md) | Special-category data (Art. 9 or 10) reaches an external sink | high | GDPR Art. 9, Art. 10 |
| [PF005](PF005.md) | Credentials or authentication data reach a log or third party | high | ISO/IEC 27001 A.5.17, A.8.15; SOC 2 CC6.1 |
| [PF006](PF006.md) | Personal data reaches outbound HTTP with a non-literal host | warning | GDPR Art. 30, Art. 44 |
| [PFC01](PFC01.md) | Coverage gap: unsupported framework, dynamic dispatch or depth bound reached | warning | none |

Severities order `info < warning < medium < high`. With the default `fail_on: medium`, `high` and
`medium` findings fail `piiflow check`; `warning` and `info` do not. A policy in
[`.privacy-flow.yml`](../configuration.md#policy) can change any rule's severity or disable it,
except PFC01: an open coverage gap always makes `check` exit 4, whatever the threshold.

## How findings come from flows

A finding is a judgement about a **flow**: one source (where personal data enters a value), one
sink (where it leaves), one category, and the full chain of `file:line:column` hops between them.
The rules read only the flow facts in the output document, the embedded policy and the declared
processors, so `piiflow validate` recomputes every finding and rejects a document whose findings
do not follow. One flow can produce more than one finding (an email address sent to an undeclared
LLM provider is PF002 and PF003): they are different obligations.

Two adjustments apply to every rule:

- **Review queue.** When the source is a name the classification table marks as *maybe* personal
  (`name`, `payload`, `title`, `value` …), the finding is `info` and `needs_review: true`.
  Record names that are not personal in your codebase under
  [`not_personal`](../configuration.md#not_personal), with a citation.
- **Heuristic sinks.** A logger recognised only by its receiver's name (`this.logger.info(...)`
  where the logger was injected) is marked `heuristic: true`; the finding keeps its severity.

## Recording a decision

Findings are never deleted. A human decision (`accepted`, `remediated` or `false_positive`, with
owner, date, rationale and optional expiry) goes in the finding's `disposition`, and
`piiflow check FILE --as-of DATE` honours it on that date ([dispositions](../output.md#dispositions)).

## Exit codes

0 success, 1 policy threshold exceeded, 2 usage, 3 invalid input, 4 coverage incomplete:
[exit codes](../exit-codes.md).
