/-
The golden corpus, computed in Lean independently of the Rust engine.

Reads `tests/golden/cases.txt` and, for each case, computes the action of its term and of its
stated normal form on every monomial `x^a` (each `a_k < n`) at every source idempotent, by
composing the defining operators of the generators one at a time. It checks that the two
actions agree and writes them to `tests/golden/actions.json`.

The operators are those of the Lean libraries:

* nilHecke and KLR (categorification-lean, `KLR.PolyRep.opX`, `opΨ`, `crossComp`, with
  `KL2.klP2` for the orientation `a → b` when `a > b`): `x_k` multiplies by `x_k`; `ψ_k` is the
  divided difference `(f - s_k f) / (x_k - x_{k+1})` when the colours at `k, k+1` agree, and
  `f ↦ P_{ab}(x_k, x_{k+1}) · s_k f` from component `ab` to component `ba` otherwise, with
  `P_ab(u, v) = u^{d_ba} + v^{d_ab}` when `a > b` and `a · b ≠ 0`, and `P_ab = 1` otherwise.
* odd nilHecke (odd-lean, `QuiverHecke.oddNilHeckeAction`, `Frontier.AllRankDivided.divided`):
  the `x_k` anticommute (monomials `x_1^{a_1} ⋯ x_n^{a_n}`, with sign `(-1)^{Σ_{j<i} a_i b_j}`
  for a product, as `OddMath.skewSign`); `x_k` multiplies on the left; `ψ_k` is the odd divided
  difference with `∂(1) = 0`, `∂(x_j) = [j ∈ {k, k+1}]`, `∂(fg) = ∂(f) g + s_k(f) ∂(g)` and
  `s_k(x_j) = -x_{s_k(j)}`.

Terms are read in the calculator's text format, restricted to expanded sums: `a b` is `a ∘ b`
(`b` applied first), subscripts are positions counted from 1, `e(...)` is at the bottom.

Core Lean only: `lean --run lean/golden/Golden.lean [cases.txt] [actions.json]`.
-/

abbrev Mono := List Nat
abbrev Poly := List (Mono × Int)

/-! ## Polynomials: terms sorted by exponent vector, largest first, no zero coefficients -/

def monoLt : Mono → Mono → Bool
  | a :: as, b :: bs => if a < b then true else if b < a then false else monoLt as bs
  | _, _ => false

def insertTerm (m : Mono) (c : Int) : Poly → Poly
  | [] => if c == 0 then [] else [(m, c)]
  | (m', c') :: rest =>
    if m == m' then
      (if c + c' == 0 then rest else (m, c + c') :: rest)
    else if monoLt m' m then
      (if c == 0 then (m', c') :: rest else (m, c) :: (m', c') :: rest)
    else (m', c') :: insertTerm m c rest

def polyAdd (p q : Poly) : Poly :=
  q.foldl (fun (acc : Poly) (t : Mono × Int) => insertTerm t.1 t.2 acc) p

def polyNormalize (p : Poly) : Poly := polyAdd [] p

def polyScale (c : Int) (p : Poly) : Poly :=
  polyNormalize (p.map fun (t : Mono × Int) => (t.1, c * t.2))

/-- `Σ_i Σ_{j<i} a_i b_j`: the interchanges needed to normal-order `x^a x^b`. -/
def crossCount (a b : Mono) : Nat := Id.run do
  let mut s := 0
  for i in [0:a.length] do
    for j in [0:i] do
      s := s + a.getD i 0 * b.getD j 0
  return s

def polyMul (odd : Bool) (p q : Poly) : Poly := Id.run do
  let mut out : Poly := []
  for (a, c) in p do
    for (b, d) in q do
      let sign : Int := if odd && crossCount a b % 2 == 1 then -1 else 1
      out := insertTerm (List.zipWith (· + ·) a b) (sign * c * d) out
  return out

def unitMono (n : Nat) : Mono := List.replicate n 0

def varMono (n k : Nat) : Mono := (unitMono n).set k 1

def swapAt {α : Type} [Inhabited α] (l : List α) (k : Nat) : List α :=
  (l.set k (l.getD (k + 1) default)).set (k + 1) (l.getD k default)

