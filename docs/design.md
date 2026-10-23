# Design notes

These notes record how the core is built and why. The specification (`docs/spec.md`) says what to
build; the conventions (`docs/conventions.md`) fix how notation is read.

## Generality

The calculator is for diagrammatic algebras in general, not for KLR algebras alone. The core types
describe a presentation of a strict monoidal supercategory:

- **colours**, each with a parity and an orientation mode; objects are words in signed colours;
- **generators**, each with a source word, a target word, a degree and a parity, possibly given as
  schemas over colour variables (a KLR crossing `psi` of colours `a, b` has source `a b`, target
  `b a`, degree `-(a,b)`);
- **relations** as schemas with side conditions and parameters;
- in a 2-category, **region weights**: the context gives the weight of the rightmost region and
  every other region's weight follows from the strands to its right, so degrees (`h(a)`) and side
  conditions can depend on the region without words storing weights.

Nothing in `crates/core` may assume that a presentation is KLR, nilHecke or of any other family.
What a family needs beyond its presentation file plugs in through one trait per strategy:

| Strategy | Trait | First implementations |
|---|---|---|
| Faithful representation | `Representation` | polynomial representation of KLR algebras (nilHecke is the one-colour case); skew-polynomial representation of the odd nilHecke algebra |
| Basis normal form | `rewrite::straighten` with a `Layout` | straightening to a basis of crossings and dots, for any presentation whose representation names those roles |
| Confluent rewriting | (none yet) | not needed so far: straightening is planned, not a search for a confluent system |
| Finite-dimensional linear algebra | `FiniteQuotient` | phase 2 |
| Bounded search | `chain.find` | both sides straightened to the basis; a general search is not built |

A representation is attached to a presentation in its file (`[representation]` section) by naming a
built-in action kind and mapping each generator to an operator of that kind; the file also states
the representation's faithfulness certificate. A new family (webs, Temperley–Lieb, Soergel
diagrammatics, Heisenberg categories) needs a new presentation file and, only if no existing action
kind fits, a new representation module. The term model, the formats, the operation API and the user
interface do not change.

## Rings as contexts

Ring arithmetic goes through a context object rather than through operator overloading on element
types: `ring.add(&a, &b)`, `ring.mul(&a, &b)`. This lets one element type serve rings that need
run-time data (the prime of ℤ/p, the relations among parameters such as π² = 1, the variables of a
polynomial ring) and keeps every ring behind one trait, `Ring`. The phase 1 rings are ℤ, ℚ, ℤ/p and
ℤ[√−1]; polynomial rings over any of them (commutative or skew) are rings in the same sense.

## Terms

A word is a source object and a list of layers from bottom to top; a layer is one generator at a
strand position, with identities elsewhere. A term is a finite sum of coefficient × word, kept
sorted with no zero coefficients. Words equal up to the interchange law are identified by a
canonical layering (each generator as low as possible, ties broken left to right) together with
the Koszul sign accumulated on the way, so equality up to interchange is a direct comparison.

Parsing is in two steps. The text format parses to a syntax tree that keeps what was written
(positional generators, idempotents, powers, sums). Elaboration in a context (a presentation and,
for KLR, a weight ν) turns the tree into an explicit term: every word gets its source object, and a
word written without an idempotent stands for the sum over all objects it applies to.

## Equality strategies and results

Layered words modulo interchange represent morphisms but do not decide equality. Every computed
value carries the strategy that produced it, the faithfulness certificate of any representation it
relied on, and any caps. A representation whose certificate is "assumed" or "unknown" can confirm a
difference only with a witness checked another way.

For the 2-categories of phase 5 (KL, BE2), equality is decided by reduction to a basis normal form,
and that is what simplify shows. Future option, only if reduction proves too slow for interactive
use: show a quick result first from a faster method that may not be correct, mark the cell as a
pending reduction (a shimmer), and replace it with the normal form when that finishes. The quick
result is never reported as decided. Not planned unless timings force it.

A certificate's status is one of `lean`, `literature`, `conditional`, `assumed` or
`unknown`. `conditional` means equal normal forms are proven equal, while distinct normal forms
differ only under a named open hypothesis; results that rely on it say so, and the UI marks them
like `assumed` results.

### The quiver Hecke basis (`rep/straighten`, representation kind `quiver-hecke-basis`)

