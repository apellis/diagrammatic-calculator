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

The canonical layering is also how a word is printed, which scatters a dot monomial over several
heights. `Context::fmt_term_monomials` gives a second reading, offered as `monomials` beside a
normal form when it differs: even dots moved down past whatever does not touch their strand and
written together as monomials (docs/conventions.md, Printing). It is printed from the stored word
with no sign to track, since only even dots move, and nothing is stored in that form. Drawings
shorten dot powers the same way (`render::dot_powers`): four or more even dots one after another
on a strand are drawn as the first of them, labelled with the exponent, and the others keep
pieces in the same place, so the canvas can still pick any of them.

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
conjecture and says so. When every colour is even the representation carries `even_certificate`,
status `lean`: the normal forms `ψ_ŵ x^a e(i)` are decorated matchings of Theorem 8.1 between
upward words (one reduced word per permutation, distinguished points at the bottom of the strands,
no bubbles), and the theorem for even data makes the decorated matchings a basis over `SYM`, so the
normal forms stay independent in `U(g)`. That identification is read off the two definitions, as
for `skm-basis`, and is said in the certificate. Only upward diagrams with `t_ij = 1` and
`s_ij^pq = 0` are covered; anything else is refused with a reason.

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

`be2` has one into `kl` for even colours (`even_only`), certified by Brundan's comparison of the
Rouquier and Khovanov–Lauda definitions (arXiv:1501.00350) and KL III nondegeneracy. It is the
second representation, after the BE2 basis normal form below, which is about ten times faster on
the same diagrams; it stays as a cross-check (`--strategy kl-translation`). The conformance checker
checks every relation of `be2` through both, in types A1, A2, B2 and G2 at every test weight. A
translation that does not apply in a context (odd colours, no `mu=`) is skipped, and the
conformance report lists it as unchecked. For odd colours the relations are checked through the
BE2 basis normal form, and those that define the leftward cups and caps and the downward dots and
crossings also against odd-lean by the Lean export.

### The KL basis normal form (`rep/kl`, representation kind `kl-basis`)

This is `kl`'s second representation, `kl-evaluation`; the first computes the same normal forms
with the BE2 evaluation (see the BE2 section below).

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
the default `Q = klq2`; the strategy refuses any other `Q`. Bubble variables are allocated per
colour and doubled when an index overflows, which restarts the evaluation. The first allocation
is the one the hom space's kept basis evaluations used, if any (so a repeat does not restart),
and otherwise a guess from the words' degrees (`basis_cache::kz_estimate`: the largest degree of
a bottom part of a word divided by `2 min d_i`, plus the largest `|⟨i, μ⟩|`, plus 2, halved when
every colour is odd, at least 4, rounded up to a power of two). It held on every workload measured; a guess too high costs a few ring variables,
one too low a whole failed evaluation (up to a third of the time).

The evaluation ring has one dot variable per strand of the longest object the words pass through.
That is enough: a run from `w` to `w2` is straightened between sorted components no longer than
`w` and `w2` into one upward diagram on `#E(src) + #F(tgt) = (|src| + |tgt|)/2` strands, since
every cup or cap of the run adds or removes one `E` and one `F`; partial traces only remove
strands. A value with more strands than dot variables is an error, never a silent reuse of a
bubble variable. Divisions by `x_a − x_b` in the smash product are first tested at a fixed point
modulo the prime `2^61 − 1` (`Ring::residue61`, a ring homomorphism): a nonzero value proves the
division inexact, and a zero value falls back to the exact test.

**Basis evaluations kept across calls** (`rep/basis_cache`, for `kl-basis` and `skm-basis`). The
evaluations of the basis words of a hom space in a degree, brought to echelon form, depend only on
the context and the hom space, not on the term being normalised, and cost almost all of the time.
They are kept in a least-recently-used cache that outlives one evaluator (in the browser, everything
one worker computes): at most 64 hom spaces and 200,000 coefficients. The key holds the strategy
and representation names, the ground ring's type and the complete debug form of the context
(presentation, specification, colours and parities, Cartan datum, weights, `μ`, parameters,
scalars, the ground ring with its run-time data), compared in full, plus the source, target,
degree and the numbers of dot and bubble variables. The tests change each component and check
that the cache is missed, and check that a hit gives the same normal form as no cache.

### The BE2 basis normal form (`rep/skm`, representation kind `skm-basis`)

