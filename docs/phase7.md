# Phase 7: live co-editing — specification and implementation

Status: implemented; local acceptance checks pass. Deployment results are recorded by the
Deploy web app workflow. Originally specified 23 October 2026.

## Accepted implementation decisions

The following decisions from Alex supersede conflicting requirements below:

- **Only committed edits synchronize.** Drafts stay private. A remote commit must not remount an
  editor or discard its draft; offer Keep mine / Take theirs. No direct shared-keystroke editor
  binding is required. Presence describes the focused cell and committed-text/canvas selections,
  not another person's private draft.
- **Recovery is ordinary editor recovery**, not a promise to retain every intermediate edit:
  session-local undo/redo, persistent checkpoints, and preservation of conflicting drafts. Stable
  cell identity must survive moves. A durable save acknowledgement follows storage, not receipt.
- **Local responsiveness comes first.** Local commits and evaluation never wait for networking.
  Remote commits converge promptly; remote evaluation may coalesce bursts. The original combined
  300 ms debounce / 150 ms end-to-end target is replaced by measured behavior without unnecessary
  recomputation.
- **Low idle cost is sufficient.** Preferred annual budget is below $100 all-in. Billing analytics
  and billing validation are not acceptance gates; Alex monitors billing. Use hibernation and
  stop maintenance after the final connection and persistence complete.
- Use Free during development; a Paid upgrade before public release remains a recommendation,
  not permission to change billing. Branches are personal; any editor may decide proposals;
  attributed pinned results remain shared and locally recomputable; dedicated view mode is later.
- Merge and deployment are authorized without further approval. Keep making progress while
  offering periodic status and useful review points; do not wait for those reviews. Report actual
  access failures rather than requiring an advance token-permission change.

The original implementation brief is retained below, subject to the accepted decisions above.
Read `AGENTS.md` first (it binds you), then this file, then the parts of `docs/spec.md`,
`docs/design.md` ("Web app", "Accounts and sharing (phase 6)") and `docs/deploy.md` it points to.
`docs/roadmap.md` is the list of everything open; phase 7 items move from there to here and leave
both when they land.

## Implementation and verification map

| Boundary | Implementation | Executable coverage |
| --- | --- | --- |
| Committed edits, UUID-owned editors, private conflicting/deleted drafts | `src/doc/model.ts`, `src/ui/WorkspacePage.tsx`, `Cell.tsx`, `Editor.tsx` | `src/doc/collaboration.test.ts`, `tests/live.spec.ts` (including actual server ID repair while a draft is open) |
| Session undo, private checkpoints and branches, safe migration/reopen | `src/doc/store.ts`, `src/account/recovery.ts`, `docsync.ts`, `account.ts`, `src/ui/CloudRecovery.tsx` | Account/doc recovery and lifecycle unit tests; `tests/recovery.spec.ts`, `tests/branches.spec.ts` |
| Durable rooms, authentication, limits, revocation and generation fences | `live/room.ts`, `validation.ts`, `server/live.ts`, `room-auth.ts`, `versions.ts` | `live/acceptance.test.ts`, `limits.test.ts`, `room.test.ts`, `server/live.test.ts`, `versions.test.ts` |
| Actual Pages-to-room integration | `server/router.ts`, external `LIVE` binding | `live/pages-seam.test.ts`: real workerd, migrations, signed sessions, tickets, durable readback, viewer/origin/session rejection, target-free AI presence |
| Authenticated presence without private branch content | `src/account/live.ts`, `live/presence.ts`, workspace UI | Live-provider/presence units and peer-received browser awareness regression |
| Atomic shared AI decisions and attributable recomputable pins | `src/ai/proposals.ts`, `src/eval/pinned.ts`, `src/ui/PinnedRecompute.tsx` | Proposal/pin unit tests; two-editor proposal race and pinned-recompute browser tests |
| Immediate local evaluation, coalesced remote work, cached undo | `src/eval/evaluator.ts`, `src/engine/engine.ts` | Local-first/cancellation units and real browser/WASM timing tests, including a 50-cell chain |

Paths in the table are relative to `web/`. Run `npm test`, `npm run typecheck`, `npm run build`
and Playwright's Chromium and timing projects there; run the repository's Rust checks in
`AGENTS.md`. `docs/deploy.md` describes the separate Worker/Pages bundles and deployment order.
Browser fixtures prove UI behavior, not production authorization: real-runtime room and Pages
seam tests are separate. An authenticated Google session is needed for signed-in production
review; an anonymous health check is not that review. No billing measurement is required.