For BE2 (Brundan–Ellis, arXiv:1701.04133v2) the upward diagrams are reduced to the basis
`ψ_ŵ x^a e(i)` of the quiver Hecke superalgebra by the presentation's own relations, with Koszul
signs from the interchange law (the straightening of `rewrite`). Every step applies a relation,
so a diagram equals its normal form in `U(g)`. That the normal forms are linearly independent in
the quiver Hecke supercategory is the basis theorem of Kang–Kashiwara–Tsuchioka (arXiv:1107.1039v2,
Corollary 3.15; odd-lean `QuiverHecke.span_nf`, `QuiverHecke.linearIndependent_nf`). That they stay
independent in `U(g)` is BE2's Nondegeneracy Conjecture, open for odd colours and proved for even
data (odd-lean `EvenBridge.nondegeneracyConjecture_of_isEven`), so the certificate is `conditional`
(Alex's choice): a result "equal" is proven, a result "different" for odd colours rests on the
conjecture and says so. Only upward diagrams with `t_ij = 1` and `s_ij^pq = 0` are covered;
anything else is refused with a reason.

### Translations (`rep/translate`, representation kind `translation`)

A representation of kind `translation` names a shipped target presentation and gives each
generator's image there, a term in the generator's colour variables. A term is carried over word by
word: each source word is first given a narrow layering (`Context::narrow`: caps moved below cups
beside them, so the diagram stays as wide as it must be), then every layer is replaced by its
image, whiskered by the strands beside it, and the images are stacked without canonical
relayering. The canonical layering moves every generator as low as it goes, which can open a
cup beside a cap that has not closed yet; the KL evaluation bends and sorts whole diagrams, and its
cost grows quickly with their width (a leftward crossing above a cap went from 2.5 seconds to 1
millisecond). The target's first representation computes the normal form, which is carried back
by the inverse renaming (every generator in the target's normal forms must be the image of exactly
one generator); a translation that would need more is refused with a reason.

`be2` uses one into `kl` for even colours (`even_only`), certified by Brundan's comparison of the
Rouquier and Khovanov–Lauda definitions (arXiv:1501.00350) and KL III nondegeneracy; the
conformance checker checks every relation of `be2` through it, in types A1, A2, B2 and G2 at every
test weight. A translation that does not apply in a context (odd colours, no `mu=`) is skipped:
the session uses the first representation that applies, and the conformance report lists the
skipped one as unchecked. For odd colours the relations that define the leftward cups and caps
and the downward dots and crossings are checked by typing only, until the Lean export covers
them or the odd normal form (a super version of the KL evaluation) exists.

### The KL basis normal form (`rep/kl`, representation kind `kl-basis`)

A 2-morphism `f : s → t` of `U(g)` in the region `μ` is bent to `1_μ → *s t` with nested cups on
the left. The evaluation `Φ` (KL III, arXiv:0807.3250, Lemma 3.9 and Theorem 1.2) writes such a
morphism as one upward diagram with bubbles per term of a sorting decomposition of `1_{*s t}`.
The decompositions come from the presentation's `decompose-*` and `mixed-*` relations. Each
upward diagram is an element of `R(ν) ⊗ Π`: the KLR polynomial representation, with the
clockwise bubbles of a reference region as extra variables. `Φ` is evaluated run by run:

- A run of cups `1 → FE`, caps `EF → 1`, dots and crossings (type RL) takes right-sorted
  coordinates to left-sorted ones. It straightens `q_k ≫ run ≫ p'_r` to an upward diagram and
  closes the bent strands with right partial traces (curls, bubble slides).
- A run of the opposite type (LR) goes back with left partial traces.

Coordinate `k` is `coef_k · f ≫ p_k`, so `f = Σ v_k ≫ q_k`. Evaluation ends in the sorting the
target already has, which avoids an expensive final decomposition.

The basis of `HOM(s, t)` (KL III Proposition 3.11 with nondegeneracy) is drawn directly, one
diagram per matching of the boundary:

- caps closed at the bottom, each by moving the left end of the arc with the least span;
- the through strands permuted by a reduced word;
- cups opened at the top;
- one dot exponent per arc, at its left end;
- real bubbles of positive degree index at the bottom right.

The normal form solves `Φ(f) = Σ a_k Φ(b_k)` by fraction-free elimination. The basis words are
checked to be independent each time, and the target to lie in their span. Bubble slides assume
the default `Q = klq2`; the strategy refuses any other `Q`. Bubble variables are allocated four
per colour and doubled when an index overflows.

## Finite-dimensional quotients

A strategy that writes terms in a basis of homogeneous words (`Evaluator::graded_basis`) makes
each graded piece `A_d e(i)` a finite-dimensional space with coordinates: the coefficients of a
term's normal form. `quotient.rs` builds quotients by two-sided ideals generated by homogeneous
elements (cyclotomic quotients, dot caps) piece by piece, and truncations that stop at a degree.
Nothing in it is specific to KLR algebras: a new family needs only an evaluator with a graded
basis, and a cyclotomic quotient needs a generator in the role `dot`. Every result names the
quotient and the degrees computed.

A quotient is itself an `Evaluator`, so a context with `Lambda`, `dotcap` or `degcap` runs every
operation in the quotient and its provenance says so. The ideal's degree-`d` part is spanned by the
sandwiches `ψ_u g ψ_w` of degree `d` and dot multiples of lower parts, which needs only that the
words `x^a ψ_w` and `ψ_w x^a` both span. A cyclotomic quotient `R^Λ(ν)` is graded symmetric with
trace degree `D = 2(Λ, ν) − (ν, ν)`, so pieces above `D/2` stop reducing at the dimension of the
dual piece and vanishing pieces are skipped; a piece larger than that is an internal error, and the
tests compute every case both ways.

## Linear algebra toolkit and solving

`linalg.rs` does exact linear algebra over any field, ranks over domains, Hermite and Smith forms
over ℤ, word-size elimination over 𝔽_p and multimodular elimination over ℚ. `toolkit.rs` works on
any `Graded` algebra (a truncated ambient algebra or a quotient): bases, multiplication and Gram
matrices, the centre, the radical by the trace form, the semisimple quotient, blocks, idempotent
lifting and the primitivity test, each refusing inputs that break its preconditions (for example
the trace-form radical needs characteristic 0 and a complete finite algebra).

`solve.rs` solves `lhs = rhs` for unknown scalars: the coefficients of the normal form of
`lhs − rhs` are the equations. Linear systems are solved exactly (tier 1); others go to
`groebner.rs` (tier 2), whose basis-size, degree and step limits make running out of budget an
answer rather than a hang. Over ℚ the step count bounds operations but not coefficient growth, so
wall time is not bounded there; prime fields do not have this problem.

## Rewriting

`rewrite.rs` works on any presentation: its rules come from the presentation's relations (one per
direction, see the conventions), and nothing in matching or applying a rule knows about KLR
algebras. A match of a pattern is a set of layers of a word that is convex in the word's
dependency graph (no layer outside the set lies both above and below it) and that interchange
moves can bring together into the pattern's shape. The word is reordered as everything not above
the set, then the set, then the rest; the pattern is then a contiguous block, compared after
canonical layering. The moves and their Koszul sign are recorded with the match, so a step can be
replayed exactly, in Lean too.

