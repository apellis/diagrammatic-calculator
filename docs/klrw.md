# Weighted KLR algebras: conventions

This note fixes how the calculator reads weighted KLR algebras (KLRW algebras) and the tensor
product algebras built from them. The Lean formalization of the same algebras uses the same
conventions, so that its basis is literally the calculator's basis and the calculator's
certificate can be `lean`. It extends the KLR section of `docs/conventions.md`; where the two
overlap they agree.

Source: B. Webster, *Weighted Khovanov–Lauda–Rouquier algebras*, arXiv:1209.2463v4 (Doc. Math.
24 (2019)), §2.1 (Definitions 2.3–2.5, Proposition 2.7, Theorem 2.8, Definition 2.11) and §3.1,
§3.4. Tensor product algebras: B. Webster, *Knot invariants and higher representation theory*,
arXiv:1309.3796v2, §4. Statement numbers are given by label where the arXiv version's numbering
is ambiguous.

## Data

- A finite set of colours `I`, with a symmetrizer `d_i > 0`.
- A finite set `Ω` of oriented edges `e : t(e) → h(e)` between colours, each with a **weight**
  `ϑ_e ∈ ℚ`, multiplicities `c_e, c_ē ≥ 0` with `d_{h(e)} c_e = d_{t(e)} c_ē`, and a polynomial
  `Q_e(u, v)` over the ground ring. In `Q_e(u, v)` the first variable `u` is the dot on the
  strand coloured `h(e)` and `v` the dot on the strand coloured `t(e)`. `Q_e` is homogeneous of
  degree `2 d_{h(e)} c_e` when `u` has degree `2 d_{h(e)}` and `v` degree `2 d_{t(e)}`, and the
  coefficients of its two pure powers are units. Webster's paper allows real weights; the
  calculator uses rational ones, which loses nothing (only the order of finitely many points
  matters, and that order is realized by rational positions).
- Assumption (†) of the paper: no two edges with the same tail, head and weight, and no oriented
  cycle of length two whose two weights are opposite. Edges violating it are merged first, the
  merged `Q` being the product.
- A loop `e` of weight `0` at `i` (at most one) has `Q_e(u, v) = (u − v) P_i(u, v)` with `P_i`
  symmetric. The first implementation allows no loops; they are a later extension.

**From a Cartan datum.** For `klr`-style input with a Cartan datum and polynomials `Q_ab`, the
default graph has one edge `e : a → b` for each pair of colours `a > b` (in the colour order) with
`(α_a, α_b) ≠ 0`, with `Q_e = Q_ba`, `c_e = −a_ba`, `c_ē = −a_ab`, and weight `0` unless the
context says otherwise. With this orientation the polynomial representation at weight `0`
coincides with the calculator's existing KLR polynomial representation (`P_ab = 1` for `a < b`,
`P_ab = Q_ba` for `a > b`).

## Loadings

A **loading** is a finite set of points of `ℚ` labelled by colours, listed from left to right:
`(i_1 @ x_1, …, i_n @ x_n)` with `x_1 < … < x_n`. Strands are numbered `1, …, n` from the left.

**Ghosts.** For each edge `e` and each point coloured `h(e)` at `x`, there is a ghost of that point
at `x + ϑ_e` (to the right when `ϑ_e > 0`). Points coloured `t(e)` interact with these ghosts;
nothing else interacts with a ghost.

A loading is **generic** when no point coloured `t(e)` sits on a ghost for `e`, and no two ghosts
for edges of different weights coincide. Only generic loadings are objects.

**Isotopy.** Two generic loadings with the same colours in the same order, and with every point
on the same side of every ghost and every red strand it interacts with, are joined by a
straight-line diagram with no events, so they are the same object. Points and ghosts that do not
interact may pass through each other: requiring otherwise would make bigons of non-interacting
strands nontrivial and break the basis theorem. The calculator keys each object by this order
type and prints it in the form the user gave.

**Equivalence** (Definition 2.11). Two loadings are equivalent when, for every edge `e`, every
point coloured `h(e)` and every point coloured `t(e)`, the ghost lies on the same side of the
point in both. Equivalent loadings give isomorphic idempotents; they are different objects.

## Diagrams and composition

