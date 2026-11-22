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
* weighted KLR algebras (contexts `klrw`, `docs/klrw.md`; B. Webster, arXiv:1209.2463v4, and
  red strands as in arXiv:1309.3796): each `ψ_k`, `to(B')` and `b(π; B')` is the straight-line
  diagram between two loadings, read as its events in height order (crossings of two strands,
  of a strand with a ghost, and of a strand with a red strand). A same-colour crossing is the
  divided difference, a crossing of different colours is `s_k`, a strand passing rightward
  through the ghost of an edge `e` multiplies by `Q_e(x_head, x_tail)`, and a strand passing
  rightward through a red strand of label `λ` multiplies by `x^{λ_i}`. Loadings are compared up
  to isotopy, by the order of the strands, ghosts and red strands.

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
  for t0 in toks do
    let mut t := t0
    if t == "+" || t == "-" then
      -- a sign right after another (or at the start) is unary: `a + -1 b`, `a - -b`
      if started then
        out := out ++ [{ coef := sign * coef, gens, idem }]
        sign := if t == "-" then -1 else 1
      else
        sign := if t == "-" then -sign else sign
      coef := 1; gens := []; idem := none; started := false
      continue
    if !started && t.startsWith "-" && t.length > 1 then
      sign := -sign
      t := sdrop t 1
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

/-! ## Weighted KLR algebras (KLRW) with red strands

The conventions of `docs/klrw.md`, written out again here from that description. A loading is a
list of black points `(colour, position)` from left to right. The context gives the default
graph (an edge `a → b` of weight `0` for `a > b` with `a_ab ≠ 0`), reoriented and reweighted by
`theta_ab=w` (tail `a`, head `b`), and red strands `λ@ϖ`. The edge `t → h` has
`Q(u, v) = u^{-a_ht} + v^{-a_th}`, `u` the dot on the `h` strand, and ghosts at `x + w` of the
points coloured `h`.

The straight-line diagram `b_π` runs strand `k` from `x_k` to `y_{π(k)} + ε^{π(k)+1}`. Each
event (two strands cross; a strand coloured `t` crosses the ghost of a strand coloured `h`; a
strand crosses a red strand) is a sign change of `a + t (b + s(ε))` on `[0, 1]`, at height
`t = -a / (b + s(ε))`; events are sorted by height for small `ε > 0`, and at equal heights (a
crossing and the ghost crossing of the same pair at weight `0`) the ghost crossing comes first.
Reading from the bottom: a crossing of strands at positions `m, m+1` acts by the divided
difference (same colour) or `s_m` (different colours); a strand coloured `t` passing the ghost
of a strand coloured `h` from left to right multiplies by `Q(x_head, x_tail)`, and from right
to left acts trivially; a strand coloured `i` crossing a red strand `λ` from left to right
multiplies by `x^{⟨λ, α_i^∨⟩}`, and from right to left acts trivially. Positions refer to the
height of the event.

Terms: `e(L)` at the bottom of each summand, `x_k`, `psi_k` (colours at `k, k+1` swap, positions
stay; the diagram is `b_{s_k}`), `to(L)` (`b_1` to `L`) and `b(π; L)` (`π` one-line, from 1). -/

abbrev Loading := List (Nat × Rat)

structure KCtx where
  cartan : List (List Int)
  ncol : Nat
  /-- `(tail, head, weight)`. -/
  edges : List (Nat × Nat × Rat)
  /-- `(position, ⟨λ, α_i^∨⟩ for each colour)`, left to right. -/
  reds : List (Rat × List Nat)

/-- Splits on spaces outside double quotes and parentheses. -/
def splitTop (s : String) : List String := Id.run do
  let mut out : List String := []
  let mut cur : List Char := []
  let mut depth : Nat := 0
  let mut quote := false
  for ch in s.toList do
    if ch == '"' then quote := !quote
    if ch == '(' then depth := depth + 1
    if ch == ')' then depth := depth - 1
    if ch == ' ' && depth == 0 && !quote then
      if !cur.isEmpty then out := out ++ [String.ofList cur]
      cur := []
    else cur := cur ++ [ch]
  if !cur.isEmpty then out := out ++ [String.ofList cur]
  return out

def parseInt (s : String) : Int :=
  if s.startsWith "-" then -((sdrop s 1).toNat! : Int) else (s.toNat! : Int)

def parseRat (s : String) : Rat :=
  match s.splitOn "/" with
  | [p, q] => mkRat (parseInt (strim p)) (strim q).toNat!
  | _ => (parseInt (strim s) : Rat)

def parseLoading (s : String) : Loading :=
  ((s.splitOn " ").filter (· ≠ "")).map fun t =>
    match t.splitOn "@" with
    | [c, x] => (colourIndex c, parseRat x)
    | _ => (0, 0)

