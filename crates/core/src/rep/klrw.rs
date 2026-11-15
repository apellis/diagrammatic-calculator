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
        for a in 0..n {
            for b in 0..a {
                if cartan.a[a][b] != 0 {
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
        let prov = Provenance {
            strategy: Strategy::BasisNormalForm,
            method: "the polynomial representation and the straight-line basis of a weighted \
                     KLR algebra (Webster, arXiv:1209.2463v4, Proposition 2.7 and Theorem 2.8)"
                .into(),
            certificate: Certificate {
                status: "literature".into(),
                reference: "B. Webster, Weighted Khovanov-Lauda-Rouquier algebras, \
                            arXiv:1209.2463v4, Theorem 2.8"
                    .into(),
                hypotheses: "coefficients in an integral domain; every Q_e nonzero".into(),
            },
        };
        Ok(Klrw { klr, data, prov })
    }

    fn coeff(&self) -> &PolyRing<R> {
        &self.klr.coeff
    }

    fn rep(&self, n: usize) -> PolyRep<'_, PolyRing<R>> {
        PolyRep::new(&self.data, self.klr.coeff.clone(), n)
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
        self.on_top(below, |rep, p| {
            let n = p.top.n();
            if to.n() != n {
                return Ok(None);
            }
            let id = Perm::identity(n);
            let perm = perm.unwrap_or(&id);
            if perm.n() != n {
                return Err(Error::invalid(format!(
                    "the permutation has {} entries but the loading below has {n} points",
                    perm.n()
                )));
            }
            if (0..n).any(|k| p.top.0[k].0 != to.0[perm.0[k] as usize].0) {
                // the colours do not match: this piece does not continue
                return Ok(None);
            }
            let seg = self.data.segment(&p.top, to, perm)?;
            Ok(Some(Piece {
                bottom: p.bottom.clone(),
                top: to.clone(),
                op: rep.apply_segment(&seg, &p.op)?,
            }))
        })
    }

    fn dot(&self, below: &Elem<C<R>>, k: usize) -> Result<Elem<C<R>>> {
        self.on_top(below, |rep, p| {
            if k == 0 || k > p.top.n() {
                return Err(Error::invalid(format!(
                    "x_{k}: the loading below has {} points",
                    p.top.n()
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
                    "psi_{k}: the loading below has {n} points"
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
                            cur = self.elem_of(self.eval(b, Some(&cur), defs)?, bl)?;
                        }
                        Ok(Val::Elem(cur))
                    }
                    None => match self.eval(b, None, defs)? {
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
                        Some(Val::Elem(e)) => self.eval(it, Some(e), defs)?,
                        _ => self.eval(it, None, defs)?,
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
                    let mut v = self.eval(it, below, defs)?;
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
            Ast::Tensor(_) => Err(Error::invalid(
                "a weighted KLR algebra has no tensor product of diagrams",
            )),
            Ast::Name { .. } | Ast::PowExpr(..) | Ast::Range { .. } => {
                Err(Error::invalid(format!(
                    "{} is not available in a klrw context",
                    crate::text::print(ast)
                )))
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
                    out.push(Summand {
                        bottom: p.bottom.clone(),
                        top: p.top.clone(),
                        perm: w.clone(),
                        dots: m.0.iter().map(|&x| x as u32).collect(),
                        coeff: c.clone(),
                    });
                }
            }
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
    fn fmt_coeff(&self, c: &C<R>) -> (bool, Option<String>) {
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
    /// strands. Terms are typed, not drawn generator by generator, so no generators are listed.
    pub fn describe(&self, spec: &ContextSpec) -> Result<Value> {
        let name = |c: u16| self.klr.colours[c as usize].name.clone();
        Ok(json!({
            "context": spec.print(),
            "presentation": "klrw",
            "title": "Weighted KLR algebra",
            "ring": spec.ring,
            "colours": self.klr.colours.iter().map(|c| json!({ "name": c.name, "odd": false, "oriented": false })).collect::<Vec<_>>(),
            "objects": Vec::<String>::new(),
            "generators": Vec::<Value>::new(),
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
            _ => {
                return Err(Error::invalid(format!(
                    "the check '{name}' is not available in a klrw context yet \
                     (equal, zero, idempotent, and homogeneous are)"
                )));
            }
        };
        let nf = self.normal_form(&should_vanish)?;
        let witness = (!nf.is_empty()).then(|| self.fmt_summands(&nf));
        Ok(self.verdict(nf.is_empty(), witness))
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
        const U: f64 = 24.0;
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
            let px = |x: f64| num(x0 + (x - lo) * U);
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
                            num(x0 + (cx - lo) * U + 5.0),
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
            x0 += (hi - lo).max(0.0) * U + 18.0;
        }
        let width = x0 - 6.0 + M;
        let height = H * U + 2.0 * M + 14.0;
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\">\n{body}</svg>\n",
            w = num(width),
            h = num(height)
        )
    }
}

/// `workspace.run` in a `klrw` context: definitions, watches and checks, in order.
pub fn run_workspace<R: Ring>(
    ws: &crate::workspace::Workspace,
    base: R,
    shown: Option<usize>,
) -> Result<Vec<crate::ops::CellResult>> {
    use crate::workspace::CellBody;
    let k = Klrw::new(&ws.context, base)?;
    let mut defs: BTreeMap<String, Elem<C<R>>> = BTreeMap::new();
    let mut out = Vec::new();
    for cell in &ws.cells {
        let (kind, r): (&'static str, Result<Value>) = match &cell.body {
            CellBody::Def { name, term } => (
                "def",
                k.term(term, &defs)
                    .map_err(|e| Error {
                        message: format!("in the definition of {name}: {}", e.message),
                        ..e
                    })
                    .and_then(|e| {
                        let nf = k.normal_form(&e)?;
                        let mut v = json!({
                            "name": name,
                            "term": k.fmt_summands(&nf),
                            "terms": nf.len(),
                        });
                        if let Some(n) = shown {
                            v["summands"] = k.summands_json(&nf, n)?;
                        }
                        defs.insert(name.clone(), e);
                        Ok(v)
                    }),
            ),
            CellBody::Watch { term } => (
                "watch",
                k.term(term, &defs).and_then(|e| {
                    let mut v = k.simplify(&e, shown)?;
                    v["term"] = json!(crate::text::print(term));
                    Ok(v)
                }),
            ),
            CellBody::Check { name, args } => (
                "check",
                args.iter()
                    .map(|a| k.term(a, &defs))
                    .collect::<Result<Vec<_>>>()
                    .and_then(|ts| k.check(name, &ts)),
            ),
            CellBody::Solve { .. } => (
                "solve",
                Err(Error::invalid(
                    "solve is not available in a klrw context yet",
                )),
            ),
            CellBody::Note { .. } | CellBody::Comment(_) | CellBody::Blank => continue,
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
}
