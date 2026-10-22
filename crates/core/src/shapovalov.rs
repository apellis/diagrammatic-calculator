//! The q-Shapovalov form on an integrable highest-weight module V(Λ), computed from the quantum
//! group alone, as an independent check on cyclotomic KLR algebras.
//!
//! Conventions: `q_i = q^{d_i}`, `[n]_i = (q_i^n − q_i^{−n})/(q_i − q_i^{−1})`, and on V(Λ) with
//! highest-weight vector `v_Λ`
//!
//! `E_i v_Λ = 0`, `E_i F_j − F_j E_i = δ_ij (K_i − K_i^{−1})/(q_i − q_i^{−1})`, `K_i v = q_i^{⟨h_i, μ⟩} v`
//!
//! for `v` of weight `μ`. The form is the symmetric bilinear form with `⟨v_Λ, v_Λ⟩ = 1` and
//! `⟨F_i u, w⟩ = ⟨u, E_i w⟩`. For a sequence `i = (i_1, …, i_m)` write
//! `F_i v_Λ = F_{i_1} ⋯ F_{i_m} v_Λ` (so `F_{i_m}` acts first). Then
//!
//! `E_a F_{b_1} ⋯ F_{b_m} v_Λ = Σ_{r : b_r = a} [⟨h_a, Λ − α_{b_{r+1}} − ⋯ − α_{b_m}⟩]_a F_{b_1} ⋯ F̂_{b_r} ⋯ F_{b_m} v_Λ`,
//!
//! which gives the form by recursion. Values are Laurent polynomials in `q` over ℤ.

use crate::cartan::Cartan;
use crate::num::Int;
use crate::poly::{Monomial, Poly, PolyRing};
use crate::ring::{Integers, Ring};
use std::collections::HashMap;

/// The ring ℤ[q, q⁻¹] the form takes values in.
pub fn laurent_ring() -> PolyRing<Integers> {
    let mut r = PolyRing::new(Integers, &["q"]);
    r.laurent[0] = true;
    r
}

/// The quantum integer `[n]_{q^d}`.
pub fn quantum_int(r: &PolyRing<Integers>, n: i64, d: i64) -> Poly<Int> {
    if n == 0 {
        return r.zero();
    }
    let sign = n.signum();
    let n = n.abs();
    // q^{d(n−1)} + q^{d(n−3)} + ⋯ + q^{−d(n−1)}
    let terms = (0..n)
        .map(|k| {
            (
                Monomial::from(vec![(d * (n - 1 - 2 * k)) as i32]),
                Int::from(sign),
            )
        })
        .collect();
    r.normalize(terms)
}

/// The Shapovalov form of V(Λ), with `lambda[i] = ⟨h_i, Λ⟩ ≥ 0`.
pub struct Shapovalov<'a> {
    cartan: &'a Cartan,
    lambda: Vec<i64>,
    ring: PolyRing<Integers>,
    memo: HashMap<(Vec<u16>, Vec<u16>), Poly<Int>>,
}

impl<'a> Shapovalov<'a> {
    pub fn new(cartan: &'a Cartan, lambda: &[i64]) -> Self {
        assert_eq!(lambda.len(), cartan.rank());
        Shapovalov {
            cartan,
            lambda: lambda.to_vec(),
            ring: laurent_ring(),
            memo: HashMap::new(),
        }
    }

    pub fn ring(&self) -> &PolyRing<Integers> {
        &self.ring
    }

    /// `⟨h_i, Λ − Σ_k α_{b_k}⟩`.
    fn pairing(&self, i: usize, b: &[u16]) -> i64 {
        self.lambda[i] - b.iter().map(|&j| self.cartan.a[i][j as usize]).sum::<i64>()
    }

    /// `⟨F_a v_Λ, F_b v_Λ⟩` for sequences of colour indices.
    pub fn form(&mut self, a: &[u16], b: &[u16]) -> Poly<Int> {
        if a.len() != b.len() {
            return self.ring.zero();
        }
        if a.is_empty() {
            return self.ring.one();
        }
        let key = (a.to_vec(), b.to_vec());
        if let Some(v) = self.memo.get(&key) {
            return v.clone();
        }
        // ⟨F_{a_1} F_{a'} v, w⟩ = ⟨F_{a'} v, E_{a_1} w⟩
        let x = a[0] as usize;
        let mut acc = self.ring.zero();
        for r in 0..b.len() {
            if b[r] as usize != x {
                continue;
            }
            let coeff = quantum_int(&self.ring, self.pairing(x, &b[r + 1..]), self.cartan.d[x]);
            if self.ring.is_zero(&coeff) {
                continue;
            }
            let mut rest = b.to_vec();
            rest.remove(r);
            let inner = self.form(&a[1..], &rest);
            acc = self.ring.add(&acc, &self.ring.mul(&coeff, &inner));
        }
        self.memo.insert(key, acc.clone());
        acc
    }

    /// The Gram matrix of the vectors `F_s v_Λ` for the given sequences.
    pub fn gram(&mut self, seqs: &[Vec<u16>]) -> Vec<Vec<Poly<Int>>> {
        seqs.iter()
            .map(|a| seqs.iter().map(|b| self.form(a, b)).collect())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(p: &Poly<Int>) -> String {
        laurent_ring().fmt_elem(p)
    }

    #[test]
    fn small_cases() {
        let a2 = Cartan::of_type("A2").unwrap();
        // V(Λ_1): F_2 F_1 v ≠ 0 with norm 1; F_1 F_2 v = 0
        let mut sh = Shapovalov::new(&a2, &[1, 0]);
        assert_eq!(s(&sh.form(&[1, 0], &[1, 0])), "1");
        assert_eq!(s(&sh.form(&[0, 1], &[0, 1])), "0");
        // sl2, V(mΛ): ⟨F^n v, F^n v⟩ = [n]! [m][m−1]⋯[m−n+1]
        let a1 = Cartan::of_type("A1").unwrap();
        let mut sh = Shapovalov::new(&a1, &[3]);
        assert_eq!(s(&sh.form(&[0], &[0])), "q^2 + 1 + q^-2");
        let f2 = sh.form(&[0, 0], &[0, 0]);
        // [2]! [3][2] = [2]^2 [3]
        let r = laurent_ring();
        let two = quantum_int(&r, 2, 1);
        let three = quantum_int(&r, 3, 1);
        assert_eq!(f2, r.mul(&r.mul(&two, &two), &three));
        assert!(r.is_zero(&sh.form(&[0, 0, 0, 0], &[0, 0, 0, 0])));
    }

    #[test]
    fn symmetric() {
        let b2 = Cartan::of_type("B2").unwrap();
        let mut sh = Shapovalov::new(&b2, &[1, 1]);
        let seqs: Vec<Vec<u16>> = vec![vec![0, 1, 1], vec![1, 0, 1], vec![1, 1, 0]];
        let g = sh.gram(&seqs);
        for (i, row) in g.iter().enumerate() {
            for (j, x) in row.iter().enumerate() {
                assert_eq!(*x, g[j][i]);
            }
        }
    }
}
