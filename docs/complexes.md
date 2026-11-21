# Complexes over weighted KLR algebras

Bounded complexes of graded right projective modules over a weighted KLR algebra `A` or its
cyclotomic quotient `T^λ` (Webster, arXiv:1209.2463 and arXiv:1309.3796), with checks, homology,
the kernel step of a projective resolution, maps and homotopies found by linear algebra, and the
standard constructions (shifts, cones, totalization, Hom complexes, Gaussian elimination, and
Euler characteristics). Phases M1 to M4 of the plan below are done. The operations
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

## Modules and resolutions

A module is given by a presentation `Q₁ --φ--> Q₀ → M → 0`, with `Q₀`, `Q₁` sums of projectives:

```json
{
  "generators": [{ "object": "i@1", "shift": 0 }],
  "relations": [{ "object": "i@1", "shift": 2 }],
  "matrix": [["x_1 e(i@1)"]]
}
```

`M` is the cokernel of `φ`, whose matrix has one row per generator and one column per relation
(the convention of every matrix here). A projective is a module with no relations, and
`module.head` writes down the presentation of `P/rad P` for a projective `P`, so simple modules
and their sums need not be presented by hand. In a module cell the same is written
`{"head": "i@1"}` (the objects of `P`, as in a context line). The module stays fixed while a resolution of it is
built, so `C₀` may change during the search without changing what is resolved.

A **resolution** of `M` is a one-dimensional complex `⋯ → C₋₁ → C₀` ending at position 0 (in
either indexing) with

```json
"resolves": { "module": { … }, "augmentation": [[ … ]] }
```

where the augmentation `ε : C₀ → Q₀` (rows the generators, columns the summands of `C₀`) induces
`C₀ → M`. It may be left out when `C₀` is the sum of the generators in order; it is then the
identity. `resolution.check` checks that `ε d` lands in the relations, that `C₀ → M` is onto,
and that the complex is exact everywhere except at its far end, where it reports the part of the
kernel still to be covered (`uncovered`, `next_at`). `resolved: true` means there is nothing
left to cover, and then `minimal` says whether the resolution is minimal. Exactness and
surjectivity are tested at the test objects, in the degree windows below.

- `resolution.start` makes the one-term resolution `C₀ = Q₀` with `ε` the identity.
- `resolution.extend` adds the next term: generators of the kernel at the far end (at `C₀`, the
  kernel of `C₀ → M`, which is computed from `ε` and the relations). It refuses a resolution that
  does not check. Each call adds one term; nothing is built automatically.
- `complex.gauss` on a resolution keeps the augmentation in step: the result is again a
  resolution of the same module.

## Operations

| Operation | Does |
|---|---|
| `hom.basis` | the basis of `Hom(P(a), P(b))` degree by degree (or in one `degree`) |
| `complex.check` | entries homogeneous of the right degree, `d² = 0`, and the square condition |
| `chainmap.check` | entries of degree zero, and `d F = F d` on every axis |
| `homotopy.check` | `f − g = d h + h d` for maps of one-dimensional complexes, `h` of step `−1` |
| `complex.homology` | dimensions of `H(Hom(T⟨d⟩, C))` at test objects `T`, with representatives |
| `complex.kernel` | generators of the kernel of the differential out of a position, and the extended complex |
| `map.lift` | `h` with `g h = f`, for `f : X → Z` and `g : Y → Z` (lifting through `g`) |
| `map.extend` | `h` with `h g = f`, for `f : X → Z` and `g : X → Y` (extending along `g`) |
| `chainmap.solve` | a chain map `C → D` extending given components, and the dimension of the choices |
| `homotopy.find` | a homotopy `h` with `f − g = d h + h d`, or a proof that none exists |
| `complex.minimal` | whether every differential lies in the radical, with a witness when one does not |
| `complex.shift` | `C[k]⟨s⟩` |
| `complex.cone` | the cone of a chain map |
| `complex.totalize` | the total complex of a multicomplex, with the blocks each position contributes |
| `complex.hom` | the cohomology of `Hom(C, D)` in each internal degree (chain maps up to homotopy at `n = 0`) |
| `complex.gauss` | Gaussian elimination of isomorphism entries: a smaller homotopy-equivalent complex |
| `complex.euler` | the Euler characteristic `Σ (−1)^p q^s [P(ℓ)]` in the Grothendieck group |
| `complex.render` | TikZ (`tikz-cd`) source for a one-dimensional complex |
| `module.check` | the presentation is well formed, and the graded dimensions of the module |
| `module.head` | the presentation of `P/rad P` for a projective `P` (a sum of simple modules) |
| `resolution.start` | the one-term resolution `Q₀ → M` of a module |
| `resolution.extend` | the next term of a resolution, covering the kernel at its far end |
| `resolution.check` | whether a complex with `resolves` is a resolution, and what is left to cover |

