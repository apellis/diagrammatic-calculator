//! Rational functions in one variable over a field, such as ℚ(q).
//!
//! An element is `num / den` with `num` and `den` dense univariate polynomials (coefficients from
//! the constant term up), `den` monic and coprime to `num`, so equal values have equal
//! representations.

use crate::poly::{Monomial, Poly, PolyRing};
use crate::ring::Ring;
use num_bigint::BigInt;

/// The field `F(v)` of rational functions in one variable `v` over a field `F`.
#[derive(Clone, Debug, PartialEq)]
pub struct RationalFunctions<F: Ring> {
    pub field: F,
    /// `F[v]`, used for printing and for naming `v`.
    pub poly: PolyRing<F>,
}

/// `num / den`, coefficients from degree 0 up, no trailing zeros, `den` monic.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RatFun<E> {
    pub num: Vec<E>,
    pub den: Vec<E>,
}

impl<F: Ring> RationalFunctions<F> {
    /// `F(v)`; `F` must be a field.
    pub fn new(field: F, var: &str) -> Self {
        assert!(field.is_field(), "rational functions need a field");
        let poly = PolyRing::new(field.clone(), &[var]);
        RationalFunctions { field, poly }
    }

    pub fn var_name(&self) -> &str {
        &self.poly.names[0]
    }

    fn trim(&self, mut a: Vec<F::Elem>) -> Vec<F::Elem> {
        while a.last().is_some_and(|c| self.field.is_zero(c)) {
            a.pop();
        }
        a
    }

    fn p_add(&self, a: &[F::Elem], b: &[F::Elem]) -> Vec<F::Elem> {
        let n = a.len().max(b.len());
        let z = self.field.zero();
        let out = (0..n)
            .map(|k| {
                self.field
                    .add(a.get(k).unwrap_or(&z), b.get(k).unwrap_or(&z))
            })
            .collect();
        self.trim(out)
    }

    fn p_neg(&self, a: &[F::Elem]) -> Vec<F::Elem> {
        a.iter().map(|c| self.field.neg(c)).collect()
    }

    fn p_mul(&self, a: &[F::Elem], b: &[F::Elem]) -> Vec<F::Elem> {
        if a.is_empty() || b.is_empty() {
            return vec![];
        }
        let mut out = vec![self.field.zero(); a.len() + b.len() - 1];
        for (i, x) in a.iter().enumerate() {
            if self.field.is_zero(x) {
                continue;
            }
            for (j, y) in b.iter().enumerate() {
                let t = self.field.mul(x, y);
                self.field.add_assign(&mut out[i + j], &t);
            }
        }
        self.trim(out)
    }

    fn p_scale(&self, a: &[F::Elem], c: &F::Elem) -> Vec<F::Elem> {
        self.trim(a.iter().map(|x| self.field.mul(x, c)).collect())
    }

    /// Quotient and remainder; `b ≠ 0`.
    fn p_divrem(&self, a: &[F::Elem], b: &[F::Elem]) -> (Vec<F::Elem>, Vec<F::Elem>) {
        let lead_inv = self.field.inv(b.last().unwrap()).expect("field");
        let mut r = a.to_vec();
        if r.len() < b.len() {
            return (vec![], r);
        }
        let mut q = vec![self.field.zero(); r.len() - b.len() + 1];
        while r.len() >= b.len() {
            let shift = r.len() - b.len();
            let c = self.field.mul(r.last().unwrap(), &lead_inv);
            for (k, y) in b.iter().enumerate() {
                let t = self.field.mul(&c, y);
                r[shift + k] = self.field.sub(&r[shift + k], &t);
            }
            q[shift] = c;
            r = self.trim(r);
        }
        (self.trim(q), r)
    }

    fn monic(&self, a: &[F::Elem]) -> Vec<F::Elem> {
        match a.last() {
            None => vec![],
            Some(l) => self.p_scale(a, &self.field.inv(l).expect("field")),
        }
    }

    /// Monic greatest common divisor.
    fn p_gcd(&self, a: &[F::Elem], b: &[F::Elem]) -> Vec<F::Elem> {
        let (mut x, mut y) = (a.to_vec(), b.to_vec());
        while !y.is_empty() {
            let r = self.p_divrem(&x, &y).1;
            x = y;
            y = r;
        }
        self.monic(&x)
    }

    /// `n / d` in canonical form; `d ≠ 0`.
    pub fn frac(&self, n: Vec<F::Elem>, d: Vec<F::Elem>) -> RatFun<F::Elem> {
        let (n, d) = (self.trim(n), self.trim(d));
        assert!(!d.is_empty(), "zero denominator");
        if n.is_empty() {
            return self.zero();
        }
        let is_one = |g: &[F::Elem]| g.len() == 1 && self.field.is_one(&g[0]);
        let g = if d.len() == 1 {
            vec![self.field.one()]
        } else {
            self.p_gcd(&n, &d)
        };
        let (n, d) = if is_one(&g) {
            (n, d)
        } else {
            (self.p_divrem(&n, &g).0, self.p_divrem(&d, &g).0)
        };
        let li = self.field.inv(d.last().unwrap()).expect("field");
        if self.field.is_one(&li) {
            RatFun { num: n, den: d }
        } else {
            RatFun {
                num: self.p_scale(&n, &li),
                den: self.p_scale(&d, &li),
            }
        }
    }

    /// The polynomial `Σ c_k v^k` as a sparse polynomial of `F[v]`.
    fn sparse(&self, a: &[F::Elem]) -> Poly<F::Elem> {
        let terms = a
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, c)| !self.field.is_zero(c))
            .map(|(k, c)| (Monomial::from(vec![k as i32]), c.clone()))
            .collect();
        Poly { terms }
    }

    fn fmt_poly(&self, a: &[F::Elem]) -> (String, bool) {
        let p = self.sparse(a);
        let s = self.poly.fmt_elem(&p);
        let paren = self.poly.needs_parens(&p) || self.poly.is_negative(&p);
        (s, paren)
    }
}

