# Deploying the web app and accounts

The web app is a static site at `https://diagcalc.pages.dev` on Cloudflare Pages (project `diagcalc`), deployed by
`.github/workflows/deploy-web.yml` on every push to `master`. The accounts API (phase 6) is a set of
Pages Functions in `web/functions/`, deployed with the site; it keeps its data in a D1 database and
an R2 bucket and sends share notifications through Resend. Until the steps below are done the
deploy skips the accounts part with a line in the run summary, the API answers that accounts are
not available, and the app stays anonymous exactly as before.

The site is served without Cloudflare Access (Alex, 23 October): anyone can use it anonymously,
and Google sign-in is the only authentication. The per-deployment addresses
(`<hash>.diagcalc.pages.dev`) stay behind Access ("Preview access: restrict previews" in the Pages
project's settings, with its Zero Trust application for `*.diagcalc.pages.dev`). The app requests
only `openid email profile`; Google's [basic-sign-in exception](https://support.google.com/cloud/answer/15549945?hl=en)
means Testing alone does not restrict these requests to listed test users or apply the usual
testing warning/authorization expiry. Account and Workspace policies can still restrict access.
Audience publication and brand verification are separate release configuration steps.

## Steps for Alex

The deploy workflow provisions what it can itself (see "What the deploy does"); these are the steps
that need a person, done once.

1. Cloudflare, My Profile, API Tokens: edit the token stored as `CLOUDFLARE_API_TOKEN` and add the
   account permissions D1: Edit and Workers R2 Storage: Edit (called "Write" in the new token
   editor; keep Cloudflare Pages: Edit). No zone or DNS permissions are needed. If R2 was never
   used on the account, open R2 in the dashboard once and enable it.
2. Google Cloud console, Google Auth Platform: create the consent screen (external audience, app
   name "diagrammatic-calculator", your support address, scopes `openid`, `email`, `profile` only).
   Testing is not an access-control boundary for these basic sign-in scopes. For production,
   review Audience, Branding, and Data Access separately; branding needs a homepage and privacy
   policy on a verified site. Then Clients: create an OAuth client of type Web application with the authorized
   redirect URI `https://diagcalc.pages.dev/api/auth/callback` (the redirect URI is built from the
   variable `APP_ORIGIN` in `web/wrangler.toml`). This is the chosen public hostname; no
   custom-domain migration is planned.
3. Cloudflare, Workers & Pages, `diagcalc`, Settings, Variables and Secrets, Production: add the
   secrets `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` from step 2.
4. (Optional share e-mails; not required for release.) Resend needs a separately controlled
   sending domain, not the site's `pages.dev` hostname. If enabling this later, verify a sending
   domain with the records Resend lists, configure its SPF, DKIM, and DMARC, and create an API key
   restricted to that domain. Add the key as the Pages secret `RESEND_API_KEY` (as in step 3), and
   set `EMAIL_FROM` in `web/wrangler.toml` to a sender on that domain. No sending domain is selected
   or provisioned by this repository.
5. GitHub, Actions, "Deploy web app": run the workflow. Its summary lists each part; the last line
   should say "Accounts: D1 database and R2 bucket bound, migrations applied."

Phase 7 uses SQLite-backed Durable Objects, available on the Workers Free plan. The workflow
does not change billing plans. Usage and any future upgrade remain the owner's decision; billing
analytics are not an application acceptance gate.

Secrets take effect with the next deployment, which step 5 makes. Share e-mails require both
`RESEND_API_KEY` and the `EMAIL_FROM` variable in `web/wrangler.toml`. `EMAIL_FROM` is empty by
default, so sharing sends no e-mail; copying share links still works.

`APP_ORIGIN` in `web/wrangler.toml` is the site's address (`https://diagcalc.pages.dev`). The
API answers only requests that reach it there (others get 421), and sign-in redirects and links in
share e-mails use it. Every deployment also has an address of its own (`<hash>.diagcalc.pages.dev`);
those keep serving the static app but not the API.

## What the deploy does

- Builds the app in a job that has no access to the secrets, and passes the built site to the
  deploy job, which installs Wrangler (without install scripts) before any secret is in its
  environment. Every action is pinned to a commit; Dependabot proposes updates.
- Runs `.github/scripts/provision-accounts.sh`, which creates, if missing and if the token allows:
  the D1 database `diagcalc`, the R2 bucket `diagcalc`, the Pages secret `SESSION_SECRET` (a fresh
  random value, never printed; an existing one is kept, since changing it signs everyone out).
  It also reports which of the other secrets are set. It does not manage DNS records.
  Nothing in it fails the run.
- If the database and bucket exist: writes the database ID (looked up by name, or the secret
  `D1_DATABASE_ID` if set) into `web/wrangler.toml`, applies the migrations in `web/migrations/`
  with `wrangler d1 migrations apply diagcalc --remote` (wrangler records which have run), then
  deploys with the D1 binding `DB`, the R2 binding `FILES`, and the variables `EMAIL_FROM` and
  `APP_ORIGIN`. Preview deployments (any branch but `master`) get no bindings (`[env.preview]`).
- Otherwise: removes `web/wrangler.toml` and deploys the site and functions with no bindings.
- Then reads the deployment back as any visitor would (`.github/scripts/check_deployed.py`, no
  credentials): `https://diagcalc.pages.dev/build-info.json` (emitted by the Vite build) must name
  the deployed commit, and the title and paragraphs of `/privacy` must be the text of `PRIVACY.md`
  at that commit. It retries for about three minutes; a lasting mismatch fails the deploy job, and
  pruning (below) then does not run. Wrangler's success message alone is not taken as proof.

A migration is never edited once it is on `master`; a schema change is the next numbered file.

### Phase 7 live rooms

The build job also bundles `web/live/room.ts` using `web/live/wrangler.toml`, without deployment
credentials. The deploy job copies the existing D1 database ID into the room configuration and
deploys that compiled Worker (`diagcalc-live`) before Pages. Both use the same `DB` and `FILES`;
Pages has the external Durable Object binding `LIVE` to its `DocRoom` class. The token therefore
also needs permission to deploy Workers. A missing permission fails the deployment rather than
silently replacing the room with a different write path.

The room has no `workers.dev` or preview URL, and its public fetch handler always returns 403.
Only Pages may issue its internal requests. Never deploy either of the test-only entrypoints in
`web/live`; the configured production entry is `room.ts`. New documents use schema generation 1.
An old document remains read-only until its owner chooses **Enable live editing**. Migration
retains a server checkpoint and a detached local recovery copy before switching generations.

Do not roll Pages back to phase 6 after migrating documents: that older application does not
understand the generation fence or committed `Y.Text` cells. Prefer a forward fix. Recover an
individual document by reading its checkpoint into a personal branch or detached copy; do not
merge an old-generation update into a migrated room. Original checkpoints are retained, but
there is no promise to retain every intermediate edit.

Every accepted update is staged and validated before it becomes authoritative, stored durably,
then broadcast and acknowledged to its sender. The REST snapshot is updated as part of that
save, so reconnect and REST readers do not depend on an in-memory room. Checkpoint maintenance
is bounded and ceases when the final socket closes. Session/access rechecks remain at most a
minute apart while a room has connections; terminal status messages notify clients without
depending on completion of the WebSocket close handshake.

Local verification (no Google or Cloudflare credentials required):

```sh
cd web
npm run typecheck
npm test
npm run build
E2E_PORT=4187 npx playwright test --project chromium --workers 2
npx wrangler@4.149.0 deploy --dry-run --config live/wrangler.toml
```

Browser fixtures exercise the UI with multiple independent contexts. The separate Miniflare
suite exercises the real room, persistence, authentication, and Pages-to-room boundary; fixture
tests alone are not a security or durability proof. Production Google sign-in remains a separate
live review step when no authenticated browser is available to the deployer.

## Running the accounts API locally

From `web/`, after `npm ci`, `npm run wasm`, and `npm run build`:

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

- **Old deployments.** Every deployment keeps its own files and server code with production
  bindings. "Prune old deployments" now follows successful "Deploy web app" runs on `master`.
  It skips superseded runs, selects only deployments older than production at selection time,
  stops on a detected production change, and does not take Cloudflare's acknowledgements as proof:
  it reads the deployment listing back (for up to a minute) until every deleted deployment is
  absent, and otherwise fails. Failures are reported as run annotations. It runs
  only trusted `master` code, never artifacts from the triggering run. The script is
  `.github/scripts/prune_deployments.py`; its tests run without credentials. For a manual dry
  run: GitHub, Actions, "Prune old deployments", Run workflow, leave "dry run" checked. Uncheck
  it to delete, or change `.github/prune-deployments` to start with `list` or `delete` and push.
  The automatic trigger becomes active when this workflow is deployed to `master`; local tests
  alone do not establish that it ran against Cloudflare.
  **Before a manual dashboard rollback, cancel any pruning run and confirm it has stopped.**
  The API's production read and deployment delete are not atomic: a rollback between them can
  make a selected obsolete deployment newer than the rollback target. Cloudflare protects the
  active production deployment, but the script cannot lock dashboard actions or preserve every
  future rollback target. Do not claim otherwise from the pre-delete check.
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
  key when `APP_ORIGIN` changes. Alternatively, use the "Register Google security events"
  workflow with a temporary repository secret `GOOGLE_RISC_KEY`, then remove the secret and
  revoke/delete the service-account key. Registration for `https://diagcalc.pages.dev` succeeded
  on 23 October 2026 ([run 40018607671](https://github.com/apellis/diagrammatic-calculator/actions/runs/40018607671));
  the run requested a verification event, but does not prove revocation closes a phase 7 room.
- **Separate tokens.** Instead of one `CLOUDFLARE_API_TOKEN` with every permission, keep the deploy
  token at Cloudflare Pages: Edit plus D1: Edit, lower Workers R2 Storage to Read once the bucket
  exists (the run summary says when it does), and remove any zone or DNS permissions.
  R2 Read must stay: the deploy checks with it that the bucket exists, and without it would deploy
  the site with accounts switched off. The app itself never uses this token.
- **Protected environment.** The deploy job runs in the GitHub environment `production`. GitHub,
  Settings, Environments, `production`: under Deployment branches allow `master` only. The secrets
  can stay repository secrets (a job in the environment reads both); moving them into the
  environment would also hide them from other workflows, but needs their values, so it can wait
  until the token is next rotated. Settings, Branches: protect `master` (no force pushes, no
  deletion).

## Public-release account steps

Publication belongs to phase 8 and needs Alex's go-ahead.

- **Google Auth Platform:** use the existing OAuth client's project, not a new client. Confirm
  the external audience and the `openid`, `email`, `profile` scopes. Verify the site's ownership
  in Search Console using a method offered for that property. The public About page is
  `https://diagcalc.pages.dev/#/about` (the app uses hash routing); the privacy policy is
  `https://diagcalc.pages.dev/privacy`. Both were reachable anonymously in the browser on
  23 October. Configure branding and authorized domains, complete any verification Google
  requires, then switch the audience to production at release (done 24 October). Console configuration still
  needs an authenticated owner; reachable pages do not establish domain verification or approval.
  See [Google's brand-verification requirements](https://developers.google.com/identity/protocols/oauth2/production-readiness/brand-verification).
- **GitHub history:** the pre-rewrite commits and the refs of pull requests #1 and #2 no longer
  resolve on GitHub (checked 24 October); the repository was made public that day.
- **Python distribution:** not published in phase 8; PyPI waits for a Pythonic API (phase 9 in
  `docs/roadmap.md`). Local build/install checks are in `docs/python.md`. Do not upload to PyPI
  merely to test packaging.