Every result carries `certified_by` and a provenance with the certificate of the basis used, and
a note saying which linear system was solved or which construction was applied.

**Linear problems.** Every problem of M2 is a finite linear system in degree-zero Hom spaces,
which are finite-dimensional in the cyclotomic quotient. `exists` is decisive. `freedom` is the
dimension of the solutions of the homogeneous system: 0 means the answer is unique. With
`basis: true`, `chainmap.solve` also returns a basis of those homogeneous solutions.
`complex.minimal` tests whether the left ideal `{g d}` of `End(C(p))` spanned by the maps `g` back
is nilpotent; when it is not, a failure names the differential and, when one is found among the
tried maps, a `witness` `g` with `g d` not nilpotent.

**Constructions.** The sign conventions of shifts, cones, totalization, and Hom complexes are in
`docs/conventions.md` (Complexes). `complex.gauss` repeatedly removes an entry `φ` of a
differential that is an isomorphism, replacing the rest of that differential's neighbourhood by
`ε − γ φ⁻¹ δ`; it lists each elimination, stops after `limit` steps (default 1000), and says
`finished: false` if it stopped early. `complex.hom` works in the window of internal degrees where
some map can be nonzero (`degree` asks for one).

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

With the module itself: `module.head` with `object: "i@1"` gives the presentation of `L` above,
`resolution.start` gives `P → L`, and each `resolution.extend` adds `P⟨2n⟩` at position `−n` with
`check.next_at` moving to `−n − 1` (the test `resolving_the_simple_module`; the workspace
`examples/klrw-a1-resolution.dcw` holds the first step as cells).

In the web app the same steps are buttons on the cells. A module cell `module L := {"head": "i@1"}`
shows its presentation `Q₁ → Q₀ → L` and its graded dimension, and **Start a resolution** adds a
complex cell `R_L` resolving `L` below it. That cell draws the terms from left to right with
their positions and `ε` into `L`. Each projective reads as its strands, `P(2i₀ i₁)⟨2⟩` for
`P(i@1)⟨2⟩` over `red=2i@0`: labels with their positions as subscripts, red strands in red. A
map between single summands is drawn on its arrow as the diagram of its element (at most four
terms, then "+ n more"); a larger matrix shows its size and number of nonzero entries and opens,
on a click, into a table with the source summands as columns and the target summands as rows
(this display comes from the engine as the cell's `display` field). The cell says whether it is a resolution
yet and what is left to cover, and offers **Cover the kernel** (one `resolution.extend`),
**Minimize** (`complex.gauss`), **Copy TikZ**, and **Copy JSON**. Each button rewrites the cell's
text, so undo, history, and branches work as for any edit, and the cell keeps naming `L`, so
editing the module cell changes what is resolved. On a complex that resolves nothing,
**Cover the kernel** becomes `complex.kernel` at the far end.

Continuing with maps and constructions:

4. `chainmap.solve` from the two-term complex to itself, with the identity fixed at 0, finds the
   identity at −1 with `freedom: 0`: a map of modules lifts uniquely to the resolution here.
5. `homotopy.find` for the identity says it is not null-homotopic, while `complex.cone` of the
   identity gives a complex on which `homotopy.find` finds a contracting homotopy, and
   `complex.gauss` reduces it to the zero complex in two eliminations.
6. `complex.hom` from `P⟨2⟩ → P` to `P` gives `H^0` in internal degree 2 and `H^1` in degree −2;
   `complex.euler` of `P⟨2⟩ → P` is `(-q^2 + 1)[i@1]`, the class `[L] = [P] − q²[P]` of the
   simple module in the Grothendieck group.

The tests `solving_for_chain_maps_and_homotopies`, `the_cone_of_the_identity_is_contractible`,
`shifts_totalization_and_euler_characteristic`, and `hom_complex_cohomology` run these steps.

## Plan

- **M1** (done): the model, the checks, homology, kernels, and Hom bases.
- **M2** (done): lifting and extending maps, solving for chain maps and homotopies, and minimality.
- **M3** (done): shifts, cones, Hom complexes (whose `H^n` in internal degree 0 of a resolution
  against a module complex computes Ext), totalization of multicomplexes, Gaussian elimination, and
  Euler characteristics.
- **M4** (done): every complex operation is a tool of the AI sidebar, Python helpers
  (`docs/python.md`), and TikZ output (`complex.render`).
- **M5** (done): modules by presentations, `P/rad P`, and resolutions of a module built step by
  step, with module and complex cells in workspaces, their web views, and the step tools on them.
- Later: spectral sequences of multicomplexes, Rickard and
  Chuang–Rouquier complexes (bounded), and modules beyond projectives.
