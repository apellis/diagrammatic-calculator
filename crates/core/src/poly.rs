//! Sparse multivariate polynomials over a coefficient ring.
//!
//! One type covers commutative polynomials, Laurent polynomials (negative exponents allowed in
//! chosen variables) and skew polynomials, in which distinct odd variables anticommute
//! (`x_i x_j = -x_j x_i` for `i ≠ j`, while `x_i²` is nonzero). Skew polynomials are what the
//! odd nilHecke algebra acts on.
//!
//! A polynomial is a list of (monomial, coefficient) pairs sorted by the monomial order, with no
//! zero coefficients, so equal polynomials have equal representations. Monomials in an odd
//! context are normal-ordered words `x_1^{a_1} ⋯ x_n^{a_n}`.

use crate::ring::Ring;
use std::cmp::Ordering;

/// Exponent vector of a monomial; its length is the number of variables of the ring.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Monomial(pub Vec<i32>);

impl Monomial {
    pub fn one(n: usize) -> Self {
        Monomial(vec![0; n])
    }
    pub fn var(n: usize, i: usize, e: i32) -> Self {
        let mut v = vec![0; n];
        v[i] = e;
        Monomial(v)
    }
    pub fn degree(&self) -> i64 {
        self.0.iter().map(|&e| e as i64).sum()
    }
    pub fn is_one(&self) -> bool {
        self.0.iter().all(|&e| e == 0)
    }
}

impl PartialOrd for Monomial {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Graded lexicographic order with `x_1 > x_2 > ⋯`.
impl Ord for Monomial {
    fn cmp(&self, other: &Self) -> Ordering {
        self.degree()
            .cmp(&other.degree())
            .then_with(|| self.0.cmp(&other.0))
    }
}

/// A polynomial: terms sorted by decreasing monomial, no zero coefficients.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Poly<E> {
    pub terms: Vec<(Monomial, E)>,
}

/// A polynomial ring over `base` in named variables, each even or odd, each polynomial or Laurent.
#[derive(Clone, Debug, PartialEq)]
pub struct PolyRing<R: Ring> {
    pub base: R,
    pub names: Vec<String>,
    /// Odd variables anticommute with each other.
    pub odd: Vec<bool>,
    /// Laurent variables may carry negative exponents.
    pub laurent: Vec<bool>,
}

impl<R: Ring> PolyRing<R> {
    /// Commutative polynomial ring in the given variables.
    pub fn new(base: R, names: &[&str]) -> Self {
        let n = names.len();
        PolyRing {
            base,
            names: names.iter().map(|s| s.to_string()).collect(),
            odd: vec![false; n],
            laurent: vec![false; n],
        }
    }

    /// Skew polynomial ring: every variable odd.
    pub fn skew(base: R, names: &[&str]) -> Self {
        let mut r = Self::new(base, names);
        r.odd = vec![true; names.len()];
        r
    }

    pub fn nvars(&self) -> usize {
        self.names.len()
    }

    pub fn is_commutative(&self) -> bool {
        self.odd.iter().filter(|&&o| o).count() < 2
    }

    pub fn var(&self, i: usize) -> Poly<R::Elem> {
        self.monomial(Monomial::var(self.nvars(), i, 1), self.base.one())
    }