/-- `s_k`, with the odd sign `(-1)^{|a| + a_k a_{k+1}}` on `x^a` in the odd case. -/
def polyS (odd : Bool) (k : Nat) (p : Poly) : Poly :=
  polyNormalize <| p.map fun (t : Mono × Int) =>
    let m := t.1
    let deg := m.foldl (· + ·) 0
    let e := deg + m.getD k 0 * m.getD (k + 1) 0
    (swapAt m k, if odd && e % 2 == 1 then -t.2 else t.2)

/-- The even divided difference on one term. -/
def evenDivTerm (k : Nat) (m : Mono) (c : Int) : Poly :=
  let p := m.getD k 0
  let q := m.getD (k + 1) 0
  if p == q then []
  else if q < p then
    (List.range (p - q)).foldl
      (fun (acc : Poly) j => insertTerm ((m.set k (q + j)).set (k + 1) (p - 1 - j)) c acc) []
  else
    (List.range (q - p)).foldl
      (fun (acc : Poly) j => insertTerm ((m.set k (p + j)).set (k + 1) (q - 1 - j)) (-c) acc) []

/-- The odd divided difference of a monomial, peeling off its first variable:
`∂(x_j r) = [j ∈ {k, k+1}] r - x_{s_k(j)} ∂(r)`. -/
partial def oddDivMono (n k : Nat) (m : Mono) : Poly :=
  match (List.range n).find? (fun j => m.getD j 0 > 0) with
  | none => []
  | some j =>
    let rest := m.set j (m.getD j 0 - 1)
    let head : Poly := if j == k || j == k + 1 then [(rest, 1)] else []
    let sj := if j == k then k + 1 else if j == k + 1 then k else j
    let tail := polyMul true [(varMono n sj, 1)] (oddDivMono n k rest)
    polyAdd head (polyScale (-1) tail)

def polyDiv (odd : Bool) (n k : Nat) (p : Poly) : Poly :=
  p.foldl (fun (acc : Poly) (t : Mono × Int) =>
    if odd then polyAdd acc (polyScale t.2 (oddDivMono n k t.1))
    else polyAdd acc (evenDivTerm k t.1 t.2)) []

/-! ## Strings, as character lists (independent of the `String.Slice` API) -/

def sdrop (s : String) (n : Nat) : String := String.ofList (s.toList.drop n)
def sdropRight (s : String) (n : Nat) : String := String.ofList (s.toList.take (s.toList.length - n))
def strim (s : String) : String :=
  String.ofList ((s.toList.dropWhile Char.isWhitespace).reverse.dropWhile Char.isWhitespace).reverse

/-! ## Contexts -/

/-- Cartan matrices, Bourbaki numbering: `a_ij = 2 (α_i, α_j) / (α_i, α_i)`; in `B2` the first
simple root is long. -/
def cartanOf : String → List (List Int)
  | "A2" => [[2, -1], [-1, 2]]
  | "B2" => [[2, -1], [-2, 2]]
  | _ => [[2]]

structure Ctx where
  odd : Bool
  n : Nat
  cartan : List (List Int)
  sources : List (List Nat)

def colourName (c : Nat) : String := if c == 0 then "i" else if c == 1 then "j" else "k"

def colourIndex (s : String) : Nat := if s == "i" then 0 else if s == "j" then 1 else 2

/-- Distinct sequences with the given multiplicities, in lexicographic order. -/
partial def sequences (mult : List Nat) : List (List Nat) :=
  if mult.all (· == 0) then [[]]
  else
    (List.range mult.length).foldl (fun (acc : List (List Nat)) c =>
      if mult.getD c 0 == 0 then acc
      else acc ++ (sequences (mult.set c (mult.getD c 0 - 1))).map (c :: ·)) []

def parseWeight (w : String) : List Nat := Id.run do
  let mut mult : List Nat := [0, 0, 0]
  for part in w.splitOn "+" do
    let digits := String.ofList (part.toList.takeWhile Char.isDigit)
    let name := sdrop part digits.length
    let k := if digits.isEmpty then 1 else digits.toNat!
    let c := colourIndex name
    mult := mult.set c (mult.getD c 0 + k)
  return mult