The super version of the KL evaluation, for `be2` with any parities (Brundan–Ellis,
arXiv:1701.04133v2), and for `kl`. The structure is the KL one: bend, evaluate run by run into upward diagrams
with bubbles, solve against the basis of reduced matchings. What changes:

- **Upward diagrams.** Values live in the quiver Hecke superalgebra (`ψ_ŵ x^a`, odd dots for odd
  colours, Koszul signs when odd dots pass each other) tensored with the bubbles of the reference
  region. For an odd colour the clockwise bubbles of even index are polynomial variables and the
  bubble of index 1, `δ`, is odd with `δ² = 0` (BE2 (2.18)); the odd indices are `δ` times the even
  ones.
- **Bubble slides** use `τ = Σ (2r+1) x^{2r} t^{2r}` for an odd colour across its own strand, KL's
  series otherwise (BE2 Section 7, with `t_ij = 1`, `s_ij^pq = 0`); `τ` is even, so `δ` slides
  freely. Counterclockwise bubbles come from the infinite Grassmannian relation with the sign
  `(−1)^{p r}`.
- **Leftward cups and caps** are odd for an odd colour `i` with `⟨i, μ⟩` even (the same in every
  region, since an odd colour pairs evenly with every simple root). Straightening a run tracks
  every odd atom (dots, odd turns, bubbles) by height and multiplies by the Koszul sign of the
  order the upward diagram lists them in; zigzags contribute their own sign.
- **Downward dots and crossings** are first written with leftward cups and caps:
  `xd = 2δ − (rotated dot)` for an odd colour and the rotated dot for an even one, and the
  downward crossing is the rotated upward one up to `(−1)^{p(a)p(b)(⟨a,λ⟩+⟨b,λ⟩+1)}`.
- **Sorting decompositions** carry BE2's signs `(−1)^{p(i)p(j)(⟨i,λ⟩+1)}` and the odd versions of
  the curl terms.

Every step is a relation of `U(g)`, so equal normal forms are equal there: a check that holds, and
a normal form of zero, are proven by the computation alone, and the result says so with
`certified_by: "computation"`; a check that fails, and a nonzero normal form, rest on the
certificate (`certified_by: "certificate"`), and the web app marks them conditional. The basis
words are checked to evaluate to independent vectors in every degree computed (a necessary
condition for the Nondegeneracy Conjecture; a failure would contradict it and is reported as an
error), and the target to lie in their span (BE2 Theorem 8.1). With an odd colour the
certificate is `conditional`, as for the quiver Hecke basis. When every colour is even the
conjecture is a theorem (odd-lean `EvenBridge.nondegeneracyConjecture_of_isEven`, for Khovanov–Lauda
scalars, a symmetrizable Cartan datum with finitely many colours and any commutative ground ring),
so the representation declares a second certificate, `even_certificate`, with status `lean`, and
results in such a context carry it: this is the session's representation for even data. The tests
check every relation in types A1, A2, B2 and G2 at the conformance checker's weights, plain and
next to a strand of each colour and orientation, against the translation into `kl`: both sides'
normal forms are identical to KL's, written in `be2`'s names (with every colour even a bubble's
dots are drawn where KL's basis puts them), and each strategy fixes the other's normal form; so
does every basis word of up to three strands in several weights.

The same evaluation computes `kl`'s normal forms: `kl`'s first representation, `basis`, has kind
`skm-basis` with `kl`'s generators in the same roles (no sideways crossing generator; `kl` draws
it as the rotated upward crossing), declared `even_only`, and refuses a `Q` other than the default
`klq2` as KL's evaluation does. Under the renaming of the roles the two presentations agree for
even colours and these scalars (Brundan, arXiv:1501.00350), so the certificate is KL's. KL's own
evaluation stays as the second representation, `kl-evaluation`; the tests check that the two give
exactly the same normal forms on both sides of every relation of `kl` in types A1, A2, B2 and G2 at
the conformance checker's weights, on every basis word of up to three strands, and on composites
with curls and bubbles. The BE2 evaluation is several times faster: its upward diagrams are sums
`Σ ψ_ŵ f` straightened by the quiver Hecke relations, where KL's are operators on polynomials
whose products divide by `x_a − x_b` at every step. Basis evaluations are
kept across calls as for KL, and bubble series are cached per strand list; a diagram with three
strands, one of them downward, takes seconds, and that cost grows quickly with the width.

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