A step replaces the block by each word of the rule's replacement and brings the result back to
canonical layering, remembering where every layer came from (`origin`). That provenance is what
lets the interface follow a strand or a dot through a step (`slide` uses it to keep track of the
dot it moves), and it replaces the eternal layer identifiers of earlier drafts.

Straightening needs to know which generator is the dot and which the crossing; a representation
declares those roles, and `rep::basis_layout` reads them, so a family with the same shape of basis
(KLR, nilHecke, odd nilHecke) straightens with no family-specific code. The strategy is planned
rather than searched: dots go through crossings to their side of the basis, then the exchange
property gives the braid moves that turn a crossing word into the canonical reduced word or expose
a double crossing. Two choices keep long words feasible. The term is advanced one layer at a time,
always rewriting the word with the most layers still to come, so equal words produced along
different paths meet and cancel early: on one random 30-layer nilHecke word this cut the steps
from about 80,000 to about 350. And only the words a step touches are re-examined. A chain keeps every
intermediate word, which grows like steps times layers; `rewrite::normal_form` gives the same
normal form without keeping the steps (B9 uses it beyond 50 layers).

## Lean bridge

`lean.rs` turns a chain into a Lean theorem over categorification-lean's KL III diagrams
(`kl3-dg`) or odd-lean's super Kac–Moody diagrams (`skm-cl`); the encodings are described in the
conventions. The presentation's `[lean]` table names, for each relation, the library lemma that
proves it and the Cartan facts it needs, so the exporter is generic in the presentation and only
the encoding is specific. Each step becomes a
`have` proved by that lemma whiskered into place, and the theorem is the chain of these steps
under rewriting. A layering that the lemma states differently from canonical layering is declared
(`layering = "left-first"`, `"right-first"`, or `"bubbles-first"` for closed components below
the strands; `pattern_layering` likewise for the lemma's left-hand side) and bridged with `dswap`
(`cswap`, which also produces the Koszul sign of an odd interchange for the proof to evaluate from
the parity hypotheses). In `skm-cl` a relation used right to left is proved from its instance left
to right, stated inside the step's proof. A lemma proves its relation in one of three ways (`proof`): by rewriting
(`rw`, then normalizing the sums), by being the statement itself (`exact`), or through a tactic
script (`tactic`), used for the weight-dependent KL relations whose library form sums over labelled
bubbles. A rule that applies only at some weights gives the exported theorem the hypothesis
`hμ_c : ip RD c μ = n` for the region right of the diagram, and each step proves the lemma's own
weight condition from it. The exporter refuses what it cannot state faithfully (scalars outside ℤ,
odd generators in `kl3-dg`) instead of exporting something weaker.

A chain rewrites each side by one of three strategies: `straighten` (to the basis of crossings and
dots, the KLR default), `shrink` (rules whose replacement words are all shorter than the pattern,
until none applies) or `once` (one step of the named relations, which checks the presentation's
reading of that relation against the library's lemma). For `shrink` and `once` the exporter checks
that both ends have the same normal form under the presentation's equality strategy.

`lean/chains/` is a Lake project pinned to one categorification-lean commit. `chains.toml` lists
the chains to export; `diagcalc lean-chains` writes `DiagcalcChains/*.lean`, a Rust test keeps
them current, and the Lean chains workflow builds them, with `Checks.lean` deriving the library's
own statement of a relation from an exported theorem. `DiagcalcSupport/KL3.lean` is written by
hand: it evaluates the library's labelled bubbles at a concrete label (real, degree zero, zero, or
fake through the infinite Grassmannian relation) and provides the tactic `dc_bub`, which finishes
a curl or decomposition relation once the library's lemma has been rewritten.

Generating presentation files from Lean was tried for KLR. The relations can be read: the
metaprogram `lean/chains/PrintRelations.lean` walks the equations of the library's `relation`
definition and prints each relation, with its side condition, in the calculator's text format. The
rest of a presentation file (degrees, parities, drawing, representations and their certificates,
the Lean lemma table) has no counterpart in the Lean definition, so the file stays hand-written.
Instead the printed relations guard it: a Rust test checks that each one, at every colouring its
side condition allows in types A₂, B₂, G₂ and A₃, is one of the calculator's rules up to sign and
vanishes in the representation, and that every relation of the file arises this way; CI checks
that the printed file is current.

## Command line and MCP

`diagcalc` exposes every operation (`call OP` takes the JSON request on stdin; the rewriting
operations also have their own commands). `diagcalc mcp` serves the same operations as MCP tools
over stdio, one tool per operation with its request schema from `spec/ops/`, so an AI client and
the command line see exactly the operation API.

## Worker pool

`pool::par_map` splits independent work (products for an ideal, entries of a Gram matrix) across
threads natively and keeps the input order, so a parallel run gives exactly the sequential answer.
A WebAssembly module has one thread. In the browser the pool is a set of Web Workers, each with its
own module instance (see "Web app"). Independent cells already run on different workers. Splitting
one heavy job into slices (rows of a Gram matrix, degrees of a quotient) for several workers,
through the same operations so the engine needs no shared memory, is not built yet; no 4a workload
needs it.

## Web app

`web/` is the browser workspace (phase 4a): Vite, TypeScript and Solid. Solid won a small trial
against Preact on what the workspace does most, a keyed list of SVG diagrams: rendering 1000
summands took 171 ms against 216 ms, a diff update of 10% of them 39 ms against 47 ms, and the
bundle was 5.0 KB against 8.4 KB gzipped (headless Chromium, median of five).

**Engine.** `crates/wasm` built with the `web` profile (LTO, no debug information) and
`wasm-bindgen --target web` (`npm run wasm`). The main thread compiles the module once and hands
it to each Web Worker, so replacing a worker is cheap. `src/engine/engine.ts` has two lanes: one
worker for quick operations (parsing a cell, describing a context), answered in order, and a pool
for evaluations, where a job can be cancelled or stopped at its time limit by replacing its
worker. Requests and responses are typed by `src/engine/ops.gen.ts`, generated from `spec/ops/`
(`npm run gen-ops`; CI checks it is current).

**Document.** `src/doc/model.ts`: a Yjs document holding the context, the cells (each an ID and
its text as one entry of a `.dcw` file), pinned snapshots and the version history. Every change is
one step through `Doc.step`, so it is one undo step (Yjs `UndoManager`) and one version, recorded
with a Yjs snapshot so any version opens read-only and can be restored. Documents are stored in
IndexedDB (`y-indexeddb`) with an index for the document list; `.dcw` export and import go through
`workspace.parse`, so the file is canonical. A `.dch` file holds the history (`spec/history.md`).

**Evaluation.** `src/eval/evaluator.ts`. A cell's result is keyed by a hash of its inputs: the
engine build, the context, the unknowns it mentions, its text and the keys of the definitions it
uses. An edit changes the keys of exactly the cells downstream of it, so every other cell, and
every state reached again by undo, is a cache hit (`src/eval/cache.ts`: memory, then IndexedDB
with a size cap and least-recently-used eviction). A cell with a new key is computed by
`workspace.run` on a small workspace of just the definitions it needs and itself, and shows as
stale until the value arrives; a job whose key no cell needs any more is cancelled. The core runs
a whole `.dcw` file the same way: each cell's context has only the unknowns that it, or the
definitions it uses, mention, so an unrelated solve cell never turns a cell's coefficients into
polynomials.

**Showing results.** `workspace.run` draws the summands of definitions and watches
(`summands`, each with its word, degree and SVG). A watch outlines summands that are new since
its previous value, shows the vanished ones faded for two seconds, and lists the changes under
"Changed". Every result carries a badge with its equality strategy and certificate, in the warning
colour when it rests on an assumption or a finite quotient or truncation, whose notes are shown.

**Tests.** Unit tests (`npm test`, Vitest) run the evaluator and the document model in Node
against the real WebAssembly engine. The Playwright suite (`npx playwright test`) drives the app in
Chromium: typing and editing cells, watch diffs, solve and check cells, context changes, history
and restore, persistence across reloads, the document list, export and import, snapshots, the gate
example, and latency. On the development container a computed edit shows its new value in about
60 ms and cached undo and redo in at most about 70 ms, against the 150 ms target.

**Focus canvas (4b).** `src/ui/Canvas.tsx` with the geometry in `src/canvas/geometry.ts`. It
edits a definition's or watch's term as written; a term that uses definitions is drawn written
out, and a gesture writes it out in full. The drawing comes from `term.layout`: one frame per
summand, with pieces (layer, generator, box, strand positions), strands with colours and
directions, lines to draw and the heights between rows. Layers are drawn at the lowest height
their dependencies allow, two consecutive layers sharing a height only when the earlier is left of
the later and their canonical order agrees, so a canonical word is drawn at its canonical levels
and any other order is reached by adjacent interchanges whose Koszul signs `term.layout`
reports. Gestures become operations: a dot drag counts the crossings it passes on its strand and
calls `slide` with that many steps; a piece drag computes a new drawn order (a layout hint, the
term unchanged); a box selection snaps to a rectangle for `relations.match`; a palette drop finds
the nearest height and strand position for `term.edit`. Every gesture writes the cell's text as
one step with a note, or stores a hint. Hints (`Doc.hints`, keyed by cell and summand word, an
order and look-only offsets) are part of the undo scope but never of the term. Each gesture has a
key: Tab between pieces, arrows slide a dot, ⌥-arrows interchange, R lists relations and 1–9 applies
one, I then a digit inserts, Delete removes, F fits; `?` lists them.

Inside the canvas, the top bar's ← and the breadcrumb (document name › cell) go back to the
document, as do the canvas's own "← Back to the document" button and Esc; from the document, ←
goes to the list of documents.

A dragged dot is drawn where it is at every moment: it follows the drawn curve of its strand
(`track` joins the strand's segments end to end through the dot, `along` finds the point of that
run at the pointer's height), a crossing on the strand glows as the dot nears it and rings once
the dot has passed its middle, and the bar says what letting go will do. On release the dot stays
where it was let go until the rewritten term is drawn; when nothing changes (a dot on a bare strand
moved within its row) it eases back along the strand to its drawn position.

Any other piece dragged up or down is redrawn, as it moves, with the drawn order it would get
(`term.layout` with that order), so a crossing stays centred between its strands and the pieces it
passes on other strands move out of its way; a −1 shows when two odd pieces swap. Letting go stores
that order. A crossing dragged sideways makes a Reidemeister-like move: `pullMove` looks among the
summand's relation matches for one whose matched pieces are all crossings and include this one,
preferring a triangle with it in the middle (a braid relation, armed by pulling the middle crossing
towards the other two) over a stacked pair (a square relation, pulled apart either way). A copy of
the crossing follows the pointer, the matched crossings glow and ring once the move is armed, and
the move's result is shown; letting go applies that match. A drag that can do nothing (sideways
with no such match, or up and down with nothing on other strands to pass) still moves the piece a
little, easing off like a rubber band, and the bar says why nothing happens; it springs back on
release.

A selection of several pieces (box-select on empty space, or ⌘/Ctrl-click) is a rectangle for
`relations.match` (R), is removed in one step by Delete (`term.edit` remove with `layers`), and is
what the AI panel receives (⌘L). A click on empty canvas clears the selection.

A click on a strand sets the insertion point there (a green ring at the nearest row); the next
palette click or I-then-digit inserts at it, by the same `dropTarget` rule as a palette drop, so a
crossing takes the clicked strand and the one to its right (the left one at the right edge). With a
single piece selected instead, an insert goes just above that piece on its leftmost strand.

**Display and entry.** Text is entered in the fixed-width font and shown typeset in the reading
font, so the fixed-width font always means plain text being entered or read exactly. `src/ui/tex.ts`
turns the calculator's text into LaTeX for KaTeX token by token (the mapping is in
docs/conventions.md, Display) and never changes what it says. A cell shows its text typeset; a
click (or Enter on it, or a click on its head) opens it in the editor, and committing or leaving
typesets it again. An expanded cell shows its text once, in a framed box that is the place to
edit it (the head then shows only the kind and the summary); a definition's or watch's text form
appears below, labelled "Normal form", only when it differs from what was typed. Text is checked
with `workspace.parse` as it is typed (after a pause) and on Enter: text that does not parse is
never written to the document, and the editor stays open with the reason and "Esc keeps the
previous text". Text that parses but cannot be elaborated (an unknown name, a generator that fits
no object) is written and shows its error as the cell's result. A result is typeset; a click shows
its text in the fixed-width font, selected, to copy. Raw views of files and data (a version's `.dcw`, a snapshot's JSON, an AI tool call's
input) stay in the fixed-width font.

