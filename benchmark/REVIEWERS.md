# Reviewer guide

For R1 and R2. The rules are in [PROTOCOL.md](PROTOCOL.md) §3; this page says how to apply them.
Label alone: do not discuss items with the other reviewer, and do not open
`benchmark/sheets/key/` or the tools' outputs in `.benchmark-cache/results/` until both of you
have finished every application.

## Where to label

Label in the **labelling desk**, a private claude.ai page whose link you get from whoever runs the
benchmark (you need Contributor access or above).
It shows one item at a time with the code around every step and a link to that line on GitHub at
the pinned commit, takes each answer with one click or key, and saves as you go. Your answers are
stored privately: the other reviewer, the page's owner and Claude cannot read them. They leave the
page only when you press **Export labels**, which saves a `labels-R1-….json` file.

To bring an export into the repository (after both reviewers have finished, or as a backup on your
own branch):

```bash
python3 benchmark/labelling/import_labels.py labels-R1-202610021530.json
```

It writes `benchmark/labels/R1/<app>.yml`, checks every value, and refuses to replace a file with
more labels than the export holds unless you add `--force`. The YAML files can still be edited by
hand, after `python3 benchmark/fetch.py && python3 benchmark/sheets.py` renders the sheets
locally; don't mix the two for the same application.

The desk is rebuilt from the committed items and the fetched applications with
`python3 benchmark/labelling/build.py <dir>`; it never reads `benchmark/sheets/key/`.

## Flow items (`F…`)

A flow claims that data read at the **source** can reach the **sink** call through the listed
steps. You see the same format whoever reported it.

| `verdict` | When |
| --- | --- |
| `tp` | The data can travel from the source to the sink, along these steps or an equivalent path, in some execution the code allows, **and** the source holds personal data. |
| `fp_path` | It cannot: a step does not pass the value on (it is overwritten, only its length is used, it goes to a different call), or the sink does not receive it. `citation`: where the path breaks. |
| `fp_not_personal` | The value does reach the sink, but it is not personal data (a product name, a feature flag, an internal job id). |
| `unsure` | You cannot decide in about five minutes of reading. Say why in `note`. |

- **Personal data** is anything Fideslang files under `user.*`: names, contact details,
  identifiers that point to a person (including a user's database id or UUID), IP addresses,
  credentials, content a user wrote, and so on. Data about organisations is not, unless it
  identifies a person (a one-person company's email is).
- **`category_ok`**: `yes` when at least one of the categories shown is right for the source.
  `user` alone is right whenever the data is personal.
- A flow into a sink that is **not** really a log, third party, LLM, outbound HTTP or browser
  storage (for example, the application's own HTTP response) is `fp_path`.
- Judge reachability, not policy: a flow is `tp` even if the application has a good reason for
  it, such as sending an email address to the email provider.

## Coverage gap items (`G…`)

The analysis says it could not see past the location shown (an unknown library call, a dynamic
call, an unsupported framework, the depth bound).

| `hides_flow` | When |
| --- | --- |
| `yes` | Following the unresolved construct by hand, personal data does reach a sink (as defined above). `citation`: the sink. |
| `no` | It does not reach a sink. |
| `unsure` | You cannot tell in about five minutes. |

## Call site items (`S…`)

A call picked by its name alone. Many are not sinks at all.

1. `is_sink`: `yes` if the call sends data to a log, a third party (SDK or API), an LLM, another
   server over HTTP, or browser storage. `no` for anything else (UI toasts, the application's own
   responses, in-process events, database writes).
2. If it is a sink, `personal`: does personal data reach it, from anywhere in the application, in
   some execution? If `yes`, give `categories` and one `source` (`file:line`) you found.

## Sinks nobody listed

If you meet a sink that receives personal data and is not an item, add it under `added_sites`
at the end of the file. These are reported separately and do not change the sampled estimates.

## Pace

Budget two to three minutes per item; most are quicker, and some take longer. Label one
application at a time, and take breaks: label quality drops after about an hour.
