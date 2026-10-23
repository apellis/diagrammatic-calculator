# Deploying the web app and accounts

The web app is a static site on Cloudflare Pages (project `diagcalc`), deployed by
`.github/workflows/deploy-web.yml` on every push to `master`. The accounts API (phase 6) is a set of
Pages Functions in `web/functions/`, deployed with the site; it keeps its data in a D1 database and
an R2 bucket and sends share notifications through Resend. Until the steps below are done the
deploy skips the accounts part with a line in the run summary, the API answers that accounts are
not available, and the app stays anonymous exactly as before.

The site is served without Cloudflare Access (Alex, 23 October): anyone can use it anonymously,
and Google sign-in is the only authentication. The per-deployment addresses
(`<hash>.diagcalc.pages.dev`) stay behind Access ("Preview access: restrict previews" in the Pages
project's settings, with its Zero Trust application for `*.diagcalc.pages.dev`). Until the Google consent screen is published, only
its test users can sign in.

## Steps for Alex

The deploy workflow provisions what it can itself (see "What the deploy does"); these are the steps
that need a person, done once.

1. Cloudflare, My Profile, API Tokens: edit the token stored as `CLOUDFLARE_API_TOKEN` and add the
   account permissions D1: Edit and Workers R2 Storage: Edit (called "Write" in the new token
   editor; keep Cloudflare Pages: Edit). Once `ape.computer` is on Cloudflare, also the zone
   permission DNS: Edit for it, for the DMARC record. If R2 was never used on the account, open R2 in the
   dashboard once and enable it.
2. Google Cloud console, Google Auth Platform: create the consent screen (external audience, app
   name "diagrammatic-calculator", your support address, scopes `openid`, `email`, `profile` only).
   While Access is on, leave it in Testing and add the allowed people as test users; publishing
   needs a homepage and privacy policy on a verified domain and waits for the public release. Then Clients: create an OAuth client of type Web application with the authorized
   redirect URI `https://diagcalc.pages.dev/api/auth/callback` (the redirect URI is built from the
   variable `APP_ORIGIN` in `web/wrangler.toml`; when the custom domain moves over, add
   `https://diagcalc.ape.computer/api/auth/callback` here first, then change `APP_ORIGIN`).
3. Cloudflare, Workers & Pages, `diagcalc`, Settings, Variables and Secrets, Production: add the
   secrets `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` from step 2.
4. (Share e-mails only; can wait for the custom domain.) Resend: add the domain `send.ape.computer` and put the records it lists (SPF, DKIM, return-path
   MX) into the `ape.computer` zone (Resend's automatic Cloudflare setup does this); wait until it
   shows verified. Then API Keys: create a key with sending access to `send.ape.computer` only, and
   add it as the Pages secret `RESEND_API_KEY` (as in step 3).
5. GitHub, Actions, "Deploy web app": run the workflow. Its summary lists each part; the last line
   should say "Accounts: D1 database and R2 bucket bound, migrations applied."

Workers Paid ($5 a month) should not be needed yet: D1, R2 and Pages Functions have free
allowances far above this app's use, and the API does little computation per request (the free
plan's limit is 10 ms of CPU per request). It is needed for Durable Objects (real-time co-editing,
phase 7), or earlier if the API ever reports exceeded CPU limits.

Secrets take effect with the next deployment, which step 5 makes. Without `RESEND_API_KEY`
everything works except that sharing sends no e-mail. The sender address is the `EMAIL_FROM`
variable in `web/wrangler.toml`.

`APP_ORIGIN` in `web/wrangler.toml` is the site's address (now `https://diagcalc.pages.dev`). The
API answers only requests that reach it there (others get 421), and sign-in redirects and links in
share e-mails use it. Every deployment also has an address of its own (`<hash>.diagcalc.pages.dev`);
those keep serving the static app but not the API.

## What the deploy does

- Builds the app in a job that has no access to the secrets, and passes the built site to the
  deploy job, which installs Wrangler (without install scripts) before any secret is in its
  environment. Every action is pinned to a commit; Dependabot proposes updates.
- Runs `.github/scripts/provision-accounts.sh`, which creates, if missing and if the token allows:
  the D1 database `diagcalc`, the R2 bucket `diagcalc`, the Pages secret `SESSION_SECRET` (a fresh
  random value, never printed; an existing one is kept, since changing it signs everyone out) and
  the TXT record `_dmarc.send.ape.computer` = `v=DMARC1; p=quarantine; adkim=s; aspf=s`. It also
  reports which of the other secrets are set. Nothing in it fails the run.
- If the database and bucket exist: writes the database ID (looked up by name, or the secret
  `D1_DATABASE_ID` if set) into `web/wrangler.toml`, applies the migrations in `web/migrations/`
  with `wrangler d1 migrations apply diagcalc --remote` (wrangler records which have run), then
  deploys with the D1 binding `DB`, the R2 binding `FILES` and the variables `EMAIL_FROM` and
  `APP_ORIGIN`. Preview deployments (any branch but `master`) get no bindings (`[env.preview]`).
- Otherwise: removes `web/wrangler.toml` and deploys the site and functions with no bindings.

A migration is never edited once it is on `master`; a schema change is the next numbered file.

## Running the accounts API locally

From `web/`, after `npm ci`, `npm run wasm` and `npm run build`:

```sh
printf 'GOOGLE_CLIENT_ID=...\nGOOGLE_CLIENT_SECRET=...\nSESSION_SECRET=...\nAPP_ORIGIN=http://localhost:8788\n' > .dev.vars   # not committed
npx wrangler@4.149.0 d1 migrations apply diagcalc --local
npx wrangler@4.149.0 pages dev dist --port 8788
```

Signing in locally needs `http://localhost:8788/api/auth/callback` among the OAuth client's
redirect URIs. The unit tests (`npm run test:server`) need none of this: they run the API against
Miniflare's D1 and R2 with a fake Google and a fake Resend.

## Hardening that needs a person

These settings live in the Cloudflare and GitHub dashboards, not in the repository.

- **Old deployments.** Every deployment keeps its own files and server code, bound to the
  production database and storage, at its own `<hash>.diagcalc.pages.dev` address. The site is
  served without Cloudflare Access, so delete old ones: GitHub, Actions, "Prune old deployments",
  Run workflow (first with "dry run" to see the list), or push a change to
  `.github/prune-deployments` whose first word is `list` or `delete`. It keeps the production
  deployment and anything newer.
- **Revoked Google access (Cross-Account Protection).** Without this, a session ends only at its
  limits (30 days, or 7 without use) or when the person signs out. With it, Google tells the API
  when it ends a person's sessions, when they remove this app's access in their Google account, or
  when it disables their account, and the API ends all their sessions here (`POST
  /api/google/risc`; nothing about the event is stored). Once, in the Google Cloud project of the
  OAuth client: APIs & Services, enable the RISC API (accept its terms if asked); IAM & Admin,
  Service accounts, create one with the role RISC Configuration Admin and a JSON key for it. Then,
  from `web/`, run `node scripts/risc-register.mjs <key.json> https://diagcalc.pages.dev`: it
  registers `<site>/api/google/risc` as the receiver and asks Google for a verification event.
  Delete the key afterwards (or keep it outside the repository); run the script again with a new
  key when `APP_ORIGIN` changes. Without a terminal: paste the key's whole JSON into a repository
  secret `GOOGLE_RISC_KEY` (GitHub, Settings, Secrets and variables, Actions), run the workflow
  "Register Google security events" (Actions, Run workflow), check its notices, then delete the
  secret and the key.
- **Rate limit.** Cloudflare, the zone, Security, WAF, Rate limiting rules (once the site is on the
  custom domain; `pages.dev` cannot have rules): one rule on URI path starting with `/api/` and
  method not GET, for example 60 requests per minute per IP, action Block for 1 minute.
- **Separate tokens.** Instead of one `CLOUDFLARE_API_TOKEN` with every permission, keep the deploy
  token at Cloudflare Pages: Edit plus D1: Edit, lower Workers R2 Storage to Read once the bucket
  exists, and remove DNS: Edit once the DMARC record exists (the run summary says when they do).
  R2 Read must stay: the deploy checks with it that the bucket exists, and without it would deploy
  the site with accounts switched off. The app itself never uses this token.
- **Protected environment.** The deploy job runs in the GitHub environment `production`. GitHub,
  Settings, Environments, `production`: under Deployment branches allow `master` only. The secrets
  can stay repository secrets (a job in the environment reads both); moving them into the
  environment would also hide them from other workflows, but needs their values, so it can wait
  until the token is next rotated. Settings, Branches: protect `master` (no force pushes, no
  deletion).
