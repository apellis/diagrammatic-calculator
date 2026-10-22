//! The nilHecke and odd nilHecke algebras in their basis `Σ_w f_w ∂_w`.
//!
//! The algebra acts faithfully on (skew) polynomials, `x_k` by multiplication and `ψ_k` by the
//! divided difference `∂_k`, and has basis `x^a ∂_w` with `∂_w = ∂_{k_1} ⋯ ∂_{k_m}` for the
//! canonical reduced word of `w` (see [`Perm::reduced_word`]). Products follow
//!
//! `∂_k ∘ g = ∂_k(g) + s_k(g) ∘ ∂_k`,
//!
//! and `∂_k ∂_w` is `±∂_{s_k w}` when `ℓ(s_k w) > ℓ(w)` and zero otherwise.
//!
//! In the even case `s_k` swaps `x_k` and `x_{k+1}` and `∂_k f = (f − s_k f)/(x_k − x_{k+1})`.
//! In the odd case (Ellis–Khovanov–Lauda) the `x` anticommute, `s_k(x_j) = −x_{s_k(j)}`,
//! `∂_k(x_j) = 1` for `j ∈ {k, k+1}` and 0 otherwise, `∂_k(fg) = ∂_k(f) g + s_k(f) ∂_k(g)`, distant
//! `∂`s anticommute and braid moves carry no sign; [`odd_word_sign`] compares reduced words.

use super::{
    BasisShape, Evaluator, PolyVector, Provenance, basis_word, lift_coeff, permute_object,
    split_monomial, strand_ring,
};
use crate::context::Context;
use crate::elab::CTerm;
use crate::error::{Error, Result};
use crate::perm::{Perm, odd_word_sign};
use crate::poly::{Monomial, Poly, PolyRing};
use crate::ring::Ring;
use crate::term::{Object, Term, Word};
use std::collections::BTreeMap;

type Value<E> = BTreeMap<(Object, Perm), Poly<E>>;
type Valued<R> = (Alg<R>, Value<<R as Ring>::Elem>);

struct Alg<R: Ring> {
    n: usize,
    odd: bool,
    ring: PolyRing<R>,
}

impl<R: Ring> Alg<R> {
    fn s(&self, k: usize, f: &Poly<R::Elem>) -> Poly<R::Elem> {
        if !self.odd {
            return self.ring.swap_vars(f, k, k + 1);
        }
        let n = self.n;
        self.ring.map_monomials(f, |m| {
            let mut e = m.clone();
            e.0.swap(k, k + 1);
            let total: i64 = m.0[..n].iter().map(|&x| x as i64).sum();
            let sign = (total + m.0[k] as i64 * m.0[k + 1] as i64) % 2 == 1;
            (e, sign)
        })
    }

    fn d(&self, k: usize, f: &Poly<R::Elem>) -> Poly<R::Elem> {
        if !self.odd {
            let diff = self.ring.sub(f, &self.s(k, f));
            return self
                .ring
                .div_by_difference(&diff, k, k + 1)
                .expect("f − s_k f is divisible");
        }
        let mut acc = self.ring.zero();
        for (m, c) in &f.terms {
            let t = self.ring.scale(&self.d_mono(k, m), c);
            acc = self.ring.add(&acc, &t);
        }
        acc
    }

    /// Odd `∂_k` of a normal-ordered monomial, peeling off its first `x`.
    fn d_mono(&self, k: usize, m: &Monomial) -> Poly<R::Elem> {
        let Some(j) = (0..self.n).find(|&j| m.0[j] > 0) else {
            return self.ring.zero();
        };
        let mut rest = m.clone();
        rest.0[j] -= 1;
        let rest_poly = self.ring.monomial(rest.clone(), self.ring.base.one());
        let sj = if j == k {
            k + 1
        } else if j == k + 1 {
            k
        } else {
            j
        };
        // ∂_k(x_j · rest) = ∂_k(x_j) rest + s_k(x_j) ∂_k(rest), with s_k(x_j) = −x_{s_k(j)}
        let tail = self.ring.mul(&self.ring.var(sj), &self.d_mono(k, &rest));
        let head = if j == k || j == k + 1 {
            rest_poly
        } else {
            self.ring.zero()
        };
        self.ring.sub(&head, &tail)
    }