## 1. Goal and gate

Several people edit one document at the same time, each in their own browser, and see each other's
changes, cursors and selections as they happen.

**Gate:** two browsers commit edits to one workspace without losing edits or private drafts.
Tests also cover a third browser joining late, a browser going offline and returning, a viewer
and a revoked collaborator. Rooms use hibernation and stop maintenance after their final
connection closes; zero-cost billing is not a gate (see the accepted decisions above).

**In scope.** The live room (a Durable Object per document), the WebSocket protocol, its
authentication and limits, the document-model changes live editing needs, presence, AI activity
visible to collaborators, evaluation under remote edits, the deploy pipeline, tests and docs.

**Out of scope.** Sharing computed results between collaborators (decided, section 2); phones;
a server that computes anything; following another person's view; comments threads; the view mode
for reviewing (a later roadmap item, section 12).

## 2. Decisions already made (do not reopen)

- **Collaborators do not share computed results** (Alex, 23 October). Each browser computes its own
  values; the cache key (`src/eval/hash.ts`) makes equal inputs give equal keys, so collaborators
  see the same values without trusting each other's.
- **Edit history and undo live outside the shared document** (Alex, 23 October): undo and redo are
  session context, as in other document editors, not part of the file. Undo reverts only your own
  steps (spec, "Undo and redo").
- **Roles** are owner, editor and viewer, as the server already enforces (design.md, "Access").
  Editing through a link needs an account. Anonymous use stays local and unchanged.
- **Mobile is a non-goal**; cheap improvements only.
- **Results relying on the BE2 Nondegeneracy Conjecture say so**; nothing in phase 7 changes how
  results are labelled.
- The server never computes mathematics. The room may do structural bookkeeping on the CRDT
  (validation, ID de-duplication), which is not computation in this sense.

## 3. Phase 6 baseline (historical)

- **Document** (`web/src/doc/model.ts`): one Yjs document per workspace with `gc: false`. Roots:
  `meta` (name, context, pinned watches, edited time), `cells` (a `Y.Array` of `Y.Map`s with `id`
  and `text` as a plain string), `history` (one entry per step with a full `Y.encodeSnapshot`),
  `snapshots` (pinned results: computed values), `hints`, `occurrences`, `drawn` (per cell, the
  summand words its canvas last drew, which occurrence IDs are carried from), `timeline`,
  `branches` (each a whole Yjs update, with the `occ-*` counters it started from) and `counters`
  (`nextId`, `occ-<letter>`, `nextBranch`). Every change
  goes through `Doc.step`, which records a version. Origins are the constants `LOCAL`
  (`author: 'you'`) and `AI`; the undo manager tracks those two origins. Reads are defensive
  (`problems()`, `cleanCopy()`).
- **Local copy**: IndexedDB through `y-indexeddb` (`src/doc/store.ts`), the working copy and the
  offline cache.
- **Saving to the account** (`src/account/sync.ts`, `docsync.ts`): a whole-state REST save
  (`PUT /api/docs/:id/snapshot` with `If-Match` on a version), 1.5 s after a change, on hide and
  every minute; a 409 means fetch, merge, save again. Viewers only fetch.
- **Server** (`web/server/`, Pages Functions at `/api/*`): Google sign-in, sessions (cookie
  `__Host-dc_session`, only the SHA-256 of the ID stored), CSRF token on unsafe methods, Origin
  check, roles (`docs.access`), link tokens (`X-Link-Token`), R2 snapshots under
  `docs/<id>/snapshots/<random>`, D1 tables `users`, `sessions`, `documents`, `shares`,
  `sent_mail`, `mail_optout`, `pending_deletes`. `APP_ORIGIN` pins the host. Logging never
  includes user data. Google security events (`POST /api/google/risc`) end a user's sessions.
- **UI facts that matter here**: a cell is mounted with `<Show when={c.text} keyed>`
  (`WorkspacePage.tsx`, around line 810), so any change to its text remounts it and destroys an open
  editor; AI proposals are page state (`proposals` signal in `WorkspacePage.tsx`); branches live in
  the shared document; timeline authors are the strings `'you'` and `'AI'`.
- **Evaluation** (`src/eval/evaluator.ts`, `src/engine/engine.ts`): a job whose key no cell needs
  is cancelled by terminating its worker; cache lookups go one cell at a time.

