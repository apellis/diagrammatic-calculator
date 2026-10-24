//! Upward diagrams of Brundan–Ellis's super Kac–Moody 2-category with bubbles: `QH(ν) ⊗ SYM`.
//!
//! An upward 2-morphism `E_ν 1_λ → E_ν' 1_λ` is written in the basis `ψ_ŵ f` of the quiver Hecke
//! superalgebra (Kang–Kashiwara–Tsuchioka, arXiv:1107.1039v2, Corollary 3.15): crossings of the
//! canonical reduced word `ŵ` of a permutation (`Perm::reduced_word`) above a polynomial `f` at the
//! bottom. The polynomial is in the dots, which anticommute on distinct odd strands, and in the
//! bubbles of the rightmost region (Brundan–Ellis, arXiv:1701.04133v2, (1.21)–(1.22)): for an even
//! colour the clockwise bubbles `W_k` (`k ≥ 1`), for an odd colour the even bubbles
//! `e_n = W_{2n}` and the odd bubble `δ = W_1` with `δ² = 0`, so that `W_{2n+1} = δ e_n`. All of
//! them supercommute, because they sit at distinct horizontal positions; the product `a b` puts
//! `a` above `b`.
//!
//! Products are computed by straightening with the defining relations of the presentation: dots
//! slide through crossings (with the sign `(−1)^{|a||b|}` and a correction for equal colours),
//! distant generators exchange heights with the Koszul sign, braid moves carry the correction of
//! `braid-corrected`, and a double crossing is the quadratic relation. The braid moves follow the
//! exchange property (`crate::rewrite::plan`). That the normal forms are linearly independent in
//! the 2-category is part of Brundan–Ellis's Nondegeneracy Conjecture (Section 8), open for odd
//! colours; the strategy's certificate says so.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::fxhash::{self, FxHashMap};
use crate::perm::Perm;
use crate::poly::{Exps, Monomial, Poly, PolyRing};
use crate::rep::kl::up::{adapted_l, adapted_r};
use crate::rewrite::{Op, perm_of, plan};
use crate::ring::Ring;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// The message of the error raised when a bubble index exceeds the allocated variables; the
/// caller retries with more.
pub(super) const OVERFLOW: &str = "skm: bubble index above the allocated variables";

/// An upward 2-morphism from a fixed bottom object: the polynomial at the bottom of each basis
/// element `ψ_ŵ`, by permutation.
pub(super) type Val<E> = BTreeMap<Perm, Poly<E>>;

pub(super) type P<R> = Poly<<R as Ring>::Elem>;
type Cache<E> = RefCell<FxHashMap<(Vec<u16>, Perm, usize), Rc<Val<E>>>>;
/// Strands `(colour, dot)` that bubbles slide across.
type Strands<E> = Vec<(u16, Poly<E>)>;
type SeriesCache<K, E> = RefCell<FxHashMap<K, Rc<Vec<Poly<E>>>>>;
/// A bubble series: colour, clockwise, the strands it slides across, and on which side.
type BubbleKey<E> = (u16, bool, Strands<E>, bool);
/// A value peeled into crossing words with the polynomials below them.
type Peeled<E> = Vec<(Perm, Vec<usize>, Poly<E>)>;

/// One step of an upward diagram, bottom first.
#[derive(Clone, Debug)]
pub(super) enum UpEv<E> {
    /// A dot on the strand at a position.
    Dot(usize),
    /// A crossing of the strands at `k` and `k + 1`.
    Cross(usize),
    /// Multiplication on top by a polynomial in the dots at this height, the bubbles, and the
    /// scalars, its monomials read with the first variable highest.
    Poly(Poly<E>),
    /// A bubble in the region left of position `pos`, given by its degree index.
    Bub {
        pos: usize,
        colour: u16,
        cw: bool,
        index: i64,
    },
}

/// The ring and the arithmetic of upward diagrams on up to `nx` strands, with bubbles of index up
/// to `kz` (even colours) or `2 kz + 1` (odd colours) for each colour.
pub(super) struct Up<'a, R: Ring> {
    pub ctx: &'a Context<R>,
    /// The Cartan matrix.
    pub a: Vec<Vec<i64>>,
    /// Which colours are odd.
    pub odd: Vec<bool>,
    /// Whether the leftward cups and caps of each colour are odd: `p(i)(⟨i, λ⟩ + 1)`, the same in
    /// every region, since an odd colour pairs evenly with every simple root.
    pub pi: Vec<bool>,
    pub nx: usize,
    pub kz: usize,
    pub nc: usize,
    /// The ring with every dot even; the dots get their parities from the strands below them.
    plain: PolyRing<R>,
    rings: RefCell<FxHashMap<Vec<u16>, Rc<PolyRing<R>>>>,
    cser: RefCell<FxHashMap<u16, Vec<P<R>>>>,
    slide_cache: SeriesCache<(Strands<R::Elem>, bool), R::Elem>,
    bubble_cache: SeriesCache<BubbleKey<R::Elem>, R::Elem>,
    cross_cache: Cache<R::Elem>,
    dot_cache: Cache<R::Elem>,
}

/// The colours after crossings listed top first, from the colours below them.
pub(super) fn after(cols: &[u16], top_first: &[usize]) -> Vec<u16> {
    let mut c = cols.to_vec();
    for &k in top_first.iter().rev() {
        c.swap(k, k + 1);
    }
    c
}

impl<'a, R: Ring> Up<'a, R> {
    pub fn new(ctx: &'a Context<R>, nx: usize, kz: usize) -> Result<Self> {
        let cartan = ctx
            .cartan
            .as_ref()
            .ok_or_else(|| Error::precondition("the BE2 normal form needs a Cartan datum"))?;
        let nc = ctx.colours.len();
        let mut names: Vec<String> = (1..=nx).map(|k| format!("x_{k}")).collect();
        for c in 0..nc {
            for k in 1..=kz {
                names.push(format!("z_{c}_{k}"));
            }
        }
        for c in 0..nc {
            names.push(format!("d_{c}"));
        }
        names.extend(ctx.spec.scalars.iter().cloned());
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut plain = PolyRing::new(ctx.base.clone(), &refs);
        let odd: Vec<bool> = ctx.colours.iter().map(|c| c.odd).collect();
        for (c, &o) in odd.iter().enumerate() {
            let d = nx + nc * kz + c;
            plain.odd[d] = o;
            plain.exterior[d] = o;
        }
        let pi = (0..nc)
            .map(|c| odd[c] && ctx.mu.as_ref().is_some_and(|m| m[c].rem_euclid(2) == 0))
            .collect();
        Ok(Up {
            ctx,
            a: cartan.a.clone(),
            odd,
            pi,
            nx,
            kz,
            nc,
            plain,
            rings: RefCell::new(fxhash::map()),
            cser: RefCell::new(fxhash::map()),
            slide_cache: RefCell::new(fxhash::map()),
            bubble_cache: RefCell::new(fxhash::map()),
            cross_cache: RefCell::new(fxhash::map()),
            dot_cache: RefCell::new(fxhash::map()),
        })
    }