**Branches (4b).** A branch is its own Yjs document, kept as an update in the main document's
`branches` map with the inputs it started from; the page shows either the document or a branch,
each with its own evaluator over the shared cache. `branch.accept` is a three-way merge of the
branch's changes since it started into the document as it is now (cells changed, added, removed,
hints), applied as one step that changes as little as possible; a cell both sides changed takes
the branch's text and is reported. Any version opens as a branch.

**AI panel (4b).** `src/ai/` and `src/ui/AIPanel.tsx`. The system prompt is `skill/SKILL.md`
(bundled with `?raw`; `diagcalc skill` prints the same file). The tools are the workspace
operations in `src/ai/tools.ts` and the term operations generated from `spec/ops/` into
`src/ai/optools.gen.ts` (`npm run gen-ops`: references inlined, `context` optional and filled from
the document). The loop (`src/ai/agent.ts`) streams each turn, runs the tool calls in the page and
sends the results back, up to 40 turns a request. With Anthropic it uses the official SDK with its
browser opt-in, Claude Opus 5.5 by default, adaptive thinking with summaries, an effort level, and
cache breakpoints on the system prompt, the tool list and the conversation. Tool inputs stream
eagerly, so each input is checked against its schema in the page before it runs, and turns cut
off at the length limit or refused never run their tools. Workspace edits are steps with the AI as
author and the model's note; a whole request undoes as one (the undo manager's capture window is
held open for the request). Removing a cell and accepting a branch are only proposed, as cards the
person accepts or rejects. Every request, step, note, milestone and proposal is logged in the
document's activity timeline (`Doc.timeline`). Pinned watches show at the top of the outline with
their term counts over the session.

