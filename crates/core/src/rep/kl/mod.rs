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
pub(crate) mod up;

use super::basis_cache;
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
use std::sync::{Arc, OnceLock};
use up::{OVERFLOW, Up};

/// The largest number of bubble variables per colour tried before giving up.
const MAX_KZ: usize = 256;

type Group<E> = Vec<(Word, Poly<E>)>;
type Echelon<E> = basis_cache::Echelon<Key, Poly<E>>;

const MESSAGES: basis_cache::Messages = basis_cache::Messages {
    dependent: "kl: the basis diagrams evaluate to dependent vectors",
    pivot: "kl: a pivot is not invertible",
    outside: "kl: a diagram evaluates outside the span of the basis",
    inexact: "kl: a basis coefficient is not exact",
};

/// The KL basis normal form as an equality strategy.
pub struct KlBasis<'a, R: Ring> {
    ctx: &'a Context<R>,
    roles: Roles,
    prov: Provenance,
    /// The context's fingerprint for [`basis_cache`], computed when first needed.
    fingerprint: OnceLock<Arc<str>>,
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
        super::require_klq2(ctx)?;
        Ok(KlBasis {
            ctx,
            roles,
            prov,
            fingerprint: OnceLock::new(),
        })
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

    /// A first guess at the bubble variables `words` need ([`basis_cache::kz_estimate`]).
    fn kz_estimate<'w>(&self, words: impl Iterator<Item = &'w Word>) -> Result<usize> {
        let mut prefix = 0;
        for w in words {
            let mut acc = 0;
            for d in self.ctx.layer_degrees(w) {
                acc += d;
                prefix = prefix.max(acc);
            }
        }
        let h = self.mu()?.iter().map(|x| x.abs()).max().unwrap_or(0);
        let cartan = self.ctx.cartan.as_ref().expect("checked by the strategy");
        let d = cartan.d.iter().copied().min().unwrap_or(1);
        let all_odd = self.ctx.colours.iter().all(|c| c.odd);
        Ok(basis_cache::kz_estimate(prefix, h, d, all_odd))
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

    /// Runs `f` with enough bubble variables: from `kz` per colour, doubling while an index
    /// overflows. `nx`, the number of dot variables, is the longest object the words pass through:
    /// `Φ` ([`eval`]) works one run of layers at a time, from an object `w` to `w2`, and straightens
    /// `q ≫ run ≫ p` (between sorted components no longer than `w` and `w2`) into one upward
    /// diagram on `#E(src) + #F(tgt) = (|src| + |tgt|) / 2` strands, since each cup or cap of the
    /// run adds or removes one `E` and one `F`. So no value has more than `max(|w|, |w2|)` strands,
    /// and partial traces only remove strands; [`Up::run`] checks it.
    fn with_up<T>(&self, nx: usize, kz: usize, f: impl Fn(&Up<'_, R>) -> Result<T>) -> Result<T> {
        let mut kz = kz;
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
        let space = basis_cache::Key {
            context: self
                .fingerprint
                .get_or_init(|| basis_cache::fingerprint("kl", &self.prov.method, self.ctx))
                .clone(),
            src: src.clone(),
            tgt: tgt.clone(),
            degree,
            nx: len,
            kz: 0,
        };
        let estimate = self.kz_estimate(bent.iter().map(|(w, _)| w).chain(&bent_basis))?;
        let kz = basis_cache::start_kz(&space, estimate);
        let coefs = self.with_up(len, kz, |up| {
            let phi = Phi::new(up, mu.clone());
            let (_, acc) = self.phi_sum(&phi, &bent)?;
            let target = phi.coords(&target_obj, &acc)?;
            let key = basis_cache::Key {
                nx: up.nx,
                kz: up.kz,
                ..space.clone()
            };
            let ech = match basis_cache::get::<Echelon<R::Elem>>(&key) {
                Some(e) => e,
                None => {
                    let mut rows = Vec::with_capacity(words.len());
                    for b in &bent_basis {
                        let (wf, vals) = phi.eval(&self.events(b)?)?;
                        rows.push(phi.coords(&wf, &vals)?);
                    }
                    let ech = Arc::new(basis_cache::echelon(&self.ctx.coeff, rows, &MESSAGES)?);
                    basis_cache::insert(key, ech.clone(), ech.cost());
                    ech
                }
            };
            basis_cache::solve(&self.ctx.coeff, &ech, target, &MESSAGES)
        })?;
        Ok(words
            .into_iter()
            .zip(coefs)
            .filter(|(_, c)| !self.ctx.coeff.is_zero(c))
            .collect())
    }
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
            let kz = self.kz_estimate(bent.iter().map(|(w, _)| w))?;
            let zero = self.with_up(len, kz, |up| {
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
        let decl = c
            .pres
            .representations
            .iter()
            .find(|d| d.kind == "kl-basis")
            .unwrap();
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

    /// The session's representation of `kl`, the BE2 evaluation (`skm-basis`).
    fn skm(c: &Context<Rationals>) -> Box<dyn Evaluator<Rationals> + '_> {
        let decl = &c.pres.representations[0];
        assert_eq!(decl.kind, "skm-basis");
        crate::rep::evaluator(c, decl).unwrap()
    }

    /// The BE2 evaluation gives exactly this module's normal forms: both sides of every relation
    /// in types A1, A2, B2 and G2 at the conformance checker's weights.
    #[test]
    fn the_skm_evaluation_agrees_on_every_relation() {
        let mut checked = 0;
        for spec in [
            "kl type=A1 mu=-3",
            "kl type=A1 mu=-2",
            "kl type=A1 mu=-1",
            "kl type=A1 mu=0",
            "kl type=A1 mu=1",
            "kl type=A1 mu=2",
            "kl type=A1 mu=3",
            "kl type=A2 mu=0",
            "kl type=A2 mu=i",
            "kl type=A2 mu=-j",
            "kl type=A2 mu=2i-j",
            "kl type=A2 mu=-i-2j",
            "kl type=B2 mu=0",
            "kl type=B2 mu=i-j",
            "kl type=B2 mu=-2j",
            "kl type=G2 mu=0",
            "kl type=G2 mu=i",
            "kl type=G2 mu=-j",
        ] {
            let mut c = ctx(spec);
            c.objects.clear();
            c.weight = None;
            let kl = strategy(&c);
            let skm = skm(&c);
            for rel in &c.pres.relations {
                for b in instances(&c, rel).unwrap() {
                    let (l, r) = instance_terms(&c, rel, &b).unwrap();
                    for t in [&l, &r] {
                        assert_eq!(
                            c.fmt_term(&skm.normal_form(t).unwrap()),
                            c.fmt_term(&kl.normal_form(t).unwrap()),
                            "{spec}: {} ({})",
                            rel.name,
                            c.fmt_term(t)
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 1000, "only {checked} relation sides");
    }

    /// The same on every basis word of up to three strands, and on composites that need the
    /// bubble slides and the sorting decompositions.
    #[test]
    fn the_skm_evaluation_agrees_on_basis_words_and_composites() {
        for spec in [
            "kl type=A1 mu=-1",
            "kl type=A1 mu=0",
            "kl type=A1 mu=2",
            "kl type=A2 mu=i",
            "kl type=B2 mu=i-j",
            "kl type=G2 mu=-j",
        ] {
            let c = ctx(spec);
            let kl = strategy(&c);
            let skm = skm(&c);
            let mut n = 0;
            for s in objects(c.colours.len(), 2) {
                for t in objects(c.colours.len(), 2) {
                    if s.len() + t.len() > 3 {
                        continue;
                    }
                    for degree in -2..=4 {
                        for w in basis::basis_between(&c, &kl.roles, &s, &t, degree).unwrap() {
                            let b = c.monomial_term(&w, c.coeff.one());
                            assert_eq!(
                                skm.normal_form(&b).unwrap(),
                                b,
                                "{spec}: {}",
                                c.fmt_term(&b)
                            );
                            n += 1;
                        }
                    }
                }
            }
            assert!(n > 20, "{spec}: only {n} words");
        }
        for (spec, t) in [
            ("kl type=A1 mu=0", "x_1 psi_2 e(i* i i)"),
            ("kl type=A1 mu=1", "cap_1 x_2 psi_2 cup_{1,i*} e(i)"),
            (
                "kl type=A1 mu=1",
                "cap_1 x_1^2 cup_{1,i} cap_1 x_1^3 cup_{1,i} e()",
            ),
            ("kl type=A1 mu=-2", "cap_1 x_2 psi_2 cup_{1,i*} e(i)"),
            ("kl type=A1 mu=-1", "x_1^2 e(i* i)"),
            ("kl type=A2 mu=0", "cap_1 psi_2 cup_{3,j} e(j* i)"),
            ("kl type=B2 mu=i-j", "psi_2 x_3 e(i* j i)"),
        ] {
            let c = ctx(spec);
            let kl = strategy(&c);
            let skm = skm(&c);
            let t = c.parse_term(t).unwrap();
            assert_eq!(
                c.fmt_term(&skm.normal_form(&t).unwrap()),
                c.fmt_term(&kl.normal_form(&t).unwrap()),
                "{spec}: {}",
                c.fmt_term(&t)
            );
        }
    }
}