    /// The ring of polynomials at the bottom of strands of the given colours: a dot is odd when
    /// its strand is.
    pub fn ring(&self, cols: &[u16]) -> Rc<PolyRing<R>> {
        if let Some(r) = self.rings.borrow().get(cols) {
            return r.clone();
        }
        assert!(
            cols.len() <= self.nx,
            "skm: more strands than dot variables"
        );
        let mut r = self.plain.clone();
        for (k, &c) in cols.iter().enumerate() {
            r.odd[k] = self.odd[c as usize];
        }
        let r = Rc::new(r);
        self.rings.borrow_mut().insert(cols.to_vec(), r.clone());
        r
    }

    fn zoff(&self) -> usize {
        self.nx
    }

    fn doff(&self) -> usize {
        self.nx + self.nc * self.kz
    }

    fn soff(&self) -> usize {
        self.doff() + self.nc
    }

    pub fn x(&self, k: usize) -> P<R> {
        self.plain.var(k)
    }

    pub fn xpow(&self, k: usize, e: i64) -> P<R> {
        let mut m: Exps = smallvec::smallvec![0; self.plain.nvars()];
        m[k] = e as i32;
        self.plain.monomial(Monomial(m), self.plain.base.one())
    }

    /// Whether variable `v` is a dot, a bubble, or a scalar.
    #[cfg(test)]
    fn is_dot(&self, v: usize) -> bool {
        v < self.nx
    }

    /// The parity of variable `v` of a polynomial at the bottom of strands `cols`.
    fn var_odd(&self, cols: &[u16], v: usize) -> bool {
        if v < self.nx {
            v < cols.len() && self.odd[cols[v] as usize]
        } else {
            self.plain.odd[v]
        }
    }

    /// `W_k` of colour `c` in the reference region (the rightmost one).
    pub fn z(&self, c: u16, k: i64) -> Result<P<R>> {
        if k < 0 {
            return Ok(self.plain.zero());
        }
        if k == 0 {
            return Ok(self.plain.one());
        }
        let ci = c as usize;
        if self.odd[ci] {
            let n = k / 2;
            if n as usize > self.kz {
                return Err(Error::limit(OVERFLOW));
            }
            let e = if n == 0 {
                self.plain.one()
            } else {
                self.plain.var(self.zoff() + ci * self.kz + n as usize - 1)
            };
            return Ok(if k % 2 == 1 {
                self.plain.mul(&self.plain.var(self.doff() + ci), &e)
            } else {
                e
            });
        }
        if k as usize > self.kz {
            return Err(Error::limit(OVERFLOW));
        }
        Ok(self.plain.var(self.zoff() + ci * self.kz + k as usize - 1))
    }

    /// The parity of a crossing of the strands at `k`, `k + 1` of colours `cols`.
    fn cross_odd(&self, cols: &[u16], k: usize) -> bool {
        self.odd[cols[k] as usize] && self.odd[cols[k + 1] as usize]
    }

    /// The parity of `ψ_ŵ` on strands `cols`: the number of crossings of two odd strands.
    pub fn perm_odd(&self, cols: &[u16], w: &Perm) -> bool {
        let n = w.n();
        let mut p = false;
        for a in 0..n {
            for b in a + 1..n {
                if w.0[a] > w.0[b] && self.odd[cols[a] as usize] && self.odd[cols[b] as usize] {
                    p = !p;
                }
            }
        }
        p
    }

    /// The quadratic relation: `ψ_k ψ_k` on the colours `cols` at positions `k`, `k + 1`.
    fn square(&self, cols: &[u16], k: usize) -> P<R> {
        let (a, b) = (cols[k] as usize, cols[k + 1] as usize);
        if a == b {
            return self.plain.zero();
        }
        if self.a[a][b] == 0 {
            return self.plain.one();
        }
        let r = self.ring(cols);
        r.add(
            &self.xpow(k, -self.a[a][b]),
            &self.xpow(k + 1, -self.a[b][a]),
        )
    }

    /// The correction of `braid-corrected`: `ψ_k ψ_{k+1} ψ_k − ψ_{k+1} ψ_k ψ_{k+1}` on the colours
    /// `cols` at positions `k, k + 1, k + 2`.
    fn braid_correction(&self, cols: &[u16], k: usize) -> Option<P<R>> {
        let (a, b) = (cols[k] as usize, cols[k + 1] as usize);
        if a != cols[k + 2] as usize || a == b {
            return None;
        }
        let d = -self.a[a][b];
        let r = self.ring(cols);
        let mut out = r.zero();
        for s in 0..d {
            let sign = self.odd[a] && (self.odd[b] != (s % 2 == 1));
            let t = r.mul(&self.xpow(k, d - 1 - s), &self.xpow(k + 2, s));
            out = if sign {
                r.sub(&out, &t)
            } else {
                r.add(&out, &t)
            };
        }
        Some(out)
    }

    pub fn ident(&self, n: usize) -> Val<R::Elem> {
        let mut v = Val::new();
        v.insert(Perm::identity(n), self.plain.one());
        v
    }

    /// `a + c · b` (`c` a sign).
    fn add_into(&self, cols: &[u16], a: &mut Val<R::Elem>, b: &Val<R::Elem>, neg: bool) {
        let r = self.ring(cols);
        for (w, f) in b {
            match a.get_mut(w) {
                Some(g) => {
                    if neg {
                        r.sub_assign(g, f);
                    } else {
                        r.add_assign(g, f);
                    }
                    if r.is_zero(g) {
                        a.remove(w);
                    }
                }
                None => {
                    let f = if neg { r.neg(f) } else { f.clone() };
                    if !r.is_zero(&f) {
                        a.insert(w.clone(), f);
                    }
                }
            }
        }
    }

    /// `v · f`: `f` multiplied at the bottom.
    pub fn mul_bottom(&self, cols: &[u16], v: &Val<R::Elem>, f: &P<R>) -> Val<R::Elem> {
        let r = self.ring(cols);
        let mut out = Val::new();
        for (w, g) in v {
            let h = r.mul(g, f);
            if !r.is_zero(&h) {
                out.insert(w.clone(), h);
            }
        }
        out
    }

    pub fn scale(&self, v: &Val<R::Elem>, c: &R::Elem) -> Val<R::Elem> {
        let mut out = Val::new();
        for (w, f) in v {
            let g = self.plain.scale(f, c);
            if !self.plain.is_zero(&g) {
                out.insert(w.clone(), g);
            }
        }
        out
    }