    pub fn var_named(&self, name: &str) -> Option<Poly<R::Elem>> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|i| self.var(i))
    }

    pub fn constant(&self, c: R::Elem) -> Poly<R::Elem> {
        self.monomial(Monomial::one(self.nvars()), c)
    }

    pub fn monomial(&self, m: Monomial, c: R::Elem) -> Poly<R::Elem> {
        debug_assert_eq!(m.0.len(), self.nvars());
        if self.base.is_zero(&c) {
            Poly { terms: vec![] }
        } else {
            Poly {
                terms: vec![(m, c)],
            }
        }
    }

    /// Sign of `m1 · m2` when both are normal-ordered: each odd `x_j` of `m2` passes every odd
    /// `x_i` of `m1` with `i > j`.
    pub fn monomial_mul_sign(&self, m1: &Monomial, m2: &Monomial) -> bool {
        let mut parity = 0i64;
        // suffix count of odd exponents of m1 at indices > j
        let mut suffix = 0i64;
        for j in (0..self.nvars()).rev() {
            if self.odd[j] {
                parity += (m2.0[j] as i64) * suffix;
                suffix += m1.0[j] as i64;
            }
        }
        parity.rem_euclid(2) == 1
    }

    /// Collects unsorted terms into canonical form.
    pub fn normalize(&self, mut terms: Vec<(Monomial, R::Elem)>) -> Poly<R::Elem> {
        terms.sort_by(|a, b| b.0.cmp(&a.0));
        let mut out: Vec<(Monomial, R::Elem)> = Vec::with_capacity(terms.len());
        for (m, c) in terms {
            match out.last_mut() {
                Some((lm, lc)) if *lm == m => self.base.add_assign(lc, &c),
                _ => out.push((m, c)),
            }
        }
        out.retain(|(_, c)| !self.base.is_zero(c));
        Poly { terms: out }
    }

    /// Multiplies every coefficient by a scalar.
    pub fn scale(&self, a: &Poly<R::Elem>, c: &R::Elem) -> Poly<R::Elem> {
        if self.base.is_zero(c) {
            return Poly { terms: vec![] };
        }
        let terms = a
            .terms
            .iter()
            .map(|(m, x)| (m.clone(), self.base.mul(x, c)))
            .filter(|(_, x)| !self.base.is_zero(x))
            .collect();
        Poly { terms }
    }

    /// Applies a map on monomials (with a sign) to every term: used for variable permutations and
    /// for the odd symmetric-group action.
    pub fn map_monomials(
        &self,
        a: &Poly<R::Elem>,
        mut f: impl FnMut(&Monomial) -> (Monomial, bool),
    ) -> Poly<R::Elem> {
        let terms = a
            .terms
            .iter()
            .map(|(m, c)| {
                let (m2, neg) = f(m);
                (m2, if neg { self.base.neg(c) } else { c.clone() })
            })
            .collect();
        self.normalize(terms)
    }

    /// Commutative case: permutes variables, sending variable `i` to variable `perm[i]`.
    pub fn permute_vars(&self, a: &Poly<R::Elem>, perm: &[usize]) -> Poly<R::Elem> {
        assert!(
            self.is_commutative(),
            "permute_vars needs commuting variables"
        );
        let n = self.nvars();
        self.map_monomials(a, |m| {
            let mut v = vec![0; n];
            for (i, &e) in m.0.iter().enumerate() {
                v[perm[i]] += e;
            }
            (Monomial(v), false)
        })
    }

    /// Commutative case: swaps variables `i` and `j`.
    pub fn swap_vars(&self, a: &Poly<R::Elem>, i: usize, j: usize) -> Poly<R::Elem> {
        let mut perm: Vec<usize> = (0..self.nvars()).collect();
        perm.swap(i, j);
        self.permute_vars(a, &perm)
    }

    /// Commutative case: `a` divided by `x_i - x_j` if the division is exact.
    ///
    /// Division by `x_i - x_j` is monic in `x_i`, so it is exact over any coefficient ring
    /// exactly when `a` vanishes at `x_i = x_j`.
    pub fn div_by_difference(
        &self,
        a: &Poly<R::Elem>,
        i: usize,
        j: usize,
    ) -> Option<Poly<R::Elem>> {
        assert!(self.is_commutative() && i != j);
        assert!(!self.laurent[i] || a.terms.iter().all(|(m, _)| m.0[i] >= 0));
        // Write a = Σ_k c_k(rest) x_i^k. Then a = (x_i - x_j) q + a(x_i := x_j), and
        // q = Σ_k c_k Σ_{l=0}^{k-1} x_i^l x_j^{k-1-l}.
        let mut q_terms = Vec::new();
        let mut rem_terms = Vec::new();
        for (m, c) in &a.terms {
            let k = m.0[i];
            let mut base = m.clone();
            base.0[i] = 0;
            for l in 0..k {
                let mut t = base.clone();
                t.0[i] = l;
                t.0[j] += k - 1 - l;
                q_terms.push((t, c.clone()));
            }
            let mut r = base.clone();
            r.0[j] += k;
            rem_terms.push((r, c.clone()));
        }
        let rem = self.normalize(rem_terms);
        rem.terms.is_empty().then(|| self.normalize(q_terms))
    }

    /// Substitutes `x_i := x_j` (commutative case).
    pub fn identify_vars(&self, a: &Poly<R::Elem>, i: usize, j: usize) -> Poly<R::Elem> {
        self.map_monomials(a, |m| {
            let mut v = m.clone();
            v.0[j] += v.0[i];
            v.0[i] = 0;
            (v, false)
        })
    }

    pub fn is_constant(&self, a: &Poly<R::Elem>) -> bool {
        a.terms.iter().all(|(m, _)| m.is_one())
    }

    /// The coefficient of the constant monomial.
    pub fn constant_term(&self, a: &Poly<R::Elem>) -> R::Elem {
        a.terms
            .iter()
            .find(|(m, _)| m.is_one())
            .map(|(_, c)| c.clone())
            .unwrap_or_else(|| self.base.zero())
    }

    /// Total degree of the highest term, `None` for zero.
    pub fn degree(&self, a: &Poly<R::Elem>) -> Option<i64> {
        a.terms.first().map(|(m, _)| m.degree())
    }

    /// Text form of one monomial without coefficient: `x_1^2 x_2`, or `1`.
    pub fn fmt_monomial(&self, m: &Monomial) -> String {
        let parts: Vec<String> =
            m.0.iter()
                .enumerate()
                .filter(|&(_, &e)| e != 0)
                .map(|(i, &e)| {
                    if e == 1 {
                        self.names[i].clone()
                    } else {
                        format!("{}^{}", self.names[i], e)
                    }
                })
                .collect();
        if parts.is_empty() {
            "1".to_string()
        } else {
            parts.join(" ")
        }
    }

    /// Writes `c · m` with sign handling; `first` controls the leading `-` versus ` - `.
    fn fmt_term(&self, m: &Monomial, c: &R::Elem, first: bool, out: &mut String) {
        let (neg, abs) = if self.base.is_negative(c) {
            (true, self.base.neg(c))
        } else {
            (false, c.clone())
        };
        if first {
            if neg {
                out.push('-');
            }
        } else {
            out.push_str(if neg { " - " } else { " + " });
        }
        let cs = self.base.fmt_elem(&abs);
        if m.is_one() {
            if self.base.needs_parens(&abs) && !first {
                out.push_str(&format!("({cs})"));
            } else {
                out.push_str(&cs);
            }
        } else {
            if !self.base.is_one(&abs) {
                if self.base.needs_parens(&abs) {
                    out.push_str(&format!("({cs}) "));
                } else {
                    out.push_str(&cs);
                    out.push(' ');
                }
            }
            out.push_str(&self.fmt_monomial(m));
        }
    }
}