def parseContext (line : String) : Ctx := Id.run do
  let words := (line.splitOn " ").filter (· ≠ "")
  let pres := words.getD 0 ""
  let mut ty := ""
  let mut nu := ""
  for w in words.drop 1 do
    match w.splitOn "=" with
    | [k, v] => if k == "type" then ty := v else if k == "nu" then nu := v
    | _ => pure ()
  if pres == "klr" then
    let mult := parseWeight nu
    let n := mult.foldl (· + ·) 0
    return { odd := false, n, cartan := cartanOf ty, sources := sequences mult }
  else
    let n := nu.toNat!
    return { odd := pres == "odd-nilhecke", n, cartan := [[2]], sources := [List.replicate n 0] }

/-! ## Terms -/

inductive Gen where
  | x (k : Nat)
  | psi (k : Nat)

structure Summand where
  coef : Int
  gens : List Gen   -- top first
  idem : Option (List Nat)

def parseTerm (s : String) : List Summand := Id.run do
  let toks := (s.splitOn " ").filter (· ≠ "")
  let mut out : List Summand := []
  let mut sign : Int := 1
  let mut coef : Int := 1
  let mut gens : List Gen := []
  let mut idem : Option (List Nat) := none
  let mut started := false
  let mut first := true
  for t0 in toks do
    let mut t := t0
    if t == "+" || t == "-" then
      if started then out := out ++ [{ coef := sign * coef, gens, idem }]
      sign := if t == "-" then -1 else 1
      coef := 1; gens := []; idem := none; started := false
      continue
    if first && t.startsWith "-" && t.length > 1 then
      sign := -1
      t := sdrop t 1
    first := false
    started := true
    if t.all Char.isDigit then
      coef := coef * (t.toNat! : Int)
    else if t.startsWith "e(" then
      idem := some (((sdropRight (sdrop t 2) 1).toList).map fun ch => colourIndex ch.toString)
    else
      match t.splitOn "_" with
      | [name, rest] =>
        let (pos, pow) := match rest.splitOn "^" with
          | [p, e] => (p.toNat!, e.toNat!)
          | _ => (rest.toNat!, 1)
        let g := if name == "x" then Gen.x (pos - 1) else Gen.psi (pos - 1)
        gens := gens ++ List.replicate pow g
      | _ => pure ()
  if started then out := out ++ [{ coef := sign * coef, gens, idem }]
  return out

/-! ## The action -/

abbrev Vec := List (List Nat × Poly)

