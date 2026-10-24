# Roadmap

The single list of open work. Phases 0 to 8 have passed; the calculator is released to alpha users.
Phase 9 is useful later work. The phase table and gates are in `docs/spec.md`; detailed phase 7 decisions are in
`docs/phase7.md`. Remove completed and discarded items rather than keeping a second backlog of
them; git history retains the decisions. Keep genuine external blockers explicit.

The public site is `https://diagcalc.pages.dev`; no custom-domain migration is planned.

## Phase 7: live co-editing — complete

Implemented in `7cfe45d`, with [CI](https://github.com/apellis/diagrammatic-calculator/actions/runs/40030129703)
and [deployment](https://github.com/apellis/diagrammatic-calculator/actions/runs/40030129712)
successful. Accepted decisions and the executable verification map are in `docs/phase7.md`.

## Phase 8: pre-release — complete

Passed 24 October (gate in `docs/spec.md`): view mode, Help page, certificate presentation, check
pills and deployment checks are live; the Google audience was published and the repository made
public on 24 October. If Google later asks for brand verification, complete it in the existing
project (About is `/#/about`, privacy `/privacy`; see `docs/deploy.md`).

## Phase 9: post-release

### Mathematical certificates

- **KL Lean certificates.** categorification-lean's `KL3.Diagram.canonicalBasis` is the family
  `IntegralSpanning.twoB`: a canonical chord diagram on `rd s ++ t` from the empty word, dots at
  the arcs' left ends, bubbles composed on, and the lower endpoints bent back down by nested
  caps. The calculator's set (`rep/kl/basis.rs`) instead has caps at the bottom, a reduced word
  of the through strands, cups at the top, dots at the arcs' left ends, and bubbles at bottom
  right. Identical indexing is not a proved identification. A `lean` certificate needs a
  unitriangular change-of-basis proof, a proof that the calculator's evaluation is well defined
  on KL Hom spaces, or a deliberate change to literally use `twoB`. Preserve the drawing
  conventions unless there is a mathematical reason to change them.
- **Odd BE2 nondegeneracy.** Adopt a valid proof when available; until then retain the conditional
  conclusions. Resolving the conjecture is a research dependency, not a calculator release task.

### Performance

Re-profile the released system before selecting work; keep exactness and determinism, and
retain changes only with measured benefits.

- Persist the basis cache to IndexedDB, with engine-version/context invalidation, to avoid cold
  hom-space evaluations after reopening a document.
- Evaluate independent basis rows in parallel, keeping thread management outside the core.
- Compact monomials, in-place polynomial addition, and `SmallVec` denominators where allocation
  profiles justify them.
- Lazy-load the AI SDK, canvas, and KaTeX fallback fonts where startup measurements justify it.
- Cache results by canonical text rather than trimmed input text. Reassess worker warming after
  phase 7; retain only residual work supported by measurements, not duplicate worker-lifetime work.

### Python

- **Pythonic Python API**, designed so it could be upstreamed into SageMath; publish to PyPI only
  after that. The current `call(op, json)` bindings stay a local-build convenience until then.
  Before publishing, also fix the metadata: a PyPI readme written for mathematicians (not the
  build guide), the SPDX form `license = "Apache-2.0"`, and a Homepage URL for the web app.

### Live revocation check

Google's security-event receiver is registered for `https://diagcalc.pages.dev`, and the tests
cover each half (a security event ends the person's sessions; ending a session closes their live
sockets). An end-to-end check in production needs a real Google account revoking access while a
room is open, which needs the owner's manual work; deferred from phase 8 (Alex, 24 October).

### Optional share notifications

- **Resend share emails:** configure a separately verified sending domain and restricted sending
  key when wanted; the `pages.dev` hostname is not a sending domain we control.
  Copying share links must continue to work without email delivery, with an accurate UI state.
