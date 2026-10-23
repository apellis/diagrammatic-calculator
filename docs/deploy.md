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

Each step is done once, in this order.

1. Cloudflare dashboard, Workers & Pages, Plans: subscribe to Workers Paid ($5 a month).
2. Storage & Databases, D1: create a database named `diagcalc` and copy its database ID.
3. R2: create a bucket named `diagcalc` (location automatic, no public access).
4. My Profile, API Tokens: edit the token stored as `CLOUDFLARE_API_TOKEN` and add the account permissions D1: Edit and Workers R2 Storage: Edit (keep Cloudflare Pages: Edit).
5. GitHub, repository settings, Secrets and variables, Actions: add the secret `D1_DATABASE_ID` with the database ID from step 2.
6. Google Cloud console, Google Auth Platform: create the consent screen (external audience, app name "diagrammatic-calculator", your support address, scopes `openid`, `email`, `profile` only) and publish it.
7. Google Auth Platform, Clients: create an OAuth client of type Web application with the authorized redirect URI `https://diagcalc.ape.computer/api/auth/callback`.
8. Cloudflare, Workers & Pages, `diagcalc`, Settings, Variables and Secrets, Production: add the secrets `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` from step 7.
9. Same place: add the secret `SESSION_SECRET`, a fresh random value of at least 32 characters (for example the output of `openssl rand -base64 48`).
10. Resend: add the domain `send.ape.computer`, and add the DNS records it lists (SPF, DKIM, and its return-path MX) to the `ape.computer` zone in Cloudflare DNS; wait until Resend shows the domain verified.
11. Cloudflare DNS: add a TXT record `_dmarc.send.ape.computer` with the value `v=DMARC1; p=quarantine; adkim=s; aspf=s`.
12. Resend, API Keys: create a key with sending access to `send.ape.computer` only, and add it on the Pages project as the secret `RESEND_API_KEY` (as in step 8).
13. GitHub, Actions, "Deploy web app": run the workflow; its summary should say "Accounts: D1 database and R2 bucket bound, migrations applied."

Secrets set in steps 8, 9 and 12 take effect with the next deployment, which step 13 makes. Without
`RESEND_API_KEY` everything works except that sharing sends no e-mail. The sender address is the
`EMAIL_FROM` variable in `web/wrangler.toml`.

## What the deploy does

- Builds the app as before.
- Without `D1_DATABASE_ID`: removes `web/wrangler.toml` and deploys the site and functions with no
  bindings.
- With it: writes the ID into `web/wrangler.toml`, applies the migrations in `web/migrations/` with
  `wrangler d1 migrations apply diagcalc --remote` (wrangler records which have run), then deploys
  with the D1 binding `DB`, the R2 binding `FILES` and the variable `EMAIL_FROM`.

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
