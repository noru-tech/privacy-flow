#!/usr/bin/env python3
"""Generate a synthetic TypeScript service of about N lines, deterministically.

Used by the performance budget (`.github/scripts/perf_budget.py`) and for local measurement:

    python3 benches/synthetic.py /tmp/synthetic 100000
    piiflow scan /tmp/synthetic --walk --timings -o /dev/null

The shape mimics a service layer: modules that import each other, typed domain objects with
personal fields, helpers that format and return them, classes with injected clients, and calls
into logs, analytics and LLM SDKs. It has no randomness: the same N always gives the same files.
"""

import pathlib
import sys

TEMPLATE = """import pino from 'pino';
import {{ PostHog }} from 'posthog-node';
import OpenAI from 'openai';
import {{ helper{prev} }} from './m{prev}';
import {{ Service{prev2} }} from './m{prev2}';

const logger = pino();
const posthog = new PostHog('key');
const openai = new OpenAI();

export interface Account{i} {{ id: string; email: string; phone_number: string; plan: string; createdAt: string }}

export function helper{i}(a: Account{i}): string {{
  const label = `${{a.plan}}-${{a.id}}`;
  return label + (a.createdAt ?? '');
}}

export function contact{i}(a: Account{i}): string {{
  return `${{a.email}} / ${{a.phone_number}}`;
}}

export class Service{i} {{
  private readonly cache = new Map<string, string>();

  constructor(private readonly prefix: string) {{}}

  record(a: Account{i}) {{
    this.cache.set(a.id, helper{prev}(a as never));
    logger.info({{ id: a.id, plan: a.plan }});
    if (a.plan === 'debug') {{
      logger.debug(contact{i}(a));
    }}
    return this.cache.get(a.id);
  }}

  async summarize(a: Account{i}) {{
    const prompt = `Summarize account ${{a.id}} on ${{a.plan}}`;
    const res = await openai.chat.completions.create({{ model: 'gpt-4o', messages: [{{ role: 'user', content: prompt }}] }});
    posthog.capture({{ distinctId: a.id, event: 'summarized', properties: {{ plan: a.plan }} }});
    return res;
  }}
}}

export function wire{i}(a: Account{i}) {{
  const s = new Service{i}('m{i}');
  const t = new Service{prev2}('m{prev2}');
  s.record(a);
  t.record(a as never);
  const items = [a, a].map((x) => helper{i}(x));
  return items.join(',');
}}
"""


def main():
    out = pathlib.Path(sys.argv[1])
    target = int(sys.argv[2]) if len(sys.argv) > 2 else 100_000
    lines_per = TEMPLATE.count("\n") + 1
    n = max(3, target // lines_per)
    out.mkdir(parents=True, exist_ok=True)
    for i in range(n):
        prev = (i - 1) % n
        prev2 = (i * 7 + 3) % n
        (out / f"m{i}.ts").write_text(TEMPLATE.format(i=i, prev=prev, prev2=prev2))
    print(f"wrote {n} modules, about {n * lines_per} lines, to {out}")


if __name__ == "__main__":
    main()