The security, speed and UX reviews of 23 October list what phase 7 must change; they are in the
project's shared files (`diagcalc/reviews-2026-10-23/`); the parts that matter are restated here,
so you do not need them.

## 4. Architecture

```text
browser A ─┐  wss /api/docs/:id/live?ticket=…   ┌───────────────────────────┐
browser B ─┼──────────────► Pages Function ────►│ Worker diagcalc-live      │
browser C ─┘  (auth, role, Origin, ticket)       │  Durable Object DocRoom    │
                                                 │  one per cloud document    │
     REST /api/docs/:id/snapshot ──► Pages ─────►│  SQLite: update log        │
                                                 │  R2 snapshot + D1 version  │
                                                 └───────────────────────────┘
```

**4.1 Where the room runs.** Cloudflare does not let a Pages project define a Durable Object, so
the room is a separate Worker, `diagcalc-live`, in `web/live/` with its own `wrangler.toml`
(`[[migrations]] tag = "v1", new_sqlite_classes = ["DocRoom"]`, bindings `DB` and `FILES` to the
same D1 database and R2 bucket). It has no public route (`workers_dev = false`, no `routes`): only
the Pages project reaches it, through a Durable Object binding `LIVE` (class `DocRoom`, script
`diagcalc-live`). Check the current Cloudflare documentation for the exact binding syntax in a
Pages `wrangler.toml`; the dashboard (Pages project, Settings, Bindings) is the fallback, and
`wrangler pages dev … --do LIVE=DocRoom@diagcalc-live` is the local equivalent. Code shared with
`web/server/` (roles, sessions, crypto) is imported, not copied.

Alternative considered: moving the whole app from Pages to a Worker with static assets, so the
room lives in the same script. That is a larger migration of the deploy and the `_headers`; do not
do it in phase 7 unless the binding route turns out not to work.

**4.2 One room per document.** `env.LIVE.idFromName(docId)` for a document saved to an account.
A document only in the browser has no room; it becomes live when it is saved to the account
(owners) or opened from a share or link.

**4.3 The room is the single writer of a document's content.** While phase 6 lets any client `PUT`
a whole snapshot, in phase 7 every change to a cloud document's content goes through its room:

- live clients send Yjs updates over the WebSocket;
- the REST save (`PUT /api/docs/:id/snapshot`) is forwarded by the Pages Function to the room,
  which merges the posted state as an update (so clients without WebSockets, and the sign-in
  upload in `syncAll`, keep working);
- the room persists: it appends each accepted update to its SQLite storage at once, and writes the
  merged state to R2 plus the new `version` in D1 at most every 10 seconds while edits arrive and
  once more when the last socket closes (an alarm), so `GET snapshot` and the document list stay
  current. The R2 write uses the existing fresh-key-then-swap pattern.
- `GET snapshot` may still read R2 directly (it can lag the room by up to 10 s, which is fine for
  a first load: the WebSocket sync brings the rest).

**4.4 Hibernation.** The room uses the WebSocket Hibernation API (`ctx.acceptWebSocket`,
`webSocketMessage`, `webSocketClose`, `webSocketError`), never `ws.accept()`, so an idle room is
not billed for duration. In-memory state is lost when it hibernates: each socket carries an
attachment (`serializeAttachment`, keep it small) with the role, user ID, display name, colour,
session hash or link marker, the awareness client IDs it has announced and when its role was last
checked. On waking, the room rebuilds its `Y.Doc` from the SQLite log (compacting the log into one
update when it exceeds 200 entries or 1 MB). No timers or intervals keep it awake; periodic work
uses alarms, which end.

**4.5 Protocol.** The y-protocols wire format (as `y-websocket` uses it): binary messages, first a
varint message type, `0` sync (sync step 1, sync step 2, update) and `1` awareness. Use the
`y-protocols` package on both sides (add it to `web/package.json`; it is the standard companion of
`yjs`). The client side is a small provider of our own in `src/account/live.ts` (about 200 lines:
connect with a ticket, sync, awareness, reconnect with backoff, status), not `y-websocket`'s
`WebsocketProvider`, so tickets, roles, close codes and the status line are ours. Close codes:
`4001` not signed in or ticket expired, `4003` not allowed (viewer wrote, role revoked),
`4008` rate or size limit, `4009` document too large, `4010` document deleted, `1012` room
restarting (reconnect).

