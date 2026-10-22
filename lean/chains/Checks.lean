/-
Checks written by hand: each exported theorem, specialised, gives the library's own statement of
the same relation. They guard against an export that builds but says something other than the
library's relation (for example a different layering or a dropped term).
-/
import DiagcalcChains.CrossDotsCrossA2
import Categorification.Diagrams.KL3.BubbleSlides

set_option backward.isDefEq.respectTransparency false

noncomputable section

namespace Categorification.KL3.Diagram

open CategoryTheory StringDiagrams QuantumGroup UDot Presentation

universe w u v

/-- `cross_dots_cross_A2`, found by the calculator, gives the library's `dg_cross_dots_cross` at
`m = 2` for colours with `C.dot i j ≠ 0` and `d_ij = d_ji = 1`. -/
example {I : Type u} {C : CartanDatum I} {X Y : Type v} [AddCommGroup X] [AddCommGroup Y]
    (RD : WeightDatum C X Y) (k : Type w) [CommRing k] (μ : X) (i j : I) (h : i ≠ j)
    (hdot : C.dot i j ≠ 0) (hij : C.dij i j = 1) (hji : C.dij j i = 1) :
    dg RD k μ [up i, up j] [up i, up j]
        ([([], .cross true i j, [])] ++ List.replicate 2 ([up j], .dot (up i), []) ++
          [([], .cross true j i, [])]) =
      if C.dot i j = 0 then
        dg RD k μ [up i, up j] [up i, up j] (List.replicate 2 ([], .dot (up i), [up j]))
      else dg RD k μ [up i, up j] [up i, up j]
          (List.replicate (2 + C.dij i j) ([], .dot (up i), [up j])) +
        dg RD k μ [up i, up j] [up i, up j]
          (List.replicate 2 ([], .dot (up i), [up j]) ++
            List.replicate (C.dij j i) ([up i], .dot (up j), [])) := by
  simp only [hdot, ↓reduceIte, hij, hji, List.replicate, List.cons_append, List.nil_append]
  rw [cross_dots_cross_A2 RD k μ i j h hdot hij hji]
  -- The calculator lays the second term out with the dot on `j` in the middle.
  congr 1
  dswap 1

end Categorification.KL3.Diagram