Diagrams are as in Definition 2.4: strands monotone in height, carrying dots, with their ghosts,
generic (no tangencies, no triple points among strands and ghosts that interact, no dots on
crossings), up to isotopy through generic diagrams. `a b` is `a` stacked on top of `b`, as for
KLR: words are read from the bottom and multiplied on the left. Dots are written `x_k`
(Webster: `y_k`), `k` counted from the left at that height.

**Degrees.** A dot on a strand coloured `i` has degree `2 d_i`; a crossing of two strands coloured
`i` has degree `−2 d_i`; a crossing of two strands of different colours has degree `0`; a crossing
of a strand coloured `t(e)` with a ghost for `e` has degree `d_{h(e)} c_e = d_{t(e)} c_ē`, in
either direction. This is the normalization forced by the bigon relation below (the two crossings
of a bigon add up to the degree of `Q_e`), and at weight `0` it gives the KLR degree
`−(α_a, α_b)` for a crossing of different colours. Webster's grading in arXiv:1209.2463v4 §2.1 is
exactly half of this one (there `Q_e` has degree `d_{h(e)} c_e` with `u` of degree `d_{h(e)}`);
the two are consistent, and the calculator and the Lean formalization use the doubled grading.

## Polynomial representation

(Proposition 2.7.) The algebra acts on `⊕_B k[x_1, …, x_n]`, one copy per loading `B`, with `x_k`
the dot on the `k`-th strand from the left. Reading a diagram from the bottom, each event acts:

| Event | Action |
|---|---|
| dot on strand `k` | multiplication by `x_k` |
| strand `j` coloured `t(e)` passes from the left of the ghost for `e` of strand `k` to its right | multiplication by `Q_e(x_k, x_j)` |
| the same, from right to left | identity |
| strands `j, j+1` of different colours cross | `s_j` (exchange `x_j` and `x_{j+1}`) |
| strands `j, j+1` of the same colour cross | `∂_j f = (f − s_j f) / (x_j − x_{j+1})` |

Strand indices `j, k` refer to the positions at the height of the event. The Demazure operator is
Webster's `(s_j − 1)/(x_{j+1} − x_j)` and the calculator's existing `∂_k`. Over an integral domain
in which every `Q_e` is nonzero this representation is faithful (proof of Theorem 2.8).

The golden corpus (`tests/golden/`, `docs/conventions.md`) includes `klrw` cases: the Lean
program `lean/golden/Golden.lean` applies the events of this table, one straight-line diagram at
a time, and the engine must reproduce its action tables. It covers `T̃^λ` only, since the
representation is not faithful on the cyclotomic quotient.

## Basis

(Theorem 2.8.) For loadings `B` (bottom) and `B'` (top) with `n` strands, `e_{B'} W e_B` is a free
`k[x_1, …, x_n]`-module with basis `b_π`, one for each permutation `π` with `colour(B'_{π(k)}) =
colour(B_k)`, where strand `k` of `B` ends at position `π(k)` of `B'`. The theorem holds for any
choice of `b_π` in which no two strands, and no strand and ghost, cross twice. The calculator and
the Lean formalization fix this choice:

**`b_π` is the straight-line diagram.** Strand `k` runs from `(x_k, 0)` to `(x'_{π(k)} +
ε^{π(k)}, 1)` along a straight line, where `ε > 0` is infinitesimal; each ghost moves with its
strand. Two lines cross at most once, so no pair crosses twice. The infinitesimal perturbation of
the top endpoints removes every coincidence: events are ordered by height, comparing heights as
rational functions of `ε` for small `ε`, and events at the same height (necessarily at different
places) commute. The top endpoint at `ε = 0` is the given loading `B'`, and the resulting diagram
is a word of elementary events (dots, crossings of two strands, crossings of a strand and a ghost)
between generic loadings, which is how it is stored and how the Lean definition computes it.

**Normal form.** Every element of `e_{B'} W e_B` is uniquely `Σ_π b_π · f_π(x)` with dots at the
bottom, as for KLR (`ψ_ŵ x^a e(i)`). It is computed by peeling the longest permutation in the
polynomial representation: `b_π` acts as `c_π · π + (shorter)` with `c_π` a product of `Q`
factors and Demazure denominators, so the coefficient of the longest `π` determines `f_π`.

At weight `0` with spacing large compared with the weights, `b_π` is generally not the
calculator's KLR basis word `ψ_ŵ` (the reduced word differs); both are bases, and the KLR
presentation keeps its own.

## Red strands and tensor product algebras

