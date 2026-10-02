# Corpus selection

Fixed on 2026-10-02, before `piiflow` was run on any candidate. Licences, activity and size come
from the GitHub API and the fetched trees; sink use comes from each application's dependency
manifest, not from `piiflow`.

## Criteria

1. **Licence:** MIT, Apache-2.0 or BSD for the whole analysed tree. Repositories that mix a
   permissive licence with a source-available or enterprise licence are excluded, even when the
   permissive part could be separated, so that the corpus and its labels can be redistributed
   without a judgement call.
2. **Application**, deployable or a starter template meant to be deployed, not a library or
   framework.
3. **At least one sink class** in use (logging counts), according to its manifest.
4. **Active:** a commit within the last 24 months.
5. **Size:** 1,000 to 200,000 lines in the analysed language and scope (tests, generated code and
   other languages' frontends excluded).
6. **Held out:** not used while developing `piiflow` (excludes documenso and Netflix Dispatch).
7. **Balance:** six TypeScript/JavaScript and six Python applications, covering supported
   frameworks (Next.js, Express, Django, Flask, FastAPI) and, deliberately, two that are not
   modelled (NestJS, Wasp), so the benchmark shows what happens outside the supported set.

## Chosen

| Application | Licence | Scope | Lines in scope | Framework | Sinks in the manifest |
| --- | --- | --- | --- | --- | --- |
| vercel/chatbot | Apache-2.0 | `.` | 20k TS | Next.js | AI SDK (LLM) |
| umami-software/umami | MIT | `.` | 103k TS | Next.js | logging |
| shadcn-ui/taxonomy | MIT | `.` | 8k TS | Next.js | nodemailer, Postmark, Stripe |
| wasp-lang/open-saas | MIT | `template/app` | 11k TS | Wasp (not modelled) | OpenAI, Stripe, Lemon Squeezy, AWS SDK |
| TryGhost/Ghost | MIT | `ghost/core` | 161k JS/TS | Express | Sentry, Mailgun, nodemailer, Stripe, Slack, AWS SDK |
| hoppscotch/hoppscotch | MIT | `packages/hoppscotch-backend` | 27k TS | NestJS (not modelled) | nodemailer, PostHog |
| healthchecks/healthchecks | BSD-3-Clause | `.` | 17k Py | Django | SMTP, outbound HTTP integrations |
| getredash/redash | BSD-2-Clause | `.` (no `client/`, `viz-lib/`) | 27k Py | Flask | Sentry, requests, boto3 |
| CTFd/CTFd | Apache-2.0 | `.` (no `CTFd/themes/`) | 23k Py | Flask | requests, boto3, SMTP |
| fastapi/full-stack-fastapi-template | MIT | `backend` | 1.5k Py | FastAPI | emails, httpx, Sentry |
| polarsource/polar | Apache-2.0 | `server` | 196k Py | FastAPI | Stripe, PostHog, OpenAI, Sentry, Logfire, httpx, boto3 |
| zylon-ai/private-gpt | Apache-2.0 | `.` | 96k Py | FastAPI | OpenAI, Anthropic, LlamaIndex, LangChain, httpx, boto3 |

## Considered and not chosen

| Repository | Reason |
| --- | --- |
| documenso/documenso | AGPL-3.0; used in development |
| Netflix/dispatch | used in development; archived |
| langfuse/langfuse, medusajs/medusa, novuhq/novu, Infisical/infisical, formbricks/formbricks, lobehub/lobehub, twentyhq/twenty, directus/directus, nocodb/nocodb, strapi/strapi, Budibase/budibase, RocketChat/Rocket.Chat, elie222/inbox-zero, baserow/baserow, pretix/pretix, PostHog/posthog, open-webui/open-webui, The-Vibe-Company/quivr | mixed or non-permissive licence (criterion 1) |
| makeplane/plane, mealie-recipes/mealie, gitroomhq/postiz-app, plausible/analytics | AGPL-3.0 |
| logto-io/logto | MPL-2.0 (weak copyleft; criterion 1) |
| payloadcms/payload, getsentry/sentry-python | framework or library (criterion 2) |
| calcom/cal.diy, apache/superset, zulip/zulip, supabase/supabase, HumanSignal/label-studio, langflow-ai/langflow, appsmithorg/appsmith, jitsi/jitsi-meet | over 200k lines in scope (criterion 5) |
| hoppscotch frontend, excalidraw/excalidraw | mostly client-side or Vue single-file components, which `piiflow` does not analyse; the corpus measures server code |
| mckaywrigley/chatbot-ui | last commit 2024-08 (criterion 4 met, but superseded by vercel/chatbot for the same pattern) |
| nextjs/saas-starter, ixartz/SaaS-Boilerplate, vercel/commerce | overlap with taxonomy (Next.js + Stripe) at smaller size; one representative chosen |
| readthedocs/readthedocs.org, inventree/InvenTree, netbox-community/netbox, wagtail/wagtail | Django coverage already provided by healthchecks; kept as reserves if a chosen repository becomes unavailable |
