//! The basis normal form of the Khovanov–Lauda 2-category `U(g)` (KL III, arXiv:0807.3250).
//!
//! A 2-morphism `f : s → t` in the region `μ` (right of everything) is bent to `1_μ → *s t`
//! ([`basis::bend`]). That space has the basis of reduced chord diagrams with dots and bubbles
//! ([`basis::basis`]; KL III Proposition 3.11 with nondegeneracy, Theorem 1.2), and the
//! evaluation `Φ` into upward diagrams with bubbles ([`eval`]) is injective on it. The normal
//! form is the solution of `Φ(f) = Σ a_k Φ(b_k)` over the basis words `b_k` of the degree of
//! `f`, unbent back to `s → t`.
//!
//! The bubble slides used by `Φ` hold for the default `Q = klq2`, which the strategy checks.

mod basis;
mod eval;
mod mono;
mod up;

use super::{Evaluator, PolyVector, Provenance, role, strand_ring};
use crate::context::Context;
use crate::elab::CTerm;
use crate::error::{Error, ErrorKind, Result};
use crate::poly::{Poly, PolyRing};
use crate::pres::RepresentationDecl;
use crate::ring::Ring;
use crate::term::{Object, Term, Word};
use basis::{Roles, bend};
use eval::{Evaluated, Key, Phi, sum_into};
use mono::Ev;
use std::collections::BTreeMap;
use up::{OVERFLOW, Up};

/// The largest number of bubble variables per colour tried before giving up.
const MAX_KZ: usize = 256;

type Group<E> = Vec<(Word, Poly<E>)>;

/// The KL basis normal form as an equality strategy.
pub struct KlBasis<'a, R: Ring> {
    ctx: &'a Context<R>,
    roles: Roles,
    prov: Provenance,
}

impl<'a, R: Ring> KlBasis<'a, R> {
    pub fn new(ctx: &'a Context<R>, decl: &RepresentationDecl, prov: Provenance) -> Result<Self> {
        let r = |name: &str| role(ctx, decl, name);
        let roles = Roles {
            x: r("x")?,
            xd: r("xd")?,
            psi: r("psi")?,
            psid: r("psid")?,
            cup_ef: r("cup_ef")?,
            cup_fe: r("cup_fe")?,
            cap_ef: r("cap_ef")?,
            cap_fe: r("cap_fe")?,
        };
        if ctx.cartan.is_none() {
            return Err(Error::precondition(
                "the KL normal form needs a Cartan datum",
            ));
        }
        let nc = ctx.colours.len();
        let q = ctx.params.get("Q");
        for a in 0..nc {
            for b in 0..nc {
                if a == b {
                    continue;
                }
                let want = ctx.default_param("klq2", a, b)?;
                if q.and_then(|t| t.get(&(a as u16, b as u16))) != Some(&want) {
                    return Err(Error::precondition(
                        "the KL normal form needs the default Q = klq2 (its bubble slides assume it)",
                    ));
                }
            }
        }
        Ok(KlBasis { ctx, roles, prov })
    }

    /// The steps of a word from the empty object.
    fn events(&self, w: &Word) -> Result<Vec<Ev>> {
        let ro = &self.roles;
        w.layers
            .iter()
            .map(|l| {
                let (g, pos) = (l.generator, l.pos as usize);
                let colour = l.binding.first().copied().unwrap_or(0);
                Ok(if g == ro.x || g == ro.xd {
                    Ev::Dot(pos)
                } else if g == ro.psi || g == ro.psid {
                    Ev::Cross(pos)
                } else if g == ro.cup_ef || g == ro.cup_fe {
                    Ev::Cup {
                        pos,
                        colour,
                        up_left: g == ro.cup_ef,
                    }
                } else if g == ro.cap_ef || g == ro.cap_fe {
                    Ev::Cap { pos, bub: None }
                } else {
                    let name = &self.ctx.pres.generators[g as usize].name;
                    return Err(Error::precondition(format!(
                        "the strategy {} does not cover the generator {name}",
                        self.prov.method
                    )));
                })
            })
            .collect()
    }

    /// The longest object a word passes through.
    fn longest(&self, w: &Word) -> usize {
        let mut obj = w.source.clone();
        let mut m = obj.len();
        for l in &w.layers {
            obj = self.ctx.apply(l, &obj);
            m = m.max(obj.len());
        }
        m
    }