Red strands come from the Crawley-Boevey trick (§3.1, §3.4). For dominant weights `λ_1, …, λ_ℓ`
at positions `ϖ_1 < … < ϖ_ℓ`, the calculator draws **red strands** labelled `λ_j` as fixed
vertical lines at `ϖ_j` (Webster: ghosts of the single strand coloured `0`, which is fixed in
place and carries no dots in the reduced quotient). A loading then lists black points and red
points; red points never move.

- A black strand coloured `i` crossing the red strand `λ_j` from left to right acts by
  multiplication by `x^{⟨λ_j, α_i^∨⟩}` on that strand; from right to left it acts by the
  identity. (In the trick, the edges `e : i → 0` carry `Q_e(u, v) = v − u` with `u` the dot on the
  `0` strand, set to zero.)
- The crossing has degree `d_i ⟨λ_j, α_i^∨⟩ = (α_i, λ_j)` in either direction.
- The relations are the red bigon `= x^{⟨λ_j, α_i^∨⟩}` on the black strand (on either side) and the
  red triple point correction `Σ_{a+b=λ−1} x_k^a x_{k+1}^b` (both from arXiv:1309.3796v2
  (4.1)), which the polynomial representation encodes.
- This algebra is Webster's `T̃^λ`. Its **cyclotomic quotient** `T^λ` kills every idempotent
  whose leftmost strand is black (the violating idempotents, arXiv:1309.3796v2 §4). In the
  weighted setting this is the reduced steadied quotient for the charge of §3.1; general steadied
  quotients are a later extension.
- The polynomial representation is faithful on `T̃^λ`, not on `T^λ`. Equality in `T^λ` is decided
  by the basis normal form of `T̃^λ` followed by the violating ideal, using the standardly
  stratified basis (arXiv:1309.3796v2 §4–5); the tensor-product Shapovalov form gives an
  independent check on `K_0(T^λ) ≅ V(λ_1) ⊗ … ⊗ V(λ_ℓ)`.
- The check (`crates/core/tests/klrw_shapovalov.rs`) uses loadings whose red and black strands
  are at weight `0` from each other. For such `B`, `B'`, `dim_q e_{B'} T^λ e_B = ⟨u_B, u_{B'}⟩`
  exactly, with no overall shift. Here `u_B` is built from the left: the black strands between
  red strands `j` and `j + 1` act on `V(λ_1) ⊗ … ⊗ V(λ_j)` by `F_i`, leftmost strand first,
  through `Δ(F_i) = F_i ⊗ K_i + 1 ⊗ F_i`. The form is the product over the factors of
  `q^{D_m/2}` times the Shapovalov form of `V(λ_m)` (normalized by `⟨v, v⟩ = 1`), with
  `D_m = 2(λ_m, ν_m) − (ν_m, ν_m)` for the weight `ν_m` lowered in factor `m`. The other three
  sign choices in the coproduct and the bar-inverted form fail on these examples. This form
  agrees with the corrected pairing of categorification-lean
  (`Stendhal.TensorPairing.correctedForm`, `fundamental_prefix_homGdimPoly_eq_correctedForm`):
  for two fundamental `sl_2` factors and one black strand the Hom matrix of the two boundary
  loadings is `[[1, q], [q, 1 + q²]]`, while the pairing as printed in arXiv:1309.3796v2,
  Theorem 4.38, gives `1 + q⁻²` in the last entry (`terminal_homGdimPoly_ne_printedForm`). The
  test `two_fundamental_factors_hom_matrix` checks the calculator's matrix.
- A second, independent check (same file, `cyclotomic_dimensions_match_lean_corner_recursion`)
  compares `dim_q e_{B'} T^λ e_B` with the deletion recursion that categorification-lean proves
  equal to it (`Stendhal.cornerPolynomial`, `Stendhal.sequenceCornerPoly_eq_wordCornerPoly`;
  arXiv:1309.3796v2, §4.7, Lemma 4.36), in types `A_1`, `A_2`, `B_2`, and `G_2` and with up to
  three red strands. Read a loading as a word from left to right and let `p_i(w)` be
  `⟨λ − ν, α_i^∨⟩` at the right end of `w` (red multiplicities at `i` minus `Σ a_ij` over the
  black letters). A terminal black `i` of one word, with the rest `u`, is deleted against each
  black `i` of the other word, with prefix `p`, at the factor
  `q_i^{p_i(u) − 1} [p_i(p)]_{q_i}`; a common terminal red is removed; the empty corner is `1`.
  The grading is the canonical one (a dot on `i` has degree `(α_i, α_i)`), which is the
  calculator's.

