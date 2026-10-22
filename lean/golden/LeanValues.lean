/-
The Lean regression values: graded dimensions and ranks stated or proved in the Lean libraries,
instantiated at small cases and written to `tests/lean-values/values.json`. The engine must
reproduce every value (`crates/core/tests/lean_values.rs`).

Like `Golden.lean`, this file uses Lean core only and never links against the libraries: it
evaluates the right-hand sides of the cited theorems, which are finite combinatorial sums, at
concrete data. Every row names the theorem it instantiates. The libraries' algebras, `gdim` and
`finrank` are noncomputable, so the left-hand sides cannot be evaluated directly.

Sources (commits as surveyed):
* categorification-lean `453a6fe`:
  - `KL2.gdim_cornerGrade2` (`KLR/KL2/Prop34KL2.lean`): for any Cartan datum,
    `gdim e(j) R(ν) e(i) = Σ_{w : w • i = j} q^{-Σ_{(a,b) ∈ inv w} i_a · i_b} Π_a (1 − q^{i_a · i_a})⁻¹`,
    with `(w • i)_{w a} = i_a` and `deg x = i · i`, `deg ψ = −i · j`;
  - `KL1.gdim_cornerGrade` (`KLR/GradedBasis.lean`): the same for nilHecke `NH_n = R(n·i)`;
  - `GradingDatum.gdim_center` (`KLR/SymGdim.lean`):
    `gdim Z(R(ν)) = Π_c Π_{a < ν_c} (1 − q^{(a+1) deg x_c})⁻¹`;
  - `CycKLR.finrank_corner` (`KLR/CyclotomicMultFree.lean`): for pairwise distinct, non-adjacent
    labels, every corner of the cyclotomic quotient has rank `Π_p ⟨h_{i_p}, Λ⟩`.
* odd-lean `79ac2f8`:
  - `NilHeckeGrading.degree_finrank_binomial` (`OddMath/Frontier/NilHeckeGrading.lean`):
    `rank ONH_a in degree d = Σ_{w ∈ S_a, 0 ≤ d + 2ℓ(w) even} C((d + 2ℓ(w))/2 + a − 1, a − 1)`;
  - `Cyclotomic.prop_5_2` (`Frontier/CyclotomicMatrix.lean`) with `finrank_OH`
    (`Frontier/OddGrassmannSchur.lean`): `rank ONH_a / ⟨x_1^N⟩ = (a!)² C(N, a)`.

Calculator conventions (`docs/conventions.md`): Bourbaki numbering, so in `B2` the first root is
long (`(i,i) = 4`) and in `G2` the first root is short (`(i,i) = 2`, `(j,j) = 6`). The libraries'
`NonVacuity.cB2` has vertex 0 short, so its labels are swapped here.

Core Lean only: `lean --run lean/golden/LeanValues.lean [values.json]`.
-/

/-! ## Combinatorics -/

/-- All permutations of `0, …, n−1`, as lists `w` with `w[a]` the image of `a`. -/
partial def perms : Nat → List (List Nat)
  | 0 => [[]]
  | n + 1 =>
    -- insert the new largest value n at every position of each permutation of `0, …, n−1`
    (perms n).flatMap fun p => (List.range (n + 1)).map fun k => p.take k ++ [n] ++ p.drop k

/-- `(w • i)_{w a} = i_a`. -/
def act (w : List Nat) (i : List Nat) : List Nat :=
  (List.range i.length).map fun b =>
    match (List.range w.length).find? (fun a => w.getD a 0 == b) with
    | some a => i.getD a 0
    | none => 0

/-- Inversions `(a, b)`, `a < b`, `w a > w b`. -/
def inversions (w : List Nat) : List (Nat × Nat) :=
  (List.range w.length).flatMap fun a =>
    ((List.range w.length).filter fun b => a < b && w.getD a 0 > w.getD b 0).map fun b => (a, b)

def dotOf (form : List (List Int)) (a b : Nat) : Int := (form.getD a []).getD b 0

/-- Number of `e ∈ ℕ^m` with `Σ e_k d_k = t` (all `d_k > 0`). -/
partial def ways : List Nat → Int → Nat
  | [], t => if t == 0 then 1 else 0
  | d :: rest, t =>
    if t < 0 then 0
    else (List.range (t.toNat / d + 1)).foldl (fun acc e => acc + ways rest (t - (e * d : Nat))) 0

def binom : Nat → Nat → Nat
  | _, 0 => 1
  | 0, _ + 1 => 0
  | n + 1, k + 1 => binom n k + binom n (k + 1)

def factorial : Nat → Nat
  | 0 => 1
  | n + 1 => (n + 1) * factorial n