**4.6 Connecting: tickets.** Browsers cannot set headers on a WebSocket, and a link token must not
appear in a URL that may be logged. So:

1. The page calls `POST /api/docs/:id/live-ticket` (CSRF-protected like every unsafe call; for a
   link viewer without an account, with `X-Link-Token` and no session). The Pages Function
   computes the role with `docs.access` and returns a ticket: an HMAC (key derived from
   `SESSION_SECRET` with its own HKDF label) over the document ID, the role, the user ID (or a
   link marker), the session hash, and an expiry 60 seconds ahead.
2. The page opens `wss://<APP_ORIGIN>/api/docs/:id/live?ticket=…`. The Pages Function checks the
   ticket, the `Origin` header (must equal `APP_ORIGIN`: WebSockets have no CORS, so this is the
   CSRF defence) and the session cookie when the ticket names a session (same session hash), then
   forwards the upgrade to the room with headers it sets itself (`X-DC-Role`, `X-DC-User`,
   `X-DC-Name`, `X-DC-Session`) after deleting any of those the client sent. The room trusts only
   these headers, which only the Pages Function can send.
3. A ticket is single-use in effect: it expires in 60 s, and the room ignores a second socket that
   presents the same ticket nonce while the first is open.

## 5. Document-model changes

These come first (milestone M1) and are worth having even before any WebSocket exists.

**5.1 Cell text as `Y.Text`.** A cell's `text` becomes a `Y.Text` so two people typing in one cell
merge character by character instead of last-writer-wins. The editor binds to it with
`y-codemirror.next` (add the dependency), which also draws remote cursors in the editor from
awareness. Cells are keyed by ID in the UI (replace `<Show when={c.text} keyed>`), so a remote
change never remounts an open editor or loses a draft. Text that does not parse is still never
committed as the cell's text: keep the draft in the editor and commit on Enter as now, but a
committed cell edited by two people at once merges. Decide and document how an open, uncommitted
draft meets a remote commit to the same cell (recommended: the draft stays, and a line under the
editor says "Ada changed this cell; their text: …, Keep mine / Take theirs").

**5.2 History out of the shared document.** Stop writing `history` (a full snapshot per step makes
the document grow quadratically: 13.7 MB after 3,000 steps). Replace it with:

- **Undo and redo**: per session, as now (`Y.UndoManager` with this session's origins only). It
  already reverts only your own steps; keep that and test it with two people.
- **Versions**: checkpoints kept outside the document. The room stores a checkpoint (the whole
  state as a Yjs update, plus who edited since the last one) in R2 under
  `docs/<id>/versions/<time>` when it persists after at least 5 minutes since the previous
  checkpoint, and keeps the last 50 plus one per day for 30 days (counted against the owner's
  storage). `GET /api/docs/:id/versions` lists them, `GET …/versions/:v` returns one; the History
  tab shows them, any one opens read-only or as a branch, and restoring is one step that sets the
  inputs (`setInputsStep`). Documents only in the browser keep a local version list in IndexedDB
  beside the document, written the same way.
- **`gc: true`** for new and migrated documents, so deleted text is garbage-collected: a shared
  document then no longer carries everything ever deleted (security review L9). Checkpoints are
  full states, so they do not need `gc: false`.
- **Migration**: a document with a non-empty `history` is converted once, by its owner's browser,
  into a fresh document (`cleanCopy`, with `gc: true`) whose first checkpoint is the old whole
  document, so nothing is lost: the old version list stays readable from that checkpoint. For a
  cloud document the conversion happens on the owner's first live open and replaces the content
  through the room as one operation (`POST /api/docs/:id/migrate`), and every other client that
  connects afterwards reloads. Shared documents opened by non-owners before the owner migrates are
  read through the REST path (no live editing) until then. Export `.dch` keeps working: it writes
  the checkpoints as versions.

**5.3 Authors.** Replace the string authors with a stable identity: user ID, name and a colour
(from a fixed palette by a hash of the user ID). The origin objects become per-session
(`{author: userId, kind: 'person' | 'ai'}`); the timeline and checkpoints store user IDs, and the
UI shows "you" for your own, the person's name for others, and "Ada's AI" for steps the AI made for
Ada. Anonymous documents keep `you`/`AI`. Names shown come from the server (the room's socket
attachments and the sharing list), never from document content.

**5.4 Branches are personal.** Branches move out of the shared document into the creator's own
storage (IndexedDB, and for signed-in users an R2 object per branch under
`docs/<id>/branches/<user>/<branch>` through a small REST route), so collaborators do not see each
other's "what if" copies. Accepting a branch is a normal step on the shared document (the existing
three-way merge). Existing branches in documents are moved to the migrating owner's storage.

