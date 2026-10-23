# Roadmap

What is left, in order. The phases and their gates are in `docs/spec.md` ("Phases, deliverables
and acceptance gates"); phases 0 to 6 have passed. This file lists everything still open: the
remaining work before going public, phase 7, later items, the back burner, and what we decided
not to do. It is the one list to keep current: an item is removed when it lands (git history
keeps it), and new findings are added here.

## Decisions for Alex

None open.

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

- Occurrence IDs are carried through canvas gestures only: text typed by hand, terms written by
  the AI and accepted branches give their new summands fresh IDs; strands are numbered per summand.

Certificates:

- `kl`: the `basis` and `kl-evaluation` certificates stay `literature`. categorification-lean's
  `KL3.Diagram.canonicalBasis` is the family `IntegralSpanning.twoB`: a canonical chord diagram on
  the word `rd s ++ t` (from the empty word), dots at the arcs' left ends, bubbles composed on, and
  the lower endpoints bent back down by nested caps. The calculator's spanning set
  (`rep/kl/basis.rs`) is drawn differently: caps at the bottom, a reduced word of the through
  strands, cups at the top, a dot at each arc's left end, bubbles at the bottom right. The two are
  indexed alike and plausibly differ by lower terms, but that is not proved. `lean` needs one of:
  the calculator's set made literally `twoB`, a proof that the change of basis is unitriangular,
  or a Lean proof that the calculator's evaluation is well defined on KL's Hom spaces.

## Phase 7: live co-editing

Specified, with its plan and the decisions it needs from Alex, in `docs/phase7.md`. The phase 7
items found by the reviews of 23 October (history out of the document, cells keyed by ID and
edited as `Y.Text`, authors, branch and proposal ownership, the top bar, evaluation under remote
edits, the room's security rules) are all there.

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
