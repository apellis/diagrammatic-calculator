//! The polynomial representation of a KLR algebra, computed in the smash product of rational
//! functions with the symmetric group.
//!
//! An element acting on `⊕_i Pol e(i)` is written uniquely as `Σ r_{i,w} · w e(i)`, with `r` a
//! rational function in `x_1, …, x_n` whose denominator is a product of differences `x_a − x_b`,
//! and `w` permuting variables by `x_a ↦ x_{w(a)}`. Products follow `(r w)(s v) = r · w(s) · wv`.
//! Generators act by
//!
//! * `x_k e(i)` = multiplication by `x_k`;
//! * `ψ_k e(i)` with `i_k = i_{k+1}`: the divided difference `(1 − s_k) / (x_k − x_{k+1})`;
//! * `ψ_k e(i)` with `i_k = a ≠ b = i_{k+1}`: `P_ab(x_k, x_{k+1}) · s_k`, where `P_ab = 1` for
//!   `a < b` and `P_ab(u, v) = Q_ba(u, v)` for `a > b`, so that `Q_ab(u, v) = P_ba(u, v) P_ab(v, u)`.
//!
//! The parameter is the presentation's parameter named `Q`. Faithfulness (over a domain, with
//! every `Q_ab ≠ 0`) is the representation's certificate; the canonical value then decides
//! equality, and peeling off the longest permutation first writes the value in the basis
//! `ψ_ŵ x^a e(i)` (Rouquier, Khovanov–Lauda).

use super::{
    BasisShape, Evaluator, PolyVector, Provenance, basis_word, lift_coeff, permute_object,
    split_monomial, strand_ring,
};
use crate::context::{Context, PARAM_VARS};
use crate::elab::CTerm;
use crate::error::{Error, Result};
use crate::perm::Perm;
use crate::poly::{Monomial, Poly, PolyRing};
use crate::ring::Ring;
use crate::term::{Object, Term, Word};
use smallvec::{SmallVec, smallvec};
use std::collections::BTreeMap;

/// The exponents of a denominator, indexed by `a·n + b`: inline for up to 4 strands.
pub(super) type Den = SmallVec<[u32; 16]>;

/// `num / Π_{a<b} (x_a − x_b)^{den[a·n+b]}`, with no factor of the denominator dividing `num`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Frac<E> {
    pub(super) num: Poly<E>,
    pub(super) den: Den,
}

pub(super) type Value<E> = BTreeMap<(Object, Perm), Frac<E>>;

/// Arithmetic on `n` strands.
pub(super) struct Alg<R: Ring> {
    pub(super) n: usize,
    pub(super) ring: PolyRing<R>,
    /// `x_a − x_b`, indexed by `a·n + b`.
    pub(super) diffs: Vec<Poly<R::Elem>>,
}

impl<R: Ring> Alg<R> {
    pub(super) fn new(ring: PolyRing<R>, n: usize) -> Self {
        let mut diffs = vec![ring.zero(); n * n];
        for a in 0..n {
            for b in a + 1..n {
                diffs[a * n + b] = ring.sub(&ring.var(a), &ring.var(b));
            }
        }
        Alg { n, ring, diffs }
    }

    pub(super) fn poly(&self, p: Poly<R::Elem>) -> Frac<R::Elem> {
        Frac {
            num: p,
            den: smallvec![0; self.n * self.n],
        }
    }

    /// Cancels factors `x_a − x_b` of the denominator that divide the numerator, trying only
    /// the factors whose index `a·n + b` is in `ks`.
    pub(super) fn canon_at(
        &self,
        mut f: Frac<R::Elem>,
        ks: impl Iterator<Item = usize>,
    ) -> Frac<R::Elem> {
        if self.ring.is_zero(&f.num) {
            f.den.iter_mut().for_each(|e| *e = 0);
            return f;
        }
        let n = self.n;
        for k in ks {
            while f.den[k] > 0 {
                match self.ring.div_by_difference(&f.num, k / n, k % n) {
                    Some(q) => {
                        f.num = q;
                        f.den[k] -= 1;
                    }
                    None => break,
                }
            }
        }
        f
    }

    pub(super) fn canon(&self, f: Frac<R::Elem>) -> Frac<R::Elem> {
        let ks: Vec<usize> = (0..f.den.len()).filter(|&k| f.den[k] > 0).collect();
        self.canon_at(f, ks.into_iter())
    }

    /// Both factors are in lowest terms. Over a domain `x_a − x_b` is prime, and a factor of one
    /// denominator cannot divide that numerator, so only the factors in exactly one of the
    /// denominators can cancel.
    pub(super) fn mul(&self, x: &Frac<R::Elem>, y: &Frac<R::Elem>) -> Frac<R::Elem> {
        let num = self.ring.mul(&x.num, &y.num);
        let den: Den = x.den.iter().zip(&y.den).map(|(a, b)| a + b).collect();
        if !self.ring.is_domain() {
            return self.canon(Frac { num, den });
        }
        let ks = (0..den.len()).filter(|&k| (x.den[k] > 0) != (y.den[k] > 0));
        self.canon_at(Frac { num, den }, ks)
    }