Proven per-object vanishing bounds are stored separately from requested computation caps. A
homogeneous component above a proven bound is discarded as exactly zero before ambient
expansion; a component above an explicit `degcap` is rejected unless vanishing is independently
known. Quotients without a known global bound compute requested pieces on demand and do not
claim whole-algebra completeness. Whole-basis and graded-dimension enumeration still require a
finite window. KL and BE2 bubble-variable allocation grows on demand, without the former fixed
retry ceiling; address-space, runtime and memory limits remain operational constraints.

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
normal form without keeping the steps (B9 uses it beyond 50 layers). The planned braid sequences
of words that have left the term are kept (a word that comes back resumes its plan) until they
number more than 2,000 and eight times the term; then they are dropped, and a word that comes back
is planned afresh. That bounds the memory by the current term rather than every word ever
produced: B9's 200-layer nilHecke words peaked at 234 MB before, 44 MB now, with the same steps.

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

**Document.** `src/doc/model.ts`: a shared Yjs document holding the context, committed cells,
pinned snapshots, proposals and activity. Each cell has an immutable UUID, a repairable public ID
and committed `Y.Text`; `cellOrder` orders UUIDs. The UI keys rows, private drafts and focus by UUID,
resolving the current public ID only when an operation runs. `Doc.step` supplies session-specific
undo origins. A separate private Yjs sidecar holds personal branches and recovery: the latest 50
automatic steps plus explicitly retained checkpoints, not every intermediate edit. Both are stored in
IndexedDB (`y-indexeddb`) with an index for the document list; `.dcw` export and import go through
`workspace.parse`, so the file is canonical. A `.dch` file holds the history (`spec/history.md`);
importing one rebuilds the document version by version (`src/doc/dcw.ts`): each version's
workspace becomes the document's state, recorded as a version with its author, note and time; the
snapshots and the AI timeline come along, entries that cannot be read are left out and counted in
a message, and new cells take numbers no version used. A document linked to a file on disk (Chrome,
Edge) keeps the file handle in IndexedDB (`diagcalc-links`), so after a reload it saves to the file
again at once if the browser still allows writing, and otherwise shows "resume saving" in the top
bar until the person allows it (browsers ask once per visit).

**Cell creation.** `src/ui/NewCell.tsx` owns the visible kind picker and local per-kind drafts.
The existing text editor retains direct commands and slash shortcuts. The composer validates
before committing and invalidates pending submissions when the draft, context or mode changes,
or when it is cancelled or unmounted. The parent makes one undoable document insertion; drawing
continues in the same focus canvas used by existing definitions, not a separate diagram format.
`DiagramStart` reads available boundaries from `context.describe`, defaults to its first object,
and offers preset selection and adjacent swaps without enumerating permutations. Presentations
with explicit oriented objects instead offer upward/downward strand buttons or an empty boundary.
Every identity is checked with `term.layout` before Start drawing is enabled; stale validation
cannot escape its context/selection snapshot. Switching Type/Draw keeps an unfinished boundary.

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

Input changes invalidate displayed results synchronously, before parsing, cache reads or remote
coalescing. Once keys are known, unaffected cells reuse their values without new engine jobs.
Stale checks and solves show only computation progress, never the previous verdict or solution.
An unparseable cell conservatively blocks later computational cells: skipping it could resolve a
name to an older, shadowed binding. `WorkspacePage` overlays UUID-owned private drafts in a
separate local evaluator for previews. The committed evaluator remains authoritative for AI
reads and frozen snapshots; exports still read the document. Discarding a draft restores committed
results without writing the document. Preview results cannot be pinned or used for canvas writes.

**Showing results.** `workspace.run` draws the summands of definitions and watches
(`summands`, each with its word, degree and SVG). A watch outlines summands that are new since
its previous value, shows the vanished ones faded for two seconds, and lists the changes under
"Changed". Every result carries a compact "How computed" disclosure containing its equality
strategy and certificate. Dependent odd BE2 results say "Assuming nondegeneracy" without
repeated long warnings; their certificate stays conditional. Other uncertain certificates and
finite-quotient/truncation notes retain warning treatment, and the notes remain visible.
The disclosure's hypotheses and reference keep the presentation file's text and are shown typeset
(`src/ui/prose.ts`: short formulas through KaTeX, Lean names in the fixed-width font).

