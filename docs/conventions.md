# Conventions

Each convention below changes answers silently if it is read differently, so each is stated here,
used by both the parser and the printer, and pinned by the golden corpus (`tests/golden/`).
References of the form `Library: Declaration` point to the Lean libraries
[categorification-lean](https://github.com/apellis/categorification-lean),
[string-diagrams-lean](https://github.com/apellis/string-diagrams-lean) and
[odd-lean](https://github.com/apellis/odd-lean), which the calculator matches.

## Reading terms

- **Composition.** `a b` (also `a * b`) means `a ∘ b`: `b` is drawn first, at the bottom, and `a`
  above it. A term read from left to right goes from top to bottom. This is the order of the
  product in categorification-lean (`KLR.KLRAlgebra`: in `ψ j * e i` the idempotent is at the
  bottom) and in odd-lean (`f * g = g ≫ f`). string-diagrams-lean's `≫` is the opposite,
  diagrammatic order.
- **Positions, not strands.** In `x_k` and `psi_k` the subscript is a position counted from the
  left, starting at 1, at the height where the generator sits; a generator whose source has `s`
  strands occupies positions `k, …, k+s-1`. It does not name a strand: after `psi_1`, position 1
  holds the strand that started in position 2. The Lean libraries count from 0, so `x_k` here is
  `x ⟨k-1⟩` there and `psi_k` is `ψ (k-1)`.
- **Idempotents.** `e(i_1 … i_n)` lists colours from left to right and sits at the bottom of what
  is written to its left: `psi_1 e(ij)` starts from the sequence `ij` and ends at `ji`. Colour
  names that are single characters may be run together (`e(iij)`); longer names are separated by
  commas or spaces. A word written without an idempotent stands for the sum over every object of
  the context it applies to (in `R(ν)`, every sequence of `ν`), which is the usual meaning of
  `x_1 ∈ R(ν)`.
- **Tensor product.** `f ⊗ g` (ASCII `f (x) g`) places `f` to the left of `g` and means
  `(f ⊗ 1)(1 ⊗ g)`: `g` at the bottom, `f` above it. With this convention the super interchange law
  reads `(f ⊗ g)(h ⊗ k) = (-1)^{|g||h|} (fh) ⊗ (gk)`.
- **Scalars.** Numbers, `I` (a square root of −1 in ℤ[√−1]), parameters (`q`, `t_ij`, `pi` or `π`)
  and declared unknowns (`c1`) are scalars; a scalar written alone is that multiple of the
  identity. `^` takes integer exponents, negative only for invertible scalars.

## Signs

- **Koszul signs.** Exchanging the heights of two generators on disjoint strands costs
  `(-1)^{|f||g|}`, whichever is on the left (string-diagrams-lean: `InterchangeData.sign`,
  `diag_interchange_diagrams`). Coefficients are even. The parity of a word is the number of odd
  generators in it, mod 2.
- **Canonical layering.** Words equal up to interchange are stored in one canonical layering:
  every generator as low as it can go, generators at the same height ordered left to right. The
  Koszul sign of the moves is absorbed into the coefficient. A generator without inputs (a cup)
  sits above its neighbouring strands. A generator without outputs (a cap) leaves its height in
  the gap it closes, so a cap enclosing it sits higher. Sorting repeats until the word sorts to
  itself.

## Weights and regions

In a 2-category (KL, BE2) the regions between strands carry weights. A context fixes the weight
`μ` of the rightmost region with `mu=`, in fundamental-weight coordinates: `mu=2i-j` means
`⟨h_i, μ⟩ = 2`, `⟨h_j, μ⟩ = −1`; a bare integer serves a one-colour datum, and `mu=0` any datum.
Moving left across a strand of colour `j` adds `α_j` if the strand points up and subtracts it if it
points down, so the region left of position `p` of an object `s` has weight
`μ + Σ_{k ≥ p} ±α_{s_k}`. This is the libraries' convention: only the rightmost weight is stored,
and the region right of a generator is `wt μ v` (categorification-lean KL3 `wt`, odd-lean SKM `wt`).
In a degree, `h(a)` is `⟨h_a, ν⟩` for the region `ν` just right of the generator; in a relation or
a term it refers to `μ`.

## KLR algebras

Data: a Cartan datum given by the symmetric form `(α_i, α_j)` (equivalently a symmetrizable Cartan
matrix `a_ij` with `(α_i, α_j) = d_i a_ij`), a weight `ν`, and polynomials `Q_ij(u, v)` for `i ≠ j`
with `Q_ij(u, v) = Q_ji(v, u)` (categorification-lean: `KLR.KLRAlgebra`, `KLRSymm`). All formulas
below are multiplied on the right by `e(i)`, and colours `i_k` are read from the bottom sequence
`i`.

| Relation | Formula | categorification-lean |
|---|---|---|
| dot slide | `(x_k ψ_k − ψ_k x_{k+1}) e(i) = δ_{i_k i_{k+1}} e(i)` | `Rel.dot_cross_left` |
| dot slide | `(ψ_k x_k − x_{k+1} ψ_k) e(i) = δ_{i_k i_{k+1}} e(i)` | `Rel.dot_cross_right` |
| square | `ψ_k² e(i) = 0` if `i_k = i_{k+1}`, else `Q_{i_k i_{k+1}}(x_k, x_{k+1}) e(i)` | `Rel.cross_sq` |
| braid | `(ψ_k ψ_{k+1} ψ_k − ψ_{k+1} ψ_k ψ_{k+1}) e(i) = Q̄(x_k, x_{k+1}, x_{k+2}) e(i)` if `i_k = i_{k+2} ≠ i_{k+1}`, else 0 | `Rel.braid` |

Here `Q̄(a, b, c) = (Q(a, b) − Q(c, b)) / (a − c)` with `Q = Q_{i_k i_{k+1}}` (`qbar_spec`). In
`Q_ij(u, v)` the first variable `u` is the dot on the left strand.

The same-colour sign is opposite to Kang–Kashiwara and Brundan–Kleshchev, whose crossing is
`τ = −ψ` on equal colours (categorification-lean: `sameCross`). The calculator follows the
library.

**Degrees.** `deg x_k e(i) = (α_{i_k}, α_{i_k})` and `deg ψ_k e(i) = −(α_{i_k}, α_{i_{k+1}})`
(`klGradingDatum2`). All KLR generators are even.

**Default `Q`.** Unless the context says otherwise,
`Q_ij(u, v) = u^{d_ij} + v^{d_ji}` when `(α_i, α_j) ≠ 0` and `Q_ij = 1` otherwise, with
`d_ij = −a_ij` (`klQ2`). The context may instead give Cautis–Lauda scalars:
`Q_ij = t_ij u^{d_ij} + t_ji v^{d_ji} + Σ s_ij^{pq} u^p v^q` (degree-filtered), and `Q_ij = t_ij`
when `(α_i, α_j) = 0` (`qCL`).

**Cartan types.** Bourbaki numbering, colours named `i, j, k, l, …` in order. In `B2` the first
root is long and the second short; in `G2` the first is short. (The Lean libraries contain no
fixed `B2` or `G2` datum; their test datum `cB2` has vertex 0 short.)

**Polynomial representation.** `R(ν)` acts on `⊕_i k[x_1, …, x_n] e(i)`
(categorification-lean: `PolyRep.crossComp`):

- `x_k` multiplies by `x_k`;
- `ψ_k e(i)` with `i_k = i_{k+1}` acts by `∂_k f = (f − s_k f)/(x_k − x_{k+1})`;
- `ψ_k e(i)` with `i_k ≠ i_{k+1}` acts by `f ↦ P_{i_k i_{k+1}}(x_k, x_{k+1}) · s_k f`, landing in
  `e(s_k i)`,

where `Q_ij(u, v) = P_ji(u, v) P_ij(v, u)`. The calculator uses `P_ij = 1` and `P_ji = Q_ij` for
`i < j` in the order of colours. For the default `Q` this is categorification-lean's `KL2.klP2`
with the orientation `a → b` exactly when `a > b`. Over an integral domain this representation is faithful when
every `Q_ij` (`i ≠ j`) is nonzero; for the default `Q` this is proved in categorification-lean
(`KL2.polyRepKL2_injective`), and for general `Q` it is Rouquier, arXiv:1112.3619v1, §3.2. Over a
ring with zero divisors it can fail (categorification-lean README, on Rouquier,
arXiv:0812.5023v1, Prop. 3.12).

## The Khovanov–Lauda 2-category

`presentations/kl.pres.toml` is the 2-category `U(g)` of Khovanov–Lauda (A categorification of
quantum sl(n), arXiv:0807.3250v1, Definition 3.1), in the form formalized in categorification-lean
(`KL3/Relations.lean`, `Rel`). An upward strand `a` is `E_a`, a downward strand `a*` is `F_a`. Degrees use the region `ν` just right
of the generator:

| Generator | Written | Source → target | Degree | categorification-lean |
|---|---|---|---|---|
| `x`, `xd` | `x` | `a → a`, `a* → a*` | `(a,a)` | `.dot (up a)`, `.dot (dn a)` |
| `psi`, `psid` | `psi` | `a b → b a`, `a* b* → b* a*` | `−(a,b)` | `.cross true a b`, `.cross false a b` |
| `cup` | `cup_{k,a}` | `1 → a a*` | `d_a(1 − h(a))` | `.cup (up a)` |
| `cupd` | `cup_{k,a*}` | `1 → a* a` | `d_a(1 + h(a))` | `.cup (dn a)` |
| `cap` | `cap` | `a a* → 1` | `d_a(1 − h(a))` | `.cap (dn a)` |
| `capd` | `cap` | `a* a → 1` | `d_a(1 + h(a))` | `.cap (up a)` |

Bubbles: `cw_{a,m}` is `cap_1 x_2^{m} cup_{1,a}` (a clockwise circle with `m` dots on its downward
side) of degree `2 d_a (m + 1 − h(a))`, `ccw_{a,m}` is `cap_1 x_2^{m} cup_{1,a*}` of degree
`2 d_a (m + 1 + h(a))`, with `h(a)` read in the region the bubble sits in. Bubbles of negative
degree are zero. A label `m < 0` with nonnegative degree names a fake bubble: writing `c_k` and `c'_k` for the
bubbles of degree `2 d_a k`, `c_0 = c'_0 = 1` and `Σ_{j=0..k} c_j c'_{k−j} = 0` for `k > 0`
(categorification-lean `cwL`, `ccwL`, `grassInv`).

Relations (the names of `kl.pres.toml`, then `Rel`): the four zigzags (biadjointness, from the
pivotal structure); `cyclic-dot-*`, `cyclic-crossing-*` (`cycDotR`, `cycDotL`, `cycCrossR`,
`cycCrossL`): the downward dot and crossing are the rotations of the upward ones;
`bubble-*-negative`, `bubble-*-one` (`cwNeg`, `ccwNeg`, `cwOne`, `ccwOne`); `curl-right`,
`curl-left` (`curlR`, `curlL`); `decompose-ef`, `decompose-fe` (`decompEF`, `decompFE`), with the
sideways crossings `cap_3 psi_2 cup_{1,b*}: E_a F_b → F_b E_a` and `cap_1 psi_2 cup_{3,b}: F_b E_a →
E_a F_b`; `mixed-ef`, `mixed-fe` (`downupEF`, `downupFE`); the KLR relations on upward strands
(`klr`), with `Q_ab = u^{d_ab} + v^{d_ba}` by default. Relations whose terms involve `h(a)` hold for
every weight of the rightmost region; the calculator instantiates them per region.

## nilHecke and odd nilHecke algebras

- The nilHecke algebra `NH_n` is the one-colour KLR algebra: `ψ_k² = 0`,
  `x_k ψ_k − ψ_k x_{k+1} = 1`, `ψ_k x_k − x_{k+1} ψ_k = 1`, the plain braid relation; degrees
  `deg x = 2`, `deg ψ = −2`.
- The odd nilHecke algebra `ONH_n` (odd-lean: `Diagrams.OddNilHecke`) has both generators odd,
  with `deg x = 2`, `deg ψ = −2`, and relations `ψ_k² = 0`, `ψ_k ψ_{k+1} ψ_k = ψ_{k+1} ψ_k ψ_{k+1}`,
  `x_k ψ_k + ψ_k x_{k+1} = 1`, `ψ_k x_k + x_{k+1} ψ_k = 1`. Distant generators anticommute by the
  Koszul rule, and `x_k²` is not zero.
- **Skew-polynomial representation** (odd-lean: `AllRankDivided`, `freeAction`): `ONH_n` acts on
  skew polynomials (`x_i x_j = −x_j x_i` for `i ≠ j`), monomials written in increasing index order.
  `x_k` multiplies on the left. `s_k` is the algebra automorphism with `s_k(x_j) = −x_{s_k(j)}` for
  every `j`. `ψ_k` acts by the odd divided difference `∂_k`, determined by `∂_k(1) = 0`,
  `∂_k(x_j) = 1` for `j ∈ {k, k+1}` and 0 otherwise, and `∂_k(fg) = ∂_k(f) g + s_k(f) ∂_k(g)`.
  The action is faithful (`QuiverHecke.oddNilHeckeAction_injective`).

## Normal forms and printing

- **Normal forms.** `simplify` writes an element in the basis of the representation strategy. For
  KLR algebras (and `NH_n`) the basis is `ψ_ŵ x^a e(i)`: a fixed reduced word `ŵ` for each
  permutation, dots at the bottom, the idempotent below them. For `ONH_n` it is `x^a ψ_ŵ`: dots on
  top in increasing index order, as in the skew-polynomial representation. Because words are
  stored in canonical layering, a dot on a strand untouched by the crossings below it is printed
  lower than the basis order suggests (`psi_1 x_1 psi_2 x_1 x_3` in `NH_3` is `ψ_1 ψ_2 x_1² x_3`).
  Terms are printed shorter permutations first.
- **Printing.** A word is printed from its canonical layering, top to bottom. Generators at the same
  height are written left to right; when some of them are odd, the coefficient carries the Koszul
  sign of that order, so that every printed term parses back to exactly the same element.
- **Golden corpus.** `tests/golden/cases.txt` states terms and their normal forms.
  `lean/golden/Golden.lean` implements the representations above in Lean independently of the
  engine, checks that each term and its stated normal form act identically on every monomial of
  bounded degree at every source, and records the action tables in `tests/golden/actions.json`.
  The engine's test suite must reproduce both the normal forms and the tables.

## Rewriting

- **Rules.** Each relation `lhs = rhs` gives one rule per direction where it can be solved: the
  pattern is the word of that side with the most layers whose coefficient in `lhs − rhs` is a
  unit (±1 over ℤ), and the rule replaces it by the rest of the relation divided by that
  coefficient. Both directions are offered.
- **Locations.** Layers are numbered from 1 at the bottom of a word in canonical layering;
  strands from 1 at the left. A rectangle `{layers: [a, b], strands: [c, d]}` includes both ends.
  An occurrence of a pattern may be spread through the word: the interchange moves that bring it
  together are listed (each by the lower of the two exchanged layers) with their Koszul sign, and
  the rectangle is where the pattern sits after them.
- **Provenance.** Each result word lists, for each of its layers, the layer of the rewritten word
  it came from, or nothing for a layer the rule created.
- **Straightening** rewrites to the basis of the presentation's first representation that has
  one (KLR: `ψ_ŵ x^a`; `ONH_n`: `x^a ψ_ŵ`), working upwards one layer at a time. Dots move through
  crossings by the dot slides; crossings are brought to the canonical reduced word by braid moves,
  or to a double crossing that the quadratic relation removes. The result agrees with `simplify`.

## Lean export

`export.lean` and `diagcalc lean-chains` write a chain as a Lean theorem in the encoding the
presentation's `[lean]` table declares:

- `kl3-dg`: categorification-lean's KL III diagrams `dg RD k μ s t L` (presentations `klr`, `kl`);
- `skm-cl`: odd-lean's normal-form diagrams `cl D Sc μ s t L` of Brundan–Ellis's super Kac–Moody
  2-category (presentation `be2`).

A word becomes the list `L` of layers bottom first, each `(left, shape, right)` with the strands
beside the generator; the `[lean]` table gives the shape of each generator (`psi` on `a b` is
`.cross true a b` in `kl3-dg`, `.cross a b` in `skm-cl`). Colours become variables `ci, cj, …`
with a hypothesis `ci ≠ cj` for each pair, and the Cartan data a step uses become hypotheses with
the context's values (`C.dot`, `C.dij` in `kl3-dg`; `D.d`, `D.dn` in `skm-cl`), so one theorem
covers every datum with those values. A weight-dependent step adds the hypotheses `ip RD c μ = n`
(`D.h c μ = n`). In `skm-cl` the parity of each colour is a hypothesis `D.parity c = 0` or `1`,
and the scalars are those the calculator uses: `Sc.t a b = 1` and `Sc.s a b p q = 0`.

Each step is one library lemma whiskered by `dg_stepL_at` (`cl_stepL_at`). Interchange moves,
which bring a word to the layer order of a lemma or back to the calculator's, are `dswap`
(`cswap`); in `skm-cl` a move exchanging two odd generators costs the Koszul sign, which the
proof evaluates from the parity hypotheses. A relation used right to left is proved from its
instance left to right. Only integer coefficients are exported, and in `kl3-dg` only even
generators; anything else is refused with a reason.

## Display

The web app shows text typeset wherever it is only read, and as plain text in a fixed-width font
wherever it is entered (a click on typeset text opens it). Typesetting changes only the look:

| Text | Shown as |
|---|---|
| `x_1^2`, `q^-1`, `x_{12}` | x₁², q⁻¹, x₁₂ (subscripts and superscripts) |
| `psi_1`, `pi`, `Lambda` and other Greek names | ψ₁, π, Λ |
| `c1`, `t2` (a name ending in digits) | c₁, t₂ |
| `e(iij)`, `Q_ij(x_1, x_2)`, `Qbar_ij` | e(iij), Q_ij(x₁, x₂), Q̄_ij, with colour names in italics |
| a name of several letters | in italics; upright when it is applied, as in idempotent(a) |
| `a b` | a b side by side (the product); a thin space between names of several letters |
| `a := t`, `*`, `(x)` or `⊗`, `∘` | a ≔ t, ·, ⊗, ∘ |
| `solve … for c1, c2` | … for c₁, c₂ |
| `ring=QQ`, `ZZ[I]`, `GF(5)`, `QQ(q)` | ℚ, ℤ[I], 𝔽₅, ℚ(q) |
| `type=A2 nu=2i+j` | type A₂, ν = 2i+j |

## Drawing

TikZ export matches the drawing style of Brundan–Ellis, *Super Kac–Moody 2-categories*
(arXiv:1701.04133v2), using core TikZ and no nonstandard packages. SVG uses the same proportions.
Details are fixed with the drawing code.

In a 2-category, every strand of an oriented colour (any colour of a presentation with downward
strands) carries one arrowhead: on its first plain vertical stretch, else below a dot, else on a
crossing, else on its cup or cap. A strand that does not start at the bottom, such as a bubble,
has its colour written beside that arrowhead, and the weight of the rightmost region is written
to the right of the diagram, in the context's `mu=` notation.