    fn mu(&self) -> Result<Vec<i64>> {
        self.ctx
            .mu
            .clone()
            .ok_or_else(|| Error::precondition("the KL normal form needs the region weight mu="))
    }

    /// Runs `f` with enough bubble variables: from 4 per colour, doubling while an index
    /// overflows.
    fn with_up<T>(&self, nx: usize, f: impl Fn(&Up<'_, R>) -> Result<T>) -> Result<T> {
        let mut kz = 4;
        loop {
            let up = Up::new(self.ctx, nx, kz)?;
            match f(&up) {
                Err(e) if e.kind == ErrorKind::Limit && e.message == OVERFLOW && kz < MAX_KZ => {
                    kz *= 2;
                }
                r => return r,
            }
        }
    }

    /// The terms grouped by source and target (and degree when `by_degree`), bent.
    fn groups(
        &self,
        t: &CTerm<R>,
        by_degree: bool,
    ) -> BTreeMap<(Object, Object, i64), Group<R::Elem>> {
        let mut out: BTreeMap<_, Group<R::Elem>> = BTreeMap::new();
        for (w, c) in &t.terms {
            let d = if by_degree {
                self.ctx.word_degree(w)
            } else {
                0
            };
            out.entry((w.source.clone(), self.ctx.target(w), d))
                .or_default()
                .push((w.clone(), c.clone()));
        }
        out
    }

    /// `Φ` of a sum of bent words, with the target object.
    fn phi_sum(
        &self,
        phi: &Phi<'_, '_, R>,
        bent: &[(Word, Poly<R::Elem>)],
    ) -> Result<Evaluated<R::Elem>> {
        let up = phi.up;
        let mut acc = Vec::new();
        let mut target = Vec::new();
        for (w, c) in bent {
            let (wf, vals) = phi.eval(&self.events(w)?)?;
            sum_into(
                up,
                &mut acc,
                &vals,
                &phi.sizes(&wf, Phi::<R>::prefers_right(&wf)),
                &up.lift(c),
            );
            target = wf;
        }
        Ok((target, acc))
    }

    /// The coefficients of a homogeneous group in the basis of `HOM(s, t)`.
    fn solve_group(
        &self,
        src: &Object,
        tgt: &Object,
        terms: &Group<R::Elem>,
    ) -> Result<Vec<(Word, Poly<R::Elem>)>> {
        let mu = self.mu()?;
        let degree = self.ctx.word_degree(&terms[0].0);
        let words = basis::basis_between(self.ctx, &self.roles, src, tgt, degree)?;
        let bent: Group<R::Elem> = terms
            .iter()
            .map(|(w, c)| (bend(&self.roles, w), c.clone()))
            .collect();
        let bent_basis: Vec<Word> = words.iter().map(|b| bend(&self.roles, b)).collect();
        let target_obj = self.ctx.target(&bent[0].0);
        let mut len = 0;
        for w in bent.iter().map(|(w, _)| w).chain(&bent_basis) {
            len = len.max(self.longest(w));
        }
        let coefs = self.with_up(2 * len + 2, |up| {
            let phi = Phi::new(up, mu.clone());
            let (_, acc) = self.phi_sum(&phi, &bent)?;
            let target = phi.coords(&target_obj, &acc)?;
            let mut rows = Vec::with_capacity(words.len());
            for b in &bent_basis {
                let (wf, vals) = phi.eval(&self.events(b)?)?;
                rows.push(phi.coords(&wf, &vals)?);
            }
            solve(&self.ctx.coeff, rows, target)
        })?;
        Ok(words
            .into_iter()
            .zip(coefs)
            .filter(|(_, c)| !self.ctx.coeff.is_zero(c))
            .collect())
    }
}

/// `b·v − a·r` for sparse vectors.
fn axpy<K: Ring>(
    k: &K,
    b: &K::Elem,
    v: &BTreeMap<Key, K::Elem>,
    a: &K::Elem,
    r: &BTreeMap<Key, K::Elem>,
) -> BTreeMap<Key, K::Elem> {
    let mut out = BTreeMap::new();
    for (key, x) in v {
        out.insert(key.clone(), k.mul(b, x));
    }
    for (key, y) in r {
        let t = k.neg(&k.mul(a, y));
        let s = match out.get(key) {
            Some(o) => k.add(o, &t),
            None => t,
        };
        if k.is_zero(&s) {
            out.remove(key);
        } else {
            out.insert(key.clone(), s);
        }
    }
    out.retain(|_, x| !k.is_zero(x));
    out
}

/// `b·u − a·w` for dense vectors.
fn axpy_dense<K: Ring>(
    k: &K,
    b: &K::Elem,
    u: &[K::Elem],
    a: &K::Elem,
    w: &[K::Elem],
) -> Vec<K::Elem> {
    u.iter()
        .zip(w)
        .map(|(x, y)| k.add(&k.mul(b, x), &k.neg(&k.mul(a, y))))
        .collect()
}

/// The coefficients `a` with `Σ a_j rows_j = target`, by fraction-free elimination (pivots made
/// one over a field), each row's pivot its least key.
fn solve<K: Ring>(
    k: &K,
    rows: Vec<BTreeMap<Key, K::Elem>>,
    target: BTreeMap<Key, K::Elem>,
) -> Result<Vec<K::Elem>> {
    let n = rows.len();
    let field = k.is_field();
    type Row<E> = (BTreeMap<Key, E>, Vec<E>);
    let mut ech: BTreeMap<Key, Row<K::Elem>> = BTreeMap::new();
    for (j, mut v) in rows.into_iter().enumerate() {
        let mut comb = vec![k.zero(); n];
        comb[j] = k.one();
        loop {
            let Some((p, a)) = v.iter().next().map(|(p, a)| (p.clone(), a.clone())) else {
                return Err(Error::invalid(
                    "kl: the basis diagrams evaluate to dependent vectors",
                ));
            };
            match ech.get(&p) {
                Some((r, rc)) => {
                    let b = r[&p].clone();
                    v = axpy(k, &b, &v, &a, r);
                    comb = axpy_dense(k, &b, &comb, &a, rc);
                }
                None => {
                    if field {
                        let inv = k
                            .inv(&a)
                            .ok_or_else(|| Error::invalid("kl: a pivot is not invertible"))?;
                        v = v.iter().map(|(q, x)| (q.clone(), k.mul(&inv, x))).collect();
                        comb = comb.iter().map(|x| k.mul(&inv, x)).collect();
                    }
                    ech.insert(p, (v, comb));
                    break;
                }
            }
        }
    }
    // s · target = Σ coef_j rows_j + t
    let mut t = target;
    let mut s = k.one();
    let mut coef = vec![k.zero(); n];
    while let Some((p, a)) = t.iter().next().map(|(p, a)| (p.clone(), a.clone())) {
        let (r, rc) = ech.get(&p).ok_or_else(|| {
            Error::invalid("kl: a diagram evaluates outside the span of the basis")
        })?;
        let b = r[&p].clone();
        t = axpy(k, &b, &t, &a, r);
        s = k.mul(&b, &s);
        coef = coef
            .iter()
            .zip(rc)
            .map(|(c, x)| k.add(&k.mul(&b, c), &k.mul(&a, x)))
            .collect();
    }
    coef.iter()
        .map(|c| {
            k.div_exact(c, &s)
                .ok_or_else(|| Error::invalid("kl: a basis coefficient is not exact"))
        })
        .collect()
}

impl<R: Ring> Evaluator<R> for KlBasis<'_, R> {
    fn provenance(&self) -> &Provenance {
        &self.prov
    }

