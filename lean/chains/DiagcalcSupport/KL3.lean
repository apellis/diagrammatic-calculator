/-
Support for the exported KL chains (hand-written): lemmas that evaluate the labelled bubbles
`cwU`/`ccwU` of categorification-lean at a concrete label, and the tactic `dc_bub h`, which
finishes a curl or decomposition relation at a weight `h : ip RD i ν = n` once the library's
lemma has been rewritten: it expands the sums, evaluates the bubbles (real, degree zero or
zero) and brings the result to the calculator's diagrams.
-/
import Categorification.Diagrams.KL3.Rewriting
import Categorification.Diagrams.KL3.BubbleSlidesGen
import Categorification.Diagrams.KL3.Pitchfork

set_option backward.isDefEq.respectTransparency false

noncomputable section

namespace Categorification.KL3.Diagram

open CategoryTheory StringDiagrams QuantumGroup UDot Presentation

universe w u v

section

variable {I : Type u} {C : CartanDatum I} {X Y : Type v} [AddCommGroup X] [AddCommGroup Y]
  (RD : WeightDatum C X Y) (k : Type w) [CommRing k]

theorem dc_ip_wt_dn (i j : I) (lam : X) :
    ip RD i (wt RD lam [dn j]) = -A C i j + ip RD i lam := by
  simp [ip, wt, sh, RD.pair_iY_iX_eq_A, sgn]

theorem dc_bubRU_dg (lam : X) (l : Letter I) (A : List (LayerData I)) :
    bubRU RD k lam l (dg RD k lam [] [] A) = dg RD k lam [l] [l] (A.map (whL [l] [])) :=
  plcL_dg_nil RD k lam [l] [] [] A

theorem dc_bubLU_dg (μ : X) (l : Letter I) (A : List (LayerData I)) :
    bubLU RD k μ l (dg RD k (wt RD μ [l]) [] [] A) = dg RD k μ [l] [l] (A.map (whL [] [l])) :=
  plcL_dg RD k μ [] [l] [] [] A

theorem dc_bubRU_neg (lam : X) (l : Letter I) (β : (pres RD k).obj (ob RD lam []) ⟶ (pres RD k).obj (ob RD lam [])) :
    bubRU RD k lam l (-β) = -(bubRU RD k lam l β : (pres RD k).obj (ob RD lam [l]) ⟶ (pres RD k).obj (ob RD lam [l])) := map_neg _ _

theorem dc_bubRU_add (lam : X) (l : Letter I) (β γ : (pres RD k).obj (ob RD lam []) ⟶ (pres RD k).obj (ob RD lam [])) :
    bubRU RD k lam l (β + γ) =
      (bubRU RD k lam l β : (pres RD k).obj (ob RD lam [l]) ⟶ (pres RD k).obj (ob RD lam [l])) + bubRU RD k lam l γ := map_add _ _ _

theorem dc_bubRU_zsmul (lam : X) (l : Letter I) (c : ℤ) (β : (pres RD k).obj (ob RD lam []) ⟶ (pres RD k).obj (ob RD lam [])) :
    bubRU RD k lam l (c • β) = c • (bubRU RD k lam l β : (pres RD k).obj (ob RD lam [l]) ⟶ (pres RD k).obj (ob RD lam [l])) := map_zsmul _ _ _

theorem dc_bubLU_neg (μ : X) (l : Letter I) (β : (pres RD k).obj (ob RD (wt RD μ [l]) []) ⟶ (pres RD k).obj (ob RD (wt RD μ [l]) [])) :
    bubLU RD k μ l (-β) = -(bubLU RD k μ l β : (pres RD k).obj (ob RD μ [l]) ⟶ (pres RD k).obj (ob RD μ [l])) := map_neg _ _

theorem dc_bubLU_add (μ : X) (l : Letter I) (β γ : (pres RD k).obj (ob RD (wt RD μ [l]) []) ⟶ (pres RD k).obj (ob RD (wt RD μ [l]) [])) :
    bubLU RD k μ l (β + γ) =
      (bubLU RD k μ l β : (pres RD k).obj (ob RD μ [l]) ⟶ (pres RD k).obj (ob RD μ [l])) + bubLU RD k μ l γ := map_add _ _ _