    /// `∂_k ∘ ∂_w` as `(sign, s_k w)`, or `None` when it is zero.
    fn d_on_basis(&self, k: usize, w: &Perm) -> Option<(bool, Perm)> {
        if w.has_left_descent(k) {
            return None;
        }
        let v = w.left_mul_simple(k);
        if !self.odd {
            return Some((false, v));
        }
        let mut word = vec![k];
        word.extend(w.reduced_word());
        Some((odd_word_sign(self.n, &word, &v.reduced_word()), v))
    }

    fn add_into(&self, val: &mut Value<R::Elem>, key: (Object, Perm), f: Poly<R::Elem>) {
        let s = match val.remove(&key) {
            Some(g) => self.ring.add(&g, &f),
            None => f,
        };
        if !self.ring.is_zero(&s) {
            val.insert(key, s);
        }
    }

    /// `∂_k ∘ x` for `x = Σ h_w ∂_w`.
    fn apply_d(&self, k: usize, x: &Value<R::Elem>) -> Value<R::Elem> {
        let mut out = Value::new();
        for ((i, w), h) in x {
            self.add_into(&mut out, (i.clone(), w.clone()), self.d(k, h));
            if let Some((neg, v)) = self.d_on_basis(k, w) {
                let sh = self.s(k, h);
                self.add_into(
                    &mut out,
                    (i.clone(), v),
                    if neg { self.ring.neg(&sh) } else { sh },
                );
            }
        }
        out
    }

    /// `top ∘ bottom`.
    fn mul(&self, top: &Value<R::Elem>, bottom: &Value<R::Elem>) -> Value<R::Elem> {
        let mut out = Value::new();
        for ((j, v), g) in bottom {
            let t = permute_object(j, v);
            for ((i, u), f) in top.range((t.clone(), Perm::identity(0))..) {
                if *i != t {
                    break;
                }
                let mut x = Value::new();
                x.insert((j.clone(), v.clone()), g.clone());
                for &k in u.reduced_word().iter().rev() {
                    x = self.apply_d(k, &x);
                }
                for (key, h) in x {
                    self.add_into(&mut out, key, self.ring.mul(f, &h));
                }
            }
        }
        out
    }
}

/// The (odd) nilHecke basis normal form as an equality strategy.
pub struct NilCoxeter<'a, R: Ring> {
    ctx: &'a Context<R>,
    dot: u16,
    crossing: u16,
    odd: bool,
    prov: Provenance,
}

impl<'a, R: Ring> NilCoxeter<'a, R> {
    pub fn new(ctx: &'a Context<R>, dot: u16, crossing: u16, odd: bool, prov: Provenance) -> Self {
        NilCoxeter {
            ctx,
            dot,
            crossing,
            odd,
            prov,
        }
    }

