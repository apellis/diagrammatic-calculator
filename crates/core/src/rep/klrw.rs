//! The `klrw` front end: weighted KLR algebras (and their red-strand extensions) read from a
//! context and from terms written with loadings, evaluated in the polynomial representation of
//! [`super::weighted`] and printed in the basis normal form. Conventions: `docs/klrw.md`.
//!
//! A weighted KLR algebra is not strict monoidal (its objects are loadings, points of `ℚ`), so
//! it does not go through the layered words of the rest of the calculator. An element is a map
//! from pairs (bottom, top) of loadings up to isotopy to an operator on `k[x_1, …, x_n]`.

use super::weighted::{Data, Edge, Loading, Op, OrderType, PolyRep, Q, Red};
use super::{Provenance, Strategy};
use crate::context::{Context, ContextSpec, poly_of_ast};
use crate::error::{Error, Result};
use crate::perm::Perm;
use crate::poly::{Poly, PolyRing};
use crate::pres::Certificate;
use crate::ring::Ring;
use crate::text::Ast;
use num_traits::Zero;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Coefficients: polynomials in the context's scalars.
type C<R> = Poly<<R as Ring>::Elem>;

/// The operator of one piece, between two representative loadings.
#[derive(Clone, Debug)]
pub struct Piece<E> {
    pub bottom: Loading,
    pub top: Loading,
    pub op: Op<E>,
}

/// An element: a piece for each pair of order types (bottom, top) where it is nonzero.
#[derive(Clone, Debug)]
pub struct Elem<E> {
    pub pieces: BTreeMap<(OrderType, OrderType), Piece<E>>,
}

impl<E> Default for Elem<E> {
    fn default() -> Self {
        Elem {
            pieces: BTreeMap::new(),
        }
    }
}

/// A value while evaluating: a scalar not yet attached to a diagram, or an element.
enum Val<E> {
    Scalar(E),
    Elem(Elem<E>),
}

/// One summand `c b(π; T) x^a e(B)` of a normal form.
#[derive(Clone, Debug)]
pub struct Summand<E> {
    pub bottom: Loading,
    pub top: Loading,
    pub perm: Perm,
    pub dots: Vec<u32>,
    pub coeff: E,
}

/// A `klrw` context: the colours, Cartan datum and parameters of the underlying `klr` context,
/// and the weighted data built from them.
pub struct Klrw<R: Ring> {
    pub klr: Context<R>,
    pub data: Data<C<R>>,
    pub prov: Provenance,
    /// Set for `quotient=cyclotomic`: the quotient `T^λ` by the violating ideal.
    pub cyclo: Option<super::klrw_cyclo::Cyclotomic<R>>,
}

fn parse_q(s: &str) -> Result<Q> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    t.parse::<Q>()
        .map_err(|_| Error::invalid(format!("'{s}' is not a rational number")))
}

fn fmt_q(x: &Q) -> String {
    x.to_string()
}

impl<R: Ring> Klrw<R> {
    pub fn new(spec: &ContextSpec, base: R) -> Result<Self> {
        let mut kspec = spec.clone();
        kspec.presentation = "klr".into();
        kspec.red = None;
        let thetas: Vec<(String, String)> = spec
            .params
            .iter()
            .filter(|(k, _)| k.starts_with("theta_"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (k, _) in &thetas {
            kspec.params.remove(k);
        }
        let cyclotomic = match kspec.params.remove("quotient").as_deref() {
            None => false,
            Some("cyclotomic") => {
                super::klrw_cyclo::check_field(&base, &spec.scalars)?;
                true
            }
            Some(q) => {
                return Err(Error::invalid(format!(
                    "quotient={q}: the only quotient is quotient=cyclotomic"
                )));
            }
        };
        let pres = crate::builtin::presentation("klr")?;
        let klr = Context::new(pres, kspec, base)?;
        let cartan = klr
            .cartan
            .clone()
            .ok_or_else(|| Error::invalid("a klrw context needs a Cartan type (type=A2)"))?;
        let n = klr.colours.len();
        let pair = |name: &str| -> Result<(usize, usize)> {
            for a in 0..n {
                if let Some(rest) = name.strip_prefix(&klr.colours[a].name)
                    && let Some(b) = klr.colour_index(rest)
                {
                    return Ok((a, b));
                }
            }
            Err(Error::invalid(format!(
                "theta_{name}: '{name}' is not a pair of colours"
            )))
        };
        // The default graph: for a > b with (α_a, α_b) ≠ 0, an edge a → b of weight 0.
        let mut orient: BTreeMap<(usize, usize), (usize, usize, Q)> = BTreeMap::new();
        for (a, row) in cartan.a.iter().enumerate().take(n) {
            for (b, &ab) in row.iter().enumerate().take(a) {
                if ab != 0 {
                    orient.insert((b, a), (a, b, Q::zero()));
                }
            }
        }
        for (k, v) in &thetas {
            let (t, h) = pair(&k["theta_".len()..])?;
            let key = (t.min(h), t.max(h));
            if !orient.contains_key(&key) {
                return Err(Error::invalid(format!(
                    "{k}: the colours {} and {} do not interact",
                    klr.colours[t].name, klr.colours[h].name
                )));
            }
            orient.insert(key, (t, h, parse_q(v)?));
        }
        let qtab = klr
            .params
            .get("Q")
            .ok_or_else(|| Error::invalid("internal: the klr presentation has no Q"))?;
        let coeff = &klr.coeff;
        let pr = &klr.params_ring;
        let nscal = klr.spec.scalars.len();
        let mut edges = Vec::new();
        for (t, h, w) in orient.into_values() {
            // Q_e(u, v) = Q_{h t}(u, v): u is the dot on the head strand.
            let q = &qtab[&(h as u16, t as u16)];
            let mut terms = Vec::new();
            for (m, c) in &q.terms {
                if m.0[2] != 0 {
                    return Err(Error::invalid("Q may not involve the variable w"));
                }
                let mut ex = vec![0i32; nscal];
                ex.copy_from_slice(&m.0[3..3 + nscal]);
                if cyclotomic && ex.iter().any(|&e| e != 0) {
                    // the quotient's spans are found by elimination with constant pivots
                    return Err(Error::invalid(
                        "quotient=cyclotomic needs Q free of the context's scalars",
                    ));
                }
                let mono = crate::poly::Monomial(ex.into_iter().collect());
                let cc = coeff.monomial(mono, c.clone());
                terms.push((m.0[0] as u32, m.0[1] as u32, cc));
            }
            let _ = pr;
            edges.push(Edge {
                tail: t as u16,
                head: h as u16,
                weight: w,
                q: terms,
                ghost_degree: -cartan.form(t, h),
            });
        }
        let mut reds = Vec::new();
        if let Some(r) = &spec.red {
            for item in r.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                let (wt, pos) = item.split_once('@').ok_or_else(|| {
                    Error::invalid(format!("red strand '{item}' should be weight@position"))
                })?;
                let exps = klr.parse_weight(wt.trim())?;
                reds.push(Red {
                    pos: parse_q(pos)?,
                    exps,
                });
            }
            reds.sort_by(|a, b| a.pos.cmp(&b.pos));
            if reds.windows(2).any(|w| w[0].pos == w[1].pos) {
                return Err(Error::invalid("two red strands at the same position"));
            }
        }
        let data = Data {
            d: cartan.d.clone(),
            edges,
            reds,
        };
        let mut method: String = "the polynomial representation and the straight-line basis of a \
                                  weighted KLR algebra (Webster, arXiv:1209.2463v4, Proposition \
                                  2.7 and Theorem 2.8)"
            .into();
        if cyclotomic {
            method.push_str(
                ", then reduction modulo the ideal of violating idempotents, degree by degree \
                 (the cyclotomic quotient T^λ, Webster, arXiv:1309.3796v2, §4)",
            );
        }
        // Without a cyclotomic quotient the basis and the faithfulness of the representation the
        // calculator uses are formalized (categorification-lean, `Categorification.WeightedKLR`;
        // with red strands, for the reduced quotient with the 0-strand dot set to zero). In T^λ
        // the identification with Webster's algebra is formalized, but not the description of
        // the violating ideal that the reduction uses.
        let certificate = if data.reds.is_empty() && !cyclotomic {
            Certificate {
                status: "lean".into(),
                reference:
                    "categorification-lean: WeightedKLR.GalleryAlgebra.straightWeightedKLRBasis \
                            and straightWeightedKLRChamberBasis (B. Webster, Weighted \
                            Khovanov-Lauda-Rouquier algebras, arXiv:1209.2463v4, Theorem 2.8, for \
                            Webster's algebra as the corner of the gallery algebra at the \
                            rank-ordered chambers, with the straight-line choice of b_pi); also \
                            weightedKLRBasisOfStanding, straightSegment and straightBasis"
                        .into(),
                hypotheses: "Webster's standing hypothesis on the Q_e, over any commutative ring \
                             (the computation itself needs coefficients in an integral domain); \
                             assumption (dagger), enforced by merging edges; no loops. The \
                             basis is proved independent through its action in the polynomial \
                             representation, so that representation is faithful and the \
                             operator decides equality. That the calculator's b_pi (top endpoints \
                             moved by eps^pi(k)) is the Lean straightSegment with rho(k) = pi(k) \
                             is read off docs/klrw.md, not proved in Lean"
                    .into(),
            }
        } else if cyclotomic && data.edges.iter().all(|e| e.weight.is_zero()) {
            Certificate {
                status: "lean".into(),
                reference: "categorification-lean: WeightedKLR.GalleryAlgebra.redReducedBasisIn \
                            and redReducedRepIn_injective (basis of the reduced quotient, acting \
                            faithfully with the dot on the 0-strand set to zero); \
                            Stendhal.mem_stViolatingIdeal_iff (spanning set of the violating \
                            ideal) and WeightedKLR.mkₐ_mem_stViolatingIdeal_iff (killing the \
                            violating idempotents of all chambers gives the Stendhal-cut \
                            quotient); Stendhal.stTensorEquiv (B. Webster, Weighted \
                            Khovanov-Lauda-Rouquier algebras, arXiv:1209.2463v4, Theorem 3.6: \
                            this quotient is T^lambda of B. Webster, Knot invariants and higher \
                            representation theory, arXiv:1309.3796v2, Section 4)"
                    .into(),
                hypotheses: "coefficients in a field; Webster's standing hypothesis on the Q_e; \
                             assumption (dagger), enforced by merging edges; no loops; every \
                             edge weight 0; distinct red positions. Under the isomorphism with \
                             T^lambda the black/red crossings are rescaled by \
                             (-1)^{alpha^vee(lambda_r)}. That the calculator's b_pi are the Lean \
                             basis elements, and its order types the Lean chambers, is read off \
                             docs/klrw.md, not proved in Lean"
                    .into(),
            }
        } else if !cyclotomic {
            Certificate {
                status: "lean".into(),
                reference: "categorification-lean: WeightedKLR.GalleryAlgebra.redReducedBasisIn \
                            (basis of the reduced quotient), linearIndependent_redReducedRepIn \
                            and redReducedRepIn_injective (it acts faithfully on polynomials \
                            with the dot on the 0-strand set to zero), for the Crawley-Boevey \
                            graph crawleyBoeveyIn (new edges i -> 0, red strands at x_0 + \
                            varpi_j); B. Webster, Weighted Khovanov-Lauda-Rouquier algebras, \
                            arXiv:1209.2463v4, Theorem 2.8 and Section 3.1"
                    .into(),
                hypotheses: "Webster's standing hypothesis on the Q_e, over any commutative ring \
                             (the computation itself needs coefficients in an integral domain); \
                             assumption (dagger), enforced by merging edges; no loops; distinct \
                             red positions. The faithful representation decides equality. That \
                             the calculator's b_pi is the Lean basis element is read off \
                             docs/klrw.md, not proved in Lean"
                    .into(),
            }
        } else {
            Certificate {
                status: "literature".into(),
                reference: "B. Webster, Weighted Khovanov-Lauda-Rouquier algebras, \
                            arXiv:1209.2463v4, Theorems 2.8, 3.5 and 3.6; B. Webster, Knot \
                            invariants and higher representation theory, arXiv:1309.3796v2, \
                            Section 4. Formalized in categorification-lean: the faithful reduced \
                            quotient (WeightedKLR.GalleryAlgebra.redReducedRepIn_injective) and \
                            Theorems 3.5 and 3.6 (Stendhal.stTildeEquiv, stTensorEquiv), with \
                            the spanning set of the violating ideal \
                            (Stendhal.mem_stViolatingIdeal_iff) and the basis of T~^lambda \
                            (Stendhal.stendhalBasis, arXiv:1309.3796v2, Proposition 4.16)"
                    .into(),
                hypotheses: "coefficients in a field; every Q_e nonzero; some edge has a \
                             nonzero weight. In Lean, Theorems 3.5 \
                             and 3.6 assume every edge weight 0 and no loops, and rescale the \
                             black/red crossings by (-1)^{alpha^vee(lambda_r)}, and they cut the \
                             algebra down to the Stendhal chambers. Not formalized: that killing \
                             the violating idempotents of all chambers, as the calculator does, \
                             gives the same corners at Stendhal chambers, which Lean proves \
                             only when every edge weight is 0"
                    .into(),
            }
        };
        let prov = Provenance {
            strategy: Strategy::BasisNormalForm,
            method,
            certificate,
        };
        Ok(Klrw {
            klr,
            data,
            prov,
            cyclo: cyclotomic.then(Default::default),
        })
    }

    pub(crate) fn coeff(&self) -> &PolyRing<R> {
        &self.klr.coeff
    }

    pub(crate) fn rep(&self, n: usize) -> PolyRep<'_, PolyRing<R>> {
        PolyRep::new(&self.data, self.klr.coeff.clone(), n)
    }

