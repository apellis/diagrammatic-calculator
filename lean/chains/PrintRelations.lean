/-
Prints the defining relations of the KLR presentation `Categorification.KLR.Diagram.pres`, read
from the Lean definition `relation` itself, one per line in the calculator's text format:

  constructor; colour variables; side condition; term (the relation says term = 0)

Run with `lake env lean PrintRelations.lean > klr-relations.txt`. The Rust test
`lean_relations_match_the_presentation` checks every printed relation against
presentations/klr.pres.toml, and CI checks that klr-relations.txt is current.
-/
import Categorification.Diagrams.KLR.Basic

open Lean Meta

namespace DiagcalcPrint

/-- The elements of a list literal. -/
partial def listElems (e : Expr) : MetaM (List Expr) := do
  let e ← whnfR e
  match_expr e with
  | List.cons _ x xs => return x :: (← listElems xs)
  | List.nil _ => return []
  | _ => throwError "not a list literal: {e}"

/-- The layers of a diagram, bottom first: generator name, strands to its left, colours. -/
partial def layers (e : Expr) : MetaM (List (String × Nat × List Expr)) := do
  let e := e.headBeta
  match e.getAppFn.constName? with
  | some ``CategoryTheory.CategoryStruct.comp =>
    let args := e.getAppArgs
    return (← layers args[args.size - 2]!) ++ (← layers args[args.size - 1]!)
  | some ``CategoryTheory.CategoryStruct.id => return []
  | some ``Categorification.KLR.Diagram.dl =>
    let args := e.getAppArgs
    -- dl {I} l g r {a b} ha hb
    let l ← listElems args[1]!
    let g ← whnfR args[2]!
    match g.getAppFn.constName?, g.getAppArgs.toList with
    | some ``Categorification.KLR.Diagram.Gen.dot, [_, c] => return [("x", l.length, [c])]
    | some ``Categorification.KLR.Diagram.Gen.cross, [_, c, d] =>
      return [("psi", l.length, [c, d])]
    | _, _ => throwError "unknown generator {g}"
  | _ =>
    match ← unfoldDefinition? e with
    | some e' => layers e'
    | none => throwError "cannot read the diagram {e}"

/-- A word in the calculator's text format: top layer first, positions from 1. -/
def word (ls : List (String × Nat × List Expr)) : String :=
  " ".intercalate (ls.reverse.map fun (g, n, _) => s!"{g}_{n + 1}")

/-- The summands of a linear combination of diagrams, with signs. -/
partial def summands (names : Expr → String) (e : Expr) : MetaM (List (Int × String)) := do
  match_expr e with
  | HSub.hSub _ _ _ _ a b =>
    return (← summands names a) ++ (← summands names b).map fun (c, s) => (-c, s)
  | HAdd.hAdd _ _ _ _ a b => return (← summands names a) ++ (← summands names b)
  | _ =>
    match e.getAppFn.constName? with
    | some ``StringDiagrams.LinDiagram.of => return [(1, word (← layers e.appArg!))]
    | some ``Categorification.KLR.Diagram.lpoly =>
      let args := e.getAppArgs
      let p := args[args.size - 1]!
      let n ← arity args[args.size - 2]!
      let vars := ", ".intercalate ((List.range n).map fun i => s!"x_{i + 1}")
      let (name, q) := match p.getAppFn.constName? with
        | some ``Categorification.KLR.qbar => ("Qbar", p.appArg!)
        | _ => ("Q", p)
      let cs := q.getAppArgs.toList.map names
      return [(1, s!"{name}_{String.join cs}({vars})")]
    | _ => throwError "cannot read the term {e}"
where
  /-- `n` for a family `Fin n → _`. -/
  arity (y : Expr) : MetaM Nat := do
    let ty ← whnf (← inferType y)
    match_expr ty.bindingDomain! with
    | Fin n => match (← evalNat n) with
      | some n => return n
      | none => throwError "unknown arity {n}"
    | _ => throwError "not a family over Fin n: {ty}"

/-- Prints each defining relation of `KLR.Diagram.pres` as `colours; condition; term = 0`. -/
def printRelations : MetaM Unit := do
  let some eqns ← getEqnsFor? ``Categorification.KLR.Diagram.relation
    | throwError "no equations"
  for eqn in eqns do
    let ty ← inferType (← mkConstWithLevelParams eqn)
    forallTelescope ty fun xs body => do
      let_expr Eq _ lhs rhs := body | throwError "not an equation"
      let r := lhs.appArg!
      let ctor := r.getAppFn.constName!.componentsRev.head!.toString
      -- colour variables in order of appearance, named a, b, c
      let cols ← xs.filterM fun x => do
        return (← inferType x).isFVar || (← whnfR (← inferType x)).isFVar
      let cols := cols.filter fun x => r.getAppArgs.contains x
      let letters := ["a", "b", "c"]
      let names (e : Expr) : String :=
        match cols.findIdx? (· == e) with
        | some i => letters[i]!
        | none => "?"
      let conds ← xs.filterM fun x => return (← inferType (← inferType x)).isProp
      let conds := conds.filter fun x => r.getAppArgs.contains x
      let condStr ← conds.toList.mapM fun h => do
        let t ← instantiateMVars (← inferType h)
        let t := t.replace fun e => if cols.contains e then some (mkConst (Name.mkSimple (names e))) else none
        return toString (← ppExpr t)
      let dom ← whnfD (← mkAppM ``Categorification.KLR.Diagram.Rel.dom #[r])
      let obj ← listElems dom.getAppArgs.back!
      let idem := s!"e({" ".intercalate (obj.map names)})"
      let terms ← summands names rhs
      let show1 (c : Int) (s : String) : String :=
        let s := if s.isEmpty then idem else s!"{s} {idem}"
        if c == 1 then s!"+ {s}" else if c == -1 then s!"- {s}" else s!"+ {c} {s}"
      let text := " ".intercalate (terms.map fun (c, s) => show1 c s)
      let text := if text.startsWith "+ " then text.drop 2 else text
      IO.println s!"{ctor}; {" ".intercalate (cols.toList.map names)}; {", ".intercalate condStr}; {text}"

end DiagcalcPrint

#eval DiagcalcPrint.printRelations