    fn normal_form(&self, t: &CTerm<R>) -> Result<CTerm<R>> {
        let mut out = Term::zero();
        for ((src, tgt, _), terms) in self.groups(t, true) {
            for (w, c) in self.solve_group(&src, &tgt, &terms)? {
                out = self.ctx.add(&out, &self.ctx.monomial_term(&w, c));
            }
        }
        Ok(out)
    }

    fn is_zero(&self, t: &CTerm<R>) -> Result<bool> {
        let mu = self.mu()?;
        for (_, terms) in self.groups(t, false) {
            let bent: Group<R::Elem> = terms
                .iter()
                .map(|(w, c)| (bend(&self.roles, w), c.clone()))
                .collect();
            let len = bent.iter().map(|(w, _)| self.longest(w)).max().unwrap_or(0);
            let zero = self.with_up(2 * len + 2, |up| {
                let phi = Phi::new(up, mu.clone());
                let (_, acc) = self.phi_sum(&phi, &bent)?;
                Ok(acc.iter().all(|v| v.is_empty()))
            })?;
            if !zero {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn act(&self, _t: &CTerm<R>, _obj: &Object, _f: &Poly<R::Elem>) -> Result<PolyVector<R::Elem>> {
        Err(Error::precondition(format!(
            "the strategy {} is a basis normal form and has no action on polynomials",
            self.prov.method
        )))
    }

    fn poly_ring(&self, n: usize) -> PolyRing<R> {
        strand_ring(self.ctx, n, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin;
    use crate::conformance::{instance_terms, instances};
    use crate::context::ContextSpec;
    use crate::ring::Rationals;

    fn ctx(spec: &str) -> Context<Rationals> {
        let pres = builtin::presentation("kl").unwrap();
        Context::new(pres, ContextSpec::parse(spec).unwrap(), Rationals).unwrap()
    }

    fn strategy(c: &Context<Rationals>) -> KlBasis<'_, Rationals> {
        let decl = &c.pres.representations[0];
        let prov = Provenance {
            strategy: super::super::Strategy::BasisNormalForm,
            method: decl.name.clone(),
            certificate: decl.certificate.clone(),
        };
        KlBasis::new(c, decl, prov).unwrap()
    }

    fn objects(nc: usize, len: usize) -> Vec<Object> {
        let mut out = vec![vec![]];
        let mut last = vec![vec![]];
        for _ in 0..len {
            let mut next = Vec::new();
            for o in &last {
                for c in 0..nc as u16 {
                    for up in [true, false] {
                        let mut o2: Object = o.clone();
                        o2.push(crate::term::Letter { colour: c, up });
                        next.push(o2);
                    }
                }
            }
            out.extend(next.iter().cloned());
            last = next;
        }
        out
    }

    /// Every basis word is its own normal form, and so is a combination of all of them: the
    /// evaluations of the basis are independent and the solve recovers the coefficients.
    #[test]
    fn basis_words_are_independent_normal_forms() {
        for (spec, size) in [
            ("kl type=A1 mu=0", 4),
            ("kl type=A1 mu=2", 4),
            ("kl type=A1 mu=-1", 4),
            ("kl type=A2 mu=i-j", 2),
        ] {
            let c = ctx(spec);
            let kl = strategy(&c);
            let nc = c.colours.len();
            let mut checked = 0;
            for s in objects(nc, 2) {
                for t in objects(nc, 2) {
                    if s.len() + t.len() > size {
                        continue;
                    }
                    for degree in -4..=4 {
                        let words = basis::basis_between(&c, &kl.roles, &s, &t, degree).unwrap();
                        if words.is_empty() {
                            continue;
                        }
                        let mut v = Vec::new();
                        for (k, w) in words.iter().enumerate() {
                            v.push((w.clone(), c.coeff.constant(Rationals.int(k as i64 + 1))));
                        }
                        let term = c.normalize(v);
                        let nf = kl.normal_form(&term).unwrap();
                        assert_eq!(
                            nf,
                            term,
                            "{spec}: {} → {}",
                            c.fmt_object(&s),
                            c.fmt_object(&t)
                        );
                        checked += words.len();
                    }
                }
            }
            assert!(checked > 20, "{spec}: only {checked} basis words");
        }
    }

    /// Both sides of every relation have the same normal form.
    #[test]
    fn relations_have_equal_normal_forms() {
        for spec in ["kl type=A1 mu=1", "kl type=A1 mu=-2", "kl type=A2 mu=-j"] {
            let mut c = ctx(spec);
            c.objects.clear();
            c.weight = None;
            let kl = strategy(&c);
            for rel in &c.pres.relations {
                for b in instances(&c, rel).unwrap() {
                    let (l, r) = instance_terms(&c, rel, &b).unwrap();
                    let (nl, nr) = (kl.normal_form(&l).unwrap(), kl.normal_form(&r).unwrap());
                    assert_eq!(nl, nr, "{spec}: {}", rel.name);
                    assert_eq!(kl.normal_form(&nl).unwrap(), nl, "{spec}: {}", rel.name);
                }
            }
        }
    }
}
