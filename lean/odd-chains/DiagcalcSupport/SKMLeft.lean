/-
Support for the exported chains of the super Kac–Moody 2-category (hand-written): the relations
of the leftward cups and caps in the calculator's normalization, and the layers of its downward
dot and crossing.

The calculator writes the leftward cup and cap of Brundan–Ellis (2.10), (2.11) with the scalars
`c_{λ;i}` set to `1`. odd-lean's `etaP cs i μ` and `epsP cs i μ` carry them; in each relation below
they cancel (by `c_wt_up`, `c_wt_dn`), so the relation holds for the calculator's diagrams once
some family `cs` of scalars exists. The lemmas are stated case by case on `⟨hᵢ, λ⟩`, with the
diagrams in the calculator's expansion of the derived generators, and proved from odd-lean's
Proposition 6.2 (`eq_6_6_a`, `eq_6_6_b`).
-/
import OddMath.SKM.ChevalleyLeft
import OddMath.SKM.Mates
import DiagcalcSupport.SKM

noncomputable section

namespace OddMath.SKM

open CategoryTheory StringDiagrams Presentation

universe w u v

variable {I : Type u} {X : Type v} [AddCommGroup X] {D : Datum I X} {k : Type w} [CommRing k]

theorem dc_zsign_solve {A : Type*} [AddCommGroup A] [Module k A] {S T U : ZMod 2} {f g : A}
    (h : zsign k S • f = zsign k T • g) (hU : U = S + T) : f = zsign k U • g := by
  rw [hU, zsign_add, mul_smul, ← h, smul_smul, dc_zsign_mul_self, one_smul]

theorem dc_zsign_solve1 {A : Type*} [AddCommGroup A] [Module k A] {S : ZMod 2} {f g : A}
    (h : zsign k S • f = g) (hS : S = 0) : f = g := by
  rw [hS, zsign_zero, one_smul] at h; exact h

theorem dc_zsign_solve2 {A : Type*} [AddCommGroup A] [Module k A] {S U : ZMod 2} {f g : A}
    (h : zsign k S • f = g) (hU : U = S) : f = zsign k U • g := by
  rw [hU, ← h, smul_smul, dc_zsign_mul_self, one_smul]

theorem dc_zmod_neg (a : ZMod 2) : -a = a := by revert a; decide

theorem dc_zmod_neg_one : (-1 : ZMod 2) = 1 := rfl

/-! ## The downward dot and crossing

The calculator's expansions of its downward dot and crossing (the derived generators `xd`,
`psid`) are odd-lean's right mates (Brundan–Ellis (2.1)), layer for layer. -/

theorem dc_xd_layers (i : I) :
    ddotL i = [([], Shape.cup i, [dn i]), ([dn i], Shape.dot i, [dn i]), ([dn i], Shape.cap i, [])] :=
  rfl

theorem dc_psid_layers (i j : I) :
    dcrossL i j = [([], Shape.cup j, [dn i, dn j]), ([dn j], Shape.cup i, [up j, dn i, dn j]),
      ([dn j, dn i], Shape.cross i j, [dn i, dn j]), ([dn j, dn i, up j], Shape.cap i, [dn j]),
      ([dn j, dn i], Shape.cap j, [])] :=
  rfl

variable {Sc : Scalars D k} (cs : CScalars Sc)

theorem dc_c_wt_dn (i : I) (μ : X) : cs.c (wt D μ [dn i]) i = cs.c μ i := by
  have := c_neg_wt_dn cs i (-μ)
  simpa only [neg_neg] using this

variable (Sc)

/-! ## zigzag-left-up: `η'` on the left of `Eᵢ`, then `ε'` on the right (6.6) -/

