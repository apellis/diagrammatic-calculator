# Complexes over weighted KLR algebras

Bounded complexes of graded right projective modules over a weighted KLR algebra `A` or its
cyclotomic quotient `T^λ` (Webster, arXiv:1209.2463 and arXiv:1309.3796), with checks, homology,
the kernel step of a projective resolution, maps and homotopies found by linear algebra, and the
standard constructions (shifts, cones, totalization, Hom complexes, Gaussian elimination, and
Euler characteristics), and modules given by presentations (simple modules among them) with their
projective resolutions. Phases M1 to M5 of the plan below are done. The operations
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

## Chain maps

A chain map cell names its source and target complexes and lists its components:

```text
chainmap F := {"source": "C", "target": "D", "map": [{"at": 0, "matrix": [["e(i@1)"]]}]}
```

`source` and `target` are names of earlier complex cells (or complexes written out in full), so
editing `C` or `D` changes what `F` is checked against. A component that is not listed is zero.
Matrix rows are the summands of the target and columns those of the source. The cell runs
`chainmap.check` and draws a ladder: `C` on top, `D` below, the components as downward arrows.
Each entry of a component is edited in place; the status line says after every edit whether
`d F = F d` still holds, and lists the positions where it fails. When it holds, **Take the cone**
adds a complex cell `Cone_F` below, built by `complex.cone`.

## In the web app

In a weighted KLR document (context `klrw …`) three kinds of cell hold this material. Each is
one line of text, so undo, history, branches, and `.dcw` files treat it like any other cell; the
buttons on a cell only rewrite that line.

### Projectives, modules, and simple modules

- A **projective** `P(ℓ)⟨s⟩` is written as a summand, `{"object": "i@1", "shift": 2}`, inside a
  complex or a presentation. As a module it is a module cell with no relations:
  `module P := {"generators": [{"object": "i@1"}]}`.
- A **module given by a presentation** lists its generators, its relations, and the matrix `φ`
  (one row per generator, one column per relation), as in "Modules and resolutions" above:
  `module M := {"generators": [{"object": "i@1"}], "relations": [{"object": "i@1", "shift": 4}], "matrix": [["x_1^2 e(i@1)"]]}`
  is `P / x² P`. Any finitely presented module can be written this way, projective or not.
- A **simple module**, or a sum of them, is `P / rad P`: `module L := {"head": "i@1"}` asks the
  engine for the presentation (`module.head`). The head may also be a summand
  `{"object": "i@1", "shift": 2}` or a list of summands, for a sum of shifted heads.
  Editing an entry of such a cell writes the presentation out explicitly.

A module cell shows `Q₁ → Q₀ → M`, the graded dimension of `M` at each test object, and
**Start a resolution**, which adds a complex cell resolving it.

### Complexes and resolutions

`complex C := {…}` holds a complex in the JSON form above (the defaults may be left out; the
engine fills in the objects and shifts it can infer, and the details say which). The cell draws
the terms from left to right with their positions, each map on its arrow, and one status line:
whether `d² = 0` holds, and where the homology is not zero, with its graded dimension there. A
complex with `"resolves": {"module": "L"}` is a resolution of the latest module cell named `L`
before it; its status says what is left to cover. The tools under the status line are
**Cover the kernel** (`resolution.extend`, or `complex.kernel` at the far end of a complex that
resolves nothing), **Minimize** (`complex.gauss`), **Copy TikZ** (`complex.render`), and
**Copy JSON**. The Euler characteristic is shown under the status line. Cones, Hom complexes,
shifts, totalization, and the other operations of the table below are tools of the AI sidebar.

**Deform…** opens the deformation solver (`complex.deform`, see "Parameters and deformations"
below) under a one-dimensional complex, also when the complex fails its checks. Without
`truncate=h^N` in the context it says what to add. Otherwise it shows a grid of check boxes for
each differential (rows are target summands, columns source summands, numbered from 0); tick the
entries that may be corrected, or the box by a differential's name for all of it, and press
**Solve to order N**. The result says whether every order was solved, lists the correction
`h^m X_m` chosen at each order and the dimension of the freedom there (with a basis under a
disclosure), or names the order where the equations have no solution; the two differentials of that
composite get red arrows. **Add the corrected complex as a new cell** adds `C_h` below and leaves
the cell as it is; **Write into this cell** replaces its differentials by the corrected ones, as
one step of the history. A result computed before the complex last changed is labelled so and
cannot be applied.

### Chain maps

`chainmap F := {"source": "C", "target": "D", "map": […]}` is drawn as a ladder (see "Chain maps"
above). Its status says whether `d F = F d`, and **Take the cone** adds the cell `Cone_F`.

### Editing maps

Every map on a cell can be edited entry by entry: a differential, a component of a chain map, the
augmentation `ε` of a resolution, or the matrix `φ` of a presentation. Click an entry (on the
arrow, or in the matrix a larger map opens into), type its new value, and press Enter; empty or `0`
writes zero, and Escape leaves the entry as it was. The field says which summands the entry goes
between and the degree it must have.

**Nothing is thrown away.** The cell keeps every entry as written, also while the cell fails its
checks, so a map can be changed through several invalid steps on the way to a valid one:

- When a check fails (an entry of the wrong degree, `d² ≠ 0`, `ε d ≠ 0` into the relations, or
  `d F ≠ F d`), the cell says so in its status line and lists the failures. Every map a failure
  involves has a red arrow, and a failure at one entry of a map (a degree) marks that entry in red.
  A failure of `d ∘ d` or of `d F = F d` marks every map in the composite. A matrix with a marked
  entry opens by itself. The tools that need a valid cell are hidden until it checks again.