    /// The ring `k[x_1, …, x_n]` (over the context's scalars) acted on at loadings with `n`
    /// black strands.
    pub fn strand_ring(&self, n: usize) -> PolyRing<PolyRing<R>> {
        self.rep(n).alg.ring.clone()
    }

    /// The action of `e` on `f · e(src)`: one polynomial for each top loading (up to isotopy)
    /// where it is nonzero, read at the representative top loading of its piece.
    pub fn act(
        &self,
        e: &Elem<C<R>>,
        src: &Loading,
        f: &Poly<C<R>>,
    ) -> Result<Vec<(Loading, Poly<C<R>>)>> {
        let ot = self.data.order_type(src);
        let rep = self.rep(src.0.len());
        let one = rep.alg.poly(rep.alg.ring.one());
        let mut out = Vec::new();
        for ((bot, _), p) in &e.pieces {
            if *bot != ot {
                continue;
            }
            let mut acc = rep.alg.poly(rep.alg.ring.zero());
            for r in rep.then_poly(&p.op, f).values() {
                rep.alg.add_assign(&mut acc, r);
            }
            let g = rep
                .alg
                .div_to_poly(&acc, &one)
                .ok_or_else(|| Error::invalid("the action is not polynomial"))?;
            if !rep.alg.ring.is_zero(&g) {
                out.push((p.top.clone(), g));
            }
        }
        Ok(out)
    }

    /// Reads `i@0 j@1/2`.
    pub fn parse_loading(&self, s: &str) -> Result<Loading> {
        let mut pts = Vec::new();
        for tok in s.split_whitespace() {
            let (c, x) = tok.split_once('@').ok_or_else(|| {
                Error::invalid(format!("'{tok}' should be colour@position, as in i@1/2"))
            })?;
            let ci = self
                .klr
                .colour_index(c)
                .ok_or_else(|| Error::invalid(format!("'{c}' is not a colour")))?;
            pts.push((ci as u16, parse_q(x)?));
        }
        let l = Loading(pts);
        self.data.check_loading(&l)?;
        Ok(l)
    }

