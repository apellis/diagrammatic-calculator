/-
Support for the exported chains of the super Kac–Moody 2-category (hand-written): positional
tactics for odd-lean's normal-form diagrams `cl D Sc μ s t L`, after categorification-lean's
`dnorm`, `dswap` and `schain` for the even calculus, with the Koszul sign of each interchange,
and the simp sets that evaluate the signs `zsign k p` once the parities of the colours are known.
-/
import OddMath.SKM.Rewriting
import OddMath.SKM.RelationsNF
import OddMath.SKM.Lemma33

noncomputable section

namespace OddMath.SKM

open CategoryTheory StringDiagrams Presentation

universe w u v

section

variable {k : Type w} [CommRing k]

@[simp] theorem dc_zsign_zero : zsign k 0 = 1 := by simp [zsign]

@[simp] theorem dc_zsign_one : zsign k 1 = -1 := by simp [zsign]

theorem dc_zsign_mul_self (p : ZMod 2) : zsign k p * zsign k p = 1 := by
  have hz : ∀ a : ZMod 2, a = 0 ∨ a = 1 := by decide
  rcases hz p with rfl | rfl <;> simp

/-- A step of a goal `a = r` through `a = z • b` with `z² = 1`: it remains to show `b = z • r`. -/
theorem dc_swap_goal {M : Type*} [AddCommGroup M] [Module k M] {a b r : M} (z : k)
    (hz : z * z = 1) (h : a = z • b) (g : b = z • r) : a = r := by
  rw [h, g, smul_smul, hz, one_smul]

theorem dc_zmod_two : (2 : ZMod 2) = 0 := rfl
theorem dc_zmod_three : (3 : ZMod 2) = 1 := rfl
theorem dc_zmod_four : (4 : ZMod 2) = 0 := rfl
theorem dc_zmod_five : (5 : ZMod 2) = 1 := rfl
theorem dc_zmod_six : (6 : ZMod 2) = 0 := rfl
theorem dc_zmod_one_add_one : (1 : ZMod 2) + 1 = 0 := rfl

end

section

variable {I : Type u} {X : Type v} [AddCommGroup X] {D : Datum I X}

@[simp] theorem dc_dom_dot (i : I) : (Shape.dot i).dom = [up i] := rfl
@[simp] theorem dc_dom_cross (i j : I) : (Shape.cross i j).dom = [up i, up j] := rfl
@[simp] theorem dc_dom_cup (i : I) : (Shape.cup i).dom = [] := rfl
@[simp] theorem dc_dom_cap (i : I) : (Shape.cap i).dom = [up i, dn i] := rfl
@[simp] theorem dc_dom_lcross (i j : I) : (Shape.lcross i j).dom = [dn i, up j] := rfl
@[simp] theorem dc_dom_dcup (i : I) (n : ℕ) : (Shape.dcup i n).dom = [] := rfl
@[simp] theorem dc_dom_dcap (i : I) (n : ℕ) : (Shape.dcap i n).dom = [dn i, up i] := rfl
@[simp] theorem dc_cod_dot (i : I) : (Shape.dot i).cod = [up i] := rfl
@[simp] theorem dc_cod_cross (i j : I) : (Shape.cross i j).cod = [up j, up i] := rfl
@[simp] theorem dc_cod_cup (i : I) : (Shape.cup i).cod = [dn i, up i] := rfl
@[simp] theorem dc_cod_cap (i : I) : (Shape.cap i).cod = [] := rfl
@[simp] theorem dc_cod_lcross (i j : I) : (Shape.lcross i j).cod = [up j, dn i] := rfl
@[simp] theorem dc_cod_dcup (i : I) (n : ℕ) : (Shape.dcup i n).cod = [up i, dn i] := rfl
@[simp] theorem dc_cod_dcap (i : I) (n : ℕ) : (Shape.dcap i n).cod = [] := rfl