    /// The basis element `ψ_word` for a word listed top first, straightened.
    pub fn word(&self, cols: &[u16], top_first: &[usize]) -> Val<R::Elem> {
        let mut v = self.ident(cols.len());
        for &k in top_first.iter().rev() {
            v = self.cross(cols, &v, k);
        }
        v
    }

    /// `ψ_k · v`.
    pub fn cross(&self, cols: &[u16], v: &Val<R::Elem>, k: usize) -> Val<R::Elem> {
        let mut out = Val::new();
        for (w, f) in v {
            let c = self.cross_basis(cols, w, k);
            self.add_into(cols, &mut out, &self.mul_bottom(cols, &c, f), false);
        }
        out
    }

    /// `x_k · v`, the dot at position `k` on top.
    pub fn dot(&self, cols: &[u16], v: &Val<R::Elem>, k: usize) -> Val<R::Elem> {
        let mut out = Val::new();
        for (w, f) in v {
            let c = self.dot_basis(cols, w, k);
            self.add_into(cols, &mut out, &self.mul_bottom(cols, &c, f), false);
        }
        out
    }

    /// `ψ_upper · v` for crossings listed top first.
    fn apply_word(&self, cols: &[u16], mut v: Val<R::Elem>, top_first: &[usize]) -> Val<R::Elem> {
        for &k in top_first.iter().rev() {
            v = self.cross(cols, &v, k);
        }
        v
    }

    /// `ψ_k ψ_ŵ`, straightened.
    fn cross_basis(&self, cols: &[u16], w: &Perm, k: usize) -> Rc<Val<R::Elem>> {
        let key = (cols.to_vec(), w.clone(), k);
        if let Some(v) = self.cross_cache.borrow().get(&key) {
            return v.clone();
        }
        let n = cols.len();
        let mut cur = vec![k];
        cur.extend(w.reduced_word());
        let mut neg = false;
        let mut out = Val::new();
        let mut main = true;
        for op in plan(n, &cur) {
            match op {
                Op::Commute(i) => {
                    let bc = after(cols, &cur[i + 2..]);
                    if self.cross_odd(&bc, cur[i]) && self.cross_odd(&bc, cur[i + 1]) {
                        neg = !neg;
                    }
                    cur.swap(i, i + 1);
                }
                Op::Braid(i) => {
                    let bc = after(cols, &cur[i + 3..]);
                    let m = cur[i].min(cur[i + 1]);
                    if let Some(c) = self.braid_correction(&bc, m) {
                        // ψ_{m,m+1,m} = ψ_{m+1,m,m+1} + C
                        let low = self.word(cols, &cur[i + 3..]);
                        let mid = self.mul_top(cols, &low, &c);
                        let full = self.apply_word(cols, mid, &cur[..i]);
                        self.add_into(cols, &mut out, &full, neg != (cur[i] != m));
                    }
                    let (j, l) = (cur[i], cur[i + 1]);
                    cur[i] = l;
                    cur[i + 1] = j;
                    cur[i + 2] = l;
                }
                Op::Square(i) => {
                    let bc = after(cols, &cur[i + 2..]);
                    let q = self.square(&bc, cur[i]);
                    if !self.plain.is_zero(&q) {
                        let low = self.word(cols, &cur[i + 2..]);
                        let mid = self.mul_top(cols, &low, &q);
                        let full = self.apply_word(cols, mid, &cur[..i]);
                        self.add_into(cols, &mut out, &full, neg);
                    }
                    main = false;
                }
            }
        }
        if main {
            let mut m = Val::new();
            m.insert(perm_of(n, &cur), self.plain.one());
            self.add_into(cols, &mut out, &m, neg);
        }
        let out = Rc::new(out);
        self.cross_cache.borrow_mut().insert(key, out.clone());
        out
    }

    /// `x_k ψ_ŵ`, the dot at top position `k`, straightened.
    fn dot_basis(&self, cols: &[u16], w: &Perm, k: usize) -> Rc<Val<R::Elem>> {
        let key = (cols.to_vec(), w.clone(), k);
        if let Some(v) = self.dot_cache.borrow().get(&key) {
            return v.clone();
        }
        let word = w.reduced_word();
        let mut pos = k;
        let mut neg = false;
        let mut out = Val::new();
        for t in 0..word.len() {
            let m = word[t];
            let bc = after(cols, &word[t + 1..]);
            let (a, b) = (bc[m], bc[m + 1]);
            let eps = self.cross_odd(&bc, m);
            if pos == m || pos == m + 1 {
                if a == b {
                    // x_1 ψ = ε ψ x_2 + 1, x_2 ψ = ε ψ x_1 − ε (ε = ±1)
                    let mut rest = word[..t].to_vec();
                    rest.extend_from_slice(&word[t + 1..]);
                    let c = self.word(cols, &rest);
                    self.add_into(cols, &mut out, &c, neg != (pos == m + 1 && !eps));
                }
                if eps {
                    neg = !neg;
                }
                pos = if pos == m { m + 1 } else { m };
            } else if eps && self.odd[bc[pos] as usize] {
                neg = !neg;
            }
        }
        let r = self.ring(cols);
        let mut m = Val::new();
        m.insert(
            w.clone(),
            if neg {
                r.neg(&self.x(pos))
            } else {
                self.x(pos)
            },
        );
        self.add_into(cols, &mut out, &m, false);
        let out = Rc::new(out);
        self.dot_cache.borrow_mut().insert(key, out.clone());
        out
    }

    #[cfg(test)]
    fn mul_top_eager(&self, cols: &[u16], v: &Val<R::Elem>, p: &P<R>) -> Val<R::Elem> {
        let mut out = Val::new();
        let nv = self.plain.nvars();
        for (m, c) in &p.terms {
            let mut cur = self.scale(v, c);
            for var in (0..nv).rev() {
                let e = m.0[var];
                if e == 0 || cur.is_empty() {
                    continue;
                }
                if self.is_dot(var) {
                    for _ in 0..e {
                        cur = self.dot(cols, &cur, var);
                    }
                } else {
                    let r = self.ring(cols);
                    let mut one: Exps = smallvec::smallvec![0; nv];
                    one[var] = e;
                    let z = self.plain.monomial(Monomial(one), self.plain.base.one());
                    let odd = self.var_odd(cols, var) && e % 2 == 1;
                    let mut next = Val::new();
                    for (w, f) in &cur {
                        let mut g = r.mul(&z, f);
                        if odd && self.perm_odd(cols, w) {
                            g = r.neg(&g);
                        }
                        if !r.is_zero(&g) {
                            next.insert(w.clone(), g);
                        }
                    }
                    cur = next;
                }
            }
            self.add_into(cols, &mut out, &cur, false);
        }
        out
    }

