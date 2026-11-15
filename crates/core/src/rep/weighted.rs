//! Weighted KLR algebras (KLRW algebras) and their red-strand extensions: loadings, the
//! straight-line diagrams between them, the polynomial representation, and the basis normal
//! form. Conventions: `docs/klrw.md`.
//!
//! A diagram is a word of elementary *events* read from the bottom: dots, crossings of two black
//! strands, crossings of a black strand with the ghost of another, and crossings of a black
//! strand with a red strand. Its value is an operator on `k[x_1, …, x_n]`, written as in
//! [`super::smash`] as `Σ r_w · w` with `r_w` a rational function whose denominator is a
//! product of differences and `w` acting by `x_a ↦ x_{w(a)}`. Over a domain with every `Q_e`
//! nonzero the representation is faithful (Webster, arXiv:1209.2463v4, proof of Theorem 2.8),
//! so the operator decides equality, and peeling off the longest permutation writes it in the
//! basis `b_π f_π(x)` of straight-line diagrams with dots at the bottom.

use super::smash::{Alg, Frac};
use crate::error::{Error, Result};
use crate::perm::Perm;
use crate::poly::{Exps, Monomial, Poly, PolyRing};
use crate::ring::Ring;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use smallvec::smallvec;
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// A rational position or weight.
pub type Q = BigRational;

/// An edge `e : t(e) → h(e)` of the weighted graph. Strands coloured `tail` interact with the
/// ghosts, shifted by `weight`, of strands coloured `head`.
#[derive(Clone, Debug)]
pub struct Edge<E> {
    pub tail: u16,
    pub head: u16,
    pub weight: Q,
    /// `Q_e(u, v) = Σ c u^a v^b` as `(a, b, c)`, with `u` the dot on the `head` strand and `v`
    /// the dot on the `tail` strand.
    pub q: Vec<(u32, u32, E)>,
    /// `d_{h(e)} c_e`, the degree of a crossing of a `tail` strand with a ghost for `e`.
    pub ghost_degree: i64,
}

/// A red strand: a vertical line at `pos` labelled by a dominant weight `λ`, given by the
/// pairings `exps[i] = ⟨λ, α_i^∨⟩`.
#[derive(Clone, Debug)]
pub struct Red {
    pub pos: Q,
    pub exps: Vec<u32>,
}

/// The data of a weighted KLR algebra, with optional red strands.
#[derive(Clone, Debug)]
pub struct Data<E> {
    /// `d_i` for each colour.
    pub d: Vec<i64>,
    pub edges: Vec<Edge<E>>,
    /// Red strands, ordered by position.
    pub reds: Vec<Red>,
}

/// A loading: black points `(colour, position)` listed left to right.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Loading(pub Vec<(u16, Q)>);

impl Loading {
    pub fn n(&self) -> usize {
        self.0.len()
    }
    pub fn colours(&self) -> Vec<u16> {
        self.0.iter().map(|p| p.0).collect()
    }
}

/// The left-to-right order of all points and the ghosts and red strands they interact with,
/// which determines a loading up to isotopy.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderType {
    pub colours: Vec<u16>,
    /// For each edge, each `tail` point `j` and each `head` point `k`: is `j` right of the ghost?
    pub ghost_sides: Vec<bool>,
    /// For each point and each red strand: is the point right of the red strand?
    pub red_sides: Vec<bool>,
}

/// One elementary event of a diagram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A dot on strand `strand` (strands are numbered by their bottom position).
    Dot { strand: usize },
    /// Strands `a` and `b` cross.
    Cross { a: usize, b: usize },
    /// Strand `tail` crosses the ghost for `edge` of strand `head`; `rightward` if it moves from
    /// the left of the ghost to its right.
    Ghost {
        tail: usize,
        head: usize,
        edge: usize,
        rightward: bool,
    },
    /// Strand `strand` crosses red strand `red`.
    RedCross {
        strand: usize,
        red: usize,
        rightward: bool,
    },
}

/// A straight-line diagram from `bottom` to `top`, strand `k` of `bottom` ending at position
/// `perm(k)` of `top`, as a word of events between generic loadings.
#[derive(Clone, Debug)]
pub struct Segment {
    pub bottom: Loading,
    pub top: Loading,
    pub perm: Perm,
    /// The events from the bottom up; strands are numbered by their position in `bottom`.
    pub events: Vec<Event>,
}