def vecInsert (s : List Nat) (f : Poly) : Vec → Vec
  | [] => if f.isEmpty then [] else [(s, f)]
  | (s', g) :: rest =>
    if s == s' then
      (let h := polyAdd g f; if h.isEmpty then rest else (s, h) :: rest)
    else if monoLt s s' then
      (if f.isEmpty then (s', g) :: rest else (s, f) :: (s', g) :: rest)
    else (s', g) :: vecInsert s f rest

def vecAdd (v w : Vec) : Vec :=
  w.foldl (fun (acc : Vec) (t : List Nat × Poly) => vecInsert t.1 t.2 acc) v

/-- `P_ab(x_k, x_{k+1})`. -/
def crossP (ctx : Ctx) (k a b : Nat) : Poly :=
  let aab := (ctx.cartan.getD a []).getD b 0
  let aba := (ctx.cartan.getD b []).getD a 0
  if aab == 0 || a < b then [(unitMono ctx.n, 1)]
  else
    polyNormalize [((unitMono ctx.n).set k (-aba).toNat, 1),
      ((unitMono ctx.n).set (k + 1) (-aab).toNat, 1)]

def applyGen (ctx : Ctx) (g : Gen) (v : Vec) : Vec :=
  v.foldl (fun (acc : Vec) (t : List Nat × Poly) =>
    let (s, f) := t
    match g with
    | Gen.x k => vecInsert s (polyMul ctx.odd [(varMono ctx.n k, 1)] f) acc
    | Gen.psi k =>
      let a := s.getD k 0
      let b := s.getD (k + 1) 0
      if a == b then vecInsert s (polyDiv ctx.odd ctx.n k f) acc
      else vecInsert (swapAt s k) (polyMul false (crossP ctx k a b) (polyS false k f)) acc) []

def actSummand (ctx : Ctx) (src : List Nat) (f : Poly) (sm : Summand) : Vec :=
  let go : Unit → Vec := fun _ =>
    sm.gens.reverse.foldl (fun v g => applyGen ctx g v) (vecInsert src (polyScale sm.coef f) [])
  match sm.idem with
  | some s => if s != src then [] else go ()
  | none => go ()

def actTerm (ctx : Ctx) (src : List Nat) (f : Poly) (t : List Summand) : Vec :=
  t.foldl (fun (acc : Vec) sm => vecAdd acc (actSummand ctx src f sm)) []

/-- Every exponent vector with entries below `n`. -/
def inputs (n : Nat) : List Mono :=
  (List.range n).foldl (fun (acc : List Mono) _ =>
    acc.foldr (fun m out => (List.range n).map (fun e => m ++ [e]) ++ out) []) [[]]

/-! ## Output -/

def jsonList (xs : List String) : String := "[" ++ ", ".intercalate xs ++ "]"

def jsonMono (m : Mono) : String := jsonList (m.map toString)

def jsonPoly (p : Poly) : String := jsonList (p.map fun (t : Mono × Int) => s!"[{jsonMono t.1}, {t.2}]")

def seqName (s : List Nat) : String := String.join (s.map colourName)

def jsonVec (v : Vec) : String :=
  jsonList (v.map fun (t : List Nat × Poly) =>
    s!"\{\"target\": \"{seqName t.1}\", \"poly\": {jsonPoly t.2}}")

structure Case where
  id : String
  context : String
  term : String
  nf : String

def readCases (lines : Array String) : List Case := Id.run do
  let mut out : List Case := []
  let mut cur : Case := { id := "", context := "", term := "", nf := "" }
  for l in lines do
    let l := strim l
    if l.startsWith "case " then
      if cur.id != "" then out := out ++ [cur]
      cur := { id := sdrop l 5, context := "", term := "", nf := "" }
    else if l.startsWith "context " then cur := { cur with context := sdrop l 8 }
    else if l.startsWith "term " then cur := { cur with term := sdrop l 5 }
    else if l.startsWith "nf " then cur := { cur with nf := sdrop l 3 }
  if cur.id != "" then out := out ++ [cur]
  return out

def main (args : List String) : IO UInt32 := do
  let input := args.getD 0 "tests/golden/cases.txt"
  let output := args.getD 1 "tests/golden/actions.json"
  let cases := readCases (← IO.FS.lines (System.FilePath.mk input))
  let mut failed := false
  let mut entries : List String := []
  for c in cases do
    let ctx := parseContext c.context
    let term := parseTerm c.term
    let nf := parseTerm c.nf
    let mut agree := true
    let mut acts : List String := []
    for src in ctx.sources do
      for a in inputs ctx.n do
        let f : Poly := [(a, 1)]
        let v := actTerm ctx src f term
        let w := actTerm ctx src f nf
        if v != w then
          agree := false
        acts := acts ++ [s!"      \{\"source\": \"{seqName src}\", \"input\": {jsonMono a}, \"output\": {jsonVec v}}"]
    if !agree then
      failed := true
      IO.eprintln s!"case {c.id}: the term and its stated normal form act differently"
    entries := entries ++ [s!"  \{\n    \"id\": \"{c.id}\",\n    \"context\": \"{c.context}\",\n    \"term\": \"{c.term}\",\n    \"normal_form\": \"{c.nf}\",\n    \"agree\": {agree},\n    \"actions\": [\n" ++ ",\n".intercalate acts ++ "\n    ]\n  }"]
  IO.FS.writeFile (System.FilePath.mk output) ("[\n" ++ ",\n".intercalate entries ++ "\n]\n")
  IO.println s!"{cases.length} cases written to {output}"
  return if failed then 1 else 0