## Ground ring

Definitions and the basis theorem hold over any commutative ring. The faithful representation
needs an integral domain with every `Q_e ≠ 0`, as for KLR. The characteristic-zero assumption in
§4 of arXiv:1209.2463v4 concerns the geometric realization (perverse sheaves on quiver
representations), not the algebra.

## Contexts and terms

A context line `context klrw type=A2 theta_ij=1/2 red="2i@0, j@5/2"` takes the Cartan datum and
the polynomials `Q` of the corresponding `klr` context, builds the default graph above, and then:

- `theta_ab=x` gives the edge between `a` and `b` the weight `x`, oriented with tail `a` and head
  `b` (reorienting the default edge if needed);
- `red="λ@ϖ, …"` adds red strands, each a dominant weight written as for `klr` (`2i`, `i+j`) at a
  rational position; two red strands may not share a position.
- `quotient=cyclotomic` computes in `T^λ` instead of `T̃^λ`: normal forms are reduced modulo the
  ideal of violating idempotents. It needs a field of coefficients; scalars (for instance the
  unknowns of a solve cell) are allowed as long as `Q` does not involve them.

**Computing in `T^λ`.** In each degree `e_{B'} T̃^λ e_B` is a finite-dimensional space with basis
`b_π x^m`. Its intersection with the violating ideal is spanned by the elements `v x_k`, with `v`
in the ideal in lower degree, and by the products `b_σ x^c b_τ` of two basis diagrams composed
through one violating loading (the middle idempotent) of each order type with the colours of `B`.
A single basis diagram need not pass through a violating loading to lie in the ideal: for one red
strand `λ` and one black strand `i` with `⟨λ, α_i^∨⟩ = 1`, the dot on the black strand to the
right of the red one is the red bigon, which factors through the violating loading. The order types are enumerated exactly (a feasibility test
of strict difference constraints on the positions). The normal form in `T^λ` is the reduced
representative modulo this span, with the columns ordered as the normal form is printed (longer
`π` first, then lexicographically larger `m` first), so it is unique.

Violating means only that the leftmost strand is black. Whether a loading can be moved to a
violating one depends on the weights: with `type=A2 red="i@0"`, the idempotent of `i` followed by
`j` survives when the ghost of `j` lies to its left (`theta_ij=-1/2`), and every idempotent of that
weight is violating when it lies to its right (`theta_ij=1/2`), because `j` then crosses the red
strand without its ghost passing `i`. Webster's tensor product algebras correspond to the first
choice (red strands far apart compared with the weights, ghosts on the side of the charge); the
calculator does not check this.

Terms are typed, or edited on the canvas (see Display). A term is a linear combination of
compositions, written top factor first, of:

| Syntax | Meaning |
|---|---|
| `e(i@0 j@1/2)` | the idempotent of a loading; on top of a diagram, it keeps only the part ending there |
| `x_k` | a dot on the `k`-th strand from the left |
| `psi_k` | strands `k` and `k+1` cross: their colours swap, the positions stay |
| `to(i@0 j@3)` | the straight-line diagram to a loading with the same colours in the same order |
| `b(π; B')` | the basis element `b_π` to the loading `B'`, with `π` in one-line notation counted from 1 |

Every generator needs a loading below it, so a term ends in `e(…)`. As in KLR contexts, a term
may also use names of definitions, `Q_ij(a, b)` with polynomials `a` and `b` in the dots, powers
`x_1^{n}` with a symbolic exponent, and sums `sum_{f=0..n} …` over a range. `a (x) b` (or `a ⊗ b`)
is the tensor product: the diagrams side by side, with every point of `b` moved right by the least
whole number that puts all of `b`, ghosts included, right of all of `a`. Red strands belong to the
context, so the tensor product of two diagrams with red strands lies in another algebra: it is
refused in contexts with red strands or the cyclotomic quotient.