OpenAI is called through its Responses API (`store: false`), which, unlike chat completions,
accepts function tools from reasoning models; a local OpenAI-compatible server speaks the
chat-completions protocol. The provider, model, effort and key are set on the settings page
(`#/settings`, reached from the document list or from the panel, which links there while no
provider is set up); the panel itself only chats. "Check the connection" there makes the kind of call the chat
makes (for OpenAI a Responses request of a few tokens to the chosen model; for Anthropic and a
compatible server the model list), with a spinner while it waits and a 30-second limit, and
reports the provider's own error message when it answers with one (green with a tick when it
passes). A compatible server has no default URL and an empty, disabled model menu until its list has
been fetched. Leaving the URL field fetches the list when the URL is usable (http or https with
a host) and marks the field invalid otherwise; ↻ refuses an unusable URL the same way; a server
that cannot be reached is reported under the menu. The model is chosen from the provider's list (Anthropic's with display names; OpenAI's filtered to chat models, newest first; a compatible
server's as given), kept per provider in local storage and fetched again with ↻; "Other…" takes a
typed name. Before a key is entered, Anthropic offers a built-in list. Keys stay in session storage unless "Remember on this
device" is ticked, and go only to the chosen provider. The first time the panel opens it says what
leaves the browser (there is no per-document switch: nothing is sent unless one writes to the AI).
Replies are shown as formatted text by `src/ui/markdown.ts`: a small, escaped Markdown subset
(paragraphs, headings, lists, emphasis, code spans and fenced code in the fixed-width font) with
LaTeX mathematics (`\(…\)`, `$…$`, `\[…\]`, `$$…$$`) typeset by KaTeX in the same style as the
rest of the app.