theorem dc_zzlu_nonneg (cs : CScalars Sc) (i : I) (ν : X) (hh : 0 ≤ D.h i ν) :
    cl D Sc ν [up i] [up i] ([([], Shape.dcup i (D.h i ν + 1).toNat, [up i])] ++
      (lcrossL i i ++ epsL i (D.h i ν).toNat).map (whL [up i] [])) =
      zsign k (D.parity i + 1) • cl D Sc ν [up i] [up i] [] := by
  have E := eq_6_6_a cs i ν
  have e1 : leta cs i ν = (cs.c ν i : k) •
      cl D Sc ν [up i] [up i, dn i, up i] [([], Shape.dcup i (D.h i ν + 1).toNat, [up i])] := by
    rw [leta, etaP_of_pos cs (by rw [h_wt_up]; omega), map_smul, plcL_cl, c_wt_up, h_wt_up,
      show D.h i ν + 2 - 1 = D.h i ν + 1 by ring]
    rfl
  have e2 : reps cs i ν = (-(zsign k (D.parity i * (D.h i ν : ZMod 2))) * (↑(cs.c ν i)⁻¹ : k)) •
      cl D Sc ν [up i, dn i, up i] [up i]
        ((lcrossL i i ++ epsL i (D.h i ν).toNat).map (whL [up i] [])) := by
    rw [reps, epsP_eq_of_nonneg cs (show 0 ≤ D.h i (wt D ν []) from hh), map_smul, plcL_cl]
    rfl
  have hA : SChain [up i] [([], Shape.dcup i (D.h i ν + 1).toNat, [up i])] [up i, dn i, up i] := by
    simp [Shape.dom, Shape.cod]
  have hB : SChain [up i, dn i, up i] ((lcrossL i i ++ epsL i (D.h i ν).toNat).map (whL [up i] []))
      [up i] := by
    simpa using ((sChain_lcrossL i i).append (sChain_epsL i _)).whisk [up i] []
  rw [e1, e2, Linear.smul_comp, Linear.comp_smul, smul_smul, cl_comp hA hB,
    ← cl_nil ν [up i]] at E
  rw [show (cs.c ν i : k) * (-zsign k (D.parity i * ↑(D.h i ν)) * ↑(cs.c ν i)⁻¹) =
      zsign k (D.parity i * ↑(D.h i ν) + 1) by
    rw [mul_comm, mul_assoc, Units.inv_mul, mul_one, zsign_add, dc_zsign_one, mul_neg_one]] at E
  refine dc_zsign_solve E ?_
  rw [ipar]
  generalize D.parity i = p
  generalize ((D.h i ν : ℤ) : ZMod 2) = q
  revert p q; decide

theorem dc_zzlu_m1 (cs : CScalars Sc) (i : I) (ν : X) (hh : D.h i ν = -1) :
    cl D Sc ν [up i] [up i] [([], Shape.dcup i 0, [up i]), ([up i], Shape.dcap i 0, [])] =
      cl D Sc ν [up i] [up i] [] := by
  have hν : D.h i (wt D ν [up i]) = 1 := by rw [h_wt_up]; omega
  have E := eq_6_6_a cs i ν
  have e1 : leta cs i ν = (cs.c ν i : k) •
      cl D Sc ν [up i] [up i, dn i, up i] [([], Shape.dcup i 0, [up i])] := by
    rw [leta, etaP_of_pos cs (by omega), map_smul, plcL_cl, c_wt_up, hν]
    rfl
  have e2 : reps cs i ν = (↑(cs.c ν i)⁻¹ : k) •
      cl D Sc ν [up i, dn i, up i] [up i] [([up i], Shape.dcap i 0, [])] := by
    rw [reps, epsP_of_neg cs (show D.h i (wt D ν []) < 0 by simp only [wt_nil]; omega), map_smul,
      plcL_cl, show (-D.h i (wt D ν []) - 1).toNat = 0 by simp only [wt_nil]; omega]
    rfl
  rw [e1, e2, Linear.smul_comp, Linear.comp_smul, smul_smul, Units.mul_inv, one_smul,
    cl_comp (by simp [Shape.dom, Shape.cod]) (by simp [Shape.dom, Shape.cod]),
    ipar, hh, show ((-1 : ℤ) : ZMod 2) + 1 = 0 by push_cast; rfl, mul_zero, zsign_zero,
    one_smul, ← cl_nil ν [up i]] at E
  exact E