impl<R: Ring> Ring for PolyRing<R> {
    type Elem = Poly<R::Elem>;

    fn zero(&self) -> Self::Elem {
        Poly { terms: vec![] }
    }
    fn one(&self) -> Self::Elem {
        self.constant(self.base.one())
    }
    fn int(&self, n: i64) -> Self::Elem {
        self.constant(self.base.int(n))
    }
    fn is_zero(&self, a: &Self::Elem) -> bool {
        a.terms.is_empty()
    }
    fn add(&self, a: &Self::Elem, b: &Self::Elem) -> Self::Elem {
        let (mut i, mut j) = (0, 0);
        let mut out = Vec::with_capacity(a.terms.len() + b.terms.len());
        while i < a.terms.len() && j < b.terms.len() {
            match a.terms[i].0.cmp(&b.terms[j].0) {
                Ordering::Greater => {
                    out.push(a.terms[i].clone());
                    i += 1;
                }
                Ordering::Less => {
                    out.push(b.terms[j].clone());
                    j += 1;
                }
                Ordering::Equal => {
                    let c = self.base.add(&a.terms[i].1, &b.terms[j].1);
                    if !self.base.is_zero(&c) {
                        out.push((a.terms[i].0.clone(), c));
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        out.extend_from_slice(&a.terms[i..]);
        out.extend_from_slice(&b.terms[j..]);
        Poly { terms: out }
    }
    fn neg(&self, a: &Self::Elem) -> Self::Elem {
        Poly {
            terms: a
                .terms
                .iter()
                .map(|(m, c)| (m.clone(), self.base.neg(c)))
                .collect(),
        }
    }
    fn mul(&self, a: &Self::Elem, b: &Self::Elem) -> Self::Elem {
        let skew = !self.is_commutative();
        let mut terms = Vec::with_capacity(a.terms.len() * b.terms.len());
        for (ma, ca) in &a.terms {
            for (mb, cb) in &b.terms {
                let m = Monomial(ma.0.iter().zip(&mb.0).map(|(x, y)| x + y).collect());
                let mut c = self.base.mul(ca, cb);
                if skew && self.monomial_mul_sign(ma, mb) {
                    c = self.base.neg(&c);
                }
                terms.push((m, c));
            }
        }
        self.normalize(terms)
    }
    fn inv(&self, b: &Self::Elem) -> Option<Self::Elem> {
        // Units: unit constants, and unit multiples of monomials in Laurent variables.
        if b.terms.len() != 1 {
            return None;
        }
        let (m, c) = &b.terms[0];
        let ci = self.base.inv(c)?;
        if m.0
            .iter()
            .enumerate()
            .any(|(i, &e)| e != 0 && !self.laurent[i])
        {
            return None;
        }
        let mi = Monomial(m.0.iter().map(|e| -e).collect());
        let mut r = self.monomial(mi, ci);
        if !self.is_commutative() && self.monomial_mul_sign(m, &r.terms[0].0) {
            r = self.neg(&r);
        }
        Some(r)
    }
    fn div_exact(&self, a: &Self::Elem, b: &Self::Elem) -> Option<Self::Elem> {
        if let Some(bi) = self.inv(b) {
            return Some(self.mul(a, &bi));
        }
        if self.is_zero(a) {
            return Some(self.zero());
        }
        if !self.is_commutative() || self.laurent.iter().any(|&l| l) {
            return None;
        }
        // Multivariate division by the leading term; exact iff the remainder vanishes.
        let (lm, lc) = b.terms.first()?.clone();
        let mut rem = a.clone();
        let mut q = self.zero();
        while let Some((m, c)) = rem.terms.first().cloned() {
            if m.0.iter().zip(&lm.0).any(|(x, y)| x < y) {
                return None;
            }
            let qc = self.base.div_exact(&c, &lc)?;
            let qm = Monomial(m.0.iter().zip(&lm.0).map(|(x, y)| x - y).collect());
            let t = self.monomial(qm, qc);
            rem = self.sub(&rem, &self.mul(&t, b));
            q = self.add(&q, &t);
        }
        Some(q)
    }
    fn fmt_elem(&self, a: &Self::Elem) -> String {
        if a.terms.is_empty() {
            return "0".to_string();
        }
        let mut s = String::new();
        for (k, (m, c)) in a.terms.iter().enumerate() {
            self.fmt_term(m, c, k == 0, &mut s);
        }
        s
    }
    fn needs_parens(&self, a: &Self::Elem) -> bool {
        a.terms.len() > 1
            || a.terms
                .first()
                .is_some_and(|(m, c)| m.is_one() && self.base.needs_parens(c))
    }
    fn characteristic(&self) -> u64 {
        self.base.characteristic()
    }
    fn is_negative(&self, a: &Self::Elem) -> bool {
        a.terms.len() == 1 && self.base.is_negative(&a.terms[0].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring::{Integers, PrimeField};

    #[test]
    fn arithmetic_and_printing() {
        let r = PolyRing::new(Integers, &["x_1", "x_2"]);
        let (x, y) = (r.var(0), r.var(1));
        let s = r.add(&x, &y);
        let d = r.sub(&x, &y);
        let p = r.mul(&s, &d);
        assert_eq!(r.fmt_elem(&p), "x_1^2 - x_2^2");
        assert_eq!(
            r.fmt_elem(&r.neg(&r.mul(&s, &s))),
            "-x_1^2 - 2 x_1 x_2 - x_2^2"
        );
        assert_eq!(r.div_by_difference(&p, 0, 1), Some(s.clone()));
        assert_eq!(r.div_by_difference(&s, 0, 1), None);
        assert_eq!(r.div_exact(&p, &d), Some(s));
    }

    #[test]
    fn skew_signs() {
        let r = PolyRing::skew(Integers, &["x_1", "x_2", "x_3"]);
        let (x1, x2, x3) = (r.var(0), r.var(1), r.var(2));
        // x_2 x_1 = -x_1 x_2
        assert_eq!(r.mul(&x2, &x1), r.neg(&r.mul(&x1, &x2)));
        // x_1^2 commutes with x_2: an even power of an odd variable passes with sign +.
        let x1sq = r.mul(&x1, &x1);
        assert_eq!(r.mul(&x2, &x1sq), r.mul(&x1sq, &x2));
        // associativity on a sample
        let a = r.add(&x1, &r.mul(&x3, &x2));
        let b = r.add(&x2, &x3);
        let c = r.sub(&x1, &r.mul(&x2, &x2));
        assert_eq!(r.mul(&r.mul(&a, &b), &c), r.mul(&a, &r.mul(&b, &c)));
    }

    #[test]
    fn laurent_inverse_and_prime_field() {
        let f = PrimeField::new(7).unwrap();
        let mut r = PolyRing::new(f, &["q"]);
        r.laurent[0] = true;
        let q = r.var(0);
        let qi = r.inv(&q).unwrap();
        assert_eq!(r.mul(&q, &qi), r.one());
        assert_eq!(r.fmt_elem(&r.scale(&qi, &6)), "6 q^-1");
    }
}