def fmtLoading (l : Loading) : String :=
  " ".intercalate (l.map fun (p : Nat × Rat) => s!"{colourName p.1}@{p.2}")

def insertRed (r : Rat × List Nat) : List (Rat × List Nat) → List (Rat × List Nat)
  | [] => [r]
  | r' :: rest => if r.1 < r'.1 then r :: r' :: rest else r' :: insertRed r rest

def parseKContext (line : String) : KCtx := Id.run do
  let words := splitTop line
  let mut ty := ""
  let mut thetas : List (String × String) := []
  let mut red := ""
  for w in words.drop 1 do
    match w.splitOn "=" with
    | [k, v] =>
      if k == "type" then ty := v
      else if k == "red" then red := String.ofList (v.toList.filter (· ≠ '"'))
      else if k.startsWith "theta_" then thetas := thetas ++ [(sdrop k 6, v)]
    | _ => pure ()
  let cartan := cartanOf ty
  let ncol := cartan.length
  let mut edges : List (Nat × Nat × Rat) := []
  for a in [0:ncol] do
    for b in [0:a] do
      if (cartan.getD a []).getD b 0 != 0 then edges := edges ++ [(a, b, 0)]
  for (name, v) in thetas do
    let t := colourIndex (String.ofList (name.toList.take 1))
    let h := colourIndex (String.ofList (name.toList.drop 1))
    edges := edges.map fun (e : Nat × Nat × Rat) =>
      if (e.1 == t && e.2.1 == h) || (e.1 == h && e.2.1 == t) then (t, h, parseRat v) else e
  let mut reds : List (Rat × List Nat) := []
  for item in red.splitOn "," do
    let item := strim item
    if item.isEmpty then continue
    match item.splitOn "@" with
    | [wt, pos] => reds := insertRed (parseRat (strim pos), parseWeight (strim wt)) reds
    | _ => pure ()
  return { cartan, ncol, edges, reds }

def cartanEntry (ctx : KCtx) (a b : Nat) : Int := (ctx.cartan.getD a []).getD b 0

/-- What is right of what, for every interacting pair: equal order types are isotopic. -/
def orderType (ctx : KCtx) (l : Loading) : List Nat × List Bool := Id.run do
  let mut sides : List Bool := []
  for (t, h, w) in ctx.edges do
    for (cj, xj) in l do
      if cj != t then continue
      for (ck, xk) in l do
        if ck == h then sides := sides ++ [decide (xk + w < xj)]
  for (_, x) in l do
    for (rp, _) in ctx.reds do
      sides := sides ++ [decide (rp < x)]
  return (l.map (·.1), sides)

inductive KAct where
  | cross (j k : Nat)
  /-- A strand of colour `t` (strand `tail`) passes the ghost of strand `head` (colour `h`):
  multiply by `x_head^qa + x_tail^qb` if `right`. -/
  | ghost (tail head qa qb : Nat) (right : Bool)
  | red (j e : Nat) (right : Bool)

structure KEv where
  a : Rat
  b : Rat
  s : List (Nat × Int)
  kind : Nat
  act : KAct

def ratSign (x : Rat) : Int := if 0 < x then 1 else if x < 0 then -1 else 0

/-- The sign of `t_u − t_v` for small `ε > 0`. -/
def heightCmp (u v : KEv) : Int := Id.run do
  -- numerator `v.a (u.b + u.s) − u.a (v.b + v.s)`, as coefficients of `ε^m`
  let mut coeffs : List (Nat × Rat) := [(0, v.a * u.b - u.a * v.b)]
  let add := fun (cs : List (Nat × Rat)) (m : Nat) (c : Rat) =>
    if cs.any (·.1 == m) then cs.map (fun (p : Nat × Rat) => if p.1 == m then (m, p.2 + c) else p)
    else cs ++ [(m, c)]
  for (m, c) in u.s do coeffs := add coeffs m (v.a * c)
  for (m, c) in v.s do coeffs := add coeffs m (-(u.a * c))
  let maxM := coeffs.foldl (fun acc (p : Nat × Rat) => max acc p.1) 0
  let mut lead : Int := 0
  for m in [0:maxM + 1] do
    if lead == 0 then
      let c := (coeffs.filter (·.1 == m)).foldl (fun acc (p : Nat × Rat) => acc + p.2) 0
      lead := ratSign c
  return lead * ratSign u.b * ratSign v.b

def evBefore (u v : KEv) : Bool :=
  let c := heightCmp u v
  c < 0 || (c == 0 && u.kind < v.kind)

def insertEv (e : KEv) : List KEv → List KEv
  | [] => [e]
  | f :: rest => if evBefore e f then e :: f :: rest else f :: insertEv e rest