    /// `p · v`: a polynomial in the dots at the top of `v`, the bubbles, and the scalars,
    /// multiplied on top, its monomials read as products in variable order (the first variable
    /// highest).
    pub fn mul_top(&self, cols: &[u16], v: &Val<R::Elem>, p: &P<R>) -> Val<R::Elem> {
        let r = self.ring(cols);
        // Group only by the dot prefix: bubble/scalar factors are applied first,
        // keeping their original order and their sign across each permutation.
        let mut groups: BTreeMap<Exps, Val<R::Elem>> = BTreeMap::new();
        for (m, c) in &p.terms {
            let dots = Exps::from_slice(&m.0[..self.nx]);
            let mut tail = m.clone();
            tail.0[..self.nx].fill(0);
            let odd = self.mono_odd(cols, &tail);
            let z = r.monomial(tail, c.clone());
            let cur = groups.entry(dots).or_default();
            for (w, f) in v {
                let mut g = r.mul(&z, f);
                if odd && self.perm_odd(cols, w) {
                    g = r.neg(&g);
                }
                match cur.get_mut(w) {
                    Some(h) => r.add_assign(h, &g),
                    None => {
                        cur.insert(w.clone(), g);
                    }
                }
            }
        }
        let mut out = Val::new();
        for (dots, mut cur) in groups {
            cur.retain(|_, f| !r.is_zero(f));
            for (var, &e) in dots.iter().enumerate().rev() {
                for _ in 0..e {
                    cur = self.dot(cols, &cur, var);
                }
            }
            self.add_into(cols, &mut out, &cur, false);
        }
        out
    }

    /// Writes `v` as `Σ ψ_word f` for the reduced words `word_of(w)` (crossing positions bottom
    /// first) instead of the canonical ones.
    pub fn peel(
        &self,
        cols: &[u16],
        v: &Val<R::Elem>,
        word_of: fn(&Perm) -> Vec<usize>,
    ) -> Result<Peeled<R::Elem>> {
        let r = self.ring(cols);
        let mut val = v.clone();
        let mut out = Vec::new();
        while let Some((w, f)) = val
            .iter()
            .max_by_key(|(w, _)| (w.length(), std::cmp::Reverse((*w).clone())))
            .map(|(w, f)| (w.clone(), f.clone()))
        {
            let word = word_of(&w);
            let top_first: Vec<usize> = word.iter().rev().copied().collect();
            let b = self.word(cols, &top_first);
            let lead = b
                .get(&w)
                .ok_or_else(|| Error::invalid("skm: a basis element has no leading term"))?;
            let q = if r.is_one(lead) {
                f
            } else if r.is_one(&r.neg(lead)) {
                r.neg(&f)
            } else {
                return Err(Error::invalid("skm: a leading coefficient is not a sign"));
            };
            let sub = self.mul_bottom(cols, &b, &q);
            self.add_into(cols, &mut val, &sub, true);
            if val.contains_key(&w) {
                return Err(Error::invalid(
                    "skm: peeling off a basis element did not cancel its leading term",
                ));
            }
            out.push((w, word, q));
        }
        Ok(out)
    }

    /// The parity of a monomial of a polynomial at the bottom of strands `cols`.
    pub fn mono_odd(&self, cols: &[u16], m: &Monomial) -> bool {
        m.0.iter()
            .enumerate()
            .filter(|&(v, &e)| e % 2 != 0 && self.var_odd(cols, v))
            .count()
            % 2
            == 1
    }

    /// The parity of the crossings `word` (bottom first) on strands of colours `cols`.
    pub fn word_odd(&self, cols: &[u16], word: &[usize]) -> bool {
        let mut c = cols.to_vec();
        let mut p = false;
        for &k in word {
            p ^= self.cross_odd(&c, k);
            c.swap(k, k + 1);
        }
        p
    }

    /// The pairing `⟨c, α_j⟩`: how an upward `j`-strand shifts the weight of colour `c`.
    pub fn shift(&self, c: u16, j: u16) -> i64 {
        self.a[c as usize][j as usize]
    }

    /// The region weight left of upward strands of the given colours, from the weight right of
    /// them.
    pub fn left_of(&self, right: &[i64], cols: &[u16]) -> Vec<i64> {
        let mut r = right.to_vec();
        for &j in cols {
            for (c, x) in r.iter_mut().enumerate() {
                *x += self.shift(c as u16, j);
            }
        }
        r
    }

    fn series_mul(&self, a: &[P<R>], b: &[P<R>], upto: usize) -> Vec<P<R>> {
        let r = &self.plain;
        let mut out = vec![r.zero(); upto + 1];
        for (i, x) in a.iter().enumerate().take(upto + 1) {
            if r.is_zero(x) {
                continue;
            }
            for (j, y) in b.iter().enumerate().take(upto + 1 - i) {
                if r.is_zero(y) {
                    continue;
                }
                out[i + j] = r.add(&out[i + j], &r.mul(x, y));
            }
        }
        out
    }

    fn wser(&self, c: u16, upto: usize) -> Result<Vec<P<R>>> {
        (0..=upto as i64).map(|k| self.z(c, k)).collect()
    }

    /// The counterclockwise bubbles `C_t` of the reference region up to `t^upto`, from the
    /// infinite Grassmannian relation `Σ_r (−1)^{p r} W_r C_{t−r} = δ_{t,0}` (`W` above `C`).
    fn cser(&self, c: u16, upto: usize) -> Result<Vec<P<R>>> {
        if let Some(s) = self.cser.borrow().get(&c)
            && s.len() > upto
        {
            return Ok(s[..=upto].to_vec());
        }
        let r = &self.plain;
        let w = self.wser(c, upto)?;
        let p = self.odd[c as usize];
        let mut out = vec![r.one()];
        for t in 1..=upto {
            let mut s = r.zero();
            for k in 1..=t {
                let term = r.mul(&w[k], &out[t - k]);
                s = if p && k % 2 == 1 {
                    r.sub(&s, &term)
                } else {
                    r.add(&s, &term)
                };
            }
            out.push(r.neg(&s));
        }
        self.cser.borrow_mut().insert(c, out.clone());
        Ok(out)
    }