/-- Distinct sequences with the given multiplicities, in lexicographic order. -/
partial def sequences (mult : List Nat) : List (List Nat) :=
  if mult.all (· == 0) then [[]]
  else
    (List.range mult.length).foldl (fun (acc : List (List Nat)) c =>
      if mult.getD c 0 == 0 then acc
      else acc ++ (sequences (mult.set c (mult.getD c 0 - 1))).map (c :: ·)) []

/-! ## The values -/

/-- `KL2.gdim_cornerGrade2` in degree `d`; `tgt = none` sums over every target. -/
def cornerDim (form : List (List Int)) (i : List Nat) (tgt : Option (List Nat)) (d : Int) : Nat :=
  let dens := i.map fun c => (dotOf form c c).toNat
  (perms i.length).foldl (fun acc w =>
    let j := act w i
    if tgt.all (· == j) then
      let deg : Int := -((inversions w).foldl (fun s p => s + dotOf form (i.getD p.1 0) (i.getD p.2 0)) 0)
      acc + ways dens (d - deg)
    else acc) 0

/-- `NilHeckeGrading.degree_finrank_binomial`: `ONH_a` in degree `d`. -/
def onhDim (a : Nat) (d : Int) : Nat :=
  (perms a).foldl (fun acc w =>
    let t : Int := d + 2 * (inversions w).length
    if 0 ≤ t && t % 2 == 0 then acc + binom (t.toNat / 2 + a - 1) (a - 1) else acc) 0

/-- `GradingDatum.gdim_center`: the centre of `R(ν)` in degree `d`. -/
def centreDim (form : List (List Int)) (mult : List Nat) (d : Int) : Nat :=
  let dens := (List.range mult.length).flatMap fun c =>
    (List.range (mult.getD c 0)).map fun a => (a + 1) * (dotOf form c c).toNat
  ways dens d

/-! ## Rows -/

def formOf : String → List (List Int)
  | "A1" => [[2]]
  | "A2" => [[2, -1], [-1, 2]]
  | "A3" => [[2, -1, 0], [-1, 2, -1], [0, -1, 2]]
  | "B2" => [[4, -2], [-2, 2]]
  | "G2" => [[2, -3], [-3, 6]]
  | _ => []

def colourName (c : Nat) : String := if c == 0 then "i" else if c == 1 then "j" else "k"

def seqName (s : List Nat) : String := String.join (s.map colourName)

def multOf (s : List Nat) (rank : Nat) : List Nat :=
  (List.range rank).map fun c => (s.filter (· == c)).length

def nuName (mult : List Nat) : String :=
  "+".intercalate ((List.range mult.length).filterMap fun c =>
    match mult.getD c 0 with
    | 0 => none
    | 1 => some (colourName c)
    | m => some (s!"{m}{colourName c}"))

structure Source where
  repo : String
  commit : String
  file : String
  decl : String

def catLean (file decl : String) : Source :=
  { repo := "apellis/categorification-lean", commit := "453a6fe", file, decl }

def oddLean (file decl : String) : Source :=
  { repo := "apellis/odd-lean", commit := "79ac2f8", file, decl }

def sourceJson (s : Source) : String :=
  s!"\{\"repo\": \"{s.repo}\", \"commit\": \"{s.commit}\", \"file\": \"{s.file}\", \"declaration\": \"{s.decl}\"}"

def dimsJson (lo hi : Int) (f : Int → Nat) : String :=
  let ds := (List.range (hi - lo + 1).toNat).map fun (k : Nat) => lo + (k : Int)
  "{" ++ ", ".intercalate (ds.map fun d => s!"\"{d}\": {f d}") ++ "}"

/-- One graded-dimension row. -/
def row (id kind context src : String) (tgt : Option String) (s : Source) (lo hi : Int)
    (f : Int → Nat) : String :=
  let t := match tgt with
    | some x => s!"\"{x}\""
    | none => "null"
  s!"  \{\"id\": \"{id}\", \"kind\": \"{kind}\", \"context\": \"{context}\", \"source_object\": \"{src}\", \"target_object\": {t}, \"source\": {sourceJson s}, \"dims\": {dimsJson lo hi f}}"

def rankRow (id context : String) (src tgt : Option String) (s : Source) (rank : Nat) : String :=
  let q (o : Option String) := match o with
    | some x => s!"\"{x}\""
    | none => "null"
  s!"  \{\"id\": \"{id}\", \"kind\": \"rank\", \"context\": \"{context}\", \"source_object\": {q src}, \"target_object\": {q tgt}, \"source\": {sourceJson s}, \"rank\": {rank}}"