The settings page also holds the appearance: a three-position switch (Light, System, Dark).
System, the default, follows the operating system's `prefers-color-scheme`; Light and Dark set
`data-theme` on the root element, which `styles.css` reads to choose the colour variables. The
choice is kept in local storage (`diagcalc.theme`) and applied by an inline script in
`index.html` before the first paint, so a page never flashes in the other theme.

**Tests (4b).** Unit tests for the canvas geometry, branches and merging, AI tool schemas,
validation and the chat-completions translation. Playwright drives the gestures (drag-slide,
keyboard paths, insert and remove, odd interchange signs, look-only moves), branches (leave,
reopen, accept, undo, discard) and the AI panel against a recorded model: requests to the
Messages API are answered with scripted streamed turns, so a what-if on a branch runs end to end,
including the request's headers, caching and thinking settings.

**Not yet.** Clicking a summand to see which terms of the definitions produced it, persisting
a linked file across reloads, importing `.dch` files, and size diagnostics when a job is stopped at
its time limit (it reports the time and points to the command-line tool). In the AI panel: the
chat itself is kept only while the page is open (the timeline is kept with the document), pinned
watches' trends cover the session only, OpenAI and compatible servers are not streamed, OpenAI
reasoning is not carried between turns, and whether
OpenAI accepts calls from a browser page is decided by the in-app check in a real browser (the
development container cannot reach OpenAI).