    /// The coefficients up to `t^upto` of `τ` (or `τ⁻¹`) for bubbles of colour `c` across an
    /// upward `j`-strand with dot `x`: `W_left = τ W_right`, `C_right = C_left τ` with
    /// `τ = Σ (2r+1) x^{2r} t^{2r}` for `c = j` odd, `τ = Σ (r+1) x^r t^r` for `c = j` even,
    /// `τ = 1 + x^{d_jc} t^{d_cj}` for `a_cj < 0`, and `τ = 1` otherwise (BE2 (7.9)–(7.12), with
    /// `t_ij = 1`, `s_ij^pq = 0`). Every coefficient is even, so it commutes with everything.
    fn tau(&self, c: u16, j: u16, x: &P<R>, upto: usize, inverse: bool) -> Vec<P<R>> {
        let r = &self.plain;
        let mut t = vec![r.zero(); upto + 1];
        t[0] = r.one();
        if c == j {
            let step = if self.odd[c as usize] { 2 } else { 1 };
            let xs = r.pow(x, step as u32);
            let mut p = r.one();
            let mut e = step;
            while e <= upto {
                p = r.mul(&p, &xs);
                t[e] = r.mul(&r.int(e as i64 + 1), &p);
                e += step;
            }
        } else {
            let acj = self.a[c as usize][j as usize];
            if acj != 0 {
                let dt = (-acj) as usize;
                if dt <= upto {
                    t[dt] = r.pow(x, (-self.a[j as usize][c as usize]) as u32);
                }
            }
        }
        if !inverse {
            return t;
        }
        let mut out = vec![r.one()];
        for m in 1..=upto {
            let mut s = r.zero();
            for k in 1..=m {
                if !r.is_zero(&t[k]) {
                    s = r.add(&s, &r.mul(&t[k], &out[m - k]));
                }
            }
            out.push(r.neg(&s));
        }
        out
    }

    /// The bubble `W_index` (clockwise) or `C_index` of colour `c` in a region separated from the
    /// reference region by upward strands `(colour, dot)`: the region lies left of them when
    /// `left` holds (the usual case: the reference region is the rightmost one), right of them
    /// otherwise.
    pub fn bubble(
        &self,
        c: u16,
        cw: bool,
        index: i64,
        strands: &[(u16, P<R>)],
        left: bool,
    ) -> Result<P<R>> {
        if index < 0 {
            return Ok(self.plain.zero());
        }
        Ok(self.bubble_series(c, cw, index as usize, strands, left)?[index as usize].clone())
    }

    /// The bubbles of [`Up::bubble`] of every index up to `upto`.
    fn bubble_series(
        &self,
        c: u16,
        cw: bool,
        upto: usize,
        strands: &[(u16, P<R>)],
        left: bool,
    ) -> Result<Rc<Vec<P<R>>>> {
        let key = (c, cw, strands.to_vec(), left);
        if let Some(s) = self.bubble_cache.borrow().get(&key)
            && s.len() > upto
        {
            return Ok(s.clone());
        }
        let mut s = if cw {
            self.wser(c, upto)?
        } else {
            self.cser(c, upto)?
        };
        for (j, x) in strands {
            let t = self.tau(c, *j, x, upto, cw != left);
            s = self.series_mul(&s, &t, upto);
        }
        let s = Rc::new(s);
        self.bubble_cache.borrow_mut().insert(key, s.clone());
        Ok(s)
    }

    /// The strands `cols` at positions `from..`, with their dots.
    pub fn strands(&self, cols: &[u16], from: usize) -> Vec<(u16, P<R>)> {
        cols.iter()
            .enumerate()
            .skip(from)
            .map(|(k, &c)| (c, self.x(k)))
            .collect()
    }

    /// The images of the bubble variables `z` when the reference region moves across upward
    /// strands (`left` when the old reference region lies left of them). The odd bubbles `δ`
    /// slide freely.
    pub fn slide_images(&self, strands: &[(u16, P<R>)], left: bool) -> Result<Rc<Vec<P<R>>>> {
        let key = (strands.to_vec(), left);
        if let Some(v) = self.slide_cache.borrow().get(&key) {
            return Ok(v.clone());
        }
        let mut out = Vec::with_capacity(self.nc * self.kz);
        for c in 0..self.nc as u16 {
            let step = if self.odd[c as usize] { 2 } else { 1 };
            let series = self.bubble_series(c, true, step * self.kz, strands, left)?;
            for k in 1..=self.kz {
                out.push(series[step * k].clone());
            }
        }
        let out = Rc::new(out);
        self.slide_cache.borrow_mut().insert(key, out.clone());
        Ok(out)
    }

    /// Substitutes the images of [`Up::slide_images`] for the bubble variables `z` of a
    /// polynomial at the bottom of strands `cols` (the images are even).
    pub fn subst_z(&self, cols: &[u16], f: &P<R>, images: &[P<R>]) -> P<R> {
        let ring = self.ring(cols);
        let (z, d) = (self.zoff(), self.doff());
        let mut out = ring.zero();
        for (m, c) in &f.terms {
            let mut base = m.clone();
            let mut t: Option<P<R>> = None;
            for (i, &e) in m.0[z..d].iter().enumerate() {
                base.0[z + i] = 0;
                if e > 0 {
                    let p = self.plain.pow(&images[i], e as u32);
                    t = Some(match t {
                        None => p,
                        Some(q) => self.plain.mul(&q, &p),
                    });
                }
            }
            let mono = ring.monomial(base, c.clone());
            let term = match t {
                None => mono,
                Some(q) => ring.mul(&mono, &q),
            };
            out = ring.add(&out, &term);
        }
        out
    }

    /// A coefficient of the term language (a polynomial in the scalars) in this ring.
    pub fn lift(&self, c: &P<R>) -> P<R> {
        let n = self.plain.nvars();
        let off = self.soff();
        Poly {
            terms: c
                .terms
                .iter()
                .map(|(m, x)| {
                    let mut e: Exps = smallvec::smallvec![0; n];
                    e[off..].copy_from_slice(&m.0);
                    (Monomial(e), x.clone())
                })
                .collect(),
        }
    }

    /// Splits a monomial into its dot exponents (`nx` of them), its bubble exponents (`z`, then
    /// `δ`), and its scalar monomial.
    pub fn split(&self, m: &Monomial) -> (Vec<i32>, Vec<i32>, Monomial) {
        let (z, s) = (self.zoff(), self.soff());
        (
            m.0[..z].to_vec(),
            m.0[z..s].to_vec(),
            Monomial::from(&m.0[s..]),
        )
    }

    /// Renames the dots `x_j ↦ x_{j+shift}` for `j < n` (`shift` may be negative when the dots
    /// below `−shift` are absent). The order of the dots is kept, so there is no sign.
    pub fn shift_dots(&self, f: &P<R>, n: usize, shift: isize) -> P<R> {
        self.plain.map_monomials(f, |m| {
            let mut e = m.clone();
            for j in 0..n {
                e.0[j] = 0;
            }
            for j in 0..n {
                if m.0[j] != 0 {
                    e.0[(j as isize + shift) as usize] = m.0[j];
                }
            }
            (e, false)
        })
    }

    /// `v` (from `from` strands) placed among identity strands: `left` colours on its left and
    /// `right` colours on its right. The bubbles keep their variables, so with strands on the
    /// right the caller moves them to the new reference region.
    pub fn embed(
        &self,
        v: &Val<R::Elem>,
        from: usize,
        left: &[u16],
        right: &[u16],
    ) -> Val<R::Elem> {
        let s = left.len();
        let n = s + from + right.len();
        let mut out = Val::new();
        for (w, f) in v {
            let mut p: Vec<u8> = (0..s as u8).collect();
            p.extend(w.0.iter().map(|&b| b + s as u8));
            p.extend((s + from) as u8..n as u8);
            out.insert(Perm(p), self.shift_dots(f, from, s as isize));
        }
        out
    }

