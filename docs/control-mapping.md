# Control mapping

Where `piiflow`'s findings and flow facts land in the frameworks a privacy or security programme
is assessed against. Each entry is an identifier with Noru's own short gloss. No normative text
from any standard is quoted.

> **Status: pending review.** This mapping must be checked by a privacy lawyer or Noru's
> compliance lead before the first release (v0.1.0). Until then it is a working draft: a guide
> for control owners, never a compliance claim. A finding is evidence that a flow exists in the
> code, not a determination that processing is unlawful, and the absence of findings is evidence
> only about the scanned scope, with its coverage gaps listed.

## By rule

| Rule | Maps to | Gloss | Typical response |
| --- | --- | --- | --- |
| [PF001](rules/PF001.md) Personal data reaches a log sink | GDPR Art. 5(1)(c); Art. 32; ISO/IEC 27001:2022 A.8.15 | Data minimisation and security of processing: logs are a second, less protected store. A.8.15: what logs record is in scope of logging controls | Remove or redact; or record why the log line is needed and how the log store is protected |
| [PF002](rules/PF002.md) Undeclared third-party processor | GDPR Art. 28; Art. 30 | A processor needs a contract; recipients belong in the record of processing | Declare the processor with a citation to its DPA, or stop the flow |
| [PF003](rules/PF003.md) Personal data reaches an LLM provider | GDPR Art. 28; Art. 30; ISO/IEC 42001:2023; EU AI Act Art. 10 | The model provider is a processor and recipient; data reaching an AI system is in scope of its governance | Minimise the prompt, or record the DPIA or AI register entry as a disposition |
| [PF004](rules/PF004.md) Special-category data reaches an external sink | GDPR Art. 9; Art. 10 | Special categories and criminal-offence data need a specific condition before any processing, including disclosure | Stop the flow, or record the Art. 9(2) condition (or Art. 10 authority) |
| [PF005](rules/PF005.md) Credentials reach a log or third party | ISO/IEC 27001:2022 A.5.17, A.8.15; SOC 2 CC6.1 | Authentication information is protected through its life cycle; logs and vendors are not where it should live | Remove, then rotate what was exposed |
| [PF006](rules/PF006.md) Outbound HTTP with a non-literal host | GDPR Art. 30; Art. 44 | The record names recipients; a transfer needs a basis, and an unknown host is an unknown country | Make the host explicit, or record the recipient and transfer basis |
| [PFC01](rules/PFC01.md) Coverage gap | none | Makes the other mappings' absence of findings meaningful | Teach the catalogue, or review and record the gap |

## Flow facts as Article 30 input

Each flow fact says which category of personal data leaves through which code path to which
processor, cited to `file:line:column`, and the `egress` section aggregates them per processor.
That is input to an Article 30 record (recipients, categories of data), not the record itself:
purposes, legal bases, data subjects and retention are judgements outside this tool, and turning
flow facts into Article 30 records is Noru's side of the [open/closed line](../NOTES.md).

## Not covered

`piiflow` is not a security vulnerability scanner (injection, SSRF and the like are out of scope),
does not inspect data at rest or in databases, and does not observe the running system. See
[KNOWN-LIMITATIONS.md](../KNOWN-LIMITATIONS.md).