/// The height of a crossing of two lines: the zero in `(0, 1)` of `a + t (b + s(ε))`, with
/// `s(ε) = Σ c ε^m`. The height is `−a / (b + s(ε))`.
#[derive(Clone, Debug)]
struct Height {
    a: Q,
    b: Q,
    s: Vec<(u32, i64)>,
}

fn cmp_heights(x: &Height, y: &Height) -> Ordering {
    // x − y = (y.a (x.b + x.s) − x.a (y.b + y.s)) / ((x.b + x.s)(y.b + y.s)).
    let mut coeffs: BTreeMap<u32, Q> = BTreeMap::new();
    coeffs.insert(0, &y.a * &x.b - &x.a * &y.b);
    for &(m, c) in &x.s {
        *coeffs.entry(m).or_insert_with(Q::zero) += &y.a * Q::from_integer(c.into());
    }
    for &(m, c) in &y.s {
        *coeffs.entry(m).or_insert_with(Q::zero) -= &x.a * Q::from_integer(c.into());
    }
    let lead = coeffs.values().find(|c| !c.is_zero());
    let Some(lead) = lead else {
        return Ordering::Equal;
    };
    let mut ord = if lead.is_positive() {
        Ordering::Greater
    } else {
        Ordering::Less
    };
    if (x.b.is_negative()) != (y.b.is_negative()) {
        ord = ord.reverse();
    }
    ord
}

impl<E: Clone> Data<E> {
    pub fn ncolours(&self) -> usize {
        self.d.len()
    }

