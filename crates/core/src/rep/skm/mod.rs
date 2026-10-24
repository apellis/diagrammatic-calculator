//! The basis normal form of Brundan–Ellis's super Kac–Moody 2-category `U(g)` with odd colours
//! (arXiv:1701.04133v2), conditional on their Nondegeneracy Conjecture (Section 8).
//!
//! As for the Khovanov–Lauda 2-category (`crate::rep::kl`), a 2-morphism `f : s → t` in the
//! region `μ` is bent to `1_μ → *s t` ([`basis::bend`]), evaluated into upward diagrams with
//! bubbles ([`eval`]), and written in the candidate basis of reduced chord diagrams with dots
//! and bubbles ([`basis`]) by solving `Φ(f) = Σ a_k Φ(b_k)`. Every step of the evaluation is a
//! relation of `U(g)`, so equal normal forms are equal in `U(g)`. That
//! the basis is one (so that distinct normal forms differ) is the Nondegeneracy Conjecture,
//! open for odd colours; a failure of spanning or independence in a degree is reported as an
//! error.
//!
//! For even colours the same evaluation computes the Khovanov–Lauda normal form: `kl` declares
//! it (without the sideways crossing, which `kl` draws as the rotated upward one), since with
//! `Q = klq2` the two presentations agree under the renaming of the roles (Brundan,
//! arXiv:1501.00350), and it is faster than [`crate::rep::kl`]'s evaluation.

mod basis;
mod eval;
mod mono;
mod up;

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
use mono::{Ev, region};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use up::{OVERFLOW, Up};

type Group<E> = Vec<(Word, Poly<E>)>;
type Echelon<E> = basis_cache::Echelon<Key, Poly<E>>;

const MESSAGES: basis_cache::Messages = basis_cache::Messages {
    dependent: "skm: the basis diagrams evaluate to dependent vectors (against the Nondegeneracy Conjecture, or a basis word to fix)",
    pivot: "skm: a pivot is not invertible",
    outside: "skm: a diagram evaluates outside the span of the basis (against the Nondegeneracy Conjecture, or a basis word to fix)",
    inexact: "skm: a basis coefficient is not exact",
};

/// The BE2 basis normal form as an equality strategy.
pub struct SkmBasis<'a, R: Ring> {
    ctx: &'a Context<R>,
    roles: Roles,
    prov: Provenance,
    /// The context's fingerprint for [`basis_cache`], computed when first needed.
    fingerprint: OnceLock<Arc<str>>,
}