    pub fn fmt_loading(&self, l: &Loading) -> String {
        l.0.iter()
            .map(|(c, x)| format!("{}@{}", self.klr.colours[*c as usize].name, fmt_q(x)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn key(&self, p: &Piece<C<R>>) -> (OrderType, OrderType) {
        (
            self.data.order_type(&p.bottom),
            self.data.order_type(&p.top),
        )
    }

    fn insert(&self, e: &mut Elem<C<R>>, p: Piece<C<R>>) {
        if p.op.is_empty() {
            return;
        }
        let k = self.key(&p);
        match e.pieces.get_mut(&k) {
            Some(q) => {
                let rep = self.rep(p.bottom.n());
                let mut op = std::mem::take(&mut q.op);
                for (w, f) in p.op {
                    match op.get_mut(&w) {
                        Some(g) => {
                            rep.alg.add_assign(g, &f);
                            if rep.alg.ring.is_zero(&g.num) {
                                op.remove(&w);
                            }
                        }
                        None => {
                            op.insert(w, f);
                        }
                    }
                }
                if op.is_empty() {
                    e.pieces.remove(&k);
                } else {
                    q.op = op;
                }
            }
            None => {
                e.pieces.insert(k, p);
            }
        }
    }

    pub fn add(&self, a: &Elem<C<R>>, b: &Elem<C<R>>) -> Elem<C<R>> {
        let mut out = a.clone();
        for p in b.pieces.values() {
            self.insert(&mut out, p.clone());
        }
        out
    }

    pub fn scale(&self, c: &C<R>, a: &Elem<C<R>>) -> Elem<C<R>> {
        let mut out = Elem::default();
        if self.coeff().is_zero(c) {
            return out;
        }
        for p in a.pieces.values() {
            let rep = self.rep(p.bottom.n());
            let cf = rep.alg.poly(rep.alg.ring.constant(c.clone()));
            let op =
                p.op.iter()
                    .map(|(w, f)| (w.clone(), rep.alg.mul(&cf, f)))
                    .collect();
            self.insert(
                &mut out,
                Piece {
                    bottom: p.bottom.clone(),
                    top: p.top.clone(),
                    op,
                },
            );
        }
        out
    }

    pub fn neg(&self, a: &Elem<C<R>>) -> Elem<C<R>> {
        self.scale(&self.coeff().neg(&self.coeff().one()), a)
    }

    /// `top ∘ bottom`.
    pub fn compose(&self, top: &Elem<C<R>>, bottom: &Elem<C<R>>) -> Elem<C<R>> {
        let mut out = Elem::default();
        for ((_, bt), pb) in &bottom.pieces {
            for ((tb, _), pt) in &top.pieces {
                if tb != bt {
                    continue;
                }
                let rep = self.rep(pb.bottom.n());
                let op = rep.compose(&pt.op, &pb.op);
                self.insert(
                    &mut out,
                    Piece {
                        bottom: pb.bottom.clone(),
                        top: pt.top.clone(),
                        op,
                    },
                );
            }
        }
        out
    }

    pub fn identity(&self, l: &Loading) -> Elem<C<R>> {
        let mut e = Elem::default();
        let rep = self.rep(l.n());
        self.insert(
            &mut e,
            Piece {
                bottom: l.clone(),
                top: l.clone(),
                op: rep.identity(),
            },
        );
        e
    }

    /// Applies `f` to the top of every piece of `below`.
    fn on_top(
        &self,
        below: &Elem<C<R>>,
        mut f: impl FnMut(&PolyRep<'_, PolyRing<R>>, &Piece<C<R>>) -> Result<Option<Piece<C<R>>>>,
    ) -> Result<Elem<C<R>>> {
        let mut out = Elem::default();
        for p in below.pieces.values() {
            let rep = self.rep(p.bottom.n());
            if let Some(q) = f(&rep, p)? {
                self.insert(&mut out, q);
            }
        }
        Ok(out)
    }

    fn straight(
        &self,
        below: &Elem<C<R>>,
        to: &Loading,
        perm: Option<&Perm>,
    ) -> Result<Elem<C<R>>> {
        let mut matched = false;
        let out = self.on_top(below, |rep, p| {
            let n = p.top.n();
            if to.n() != n {
                return Ok(None);
            }
            let id = Perm::identity(n);
            let perm = perm.unwrap_or(&id);
            if perm.n() != n {
                return Err(Error::invalid(format!(
                    "the permutation has {} entries but the loading below has {}",
                    perm.n(),
                    plural(n, "point")
                )));
            }
            if (0..n).any(|k| p.top.0[k].0 != to.0[perm.0[k] as usize].0) {
                // the colours do not match: this piece does not continue
                return Ok(None);
            }
            matched = true;
            let seg = self.data.segment(&p.top, to, perm)?;
            Ok(Some(Piece {
                bottom: p.bottom.clone(),
                top: to.clone(),
                op: rep.apply_segment(&seg, &p.op)?,
            }))
        })?;
        if !matched {
            // A typo in the colours would otherwise give 0 silently; a sum whose other parts
            // end elsewhere keeps only the parts that continue.
            if let Some(p) = below.pieces.values().next() {
                let what = if perm.is_some() { "b(...)" } else { "to(...)" };
                return Err(Error::invalid(format!(
                    "{what} goes to {}, but the diagram below ends at {}: no part of it has \
                     the same colours in the same order",
                    self.fmt_loading(to),
                    self.fmt_loading(&p.top)
                )));
            }
        }
        Ok(out)
    }

    fn dot(&self, below: &Elem<C<R>>, k: usize) -> Result<Elem<C<R>>> {
        self.on_top(below, |rep, p| {
            if k == 0 || k > p.top.n() {
                return Err(Error::invalid(format!(
                    "x_{k}: the loading below has {}",
                    plural(p.top.n(), "point")
                )));
            }
            Ok(Some(Piece {
                bottom: p.bottom.clone(),
                top: p.top.clone(),
                op: rep.dot(k - 1, &p.op),
            }))
        })
    }

    /// `psi_k`: the colours at `k`, `k + 1` exchange places, positions kept.
    fn psi(&self, below: &Elem<C<R>>, k: usize) -> Result<Elem<C<R>>> {
        self.on_top(below, |rep, p| {
            let n = p.top.n();
            if k == 0 || k >= n {
                return Err(Error::invalid(format!(
                    "psi_{k}: the loading below has {}",
                    plural(n, "point")
                )));
            }
            let mut to = p.top.clone();
            let (a, b) = (to.0[k - 1].0, to.0[k].0);
            to.0[k - 1].0 = b;
            to.0[k].0 = a;
            self.data.check_loading(&to).map_err(|e| Error {
                message: format!("psi_{k} ends at {}: {}", self.fmt_loading(&to), e.message),
                ..e
            })?;
            let s = Perm::simple(n, k - 1);
            let seg = self.data.segment(&p.top, &to, &s)?;
            Ok(Some(Piece {
                bottom: p.bottom.clone(),
                top: to,
                op: rep.apply_segment(&seg, &p.op)?,
            }))
        })
    }

    fn needs_below(name: &str) -> Error {
        Error::invalid(format!(
            "{name} needs a loading below it, as in {name} e(i@0 j@1)"
        ))
    }

    fn eval(
        &self,
        ast: &Ast,
        below: Option<&Elem<C<R>>>,
        defs: &BTreeMap<String, Elem<C<R>>>,
    ) -> Result<Val<C<R>>> {
        self.eval_in(ast, below, defs, &[])
    }

    /// An integer expression of a term (an exponent or a summation bound), with the summation
    /// indices in `ints`.
    fn eval_int(&self, e: &crate::expr::TermInt, ints: &[(String, i64)]) -> Result<i64> {
        let scope = crate::elab::Scope::<R> {
            ints: ints.to_vec(),
            ..Default::default()
        };
        self.klr.eval_int(e, &scope)
    }

    /// `Q_ij(a, b)`: the KLR polynomial of the context with the arguments substituted, applied
    /// on top of `below`.
    fn eval_param(
        &self,
        name: &str,
        sub: &str,
        args: &[Ast],
        below: &Elem<C<R>>,
        defs: &BTreeMap<String, Elem<C<R>>>,
        ints: &[(String, i64)],
    ) -> Result<Elem<C<R>>> {
        let decl = self
            .klr
            .pres
            .parameter(name)
            .ok_or_else(|| Error::invalid(format!("unknown parameter '{name}'")))?;
        if args.len() != decl.vars.len() {
            return Err(Error::invalid(format!(
                "{name} takes {} arguments",
                decl.vars.len()
            )));
        }
        let cols = self.klr.read_object(sub, &crate::elab::Scope::default())?;
        if cols.len() != 2 || cols[0].colour == cols[1].colour {
            return Err(Error::invalid(format!(
                "{name}_{sub} needs two distinct colours"
            )));
        }
        let poly = self
            .klr
            .params
            .get(name)
            .and_then(|t| t.get(&(cols[0].colour, cols[1].colour)))
            .ok_or_else(|| Error::invalid(format!("no value for {name}_{sub}")))?
            .clone();
        let nparam = crate::context::PARAM_VARS.len();
        let mut acc = Elem::default();
        for (m, c) in &poly.terms {
            if m.0[args.len()..nparam].iter().any(|&e| e != 0) {
                return Err(Error::invalid(format!(
                    "{name} uses more variables than it has arguments"
                )));
            }
            let scal = crate::poly::Monomial::from(&m.0[nparam..]);
            let coef = self.coeff().monomial(scal, c.clone());
            let mut cur = self.scale(&coef, below);
            for (k, a) in args.iter().enumerate() {
                for _ in 0..m.0[k] {
                    cur = self.elem_of(self.eval_in(a, Some(&cur), defs, ints)?, &cur)?;
                }
            }
            acc = self.add(&acc, &cur);
        }
        Ok(acc)
    }

    fn eval_in(
        &self,
        ast: &Ast,
        below: Option<&Elem<C<R>>>,
        defs: &BTreeMap<String, Elem<C<R>>>,
        ints: &[(String, i64)],
    ) -> Result<Val<C<R>>> {
        let attach = |v: Val<C<R>>| -> Result<Val<C<R>>> {
            Ok(match (v, below) {
                (Val::Scalar(c), Some(b)) => Val::Elem(self.scale(&c, b)),
                (Val::Elem(e), Some(b)) => Val::Elem(self.compose(&e, b)),
                (v, None) => v,
            })
        };
        match ast {
            Ast::Int(_) | Ast::Frac(..) | Ast::Imag => {
                attach(Val::Scalar(poly_of_ast(ast, self.coeff())?))
            }
            Ast::Idem(s) => {
                let l = self.parse_loading(s)?;
                match below {
                    None => Ok(Val::Elem(self.identity(&l))),
                    Some(b) => {
                        let ot = self.data.order_type(&l);
                        let mut out = Elem::default();
                        for (k, p) in &b.pieces {
                            if k.1 == ot {
                                out.pieces.insert(k.clone(), p.clone());
                            }
                        }
                        Ok(Val::Elem(out))
                    }
                }
            }
            Ast::Raw { name, raw } => {
                let b = below.ok_or_else(|| Self::needs_below(&format!("{name}(...)")))?;
                match name.as_str() {
                    "to" => Ok(Val::Elem(self.straight(
                        b,
                        &self.parse_loading(raw)?,
                        None,
                    )?)),
                    "b" => {
                        let (ps, ls) = raw
                            .split_once(';')
                            .ok_or_else(|| Error::invalid("b(...) is written b(2 1; j@0 i@1)"))?;
                        let ent: Vec<usize> = ps
                            .split_whitespace()
                            .map(|t| {
                                t.parse::<usize>().map_err(|_| {
                                    Error::invalid(format!("'{t}' is not a strand number"))
                                })
                            })
                            .collect::<Result<_>>()?;
                        let n = ent.len();
                        let mut seen = vec![false; n];
                        for &e in &ent {
                            if e == 0 || e > n || seen[e - 1] {
                                return Err(Error::invalid(format!(
                                    "'{}' is not a permutation in one-line notation",
                                    ps.trim()
                                )));
                            }
                            seen[e - 1] = true;
                        }
                        let perm = Perm(ent.iter().map(|&e| (e - 1) as u8).collect());
                        let l = self.parse_loading(ls)?;
                        Ok(Val::Elem(self.straight(b, &l, Some(&perm))?))
                    }
                    _ => Err(Error::invalid(format!("unknown diagram '{name}(...)'"))),
                }
            }
            Ast::Name {
                name,
                sub,
                args: None,
            } => {
                let full = match sub {
                    Some(s) => format!("{name}_{s}"),
                    None => name.clone(),
                };
                if let Some(e) = defs.get(&full) {
                    return attach(Val::Elem(e.clone()));
                }
                let index = || -> Option<usize> { sub.as_ref()?.parse().ok() };
                match (name.as_str(), index()) {
                    ("x", Some(k)) => {
                        let b = below.ok_or_else(|| Self::needs_below(&full))?;
                        Ok(Val::Elem(self.dot(b, k)?))
                    }
                    ("psi", Some(k)) => {
                        let b = below.ok_or_else(|| Self::needs_below(&full))?;
                        Ok(Val::Elem(self.psi(b, k)?))
                    }
                    _ => attach(Val::Scalar(
                        poly_of_ast(ast, self.coeff())
                            .map_err(|_| Error::invalid(format!("'{full}' is not defined")))?,
                    )),
                }
            }
            Ast::Pow(b, e) => {
                if *e < 0 {
                    return attach(Val::Scalar(poly_of_ast(ast, self.coeff())?));
                }
                match below {
                    Some(bl) => {
                        let mut cur = bl.clone();
                        for _ in 0..*e {
                            cur = self.elem_of(self.eval_in(b, Some(&cur), defs, ints)?, bl)?;
                        }
                        Ok(Val::Elem(cur))
                    }
                    None => match self.eval_in(b, None, defs, ints)? {
                        Val::Scalar(c) => Ok(Val::Scalar(self.coeff().pow(&c, *e as u32))),
                        Val::Elem(x) => {
                            if *e == 0 {
                                return Err(Error::invalid(
                                    "the zeroth power of a diagram needs a loading below it",
                                ));
                            }
                            let mut cur = x.clone();
                            for _ in 1..*e {
                                cur = self.compose(&x, &cur);
                            }
                            Ok(Val::Elem(cur))
                        }
                    },
                }
            }
            Ast::Comp(items) => {
                let mut cur: Option<Val<C<R>>> = below.map(|b| Val::Elem(b.clone()));
                let mut scalar = self.coeff().one();
                for it in items.iter().rev() {
                    let v = match &cur {
                        Some(Val::Elem(e)) => self.eval_in(it, Some(e), defs, ints)?,
                        _ => self.eval_in(it, None, defs, ints)?,
                    };
                    match (v, cur.is_some()) {
                        (Val::Scalar(c), _) => scalar = self.coeff().mul(&scalar, &c),
                        (Val::Elem(e), true) => cur = Some(Val::Elem(e)),
                        (Val::Elem(e), false) => cur = Some(Val::Elem(e)),
                    }
                }
                Ok(match cur {
                    Some(Val::Elem(e)) => Val::Elem(self.scale(&scalar, &e)),
                    _ => Val::Scalar(scalar),
                })
            }
            Ast::Sum(items) => {
                let mut acc: Option<Val<C<R>>> = None;
                for (neg, it) in items {
                    let mut v = self.eval_in(it, below, defs, ints)?;
                    if *neg {
                        v = match v {
                            Val::Scalar(c) => Val::Scalar(self.coeff().neg(&c)),
                            Val::Elem(e) => Val::Elem(self.neg(&e)),
                        };
                    }
                    acc = Some(match (acc, v) {
                        (None, v) => v,
                        (Some(Val::Scalar(a)), Val::Scalar(b)) => {
                            Val::Scalar(self.coeff().add(&a, &b))
                        }
                        (Some(Val::Elem(a)), Val::Elem(b)) => Val::Elem(self.add(&a, &b)),
                        _ => {
                            return Err(Error::invalid(
                                "a sum of a scalar and a diagram: write the scalar times an \
                                 idempotent, as in 2 e(i@0)",
                            ));
                        }
                    });
                }
                Ok(acc.unwrap_or(Val::Elem(Elem::default())))
            }
            Ast::Id(_) => Err(Error::invalid(
                "in a klrw context, write the identity of a loading as e(i@0 j@1)",
            )),
            Ast::Tensor(items) => {
                if below.is_some() {
                    return Err(Error::invalid(
                        "in klrw each factor of a tensor product is a whole term with its \
                         loadings, so nothing can lie below it: compose on top of it instead",
                    ));
                }
                let mut scalar = self.coeff().one();
                let mut acc: Option<Elem<C<R>>> = None;
                for it in items {
                    match self.eval_in(it, None, defs, ints)? {
                        Val::Scalar(c) => scalar = self.coeff().mul(&scalar, &c),
                        Val::Elem(e) => {
                            acc = Some(match acc {
                                None => e,
                                Some(a) => self.tensor(&a, &e)?,
                            })
                        }
                    }
                }
                match acc {
                    Some(e) => attach(Val::Elem(self.scale(&scalar, &e))),
                    None => attach(Val::Scalar(scalar)),
                }
            }
            Ast::Name {
                name,
                sub,
                args: Some(args),
            } => {
                let full = format!("{name}_{}(...)", sub.as_deref().unwrap_or(""));
                let b = below.ok_or_else(|| Self::needs_below(&full))?;
                Ok(Val::Elem(self.eval_param(
                    name,
                    sub.as_deref().unwrap_or(""),
                    args,
                    b,
                    defs,
                    ints,
                )?))
            }
            Ast::PowExpr(b, e) => {
                let n = self.eval_int(e, ints)?;
                self.eval_in(&Ast::Pow(b.clone(), n), below, defs, ints)
            }
            Ast::Range { var, lo, hi, body } => {
                let (lo, hi) = (self.eval_int(lo, ints)?, self.eval_int(hi, ints)?);
                let mut inner = ints.to_vec();
                inner.push((var.clone(), 0));
                let mut items = Vec::new();
                for v in lo..=hi {
                    inner.last_mut().unwrap().1 = v;
                    items.push((false, self.eval_in(body, below, defs, &inner)?));
                }
                let mut acc: Option<Val<C<R>>> = None;
                for (_, v) in items {
                    acc = Some(match (acc, v) {
                        (None, v) => v,
                        (Some(Val::Scalar(a)), Val::Scalar(b)) => {
                            Val::Scalar(self.coeff().add(&a, &b))
                        }
                        (Some(Val::Elem(a)), Val::Elem(b)) => Val::Elem(self.add(&a, &b)),
                        (Some(Val::Scalar(a)), Val::Elem(b))
                        | (Some(Val::Elem(b)), Val::Scalar(a))
                            if self.coeff().is_zero(&a) =>
                        {
                            Val::Elem(b)
                        }
                        _ => {
                            return Err(Error::invalid(
                                "a sum of a scalar and a diagram: write the scalar times an \
                                 idempotent, as in 2 e(i@0)",
                            ));
                        }
                    });
                }
                Ok(acc.unwrap_or(Val::Elem(Elem::default())))
            }
        }
    }

    fn elem_of(&self, v: Val<C<R>>, below: &Elem<C<R>>) -> Result<Elem<C<R>>> {
        Ok(match v {
            Val::Elem(e) => e,
            Val::Scalar(c) => self.scale(&c, below),
        })
    }

    /// Evaluates a term to an element.
    pub fn term(&self, ast: &Ast, defs: &BTreeMap<String, Elem<C<R>>>) -> Result<Elem<C<R>>> {
        match self.eval(ast, None, defs)? {
            Val::Elem(e) => Ok(e),
            Val::Scalar(c) if self.coeff().is_zero(&c) => Ok(Elem::default()),
            Val::Scalar(_) => Err(Error::invalid(
                "a scalar alone is not an element: multiply it by an idempotent, as in 2 e(i@0)",
            )),
        }
    }

    /// The basis normal form: summands `c b(π; T) x^a e(B)`.
    pub fn normal_form(&self, e: &Elem<C<R>>) -> Result<Vec<Summand<C<R>>>> {
        let mut out = Vec::new();
        for p in e.pieces.values() {
            let rep = self.rep(p.bottom.n());
            let nf = rep.normal_form(&p.bottom, &p.top, &p.op)?;
            let mut piece = Vec::new();
            let mut perms: Vec<(&Perm, &Poly<C<R>>)> = nf.iter().collect();
            perms.sort_by(|a, b| b.0.length().cmp(&a.0.length()).then(a.0.cmp(b.0)));
            for (w, f) in perms {
                let mut terms: Vec<_> = f.terms.iter().collect();
                // higher degree first, then lexicographically larger exponents first
                terms.sort_by(|a, b| {
                    let da: i32 = a.0.0.iter().sum();
                    let db: i32 = b.0.0.iter().sum();
                    db.cmp(&da).then(b.0.0.cmp(&a.0.0))
                });
                for (m, c) in terms {
                    piece.push(Summand {
                        bottom: p.bottom.clone(),
                        top: p.top.clone(),
                        perm: w.clone(),
                        dots: m.0.iter().map(|&x| x as u32).collect(),
                        coeff: c.clone(),
                    });
                }
            }
            if self.cyclo.is_some() {
                piece = self.reduce_cyclotomic(piece)?;
                // the order of the normal form: longer permutations first, then higher degree
                piece.sort_by(|a, b| {
                    let da: u32 = a.dots.iter().sum();
                    let db: u32 = b.dots.iter().sum();
                    b.perm
                        .length()
                        .cmp(&a.perm.length())
                        .then(a.perm.cmp(&b.perm))
                        .then(db.cmp(&da))
                        .then(b.dots.cmp(&a.dots))
                });
            }
            out.extend(piece);
        }
        Ok(out)
    }

    /// The diagram of a summand, without its coefficient: `b(π; T) x^a e(B)`.
    pub fn fmt_word(&self, s: &Summand<C<R>>) -> String {
        let mut parts = Vec::new();
        if !s.perm.is_identity() {
            let ent: Vec<String> = s.perm.0.iter().map(|&k| (k + 1).to_string()).collect();
            parts.push(format!(
                "b({}; {})",
                ent.join(" "),
                self.fmt_loading(&s.top)
            ));
        } else if !self.data.isotopic(&s.bottom, &s.top) {
            parts.push(format!("to({})", self.fmt_loading(&s.top)));
        }
        for (k, &a) in s.dots.iter().enumerate() {
            match a {
                0 => {}
                1 => parts.push(format!("x_{}", k + 1)),
                a => parts.push(format!("x_{}^{a}", k + 1)),
            }
        }
        parts.push(format!("e({})", self.fmt_loading(&s.bottom)));
        parts.join(" ")
    }

    /// `(negative, coefficient text unless it is 1)`.
    pub(crate) fn fmt_coeff(&self, c: &C<R>) -> (bool, Option<String>) {
        let r = self.coeff();
        let neg = r.is_negative(c);
        let abs = if neg { r.neg(c) } else { c.clone() };
        let t = (!r.is_one(&abs)).then(|| {
            let s = r.fmt_elem(&abs);
            if r.needs_parens(&abs) {
                format!("({s})")
            } else {
                s
            }
        });
        (neg, t)
    }

    pub fn fmt_summands(&self, nf: &[Summand<C<R>>]) -> String {
        if nf.is_empty() {
            return "0".into();
        }
        let mut s = String::new();
        for (k, t) in nf.iter().enumerate() {
            let (neg, c) = self.fmt_coeff(&t.coeff);
            if k == 0 {
                if neg {
                    s.push('-');
                }
            } else {
                s.push_str(if neg { " - " } else { " + " });
            }
            if let Some(c) = c {
                s.push_str(&c);
                s.push(' ');
            }
            s.push_str(&self.fmt_word(t));
        }
        s
    }

    /// The degree of a summand: its basis diagram's events, plus `2 d_i` for each dot.
    pub fn degree(&self, s: &Summand<C<R>>) -> Result<i64> {
        let seg = self.data.segment(&s.bottom, &s.top, &s.perm)?;
        let colours = s.bottom.colours();
        let mut d: i64 = seg
            .events
            .iter()
            .map(|e| self.data.event_degree(&colours, e))
            .sum();
        for (k, &a) in s.dots.iter().enumerate() {
            d += 2 * self.data.d[colours[k] as usize] * a as i64;
        }
        Ok(d)
    }

    pub fn summands_json(&self, nf: &[Summand<C<R>>], shown: usize) -> Result<Value> {
        nf.iter()
            .take(shown)
            .map(|s| {
                let (neg, c) = self.fmt_coeff(&s.coeff);
                let single = self.fmt_summands(std::slice::from_ref(s));
                Ok(json!({
                    "text": single,
                    "word": self.fmt_word(s),
                    "degree": self.degree(s)?,
                    "source": self.fmt_loading(&s.bottom),
                    "target": self.fmt_loading(&s.top),
                    "negative": neg,
                    "coefficient": c,
                    "svg": self.svg(std::slice::from_ref(s), false),
                }))
            })
            .collect::<Result<Vec<_>>>()
            .map(Value::Array)
    }

    /// What `context.describe` reports: the colours, the edges with their weights, and the red
    /// strands. The generators are the dot and the crossing, which the canvas inserts.
    pub fn describe(&self, spec: &ContextSpec) -> Result<Value> {
        let name = |c: u16| self.klr.colours[c as usize].name.clone();
        Ok(json!({
            "context": spec.print(),
            "presentation": "klrw",
            "title": "Weighted KLR algebra",
            "ring": spec.ring,
            "colours": self.klr.colours.iter().map(|c| json!({ "name": c.name, "odd": false, "oriented": false })).collect::<Vec<_>>(),
            "objects": Vec::<String>::new(),
            "generators": [
                json!({ "name": "x", "written": "x", "draw": "dot", "inputs": 1, "outputs": 1, "source": "a", "target": "a" }),
                json!({ "name": "psi", "written": "psi", "draw": "crossing", "inputs": 2, "outputs": 2, "source": "a b", "target": "b a" }),
            ],
            "relations": Vec::<String>::new(),
            "edges": self.data.edges.iter().map(|e| json!({
                "tail": name(e.tail),
                "head": name(e.head),
                "weight": fmt_q(&e.weight),
                "ghost_degree": e.ghost_degree,
            })).collect::<Vec<_>>(),
            "red": self.data.reds.iter().map(|r| json!({
                "position": fmt_q(&r.pos),
                "weight": r.exps,
            })).collect::<Vec<_>>(),
            "representations": [json!({
                "name": "weighted",
                "kind": "weighted-klr-basis",
                "certificate": self.prov.certificate.status,
            })],
            "scalars": spec.scalars,
            "provenance": self.provenance_json(),
        }))
    }

    pub fn provenance_json(&self) -> Value {
        json!({
            "strategy": self.prov.strategy.as_str(),
            "method": self.prov.method,
            "certificate": {
                "status": self.prov.certificate.status,
                "reference": self.prov.certificate.reference,
                "hypotheses": self.prov.certificate.hypotheses,
            }
        })
    }

    /// `simplify`: the normal form, as for every other context.
    pub fn simplify(&self, e: &Elem<C<R>>, shown: Option<usize>) -> Result<Value> {
        let nf = self.normal_form(e)?;
        let mut v = json!({
            "normal_form": self.fmt_summands(&nf),
            "terms": nf.len(),
            "certified_by": "certificate",
            "provenance": self.provenance_json(),
        });
        if let Some(n) = shown {
            v["summands"] = self.summands_json(&nf, n)?;
        }
        Ok(v)
    }

    pub fn check(&self, name: &str, args: &[Elem<C<R>>]) -> Result<Value> {
        let arity = |n: usize| {
            if args.len() == n {
                Ok(())
            } else {
                Err(Error::invalid(format!("{name} takes {n} argument(s)")))
            }
        };
        let should_vanish = match name {
            "equal" => {
                arity(2)?;
                self.add(&args[0], &self.neg(&args[1]))
            }
            "zero" => {
                arity(1)?;
                args[0].clone()
            }
            "idempotent" => {
                arity(1)?;
                self.add(&self.compose(&args[0], &args[0]), &self.neg(&args[0]))
            }
            "homogeneous" => {
                arity(1)?;
                let nf = self.normal_form(&args[0])?;
                let degs: std::collections::BTreeSet<i64> =
                    nf.iter().map(|s| self.degree(s)).collect::<Result<_>>()?;
                let ok = degs.len() <= 1;
                return Ok(self.verdict(ok, (!ok).then(|| format!("degrees {degs:?}"))));
            }
            "orthogonal" => {
                arity(2)?;
                let ab = self.normal_form(&self.compose(&args[0], &args[1]))?;
                if !ab.is_empty() {
                    return Ok(self.verdict(
                        false,
                        Some(format!("first ∘ second = {}", self.fmt_summands(&ab))),
                    ));
                }
                self.compose(&args[1], &args[0])
            }
            "central" => {
                arity(1)?;
                return self.central(&args[0]);
            }
            "primitive" => {
                arity(1)?;
                return self.primitive(&args[0]);
            }
            _ => return Err(Error::invalid(format!("unknown check '{name}'"))),
        };
        let nf = self.normal_form(&should_vanish)?;
        let witness = (!nf.is_empty()).then(|| self.fmt_summands(&nf));
        Ok(self.verdict(nf.is_empty(), witness))
    }

    /// `a ⊗ b`: `b` placed to the right of `a`, its positions shifted by the least integer that
    /// puts every point and ghost of `b` strictly right of every point and ghost of `a`. Defined
    /// without red strands: they belong to the context, so the result would lie in another
    /// algebra (the one with both lists of red strands), and in the cyclotomic quotient a
    /// strand killed in `b` for lying left of all its red strands need not be killed in `a ⊗ b`.
    pub fn tensor(&self, a: &Elem<C<R>>, b: &Elem<C<R>>) -> Result<Elem<C<R>>> {
        if !self.data.reds.is_empty() || self.cyclo.is_some() {
            return Err(Error::precondition(
                "a tensor product of klrw diagrams is defined without red strands: the red \
                 strands belong to the context, so a ⊗ b would lie in the algebra with both \
                 lists of red strands (write that context and the juxtaposed term directly)",
            ));
        }
        let (na, nb) = (self.normal_form(a)?, self.normal_form(b)?);
        // the reach of a point of colour c: itself and the ghosts it casts
        let reach = |c: u16, p: &Q| -> (Q, Q) {
            let (mut lo, mut hi) = (p.clone(), p.clone());
            for e in self.data.edges.iter().filter(|e| e.head == c) {
                let g = p + &e.weight;
                if g < lo {
                    lo = g.clone();
                }
                if g > hi {
                    hi = g;
                }
            }
            (lo, hi)
        };
        let points = |nf: &[Summand<C<R>>]| -> Vec<(u16, Q)> {
            nf.iter()
                .flat_map(|s| s.bottom.0.iter().chain(&s.top.0).cloned())
                .collect()
        };
        let hi_a = points(&na).iter().map(|(c, p)| reach(*c, p).1).max();
        let lo_b = points(&nb).iter().map(|(c, p)| reach(*c, p).0).min();
        let shift = match (hi_a, lo_b) {
            (Some(h), Some(l)) => (h - l).floor() + Q::from_integer(1.into()),
            _ => Q::from_integer(0.into()),
        };
        let shifted = |l: &Loading| Loading(l.0.iter().map(|(c, p)| (*c, p + &shift)).collect());
        let mut out: Option<Elem<C<R>>> = None;
        for x in &na {
            for y in &nb {
                let n = x.bottom.n();
                let mut bottom = x.bottom.clone();
                bottom.0.extend(shifted(&y.bottom).0);
                let mut top = x.top.clone();
                top.0.extend(shifted(&y.top).0);
                let mut perm = x.perm.clone();
                perm.0.extend(y.perm.0.iter().map(|&k| k + n as u8));
                let mut dots = x.dots.clone();
                dots.extend(y.dots.iter().copied());
                let s = Summand {
                    bottom,
                    top,
                    perm,
                    dots,
                    coeff: self.coeff().mul(&x.coeff, &y.coeff),
                };
                let e = self.summand_elem(&s)?;
                out = Some(match out {
                    None => e,
                    Some(o) => self.add(&o, &e),
                });
            }
        }
        Ok(out.unwrap_or_default())
    }

    /// The element of a normal-form summand.
    pub(crate) fn summand_elem(&self, s: &Summand<C<R>>) -> Result<Elem<C<R>>> {
        let mut cur = self.identity(&s.bottom);
        for (k, &m) in s.dots.iter().enumerate() {
            for _ in 0..m {
                cur = self.dot(&cur, k + 1)?;
            }
        }
        cur = self.straight(&cur, &s.top, Some(&s.perm))?;
        Ok(self.scale(&s.coeff, &cur))
    }

    /// `central`: `z` commutes with every idempotent, and, at every order type of loadings with
    /// the colours of `z`, with the dots, the crossings `psi_k`, and the straight-line diagrams
    /// to every other order type, which generate the algebra.
    fn central(&self, z: &Elem<C<R>>) -> Result<Value> {
        let nf = self.normal_form(z)?;
        if let Some(s) = nf
            .iter()
            .find(|s| self.data.order_type(&s.bottom) != self.data.order_type(&s.top))
        {
            return Ok(self.verdict(
                false,
                Some(format!(
                    "{} does not commute with the idempotents",
                    self.fmt_word(s)
                )),
            ));
        }
        let mut contents: Vec<Vec<u16>> = nf
            .iter()
            .map(|s| {
                let mut c = s.bottom.colours();
                c.sort();
                c
            })
            .collect();
        contents.sort();
        contents.dedup();
        for colours in contents {
            let types = self.order_types(&colours);
            for a in &types {
                let ea = self.identity(a);
                let n = a.n();
                let mut gens: Vec<(String, Elem<C<R>>)> = Vec::new();
                for k in 1..=n {
                    gens.push((
                        format!("x_{k} e({})", self.fmt_loading(a)),
                        self.dot(&ea, k)?,
                    ));
                }
                for k in 1..n {
                    if let Ok(g) = self.psi(&ea, k) {
                        gens.push((format!("psi_{k} e({})", self.fmt_loading(a)), g));
                    }
                }
                for b in &types {
                    if b != a && b.colours() == a.colours() {
                        gens.push((
                            format!("to({}) e({})", self.fmt_loading(b), self.fmt_loading(a)),
                            self.straight(&ea, b, None)?,
                        ));
                    }
                }
                for (name, g) in gens {
                    let comm = self.add(&self.compose(z, &g), &self.neg(&self.compose(&g, z)));
                    let c = self.normal_form(&comm)?;
                    if !c.is_empty() {
                        return Ok(self.verdict(
                            false,
                            Some(format!(
                                "with {name}: z g - g z = {}",
                                self.fmt_summands(&c)
                            )),
                        ));
                    }
                }
            }
        }
        Ok(self.verdict(true, None))
    }

    /// `primitive`, in a cyclotomic quotient (finite-dimensional): the idempotent `e` is
    /// primitive when `eAe` is local. Over a field of characteristic 0 the radical of a
    /// finite-dimensional algebra `B` is the radical of its trace form `(a, b) ↦ tr(L_{ab})`
    /// (Dickson's criterion), so `B / rad B` has the dimension of the rank of that form; `B` is
    /// local exactly when the rank is 1, the quotient of a split algebra then being the field.
    fn primitive(&self, e: &Elem<C<R>>) -> Result<Value> {
        if self.cyclo.is_none() {
            return Err(Error::precondition(
                "primitive needs a finite-dimensional algebra: give the context \
                 quotient=cyclotomic",
            ));
        }
        let ring = self.coeff().clone();
        let enf = self.normal_form(e)?;
        if enf.is_empty() {
            return Ok(self.verdict(false, Some("the element is zero".into())));
        }
        let ee = self.add(&self.compose(e, e), &self.neg(e));
        if !self.normal_form(&ee)?.is_empty() {
            return Err(Error::precondition("the element is not an idempotent"));
        }
        // the loadings of e, one per order type
        let mut loads: BTreeMap<OrderType, Loading> = BTreeMap::new();
        for s in &enf {
            for l in [&s.bottom, &s.top] {
                loads
                    .entry(self.data.order_type(l))
                    .or_insert_with(|| l.clone());
            }
        }
        // a basis of ⊕ e_b T^λ e_a over these loadings, indexed by its normal-form summands
        type Key = (OrderType, OrderType, Perm, Vec<u32>);
        let mut index: BTreeMap<Key, usize> = BTreeMap::new();
        let mut basis: Vec<Elem<C<R>>> = Vec::new();
        for a in loads.values() {
            for b in loads.values() {
                for d in self.graded_dimension(a, b)?.keys() {
                    let piece = self.ideal_piece(a, b, *d)?;
                    for j in piece.span.free_columns() {
                        let (perm, dots) = &piece.cols[j];
                        let s = Summand {
                            bottom: a.clone(),
                            top: b.clone(),
                            perm: perm.clone(),
                            dots: dots.clone(),
                            coeff: ring.one(),
                        };
                        index.insert(
                            (
                                self.data.order_type(a),
                                self.data.order_type(b),
                                perm.clone(),
                                dots.clone(),
                            ),
                            basis.len(),
                        );
                        basis.push(self.summand_elem(&s)?);
                    }
                }
            }
        }
        let dim = basis.len();
        let coords = |x: &Elem<C<R>>| -> Result<Vec<C<R>>> {
            let mut v = vec![ring.zero(); dim];
            for s in self.normal_form(x)? {
                let k = (
                    self.data.order_type(&s.bottom),
                    self.data.order_type(&s.top),
                    s.perm.clone(),
                    s.dots.clone(),
                );
                let i = index
                    .get(&k)
                    .ok_or_else(|| Error::invalid("internal: a summand outside the basis"))?;
                v[*i] = ring.add(&v[*i], &s.coeff);
            }
            Ok(v)
        };
        // eAe: the span of e b e
        let mut span = crate::linalg::Reducer::new(ring.clone(), dim);
        for b in &basis {
            span.insert(coords(&self.compose(e, &self.compose(b, e)))?);
        }
        let rows = span.basis();
        let pivots = span.pivots();
        let m = rows.len();
        // the elements of the basis of eAe, and coordinates in it (rows are reduced)
        let elems: Vec<Elem<C<R>>> = rows
            .iter()
            .map(|r| {
                let mut acc = Elem::default();
                for (i, c) in r.iter().enumerate() {
                    if !ring.is_zero(c) {
                        acc = self.add(&acc, &self.scale(c, &basis[i]));
                    }
                }
                acc
            })
            .collect();
        let local = |x: &Elem<C<R>>| -> Result<Vec<C<R>>> {
            let v = coords(x)?;
            (0..m)
                .map(|k| {
                    ring.div_exact(&v[pivots[k]], &rows[k][pivots[k]])
                        .ok_or_else(|| Error::invalid("internal: a pivot does not divide"))
                })
                .collect()
        };
        // left multiplication matrices: lm[k][j][i] = coordinate i of x_k x_j
        let mut lm: Vec<Vec<Vec<C<R>>>> = Vec::with_capacity(m);
        for x in &elems {
            lm.push(
                elems
                    .iter()
                    .map(|y| local(&self.compose(x, y)))
                    .collect::<Result<_>>()?,
            );
        }
        // the trace form tr(L_k L_l) = Σ_{i,j} L_k[i][j] L_l[j][i]
        let mut gram = crate::linalg::Reducer::new(ring.clone(), m);
        for k in 0..m {
            let row: Vec<C<R>> = (0..m)
                .map(|l| {
                    let mut t = ring.zero();
                    for (i, row) in lm[l].iter().enumerate() {
                        for (j, x) in row.iter().enumerate() {
                            t = ring.add(&t, &ring.mul(&lm[k][j][i], x));
                        }
                    }
                    t
                })
                .collect();
            gram.insert(row);
            if gram.rank() > 1 {
                break;
            }
        }
        let ok = gram.rank() == 1;
        let witness = (!ok).then(|| {
            format!("e A e has dimension {m} and e A e / e J e has dimension greater than 1")
        });
        Ok(self.verdict(ok, witness))
    }

    fn verdict(&self, holds: bool, witness: Option<String>) -> Value {
        json!({
            "holds": holds,
            "witness": witness,
            "certified_by": "certificate",
            "provenance": self.provenance_json(),
        })
    }

    /// A drawing of summands, each its straight-line basis diagram with dots at the bottom.
    /// Ghosts are dashed lines of class `ghost` (a page may hide them without moving anything),
    /// red strands thick lines of class `red`.
    pub fn svg(&self, nf: &[Summand<C<R>>], signs: bool) -> String {
        const M: f64 = 12.0;
        const H: f64 = 3.0;
        let f = |x: &Q| -> f64 {
            let (n, d) = (x.numer().to_string(), x.denom().to_string());
            n.parse::<f64>().unwrap_or(0.0) / d.parse::<f64>().unwrap_or(1.0)
        };
        let num = |x: f64| -> String {
            let s = format!("{x:.2}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s == "-0" { "0".into() } else { s.into() }
        };
        let mut body = String::new();
        let mut x0 = M;
        const U: f64 = 24.0;
        let base = M + H * U;
        if nf.is_empty() {
            body.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"14\">0</text>\n",
                num(x0),
                num(base - H * U / 2.0)
            ));
            x0 += 12.0;
        }
        for (k, s) in nf.iter().enumerate() {
            if signs {
                let (neg, c) = self.fmt_coeff(&s.coeff);
                let mut lead = String::new();
                if k > 0 || neg {
                    lead.push_str(if neg { "\u{2212} " } else { "+ " });
                }
                if let Some(c) = c {
                    lead.push_str(&c);
                }
                if !lead.is_empty() {
                    body.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" font-size=\"14\" font-family=\"serif\">{}</text>\n",
                        num(x0),
                        num(base - H * U / 2.0 + 5.0),
                        lead.trim_end()
                            .replace('&', "&amp;")
                            .replace('<', "&lt;")
                            .replace('>', "&gt;")
                    ));
                    x0 += 8.0 * lead.chars().count() as f64 + 6.0;
                }
            }
            // horizontal extent: points, ghosts and red strands at both ends
            let mut xs: Vec<f64> = Vec::new();
            for l in [&s.bottom, &s.top] {
                for (c, x) in &l.0 {
                    xs.push(f(x));
                    for e in &self.data.edges {
                        if e.head == *c {
                            xs.push(f(&(x + &e.weight)));
                        }
                    }
                }
            }
            for r in &self.data.reds {
                xs.push(f(&r.pos));
            }
            let lo = xs.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let u = 24.0 * spread(&xs);
            let px = |x: f64| num(x0 + (x - lo) * u);
            let py = |y: f64| num(base - y * U);
            for r in &self.data.reds {
                let x = f(&r.pos);
                body.push_str(&format!(
                    "<path class=\"red\" d=\"M {} {} L {} {}\" stroke=\"#c0392b\" stroke-width=\"3\"/>\n",
                    px(x),
                    py(0.0),
                    px(x),
                    py(H)
                ));
            }
            for (j, (c, xb)) in s.bottom.0.iter().enumerate() {
                let xt = &s.top.0[s.perm.0[j] as usize].1;
                let (a, b) = (f(xb), f(xt));
                for e in &self.data.edges {
                    if e.head == *c {
                        let w = f(&e.weight);
                        body.push_str(&format!(
                            "<path class=\"ghost k{c}\" d=\"M {} {} L {} {}\" stroke-dasharray=\"4 3\" stroke-width=\"1\"/>\n",
                            px(a + w),
                            py(0.0),
                            px(b + w),
                            py(H)
                        ));
                    }
                }
                body.push_str(&format!(
                    "<path class=\"k{c}\" d=\"M {} {} L {} {}\"/>\n",
                    px(a),
                    py(0.0),
                    px(b),
                    py(H)
                ));
                let dots = s.dots[j];
                if dots > 0 {
                    let t = 0.35;
                    let (cx, cy) = (a + (b - a) * t / H, t);
                    body.push_str(&format!(
                        "<circle class=\"k{c}\" cx=\"{}\" cy=\"{}\" r=\"3\" fill=\"currentColor\"/>\n",
                        px(cx),
                        py(cy)
                    ));
                    if dots > 1 {
                        body.push_str(&format!(
                            "<text class=\"k{c}\" x=\"{}\" y=\"{}\" font-size=\"10\" stroke=\"none\" fill=\"currentColor\">{dots}</text>\n",
                            num(x0 + (cx - lo) * u + 5.0),
                            num(base - cy * U + 3.5)
                        ));
                    }
                }
                body.push_str(&format!(
                    "<text class=\"k{c}\" x=\"{}\" y=\"{}\" font-size=\"11\" font-style=\"italic\" text-anchor=\"middle\" stroke=\"none\" fill=\"currentColor\">{}</text>\n",
                    px(a),
                    num(base + 13.0),
                    self.klr.colours[*c as usize].name
                ));
            }
            x0 += (hi - lo).max(0.0) * u + 18.0;
        }
        let width = x0 - 6.0 + M;
        let height = H * U + 2.0 * M + 14.0;
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\">\n{body}</svg>\n",
            w = num(width),
            h = num(height)
        )
    }

