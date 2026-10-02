# Demo

A sign-up function that logs, tracks and prompts. `piiflow scan examples/demo -f table` reports
an email address in a log line (PF001), the same email address sent to PostHog (PF002), and a
phone number that reaches OpenAI through a helper in another file (PF002, PF003). The helper does
not use the email address, so the email address does not reach the model.