## Accounts and sharing (phase 6)

**Where the server runs.** Pages Functions (`web/functions/api/[[path]].ts`, one function for every
path under `/api/`), not a separate Worker. They are served from the app's own origin, so the
session cookie is first-party and needs no CORS; they deploy with the site in the same
`wrangler pages deploy`, so the app and the API never disagree about versions and every preview
deployment has its API; and Cloudflare Access already covers them. A Worker routed under the same
host would give the same origin, but needs a route, its own deploy and its own configuration. The
function is invoked only for `/api/*` (the generated `_routes.json`); static files are served
without it, so they cost nothing and behave exactly as before. The code is in `web/server/`, plain
TypeScript on Web APIs (`fetch`, Web Crypto), so the same modules run in Workers and, for tests, in
Node.

**Sign-in.** Google OpenID Connect, authorization code flow with PKCE. `/api/auth/login` makes a
state, a nonce and a code verifier, keeps them in a short-lived cookie encrypted with AES-GCM
(a key derived from `SESSION_SECRET` with HKDF), and sends the browser to Google with the state,
nonce and S256 challenge. `/api/auth/callback` checks the state against the cookie, exchanges the
code with the verifier and the client secret, and validates the ID token: an RS256 signature by a
key from Google's JWKS (cached as long as Google's `Cache-Control` says, fetched again once for an
unknown key ID), issuer, audience (our client ID, and `azp` when there are several audiences),
expiry and issue time with a minute's skew, the nonce, and a verified e-mail address. The user is
keyed by Google's `sub`; the address is refreshed at each sign-in.