    /// The summands as TikZ pictures, laid out as [`Klrw::svg`] lays them out: straight
    /// strands from the bottom loading to the top one, dots near the bottom, ghosts dashed
    /// and red strands thick and red. One unit of position is one unit of `x`.
    pub fn tikz(&self, nf: &[Summand<C<R>>]) -> String {
        const H: f64 = 3.0;
        let f = |x: &Q| -> f64 {
            let (n, d) = (x.numer().to_string(), x.denom().to_string());
            n.parse::<f64>().unwrap_or(0.0) / d.parse::<f64>().unwrap_or(1.0)
        };
        let num = |x: f64| -> String {
            let s = format!("{x:.3}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s == "-0" { "0".into() } else { s.into() }
        };
        if nf.is_empty() {
            return "0".into();
        }
        let mut out = String::new();
        for (k, s) in nf.iter().enumerate() {
            let (neg, c) = self.fmt_coeff(&s.coeff);
            if k == 0 {
                if neg {
                    out.push('-');
                }
            } else {
                out.push_str(if neg { "\n-\n" } else { "\n+\n" });
            }
            if let Some(c) = c {
                out.push_str(c.trim_end());
                out.push_str("\\;");
            }
            // the same horizontal stretch as the drawings in the document
            let mut xs: Vec<f64> = Vec::new();
            for l in [&s.bottom, &s.top] {
                for (c, x) in &l.0 {
                    xs.push(f(x));
                    for e in self.data.edges.iter().filter(|e| e.head == *c) {
                        xs.push(f(&(x + &e.weight)));
                    }
                }
            }
            xs.extend(self.data.reds.iter().map(|r| f(&r.pos)));
            out.push_str(&format!(
                "\\begin{{tikzpicture}}[x={}cm,y=0.5cm,baseline={{([yshift=-.5ex]current bounding box.center)}}]\n",
                num(0.5 * spread(&xs))
            ));
            for r in &self.data.reds {
                let x = num(f(&r.pos));
                out.push_str(&format!(
                    "\\draw[red,very thick] ({x},0) -- ({x},{});\n",
                    num(H)
                ));
            }
            for (j, (c, xb)) in s.bottom.0.iter().enumerate() {
                let xt = &s.top.0[s.perm.0[j] as usize].1;
                let (a, b) = (f(xb), f(xt));
                for e in &self.data.edges {
                    if e.head == *c {
                        let w = f(&e.weight);
                        out.push_str(&format!(
                            "\\draw[dashed,gray] ({},0) -- ({},{});\n",
                            num(a + w),
                            num(b + w),
                            num(H)
                        ));
                    }
                }
                out.push_str(&format!(
                    "\\draw ({},0) -- ({},{});\n",
                    num(a),
                    num(b),
                    num(H)
                ));
                let dots = s.dots[j];
                if dots > 0 {
                    let t = 0.35;
                    let (cx, cy) = (num(a + (b - a) * t / H), num(t));
                    out.push_str(&format!("\\fill ({cx},{cy}) circle (2pt);"));
                    if dots > 1 {
                        out.push_str(&format!(
                            " \\node[right=2.5pt,inner sep=0pt] at ({cx},{cy}) {{$\\scriptstyle {dots}$}};"
                        ));
                    }
                    out.push('\n');
                }
                out.push_str(&format!(
                    "\\node[below] at ({},0) {{$\\scriptstyle {}$}};\n",
                    num(a),
                    self.klr.colours[*c as usize].name
                ));
            }
            out.push_str("\\end{tikzpicture}");
        }
        out.push('\n');
        out
    }
}

/// `workspace.run` in a `klrw` context: definitions, watches and checks, in order.
/// One set of extra scalars of a workspace: its context and the definitions elaborated in it.
struct Scoped<R: Ring> {
    k: Klrw<R>,
    terms: BTreeMap<usize, Result<Elem<C<R>>>>,
}

impl<R: Ring> Scoped<R> {
    fn new(ws: &crate::workspace::Workspace, base: &R, extra: &[&String]) -> Result<Self> {
        let mut spec = ws.context.clone();
        for u in extra {
            if !spec.scalars.contains(u) {
                spec.scalars.push((*u).clone());
            }
        }
        Ok(Scoped {
            k: Klrw::new(&spec, base.clone())?,
            terms: BTreeMap::new(),
        })
    }