Normal forms are printed as
`c b(π; B') x^a e(B)`, written `to(B')` when `π` is the identity and `B'` is not isotopic to `B`,
and with no diagram when it is. Workspaces accept `def`, `watch`, `check`, and `solve` cells, and
`workspace.trace` follows a summand back to the definitions it came from. The checks are
`equal`, `zero`, `idempotent`, `orthogonal` (both products of two terms vanish), `central`
(commutes with every generator of the algebra), and `primitive` (an idempotent `e` with `eAe`
local; only in the cyclotomic quotient, where `eAe` is finite-dimensional, decided by the rank of
the trace form on `eAe`, which is 1 exactly when `eAe/rad` is one-dimensional, over a field of
characteristic zero). The quotient `quotient=cyclotomic` needs a field, and accepts scalars as
long as the `Q_e` do not involve them.

## Display

Ghosts are drawn as thin dashed lines in the colour of their strand, offset by `ϑ_e`; red strands
are drawn thick and red. Ghosts are a view: Settings › Appearance › "Show ghost strands" hides
them in every drawing without changing any element. Hidden ghosts leave the positions of
strands unchanged, so crossings with a ghost appear as bends in the strand.

**Canvas.** The canvas draws each summand of the normal form bottom to top: one layer per dot
(dots by strand, left to right), then, unless the summand is an idempotent, one layer, two units
high, for the straight-line diagram `b_π` (or `to(B')`). Strands sit at their positions in the
loading, shifted so that the leftmost point, ghost, or red strand is at `x = 0`, and stretched
horizontally when two of them are less than 0.6 apart (by at most a factor of 4; the drawings in
the document and the TikZ output do the same); every height between layers lists the drawn positions of its strands
(`xs` in `term.layout`). Dropping a dot or a crossing (`psi_k`, colours swapped, positions kept) onto the
diagram, or deleting a layer, edits the term and reduces it to the normal form again, so one edit
can give several summands. The diff viewer pairs unchanged summands and keeps the dots a new
summand shares with an old one. Rewriting with relations (`relations.match`, `relation.apply`,
`interchange`, `slide`, `straighten`, `chain.find`, `export.lean`) does not apply: terms are always
in the normal form, so the canvas offers no relations, no dot slides, and no interchanges, and
the operations say why when called directly.

TikZ output (`term.render` with `format: "tikz"`, the TikZ copy button) draws the same
picture: straight strands, dots near the bottom, ghosts dashed and gray, red strands thick and red,
one unit of position per unit of `x`, stretched as on the canvas.

## Certificate

Without red strands or a cyclotomic quotient, results carry the certificate `lean`:
categorification-lean proves Theorem 2.8 for Webster's algebra (the corner of its gallery algebra
at the rank-ordered chambers) under Webster's standing hypothesis on the `Q_e`, over any
commutative ring, with the straight-line segments whose top endpoints are moved by `ε^{ρ(k)}`,
`ρ` injective and nonzero (`WeightedKLR.GalleryAlgebra.straightWeightedKLRBasis`,
`straightWeightedKLRChamberBasis`; the general form for any simple segments is
`weightedKLRBasisOfStanding`, and the segments are `straightSegment`, `straightBasis`).
The calculator's `b_π` is the case `ρ(k) = π(k)`; that identification is read off this document,
not proved in Lean. Independence is proved through the polynomial representation, so the
representation is faithful. With red strands and no quotient, results are `lean` as well:
categorification-lean proves the basis of the reduced quotient for the Crawley-Boevey graph with
new edges `i → 0` (`WeightedKLR.GalleryAlgebra.redReducedBasisIn`, on `crawleyBoeveyIn`), the
calculator's orientation, and that the reduced quotient acts faithfully on the polynomial
representation with the dot on the `0`-strand set to zero (`linearIndependent_redReducedRepIn`,
`redReducedRepIn_injective`), which is how the calculator decides equality. The hypotheses are
the standing hypothesis, (†), no loops, and distinct red positions. In `T^λ` the certificate is
`lean` when every edge weight is `0`. Then the violating ideal is spanned by products of two basis
terms through a violating idempotent (`Stendhal.mem_stViolatingIdeal_iff`), and killing the
violating idempotents of all chambers, as the calculator does, gives exactly the quotient of the
algebra cut down to the Stendhal chambers (`WeightedKLR.mkₐ_mem_stViolatingIdeal_iff`; every
chamber is a relabelling of a Stendhal chamber). That quotient is Webster's `T^λ`
(`Stendhal.stTensorEquiv`, arXiv:1209.2463v4 Theorem 3.6), with the black/red crossings rescaled
by `(−1)^{α^∨(λ_r)}`. With some nonzero edge weight the certificate stays `literature`: Lean
defines Stendhal chambers only when every edge weight is `0`.
