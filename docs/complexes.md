# Complexes over weighted KLR algebras

Bounded complexes of graded right projective modules over a weighted KLR algebra `A` or its
cyclotomic quotient `T^λ` (Webster, arXiv:1209.2463 and arXiv:1309.3796), with checks, homology,
and the kernel step of a projective resolution. This is phase M1 of the plan below. The operations
are ordinary calculator operations (`spec/ops/`), so the command line, the Python bindings, and the
web app all send the same JSON messages. An agent building a resolution step by step uses
`complex.kernel` and `complex.homology` in a loop (worked example below).

## Objects and maps

- An object is a loading `ℓ`, written as in a KLRW context (`i@1`, or `e(i@1)`), standing for
  `P(ℓ) = e(ℓ) A`. Isotopic loadings (the same order type) are the same object.
- `P(ℓ)⟨s⟩` has its generator in degree `s`. A *summand* is `{ "object": ℓ, "shift": s }`; a term
  of a complex is a list of summands.
- A morphism `P(a) → P(b)` is left multiplication by an element of `e(b) A e(a)`: a term whose
  bottom is `a` and whose top is `b`. Composition is stacking: `g ∘ f` puts `g` on top of `f`.
- A map `P(a)⟨s⟩ → P(b)⟨t⟩` of degree zero is an element of `e(b) A e(a)` of degree `s − t`.
- A matrix between two terms has one row per summand of the target and one column per summand of
  the source; `null` or an omitted entry is zero.

## Complexes

```json
{
  "indexing": "cohomological",
  "squares": "commute",
  "odd_signs": "koszul",
  "dimension": 1,
  "terms": [
    { "at": -1, "summands": [{ "object": "i@1", "shift": 2 }] },
    { "at": 0, "summands": [{ "object": "i@1", "shift": 0 }] }
  ],
  "differentials": [{ "from": -1, "matrix": [["x_1 e(i@1)"]] }]
}
```

- `indexing`: `cohomological` (the default; `d` goes from position `p` to `p + 1`) or
  `homological` (`d` goes from `p` to `p − 1`).
- `dimension`: the number of axes. Positions are integers for one axis and lists of integers
  otherwise; each differential names its `axis` (default 0). Multicomplexes are stored as such,
  which is the shape spectral sequences will need.
- `squares`: for two or more axes, whether the differentials of different axes `commute` or
  `anticommute`.
- `odd_signs`: the sign rule for odd morphisms, `koszul` or `ignore`. Weighted KLR algebras are
  purely even, so it has no effect yet; it is stored and reported so that the sign rule is fixed
  when odd algebras arrive.

Positions not listed are zero, and differentials not listed are zero.

## Operations

| Operation | Does |
|---|---|
| `hom.basis` | the basis of `Hom(P(a), P(b))` degree by degree (or in one `degree`) |
| `complex.check` | entries homogeneous of the right degree, `d² = 0`, and the square condition |
| `chainmap.check` | entries of degree zero, and `d F = F d` on every axis |
| `homotopy.check` | `f − g = d h + h d` for maps of one-dimensional complexes, `h` of step `−1` |
| `complex.homology` | dimensions of `H(Hom(T⟨d⟩, C))` at test objects `T`, with representatives |
| `complex.kernel` | generators of the kernel of the differential out of a position, and the extended complex |

Every result carries `certified_by` and a provenance with the certificate of the basis used.

**Homology.** Homology of a complex of projectives is computed through the functors
`Hom(T⟨d⟩, −)` for the test objects `T`: for each colour multiset of the complex, the loadings the
caller wrote and one loading of every order type. These detect exactness of complexes of
projectives over the finite-dimensional algebra `T^λ` (every simple module is a quotient of some
`P(T)`). The result lists, for each position, the nonzero pieces `(T, d, dimension)` and a
representative cycle for each, and says `zero` when every piece vanishes.

**Kernels.** `complex.kernel` at position `p` returns projectives `T⟨d⟩` and a matrix
`K : ⊕ T⟨d⟩ → C(p)` whose columns generate the kernel of `d : C(p) → C(p + step)`: degree by
degree, each new generator is chosen modulo the span of the earlier ones. When the position before
`p` is empty and the complex is one-dimensional, the result also contains `extended`, the complex
with `⊕ T⟨d⟩` put before `p` and `K` as the new differential.

**Degree windows.** In the cyclotomic quotient every Hom space is finite-dimensional and every
search is complete. Without the quotient Hom spaces are unbounded above; the search then stops at a
window (`max_degree`, or six degrees above the lowest), and the result says so in `window`
(`truncated: true`), in `complete: false`, and in a provenance note. Nothing is cut silently.

## Worked example: resolving a simple module

Take one black strand right of the red strand `2i` in type `A₁`,
`klrw type=A1 ring=QQ red="2i@0" quotient=cyclotomic`. Then `e A e = k[x]/(x²)` with `x = x_1` of
degree 2, and the simple module `L` is `P/xP`.

1. Start with the presentation `P⟨2⟩ --x--> P` (the complex above). `complex.homology` at 0 gives
   one piece in degree 0 (that is `L`), and at −1 one piece in degree 4 with representative
   `x_1 e(i@1)`: the map `x` has a kernel.
2. `complex.kernel` at −1 returns the summand `P⟨4⟩` and the matrix `[[x_1 e(i@1)]]`, complete,
   and the extended complex `P⟨4⟩ → P⟨2⟩ → P`.
3. Repeating gives `P⟨6⟩`, `P⟨8⟩`, …: the minimal projective resolution
   `⋯ → P⟨4⟩ → P⟨2⟩ → P → L`. After each step `complex.homology` at the new position is zero.

The test `a_resolution_step_by_step` in `crates/core/tests/complexes.rs` runs exactly this loop.

## Plan

- **M1** (done): the model, the checks, homology, kernels, and Hom bases.
- **M2**: lifting maps through surjections and along resolutions, homotopy search, and minimality.
- **M3**: cones, Hom complexes and Ext, totalization of multicomplexes, Gaussian elimination, and
  Euler characteristics.
- **M4**: the AI sidebar, a web view of complexes, TikZ output, and Python helpers.
- Later: Rickard and Chuang–Rouquier complexes (bounded), and modules beyond projectives.