    /// The definitions cell `i` uses, by name.
    fn bound(
        &mut self,
        ws: &crate::workspace::Workspace,
        deps: &crate::ops::Dependencies,
        i: usize,
    ) -> Result<BTreeMap<String, Elem<C<R>>>> {
        let mut defs = BTreeMap::new();
        for &j in &deps.defs[i] {
            let t = self.def(ws, deps, j)?;
            defs.insert(ws.cells[j].defines().unwrap_or_default().to_string(), t);
        }
        Ok(defs)
    }

    /// The element of the definition in cell `j`.
    fn def(
        &mut self,
        ws: &crate::workspace::Workspace,
        deps: &crate::ops::Dependencies,
        j: usize,
    ) -> Result<Elem<C<R>>> {
        if let Some(t) = self.terms.get(&j) {
            return t.clone();
        }
        let crate::workspace::CellBody::Def { name, term } = &ws.cells[j].body else {
            return Err(Error::invalid("not a definition"));
        };
        let t = self
            .bound(ws, deps, j)
            .and_then(|defs| self.k.term(term, &defs))
            .map_err(|e| Error {
                message: format!("in the definition of {name}: {}", e.message),
                ..e
            });
        self.terms.insert(j, t.clone());
        t
    }
}

/// `workspace.run` in a klrw context. As for the other contexts, each cell runs with the
/// unknowns of solve cells that it, or the definitions it uses, mention as extra scalars.
pub fn run_workspace<R: Ring>(
    ws: &crate::workspace::Workspace,
    base: R,
    shown: Option<usize>,
    scalars: &[String],
) -> Result<Vec<crate::ops::CellResult>> {
    use crate::workspace::CellBody;
    let deps = crate::ops::Dependencies::of(ws);
    let mut sets: Vec<Vec<String>> = Vec::new();
    let mut scoped: Vec<Scoped<R>> = Vec::new();
    let mut out = Vec::new();
    for (i, cell) in ws.cells.iter().enumerate() {
        if matches!(
            cell.body,
            CellBody::Note { .. } | CellBody::Comment(_) | CellBody::Blank
        ) {
            continue;
        }
        let set = &deps.unknowns[i];
        let w = match sets.iter().position(|s| s == set) {
            Some(w) => w,
            None => {
                let extra: Vec<&String> = scalars.iter().chain(set).collect();
                scoped.push(Scoped::new(ws, &base, &extra)?);
                sets.push(set.clone());
                sets.len() - 1
            }
        };
        let sc = &mut scoped[w];
        let (kind, r): (&'static str, Result<Value>) = match &cell.body {
            CellBody::Def { name, .. } => (
                "def",
                sc.def(ws, &deps, i).and_then(|e| {
                    let nf = sc.k.normal_form(&e)?;
                    // terms are kept in the basis normal form (and reduced in the quotient), so
                    // a definition's value rests on the same certificate as a watch
                    let mut v = json!({
                        "name": name,
                        "term": sc.k.fmt_summands(&nf),
                        "terms": nf.len(),
                        "provenance": sc.k.provenance_json(),
                    });
                    if let Some(n) = shown {
                        v["summands"] = sc.k.summands_json(&nf, n)?;
                    }
                    Ok(v)
                }),
            ),
            CellBody::Watch { term } => (
                "watch",
                sc.bound(ws, &deps, i)
                    .and_then(|defs| sc.k.term(term, &defs))
                    .and_then(|e| {
                        let mut v = sc.k.simplify(&e, shown)?;
                        v["term"] = json!(crate::text::print(term));
                        Ok(v)
                    }),
            ),
            CellBody::Check { name, args } => (
                "check",
                sc.bound(ws, &deps, i).and_then(|defs| {
                    args.iter()
                        .map(|a| sc.k.term(a, &defs))
                        .collect::<Result<Vec<_>>>()
                        .and_then(|ts| sc.k.check(name, &ts))
                }),
            ),
            CellBody::Solve { lhs, rhs, unknowns } => ("solve", {
                let bound = sc.bound(ws, &deps, i);
                let k = &sc.k;
                bound.and_then(|defs| {
                    let d = k.add(&k.term(lhs, &defs)?, &k.neg(&k.term(rhs, &defs)?));
                    let nf = k.normal_form(&d)?;
                    let mut v = crate::solve::solve_coefficients(
                        k.coeff(),
                        &ws.context.ring,
                        nf.iter().map(|s| &s.coeff),
                        unknowns,
                        &crate::groebner::Limits::default(),
                    )?;
                    v["provenance"] = k.provenance_json();
                    Ok(v)
                })
            }),
            CellBody::Complex { complex, .. } => {
                ("complex", crate::ops_complex::complex_cell(&sc.k, complex))
            }
            CellBody::Note { .. } | CellBody::Comment(_) | CellBody::Blank => unreachable!(),
        };
        let mut value = crate::ops::with_status(r);
        if let Some(o) = value.as_object_mut()
            && let Some(kd) = o.remove("kind")
        {
            o.insert("error_kind".into(), kd);
        }
        out.push(crate::ops::CellResult {
            id: cell.id.clone(),
            kind,
            value,
        });
    }
    Ok(out)
}

/// `workspace.trace` in a klrw context, as for the other contexts: each summand of each
/// definition a watch uses gets its own tag scalar, and the coefficient of the traced summand
/// of the watch is split by its monomials in the tags.
pub fn trace_workspace<R: Ring>(
    ws: &crate::workspace::Workspace,
    base: R,
    scalars: &[String],
    cell: &str,
    word: &str,
) -> Result<Value> {
    use crate::workspace::CellBody;
    let deps = crate::ops::Dependencies::of(ws);
    let i = ws
        .cells
        .iter()
        .position(|c| c.id.as_deref() == Some(cell))
        .ok_or_else(|| Error::invalid(format!("there is no cell '{cell}'")))?;
    let CellBody::Watch { term } = &ws.cells[i].body else {
        return Err(Error::invalid("only the summands of a watch are traced"));
    };
    let mut direct = deps.defs[i].clone();
    direct.sort_unstable();
    direct.dedup();
    let extra: Vec<&String> = scalars.iter().chain(&deps.unknowns[i]).collect();
    let mut plain = Scoped::new(ws, &base, &extra)?;
    let mut written: Vec<(usize, Vec<Summand<C<R>>>)> = Vec::new();
    for &j in &direct {
        let e = plain.def(ws, &deps, j)?;
        written.push((j, plain.k.normal_form(&e)?));
    }
    let count: usize = written.iter().map(|(_, t)| t.len()).sum();
    if count > crate::ops::TRACE_TERMS {
        return Err(Error::limit(format!(
            "the definitions this watch uses have {count} terms; tracing follows at most {}",
            crate::ops::TRACE_TERMS
        )));
    }
    let first_tag = plain.k.klr.spec.scalars.len();
    let tags: Vec<String> = (0..count).map(|k| format!("trace__{k}")).collect();
    let mut all = extra.clone();
    all.extend(tags.iter());
    let mut tagged_sc = Scoped::new(ws, &base, &all)?;
    let mut tagged = BTreeMap::new();
    let mut labels: Vec<Value> = Vec::new();
    let mut n = 0;
    for (j, nf) in &written {
        let name = ws.cells[*j].defines().unwrap_or_default().to_string();
        // the definition, recomputed with the tags present, so its coefficients live here
        let e = tagged_sc.def(ws, &deps, *j)?;
        let k = &tagged_sc.k;
        let mut sum: Option<Elem<C<R>>> = None;
        for (m, s) in k.normal_form(&e)?.iter().enumerate() {
            let tag = k.coeff().var(first_tag + n);
            let mut s = s.clone();
            s.coeff = k.coeff().mul(&s.coeff, &tag);
            let el = k.summand_elem(&s)?;
            sum = Some(match sum {
                Some(a) => k.add(&a, &el),
                None => el,
            });
            labels.push(json!({
                "name": name,
                "cell": ws.cells[*j].id,
                "term": m + 1,
                "text": plain.k.fmt_summands(std::slice::from_ref(&nf[m.min(nf.len() - 1)])),
            }));
            n += 1;
        }
        if let Some(sum) = sum {
            tagged.insert(name, sum);
        } else {
            tagged.insert(name, e);
        }
    }
    let k = &tagged_sc.k;
    let nf = k.normal_form(&k.term(term, &tagged)?)?;
    let Some(target) = nf.iter().find(|s| k.fmt_word(s) == word) else {
        return Err(Error::invalid(format!(
            "the value of {cell} has no summand {word}"
        )));
    };
    let mut parts: BTreeMap<Vec<i32>, Vec<(crate::poly::Monomial, R::Elem)>> = BTreeMap::new();
    let mut total = Vec::new();
    for (m, a) in &target.coeff.terms {
        let tg: Vec<i32> = m.0[first_tag..].to_vec();
        let mut rest = m.clone();
        for e in rest.0[first_tag..].iter_mut() {
            *e = 0;
        }
        total.push((rest.clone(), a.clone()));
        parts.entry(tg).or_default().push((rest, a.clone()));
    }
    let text = |c: C<R>| {
        let mut s = target.clone();
        s.coeff = c;
        k.fmt_summands(std::slice::from_ref(&s))
    };
    let sources: Vec<Value> = parts
        .into_iter()
        .rev()
        .map(|(tg, terms)| {
            let factors: Vec<Value> = tg
                .iter()
                .enumerate()
                .filter(|(_, e)| **e > 0)
                .map(|(t, e)| {
                    let mut v = labels[t].clone();
                    v["power"] = json!(e);
                    v
                })
                .collect();
            json!({ "factors": factors, "text": text(k.coeff().normalize(terms)) })
        })
        .collect();
    Ok(json!({
        "word": word,
        "text": text(k.coeff().normalize(total)),
        "sources": sources,
    }))
}

/// How much a drawing stretches positions horizontally so that no two of `xs` (strands, ghosts,
/// and red strands) are drawn closer than 0.6 of a unit: 1 when they are already that far apart,
/// at most 4. Only the drawing stretches; positions in text and TikZ are as written.
fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

pub(crate) fn spread(xs: &[f64]) -> f64 {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    v.sort_by(f64::total_cmp);
    let gap = v
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|g| *g > 1e-9)
        .fold(f64::INFINITY, f64::min);
    if gap < 0.6 { (0.6 / gap).min(4.0) } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring::Rationals;