    /// Both summands are in lowest terms. Over a domain, on the common denominator, a factor
    /// with a higher power in one denominator divides the other lifted numerator but not this
    /// one, so only factors with equal positive powers can cancel.
    pub(super) fn add(&self, x: &Frac<R::Elem>, y: &Frac<R::Elem>) -> Frac<R::Elem> {
        let den: Den = x.den.iter().zip(&y.den).map(|(a, b)| *a.max(b)).collect();
        let lift = |f: &Frac<R::Elem>| {
            let mut num = f.num.clone();
            for (k, (&have, &want)) in f.den.iter().zip(&den).enumerate() {
                for _ in have..want {
                    num = self.ring.mul(&num, &self.diffs[k]);
                }
            }
            num
        };
        let num = self.ring.add(&lift(x), &lift(y));
        if !self.ring.is_domain() {
            return self.canon(Frac { num, den });
        }
        let ks = (0..den.len()).filter(|&k| x.den[k] > 0 && x.den[k] == y.den[k]);
        self.canon_at(Frac { num, den }, ks)
    }

    /// [`Alg::add`] into `x`, lifting only the summand that needs it.
    pub(super) fn add_assign(&self, x: &mut Frac<R::Elem>, y: &Frac<R::Elem>) {
        if !self.ring.is_domain() {
            *x = self.add(x, y);
            return;
        }
        let mut ks: SmallVec<[usize; 16]> = SmallVec::new();
        let mut lifted_y: Option<Poly<R::Elem>> = None;
        for k in 0..x.den.len() {
            let (have, want) = (x.den[k], y.den[k]);
            if have > 0 && have == want {
                ks.push(k);
            } else if have < want {
                for _ in have..want {
                    x.num = self.ring.mul(&x.num, &self.diffs[k]);
                }
                x.den[k] = want;
            } else if want < have {
                let ly = lifted_y.get_or_insert_with(|| y.num.clone());
                for _ in want..have {
                    *ly = self.ring.mul(ly, &self.diffs[k]);
                }
            }
        }
        self.ring
            .add_assign(&mut x.num, lifted_y.as_ref().unwrap_or(&y.num));
        let f = std::mem::replace(
            x,
            Frac {
                num: self.ring.zero(),
                den: Den::new(),
            },
        );
        *x = self.canon_at(f, ks.into_iter());
    }

    pub(super) fn neg(&self, x: &Frac<R::Elem>) -> Frac<R::Elem> {
        Frac {
            num: self.ring.neg(&x.num),
            den: x.den.clone(),
        }
    }

    /// `w(p)`: `x_a ↦ x_{w(a)}`, scalars fixed.
    pub(super) fn act_poly(&self, w: &Perm, p: &Poly<R::Elem>) -> Poly<R::Elem> {
        if w.is_identity() {
            return p.clone();
        }
        let mut perm: Vec<usize> = (0..self.ring.nvars()).collect();
        for (a, &b) in w.0.iter().enumerate() {
            perm[a] = b as usize;
        }
        self.ring.permute_vars(p, &perm)
    }

    pub(super) fn act(&self, w: &Perm, f: &Frac<R::Elem>) -> Frac<R::Elem> {
        if w.is_identity() {
            return f.clone();
        }
        let n = self.n;
        let mut num = self.act_poly(w, &f.num);
        let mut den: Den = smallvec![0; n * n];
        for a in 0..n {
            for b in a + 1..n {
                let e = f.den[a * n + b];
                if e == 0 {
                    continue;
                }
                let (wa, wb) = (w.0[a] as usize, w.0[b] as usize);
                if wa < wb {
                    den[wa * n + wb] += e;
                } else {
                    den[wb * n + wa] += e;
                    if e % 2 == 1 {
                        num = self.ring.neg(&num);
                    }
                }
            }
        }
        Frac { num, den }
    }

    /// `x / y`, which must be a polynomial.
    pub(super) fn div_to_poly(
        &self,
        x: &Frac<R::Elem>,
        y: &Frac<R::Elem>,
    ) -> Option<Poly<R::Elem>> {
        // x / y = x.num · Π y.den / (y.num · Π x.den)
        let mut top = x.num.clone();
        for (k, &e) in y.den.iter().enumerate() {
            for _ in 0..e {
                top = self.ring.mul(&top, &self.diffs[k]);
            }
        }
        let q = self.ring.div_exact(&top, &y.num)?;
        let f = self.canon(Frac {
            num: q,
            den: x.den.clone(),
        });
        f.den.iter().all(|&e| e == 0).then_some(f.num)
    }