/-- Every corner `e(j) R(ν) e(i)` with `i` the given source, plus the whole `R(ν) e(i)`. -/
def cornerRows (ty : String) (mult : List Nat) (span : Nat) : List String :=
  let form := formOf ty
  let ctx := s!"klr type={ty} nu={nuName mult}"
  let src := sequences mult
  let lo : Int := -((List.range mult.length).foldl (fun s c =>
    s + (mult.getD c 0 * (mult.getD c 0 - 1) : Nat) * (dotOf form c c).toNat / 2) 0 : Nat)
  let decl := catLean "KLR/KL2/Prop34KL2.lean" "KL2.gdim_cornerGrade2"
  src.flatMap fun i =>
    (src.map fun j =>
      row s!"{ty}-{nuName mult}-{seqName j}-{seqName i}" "corner" ctx (seqName i) (some (seqName j))
        decl lo (lo + span) (cornerDim form i (some j))) ++
    [row s!"{ty}-{nuName mult}-all-{seqName i}" "corner" ctx (seqName i) none decl lo (lo + span)
        (cornerDim form i none)]

def rows : List String :=
  -- nilHecke NH_n = R(n·i) (KL I) and odd nilHecke ONH_n (odd-lean)
  ((List.range 3).flatMap fun k =>
    let n := k + 2
    let lo : Int := -((n * (n - 1) : Nat) : Int)
    [ row s!"nh{n}" "algebra" s!"nilhecke nu={n}" (seqName (List.replicate n 0)) none
        (catLean "KLR/GradedBasis.lean" "KL1.gdim_cornerGrade") lo (lo + 12)
        (cornerDim (formOf "A1") (List.replicate n 0) none),
      row s!"onh{n}" "algebra" s!"odd-nilhecke nu={n}" (seqName (List.replicate n 0)) none
        (oddLean "OddMath/Frontier/NilHeckeGrading.lean" "NilHeckeGrading.degree_finrank_binomial")
        lo (lo + 12) (onhDim n) ]) ++
  -- KLR corners (KL II), Bourbaki labels
  cornerRows "A2" [1, 1] 6 ++ cornerRows "A2" [2, 1] 6 ++
  cornerRows "B2" [1, 1] 8 ++ cornerRows "B2" [2, 1] 8 ++ cornerRows "B2" [1, 2] 8 ++
  cornerRows "G2" [1, 1] 8 ++ cornerRows "G2" [2, 1] 8 ++
  cornerRows "A3" [1, 1, 1] 4 ++ cornerRows "A3" [1, 2, 1] 4 ++
  -- centres
  ([("nilhecke nu=2", "A1", [2]), ("nilhecke nu=3", "A1", [3]),
    ("klr type=A2 nu=2i+j", "A2", [2, 1]), ("klr type=B2 nu=2i+j", "B2", [2, 1]),
    ("klr type=B2 nu=i+2j", "B2", [1, 2]), ("klr type=G2 nu=2i+j", "G2", [2, 1])].map
    fun (ctx, ty, mult) =>
      row s!"centre-{ctx}" "centre" ctx "" none
        (catLean "KLR/SymGdim.lean" "GradingDatum.gdim_center") 0 10 (centreDim (formOf ty) mult)) ++
  -- cyclotomic quotients
  ([(2, 2), (2, 3), (2, 4), (3, 3), (3, 4)].map fun (a, N) =>
    rankRow s!"onh{a}-cyclotomic-{N}" s!"odd-nilhecke nu={a} Lambda={N}" none none
      (oddLean "OddMath/Frontier/CyclotomicMatrix.lean"
        "Cyclotomic.prop_5_2 with OddGrassmannSchur.finrank_OH")
      (factorial a * factorial a * binom N a)) ++
  ((sequences [1, 0, 1]).flatMap fun i => (sequences [1, 0, 1]).map fun j =>
    rankRow s!"A3-i+k-cyclotomic-{seqName j}-{seqName i}" "klr type=A3 nu=i+k Lambda=2i+3k"
      (some (seqName i)) (some (seqName j))
      (catLean "KLR/CyclotomicMultFree.lean" "CycKLR.finrank_corner") (2 * 3))

def main (args : List String) : IO UInt32 := do
  let output := args.getD 0 "tests/lean-values/values.json"
  let rs := rows
  IO.FS.writeFile (System.FilePath.mk output)
    ("{\"format\": \"diagcalc-lean-values/1\", \"rows\": [\n" ++ ",\n".intercalate rs ++ "\n]}\n")
  IO.println s!"{rs.length} rows written to {output}"
  return 0