theorem dc_zzlu_le (cs : CScalars Sc) (i : I) (ν : X) (hh : D.h i ν ≤ -2) :
    cl D Sc ν [up i] [up i] ((etaL i (-D.h i ν - 2).toNat ++ lcrossL i i).map (whL [] [up i]) ++
      [([up i], Shape.dcap i (-D.h i ν - 1).toNat, [])]) = cl D Sc ν [up i] [up i] [] := by
  have hν : D.h i (wt D ν [up i]) ≤ 0 := by rw [h_wt_up]; omega
  have E := eq_6_6_a cs i ν
  have e1 : leta cs i ν = (zsign k (ipar D i (wt D ν [up i])) * (cs.c ν i : k)) •
      cl D Sc ν [up i] [up i, dn i, up i]
        ((etaL i (-D.h i ν - 2).toNat ++ lcrossL i i).map (whL [] [up i])) := by
    rw [leta, etaP_eq_of_nonpos cs hν, map_smul, plcL_cl, c_wt_up, h_wt_up,
      show -(D.h i ν + 2) = -D.h i ν - 2 by ring]
    rfl
  have e2 : reps cs i ν = (↑(cs.c ν i)⁻¹ : k) •
      cl D Sc ν [up i, dn i, up i] [up i] [([up i], Shape.dcap i (-D.h i ν - 1).toNat, [])] := by
    rw [reps, epsP_of_neg cs (show D.h i (wt D ν []) < 0 by rw [wt_nil]; omega), map_smul, plcL_cl]
    rfl
  have hA : SChain [up i] ((etaL i (-D.h i ν - 2).toNat ++ lcrossL i i).map (whL [] [up i]))
      [up i, dn i, up i] := by
    simpa using ((sChain_etaL i _).append (sChain_lcrossL i i)).whisk [] [up i]
  have hB : SChain [up i, dn i, up i] [([up i], Shape.dcap i (-D.h i ν - 1).toNat, [])] [up i] := by
    simp [Shape.dom, Shape.cod]
  rw [e1, e2, Linear.smul_comp, Linear.comp_smul, smul_smul, mul_assoc, Units.mul_inv, mul_one,
    cl_comp hA hB, ← cl_nil ν [up i]] at E
  refine (dc_zsign_solve E (U := 0) ?_).trans (by rw [zsign_zero, one_smul])
  rw [ipar, ipar, h_wt_up]
  push_cast
  generalize D.parity i = p
  generalize ((D.h i ν : ℤ) : ZMod 2) = q
  revert p q; decide

/-! ## zigzag-left-down: `η'` on the right of `Fᵢ`, then `ε'` on the left (6.6) -/

theorem dc_zzld_ge (cs : CScalars Sc) (i : I) (μ : X) (hh : 2 ≤ D.h i μ) :
    cl D Sc μ [dn i] [dn i] ([([dn i], Shape.dcup i (D.h i μ - 1).toNat, [])] ++
      (lcrossL i i ++ epsL i (D.h i μ - 2).toNat).map (whL [] [dn i])) =
      zsign k (D.parity i * D.h i μ + 1) • cl D Sc μ [dn i] [dn i] [] := by
  have E := eq_6_6_b cs i μ
  have hA : SChain [dn i] [([dn i], Shape.dcup i (D.h i μ - 1).toNat, [])] [dn i, up i, dn i] := by
    simp [Shape.dom, Shape.cod]
  have hB : SChain [dn i, up i, dn i]
      ((lcrossL i i ++ epsL i (D.h i μ - 2).toNat).map (whL [] [dn i])) [dn i] := by
    simpa using ((sChain_lcrossL i i).append (sChain_epsL i _)).whisk [] [dn i]
  rw [retaF, lepsF, etaP_of_pos cs (by simp only [wt_nil]; omega),
    epsP_eq_of_nonneg cs (by rw [h_wt_dn]; omega), map_smul, map_smul, plcL_cl, plcL_cl,
    Linear.smul_comp, Linear.comp_smul, smul_smul, dc_c_wt_dn, h_wt_dn, wt_nil] at E
  change _ • (cl D Sc μ [dn i] [dn i, up i, dn i] [([dn i], Shape.dcup i (D.h i μ - 1).toNat, [])] ≫
    cl D Sc μ [dn i, up i, dn i] [dn i]
      ((lcrossL i i ++ epsL i (D.h i μ - 2).toNat).map (whL [] [dn i]))) = _ at E
  rw [cl_comp hA hB, ← cl_nil μ [dn i],
    show (cs.c μ i : k) * (-zsign k (D.parity i * ↑(D.h i μ - 2)) * ↑(cs.c μ i)⁻¹) =
      zsign k (D.parity i * ↑(D.h i μ - 2) + 1) by
    rw [mul_comm, mul_assoc, Units.inv_mul, mul_one, zsign_add, dc_zsign_one, mul_neg_one]] at E
  refine dc_zsign_solve2 E ?_
  push_cast
  generalize D.parity i = p
  generalize ((D.h i μ : ℤ) : ZMod 2) = q
  revert p q; decide