    pub(super) fn val_add(&self, a: &mut Value<R::Elem>, b: &Value<R::Elem>) {
        for (k, f) in b {
            match a.get_mut(k) {
                Some(g) => {
                    self.add_assign(g, f);
                    if self.ring.is_zero(&g.num) {
                        a.remove(k);
                    }
                }
                None => {
                    a.insert(k.clone(), f.clone());
                }
            }
        }
    }

    /// Adds `f` at the key `k` of `a`.
    fn val_add_one(&self, a: &mut Value<R::Elem>, k: (Object, Perm), f: Frac<R::Elem>) {
        match a.get_mut(&k) {
            Some(g) => {
                self.add_assign(g, &f);
                if self.ring.is_zero(&g.num) {
                    a.remove(&k);
                }
            }
            None => {
                a.insert(k, f);
            }
        }
    }

    /// `top ∘ bottom`.
    pub(super) fn val_mul(&self, top: &Value<R::Elem>, bottom: &Value<R::Elem>) -> Value<R::Elem> {
        let mut out = Value::new();
        for ((j, v), s) in bottom {
            let t = permute_object(j, v);
            for ((i, w), r) in top.range((t.clone(), Perm::identity(0))..) {
                if *i != t {
                    break;
                }
                let f = self.mul(r, &self.act(w, s));
                self.val_add_one(&mut out, (j.clone(), w.compose(v)), f);
            }
        }
        out
    }
}

/// The KLR polynomial representation as an equality strategy.
pub struct Smash<'a, R: Ring> {
    ctx: &'a Context<R>,
    dot: u16,
    crossing: u16,
    prov: Provenance,
}

impl<'a, R: Ring> Smash<'a, R> {
    pub fn new(ctx: &'a Context<R>, dot: u16, crossing: u16, prov: Provenance) -> Self {
        Smash {
            ctx,
            dot,
            crossing,
            prov,
        }
    }

    fn shape(&self) -> BasisShape<'_, R> {
        BasisShape {
            ctx: self.ctx,
            dot: self.dot,
            crossing: self.crossing,
            dots_on_top: false,
        }
    }

    fn alg(&self, n: usize) -> Alg<R> {
        Alg::new(strand_ring(self.ctx, n, false), n)
    }

    /// `P_ab(x_k, x_{k+1})` in the strand ring.
    fn p_poly(&self, alg: &Alg<R>, a: u16, b: u16, k: usize) -> Result<Poly<R::Elem>> {
        if a < b {
            return Ok(alg.ring.one());
        }
        let q = self
            .ctx
            .params
            .get("Q")
            .and_then(|t| t.get(&(b, a)))
            .ok_or_else(|| {
                Error::precondition("the KLR polynomial representation needs the parameter Q")
            })?;
        let np = PARAM_VARS.len();
        let mut terms = Vec::new();
        for (m, c) in &q.terms {
            if m.0[2] != 0 {
                return Err(Error::invalid("Q may only use the variables u and v"));
            }
            let mut e: crate::poly::Exps = smallvec::smallvec![0; alg.ring.nvars()];
            e[k] = m.0[0];
            e[k + 1] = m.0[1];
            e[alg.n..].copy_from_slice(&m.0[np..]);
            terms.push((Monomial(e), c.clone()));
        }
        Ok(alg.ring.normalize(terms))
    }

    fn word_value(&self, alg: &Alg<R>, w: &Word) -> Result<Value<R::Elem>> {
        let n = alg.n;
        let mut val = Value::new();
        val.insert(
            (w.source.clone(), Perm::identity(n)),
            alg.poly(alg.ring.one()),
        );
        let mut obj = w.source.clone();
        for l in &w.layers {
            let k = l.pos as usize;
            let mut g = Value::new();
            if l.generator == self.dot {
                g.insert((obj.clone(), Perm::identity(n)), alg.poly(alg.ring.var(k)));
            } else if l.generator == self.crossing {
                let (a, b) = (obj[k].colour, obj[k + 1].colour);
                if a == b {
                    let mut den: Den = smallvec![0; n * n];
                    den[k * n + k + 1] = 1;
                    let inv = Frac {
                        num: alg.ring.one(),
                        den,
                    };
                    g.insert((obj.clone(), Perm::identity(n)), inv.clone());
                    g.insert((obj.clone(), Perm::simple(n, k)), alg.neg(&inv));
                } else {
                    let p = self.p_poly(alg, a, b, k)?;
                    g.insert((obj.clone(), Perm::simple(n, k)), alg.poly(p));
                }
            } else {
                let name = &self.ctx.pres.generators[l.generator as usize].name;
                return Err(Error::precondition(format!(
                    "the representation {} does not cover the generator {name}",
                    self.prov.method
                )));
            }
            val = alg.val_mul(&g, &val);
            obj = self.ctx.apply(l, &obj);
        }
        Ok(val)
    }

    fn strands(&self, t: &CTerm<R>) -> Result<Option<usize>> {
        let mut n = None;
        for (w, _) in &t.terms {
            match n {
                None => n = Some(w.source.len()),
                Some(m) if m != w.source.len() => {
                    return Err(Error::precondition(
                        "the terms have different numbers of strands",
                    ));
                }
                _ => {}
            }
        }
        Ok(n)
    }

    fn value(&self, alg: &Alg<R>, t: &CTerm<R>) -> Result<Value<R::Elem>> {
        let mut out = Value::new();
        for (w, c) in &t.terms {
            let c = alg.poly(lift_coeff(c, alg.n));
            let mut v = self.word_value(alg, w)?;
            for f in v.values_mut() {
                *f = alg.mul(f, &c);
            }
            alg.val_add(&mut out, &v);
        }
        Ok(out)
    }
}