    /// Checks that the points are strictly increasing and generic: no point sits on a ghost it
    /// interacts with or on a red strand, and no two ghosts for edges of different weights
    /// coincide.
    pub fn check_loading(&self, l: &Loading) -> Result<()> {
        for (c, _) in &l.0 {
            if *c as usize >= self.ncolours() {
                return Err(Error::invalid(format!("unknown colour {c}")));
            }
        }
        for w in l.0.windows(2) {
            if w[0].1 >= w[1].1 {
                return Err(Error::invalid(
                    "the points of a loading must be listed strictly left to right",
                ));
            }
        }
        for (j, (cj, xj)) in l.0.iter().enumerate() {
            for r in &self.reds {
                if *xj == r.pos {
                    return Err(Error::invalid(format!(
                        "point {} of the loading sits on a red strand",
                        j + 1
                    )));
                }
            }
            for e in &self.edges {
                if e.tail != *cj {
                    continue;
                }
                for (k, (ck, xk)) in l.0.iter().enumerate() {
                    if *ck == e.head && k != j && *xj == xk + &e.weight {
                        return Err(Error::invalid(format!(
                            "point {} of the loading sits on a ghost of point {}",
                            j + 1,
                            k + 1
                        )));
                    }
                }
            }
        }
        let mut ghosts: Vec<(Q, &Q)> = Vec::new();
        for e in &self.edges {
            for (ck, xk) in &l.0 {
                if *ck == e.head {
                    ghosts.push((xk + &e.weight, &e.weight));
                }
            }
        }
        for (i, (g, w)) in ghosts.iter().enumerate() {
            for (h, v) in &ghosts[i + 1..] {
                if g == h && w != v {
                    return Err(Error::invalid(
                        "two ghosts for edges of different weights coincide",
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn order_type(&self, l: &Loading) -> OrderType {
        let mut ghost_sides = Vec::new();
        for e in &self.edges {
            for (cj, xj) in &l.0 {
                if *cj != e.tail {
                    continue;
                }
                for (ck, xk) in &l.0 {
                    if *ck == e.head {
                        ghost_sides.push(*xj > xk + &e.weight);
                    }
                }
            }
        }
        let mut red_sides = Vec::new();
        for (_, x) in &l.0 {
            for r in &self.reds {
                red_sides.push(*x > r.pos);
            }
        }
        OrderType {
            colours: l.colours(),
            ghost_sides,
            red_sides,
        }
    }

    /// True if the two loadings are joined by a straight-line diagram without events.
    pub fn isotopic(&self, a: &Loading, b: &Loading) -> bool {
        self.order_type(a) == self.order_type(b)
    }

    /// The straight-line diagram `b_π` from `bottom` to `top`. Strand `k` runs from `x_k` to
    /// `y_{π(k)} + ε^{π(k)+1}`, `ε > 0` infinitesimal; events are ordered by height.
    pub fn segment(&self, bottom: &Loading, top: &Loading, perm: &Perm) -> Result<Segment> {
        self.check_loading(bottom)?;
        self.check_loading(top)?;
        let n = bottom.n();
        if top.n() != n || perm.n() != n {
            return Err(Error::invalid(
                "a straight-line diagram needs the same number of strands at both ends",
            ));
        }
        for k in 0..n {
            if bottom.0[k].0 != top.0[perm.0[k] as usize].0 {
                return Err(Error::invalid(format!(
                    "strand {} changes colour along the diagram",
                    k + 1
                )));
            }
        }
        let x = |k: usize| &bottom.0[k].1;
        let z = |k: usize| &top.0[perm.0[k] as usize].1;
        let p = |k: usize| perm.0[k] as u32 + 1;
        let delta = |k: usize| z(k) - x(k);
        let mut evs: Vec<(Height, Event, u8)> = Vec::new();
        // Lines `f(t) = a + t(b + s)`: an event exactly when the sign changes from t = 0 to 1.
        let mut add = |a: Q, b: Q, s: Vec<(u32, i64)>, ev: Event, kind: u8| {
            let end = &a + &b;
            if a.is_positive() != end.is_positive() {
                debug_assert!(!a.is_zero() && !end.is_zero());
                evs.push((Height { a, b, s }, ev, kind));
            }
        };
        for j in 0..n {
            for k in j + 1..n {
                add(
                    x(j) - x(k),
                    delta(j) - delta(k),
                    vec![(p(j), 1), (p(k), -1)],
                    Event::Cross { a: j, b: k },
                    1,
                );
            }
        }
        for (ei, e) in self.edges.iter().enumerate() {
            for (j, bj) in bottom.0.iter().enumerate() {
                if bj.0 != e.tail {
                    continue;
                }
                for (k, bk) in bottom.0.iter().enumerate() {
                    if k == j || bk.0 != e.head {
                        continue;
                    }
                    let a = x(j) - x(k) - &e.weight;
                    let rightward = a.is_negative();
                    add(
                        a,
                        delta(j) - delta(k),
                        vec![(p(j), 1), (p(k), -1)],
                        Event::Ghost {
                            tail: j,
                            head: k,
                            edge: ei,
                            rightward,
                        },
                        0,
                    );
                }
            }
        }
        for (ri, r) in self.reds.iter().enumerate() {
            for j in 0..n {
                let a = x(j) - &r.pos;
                let rightward = a.is_negative();
                add(
                    a,
                    delta(j),
                    vec![(p(j), 1)],
                    Event::RedCross {
                        strand: j,
                        red: ri,
                        rightward,
                    },
                    0,
                );
            }
        }
        // Normalize the ε-polynomials (two equal exponents cannot occur: π is injective).
        evs.sort_by(|u, v| {
            cmp_heights(&u.0, &v.0)
                .then(u.2.cmp(&v.2))
                .then_with(|| strands(&u.1).cmp(&strands(&v.1)))
        });
        // Events at the same height must be at different places, except a crossing of two
        // strands and a crossing of one with the ghost of the other for an edge of weight 0.
        for (i, u) in evs.iter().enumerate() {
            for v in &evs[i + 1..] {
                if cmp_heights(&u.0, &v.0) != Ordering::Equal {
                    break;
                }
                let (su, sv) = (strands(&u.1), strands(&v.1));
                let shared = su.iter().any(|s| sv.contains(s));
                let weight_zero_pair =
                    matches!((&u.1, &v.1), (Event::Ghost { .. }, Event::Cross { .. })) && {
                        let mut a = su.clone();
                        let mut b = sv.clone();
                        a.sort();
                        b.sort();
                        a == b
                    };
                if shared && !weight_zero_pair {
                    return Err(Error::invalid(
                        "the straight-line diagram has a triple point",
                    ));
                }
            }
        }
        Ok(Segment {
            bottom: bottom.clone(),
            top: top.clone(),
            perm: perm.clone(),
            events: evs.into_iter().map(|e| e.1).collect(),
        })
    }

    /// The degree of an event: dots `2 d_i`, same-colour crossings `−2 d_i`, other crossings 0,
    /// ghost crossings `d_{h(e)} c_e`, red crossings `d_i ⟨λ, α_i^∨⟩`.
    pub fn event_degree(&self, colours: &[u16], ev: &Event) -> i64 {
        match ev {
            Event::Dot { strand } => 2 * self.d[colours[*strand] as usize],
            Event::Cross { a, b } => {
                if colours[*a] == colours[*b] {
                    -2 * self.d[colours[*a] as usize]
                } else {
                    0
                }
            }
            Event::Ghost { edge, .. } => self.edges[*edge].ghost_degree,
            Event::RedCross { strand, red, .. } => {
                let c = colours[*strand] as usize;
                self.d[c] * self.reds[*red].exps[c] as i64
            }
        }
    }
}

fn strands(e: &Event) -> Vec<usize> {
    match e {
        Event::Dot { strand } => vec![*strand],
        Event::Cross { a, b } => vec![*a, *b],
        Event::Ghost { tail, head, .. } => vec![*tail, *head],
        Event::RedCross { strand, .. } => vec![*strand],
    }
}

/// An operator `Σ r_w · w` on `k[x_1, …, x_n]`.
pub type Op<E> = BTreeMap<Perm, Frac<E>>;

/// The polynomial representation on loadings with `n` black strands.
pub struct PolyRep<'a, R: Ring> {
    pub data: &'a Data<R::Elem>,
    pub alg: Alg<R>,
}

impl<'a, R: Ring> PolyRep<'a, R> {
    pub fn new(data: &'a Data<R::Elem>, base: R, n: usize) -> Self {
        let names: Vec<String> = (1..=n).map(|k| format!("x_{k}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        PolyRep {
            data,
            alg: Alg::new(PolyRing::new(base, &refs), n),
        }
    }

    pub fn n(&self) -> usize {
        self.alg.n
    }

    pub fn identity(&self) -> Op<R::Elem> {
        let mut op = Op::new();
        op.insert(Perm::identity(self.n()), self.alg.poly(self.alg.ring.one()));
        op
    }

    fn add_into(&self, op: &mut Op<R::Elem>, w: Perm, f: Frac<R::Elem>) {
        match op.get_mut(&w) {
            Some(g) => {
                self.alg.add_assign(g, &f);
                if self.alg.ring.is_zero(&g.num) {
                    op.remove(&w);
                }
            }
            None => {
                if !self.alg.ring.is_zero(&f.num) {
                    op.insert(w, f);
                }
            }
        }
    }

    /// `p · op`.
    fn mul_poly(&self, p: &Poly<R::Elem>, op: &Op<R::Elem>) -> Op<R::Elem> {
        let pf = self.alg.poly(p.clone());
        op.iter()
            .map(|(w, r)| (w.clone(), self.alg.mul(&pf, r)))
            .filter(|(_, r)| !self.alg.ring.is_zero(&r.num))
            .collect()
    }

    /// `s_m · op`.
    fn swap(&self, m: usize, op: &Op<R::Elem>) -> Op<R::Elem> {
        let s = Perm::simple(self.n(), m);
        op.iter()
            .map(|(w, r)| (s.compose(w), self.alg.act(&s, r)))
            .collect()
    }

    /// `∂_m · op`, with `∂_m = (x_m − x_{m+1})⁻¹ (1 − s_m)`.
    fn demazure(&self, m: usize, op: &Op<R::Elem>) -> Op<R::Elem> {
        let n = self.n();
        let mut den = smallvec![0; n * n];
        den[m * n + m + 1] = 1;
        let inv = Frac {
            num: self.alg.ring.one(),
            den,
        };
        let s = Perm::simple(n, m);
        let mut out = Op::new();
        for (w, r) in op {
            self.add_into(&mut out, w.clone(), self.alg.mul(&inv, r));
            let t = self.alg.mul(&self.alg.neg(&inv), &self.alg.act(&s, r));
            self.add_into(&mut out, s.compose(w), t);
        }
        out
    }

    /// `Q_e(x_u, x_v)` in the strand ring.
    fn q_poly(&self, e: &Edge<R::Elem>, u: usize, v: usize) -> Poly<R::Elem> {
        let n = self.n();
        let terms =
            e.q.iter()
                .map(|(a, b, c)| {
                    let mut ex: Exps = smallvec![0; n];
                    ex[u] += *a as i32;
                    ex[v] += *b as i32;
                    (Monomial(ex), c.clone())
                })
                .collect();
        self.alg.ring.normalize(terms)
    }

    /// `ev · op`, where `order[m]` is the strand at position `m` just below the event (updated
    /// for crossings) and `colours[k]` the colour of strand `k`.
    pub fn apply_event(
        &self,
        ev: &Event,
        order: &mut [usize],
        colours: &[u16],
        op: &Op<R::Elem>,
    ) -> Result<Op<R::Elem>> {
        let pos = |order: &[usize], k: usize| order.iter().position(|&s| s == k).unwrap();
        Ok(match ev {
            Event::Dot { strand } => {
                let m = pos(order, *strand);
                self.mul_poly(&self.alg.ring.var(m), op)
            }
            Event::Cross { a, b } => {
                let (pa, pb) = (pos(order, *a), pos(order, *b));
                let m = pa.min(pb);
                if pa.abs_diff(pb) != 1 {
                    return Err(Error::invalid(
                        "a crossing of two strands that are not adjacent",
                    ));
                }
                order.swap(m, m + 1);
                if colours[*a] == colours[*b] {
                    self.demazure(m, op)
                } else {
                    self.swap(m, op)
                }
            }
            Event::Ghost {
                tail,
                head,
                edge,
                rightward,
            } => {
                if !rightward {
                    return Ok(op.clone());
                }
                let e = &self.data.edges[*edge];
                let q = self.q_poly(e, pos(order, *head), pos(order, *tail));
                self.mul_poly(&q, op)
            }
            Event::RedCross {
                strand,
                red,
                rightward,
            } => {
                if !rightward {
                    return Ok(op.clone());
                }
                let m = pos(order, *strand);
                let c = colours[*strand] as usize;
                let e = self.data.reds[*red].exps[c];
                let p = self.alg.ring.pow(&self.alg.ring.var(m), e);
                self.mul_poly(&p, op)
            }
        })
    }

    /// `seg · op`.
    pub fn apply_segment(&self, seg: &Segment, op: &Op<R::Elem>) -> Result<Op<R::Elem>> {
        let colours = seg.bottom.colours();
        let mut order: Vec<usize> = (0..self.n()).collect();
        let mut out = op.clone();
        for ev in &seg.events {
            out = self.apply_event(ev, &mut order, &colours, &out)?;
        }
        debug_assert!((0..self.n()).all(|k| order[seg.perm.0[k] as usize] == k));
        Ok(out)
    }

    /// The operator of a dot on the strand at position `m`, composed on top of `op`.
    pub fn dot(&self, m: usize, op: &Op<R::Elem>) -> Op<R::Elem> {
        self.mul_poly(&self.alg.ring.var(m), op)
    }

    /// `op ∘ f`: `f` (a polynomial in the bottom variables) first.
    pub fn then_poly(&self, op: &Op<R::Elem>, f: &Poly<R::Elem>) -> Op<R::Elem> {
        op.iter()
            .map(|(w, r)| {
                let wf = self.alg.poly(self.alg.act_poly(w, f));
                (w.clone(), self.alg.mul(r, &wf))
            })
            .filter(|(_, r)| !self.alg.ring.is_zero(&r.num))
            .collect()
    }

    /// `top ∘ bottom`.
    pub fn compose(&self, top: &Op<R::Elem>, bottom: &Op<R::Elem>) -> Op<R::Elem> {
        let mut out = Op::new();
        for (v, s) in bottom {
            for (w, r) in top {
                let f = self.alg.mul(r, &self.alg.act(w, s));
                self.add_into(&mut out, w.compose(v), f);
            }
        }
        out
    }

    /// The operator of `b_π` from `bottom` to `top`.
    pub fn basis_op(&self, bottom: &Loading, top: &Loading, perm: &Perm) -> Result<Op<R::Elem>> {
        let seg = self.data.segment(bottom, top, perm)?;
        self.apply_segment(&seg, &self.identity())
    }

    /// Writes `op`, an operator from `bottom` to `top`, as `Σ_π b_π ∘ f_π` with `f_π` a
    /// polynomial in the bottom variables (dots at the bottom). Fails if `op` is not in the
    /// image of the algebra.
    pub fn normal_form(
        &self,
        bottom: &Loading,
        top: &Loading,
        op: &Op<R::Elem>,
    ) -> Result<BTreeMap<Perm, Poly<R::Elem>>> {
        let mut rest = op.clone();
        let mut out = BTreeMap::new();
        let mut cache: BTreeMap<Perm, Op<R::Elem>> = BTreeMap::new();
        while let Some(w) = rest
            .keys()
            .max_by(|a, b| a.length().cmp(&b.length()).then(b.cmp(a)))
            .cloned()
        {
            if !cache.contains_key(&w) {
                cache.insert(w.clone(), self.basis_op(bottom, top, &w)?);
            }
            let b = &cache[&w];
            let lead = b.get(&w).ok_or_else(|| {
                Error::invalid("internal: a basis diagram lacks its leading permutation")
            })?;
            let g = self
                .alg
                .div_to_poly(&rest[&w], lead)
                .ok_or_else(|| Error::invalid("the operator is not in the algebra"))?;
            let f = self.alg.act_poly(&w.inverse(), &g);
            let sub = self.then_poly(b, &f);
            for (u, r) in sub {
                self.add_into(&mut rest, u, self.alg.neg(&r));
            }
            debug_assert!(!rest.contains_key(&w));
            out.insert(w, f);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::Rat;
    use crate::ring::Rationals;
    use num_bigint::BigInt;

    fn q(n: i64, d: i64) -> Q {
        Q::new(BigInt::from(n), BigInt::from(d))
    }
    fn qi(n: i64) -> Q {
        q(n, 1)
    }
    fn rat(n: i64) -> Rat {
        Rationals.int(n)
    }
    fn load(pts: &[(u16, Q)]) -> Loading {
        Loading(pts.to_vec())
    }

    /// Two colours `a = 0`, `b = 1`, one edge `a → b` of weight `w` with `Q_e(u, v) = u + v`
    /// (type `A_2`, `c_e = 1`).
    fn a2(w: Q) -> Data<Rat> {
        Data {
            d: vec![1, 1],
            edges: vec![Edge {
                tail: 0,
                head: 1,
                weight: w,
                q: vec![(1, 0, rat(1)), (0, 1, rat(1))],
                ghost_degree: 1,
            }],
            reds: vec![],
        }
    }

    fn poly_of(rep: &PolyRep<Rationals>, op: &Op<Rat>) -> Poly<Rat> {
        assert_eq!(op.len(), 1);
        let (w, f) = op.iter().next().unwrap();
        assert!(w.is_identity());
        assert!(f.den.iter().all(|&e| e == 0));
        let _ = rep;
        f.num.clone()
    }

    #[test]
    fn weight_zero_bigon_is_q() {
        let data = a2(qi(0));
        let rep = PolyRep::new(&data, Rationals, 2);
        let ab = load(&[(0, qi(0)), (1, qi(1))]);
        let ba = load(&[(1, qi(0)), (0, qi(1))]);
        let s = Perm::simple(2, 0);
        let up = data.segment(&ab, &ba, &s).unwrap();
        let down = data.segment(&ba, &ab, &s).unwrap();
        let op = rep
            .apply_segment(&down, &rep.apply_segment(&up, &rep.identity()).unwrap())
            .unwrap();
        // Q_e(x_b, x_a) = x_1 + x_2.
        let want = rep.alg.ring.add(&rep.alg.ring.var(0), &rep.alg.ring.var(1));
        assert_eq!(poly_of(&rep, &op), want);
        let nf = rep.normal_form(&ab, &ab, &op).unwrap();
        assert_eq!(nf.len(), 1);
        assert_eq!(nf[&Perm::identity(2)], want);
    }

    #[test]
    fn ghost_bigon_without_crossing() {
        // b at 0, its ghost at 2; a moves from 1 to 3 through the ghost and back.
        let data = a2(qi(2));
        let rep = PolyRep::new(&data, Rationals, 2);
        let l = load(&[(1, qi(0)), (0, qi(1))]);
        let r = load(&[(1, qi(0)), (0, qi(3))]);
        let id = Perm::identity(2);
        let up = data.segment(&l, &r, &id).unwrap();
        assert_eq!(up.events.len(), 1);
        let down = data.segment(&r, &l, &id).unwrap();
        let op = rep
            .apply_segment(&down, &rep.apply_segment(&up, &rep.identity()).unwrap())
            .unwrap();
        let want = rep.alg.ring.add(&rep.alg.ring.var(0), &rep.alg.ring.var(1));
        assert_eq!(poly_of(&rep, &op), want);
        assert!(!data.isotopic(&l, &r));
        assert!(data.isotopic(&l, &load(&[(1, qi(0)), (0, q(3, 2))])));
    }

    #[test]
    fn nilhecke_square_is_zero() {
        let data = Data::<Rat> {
            d: vec![1],
            edges: vec![],
            reds: vec![],
        };
        let rep = PolyRep::new(&data, Rationals, 2);
        let l = load(&[(0, qi(0)), (0, qi(1))]);
        let s = Perm::simple(2, 0);
        let seg = data.segment(&l, &l, &s).unwrap();
        let once = rep.apply_segment(&seg, &rep.identity()).unwrap();
        assert_eq!(rep.apply_segment(&seg, &once).unwrap().len(), 0);
        let nf = rep.normal_form(&l, &l, &once).unwrap();
        assert_eq!(nf.len(), 1);
        assert_eq!(nf[&s], rep.alg.ring.one());
    }

    #[test]
    fn red_bigon() {
        let data = Data::<Rat> {
            d: vec![1],
            edges: vec![],
            reds: vec![Red {
                pos: qi(0),
                exps: vec![2],
            }],
        };
        let rep = PolyRep::new(&data, Rationals, 1);
        let l = load(&[(0, qi(-1))]);
        let r = load(&[(0, qi(1))]);
        let id = Perm::identity(1);
        let op = rep
            .apply_segment(
                &data.segment(&r, &l, &id).unwrap(),
                &rep.apply_segment(&data.segment(&l, &r, &id).unwrap(), &rep.identity())
                    .unwrap(),
            )
            .unwrap();
        let want = rep.alg.ring.pow(&rep.alg.ring.var(0), 2);
        assert_eq!(poly_of(&rep, &op), want);
    }

    /// Every basis diagram is its own normal form, and a composite of two straight-line
    /// diagrams has a polynomial normal form that re-expands to it.
    #[test]
    fn normal_forms_of_composites() {
        let data = a2(q(1, 2));
        let n = 3;
        let rep = PolyRep::new(&data, Rationals, n);
        let b = load(&[(0, qi(0)), (1, qi(1)), (0, qi(2))]);
        let c = load(&[(1, q(-1, 3)), (0, q(1, 3)), (0, qi(4))]);
        let d = load(&[(0, qi(-2)), (0, q(1, 4)), (1, q(5, 4))]);
        let perms = |from: &Loading, to: &Loading| -> Vec<Perm> {
            let mut out = Vec::new();
            let mut v: Vec<u8> = (0..n as u8).collect();
            permutations(&mut v, 0, &mut |p| {
                if (0..n).all(|k| from.0[k].0 == to.0[p[k] as usize].0) {
                    out.push(Perm(p.to_vec()));
                }
            });
            out
        };
        for w in perms(&b, &d) {
            let op = rep.basis_op(&b, &d, &w).unwrap();
            let nf = rep.normal_form(&b, &d, &op).unwrap();
            assert_eq!(nf.len(), 1);
            assert_eq!(nf[&w], rep.alg.ring.one());
        }
        for u in perms(&b, &c) {
            for v in perms(&c, &d) {
                let op = rep.compose(
                    &rep.basis_op(&c, &d, &v).unwrap(),
                    &rep.basis_op(&b, &c, &u).unwrap(),
                );
                let nf = rep.normal_form(&b, &d, &op).unwrap();
                let mut back = Op::new();
                for (w, f) in &nf {
                    let t = rep.then_poly(&rep.basis_op(&b, &d, w).unwrap(), f);
                    for (x, r) in t {
                        rep.add_into(&mut back, x, r);
                    }
                }
                assert_eq!(back, op);
            }
        }
    }

    fn permutations(v: &mut Vec<u8>, k: usize, f: &mut impl FnMut(&[u8])) {
        if k == v.len() {
            f(v);
            return;
        }
        for i in k..v.len() {
            v.swap(k, i);
            permutations(v, k + 1, f);
            v.swap(k, i);
        }
    }

    #[test]
    fn isotopic_loadings_are_joined_without_events() {
        let data = a2(q(1, 2));
        let b = load(&[(0, qi(0)), (1, qi(1)), (0, qi(2))]);
        let c = load(&[(0, q(-7, 2)), (1, qi(-3)), (0, qi(5))]);
        assert!(data.isotopic(&b, &c));
        let seg = data.segment(&b, &c, &Perm::identity(3)).unwrap();
        assert!(seg.events.is_empty());
    }

    #[test]
    fn ghost_on_point_is_rejected() {
        let data = a2(qi(1));
        assert!(
            data.check_loading(&load(&[(1, qi(0)), (0, qi(1))]))
                .is_err()
        );
    }
}
