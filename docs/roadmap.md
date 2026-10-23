# Roadmap

What is left, in order. The phases and their gates are in `docs/spec.md` ("Phases, deliverables
and acceptance gates"); phases 0 to 6 have passed. This file lists everything still open: the
remaining work before going public, phase 7, later items, the back burner, and what we decided
not to do. It is the one list to keep current: an item is removed when it lands (git history
keeps it), and new findings are added here.

## Decisions for Alex

1. **Dot powers.** x₁¹⁵ is drawn as 15 separate dots, which makes very tall diagrams. Proposal: a
   single dot labelled with its power above 3. Changes `docs/conventions.md` and the golden corpus.
2. **Normal-form display.** Normal forms print the canonical layering (x₁⁴x₂x₁x₂x₁x₂x₃…), which is
   correct but hard to read. Proposal: an optional view with commuting dots gathered into monomials;
   the text form stays as it is.

## Before going public

Owner steps (dashboards; see `docs/deploy.md` "Hardening that needs a person"):

- Google sign-in, published: verify `diagcalc.pages.dev` (or the custom domain) in Google Search
  Console, set the consent screen's homepage to the About page and its privacy policy to
  `/privacy`, submit for brand verification, then publish. Until then only test users can sign
  in.
- Custom domain `diagcalc.ape.computer`, when wanted: add its callback URL to the OAuth client,
  then change `APP_ORIGIN`; add the WAF rate limit on `/api/` writes; Resend for share e-mails.
- Before that, ask GitHub Support to remove the pre-rewrite history (master was rewritten on
  23 October to replace a personal e-mail address with the owner's noreply address): the refs of
  pull requests #1 and #2 and the cached old commits. Delete the branch `wip/odd-normal-form`
  (old history; its work landed on master).
- Register the receiver of Google's security events, so sessions end when Google access is
  revoked (`docs/deploy.md`, "Revoked Google access"; a service account key and one script run).
- Make the repository public, with the licence already in place.

Polish (all remaining UX polish happens before going public):

- AI panel: claims in replies linked to the operation that produced them; comments on a cell as
  instructions to the AI; milestones with key diagrams in the timeline.
- Occurrence IDs are carried through canvas gestures only: text typed by hand, terms written by
  the AI and accepted branches give their new summands fresh IDs; strands are numbered per summand.

Certificates:

- `kl`: the `basis` and `kl-evaluation` certificates say `literature`, though categorification-lean
  proves the basis theorem (`KL3.Diagram.calculusNondeg`, `KL3.Diagram.canonicalBasis`, every
  commutative ring, finitely many colours). Check that the calculator's spanning set is the one
  `canonicalBasis` is about, then change the status to `lean`.
- `be2`, `quiver-hecke-basis` for even colours: likewise check that its ψ_ŵ x^a basis is one the
  even-data theorem covers before giving it `lean`.

## Phase 7: live co-editing

Deliverables and gate as in the spec. Decided (Alex, 23 October): collaborators do not share
computed results (each browser computes its own; the cache key makes them identical), and edit
history and undo live outside the document as session context.

Structure to change first:

- History outside the live document: today `Y.Doc({gc: false})` plus a snapshot per step makes the
  document grow quadratically (13.7 MB after 3,000 steps).
- Cells keyed by ID, not text, and each cell's text a `Y.Text` bound to the editor, so a
  collaborator's change does not remount an open editor or lose a draft.
- Authors as stable user IDs with name and colour, shared with presence; "you" computed when
  displayed.
- Ownership of branches (per person, or shared with an owner badge) and AI proposals stored in the
  document, so collaborators see them.
- Room in the top bar for avatars and sync state (an overflow menu).
- Remote edits: debounce recomputation, let running jobs finish, cancel cooperatively at row
  boundaries instead of terminating the worker; batch cache lookups.

Security requirements for the Durable Object relay (from the security review): authenticate the
WebSocket upgrade (session cookie or link token, `Origin` check), filter writes by role, size and
rate limits, validate each update on a scratch copy, presence rendered as text with names from the
server, revocation that closes or downgrades open sockets at once, the Durable Object as the only
writer of the snapshot, role re-checked after hibernation, no content in logs.

## Later

- **View mode** (Alex, 23 October): a mode with no editing affordances, optimised for reviewing the
  parts of a document.
- **Sharing roles** (Alex, 23 October): owner, editor and viewer. The server and API already have
  the roles and refuse a viewer's saves; the UI still shows viewers the editing controls. Goes with
  the view mode.
- **Speed:**
  - persist the basis cache to IndexedDB, so reopening a document needs no first evaluation per hom
    space;
  - lazy canonicalisation in `smash::val_mul` (estimated 2–3×, also for the KLR representation);
  - group `mul_top` terms by dot monomial and add in place in `rep/skm/up.rs` (estimated 1.5–2×);
  - basis rows in parallel (2.5× on 4 threads measured);
  - compact monomials, in-place polynomial addition, `SmallVec` denominators;
  - two WASM modules (size-optimised for the interactive lane, speed-optimised for jobs), and
    lazy-loading the AI SDK, the canvas and KaTeX's fallback fonts;
  - web pipeline: show results before caching, batched lookups, warm workers, cache keyed on
    canonical text (about −20 ms per edit).
- **Odd BE2 normal forms** become proven when BE2's Nondegeneracy Conjecture for odd colours is;
  until then they are marked conditional.
- **Old deployments** pile up again with each deploy (each keeps its code and the production
  bindings). Run "Prune old deployments" from time to time, or have the deploy prune after a
  successful production deploy.
- **AI providers:** stream OpenAI and compatible servers; carry OpenAI reasoning between turns.
- Python package on PyPI (with the public release).

## Back burner

- Phones and tablets beyond cheap fixes (Alex, 23 October: mobile is a non-goal).
- A fast, not-yet-proven result shown first and marked pending (Alex, 22 October: only if normal
  forms are too slow interactively; the speed review found them fast enough after the fixes).

## Won't do

- Trusted Types: not workable while Solid renders templates through `innerHTML`; the
  Content-Security-Policy is the defence in place.
- Sharing computed results between collaborators (decided for phase 7).
