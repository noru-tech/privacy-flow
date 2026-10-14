#!/usr/bin/env python3
"""Generate a synthetic TypeScript (or C#) service of about N lines, deterministically.

Used by the performance budget (`.github/scripts/perf_budget.py`) and for local measurement:

    python3 benches/synthetic.py /tmp/synthetic 100000
    python3 benches/synthetic.py /tmp/synthetic-cs 100000 --lang cs
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


# The same shape in C#: a namespace per module, records with personal fields, services with an
# injected logger and HTTP client, a minimal API endpoint and a MediatR request.
CS_TEMPLATE = """using MediatR;
using Shop.M{prev};
using Shop.M{prev2};

namespace Shop.M{i};

public record Account{i}(string Id, string Email, string PhoneNumber, string Plan, DateTime CreatedAt);

public record Summarize{i}(Account{i} Account) : IRequest<string>;

public static class Helpers{i}
{{
    public static string Label(Account{i} a) => $"{{a.Plan}}-{{a.Id}}" + a.CreatedAt;

    public static string Contact(this Account{i} a) => $"{{a.Email}} / {{a.PhoneNumber}}";
}}

public class Service{i}(ILogger<Service{i}> logger, HttpClient http, Service{prev2} other)
{{
    private readonly Dictionary<string, string> cache = new();

    public string? Record(Account{i} a)
    {{
        cache[a.Id] = Helpers{i}.Label(a);
        logger.LogInformation("recorded {{Id}} on {{Plan}}", a.Id, a.Plan);
        if (a.Plan == "debug")
        {{
            logger.LogDebug("contact {{Contact}}", a.Contact());
        }}
        return cache.GetValueOrDefault(a.Id);
    }}

    public async Task Sync(Account{i} a)
    {{
        await http.PostAsJsonAsync("/accounts", new {{ a.Id, a.Plan }});
        other.Touch(a.Id);
    }}

    public void Touch(string id) => logger.LogTrace("touched {{Id}}", id);
}}

public class Summarize{i}Handler(ILogger<Summarize{i}Handler> logger) : IRequestHandler<Summarize{i}, string>
{{
    public Task<string> Handle(Summarize{i} request, CancellationToken ct)
    {{
        logger.LogInformation("summarizing {{Id}}", request.Account.Id);
        return Task.FromResult(request.Account.Plan);
    }}
}}

public static class Endpoints{i}
{{
    public static void Map{i}(this IEndpointRouteBuilder app)
    {{
        app.MapPost("/m{i}/accounts", async (Account{i} account, Service{i} service, ISender sender) =>
        {{
            service.Record(account);
            await service.Sync(account);
            return await sender.Send(new Summarize{i}(account));
        }});
    }}
}}
"""

CS_PROJECT = """<Project Sdk="Microsoft.NET.Sdk.Web">
  <PropertyGroup>
    <TargetFramework>net9.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
  </PropertyGroup>
</Project>
"""


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    lang = "cs" if "--lang" in sys.argv and sys.argv[sys.argv.index("--lang") + 1] == "cs" else "ts"
    args = [a for a in args if a != "cs"]
    out = pathlib.Path(args[0])
    target = int(args[1]) if len(args) > 1 else 100_000
    template = CS_TEMPLATE if lang == "cs" else TEMPLATE
    lines_per = template.count("\n") + 1
    n = max(3, target // lines_per)
    out.mkdir(parents=True, exist_ok=True)
    if lang == "cs":
        (out / "Shop.csproj").write_text(CS_PROJECT)
    for i in range(n):
        prev = (i - 1) % n
        prev2 = (i * 7 + 3) % n
        name = f"M{i}.cs" if lang == "cs" else f"m{i}.ts"
        (out / name).write_text(template.format(i=i, prev=prev, prev2=prev2))
    print(f"wrote {n} modules, about {n * lines_per} lines, to {out}")


if __name__ == "__main__":
    main()