    fn ctx(s: &str) -> Klrw<Rationals> {
        Klrw::new(&ContextSpec::parse(s).unwrap(), Rationals).unwrap()
    }

    fn nf(k: &Klrw<Rationals>, t: &str) -> String {
        let e = k
            .term(&crate::text::parse(t).unwrap(), &BTreeMap::new())
            .unwrap();
        k.fmt_summands(&k.normal_form(&e).unwrap())
    }

    #[test]
    fn weight_zero_bigon_is_q() {
        let k = ctx("klrw type=A2");
        // Q_ij(u, v) = u + v at weight 0, as for KLR
        assert_eq!(
            nf(&k, "psi_1 psi_1 e(i@0 j@1)"),
            "x_1 e(i@0 j@1) + x_2 e(i@0 j@1)"
        );
    }

    #[test]
    fn straight_line_to_other_colours_is_an_error() {
        let k = ctx("klrw type=A2");
        let err = |t: &str| {
            k.term(&crate::text::parse(t).unwrap(), &BTreeMap::new())
                .err()
                .map(|e| e.message)
                .unwrap_or_default()
        };
        assert!(err("to(j@0) e(i@0)").contains("no part of it has the same colours"));
        assert!(err("psi_2 e(i@0)").contains("has 1 point"));
        // a sum keeps the parts that continue
        assert_eq!(nf(&k, "to(i@0) (e(i@0) + e(j@0))"), "e(i@0)");
    }