theorem dc_bubLU_zsmul (μ : X) (l : Letter I) (c : ℤ)
    (β : (pres RD k).obj (ob RD (wt RD μ [l]) []) ⟶ (pres RD k).obj (ob RD (wt RD μ [l]) [])) :
    bubLU RD k μ l (c • β) = c • (bubLU RD k μ l β : (pres RD k).obj (ob RD μ [l]) ⟶ (pres RD k).obj (ob RD μ [l])) := map_zsmul _ _ _

/-- Composition with a negated endomorphism, for the `End` negation that the presentation's
shortcut instances pick (which `Preadditive.neg_comp` does not match syntactically). -/
theorem dc_end_neg_comp {a : Obj (psig RD)} (f g : End ((pres RD k).obj a)) :
    (-f) ≫ g = -(f ≫ g : End ((pres RD k).obj a)) := Preadditive.neg_comp ..

theorem dc_end_comp_neg {a : Obj (psig RD)} (f g : End ((pres RD k).obj a)) :
    f ≫ (-g) = -(f ≫ g : End ((pres RD k).obj a)) := Preadditive.comp_neg ..

theorem dc_end_add_comp {a : Obj (psig RD)} (f g h : End ((pres RD k).obj a)) :
    (f + g) ≫ h = (f ≫ h : End ((pres RD k).obj a)) + g ≫ h := Preadditive.add_comp ..

theorem dc_end_comp_add {a : Obj (psig RD)} (f g h : End ((pres RD k).obj a)) :
    f ≫ (g + h) = (f ≫ g : End ((pres RD k).obj a)) + f ≫ h := Preadditive.comp_add ..

theorem dc_cwU_one {ν : X} {i : I} {m : ℤ} (h : m = ip RD i ν - 1) : cwU RD k ν i m = 𝟙 _ := by
  subst h; exact cwU_deg0 RD k ν i
theorem dc_ccwU_one {ν : X} {i : I} {m : ℤ} (h : m = -ip RD i ν - 1) : ccwU RD k ν i m = 𝟙 _ := by
  subst h; exact ccwU_deg0 RD k ν i
theorem dc_cwU_real {ν : X} {i : I} {m : ℤ} (h : 0 ≤ m) :
    cwU RD k ν i m = dg RD k ν [] [] (cwLs i m.toNat) := by
  obtain ⟨n, rfl⟩ := Int.eq_ofNat_of_zero_le h; simpa using cwU_of_nonneg RD k ν i n
theorem dc_ccwU_real {ν : X} {i : I} {m : ℤ} (h : 0 ≤ m) :
    ccwU RD k ν i m = dg RD k ν [] [] (ccwLs i m.toNat) := by
  obtain ⟨n, rfl⟩ := Int.eq_ofNat_of_zero_le h; simpa using ccwU_of_nonneg RD k ν i n
/-- A fake counterclockwise bubble by the infinite Grassmannian relation in degree
`d = m + 1 + ⟨i, ρ⟩ ≥ 1`: minus the products of lower counterclockwise and clockwise bubbles. -/
theorem dc_ccwU_fake {ρ : X} {i : I} {m : ℤ} (_h1 : m < 0) (h2 : 0 < m + 1 + ip RD i ρ) :
    ccwU RD k ρ i m = -∑ j ∈ Finset.range (m + 1 + ip RD i ρ).toNat,
      ccwU RD k ρ i (-ip RD i ρ - 1 + j) ≫
        cwU RD k ρ i (ip RD i ρ - 1 + (((m + 1 + ip RD i ρ).toNat : ℕ) - (j : ℤ))) := by
  have G := grassmannian_all RD k ρ i (m + 1 + ip RD i ρ).toNat
  rw [ite_eq_right (by omega), Finset.sum_range_succ, sub_self, add_zero, cwU_deg0, Category.comp_id,
    show -ip RD i ρ - 1 + (((m + 1 + ip RD i ρ).toNat : ℕ) : ℤ) = m by omega] at G
  rw [eq_neg_iff_add_eq_zero, add_comm]; exact G