theorem dc_zzld_one (cs : CScalars Sc) (i : I) (μ : X) (hh : D.h i μ = 1) :
    cl D Sc μ [dn i] [dn i] [([dn i], Shape.dcup i 0, []), ([], Shape.dcap i 0, [dn i])] =
      cl D Sc μ [dn i] [dn i] [] := by
  have E := eq_6_6_b cs i μ
  rw [retaF, lepsF, etaP_of_pos cs (by simp only [wt_nil]; omega),
    epsP_of_neg cs (by rw [h_wt_dn]; omega), map_smul, map_smul, plcL_cl, plcL_cl,
    Linear.smul_comp, Linear.comp_smul, smul_smul, dc_c_wt_dn, wt_nil, Units.mul_inv, one_smul,
    h_wt_dn, hh, show ((1 : ℤ) - 1).toNat = 0 by rfl, show (-((1 : ℤ) - 2) - 1).toNat = 0 by rfl]
    at E
  change cl D Sc μ [dn i] [dn i, up i, dn i] [([dn i], Shape.dcup i 0, [])] ≫
    cl D Sc μ [dn i, up i, dn i] [dn i] [([], Shape.dcap i 0, [dn i])] = _ at E
  rw [cl_comp (by simp [Shape.dom, Shape.cod]) (by simp [Shape.dom, Shape.cod]),
    ← cl_nil μ [dn i]] at E
  exact E

theorem dc_zzld_le (cs : CScalars Sc) (i : I) (μ : X) (hh : D.h i μ ≤ 0) :
    cl D Sc μ [dn i] [dn i] ((etaL i (-D.h i μ).toNat ++ lcrossL i i).map (whL [dn i] []) ++
      [([], Shape.dcap i (-D.h i μ + 1).toNat, [dn i])]) =
      zsign k (D.parity i * (D.h i μ + 1)) • cl D Sc μ [dn i] [dn i] [] := by
  have E := eq_6_6_b cs i μ
  have hA : SChain [dn i] ((etaL i (-D.h i μ).toNat ++ lcrossL i i).map (whL [dn i] []))
      [dn i, up i, dn i] := by
    simpa using ((sChain_etaL i _).append (sChain_lcrossL i i)).whisk [dn i] []
  have hB : SChain [dn i, up i, dn i] [([], Shape.dcap i (-D.h i μ + 1).toNat, [dn i])] [dn i] := by
    simp [Shape.dom, Shape.cod]
  rw [retaF, lepsF, etaP_eq_of_nonpos cs (by simp only [wt_nil]; omega),
    epsP_of_neg cs (by rw [h_wt_dn]; omega), map_smul, map_smul, plcL_cl, plcL_cl,
    Linear.smul_comp, Linear.comp_smul, smul_smul, dc_c_wt_dn, wt_nil, h_wt_dn,
    show -(D.h i μ - 2) - 1 = -D.h i μ + 1 by ring, mul_assoc, Units.mul_inv, mul_one] at E
  change _ • (cl D Sc μ [dn i] [dn i, up i, dn i]
      ((etaL i (-D.h i μ).toNat ++ lcrossL i i).map (whL [dn i] [])) ≫
    cl D Sc μ [dn i, up i, dn i] [dn i] [([], Shape.dcap i (-D.h i μ + 1).toNat, [dn i])]) = _ at E
  rw [cl_comp hA hB, ← cl_nil μ [dn i]] at E
  refine dc_zsign_solve2 E ?_
  unfold ipar
  ring

/-! ## Bubbles: the leftward cup or cap closed by a rightward one (Brundan–Ellis (2.13), (2.14))

These need no scalars: odd-lean's inversion relations `cl_invP₅`, `cl_invM₅` are already in the
calculator's normalization. -/

theorem dc_bubL_neg (i : I) (ν : X) (m : ℕ) (hm : (m : ℤ) < D.h i ν - 1) :
    cl D Sc ν [] [] ([([], Shape.dcup i (D.h i ν - 1).toNat, [])] ++ epsL i m) = 0 := by
  rw [show [([], Shape.dcup i (D.h i ν - 1).toNat, [])] = dcupL i (D.h i ν - 1).toNat from rfl,
    cl_invP₅ Sc i ν m _ (by omega) (by omega), ite_eq_right (by omega)]

theorem dc_bubL_one (i : I) (ν : X) (hh : 0 < D.h i ν) :
    cl D Sc ν [] [] ([([], Shape.dcup i (D.h i ν - 1).toNat, [])] ++ epsL i (D.h i ν - 1).toNat) =
      cl D Sc ν [] [] [] := by
  rw [show [([], Shape.dcup i (D.h i ν - 1).toNat, [])] = dcupL i (D.h i ν - 1).toNat from rfl,
    cl_invP₅ Sc i ν _ _ (by omega) (by omega), ite_eq_left rfl]