/-- The events of `b_π` from `bot` to `top`, bottom first; `none` if a strand changes colour. -/
def segmentEvents (ctx : KCtx) (bot top : Loading) (perm : List Nat) : Option (List KEv) := Id.run do
  let n := bot.length
  for k in [0:n] do
    if (bot.getD k (0, 0)).1 != (top.getD (perm.getD k 0) (0, 0)).1 then return none
  let x := fun k => (bot.getD k (0, 0)).2
  let z := fun k => (top.getD (perm.getD k 0) (0, 0)).2
  let p := fun k => perm.getD k 0 + 1
  let col := fun k => (bot.getD k (0, 0)).1
  let mut evs : List KEv := []
  let mut cands : List KEv := []
  for j in [0:n] do
    for k in [j+1:n] do
      cands := cands ++ [KEv.mk (x j - x k) ((z j - x j) - (z k - x k))
        [(p j, 1), (p k, -1)] 1 (KAct.cross j k)]
  for (t, h, w) in ctx.edges do
    for j in [0:n] do
      if col j != t then continue
      for k in [0:n] do
        if k == j || col k != h then continue
        let a := x j - x k - w
        let qa := (-(cartanEntry ctx h t)).toNat
        let qb := (-(cartanEntry ctx t h)).toNat
        cands := cands ++ [KEv.mk a ((z j - x j) - (z k - x k)) [(p j, 1), (p k, -1)]
          0 (KAct.ghost j k qa qb (decide (a < 0)))]
  for (rp, exps) in ctx.reds do
    for j in [0:n] do
      let a := x j - rp
      cands := cands ++ [KEv.mk a (z j - x j) [(p j, 1)] 0
        (KAct.red j (exps.getD (col j) 0) (decide (a < 0)))]
  for e in cands do
    if decide (0 < e.a) != decide (0 < e.a + e.b) then evs := insertEv e evs
  return some evs

def polyVarPow (n m e : Nat) : Poly := [((unitMono n).set m e, 1)]

/-- Applies the events to `f`, a polynomial in the variables of the bottom positions. -/
def applyEvents (bot : Loading) (evs : List KEv) (f : Poly) : Poly := Id.run do
  let n := bot.length
  let mut order : List Nat := List.range n
  let mut g := f
  let pos := fun (o : List Nat) (k : Nat) => (o.findIdx? (· == k)).getD 0
  for e in evs do
    match e.act with
    | KAct.cross j k =>
      let m := min (pos order j) (pos order k)
      order := swapAt order m
      if (bot.getD j (0, 0)).1 == (bot.getD k (0, 0)).1 then g := polyDiv false n m g
      else g := polyS false m g
    | KAct.ghost tail head qa qb right =>
      if right then
        let q := polyAdd (polyVarPow n (pos order head) qa) (polyVarPow n (pos order tail) qb)
        g := polyMul false q g
    | KAct.red j ex right =>
      if right then g := polyMul false (polyVarPow n (pos order j) ex) g
  return g

inductive KGen where
  | x (k : Nat)
  | psi (k : Nat)
  | to (l : Loading)
  | b (perm : List Nat) (l : Loading)

structure KSummand where
  coef : Int
  gens : List KGen   -- top first
  idem : Loading

def parseKTerm (s : String) : List KSummand := Id.run do
  let toks := splitTop s
  let mut out : List KSummand := []
  let mut sign : Int := 1
  let mut coef : Int := 1
  let mut gens : List KGen := []
  let mut idem : Loading := []
  let mut started := false
  for t0 in toks do
    let mut t := t0
    if t == "+" || t == "-" then
      -- a sign right after another (or at the start) is unary: `a + -1 b`, `a - -b`
      if started then
        out := out ++ [{ coef := sign * coef, gens, idem }]
        sign := if t == "-" then -1 else 1
      else
        sign := if t == "-" then -sign else sign
      coef := 1; gens := []; idem := []; started := false
      continue
    if !started && t.startsWith "-" && t.length > 1 then
      sign := -sign
      t := sdrop t 1
    started := true
    let inner := fun (k : Nat) => sdropRight (sdrop t k) 1
    if t.all Char.isDigit then
      coef := coef * (t.toNat! : Int)
    else if t.startsWith "e(" then idem := parseLoading (inner 2)
    else if t.startsWith "to(" then gens := gens ++ [KGen.to (parseLoading (inner 3))]
    else if t.startsWith "b(" then
      match (inner 2).splitOn ";" with
      | [ps, ls] =>
        let perm := ((ps.splitOn " ").filter (· ≠ "")).map (·.toNat! - 1)
        gens := gens ++ [KGen.b perm (parseLoading (strim ls))]
      | _ => pure ()
    else
      match t.splitOn "_" with
      | [name, rest] =>
        let (p, pow) := match rest.splitOn "^" with
          | [p, e] => (p.toNat!, e.toNat!)
          | _ => (rest.toNat!, 1)
        let g := if name == "x" then KGen.x (p - 1) else KGen.psi (p - 1)
        gens := gens ++ List.replicate pow g
      | _ => pure ()
  if started then out := out ++ [{ coef := sign * coef, gens, idem }]
  return out