- When the engine cannot read an entry at all (a typing error), the cell shows the message with
  **cannot be checked** and still draws every map as written from the cell's text, so the entry
  can be corrected in place.
- While the engine checks the last change, the cell shows the maps as just written, and further
  edits apply to them.

Each edit is one step of the document's history, so Undo restores the entry before it.

### The map pane

Double-click a map (its arrow, its name, or one of its entries) to open it in a pane in the
cell's own row: the cell on the left and the map on the right, with the rest of the document
above and below as before. **✕ Close**, or Escape outside a text field or the canvas, closes it.
The pane shows the map's matrix, the
failures that involve it, and the chosen entry: the summands it goes between, its degree, its text
in a field, and, when it is not zero, its diagram on an embedded canvas. The canvas has the usual
gestures and keys (press `?` on it); every change made there, or with **Write** or **Set to 0**,
is written into the cell at once, checked again, and can be undone. Red marks in the pane are
those of the cell, and the first entry that a failed check involves is chosen when the pane opens.
If the cell no longer has the map (its terms changed), the pane says so.

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
| `complex.deform` | corrections `h^m X_m` of chosen entries making `d² = 0` order by order, with the freedom and the first obstruction (needs `truncate`) |

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
(this display comes from the engine as the cell's `display` field). Clicking an entry, of a
differential, of `ε`, or of a presentation matrix `φ`, edits it in place: the field names the
source and target summands and the degree the entry must have (`s − t` from `P(a)⟨s⟩` to
`P(b)⟨t⟩`), Enter rewrites the cell's text (empty or `0` writes zero), and the engine checks the
result again. Editing a `{"head": …}` module writes it out as an explicit presentation. A complex that resolves
nothing has one status line: `d² = 0` and where its homology is not zero, with the graded
dimensions there (or `not a complex` and the failed checks), and after a change of verdict the line
it had before. The cell says whether it is a resolution
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

## Parameters and deformations

A context with `truncate=h^N` (and `hdeg=d`, the internal degree of `h`, default 0) computes with
coefficients in `k[h]/(h^{N+1})`: `h` is a scalar, every product drops the terms with `h^{N+1}`
or higher, and Hom spaces are counted over `k`. The degree-`e` part of `Hom(P(a), P(b))` has the
basis `h^j b`, `0 ≤ j ≤ N`, with `b` running over a basis of the `h`-free maps of degree `e − jd`.
Every operation of this page (checks, homology, kernels, cones, Hom complexes, Gaussian
elimination, and the rest) works unchanged over this ring, and every result carries a
`truncation` record in its provenance saying which terms were dropped. A term whose summands
have different degrees once `h` is counted with degree `d` is an error, never truncated. The
truncation applies to complex, module, and chain-map cells and to the operations here; ordinary
term cells keep exact polynomials in `h`.

**Deformations.** A differential over `k[h]/(h^{N+1})` is `d = Σ_a h^a d_a` with each `d_a` free
of `h`. `complex.deform` takes a one-dimensional complex and a list of `unknowns` (references
`{"map": "differential", "from": p}`, with `row` and `col` to pick one entry, rows and columns
numbered from 0) and adds to those entries corrections `h^m X_m`, `1 ≤ m ≤ N`, so that
`d² = 0`. The coefficient of `h^m` in `d²` is `Σ_{a+b=m} d_b d_a`, and the only terms in it that
involve `X_m` are `d_0 X_m + X_m d_0`; once the lower orders are fixed, order `m` is the linear
system

```text
d_0 X_m + X_m d_0 = − Σ_{a+b=m} d_b d_a        (with d_m its given part)
```

in the `h`-free category, `X_m` of internal degree `−md` and supported on the unknown entries.
This is the order-by-order Maurer–Cartan equation behind iterated cones and deformations of
differentials. The result lists, for each order, the `correction` chosen (as `h^m X_m`) and a
basis of the `freedom` (the solutions of the homogeneous system: any other choice differs from
the correction by a combination of them; a choice at one order changes the equations at later
orders). `complex` is the corrected complex and `solved_to_order` the last order solved.

When an order has no solution, `failures` holds one failure with `condition: "obstruction"`, its
`order`, the coefficient of `h^m` in `d²` (with the unknowns zero) at the first position where
the system becomes inconsistent, and `maps` naming the two differentials of that composite, as
for `complex.check`; `complex` is then corrected through order `m − 1`. When the `h`-free part
itself does not square to zero, the failure has `condition: "d^2"` and order 0, and
`solved_to_order` is null. Nothing in the input is discarded: the given parts of the
differential stay as written and the corrections are added to them.

Corrections start at order 1, so unknowns of order 0, such as the homotopies of an iterated
cone, are found by a bookkeeping parameter: scale the off-diagonal blocks by `t^k` (block `k`
steps from the diagonal), take `truncate=t^K hdeg=0`, mark those blocks unknown, and set `t = 1`
in the answer.

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
- **P1** (done): coefficients in `k[h]/(h^{N+1})` for every complex operation (`truncate`, `hdeg`).
- **T1** (done): `complex.deform`, unknown blocks solved order by order with freedom and obstructions,
  and the **Deform…** tool on complex cells.
- Later: spectral sequences of multicomplexes, and Rickard and
  Chuang–Rouquier complexes (bounded).