    #[test]
    fn ghost_bigon() {
        // j is the head of the edge i -> j of weight 1/2: its ghost sits 1/2 to the right
        let k = ctx("klrw type=A2 theta_ij=1/2");
        // i at 1 passes the ghost of j (at 1/2) and comes back: the bigon is Q
        let t = "to(j@0 i@1/4) to(j@0 i@1) e(j@0 i@1/4)";
        assert_eq!(nf(&k, t), "x_1 e(j@0 i@1/4) + x_2 e(j@0 i@1/4)");
        // a strand passing a ghost from right to left and back is also the bigon
        let t = "to(j@0 i@1) to(j@0 i@1/4) e(j@0 i@1)";
        assert_eq!(nf(&k, t), "x_1 e(j@0 i@1) + x_2 e(j@0 i@1)");
        // crossing then dot: the dot slides to the bottom strand it lies on
        assert_eq!(
            nf(&k, "x_1 psi_1 e(i@0 j@2)"),
            "b(2 1; j@0 i@2) x_2 e(i@0 j@2)"
        );
    }

    #[test]
    fn q_calls_symbolic_powers_and_ranges() {
        let k = ctx("klrw type=A2");
        assert_eq!(
            nf(&k, "psi_1 psi_1 e(i@0 j@1) - Q_ij(x_1, x_2) e(i@0 j@1)"),
            "0"
        );
        assert_eq!(
            nf(&k, "sum_{f=0..2} x_1^{f} x_2^{2-f} e(i@0 j@1)"),
            nf(
                &k,
                "x_2^2 e(i@0 j@1) + x_1 x_2 e(i@0 j@1) + x_1^2 e(i@0 j@1)"
            )
        );
        assert_eq!(nf(&k, "sum_{f=1..0} x_1 e(i@0)"), "0");
    }

