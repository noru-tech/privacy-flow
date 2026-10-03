# Benchmark results

## piiflow and Privado, head to head (10 applications both completed)

| Question | piiflow | Privado |
| --- | --- | --- |
| Of the flows a tool reports, how many are real? | 72 % [65–79 %] (n=127) | 22 % [2–41 %] (n=103) |
| ...counting only piiflow's findings (not its maybe-personal ones) | 65 % [53–77 %] (n=71) | |
| Of the sinks that really receive personal data, how many does it find? | 32 % [24–42 %] (n=108) | 17 % [11–25 %] (n=108) |
| ...or flags as a place it could not see | 40 % [31–49 %] (n=108) | |
| ...counting only a flow's sink on the line (the original rule) | 31 % [23–40 %] (n=108) | 14 % [9–22 %] (n=108) |

A sink is found when a flow ends on its line or enters a call on its line that leads to a sink (PROTOCOL.md, Deviations, 2026-10-03).

Of 108 sampled sinks that receive personal data: both tools found 10, only piiflow 25, only Privado 8, neither 65.

## Detail

Proportions with 95 % Wilson intervals; see PROTOCOL.md.

## Precision

| | Pooled | Weighted |
| --- | --- | --- |
| piiflow, all sampled flows | 61 % [52–69 %] (n=124) | |
| Privado, all sampled flows | 18 % [12–27 %] (n=103) | |
| piiflow PF001 | 68 % [47–84 %] (n=22) | 0.6578 |
| piiflow PF002 | 73 % [48–89 %] (n=15) | 0.7235 |
| piiflow PF003 | 50 % [19–81 %] (n=6) | 0.5 |
| piiflow PF004 | — | — |
| piiflow PF005 | 62 % [39–82 %] (n=16) | 0.5385 |
| piiflow PF006 | 67 % [39–86 %] (n=12) | 0.5444 |
| piiflow maybe-personal (info) | 54 % [41–66 %] (n=56) | 0.7627 |
| privado:internal_apis | 6 % [1–28 %] (n=16) | 0.1007 |
| privado:leakages | 23 % [13–37 %] (n=44) | 0.1905 |
| privado:third_parties | 19 % [10–33 %] (n=43) | 0.2376 |

## Recall on sampled sink sites that receive personal data

| Scope | piiflow found | piiflow found or flagged | Privado found |
| --- | --- | --- | --- |
| all | 32 % [24–42 %] (n=108) | 40 % [31–49 %] (n=108) | 17 % [11–25 %] (n=108) |
| typescript | 28 % [19–39 %] (n=72) | 36 % [26–48 %] (n=72) | 8 % [4–17 %] (n=72) |
| python | 42 % [27–58 %] (n=36) | 47 % [32–63 %] (n=36) | 33 % [20–50 %] (n=36) |

Coverage gaps that hide a real flow: 29 % [21–39 %] (n=89).

`unsure` labels excluded: {'flow': 23, 'gap': 5, 'site': 19}. Sites added by reviewers: 0.

Not scored (not labelled by both reviewers): polar, private-gpt.
