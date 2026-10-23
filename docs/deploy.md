# Deploying the web app and accounts

The web app is a static site on Cloudflare Pages (project `diagcalc`), deployed by
`.github/workflows/deploy-web.yml` on every push to `master`. The accounts API (phase 6) is a set of
Pages Functions in `web/functions/`, deployed with the site; it keeps its data in a D1 database and
an R2 bucket and sends share notifications through Resend. Until the steps below are done the
deploy skips the accounts part with a line in the run summary, the API answers that accounts are
not available, and the app stays anonymous exactly as before.

Cloudflare Access stays in front of the whole site, API included. While it does, people pass Access
and then sign in to the app, and someone a document is shared with must also be on the Access
allowlist to open it. Removing Access is part of the public release.

## Steps for Alex

The deploy workflow provisions what it can itself (see "What the deploy does"); these are the steps
that need a person, done once.

1. Cloudflare, My Profile, API Tokens: edit the token stored as `CLOUDFLARE_API_TOKEN` and add the
   account permissions D1: Edit and Workers R2 Storage: Edit, and the zone permission DNS: Edit for
   `ape.computer` (keep Cloudflare Pages: Edit). If R2 was never used on the account, open R2 in the
   dashboard once and enable it.
2. Google Cloud console, Google Auth Platform: create the consent screen (external audience, app
   name "diagrammatic-calculator", your support address, scopes `openid`, `email`, `profile` only)
   and publish it. Then Clients: create an OAuth client of type Web application with the authorized
   redirect URI `https://diagcalc.ape.computer/api/auth/callback`.
3. Cloudflare, Workers & Pages, `diagcalc`, Settings, Variables and Secrets, Production: add the
   secrets `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` from step 2.
4. Resend: add the domain `send.ape.computer` and put the records it lists (SPF, DKIM, return-path
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

## What the deploy does

- Builds the app as before.
- Runs `.github/scripts/provision-accounts.sh`, which creates, if missing and if the token allows:
  the D1 database `diagcalc`, the R2 bucket `diagcalc`, the Pages secret `SESSION_SECRET` (a fresh
  random value, never printed; an existing one is kept, since changing it signs everyone out) and
  the TXT record `_dmarc.send.ape.computer` = `v=DMARC1; p=quarantine; adkim=s; aspf=s`. It also
  reports which of the other secrets are set. Nothing in it fails the run.
- If the database and bucket exist: writes the database ID (looked up by name, or the secret
  `D1_DATABASE_ID` if set) into `web/wrangler.toml`, applies the migrations in `web/migrations/`
  with `wrangler d1 migrations apply diagcalc --remote` (wrangler records which have run), then
  deploys with the D1 binding `DB`, the R2 binding `FILES` and the variable `EMAIL_FROM`.
- Otherwise: removes `web/wrangler.toml` and deploys the site and functions with no bindings.

A migration is never edited once it is on `master`; a schema change is the next numbered file.

## Running the accounts API locally

From `web/`, after `npm ci`, `npm run wasm` and `npm run build`:

```sh
printf 'GOOGLE_CLIENT_ID=...\nGOOGLE_CLIENT_SECRET=...\nSESSION_SECRET=...\n' > .dev.vars   # not committed
npx wrangler@4.149.0 d1 migrations apply diagcalc --local
npx wrangler@4.149.0 pages dev dist --port 8788
```

Signing in locally needs `http://localhost:8788/api/auth/callback` among the OAuth client's
redirect URIs. The unit tests (`npm run test:server`) need none of this: they run the API against
Miniflare's D1 and R2 with a fake Google and a fake Resend.