    fn chk(k: &Klrw<Rationals>, name: &str, ts: &[&str]) -> Value {
        let args: Vec<_> = ts
            .iter()
            .map(|t| {
                k.term(&crate::text::parse(t).unwrap(), &BTreeMap::new())
                    .unwrap()
            })
            .collect();
        k.check(name, &args).unwrap()
    }

    #[test]
    fn orthogonal_central_primitive() {
        let k = ctx("klrw type=A2");
        assert_eq!(
            chk(&k, "orthogonal", &["e(i@0 j@1)", "e(j@0 i@1)"])["holds"],
            true
        );
        assert_eq!(
            chk(&k, "orthogonal", &["e(i@0 j@1)", "e(i@0 j@1)"])["holds"],
            false
        );
        // one strand: everything commutes; with two strands a dot does not
        assert_eq!(chk(&k, "central", &["x_1 e(i@0)"])["holds"], true);
        assert_eq!(chk(&k, "central", &["x_1 e(i@0 i@1)"])["holds"], false);
        // the symmetric polynomial x_1 + x_2 on two strands of one colour is central
        assert_eq!(
            chk(&k, "central", &["x_1 e(i@0 i@1) + x_2 e(i@0 i@1)"])["holds"],
            true
        );
        let c = ctx("klrw type=A1 red=\"2i@0\" quotient=cyclotomic");
        // one strand right of the red strand: k[x]/(x^2) is local
        assert_eq!(chk(&c, "primitive", &["e(i@1)"])["holds"], true);
        // two strands: the cyclotomic nilHecke algebra is a matrix algebra of size 2
        assert_eq!(chk(&c, "primitive", &["e(i@1 i@2)"])["holds"], false);
        assert_eq!(
            chk(&c, "primitive", &["x_1 psi_1 e(i@1 i@2)"])["holds"],
            true
        );
        assert!(
            c.check(
                "primitive",
                &[
                    k.term(&crate::text::parse("x_1 e(i@0)").unwrap(), &BTreeMap::new())
                        .unwrap()
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn tensor_product_shifts_the_right_factor() {
        let k = ctx("klrw type=A2 theta_ij=1/2");
        // the left factor reaches 0 and the right one starts at 0, so it moves by 1
        assert_eq!(nf(&k, "(x_1 e(i@0)) (x) e(j@0)"), nf(&k, "x_1 e(i@0 j@1)"));
        // j is the head of the edge, so its ghost at 1/2 also counts: the shift is 1
        assert_eq!(
            nf(&k, "e(j@0) (x) (psi_1 e(i@0 i@1))"),
            nf(&k, "psi_2 e(j@0 i@1 i@2)")
        );
        let c = ctx("klrw type=A1 red=\"1i@0\"");
        let e = c.term(
            &crate::text::parse("e(i@1) (x) e(i@1)").unwrap(),
            &BTreeMap::new(),
        );
        assert!(e.is_err());
    }

    #[test]
    fn tikz_draws_strands_ghosts_and_red_strands() {
        let k = ctx("klrw type=A2 theta_ij=1/2 red=\"1i@0\"");
        let e = k
            .term(
                &crate::text::parse("2 x_1 e(j@1 i@2)").unwrap(),
                &BTreeMap::new(),
            )
            .unwrap();
        let t = k.tikz(&k.normal_form(&e).unwrap());
        assert!(t.starts_with("2\\;\\begin{tikzpicture}"), "{t}");
        assert!(t.contains("\\draw[red,very thick] (0,0) -- (0,3);"), "{t}");
        assert!(t.contains("\\draw[dashed,gray] (1.5,0) -- (1.5,3);"), "{t}");
        assert!(t.contains("\\fill (1,0.35) circle (2pt);"), "{t}");
        assert!(t.trim_end().ends_with("\\end{tikzpicture}"));
        assert_eq!(k.tikz(&[]), "0");
    }

    #[test]
    fn normal_form_round_trip() {
        let k = ctx("klrw type=A2 theta_ij=1/2");
        let t = "x_1 psi_1 e(i@0 j@2)";
        let s = nf(&k, t);
        assert_eq!(nf(&k, &s), s);
    }

    #[test]
    fn nilhecke_relation() {
        let k = ctx("klrw type=A1");
        // psi x_1 - x_2 psi = e on two strands of one colour
        let s = nf(&k, "psi_1 x_1 e(i@0 i@1) - x_2 psi_1 e(i@0 i@1)");
        assert_eq!(s, "e(i@0 i@1)");
    }

    #[test]
    fn cyclotomic_one_red_strand() {
        let k = ctx("klrw type=A1 red=\"2i@0\" quotient=cyclotomic");
        // a black strand left of every red strand is killed
        assert_eq!(nf(&k, "e(i@-1)"), "0");
        assert_eq!(nf(&k, "x_1 to(i@-1) e(i@1)"), "0");
        // one strand right of the red strand: k[x]/(x^2)
        assert_eq!(nf(&k, "x_1 e(i@1)"), "x_1 e(i@1)");
        assert_eq!(nf(&k, "x_1^2 e(i@1)"), "0");
        // two strands: the cyclotomic nilHecke algebra, where x_1 + x_2 = 0 and x_1 x_2 = 0
        assert_eq!(nf(&k, "x_1 e(i@1 i@2) + x_2 e(i@1 i@2)"), "0");
        assert_eq!(nf(&k, "x_1 x_2 e(i@1 i@2)"), "0");
        assert_ne!(nf(&k, "psi_1 e(i@1 i@2)"), "0");
        // without the quotient, nothing is killed
        let t = ctx("klrw type=A1 red=\"2i@0\"");
        assert_eq!(nf(&t, "x_1^2 e(i@1)"), "x_1^2 e(i@1)");
    }

    #[test]
    fn certificates() {
        let status = |s: &str| ctx(s).prov.certificate.status;
        assert_eq!(status("klrw type=A2"), "lean");
        assert_eq!(status("klrw type=A2 red=\"i@0\""), "lean");
        assert_eq!(
            status("klrw type=A2 red=\"i@0\" quotient=cyclotomic"),
            "lean"
        );
        assert_eq!(
            status("klrw type=A2 theta_ij=1/2 red=\"i@0\" quotient=cyclotomic"),
            "literature"
        );
    }

    #[test]
    fn cyclotomic_needs_a_field() {
        let spec = ContextSpec::parse("klrw type=A1 quotient=cyclotomic").unwrap();
        assert!(Klrw::new(&spec, crate::ring::Integers).is_err());
        // scalars are fine as long as Q does not involve them
        let spec = ContextSpec::parse("klrw type=A1 scalars=t quotient=cyclotomic").unwrap();
        assert!(Klrw::new(&spec, Rationals).is_ok());
        let spec = ContextSpec::parse("klrw type=A1 quotient=other").unwrap();
        assert!(Klrw::new(&spec, Rationals).is_err());
    }

    #[test]
    fn cyclotomic_two_red_strands() {
        // V(1) ⊗ V(1) for sl_2: one black strand
        let k = ctx("klrw type=A1 red=\"i@0, i@2\" quotient=cyclotomic");
        assert_eq!(nf(&k, "x_1 e(i@1)"), "0");
        assert_eq!(nf(&k, "x_1 e(i@3)"), "x_1 e(i@3)");
        assert_eq!(nf(&k, "x_1^2 e(i@3)"), "0");
        // through the second red strand and back is x, which survives
        assert_eq!(nf(&k, "to(i@3) to(i@1) e(i@3)"), "x_1 e(i@3)");
        // A2 with the ghost of j at x_j − 1/2 and λ = Λ_i: only i before the ghost of j
        // survives, and its endomorphisms are the scalars (weight Λ_i − α_i − α_j of V(Λ_i))
        let w = ctx("klrw type=A2 theta_ij=-1/2 red=\"i@0\" quotient=cyclotomic");
        assert_eq!(nf(&w, "e(i@1 j@3)"), "e(i@1 j@3)");
        assert_eq!(nf(&w, "x_1 e(i@1 j@3)"), "0");
        assert_eq!(nf(&w, "x_2 e(i@1 j@3)"), "0");
        assert_eq!(nf(&w, "e(j@1 i@2)"), "0");
        // with the ghost to the right instead, j can pass the red strand without crossing
        // its ghost through i, so every idempotent of this weight is violating
        let w = ctx("klrw type=A2 theta_ij=1/2 red=\"i@0\" quotient=cyclotomic");
        assert_eq!(nf(&w, "e(i@1 j@2)"), "0");
    }
}