    /// Applies upward steps on top of `v` (from strands `cols`), whose top has the colours
    /// `top`.
    pub fn run(
        &self,
        cols: &[u16],
        mut v: Val<R::Elem>,
        top: &mut [u16],
        evs: &[UpEv<R::Elem>],
    ) -> Result<Val<R::Elem>> {
        if top.len() > self.nx || cols.len() > self.nx {
            return Err(Error::invalid(format!(
                "skm: a value on {} strands, but the ring has {} dot variables",
                top.len().max(cols.len()),
                self.nx
            )));
        }
        for ev in evs {
            if v.is_empty() {
                return Ok(v);
            }
            match ev {
                UpEv::Dot(k) => v = self.dot(cols, &v, *k),
                UpEv::Poly(p) => v = self.mul_top(cols, &v, p),
                UpEv::Cross(k) => {
                    v = self.cross(cols, &v, *k);
                    top.swap(*k, k + 1);
                }
                UpEv::Bub {
                    pos,
                    colour,
                    cw,
                    index,
                } => {
                    let p = self.bubble(*colour, *cw, *index, &self.strands(top, *pos), true)?;
                    v = self.mul_top(cols, &v, &p);
                }
            }
        }
        Ok(v)
    }

    /// `a + b` over strands `cols`.
    pub fn add_into_pub(&self, cols: &[u16], a: &mut Val<R::Elem>, b: &Val<R::Elem>) {
        self.add_into(cols, a, b, false);
    }

    /// `a + c · b` over strands `cols` (`c` an integer).
    pub fn add_scaled(&self, cols: &[u16], a: &mut Val<R::Elem>, b: &Val<R::Elem>, c: i64) {
        if c == 0 {
            return;
        }
        let b = if c == 1 || c == -1 {
            b.clone()
        } else {
            self.scale(b, &self.plain.base.int(c.abs()))
        };
        self.add_into(cols, a, &b, c < 0);
    }

    /// Closes the rightmost strand of `v` (from `cols`, right region `right`) to the right:
    /// `(1 ⊗ ε)(v ⊗ 1)(1 ⊗ η')`. The result has the right region `ν = right + α`, which is also
    /// the new reference region of the bubbles. The closed strand is a bubble (BE2 (2.13)) or a
    /// right curl (BE2 (5.20)–(5.21)), with the Koszul signs of bringing its pieces together.
    pub fn ptr_r(
        &self,
        v: &Val<R::Elem>,
        cols: &[u16],
        right: &[i64],
    ) -> Result<(Val<R::Elem>, Vec<i64>)> {
        let n = cols.len();
        let cn = cols[n - 1];
        let nu = self.left_of(right, &[cn]);
        let h = nu[cn as usize];
        let rest = &cols[..n - 1];
        let p = self.odd[cn as usize];
        let eta = self.pi[cn as usize];
        let ring = self.ring(cols);
        let images = self.slide_images(&[(cn, self.x(n - 1))], false)?;
        let mut out = Val::new();
        for (w, word, f) in self.peel(cols, v, adapted_r)? {
            let f = self.subst_z(cols, &f, &images);
            let k0 = w.inverse().0[n - 1] as usize;
            if k0 != n - 1 && cols[k0] != cn {
                return Err(Error::invalid(
                    "skm: a partial trace joins different colours",
                ));
            }
            // the chain crossings below the curl: the moving strand past strands k0+1..n-2
            let c_odd = k0 < n - 1
                && p
                && (k0 + 1..n - 1)
                    .filter(|&j| self.odd[cols[j] as usize])
                    .count()
                    % 2
                    == 1;
            let mut parts: BTreeMap<i32, Vec<(Monomial, R::Elem)>> = BTreeMap::new();
            for (m, c) in &f.terms {
                let e = m.0[n - 1];
                let mut m2 = m.clone();
                m2.0[n - 1] = 0;
                let x_odd = (0..n - 1)
                    .filter(|&j| m2.0[j] % 2 != 0 && self.odd[cols[j] as usize])
                    .count()
                    % 2
                    == 1;
                let b_odd = self.mono_odd(cols, &m2) != x_odd;
                let neg = (b_odd && eta) ^ ((eta ^ (p && e % 2 != 0)) && (x_odd ^ c_odd));
                let c = if neg { ring.base.neg(c) } else { c.clone() };
                parts.entry(e).or_default().push((m2, c));
            }
            for (e, terms) in parts {
                let e = e as i64;
                let fe = self.ring(rest).normalize(terms);
                let mut evs = vec![UpEv::Poly(fe)];
                if k0 == n - 1 {
                    evs.push(UpEv::Poly(self.z(cn, e + 1 - h)?));
                    evs.extend(word.iter().map(|&k| UpEv::Cross(k)));
                } else {
                    // BE2 (5.21): −Σ_r (−1)^{p(r+1)} x^r W_{e−r−h}
                    let r = &self.plain;
                    let mut pe = r.zero();
                    for j in 0..=(e - h).max(-1) {
                        let t = r.mul(&self.xpow(n - 2, j), &self.z(cn, e - j - h)?);
                        pe = if p && (j + 1) % 2 == 1 {
                            r.add(&pe, &t)
                        } else {
                            r.sub(&pe, &t)
                        };
                    }
                    let chain = n - 1 - k0;
                    evs.extend(word[..chain - 1].iter().map(|&k| UpEv::Cross(k)));
                    evs.push(UpEv::Poly(pe));
                    evs.extend(word[chain..].iter().map(|&k| UpEv::Cross(k)));
                }
                let r = self.run(rest, self.ident(n - 1), &mut rest.to_vec(), &evs)?;
                self.add_into(rest, &mut out, &r, false);
            }
        }
        Ok((out, nu))
    }