impl<'a, R: Ring> SkmBasis<'a, R> {
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
            lcross: if decl.roles.contains_key("lcross") {
                Some(r("lcross")?)
            } else {
                None
            },
            kl_bubbles: ctx.colours.iter().all(|c| !c.odd),
        };
        if ctx.cartan.is_none() {
            return Err(Error::precondition(
                "the BE2 normal form needs a Cartan datum",
            ));
        }
        if ctx.mu.is_none() {
            return Err(Error::precondition(
                "the BE2 normal form needs the region weight mu=",
            ));
        }
        super::require_klq2(ctx)?;
        Ok(SkmBasis {
            ctx,
            roles,
            prov,
            fingerprint: OnceLock::new(),
        })
    }

    /// The steps of a word from the empty object, with a sign: the sideways crossing is the
    /// leftward rotation of the upward one up to `(−1)^{p(a)p(b)(⟨a,λ⟩+1)}` (`lcross-rotation`).
    fn events(&self, w: &Word) -> Result<(i64, Vec<Ev>)> {
        let ro = &self.roles;
        let cartan = self.ctx.cartan.as_ref().expect("checked by the strategy");
        let mu = self.mu()?;
        let mut obj = w.source.clone();
        let mut coef = 1;
        let mut out = Vec::new();
        for l in &w.layers {
            let (g, pos) = (l.generator, l.pos as usize);
            let colour = l.binding.first().copied().unwrap_or(0);
            if g == ro.x || g == ro.xd {
                out.push(Ev::Dot(pos));
            } else if g == ro.psi || g == ro.psid {
                out.push(Ev::Cross(pos));
            } else if g == ro.cup_ef || g == ro.cup_fe {
                out.push(Ev::Cup {
                    pos,
                    colour,
                    up_left: g == ro.cup_ef,
                });
            } else if g == ro.cap_ef || g == ro.cap_fe {
                out.push(Ev::Cap { pos, bub: None });
            } else if Some(g) == ro.lcross {
                let (a, b) = (obj[pos].colour, obj[pos + 1].colour);
                let h = region(&cartan.a, &mu, &obj, pos + 2);
                let odd = |c: u16| self.ctx.colours[c as usize].odd;
                if odd(a) && odd(b) && (h[a as usize] + 1) % 2 != 0 {
                    coef = -coef;
                }
                out.extend([
                    Ev::Cup {
                        pos: pos + 2,
                        colour: a,
                        up_left: true,
                    },
                    Ev::Cross(pos + 1),
                    Ev::Cap { pos, bub: None },
                ]);
            } else {
                let name = &self.ctx.pres.generators[g as usize].name;
                return Err(Error::precondition(format!(
                    "the strategy {} does not cover the generator {name}",
                    self.prov.method
                )));
            }
            obj = self.ctx.apply(l, &obj);
        }
        Ok((coef, out))
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
            .ok_or_else(|| Error::precondition("the BE2 normal form needs the region weight mu="))
    }

    /// Runs `f` with enough bubble variables: from `kz` per colour, doubling while an index
    /// overflows. `nx`, the number of dot variables, is the longest object the words pass through,
    /// for the reason given at `crate::rep::kl`'s `with_up` (the evaluation has the same runs);
    /// [`Up::run`] checks it.
    fn with_up<T>(&self, nx: usize, kz: usize, f: impl Fn(&Up<'_, R>) -> Result<T>) -> Result<T> {
        let mut kz = kz.max(1);
        loop {
            let up = Up::new(self.ctx, nx, kz)?;
            match f(&up) {
                Err(e) if e.kind == ErrorKind::Limit && e.message == OVERFLOW => {
                    kz = kz.checked_mul(2).ok_or_else(|| {
                        Error::limit("bubble variable count exceeds the supported address space")
                    })?;
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
            let (k, evs) = self.events(w)?;
            let (wf, vals) = phi.eval(k, &evs)?;
            sum_into(
                up,
                &mut acc,
                &vals,
                &phi.bottoms(&wf, Phi::<R>::prefers_right(&wf)),
                c,
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
        crate::progress::reached(|s| s.basis = Some(words.len()));
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
                .get_or_init(|| basis_cache::fingerprint("skm", &self.prov.method, self.ctx))
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
                        let (k, evs) = self.events(b)?;
                        let (wf, vals) = phi.eval(k, &evs)?;
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

impl<R: Ring> Evaluator<R> for SkmBasis<'_, R> {
    fn provenance(&self) -> &Provenance {
        &self.prov
    }

    fn rewrites_by_relations(&self) -> bool {
        true
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
    use crate::term::Letter;

    fn ctx(spec: &str) -> Context<Rationals> {
        let pres = builtin::presentation("be2").unwrap();
        Context::new(pres, ContextSpec::parse(spec).unwrap(), Rationals).unwrap()
    }

    fn strategy(c: &Context<Rationals>) -> SkmBasis<'_, Rationals> {
        let decl = c
            .pres
            .representations
            .iter()
            .find(|d| d.kind == "skm-basis")
            .unwrap();
        let prov = Provenance {
            strategy: super::super::Strategy::BasisNormalForm,
            method: decl.name.clone(),
            certificate: decl.certificate_in(c).clone(),
        };
        SkmBasis::new(c, decl, prov).unwrap()
    }

    #[test]
    fn bubble_variables_grow_past_the_old_retry_ceiling() {
        for (spec, index) in [("be2 type=A1 mu=0", 300), ("be2 type=A1 odd=i mu=0", 600)] {
            let c = ctx(spec);
            let skm = strategy(&c);
            let kz = skm
                .with_up(0, 4, |up| {
                    up.z(0, index)?;
                    Ok(up.kz)
                })
                .unwrap();
            assert!(kz >= 300);
        }
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
                        o2.push(Letter { colour: c, up });
                        next.push(o2);
                    }
                }
            }
            out.extend(next.iter().cloned());
            last = next;
        }
        out
    }

    /// The translation into `kl`, in a context whose colours are all even.
    fn kl_translation<'a>(
        c: &'a Context<Rationals>,
    ) -> Option<Box<dyn super::super::Evaluator<Rationals> + 'a>> {
        if c.colours.iter().any(|col| col.odd) {
            return None;
        }
        let decl = c
            .pres
            .representations
            .iter()
            .find(|d| d.name == "kl-translation")
            .unwrap();
        Some(super::super::evaluator(c, decl).unwrap())
    }

    /// For even colours: the normal form `n` of `t` here is the one the translation into `kl`
    /// gives (both are written in KL's basis), and each is fixed by the other strategy.
    fn agrees_with_kl(
        c: &Context<Rationals>,
        skm: &SkmBasis<'_, Rationals>,
        kl: &dyn super::super::Evaluator<Rationals>,
        t: &CTerm<Rationals>,
        n: &CTerm<Rationals>,
    ) {
        let k = kl.normal_form(t).unwrap();
        assert_eq!(
            c.fmt_term(&k),
            c.fmt_term(n),
            "{}: skm and kl give different normal forms of {}",
            c.spec.print(),
            c.fmt_term(t)
        );
        assert_eq!(
            c.fmt_term(&skm.normal_form(&k).unwrap()),
            c.fmt_term(n),
            "{}: skm and kl disagree on {}",
            c.spec.print(),
            c.fmt_term(t)
        );
        assert_eq!(
            c.fmt_term(&kl.normal_form(n).unwrap()),
            c.fmt_term(&k),
            "{}: kl and skm disagree on {}",
            c.spec.print(),
            c.fmt_term(t)
        );
    }

    /// Both sides of every relation have the same normal form, and normal forms are fixed. For
    /// even colours the normal forms also agree with the translation into `kl`.
    fn relations_agree(spec: &str) {
        let mut c = ctx(spec);
        c.objects.clear();
        c.weight = None;
        let skm = strategy(&c);
        let kl = kl_translation(&c);
        for rel in &c.pres.relations {
            for b in instances(&c, rel).unwrap() {
                let (l, r) = instance_terms(&c, rel, &b).unwrap();
                let (nl, nr) = (skm.normal_form(&l).unwrap(), skm.normal_form(&r).unwrap());
                assert_eq!(
                    c.fmt_term(&nl),
                    c.fmt_term(&nr),
                    "{spec}: {} ({})",
                    rel.name,
                    c.fmt_term(&l)
                );
                assert_eq!(skm.normal_form(&nl).unwrap(), nl, "{spec}: {}", rel.name);
                if let Some(kl) = &kl {
                    agrees_with_kl(&c, &skm, kl.as_ref(), &l, &nl);
                    agrees_with_kl(&c, &skm, kl.as_ref(), &r, &nr);
                }
            }
        }
    }

    /// Both sides of every relation stay equal next to a strand of each colour and orientation,
    /// plain or with a dot (odd for an odd colour, so the interchange signs are exercised). A
    /// strand on the right moves the relation to another region, so the right weight there is
    /// chosen to keep the relation's region.
    fn padded_relations_agree(base: &str, mu: &str, width: usize) {
        let spec = format!("{base} mu={mu}");
        let mut c = ctx(&spec);
        c.objects.clear();
        c.weight = None;
        let skm = strategy(&c);
        let a = c.cartan.as_ref().unwrap().a.clone();
        let here = c.mu.clone().unwrap();
        // per pad: its context, the pad terms, whether it goes on the right
        let mut sides = Vec::new();
        for (col, info) in c.colours.iter().enumerate() {
            for (up, star, dot) in [(true, "", "x"), (false, "*", "xd")] {
                let pads = [
                    format!("e({}{star})", info.name),
                    format!("{dot}_1 e({}{star})", info.name),
                ];
                sides.push((c.clone(), pads.clone(), false));
                let sgn = if up { 1 } else { -1 };
                let right: Vec<i64> = (0..here.len()).map(|k| here[k] - sgn * a[k][col]).collect();
                let mut r = c.fmt_region(&right);
                if r.is_empty() {
                    r = "0".into();
                }
                let mut c2 = ctx(&format!("{base} mu={r}"));
                c2.objects.clear();
                c2.weight = None;
                sides.push((c2, pads, true));
            }
        }
        for rel in &c.pres.relations {
            for b in instances(&c, rel).unwrap() {
                let (l, r) = instance_terms(&c, rel, &b).unwrap();
                // three strands with a downward one take seconds, some a minute
                let wide = l
                    .terms
                    .iter()
                    .chain(&r.terms)
                    .any(|(w, _)| skm.longest(w) > width);
                if wide {
                    continue;
                }
                for (c2, pads, right) in &sides {
                    let skm2 = strategy(c2);
                    let kl2 = kl_translation(c2);
                    for p in pads {
                        let p = c2.parse_term(p).unwrap();
                        let (pl, pr) = if *right {
                            (c2.tensor(&l, &p), c2.tensor(&r, &p))
                        } else {
                            (c2.tensor(&p, &l), c2.tensor(&p, &r))
                        };
                        let t0 = std::time::Instant::now();
                        let (nl, nr) = (
                            skm2.normal_form(&pl).unwrap(),
                            skm2.normal_form(&pr).unwrap(),
                        );
                        if std::env::var("SKM_TRACE").is_ok() {
                            eprintln!(
                                "{spec} {} {:?} {}",
                                rel.name,
                                t0.elapsed(),
                                c2.fmt_term(&pl)
                            );
                        }
                        assert_eq!(
                            c2.fmt_term(&nl),
                            c2.fmt_term(&nr),
                            "{spec}: {} ({} = {})",
                            rel.name,
                            c2.fmt_term(&pl),
                            c2.fmt_term(&pr)
                        );
                        if let Some(kl2) = &kl2 {
                            agrees_with_kl(c2, &skm2, kl2.as_ref(), &pl, &nl);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn padded_relations_have_equal_normal_forms() {
        for mu in ["-1", "0", "1"] {
            padded_relations_agree("be2 type=A1 odd=i", mu, 1);
        }
        padded_relations_agree("be2 type=A1xA1 odd=i,j", "i-j", 1);
    }

    /// The same for relations on two strands, which makes three with the pad: a few minutes in
    /// a release build (`cargo test --release -p diagcalc-core -- --ignored skm`).
    #[test]
    #[ignore]
    fn padded_wide_relations_have_equal_normal_forms() {
        for mu in ["-2", "-1", "0", "1", "2"] {
            padded_relations_agree("be2 type=A1 odd=i", mu, 2);
        }
        padded_relations_agree("be2 type=A1xA1 odd=i,j", "i-j", 2);
    }

    #[test]
    fn relations_have_equal_normal_forms_odd_a1() {
        for mu in [-2, -1, 0, 1, 2] {
            relations_agree(&format!("be2 type=A1 odd=i mu={mu}"));
        }
    }

    /// Even colours, where this is the session's representation with a Lean certificate: every
    /// relation in types A1, A2, B2, and G2 at the conformance checker's weights, against the
    /// translation into `kl`.
    #[test]
    fn relations_have_equal_normal_forms_even() {
        for spec in [
            "be2 type=A1 mu=-3",
            "be2 type=A1 mu=-2",
            "be2 type=A1 mu=-1",
            "be2 type=A1 mu=0",
            "be2 type=A1 mu=1",
            "be2 type=A1 mu=2",
            "be2 type=A1 mu=3",
            "be2 type=A2 mu=0",
            "be2 type=A2 mu=i",
            "be2 type=A2 mu=-j",
            "be2 type=A2 mu=2i-j",
            "be2 type=A2 mu=-i-2j",
            "be2 type=B2 mu=0",
            "be2 type=B2 mu=i-j",
            "be2 type=B2 mu=-2j",
            "be2 type=G2 mu=0",
            "be2 type=G2 mu=i",
            "be2 type=G2 mu=-j",
        ] {
            relations_agree(spec);
        }
    }

    /// The same next to a strand of each colour and orientation, against the translation too.
    #[test]
    fn padded_relations_have_equal_normal_forms_even() {
        for mu in ["-1", "0", "1"] {
            padded_relations_agree("be2 type=A1", mu, 1);
        }
        padded_relations_agree("be2 type=A2", "i", 1);
        padded_relations_agree("be2 type=B2", "i-j", 1);
        padded_relations_agree("be2 type=G2", "-j", 1);
    }

    #[test]
    fn relations_have_equal_normal_forms_two_colours() {
        relations_agree("be2 type=B2 odd=j mu=i");
        relations_agree("be2 type=A1xA1 odd=i,j mu=i-j");
    }

    /// For even colours the normal form agrees with the translation into `kl`: each basis word,
    /// written in KL's basis and carried back, has itself as normal form here.
    #[test]
    fn even_colours_agree_with_the_kl_translation() {
        for spec in [
            "be2 type=A1 mu=-2",
            "be2 type=A1 mu=0",
            "be2 type=A1 mu=1",
            "be2 type=A2 mu=i",
            "be2 type=A2 mu=-i-2j",
            "be2 type=B2 mu=i-j",
            "be2 type=G2 mu=0",
            "be2 type=G2 mu=-j",
        ] {
            let c = ctx(spec);
            let skm = strategy(&c);
            let kl = kl_translation(&c).unwrap();
            let mut n = 0;
            for s in objects(c.colours.len(), 2) {
                for t in objects(c.colours.len(), 2) {
                    if s.len() + t.len() > 3 {
                        continue;
                    }
                    for degree in -2..=4 {
                        for w in basis::basis_between(&c, &skm.roles, &s, &t, degree).unwrap() {
                            let b = c.monomial_term(&w, c.coeff.one());
                            let k = kl.normal_form(&b).unwrap();
                            assert_eq!(
                                skm.normal_form(&k).unwrap(),
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
    }

    /// Every basis word is its own normal form, and so is a combination of all of them.
    #[test]
    fn basis_words_are_independent_normal_forms() {
        for (spec, size) in [
            ("be2 type=A1 odd=i mu=0", 4),
            ("be2 type=A1 odd=i mu=1", 4),
            ("be2 type=A1 odd=i mu=-2", 4),
            ("be2 type=B2 odd=j mu=i", 2),
        ] {
            let c = ctx(spec);
            let skm = strategy(&c);
            let nc = c.colours.len();
            let mut checked = 0;
            for s in objects(nc, 2) {
                for t in objects(nc, 2) {
                    if s.len() + t.len() > size {
                        continue;
                    }
                    for degree in -4..=4 {
                        let words = basis::basis_between(&c, &skm.roles, &s, &t, degree).unwrap();
                        if words.is_empty() {
                            continue;
                        }
                        let mut v = Vec::new();
                        for (k, w) in words.iter().enumerate() {
                            v.push((w.clone(), c.coeff.constant(Rationals.int(k as i64 + 1))));
                        }
                        let term = c.normalize(v);
                        let nf = skm.normal_form(&term).unwrap();
                        assert_eq!(
                            nf,
                            term,
                            "{spec}: {} → {} degree {degree}",
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
}