theorem dc_par_dot (i : I) : (Shape.dot i).parity D = D.parity i := rfl
theorem dc_par_cross (i j : I) : (Shape.cross i j).parity D = D.parity i * D.parity j := rfl
theorem dc_par_cup (i : I) : (Shape.cup i).parity D = 0 := rfl
theorem dc_par_cap (i : I) : (Shape.cap i).parity D = 0 := rfl
theorem dc_par_lcross (i j : I) : (Shape.lcross i j).parity D = D.parity i * D.parity j := rfl
theorem dc_par_dcup (i : I) (n : ℕ) : (Shape.dcup i n).parity D = D.parity i * n := rfl
theorem dc_par_dcap (i : I) (n : ℕ) : (Shape.dcap i n).parity D = D.parity i * n := rfl

end

/-- Normalizes the lists of layers produced by `cswap` and the placement of a step. -/
macro "cnorm" : tactic => `(tactic| simp only [swapLR, swapRL, whL, crossL, dotsL, sigmaL,
  lcrossL, epsL, etaL, dcupL, dcapL, List.replicate_succ, List.replicate_zero,
  List.take_succ_cons, List.length_map, List.take_zero, List.take_nil, List.drop_succ_cons,
  List.drop_zero, List.drop_nil, List.length_cons, List.length_nil, List.length_append,
  dc_dom_dot, dc_dom_cross, dc_dom_cup, dc_dom_cap, dc_dom_lcross, dc_dom_dcup, dc_dom_dcap,
  dc_cod_dot, dc_cod_cross, dc_cod_cup, dc_cod_cap, dc_cod_lcross, dc_cod_dcup, dc_cod_dcap,
  List.map_cons, List.map_nil, List.cons_append, List.nil_append, List.append_nil,
  List.singleton_append, List.append_assoc, Nat.add_zero, Nat.zero_add, Nat.reduceAdd])

/-- Closes `SChain s ls t` for explicit lists of layers. -/
macro "cchain" : tactic => `(tactic| (
  try simp only [whL, List.map_append, List.map_cons, List.map_nil, List.append_assoc,
    List.cons_append, List.nil_append, List.singleton_append, List.append_nil]
  repeat' (first
    | exact rfl
    | refine ⟨rfl, ?_⟩)))

/-- Writes the parities of the generators in a goal as parities of colours. -/
macro "cpar" : tactic => `(tactic| simp only [dc_par_dot, dc_par_cross, dc_par_cup, dc_par_cap,
  dc_par_lcross, dc_par_dcup, dc_par_dcap])

/-- Evaluates the signs `zsign k p` of a goal once the parities of its colours are rewritten to
`0` and `1`, and collects scalar multiples. -/
macro "csign" : tactic => `(tactic| simp only [dc_par_dot, dc_par_cross, dc_par_cup, dc_par_cap,
  dc_par_lcross, dc_par_dcup, dc_par_dcap, dc_zsign_zero, dc_zsign_one, dc_zmod_two,
  dc_zmod_three, dc_zmod_four, dc_zmod_five, dc_zmod_six, dc_zmod_one_add_one, Nat.cast_zero,
  Nat.cast_one, Nat.cast_ofNat, mul_one, one_mul, mul_zero, zero_mul, add_zero, zero_add,
  mul_neg, neg_mul, neg_neg, one_smul, neg_one_smul, neg_smul, smul_neg, smul_add, smul_sub,
  zero_smul, smul_zero, Finset.sum_const_zero, Finset.sum_range_succ, Finset.sum_range_zero,
  Nat.reduceSub, Nat.reduceAdd, Nat.sub_zero, Nat.cast_id])

/-- `cswap n`: exchange the layers `n` and `n + 1` of the left-hand side `cl μ s t L` of the
goal `cl μ s t L = r` by the super interchange law; the goal becomes `cl μ s t L' = z • r` with
`z = zsign k (|g| |h|)` the Koszul sign. -/
macro "cswap " n:term:max : tactic => `(tactic| (
  first
    | refine dc_swap_goal _ (dc_zsign_mul_self _)
        (cl_swapLR_at _ $n _ _ _ (by exact rfl) (by exact rfl) (by exact rfl)) ?_
    | refine dc_swap_goal _ (dc_zsign_mul_self _)
        (cl_swapRL_at _ $n _ _ _ (by exact rfl) (by exact rfl) (by exact rfl)) ?_
  try cnorm))

end OddMath.SKM