**Sessions and CSRF.** The session cookie `__Host-dc_session` is `HttpOnly; Secure;
SameSite=Lax; Path=/`, holds a random 256-bit ID and its HMAC, and lasts 30 days; D1 stores only
the SHA-256 of the ID, so the table alone cannot be used to sign in, and signing out deletes the
row. Every POST, PUT, PATCH and DELETE needs the header `X-CSRF-Token`, an HMAC of the session ID
that `GET /api/me` hands to the page, and is refused when `Origin` or `Sec-Fetch-Site` shows
another site. Lax cookies already keep other sites' forms from carrying the session; the token
covers the rest.

**Data.** D1 (`web/migrations/`) holds `users`, `sessions`, `documents` (owner, name, times,
version, size, the R2 key of the current snapshot, link role and token), `shares` (document,
lower-case e-mail address, role) and `sent_mail` (only sender and time, for the daily limit). Shares
are by address, so a document can be shared with someone who has never signed in. R2 holds each
document's content, a Yjs update, under `docs/<id>/snapshots/<random>`, and exported files under
`docs/<id>/files/<name>`. The server never reads a snapshot.

**Saving.** `PUT /api/docs/:id/snapshot` carries `If-Match: "<version>"`. The content is written to
a fresh R2 key, then D1 moves the version on only if it is still the one the page read; otherwise
the fresh object is deleted and the answer is 409 with the current version. The page then fetches
the current content, merges it into its own (Yjs merges without losing either side's steps) and
saves again. A save that loses the race therefore never overwrites another, and the previous
snapshot is deleted only after the new one is current.

**Access.** Every document route first works out the caller's role: owner, a share to their
address (editor or viewer), or the document's link (token in `X-Link-Token`, compared in constant
time), taking the highest. Without one the answer is 404, so a document's existence is not
revealed. Viewers read; editors also save, rename and keep files; only the owner shares, sets the
link and deletes. A link can be read without an account, but editing through a link needs one, so
every change has an author. Turning link sharing off and on issues a new token, which revokes old
links. Someone may remove their own share.

**Mail.** Only share notifications, through Resend, when `RESEND_API_KEY` and `EMAIL_FROM` are set:
one message when someone is first added (not when a role changes), with the document's name and
the inviter's name escaped, and at most 50 a day per sender. A refused message is reported to the
page as not notified; sharing itself still succeeds.

**Limits and logging.** Snapshots and files up to 16 MB, 1000 documents per owner, 100 shares per
document. Responses are `no-store`. The API logs nothing about users: an internal error is logged
as the route pattern (IDs and names replaced) and the error's kind.

**Configuration.** Missing secrets or bindings make `GET /api/me` answer `{accounts: false}` and
every other route 503, so a deploy without them is the anonymous app. `docs/deploy.md` lists the
names and the steps.

**Tests.** `npm run test:server` (its own Vitest configuration, `vitest.server.config.ts`, without
the Solid plugin) runs the API in Node against Miniflare's D1 and R2, the same SQLite and object
store as the Workers runtime, with the migrations applied as wrangler applies them. A fake Google
checks the PKCE verifier, client secret and redirect URI at its token endpoint and signs real RS256
ID tokens; a fake Resend records messages; a clock is injected. The tests cover the PKCE test
vector of RFC 7636, every ID-token check, the whole sign-in and its failures (wrong state, missing
or tampered cookie, reused code, replayed nonce), session expiry and sign-out, CSRF, the role of
each kind of caller on every route, link sharing, saving races, notifications and their limit,
deleting documents and accounts, and that no user data reaches the log. The functions also build
with `wrangler pages functions build`, and the migrations apply with
`wrangler d1 migrations apply --local`.