**5.5 AI proposals in the document.** Proposals (`WorkspacePage.tsx` `proposals`) move into a
shared `proposals` map, so everyone sees a pending proposal, who asked for it and what it would
change. Any editor may accept or reject one; the timeline records who did. The AI of a person who
is not an editor makes no proposals (viewers cannot change the document).

**5.6 IDs under concurrency.** `counters` (`nextId`, `occ-*`) are last-writer-wins, so two people
adding a cell at the same moment can get the same ID. The room de-duplicates after applying each
update: when two cells (or two occurrence lists) share an ID, the one whose `Y.Map` was created
later in Yjs's total order (client ID, then clock) gets a fresh number past every number in use,
written by the room as one transaction with origin `room`, which every client receives. The rename
is deterministic and needs no mathematics. Cell IDs are handles (definitions refer to each other by
name), so a rename breaks nothing except the AI's in-flight references, which it re-reads.

**5.7 Pinned snapshots.** Pinned results and milestone diagrams are computed values that a person
chose to keep. They stay in the shared document as a record, labelled with who computed them
("computed by Ada's browser"), and gain a "Recompute" button that evaluates the cell's frozen
inputs locally and says whether the value matches. Nothing else computed is shared.

**5.8 Schema and limits.** The room accepts only these roots: `meta`, `cells`, `snapshots`,
`hints`, `occurrences`, `drawn`, `timeline`, `counters`, `proposals` (and, during migration only,
the old `history` and `branches`). Document size cap 16 MB, as now.

## 6. The room: rules it enforces

These come from the security review's phase 7 requirements and are all tested (section 10).

1. **Authentication.** Only through a ticket checked by the Pages Function (4.6). A socket whose
   session ends (sign-out, sign-out everywhere, Google security event, idle or age limit) is closed
   with `4001` within a minute: the room re-checks each socket's session hash against D1 at most
   every 60 s, when it next receives a message from it, and in an alarm while sockets are open.
2. **Roles.** Viewers may send sync step 1 and awareness; sync step 2 and updates from a viewer
   close the socket with `4003`. Editors and owners may send everything. A link editor must be
   signed in (as now).
3. **Revocation at once.** Unsharing, a role downgrade, turning the link off or rotating it, and
   deleting the document make the Pages Function call the room (`POST /internal/revoke` on the
   stub, not reachable from outside); the room re-checks every socket's role immediately and closes
   (`4003`, or `4010` for deletion) or downgrades it. Account deletion and sign-out everywhere do
   the same for every room the user has a socket in: keep a D1 table `live_sockets (doc_id,
   user_id, since)` that rooms update on connect and disconnect, so the Pages Function knows which
   rooms to call.
4. **Limits.** Per message 1 MB; per document 16 MB (an update that would pass it closes the socket
   with `4009` and is not applied); per socket 50 messages a second averaged over 5 s; 20 sockets
   per document and 10 per user across documents; awareness state at most 4 KB and 10 updates a
   second. Over a limit: close with `4008`.
5. **Validation.** Each update is decoded (`Y.decodeUpdate`) before it is applied: it is refused
   if it fails to decode, touches a root not in 5.8, or puts anything but a `Y.Map` into `cells`.
   It is then applied inside try/catch; a throw closes the socket and the update is dropped. The
   client keeps reading defensively (`problems()`, `cleanCopy()`), as now.
6. **Presence is text, and names come from the server.** The room rewrites the `user` field of
   every awareness state it relays from the socket's attachment (ID, name, colour), so nobody can
   appear as someone else. Clients render awareness only as text and as positions; e-mail
   addresses never appear in awareness.
7. **Single writer** (4.3): the REST save is forwarded to the room; nothing else writes the
   document's R2 snapshot.
8. **Logging**: as the REST API, no content, names or addresses; route patterns and error kinds
   only.

## 7. Presence

- Each client publishes, through awareness: the open cell (ID), the editor cursor and selection
  (as Yjs relative positions, so they survive edits), the canvas selection (`Selection` is already
  serialisable: cell, summand, word, layers, rectangle), and whether its AI is working.