impl<R: Ring> Evaluator<R> for Smash<'_, R> {
    fn provenance(&self) -> &Provenance {
        &self.prov
    }

    fn normal_form(&self, t: &CTerm<R>) -> Result<CTerm<R>> {
        let Some(n) = self.strands(t)? else {
            return Ok(Term::zero());
        };
        let alg = self.alg(n);
        let mut val = self.value(&alg, t)?;
        let mut out = Term::zero();
        while let Some(((i, w), r)) = val
            .iter()
            .max_by_key(|((i, w), _)| (w.length(), std::cmp::Reverse((i.clone(), w.clone()))))
            .map(|(k, r)| (k.clone(), r.clone()))
        {
            let word = w.reduced_word();
            // each (i, w) is peeled at most once: later steps only touch shorter permutations
            let bw = basis_word(
                self.ctx,
                &i,
                &word,
                &vec![0; n],
                self.dot,
                self.crossing,
                false,
            )?;
            let lead_val = &self.word_value(&alg, &bw)?;
            let lead = lead_val
                .get(&(i.clone(), w.clone()))
                .ok_or_else(|| Error::invalid("a basis element has no leading term"))?;
            let q = alg
                .div_to_poly(&r, lead)
                .ok_or_else(|| Error::invalid("the value is not in the image of the algebra"))?;
            let f = alg.act_poly(&w.inverse(), &q);
            for (m, c) in &f.terms {
                let (exps, scal) = split_monomial(m, n);
                let bw = basis_word(self.ctx, &i, &word, &exps, self.dot, self.crossing, false)?;
                let coef = self.ctx.coeff.monomial(scal, c.clone());
                out = self.ctx.add(&out, &self.ctx.monomial_term(&bw, coef));
            }
            let mut fv = Value::new();
            fv.insert((i.clone(), Perm::identity(n)), alg.poly(alg.ring.neg(&f)));
            let sub = alg.val_mul(lead_val, &fv);
            alg.val_add(&mut val, &sub);
            if val.contains_key(&(i.clone(), w.clone())) {
                return Err(Error::invalid(
                    "peeling off a basis element did not cancel its leading term",
                ));
            }
        }
        Ok(out)
    }

    fn is_zero(&self, t: &CTerm<R>) -> Result<bool> {
        let Some(n) = self.strands(t)? else {
            return Ok(true);
        };
        Ok(self.value(&self.alg(n), t)?.is_empty())
    }

    fn act(&self, t: &CTerm<R>, obj: &Object, f: &Poly<R::Elem>) -> Result<PolyVector<R::Elem>> {
        let n = obj.len();
        let alg = self.alg(n);
        let only: CTerm<R> = Term {
            terms: t
                .terms
                .iter()
                .filter(|(w, _)| &w.source == obj)
                .cloned()
                .collect(),
        };
        let val = self.value(&alg, &only)?;
        let mut by_target: BTreeMap<Object, Frac<R::Elem>> = BTreeMap::new();
        let ff = alg.poly(f.clone());
        for ((i, w), r) in &val {
            let x = alg.mul(r, &alg.act(w, &ff));
            let tgt = permute_object(i, w);
            let s = match by_target.get(&tgt) {
                Some(y) => alg.add(y, &x),
                None => x,
            };
            by_target.insert(tgt, s);
        }
        let mut out = Vec::new();
        for (o, x) in by_target {
            if x.den.iter().any(|&e| e != 0) {
                return Err(Error::invalid("the action produced a non-polynomial"));
            }
            if !alg.ring.is_zero(&x.num) {
                out.push((o, x.num));
            }
        }
        Ok(out)
    }

    fn poly_ring(&self, n: usize) -> PolyRing<R> {
        strand_ring(self.ctx, n, false)
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