**Gathered dots.** Next to a watch's text form, "Gather dots" switches every watch to the reading
with its commuting dots gathered into monomials (`monomials`), and "Canonical layering" back; the
choice is kept in this browser (`diagcalc.monomials`). The drawings and the cell text do not change.

**Tracing a summand.** Clicking a summand of a fresh watch (or Enter on it) asks `workspace.trace`
which terms of the definitions produced it. The core multiplies each term of each definition the
watch uses directly by its own new scalar, evaluates the watch, and reads the summand's
coefficient off by those scalars: each monomial in them is one product of definition terms, and
its coefficient (with the scalars set to 1) is that product's share, so the shares add up to the
summand. Above 120 definition terms it declines rather than guess. Tracing is exact and runs on
the job lane; a cached value is traced from the inputs it was computed from.

**Size diagnostics.** A job stopped at its time limit reports the sizes it had reached, since the
worker is ended and cannot answer. The core keeps a running record (`progress.rs`: stage, cell,
most summands, layers and dots in a summand, largest basis solved in, largest polynomial,
rewriting steps), with nothing measured unless a host installs a sink; the WebAssembly build's
`enable_progress` sends it to the worker's `diagcalcProgress`, which posts it to the page at each
change of stage and at most every 150 ms, and the engine attaches the last record to the timeout.

**Tests.** Unit tests (`npm test`, Vitest) run the evaluator and the document model in Node
against the real WebAssembly engine. The Playwright suite (`npx playwright test`) drives the app in
Chromium: typing and editing cells, watch diffs, solve and check cells, context changes, history
and restore, persistence across reloads, the document list, export and import, snapshots, the gate
example, and latency. On the development container a computed edit shows its new value in about
60 ms and cached undo and redo in at most about 70 ms, against the 150 ms target.

**Focus canvas (4b).** `src/ui/Canvas.tsx` with the geometry in `src/canvas/geometry.ts`. It
edits a definition's term as written; a term that uses definitions is drawn written
out, and a gesture writes it out in full. Watches and viewer canvases are read-only: no editing
gestures, previews, insertion palette, editing cursors or editing help. Selection for inspection
and AI questions, pan and zoom remain available. The drawing comes from `term.layout`: one frame per
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
one, I then a digit inserts, Delete removes, F fits; `?` lists them. However far one pans or zooms, at least 80 pixels of the drawing stay in view on each axis (the
pan is clamped, again whenever the drawing or the window changes size), and a summand reached
with the arrow keys is panned into view.