- The top bar gets an avatar stack (initials on the user's colour, name on hover) and the
  connection state, in an overflow menu when space is short (the top bar is already full).
- Cells show a coloured bar and the name of whoever has them open; editors show remote cursors
  (from `y-codemirror.next`); the canvas outlines pieces others have selected, in their colour.
- Presence of viewers is shown too (they are in the room).

## 8. Evaluation under remote edits

From the speed review:

- **Debounce** recomputation after remote changes: 300 ms after the last remote update to a cell's
  inputs, not on every update.
- **Do not kill useful jobs.** A running job whose key some cell still needs keeps running; one no
  cell needs is cancelled cooperatively (a flag the engine checks at row boundaries) where the
  engine supports it, and by terminating its worker only after a grace period (2 s).
- **Batch cache lookups**: one IndexedDB transaction for all cells that changed keys
  (`cache.getMany`), instead of one awaited lookup per cell.
- **Results are never synced**; the shared cache key keeps everyone's values identical.
- Target: with 3 people editing a 50-cell document, a remote edit to one cell shows its new value
  in the other browsers within the interactive target (150 ms plus the computation) and causes no
  recomputation of unaffected cells.

## 9. Cost

- Durable Objects backed by SQLite are now offered on the Workers Free plan, with daily limits
  (about 100,000 requests and 13,000 GB-s of duration a day; incoming WebSocket messages are billed
  at 20 per request). Over a free limit, operations fail until the next day: an outage, not a bill.
  Verify the current numbers on Cloudflare's pricing page before deciding.
- Expected use while the app has few users is far below those limits. A hibernating room costs no
  duration.
- **Recommendation**: build and test on the Free plan; move to Workers Paid ($5 a month, within the
  $100 a year budget) before the public release, so a busy day cannot take live editing down.
  That is Alex's decision (section 12).
- Show "an idle document costs nothing" in the gate by: a test in Miniflare that the room schedules
  no alarm and holds no timer once the last socket has closed and the final persist ran; and, after
  deploy, the room's duration in the Cloudflare analytics for a document left open and idle for an
  hour (sockets open, no edits) is zero beyond the connect and persist.

## 10. Testing

- **Room unit tests** (`web/live/*.test.ts`, Vitest against Miniflare with Durable Objects and
  hibernation): sync of two clients, persistence and reload after eviction, compaction, the REST
  save through the room, every rule of section 6 (each limit, viewer writes, bad updates, unknown
  roots, spoofed presence, expired tickets, revoked roles, deleted documents, ended sessions),
  ID de-duplication, migration.
- **Convergence fuzz test** (Node, no browser): N simulated clients (3 to 5) make random edits
  (add, remove, move and edit cells; type in the same cell; context changes; hints; occurrences)
  through a room in Miniflare with random delays, disconnects and reconnects; at the end every
  client and the persisted state are identical, no client's acknowledged edit is missing, and every
  ID is unique. Seeded, so a failure reproduces.
- **Browser tests** (Playwright, several contexts against `wrangler pages dev` with the room bound
  and the existing fake Google, or against an extension of `tests/mockapi.ts` with an in-memory
  room — choose the first if it runs in CI in reasonable time): the gate script (section 1), two
  people typing in one cell, presence, undo reverting only one's own steps, a viewer who cannot
  write, revoking a share while connected, going offline and back, the AI's step and proposal
  appearing in another browser, and the migration of an old document.
- **CI**: everything runs in the existing workflows; a red run e-mails Alex, so nothing flaky is
  merged (fix causes, never retry or skip).

## 11. Plan

Each milestone is pushed to `master` when green (standing permission; AGENTS.md "Checks" first,
plus `npm run typecheck`, `npm test`, `npm run test:server` and Playwright for web changes), keeps
the app working for everyone at every step, and updates `docs/design.md` and this file.

| | Milestone | Done when |
|---|---|---|
| M0 | **Setup.** Alex's decisions (section 12); the Cloudflare token gains Workers Scripts: Edit (Alex, in the dashboard); `web/live/` skeleton Worker with a `DocRoom` that echoes; the Pages binding works in a preview and in production; local dev with `wrangler pages dev --do`. | A test page in a preview opens a WebSocket to a room through `/api/docs/:id/live` and gets an echo. |
| M1 | **Document model** (section 5): `Y.Text` cells and the editor binding, cells keyed by ID, history replaced by checkpoints (local for browser-only documents), `gc: true`, migration of old documents, authors, personal branches, proposals in the document, pinned snapshots labelled. No networking yet. | All existing unit and browser tests pass (updated where the model changed); new tests for migration, checkpoints, two `Doc`s merging concurrent typing in one cell, undo per origin. |
| M2 | **Room persistence and sync**: SQLite log, compaction, hibernation, R2 + D1 persistence, the REST save through the room, checkpoints, ID de-duplication. | Room unit tests and the convergence fuzz test pass. |
| M3 | **Security** (section 6): tickets, Origin, roles, limits, validation, presence rewriting, revocation, session re-checks, `live_sockets`. | Every rule has a passing test; a review of the diff against section 6 finds no gap. |
| M4 | **Client provider and status**: `src/account/live.ts`, reconnect with backoff, fallback to the REST sync when WebSockets fail, top-bar status ("Live with 2 others", "Reconnecting…", "Offline: saved in this browser"), `DocSync` reduced to the fallback. | The gate script passes in Playwright with two and three browsers, offline and back. |
| M5 | **Presence UI** (section 7) and AI activity visible to all (timeline entries with authors, "Ada's AI is working" from awareness, shared proposals). | Browser tests for presence and AI visibility pass. |
| M6 | **Evaluation under remote edits** (section 8). | The 3-person, 50-cell measurement meets its target, recorded in design.md. |
| M7 | **Deploy and docs**: the deploy workflow deploys `diagcalc-live` before the Pages site (a separate step; see the constraints), docs (`design.md` section "Live editing (phase 7)", `deploy.md` steps for Alex, `spec.md` phase row marked passed with evidence), roadmap updated, gate report to Alex with the CI runs and the idle-cost evidence. | Gate passed and recorded. |

M1 and M0 can proceed in parallel; M2 needs M0; M3 needs M2; M4 needs M1 and M3.

## 12. Decisions for Alex (before or during M0)

1. **Plan for Durable Objects**: Free plan now and Workers Paid before the public release
   (recommended), or Workers Paid from the start.
2. **An uncommitted draft meeting a remote commit** to the same cell: keep the draft and offer
   Keep mine / Take theirs (recommended), or replace the draft.
3. **Branches personal** (recommended) or shared with an owner badge.
4. **Who may accept an AI proposal**: any editor (recommended), or only the person whose AI made it.
5. **Pinned snapshots** stay shared and labelled with who computed them, with a Recompute button
   (recommended), or are dropped from shared documents (each person pins their own).
6. **View mode**: the later roadmap item (a mode with no editing affordances, for reviewing) is
   separate; viewers in phase 7 see the normal UI with editing disabled as today. Bring it into
   phase 7 or keep it for later (recommended: later).

## 13. Constraints for the implementer

- AGENTS.md binds: general (nothing KLR-specific in core types), exact and deterministic, every
  result says how it was obtained, publication-ready (no secrets, machine names, local paths),
  commit straight to `master` with green CI, commit messages with a summary line and a body and no
  co-author trailers, no "generated with" footers and no model names.
- **CI must stay quiet**: Alex is e-mailed for every red run. Run the checks locally before every
  push; fix a red run before anything else.
- **Never change the "Accounts storage" step** of `.github/workflows/deploy-web.yml`, nor
  `.github/scripts/provision-accounts.sh`, without Alex's explicit agreement in his own words.
  Deploying the Worker is a new, separate step or job. Its token needs Workers Scripts: Edit,
  which Alex adds (M0).
- **Privacy policy**: never add, edit or remove any privacy-policy wording (`PRIVACY.md` and the
  `/privacy` page built from it) on any branch: Alex approves every word first, in his own words.
  Phase 7 shows collaborators each other's names and presence, which the approved policy covers
  ("used only for your account and for sharing"); if anything you build stores or shows a new kind
  of personal data (for example e-mail addresses in presence, or IP addresses), stop and ask.
- Cloudflare Access stays off the site; the per-deployment addresses stay restricted (deploy.md).
- No new external services beyond Cloudflare. New npm dependencies only `y-protocols` and
  `y-codemirror.next` unless Alex agrees; pin exact versions.
- Memory in the development container is limited (about 15 GB; builds with `CARGO_BUILD_JOBS=2`);
  the web build does not need the Rust build except for the WebAssembly package.