impl<F: Ring> Ring for RationalFunctions<F> {
    type Elem = RatFun<F::Elem>;

    fn zero(&self) -> Self::Elem {
        RatFun {
            num: vec![],
            den: vec![self.field.one()],
        }
    }
    fn one(&self) -> Self::Elem {
        self.int(1)
    }
    fn int(&self, n: i64) -> Self::Elem {
        RatFun {
            num: self.trim(vec![self.field.int(n)]),
            den: vec![self.field.one()],
        }
    }
    fn is_zero(&self, a: &Self::Elem) -> bool {
        a.num.is_empty()
    }
    fn add(&self, a: &Self::Elem, b: &Self::Elem) -> Self::Elem {
        if a.den == b.den {
            return self.frac(self.p_add(&a.num, &b.num), a.den.clone());
        }
        self.frac(
            self.p_add(&self.p_mul(&a.num, &b.den), &self.p_mul(&b.num, &a.den)),
            self.p_mul(&a.den, &b.den),
        )
    }
    fn neg(&self, a: &Self::Elem) -> Self::Elem {
        RatFun {
            num: self.p_neg(&a.num),
            den: a.den.clone(),
        }
    }
    fn mul(&self, a: &Self::Elem, b: &Self::Elem) -> Self::Elem {
        if a.den.len() == 1 && b.den.len() == 1 {
            return RatFun {
                num: self.p_mul(&a.num, &b.num),
                den: a.den.clone(),
            };
        }
        self.frac(self.p_mul(&a.num, &b.num), self.p_mul(&a.den, &b.den))
    }
    fn inv(&self, b: &Self::Elem) -> Option<Self::Elem> {
        (!b.num.is_empty()).then(|| self.frac(b.den.clone(), b.num.clone()))
    }
    fn div_exact(&self, a: &Self::Elem, b: &Self::Elem) -> Option<Self::Elem> {
        self.inv(b).map(|bi| self.mul(a, &bi))
    }
    fn fmt_elem(&self, a: &Self::Elem) -> String {
        let (n, np) = self.fmt_poly(&a.num);
        if a.den.len() == 1 {
            return n;
        }
        let (d, dp) = self.fmt_poly(&a.den);
        let n = if np && a.num.len() > 1 {
            format!("({n})")
        } else {
            n
        };
        let d = if dp { format!("({d})") } else { d };
        format!("{n}/{d}")
    }
    /// A quotient prints as a product (`(q + 1)/q^2` is `(q + 1) ∘ (q^2)^-1`), so only a sum
    /// needs brackets.
    fn needs_parens(&self, a: &Self::Elem) -> bool {
        a.den.len() == 1 && self.poly.needs_parens(&self.sparse(&a.num))
    }
    fn characteristic(&self) -> u64 {
        self.field.characteristic()
    }
    fn rational(&self, n: &BigInt, d: &BigInt) -> Option<Self::Elem> {
        let c = self.field.rational(n, d)?;
        Some(RatFun {
            num: self.trim(vec![c]),
            den: vec![self.field.one()],
        })
    }
    fn named(&self, name: &str) -> Option<Self::Elem> {
        (name == self.var_name()).then(|| RatFun {
            num: vec![self.field.zero(), self.field.one()],
            den: vec![self.field.one()],
        })
    }
    fn is_field(&self) -> bool {
        true
    }
    fn is_negative(&self, a: &Self::Elem) -> bool {
        self.poly.is_negative(&self.sparse(&a.num))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring::{PrimeField, Rationals};

    #[test]
    fn arithmetic_and_printing() {
        let k = RationalFunctions::new(Rationals, "q");
        let q = k.named("q").unwrap();
        let one = k.one();
        let qp1 = k.add(&q, &one);
        let qm1 = k.sub(&q, &one);
        // (q^2 - 1) / (q - 1) = q + 1
        let a = k.div_exact(&k.mul(&qp1, &qm1), &qm1).unwrap();
        assert_eq!(a, qp1);
        assert_eq!(k.fmt_elem(&a), "q + 1");
        let b = k.inv(&qp1).unwrap();
        assert_eq!(k.fmt_elem(&b), "1/(q + 1)");
        assert_eq!(k.fmt_elem(&k.inv(&q).unwrap()), "1/q");
        let q2 = k.mul(&q, &q);
        assert_eq!(k.fmt_elem(&k.div_exact(&qp1, &q2).unwrap()), "(q + 1)/q^2");
        let half = k.rational(&BigInt::from(1), &BigInt::from(2)).unwrap();
        // 1/(2q + 2) is normalized with a monic denominator
        let c = k.inv(&k.add(&k.mul(&k.int(2), &q), &k.int(2))).unwrap();
        assert_eq!(c, k.mul(&half, &b));
        assert_eq!(k.fmt_elem(&c), "1/2/(q + 1)");
        assert!(k.is_zero(&k.sub(&k.add(&b, &k.neg(&b)), &k.zero())));
    }

    #[test]
    fn over_a_prime_field() {
        let k = RationalFunctions::new(PrimeField::new(3).unwrap(), "t");
        let t = k.named("t").unwrap();
        // (t^3 - 1) = (t - 1)^3 in characteristic 3
        let tm1 = k.sub(&t, &k.one());
        let cube = k.pow(&tm1, 3);
        assert_eq!(k.sub(&k.pow(&t, 3), &k.one()), cube);
        assert_eq!(k.div_exact(&cube, &k.mul(&tm1, &tm1)).unwrap(), tm1);
    }
}