    fn shape(&self) -> BasisShape<'_, R> {
        BasisShape {
            ctx: self.ctx,
            dot: self.dot,
            crossing: self.crossing,
            dots_on_top: true,
        }
    }

    fn alg(&self, n: usize) -> Alg<R> {
        Alg {
            n,
            odd: self.odd,
            ring: strand_ring(self.ctx, n, self.odd),
        }
    }

    fn word_value(&self, alg: &Alg<R>, w: &Word) -> Result<Value<R::Elem>> {
        let n = alg.n;
        let mut val = Value::new();
        val.insert((w.source.clone(), Perm::identity(n)), alg.ring.one());
        let mut obj = w.source.clone();
        for l in &w.layers {
            let k = l.pos as usize;
            let mut g = Value::new();
            if l.generator == self.dot {
                g.insert((obj.clone(), Perm::identity(n)), alg.ring.var(k));
            } else if l.generator == self.crossing {
                if obj[k].colour != obj[k + 1].colour {
                    return Err(Error::precondition(
                        "the nilCoxeter basis needs strands of one colour",
                    ));
                }
                g.insert((obj.clone(), Perm::simple(n, k)), alg.ring.one());
            } else {
                let name = &self.ctx.pres.generators[l.generator as usize].name;
                return Err(Error::precondition(format!(
                    "the representation {} does not cover the generator {name}",
                    self.prov.method
                )));
            }
            val = alg.mul(&g, &val);
            obj = self.ctx.apply(l, &obj);
        }
        Ok(val)
    }

    fn value(&self, t: &CTerm<R>) -> Result<Option<Valued<R>>> {
        let Some(n) = t.terms.first().map(|(w, _)| w.source.len()) else {
            return Ok(None);
        };
        let alg = self.alg(n);
        let mut out = Value::new();
        for (w, c) in &t.terms {
            if w.source.len() != n {
                return Err(Error::precondition(
                    "the terms have different numbers of strands",
                ));
            }
            let c = lift_coeff(c, n);
            for (key, f) in self.word_value(&alg, w)? {
                alg.add_into(&mut out, key, alg.ring.mul(&c, &f));
            }
        }
        Ok(Some((alg, out)))
    }
}

impl<R: Ring> Evaluator<R> for NilCoxeter<'_, R> {
    fn provenance(&self) -> &Provenance {
        &self.prov
    }

    fn normal_form(&self, t: &CTerm<R>) -> Result<CTerm<R>> {
        let Some((alg, val)) = self.value(t)? else {
            return Ok(Term::zero());
        };
        let mut out = Term::zero();
        for ((i, w), f) in &val {
            let word = w.reduced_word();
            for (m, c) in &f.terms {
                let (exps, scal) = split_monomial(m, alg.n);
                let bw = basis_word(self.ctx, i, &word, &exps, self.dot, self.crossing, true)?;
                let coef = self.ctx.coeff.monomial(scal, c.clone());
                out = self.ctx.add(&out, &self.ctx.monomial_term(&bw, coef));
            }
        }
        Ok(out)
    }

    fn is_zero(&self, t: &CTerm<R>) -> Result<bool> {
        Ok(self.value(t)?.is_none_or(|(_, v)| v.is_empty()))
    }

    fn act(&self, t: &CTerm<R>, obj: &Object, f: &Poly<R::Elem>) -> Result<PolyVector<R::Elem>> {
        let only: CTerm<R> = Term {
            terms: t
                .terms
                .iter()
                .filter(|(w, _)| &w.source == obj)
                .cloned()
                .collect(),
        };
        let Some((alg, val)) = self.value(&only)? else {
            return Ok(vec![]);
        };
        let mut by_target: BTreeMap<Object, Poly<R::Elem>> = BTreeMap::new();
        for ((i, w), h) in &val {
            let mut g = f.clone();
            for &k in w.reduced_word().iter().rev() {
                g = alg.d(k, &g);
            }
            let x = alg.ring.mul(h, &g);
            let e = by_target
                .entry(permute_object(i, w))
                .or_insert_with(|| alg.ring.zero());
            *e = alg.ring.add(e, &x);
        }
        Ok(by_target
            .into_iter()
            .filter(|(_, p)| !alg.ring.is_zero(p))
            .collect())
    }

    fn poly_ring(&self, n: usize) -> PolyRing<R> {
        strand_ring(self.ctx, n, self.odd)
    }

    fn graded_basis(&self, src: &Object, degree: i64) -> Result<Vec<Word>> {
        self.shape().graded_basis(src, degree)
    }

    fn min_degree(&self, src: &Object) -> Result<i64> {
        self.shape().min_degree(src)
    }

    fn crossing_words(&self, src: &Object) -> Result<Vec<Word>> {
        self.shape().crossing_words(src)
    }

    fn max_degree_below_dot_cap(&self, src: &Object, cap: u32) -> Result<i64> {
        self.shape().max_degree_below_dot_cap(src, cap)
    }
}