theorem dc_bubR_neg (i : I) (ν : X) (m : ℕ) (hm : (m : ℤ) < -D.h i ν - 1) :
    cl D Sc ν [] [] (etaL i m ++ [([], Shape.dcap i (-D.h i ν - 1).toNat, [])]) = 0 := by
  rw [show [([], Shape.dcap i (-D.h i ν - 1).toNat, [])] = dcapL i (-D.h i ν - 1).toNat from rfl,
    cl_invM₅ Sc i ν m _ (by omega) (by omega), ite_eq_right (by omega)]

theorem dc_bubR_one (i : I) (ν : X) (hh : D.h i ν < 0) :
    cl D Sc ν [] [] (etaL i (-D.h i ν - 1).toNat ++ [([], Shape.dcap i (-D.h i ν - 1).toNat, [])]) =
      cl D Sc ν [] [] [] := by
  rw [show [([], Shape.dcap i (-D.h i ν - 1).toNat, [])] = dcapL i (-D.h i ν - 1).toNat from rfl,
    cl_invM₅ Sc i ν _ _ (by omega) (by omega), ite_eq_left rfl]

/-- Fits a lemma `h` above, at the weights and parities of the goal's context, to the goal: the
weights and parities rewritten by the hypotheses, the integers evaluated, the layers normalized,
and the signs computed. -/
macro "dc_fit " h:ident : tactic => `(tactic| (
  try simp only [*, Int.reduceNeg, Int.reduceAdd, Int.reduceSub, Int.reduceMul, Int.reduceToNat,
    Int.cast_zero, Int.cast_one, Int.cast_ofNat, Int.cast_neg, Int.cast_add, dc_zmod_neg,
    swapLR, swapRL, whL, crossL, dotsL, sigmaL, lcrossL, epsL, etaL, dcupL, dcapL,
    List.replicate_succ, List.replicate_zero, List.map_cons, List.map_nil, List.cons_append,
    List.nil_append, List.append_nil, List.singleton_append, List.append_assoc,
    dc_zsign_zero, dc_zsign_one, dc_zmod_two, dc_zmod_three, dc_zmod_four, dc_zmod_five,
    dc_zmod_six, dc_zmod_one_add_one, Nat.cast_zero, Nat.cast_one, Nat.cast_ofNat, mul_one,
    one_mul, mul_zero, zero_mul, add_zero, zero_add, mul_neg, neg_mul, neg_neg, one_smul,
    neg_one_smul, neg_smul, smul_neg] at $h:ident
  exact $h))

/-- zigzag-left-up at the weight of the goal's context. -/
macro "dc_zzlu " Sc:term:max cs:term:max i:term:max ν:term:max : tactic => `(tactic| first
  | (have L := dc_zzlu_nonneg $Sc $cs $i $ν (by omega); dc_fit L)
  | (have L := dc_zzlu_m1 $Sc $cs $i $ν (by omega); dc_fit L)
  | (have L := dc_zzlu_le $Sc $cs $i $ν (by omega); dc_fit L))

/-- zigzag-left-down at the weight of the goal's context. -/
macro "dc_zzld " Sc:term:max cs:term:max i:term:max ν:term:max : tactic => `(tactic| first
  | (have L := dc_zzld_ge $Sc $cs $i $ν (by omega); dc_fit L)
  | (have L := dc_zzld_one $Sc $cs $i $ν (by omega); dc_fit L)
  | (have L := dc_zzld_le $Sc $cs $i $ν (by omega); dc_fit L))

/-- The bubble relations at the weight of the goal's context. -/
macro "dc_bubln " Sc:term:max i:term:max ν:term:max m:term:max : tactic => `(tactic|
  (have L := dc_bubL_neg $Sc $i $ν $m (by omega); dc_fit L))
macro "dc_bubl1 " Sc:term:max i:term:max ν:term:max : tactic => `(tactic|
  (have L := dc_bubL_one $Sc $i $ν (by omega); dc_fit L))
macro "dc_bubrn " Sc:term:max i:term:max ν:term:max m:term:max : tactic => `(tactic|
  (have L := dc_bubR_neg $Sc $i $ν $m (by omega); dc_fit L))
macro "dc_bubr1 " Sc:term:max i:term:max ν:term:max : tactic => `(tactic|
  (have L := dc_bubR_one $Sc $i $ν (by omega); dc_fit L))

end OddMath.SKM