**Occurrence IDs.** A generator occurrence has an ID (`d` dot, `c` crossing, `u` cup, `n` cap,
`b` bubble, `g` any other piece) numbered through the whole document in the order occurrences are
made: `d3` is the third dot the document ever had. The engine knows nothing of IDs; it reports
where occurrences go. Every operation that changes a term (`relation.apply`, `slide`,
`straighten`, `term.edit`) returns a provenance map: for each summand of the result, in printed
order, and each layer of its canonical layering, the occurrences `[summand, layer]` of the
starting term it descends from. A layer outside a rewritten rectangle descends from itself; a
layer a rule creates continues the pattern's layer of the same generator, matched bottom to top
(`rewrite::Summand::continues`), so a dot slid through a crossing is the same dot and the crossing
the same crossing, and a braid move keeps its three crossings. An occurrence may descend into
several summands (it was split: it keeps its ID in each), a layer may descend from several (equal
words from different rewrites were added together: it takes the oldest ID its summand does not
already use), and a layer that descends from nothing was created. Pattern layers left over are
deleted. `rewrite::Provenance` composes the steps of a chain. The document keeps the IDs in
`Doc.occurrences`, keyed like the hints by cell and summand word, one ID per canonical layer, with
one counter per letter that undo leaves alone, so an ID is never reused. A gesture computes the
new summands' IDs from the provenance map (`src/canvas/occurrences.ts`, for the first 200 summands)
and stores them in the same undoable step as the new text. A term no gesture made (text typed by
hand, a term the AI wrote, a changed definition the cell uses) gets its IDs when it is drawn,
outside the undo history: drawing records creation order and changes no input. The document keeps,
per cell, the words its canvas last drew (`Doc.drawn`, the first 60 summands); a summand drawn
without stored IDs takes them from those by `term.diff`, which matches the two terms as a diff
would. A summand whose word is still there is that summand; each other new summand is paired with
at most one old summand that is gone, best aligned pair first (more pieces kept, then more kept in
the same place, then the same source), and its layers keep the old layers of the same generator
that a longest common subsequence of the two words lines up, bottom to top (as a rule-created
layer continues the pattern's, so a braid typed by hand keeps its three crossings). What is left
over is new and gets fresh IDs; only the first 200 changed summands of each side are compared, and
the response says when more were left out. Accepting a branch carries the IDs of the cells whose
text it takes, with their drawn words, in the same step: an ID numbered below the counters the
branch started from (`BranchRecord.counters`; for a branch of a branch, the lower of the
document's and the parent's) is shared and stays, and one the branch made gets the document's next
number of its kind, since the document may have used that number meanwhile. Strands (`s1`, `s2`,
…) are numbered through the document the same way, each by where it starts (`strandAnchors` in
`src/canvas/occurrences.ts`): a strand from the k-th point of the bottom edge is stored under its
cell and that point, shared by every summand of the cell (rewrites keep the bottom edge); a strand
that does not reach the bottom (a bubble, a strand from a cup) under the occurrence ID of its
lowest piece, so it follows that piece through rewrites and edits.

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

**Branches.** A branch is its own Yjs document, kept as an update in the private sidecar's
`branches` map with the inputs it started from. Cloud branch storage is scoped to the current user
and document, never broadcast to collaborators. The page shows either the document or a branch,
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

Claims link to their operations: every tool result sent to the model carries a reference (`ref`:
op1, op2, … in the order of the conversation's calls, a field of a JSON result or a prefix of a
text one), and the system prompt asks the model to end each sentence that states a result with
the reference of the call it comes from (`… simplifies to 0 [op4]`). The reply shows each
reference as a small button (`markdown(…, {refs: true})`, never inside code); clicking it shows
that call's input and full result below the paragraph, or says that no call of the conversation
has that reference. The tool lines in the chat carry their references and results too.

Comments on a cell: "Tell the AI" in an open cell's tools takes an instruction about that cell. It
is the person's own message, so it is an instruction (unlike text in the workspace), sent as
`[comment on cell <id>] <text>` with the cell's first line quoted as data. While the AI is busy it
waits and goes with the next tool results, so the next turn sees it (`run`'s `interject`); while
it is idle it starts a request of its own. The cell lists its comments (waiting or sent) while the
page is open; the chat shows them, and the timeline records only that a comment was made, as it
does for requests.

Milestones: `milestone_write` takes up to four def or watch cells whose current values are the
milestone's key diagrams. Each value is frozen as a snapshot (named after the milestone and the
cell; a result already frozen is reused), and the timeline entry lists the snapshots' keys. The
Activity tab shows the summary as formatted text and each key diagram drawn from its snapshot, with
Open for the snapshot dialog, so a later edit never changes what a milestone reported.

OpenAI is called through its Responses API (`store: false`); a local OpenAI-compatible server
speaks the chat-completions protocol. Both stream through the SSE parser in
`src/ai/provider-stream.ts`, including text, tool calls and usage. The Responses adapter requests
encrypted reasoning and retains the ordered provider output in memory alongside each assistant
content array, so later tool/user turns replay the original reasoning and tool arguments. This
continuation data is not rendered, serialized or synchronized; it lasts only as long as the
conversation. Compatible servers returning JSON instead of SSE still work; a server rejecting
the streaming request reports its error rather than silently retrying a potentially billed call.
The provider, model, effort and key are set on the settings page
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

It also lists the keyboard shortcuts (`src/ui/shortcuts.ts`). The main ones (command palette, AI
panel, ask the AI, scratch branch, export, import, insert a cell below, open the canvas, the
shortcut sheet) can be rebound: Change listens for the next combination, which is refused when it
is fixed (undo, redo, commit, Esc, Tab, select all), already taken, a plain key for an action that
also works while typing in a cell, or a key needed for moving around. Only bindings that differ
from the defaults are kept, in local storage (`diagcalc.shortcuts`); every key handler, the
palette and the sheets read them. The AI section has "Ask before the AI changes a document's
context" (`diagcalc.ai.askBeforeContext`, off by default): when on, `context_set` on one's own
document becomes a proposal like the changes the AI makes to someone else's document.

**Tests (4b).** Unit tests for the canvas geometry, branches and merging, AI tool schemas,
validation and the chat-completions translation. Playwright drives the gestures (drag-slide,
keyboard paths, insert and remove, odd interchange signs, look-only moves), branches (leave,
reopen, accept, undo, discard) and the AI panel against a recorded model: requests to the
Messages API are answered with scripted streamed turns, so a what-if on a branch runs end to end,
including the request's headers, caching and thinking settings.

**Session boundaries.** The chat itself is kept only while the page is open (the timeline is
kept with the document), and pinned watches' trends cover the session only. Provider reachability
is checked in the person's browser; transport tests with recorded responses do not establish
that a particular key or provider accepts live browser calls.

## Accounts and sharing (phase 6)

**Where the server runs.** Pages Functions (`web/functions/api/[[path]].ts`, one function for every
path under `/api/`), not a separate Worker. They are served from the app's own origin, so the
session cookie is first-party and needs no CORS; they deploy with the site in the same
`wrangler pages deploy`, so the app and the API never disagree about versions and every preview
deployment has its API. A Worker routed under the same
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

**Revoked access.** `POST /api/google/risc` receives Google's security events (Cross-Account
Protection, push delivery), the one route that takes neither a session nor a CSRF token. The body is
a security event token, checked like an ID token: an RS256 signature by a key from Google's JWKS
(the keys Google's RISC discovery document names), issuer Google, audience our client ID, issued
no later than now (such tokens have no expiry). For the events sessions-revoked, tokens-revoked
(the person removed this app's access) and account-disabled, every session of the Google account
it names is deleted, as "Sign out everywhere" does; other events are acknowledged with 202. Nothing
about an event is stored, so a delivery Google repeats at most signs the person out again. This
costs one request per event and nothing between them, where re-checking with Google at each
session rotation would need a refresh token kept for every user. It works once the owner registers
the receiver (`docs/deploy.md`, "Revoked Google access"); until then sessions end at their limits.

**Data.** D1 (`web/migrations/`) holds `users`, `sessions`, `documents` (owner, name, times,
version, size, the R2 key of the current snapshot, link role and token), `shares` (document,
lower-case e-mail address, role) and `sent_mail` (only sender and time, for the daily limit). Shares
are by address, so a document can be shared with someone who has never signed in. R2 holds each
document's content, a Yjs update, under `docs/<id>/snapshots/<random>`, and exported files under
`docs/<id>/files/<name>`. The live room reads snapshots to validate and merge CRDT structure; it
never computes mathematics. Migration adds document generations, checkpoint and personal-branch
metadata, and bounded socket/nonce admission records.

**REST saving.** `PUT /api/docs/:id/snapshot` carries `If-Match: "<version>"` and
`X-Doc-Generation`. Pages authenticates it and forwards it to the live room, which serializes it
with WebSocket writes and stages structural validation before changing authority. Content is written to
a fresh R2 key, then D1 moves the version on only if it is still the one the page read; otherwise
the fresh object is deleted and the answer is 409 with the current version. The page then fetches
the current content, merges it into its own (Yjs merges without losing either side's steps) and
saves again within the same generation. A generation mismatch instead preserves a detached local
recovery copy and reopens a fresh current-generation document; it must never merge old schema
updates into the new room. A save that loses the race never overwrites another, and the previous
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
ID tokens and security event tokens; a fake Resend records messages; a clock is injected. The
tests cover the PKCE test vector of RFC 7636, every ID-token check, Google's security events and
their checks, the whole sign-in and its failures (wrong state, missing
or tampered cookie, reused code, replayed nonce), session expiry and sign-out, CSRF, the role of
each kind of caller on every route, link sharing, saving races, notifications and their limit,
deleting documents and accounts, and that no user data reaches the log. The functions also build
with `wrangler pages functions build`, and the migrations apply with
`wrangler d1 migrations apply --local`.

**The page.** `src/account/`. At start the page asks `GET /api/me` (five-second limit). When the
answer is not the API's JSON (a static host, the development server, no network) or says accounts
are not available, the page is the anonymous app, unchanged: no button, no request beyond that
one. Otherwise the document list shows Sign in (a navigation to `/api/auth/login?return=` the
current page) or the account's name and Sign out, and a failed or cancelled sign-in is reported
once from the `?signin=` the server returns with.

**Saving to the account.** The local copy in IndexedDB stays the working copy and the offline
cache; its index entry gains a link to the account (the server's ID and version, the Yjs state
vector of that version, the role, and a link token when it was opened by one). Syncing
(`src/account/sync.ts`) fetches the server's content only when its version moved on, applies it
with the origin `cloud` (so it is neither a step nor undoable), and saves only when this copy has
updates the server's state vector lacks; a 409 means fetch, merge and save again. Viewers only
fetch. On sign-in the document list syncs everything (`syncAll`): documents made here are saved to
the account (each once, even if an open document tries at the same moment), the account's
documents missing here are copied here, and changed copies are brought in step. A document that
belongs to another account signed in earlier on this browser is left alone and tagged. An open
document (`src/account/docsync.ts`) prefers the live provider. Local commits and evaluation happen
immediately, independent of network acknowledgement; remote evaluation coalesces bursts. REST
fallback syncs on open, after changes, on hide and periodically. The top bar distinguishes live,
saving, offline, view-only and migration/recovery states. Viewers have no editable shared controls.
Detached recovery copies carry a durable local-only marker respected by all upload paths.
Exporting a `.dcw` also keeps a copy with the document in R2; "Files kept with this document" in
the command palette lists them, to download, open as a new document or (for editors) remove.
Deleting an owned document deletes it from the account; removing a shared one only removes the
copy here. Signing out keeps the copies in this browser, unless the person ticks "Also remove this
account's documents from this browser" (for a shared computer): every copy is first synced, then
removed, except one with unsaved work, local-only recovery or private branches, which is kept and
counted in the notice. Clear-sign-out explicitly warns that private checkpoints of removed copies
are permanently lost; checkpoint-only history does not force every copy to be retained. The
result cache is cleared too.

**Sharing.** The Share button opens a dialog: add a person by address as viewer or editor (with or
without an e-mail), change or remove their access, and set the link (off, anyone can view, anyone
signed in can edit), with the link to copy. Non-owners see who has access and can leave. "Shared
with me" on the document list opens `#/s/<id>`, which finds or makes the local copy and goes on to
`#/doc/<local id>`; `#/s/<id>/<token>` does the same through a link, without an account for
viewing.

**Browser tests.** `tests/accounts.spec.ts` runs the app against `tests/mockapi.ts`, an in-memory
stand-in for the API with the same routes and rules (roles, CSRF token, versioned saves), shared by
several browser contexts so they can be several devices and several people. It checks that with no
API the app is unchanged, the sign-in round trip and its failure message, the phase 6 gate end to
end (a document made anonymously survives sign-in, opens on a second device, and is shared
read-only with a second account, whose shared editing controls are disabled), opening by a view link without an
account, signing out (keeping the copies, or removing them on a shared computer), the files kept
with a document, and deleting from the account.

## Live co-editing (phase 7)

`web/live/room.ts` exports the SQLite-backed `DocRoom`. The production Worker fetch handler is
always forbidden; Pages reaches the class through its external `LIVE` binding. Signed short-lived
tickets bind document, generation and session/link access, with one-use nonce admission. The room
rechecks access on messages and within one minute while idle. Terminal status frames revoke the
client immediately without relying on a peer's close handshake. Viewers never write.

Updates are staged on a clone, bounded by message and resulting-document limits, then persisted
to R2/D1 and transactional DO storage before broadcast and the sender's single durable-save ack.
The room repairs colliding public cell IDs without changing UUIDs. Checkpoints and personal
branches are distinct from the shared working state. Generation-zero documents are read-only
until an owner migrates them, preserving the exact legacy checkpoint and detached local work.

`src/account/live.ts` speaks the Yjs protocol and maintains authenticated presence. A private
editor draft is never a shared Yjs edit. Committed-text cursors use relative positions; private
branches publish only generic branch/AI-active presence, not their names, text or selections.

Shared proposal decisions use a hash of the full shared update (including deletions), a staged
candidate and a room compare-and-set. The successful client applies the decided change as one
local undo step; competing decisions fail without a second application. Pinned results capture
their dependency workspace and run settings; recomputation compares value and provenance, not
incidental SVG/drawing IDs, and leaves the shared document unchanged.

`tests/live.spec.ts` and `tests/recovery.spec.ts` exercise multiple independent browser contexts,
private drafts, UUID repair, reconnect, recovery, proposals and presence privacy. The separate
`live/` Miniflare tests execute the production room and Pages router with real storage and session
signatures, including save readback, nonce/session/origin rejection, revocation, generation fencing
and idle-alarm cessation. Browser transport fixtures alone are not a security or persistence proof.