/-- One summand applied to `f e(src)`: the top loading and the polynomial, or `none`. -/
def actKSummand (ctx : KCtx) (src : Loading) (f : Poly) (sm : KSummand) :
    Option (Loading × Poly) := Id.run do
  if orderType ctx sm.idem != orderType ctx src then return none
  let n := src.length
  let mut cur := src
  let mut g := polyScale sm.coef f
  for gen in sm.gens.reverse do
    let (to, perm) := match gen with
      | KGen.x _ => (cur, List.range n)
      | KGen.psi k =>
        (((cur.set k ((cur.getD (k + 1) (0, 0)).1, (cur.getD k (0, 0)).2)).set (k + 1)
          ((cur.getD k (0, 0)).1, (cur.getD (k + 1) (0, 0)).2)), swapAt (List.range n) k)
      | KGen.to l => (l, List.range n)
      | KGen.b perm l => (l, perm)
    match gen with
    | KGen.x k => g := polyMul false [(varMono n k, 1)] g
    | _ =>
      match segmentEvents ctx cur to perm with
      | none => return none
      | some evs =>
        g := applyEvents cur evs g
        cur := to
  return some (cur, g)

abbrev KVec := List (Loading × Poly)

def kvecInsert (ctx : KCtx) (l : Loading) (f : Poly) : KVec → KVec
  | [] => if f.isEmpty then [] else [(l, f)]
  | (l', g) :: rest =>
    if orderType ctx l == orderType ctx l' then
      (let h := polyAdd g f; if h.isEmpty then rest else (l', h) :: rest)
    else (l', g) :: kvecInsert ctx l f rest

def actKTerm (ctx : KCtx) (src : Loading) (f : Poly) (t : List KSummand) : KVec :=
  t.foldl (fun (acc : KVec) sm =>
    match actKSummand ctx src f sm with
    | some (l, g) => kvecInsert ctx l g acc
    | none => acc) []

/-- Equal as vectors: the same components up to isotopy, in any order. -/
def kvecEq (ctx : KCtx) (v w : KVec) : Bool :=
  v.length == w.length && v.all fun (l, f) =>
    w.any fun (l', g) => orderType ctx l == orderType ctx l' && f == g

def jsonKVec (v : KVec) : String :=
  jsonList (v.map fun (t : Loading × Poly) =>
    s!"\{\"target\": \"{fmtLoading t.1}\", \"poly\": {jsonPoly t.2}}")

/-- The source loadings: the bottom idempotents of the term, one per order type. -/
def kSources (ctx : KCtx) (t : List KSummand) : List Loading :=
  t.foldl (fun (acc : List Loading) sm =>
    if acc.any (fun l => orderType ctx l == orderType ctx sm.idem) then acc else acc ++ [sm.idem]) []

def jsonEsc (s : String) : String :=
  String.join (s.toList.map fun ch => if ch == '"' then "\\\"" else ch.toString)

def main (args : List String) : IO UInt32 := do
  let input := args.getD 0 "tests/golden/cases.txt"
  let output := args.getD 1 "tests/golden/actions.json"
  let cases := readCases (← IO.FS.lines (System.FilePath.mk input))
  let mut failed := false
  let mut entries : List String := []
  for c in cases do
    let mut agree := true
    let mut acts : List String := []
    if c.context.startsWith "klrw " then
      let ctx := parseKContext c.context
      let term := parseKTerm c.term
      let nf := parseKTerm c.nf
      for src in kSources ctx term do
        for a in inputs src.length do
          let f : Poly := [(a, 1)]
          let v := actKTerm ctx src f term
          let w := actKTerm ctx src f nf
          if !kvecEq ctx v w then
            agree := false
          acts := acts ++ [s!"      \{\"source\": \"{fmtLoading src}\", \"input\": {jsonMono a}, \"output\": {jsonKVec v}}"]
    else
      let ctx := parseContext c.context
      let term := parseTerm c.term
      let nf := parseTerm c.nf
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
    entries := entries ++ [s!"  \{\n    \"id\": \"{c.id}\",\n    \"context\": \"{jsonEsc c.context}\",\n    \"term\": \"{c.term}\",\n    \"normal_form\": \"{c.nf}\",\n    \"agree\": {agree},\n    \"actions\": [\n" ++ ",\n".intercalate acts ++ "\n    ]\n  }"]
  IO.FS.writeFile (System.FilePath.mk output) ("[\n" ++ ",\n".intercalate entries ++ "\n]\n")
  IO.println s!"{cases.length} cases written to {output}"
  return if failed then 1 else 0