    /// Closes the leftmost strand of `v` (from `cols`, right region `right`) to the left:
    /// `(ε' ⊗ 1)(1 ⊗ v)(η ⊗ 1)`. The right region and the reference region of the bubbles stay.
    /// The closed strand is a counterclockwise bubble or a left curl (BE2 (5.18)), with the
    /// Koszul signs of bringing its pieces together.
    pub fn ptr_l(&self, v: &Val<R::Elem>, cols: &[u16], right: &[i64]) -> Result<Val<R::Elem>> {
        let n = cols.len();
        let c1 = cols[0];
        let rest = &cols[1..];
        let p = self.odd[c1 as usize];
        let eps = self.pi[c1 as usize];
        let rring = self.ring(rest);
        let mut out = Val::new();
        for (w, word, f) in self.peel(cols, v, adapted_l)? {
            let k0 = w.inverse().0[0] as usize;
            if k0 != 0 && cols[k0] != c1 {
                return Err(Error::invalid(
                    "skm: a partial trace joins different colours",
                ));
            }
            // the crossings below and above the curl
            let (pre_odd, post_odd) = if k0 == 0 {
                (false, self.word_odd(cols, &word))
            } else {
                let pre = p && (1..k0).filter(|&j| self.odd[cols[j] as usize]).count() % 2 == 1;
                let mut mid: Vec<u16> = vec![cols[k0], cols[0]];
                mid.extend_from_slice(&cols[1..k0]);
                mid.extend_from_slice(&cols[k0 + 1..]);
                (pre, self.word_odd(&mid, &word[k0..]))
            };
            let mut parts: BTreeMap<i32, Vec<(Monomial, R::Elem)>> = BTreeMap::new();
            for (m, c) in &f.terms {
                let e = m.0[0];
                let mut m2 = m.clone();
                m2.0[0] = 0;
                let g_odd = self.mono_odd(cols, &m2);
                let neg = if k0 == 0 {
                    (e % 2 != 0 && p && g_odd) ^ (eps && (g_odd ^ post_odd))
                } else {
                    (e % 2 != 0 && p && pre_odd) ^ (eps && post_odd)
                };
                let c = if neg { rring.base.neg(c) } else { c.clone() };
                parts.entry(e).or_default().push((m2, c));
            }
            for (e, terms) in parts {
                let e = e as i64;
                let g = self.shift_dots(&rring.normalize(terms), n, -1);
                let evs = if k0 == 0 {
                    // a counterclockwise bubble with e dots; its outside region is right of the
                    // closed strand
                    let rho = self.left_of(right, rest);
                    let b = self.bubble(
                        c1,
                        false,
                        e + 1 + rho[c1 as usize],
                        &self.strands(rest, 0),
                        true,
                    )?;
                    let mut evs = vec![UpEv::Poly(b), UpEv::Poly(g)];
                    evs.extend(word.iter().map(|&k| UpEv::Cross(k - 1)));
                    evs
                } else {
                    // the object (without the closed strand) at the height of the curl
                    let mut mid: Vec<u16> = rest.to_vec();
                    let s = mid.remove(k0 - 1);
                    mid.insert(0, s);
                    let hnu = self.left_of(right, &mid[1..])[c1 as usize] + 2;
                    let strands = self.strands(&mid, 0);
                    // BE2 (5.18): Σ_r (−1)^{p ⟨i,ν⟩ r} C_{e+⟨i,ν⟩−r} then x^r
                    let r = &self.plain;
                    let mut pe = r.zero();
                    for j in 0..=(e + hnu).max(-1) {
                        let b = self.bubble(c1, false, e + hnu - j, &strands, true)?;
                        let t = r.mul(&self.xpow(0, j), &b);
                        pe = if p && hnu % 2 != 0 && j % 2 == 1 {
                            r.sub(&pe, &t)
                        } else {
                            r.add(&pe, &t)
                        };
                    }
                    let mut evs = vec![UpEv::Poly(g)];
                    evs.extend(word[..k0 - 1].iter().map(|&k| UpEv::Cross(k - 1)));
                    evs.push(UpEv::Poly(pe));
                    evs.extend(word[k0..].iter().map(|&k| UpEv::Cross(k - 1)));
                    evs
                };
                let r = self.run(rest, self.ident(n - 1), &mut rest.to_vec(), &evs)?;
                self.add_into(rest, &mut out, &r, false);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin;
    use crate::context::ContextSpec;
    use crate::elab::CTerm;
    use crate::ring::Rationals;
    use crate::term::{Layer, Letter, Word};

    fn ctx(spec: &str) -> Context<Rationals> {
        let pres = builtin::presentation("be2").unwrap();
        Context::new(pres, ContextSpec::parse(spec).unwrap(), Rationals).unwrap()
    }

    fn gen_id(c: &Context<Rationals>, name: &str) -> u16 {
        c.pres
            .generators
            .iter()
            .position(|g| g.name == name)
            .unwrap() as u16
    }

    /// A value as a term of the context: dots at the bottom (the first variable highest), then
    /// the crossings of the canonical word.
    fn to_term(
        c: &Context<Rationals>,
        up: &Up<'_, Rationals>,
        cols: &[u16],
        v: &Val<<Rationals as Ring>::Elem>,
    ) -> CTerm<Rationals> {
        let (x, psi) = (gen_id(c, "x"), gen_id(c, "psi"));
        let src: Vec<Letter> = cols
            .iter()
            .map(|&col| Letter {
                colour: col,
                up: true,
            })
            .collect();
        let mut words = Vec::new();
        for (w, f) in v {
            for (m, k) in &f.terms {
                // The rewriter lists the dots with the first variable lowest: reverse the order,
                // with the Koszul sign of the odd dots.
                let mut sign = false;
                for i in 0..cols.len() {
                    for j in i + 1..cols.len() {
                        if up.var_odd(cols, i)
                            && up.var_odd(cols, j)
                            && m.0[i] % 2 == 1
                            && m.0[j] % 2 == 1
                        {
                            sign = !sign;
                        }
                    }
                }
                let k = if sign { Rationals.neg(k) } else { k.clone() };
                let mut layers = Vec::new();
                for (var, &col) in cols.iter().enumerate() {
                    for _ in 0..m.0[var] {
                        layers.push(Layer {
                            generator: x,
                            pos: var as u16,
                            binding: vec![col],
                        });
                    }
                }
                assert!(m.0[up.nx..].iter().all(|&e| e == 0));
                let mut cur = cols.to_vec();
                for &kk in w.reduced_word().iter().rev() {
                    layers.push(Layer {
                        generator: psi,
                        pos: kk as u16,
                        binding: vec![cur[kk], cur[kk + 1]],
                    });
                    cur.swap(kk, kk + 1);
                }
                words.push((
                    Word {
                        source: src.clone(),
                        layers,
                    },
                    c.coeff.constant(k.clone()),
                ));
            }
        }
        canon(c, words)
    }

    /// Words in canonical layering, with the interchange signs.
    fn canon(
        c: &Context<Rationals>,
        words: Vec<(Word, Poly<<Rationals as Ring>::Elem>)>,
    ) -> CTerm<Rationals> {
        let words = words
            .into_iter()
            .map(|(w, k)| {
                let (w, neg) = c.canonical(&w);
                (w, if neg { c.coeff.neg(&k) } else { k })
            })
            .collect();
        c.normalize(words)
    }

    /// A word of dots and crossings, evaluated by the engine from the bottom.
    fn eval(
        up: &Up<'_, Rationals>,
        cols: &[u16],
        word: &[(bool, usize)],
    ) -> Val<<Rationals as Ring>::Elem> {
        let mut v = up.ident(cols.len());
        for &(is_dot, k) in word {
            v = if is_dot {
                up.dot(cols, &v, k)
            } else {
                up.cross(cols, &v, k)
            };
        }
        v
    }

    fn as_word(c: &Context<Rationals>, cols: &[u16], word: &[(bool, usize)]) -> CTerm<Rationals> {
        let (x, psi) = (gen_id(c, "x"), gen_id(c, "psi"));
        let src: Vec<Letter> = cols
            .iter()
            .map(|&col| Letter {
                colour: col,
                up: true,
            })
            .collect();
        let mut cur = cols.to_vec();
        let mut layers = Vec::new();
        for &(is_dot, k) in word {
            if is_dot {
                layers.push(Layer {
                    generator: x,
                    pos: k as u16,
                    binding: vec![cur[k]],
                });
            } else {
                layers.push(Layer {
                    generator: psi,
                    pos: k as u16,
                    binding: vec![cur[k], cur[k + 1]],
                });
                cur.swap(k, k + 1);
            }
        }
        canon(
            c,
            vec![(
                Word {
                    source: src,
                    layers,
                },
                c.coeff.one(),
            )],
        )
    }

    /// The engine agrees with straightening by the presentation's relations on random words.
    #[test]
    fn agrees_with_rewriting() {
        use crate::rep::Evaluator;
        let mut seed = 12345u64;
        let mut rnd = |m: usize| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % m
        };
        for (spec, colsets) in [
            ("be2 type=A1 odd=i", vec![vec![0u16, 0, 0], vec![0, 0]]),
            (
                "be2 type=B2 odd=j",
                vec![vec![1u16, 0, 1], vec![0, 1, 1], vec![1, 1, 0, 1]],
            ),
            (
                "be2 type=A1xA1 odd=i,j",
                vec![vec![0u16, 1, 0], vec![0, 1, 1, 0]],
            ),
            ("be2 type=A2", vec![vec![0u16, 1, 0], vec![1, 0, 0]]),
        ] {
            let c = ctx(spec);
            let decl = c
                .pres
                .representations
                .iter()
                .find(|d| d.name == "quiver-hecke-basis")
                .unwrap();
            let prov = crate::rep::Provenance {
                strategy: crate::rep::Strategy::BasisNormalForm,
                method: decl.name.clone(),
                certificate: decl.certificate.clone(),
            };
            let qh = crate::rep::straighten::QuiverHeckeBasis::new(
                &c,
                gen_id(&c, "x"),
                gen_id(&c, "psi"),
                prov,
            )
            .unwrap();
            let up = Up::new(&c, 6, 2).unwrap();
            for cols in colsets {
                let n = cols.len();
                for _ in 0..150 {
                    let len = 2 + rnd(9);
                    let word: Vec<(bool, usize)> = (0..len)
                        .map(|_| {
                            if rnd(3) == 0 {
                                (true, rnd(n))
                            } else {
                                (false, rnd(n - 1))
                            }
                        })
                        .collect();
                    let v = eval(&up, &cols, &word);
                    let mine = to_term(&c, &up, &cols, &v);
                    let theirs = qh.normal_form(&as_word(&c, &cols, &word)).unwrap();
                    let theirs = canon(&c, theirs.terms);
                    assert_eq!(mine, theirs, "{spec} {cols:?} {word:?}");
                }
            }
        }
    }
    #[test]
    fn grouped_mul_top_matches_eager() {
        for spec in ["be2 type=A2", "be2 type=B2 odd=j", "be2 type=A1xA1 odd=i,j"] {
            let c = ctx(spec);
            let up = Up::new(&c, 3, 3).unwrap();
            for cols in [vec![0, 1, 0], vec![1, 1, 0], vec![1, 0, 1]] {
                let r = up.ring(&cols);
                let mut v = up.word(&cols, &[0, 1, 0]);
                up.add_into(&cols, &mut v, &up.ident(3), false);
                for degree in 0..4 {
                    let mut p = r.zero();
                    for dot in 0..3 {
                        for var in up.nx..r.nvars() {
                            let mut m = Monomial::one(r.nvars());
                            m.0[dot] = degree;
                            m.0[var] = 1;
                            r.add_assign(
                                &mut p,
                                &r.monomial(m, r.base.int(if var % 2 == 0 { 2 } else { -3 })),
                            );
                        }
                    }
                    // Mixed tail products exercise two odd bubbles; multiplying the
                    // value by an odd bubble also exercises its exterior square.
                    let tail = r.mul(&r.var(up.doff()), &r.var(up.doff() + 1));
                    r.add_assign(&mut p, &r.mul(&up.x(0), &tail));
                    let v = up.mul_bottom(&cols, &v, &r.add(&r.one(), &r.var(up.doff())));
                    assert_eq!(
                        up.mul_top(&cols, &v, &p),
                        up.mul_top_eager(&cols, &v, &p),
                        "{spec} {cols:?} {degree}"
                    );
                }
            }
        }
    }
    #[test]
    #[ignore = "paired arithmetic timing; run release with --nocapture"]
    fn arithmetic_timing() {
        for spec in ["be2 type=A2", "be2 type=A1xA1 odd=i,j"] {
            let c = ctx(spec);
            let up = Up::new(&c, 3, 4).unwrap();
            let cols = [0, 1, 0];
            let r = up.ring(&cols);
            let v = up.word(&cols, &[0, 1, 0]);
            for dense in [false, true] {
                let mut p = r.zero();
                for dot in 0..3 {
                    for var in up.nx..if dense { r.nvars() } else { up.nx + 1 } {
                        let mut m = Monomial::one(r.nvars());
                        m.0[dot] = 3;
                        m.0[var] = 1;
                        r.add_assign(&mut p, &r.monomial(m, r.base.one()));
                    }
                }
                assert_eq!(up.mul_top(&cols, &v, &p), up.mul_top_eager(&cols, &v, &p));
                let mut samples = [Vec::new(), Vec::new()];
                for pass in 0..9 {
                    for which in [pass % 2, 1 - pass % 2] {
                        let start = std::time::Instant::now();
                        for _ in 0..100 {
                            std::hint::black_box(if which == 0 {
                                up.mul_top_eager(&cols, &v, &p)
                            } else {
                                up.mul_top(&cols, &v, &p)
                            });
                        }
                        samples[which].push(start.elapsed().as_secs_f64() * 1e4);
                    }
                }
                for s in &mut samples {
                    s.sort_by(f64::total_cmp);
                }
                println!(
                    "mul_top {spec} dense={dense} eager_us={} candidate_us={} samples={samples:?}",
                    samples[0][4], samples[1][4]
                );
            }
        }
    }
}