/-- A fake clockwise bubble by the infinite Grassmannian relation in degree
`d = m + 1 - ⟨i, ρ⟩ ≥ 1`. -/
theorem dc_cwU_fake {ρ : X} {i : I} {m : ℤ} (_h1 : m < 0) (h2 : 0 < m + 1 - ip RD i ρ) :
    cwU RD k ρ i m = -∑ j ∈ Finset.range (m + 1 - ip RD i ρ).toNat,
      ccwU RD k ρ i (-ip RD i ρ - 1 + ((j + 1 : ℕ) : ℤ)) ≫
        cwU RD k ρ i (ip RD i ρ - 1 + (((m + 1 - ip RD i ρ).toNat : ℕ) - ((j + 1 : ℕ) : ℤ))) := by
  have G := grassmannian_all RD k ρ i (m + 1 - ip RD i ρ).toNat
  rw [ite_eq_right (by omega), Finset.sum_range_succ', Nat.cast_zero, add_zero, ccwU_deg0,
    Category.id_comp, sub_zero,
    show ip RD i ρ - 1 + (((m + 1 - ip RD i ρ).toNat : ℕ) : ℤ) = m by omega] at G
  rw [eq_neg_iff_add_eq_zero, add_comm]; exact G

theorem dc_cwU_zero {ν : X} {i : I} {m : ℤ} (h : m + 1 - ip RD i ν < 0) : cwU RD k ν i m = 0 :=
  cwU_eq_zero RD k ν i m h
theorem dc_ccwU_zero {ν : X} {i : I} {m : ℤ} (h : m + 1 + ip RD i ν < 0) : ccwU RD k ν i m = 0 :=
  ccwU_eq_zero RD k ν i m h

end

macro "dc_bub" h:ident : tactic => `(tactic| (
  simp only [$h:ident, Finset.sum_range_succ, Finset.sum_range_zero, zero_add, add_zero, Int.reduceNeg,
    Int.reduceAdd, Int.reduceSub, Int.reduceToNat, Nat.cast_zero, Nat.cast_one, Nat.cast_ofNat,
    Nat.reduceSub, Nat.reduceAdd, Int.reduceMul, ip_wt_up, dc_ip_wt_dn, A_self]
  try simp (disch := first | omega | (simp only [ip_wt_up, dc_ip_wt_dn, A_self, $h:ident]; omega))
    only [dc_cwU_one, dc_ccwU_one, dc_cwU_real, dc_ccwU_real, dc_cwU_zero, dc_ccwU_zero,
      dc_ccwU_fake, dc_cwU_fake, $h:ident, ip_wt_up, dc_ip_wt_dn, A_self, Finset.sum_range_succ,
      Finset.sum_range_zero, zero_add, add_zero, Int.reduceNeg, Int.reduceAdd, Int.reduceSub,
      Int.reduceToNat, Nat.cast_zero, Nat.cast_one, Nat.cast_ofNat, Nat.reduceSub, Nat.reduceAdd,
      Int.reduceMul, neg_add_rev, Preadditive.add_comp, Preadditive.comp_add, Preadditive.neg_comp,
      Preadditive.comp_neg, neg_neg]
  iterate 3 (
    try simp only [bubRU_id, bubLU_id, bubRU_zero, bubLU_zero, dotsU_zero, Category.comp_id,
      Category.id_comp, Limits.zero_comp, Limits.comp_zero, dc_bubRU_neg, dc_bubRU_add,
      dc_bubRU_zsmul, dc_bubLU_neg, dc_bubLU_add, dc_bubLU_zsmul, neg_one_zsmul, neg_zsmul, one_zsmul, neg_one_smul, neg_neg, dc_bubRU_dg, dc_bubLU_dg,
      cwLs, ccwLs, dotCapEFLs, cupDotEFLs, dotCapFELs, cupDotFELs,
      List.replicate_succ, List.replicate_zero, List.singleton_append, List.cons_append,
      List.nil_append, List.append_nil, List.map_cons, List.map_nil, whL, neg_smul, one_smul,
      Preadditive.add_comp, Preadditive.comp_add, Preadditive.neg_comp, Preadditive.comp_neg,
      Preadditive.zsmul_comp, Preadditive.comp_zsmul, dc_end_neg_comp, dc_end_comp_neg,
      dc_end_add_comp, dc_end_comp_add, dotsU, dg_nil]
    repeat rw [dg_comp (by schain) (by schain)]
  )
  try simp only [dg_nil, Category.comp_id, Category.id_comp]
  try dnorm
  try abel))

end Categorification.KL3.Diagram
