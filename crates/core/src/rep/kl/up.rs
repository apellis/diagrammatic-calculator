//! Upward diagrams of the Khovanov–Lauda 2-category with bubbles: `R(ν) ⊗ Π_λ`.
//!
//! An upward 2-morphism `E_i 1_λ → E_j 1_λ` is written in the polynomial representation of the
//! KLR algebra (the smash product of `crate::rep::smash`), with the clockwise bubbles of the
//! rightmost region as extra variables `z_{c,k}` (`k ≥ 1`) fixed by every permutation. By
//! nondegeneracy (KL III Theorem 1.2) this is faithful: `END(E_ν 1_λ) ≅ R(ν) ⊗ Π_λ`, with `Π_λ`
//! the polynomial ring in the clockwise bubbles of positive degree index.
//!
//! Bubbles are named by their degree index: the clockwise bubble with `m` dots in a region `ρ` is
//! `W_k(ρ)` with `k = m + 1 − ⟨i, ρ⟩`, the counterclockwise one `C_k(ρ)` with
//! `k = m + 1 + ⟨i, ρ⟩`, `W_0 = C_0 = 1`, negative indices give zero, and the series
//! `W(t) = Σ W_k t^k`, `C(t) = Σ C_k t^k` are mutually inverse (infinite Grassmannian relation,
//! which also defines the fake bubbles). Across an upward `j`-strand with a dot `x`, the bubbles
//! of colour `i` satisfy `W_left = τ W_right` and `C_right = C_left τ` with `τ = 1/(1 − x t)²` for
//! `i = j`, `τ = 1 + x^{d_ji} t^{d_ij}` (`d_ij = −a_ij`) for `a_ij < 0`, and `τ = 1` otherwise
//! (KL III Propositions 3.3 and 3.4, in all degrees: categorification-lean
//! `KL3.Diagram.slides_same`, `slides_gen`).
//!
//! The partial traces (closing the rightmost strand to the right, or the leftmost to the left)
//! are computed in the basis `ψ_ŵ x^a` with reduced words adapted to the closed strand, using the
//! curl relations (Markov lemma, KL III §3.2).

use crate::context::{Context, PARAM_VARS};
use crate::error::{Error, Result};
use crate::fxhash::{self, FxHashMap};
use crate::perm::Perm;
use crate::poly::{Exps, Monomial, Poly, PolyRing};
use crate::rep::smash::{Alg, Den, Frac, Value};
use crate::ring::Ring;
use crate::term::{Letter, Object};
use std::cell::RefCell;
use std::collections::BTreeMap;

/// The message of the error raised when a bubble index exceeds the allocated variables; the
/// caller retries with more.
pub(super) const OVERFLOW: &str = "kl: bubble index above the allocated variables";

/// One step of an upward diagram, bottom first.
#[derive(Clone, Debug)]
pub(super) enum UpEv<E> {
    /// A dot on the strand at a position.
    Dot(usize),
    /// A crossing of the strands at `k` and `k + 1`.
    Cross(usize),
    /// Multiplication by a polynomial in the dots at this height and the bubbles.
    Poly(Poly<E>),
    /// A bubble in the region left of position `pos`, given by its degree index.
    Bub {
        pos: usize,
        colour: u16,
        cw: bool,
        index: i64,
    },
}

/// The ring and the arithmetic of upward diagrams on up to `nx` strands, with clockwise bubbles
/// of index up to `kz` for each colour.
pub(super) struct Up<'a, R: Ring> {
    pub ctx: &'a Context<R>,
    /// The Cartan matrix.
    pub a: Vec<Vec<i64>>,
    pub ring: PolyRing<R>,
    pub nx: usize,
    pub kz: usize,
    pub nc: usize,
    algs: Vec<Alg<R>>,
    cser: RefCell<FxHashMap<u16, Vec<Poly<R::Elem>>>>,
    basis: RefCell<BasisCache<R::Elem>>,
}

pub(super) type P<R> = Poly<<R as Ring>::Elem>;
type BasisCache<E> = FxHashMap<(Vec<u16>, Vec<usize>), Value<E>>;
/// A term of [`Up::peel`]: the permutation, its reduced word and the polynomial.
pub(super) type Peeled<R> = (Perm, Vec<usize>, P<R>);

/// The colours of an object of upward strands.
pub(super) fn ups(cols: &[u16]) -> Object {
    cols.iter()
        .map(|&c| Letter {
            colour: c,
            up: true,
        })
        .collect()
}

/// A reduced word of `w` (crossing positions, bottom first) whose first crossings carry the
/// strand ending at the rightmost top position there: `ψ_{k} … ψ_{n−2}`, then a word of the rest.
pub(crate) fn adapted_r(w: &Perm) -> Vec<usize> {
    let n = w.n();
    if n <= 1 {
        return vec![];
    }
    let k0 = w.inverse().0[n - 1] as usize;
    let mut out: Vec<usize> = (k0..n - 1).collect();
    let c = Perm(
        (0..n)
            .map(|j| {
                if j == k0 {
                    (n - 1) as u8
                } else if j > k0 {
                    (j - 1) as u8
                } else {
                    j as u8
                }
            })
            .collect(),
    );
    let u = w.compose(&c.inverse());
    debug_assert_eq!(u.0[n - 1] as usize, n - 1);
    out.extend(adapted_r(&Perm(u.0[..n - 1].to_vec())));
    out
}

/// The mirror of [`adapted_r`]: the strand ending at top position 0 moves there first.
pub(crate) fn adapted_l(w: &Perm) -> Vec<usize> {
    let n = w.n();
    if n <= 1 {
        return vec![];
    }
    let k0 = w.inverse().0[0] as usize;
    let mut out: Vec<usize> = (0..k0).rev().collect();
    let c = Perm(
        (0..n)
            .map(|j| {
                if j == k0 {
                    0
                } else if j < k0 {
                    (j + 1) as u8
                } else {
                    j as u8
                }
            })
            .collect(),
    );
    let u = w.compose(&c.inverse());
    debug_assert_eq!(u.0[0], 0);
    let rest = Perm(u.0[1..].iter().map(|&b| b - 1).collect());
    out.extend(adapted_l(&rest).into_iter().map(|k| k + 1));
    out
}

impl<'a, R: Ring> Up<'a, R> {
    pub fn new(ctx: &'a Context<R>, nx: usize, kz: usize) -> Result<Self> {
        let cartan = ctx
            .cartan
            .as_ref()
            .ok_or_else(|| Error::precondition("the KL normal form needs a Cartan datum"))?;
        let nc = ctx.colours.len();
        let mut names: Vec<String> = (1..=nx).map(|k| format!("x_{k}")).collect();
        for c in 0..nc {
            for k in 1..=kz {
                names.push(format!("z_{c}_{k}"));
            }
        }
        names.extend(ctx.spec.scalars.iter().cloned());
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let ring = PolyRing::new(ctx.base.clone(), &refs);
        let algs = (0..=nx).map(|n| Alg::new(ring.clone(), n)).collect();
        Ok(Up {
            ctx,
            a: cartan.a.clone(),
            ring,
            nx,
            kz,
            nc,
            algs,
            cser: RefCell::new(fxhash::map()),
            basis: RefCell::new(fxhash::map()),
        })
    }

    pub fn alg(&self, n: usize) -> &Alg<R> {
        &self.algs[n]
    }

    fn zoff(&self) -> usize {
        self.nx
    }

    fn soff(&self) -> usize {
        self.nx + self.nc * self.kz
    }

    pub fn x(&self, k: usize) -> P<R> {
        self.ring.var(k)
    }

    fn xpow(&self, k: usize, e: i64) -> P<R> {
        let mut m: Exps = smallvec::smallvec![0; self.ring.nvars()];
        m[k] = e as i32;
        self.ring.monomial(Monomial(m), self.ring.base.one())
    }

    /// `W_k` of the reference region (the rightmost one).
    pub fn z(&self, c: u16, k: i64) -> Result<P<R>> {
        if k < 0 {
            return Ok(self.ring.zero());
        }
        if k == 0 {
            return Ok(self.ring.one());
        }
        if k as usize > self.kz {
            return Err(Error::limit(OVERFLOW));
        }
        Ok(self
            .ring
            .var(self.zoff() + c as usize * self.kz + k as usize - 1))
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
        let mut out = vec![self.ring.zero(); upto + 1];
        for (i, x) in a.iter().enumerate().take(upto + 1) {
            if self.ring.is_zero(x) {
                continue;
            }
            for (j, y) in b.iter().enumerate().take(upto + 1 - i) {
                if self.ring.is_zero(y) {
                    continue;
                }
                let p = self.ring.mul(x, y);
                out[i + j] = self.ring.add(&out[i + j], &p);
            }
        }
        out
    }

    fn wser(&self, c: u16, upto: usize) -> Result<Vec<P<R>>> {
        (0..=upto as i64).map(|k| self.z(c, k)).collect()
    }

    /// `C = W⁻¹` of the reference region, up to `t^upto`.
    fn cser(&self, c: u16, upto: usize) -> Result<Vec<P<R>>> {
        if let Some(s) = self.cser.borrow().get(&c)
            && s.len() > upto
        {
            return Ok(s[..=upto].to_vec());
        }
        let w = self.wser(c, upto)?;
        let mut out = vec![self.ring.one()];
        for m in 1..=upto {
            let mut s = self.ring.zero();
            for k in 1..=m {
                s = self.ring.add(&s, &self.ring.mul(&w[k], &out[m - k]));
            }
            out.push(self.ring.neg(&s));
        }
        self.cser.borrow_mut().insert(c, out.clone());
        Ok(out)
    }

    /// The coefficients up to `t^upto` of `τ` (or `τ⁻¹`) for bubbles of colour `c` across a
    /// `j`-strand with dot `x`.
    fn tau(&self, c: u16, j: u16, x: &P<R>, upto: usize, inverse: bool) -> Vec<P<R>> {
        let r = &self.ring;
        let mut out = vec![r.zero(); upto + 1];
        out[0] = r.one();
        if c == j {
            if inverse {
                if upto >= 1 {
                    out[1] = r.mul(&r.int(-2), x);
                }
                if upto >= 2 {
                    out[2] = r.mul(x, x);
                }
            } else {
                let mut p = r.one();
                for (e, o) in out.iter_mut().enumerate().skip(1) {
                    p = r.mul(&p, x);
                    *o = r.mul(&r.int(e as i64 + 1), &p);
                }
            }
            return out;
        }
        let acj = self.a[c as usize][j as usize];
        if acj == 0 {
            return out;
        }
        let (dt, dx) = ((-acj) as usize, -self.a[j as usize][c as usize]);
        let xd = r.pow(x, dx as u32);
        let mut m = 1;
        let mut term = r.one();
        while m * dt <= upto {
            term = r.mul(&term, &xd);
            if inverse && m % 2 == 1 {
                out[m * dt] = r.neg(&term);
            } else {
                out[m * dt] = term.clone();
            }
            if !inverse {
                break;
            }
            m += 1;
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
            return Ok(self.ring.zero());
        }
        let upto = index as usize;
        let mut s = if cw {
            self.wser(c, upto)?
        } else {
            self.cser(c, upto)?
        };
        for (j, x) in strands {
            let t = self.tau(c, *j, x, upto, cw != left);
            s = self.series_mul(&s, &t, upto);
        }
        Ok(s.swap_remove(upto))
    }

    /// The strands `cols` at positions `from..`, with their dots.
    fn strands(&self, cols: &[u16], from: usize) -> Vec<(u16, P<R>)> {
        cols.iter()
            .enumerate()
            .skip(from)
            .map(|(k, &c)| (c, self.x(k)))
            .collect()
    }

    /// A coefficient of the term language (a polynomial in the scalars) in this ring.
    pub fn lift(&self, c: &P<R>) -> P<R> {
        let n = self.ring.nvars();
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

    /// Splits a monomial into its dot exponents (`nx` of them), its bubble exponents and its
    /// scalar monomial.
    pub fn split(&self, m: &Monomial) -> (Vec<i32>, Vec<i32>, Monomial) {
        let (z, s) = (self.zoff(), self.soff());
        (
            m.0[..z].to_vec(),
            m.0[z..s].to_vec(),
            Monomial::from(&m.0[s..]),
        )
    }

    /// `P_ab(x_k, x_{k+1})`, as in the KLR polynomial representation.
    fn p_poly(&self, a: u16, b: u16, k: usize) -> Result<P<R>> {
        if a < b {
            return Ok(self.ring.one());
        }
        let q = self
            .ctx
            .params
            .get("Q")
            .and_then(|t| t.get(&(b, a)))
            .ok_or_else(|| Error::precondition("the KL normal form needs the parameter Q"))?;
        let np = PARAM_VARS.len();
        let mut terms = Vec::new();
        for (m, c) in &q.terms {
            if m.0[2] != 0 {
                return Err(Error::invalid("Q may only use the variables u and v"));
            }
            let mut e: Exps = smallvec::smallvec![0; self.ring.nvars()];
            e[k] = m.0[0];
            e[k + 1] = m.0[1];
            e[self.soff()..].copy_from_slice(&m.0[np..]);
            terms.push((Monomial(e), c.clone()));
        }
        Ok(self.ring.normalize(terms))
    }

    pub fn ident(&self, cols: &[u16]) -> Value<R::Elem> {
        let n = cols.len();
        let mut v = Value::new();
        v.insert(
            (ups(cols), Perm::identity(n)),
            self.alg(n).poly(self.ring.one()),
        );
        v
    }

    /// Multiplies every coefficient by a polynomial that the permutations fix or that sits on
    /// top (multiplication after the diagram).
    pub fn mul_poly(&self, n: usize, v: &Value<R::Elem>, p: &P<R>) -> Value<R::Elem> {
        if self.ring.is_zero(p) {
            return Value::new();
        }
        let alg = self.alg(n);
        let f = alg.poly(p.clone());
        let mut out = Value::new();
        for (k, r) in v {
            let x = alg.mul(r, &f);
            if !self.ring.is_zero(&x.num) {
                out.insert(k.clone(), x);
            }
        }
        out
    }

    pub fn scale(&self, v: &Value<R::Elem>, c: i64) -> Value<R::Elem> {
        if c == 1 {
            return v.clone();
        }
        let k = self.ring.base.int(c);
        let mut out = Value::new();
        for (key, f) in v {
            let num = self.ring.scale(&f.num, &k);
            if !self.ring.is_zero(&num) {
                out.insert(
                    key.clone(),
                    Frac {
                        num,
                        den: f.den.clone(),
                    },
                );
            }
        }
        out
    }

    pub fn add_into(&self, n: usize, a: &mut Value<R::Elem>, b: &Value<R::Elem>) {
        self.alg(n).val_add(a, b);
    }

    /// Applies upward steps on top of `v`, whose top object has the colours `top`.
    pub fn run(
        &self,
        mut v: Value<R::Elem>,
        top: &mut [u16],
        evs: &[UpEv<R::Elem>],
    ) -> Result<Value<R::Elem>> {
        let n = top.len();
        if n > self.nx {
            return Err(Error::invalid(format!(
                "kl: a value on {n} strands, but the ring has {} dot variables",
                self.nx
            )));
        }
        let alg = self.alg(n);
        for ev in evs {
            if v.is_empty() {
                return Ok(v);
            }
            match ev {
                UpEv::Dot(k) => v = self.mul_poly(n, &v, &self.x(*k)),
                UpEv::Poly(p) => v = self.mul_poly(n, &v, p),
                UpEv::Bub {
                    pos,
                    colour,
                    cw,
                    index,
                } => {
                    let p = self.bubble(*colour, *cw, *index, &self.strands(top, *pos), true)?;
                    v = self.mul_poly(n, &v, &p);
                }
                UpEv::Cross(k) => {
                    let k = *k;
                    let o = ups(top);
                    let (a, b) = (top[k], top[k + 1]);
                    let mut g = Value::new();
                    if a == b {
                        let mut den: Den = smallvec::smallvec![0; n * n];
                        den[k * n + k + 1] = 1;
                        let inv = Frac {
                            num: self.ring.one(),
                            den,
                        };
                        g.insert((o.clone(), Perm::identity(n)), inv.clone());
                        g.insert((o, Perm::simple(n, k)), alg.neg(&inv));
                    } else {
                        let p = self.p_poly(a, b, k)?;
                        g.insert((o, Perm::simple(n, k)), alg.poly(p));
                    }
                    v = alg.val_mul(&g, &v);
                    top.swap(k, k + 1);
                }
            }
        }
        Ok(v)
    }

    /// The value of `ψ_word e(cols)`, with crossings bottom first.
    fn crossings(&self, cols: &[u16], word: &[usize]) -> Result<Value<R::Elem>> {
        let key = (cols.to_vec(), word.to_vec());
        if let Some(v) = self.basis.borrow().get(&key) {
            return Ok(v.clone());
        }
        let evs: Vec<UpEv<R::Elem>> = word.iter().map(|&k| UpEv::Cross(k)).collect();
        let v = self.run(self.ident(cols), &mut cols.to_vec(), &evs)?;
        self.basis.borrow_mut().insert(key, v.clone());
        Ok(v)
    }

    /// Writes `v` (from the upward object `cols`) as `Σ ψ_ŵ ∘ f e(cols)` with `f` a polynomial in
    /// the dots at the bottom and the bubbles, for the reduced words `word_of(w)` (bottom first).
    pub fn peel(
        &self,
        v: &Value<R::Elem>,
        cols: &[u16],
        word_of: fn(&Perm) -> Vec<usize>,
    ) -> Result<Vec<Peeled<R>>> {
        let n = cols.len();
        let alg = self.alg(n);
        let obj = ups(cols);
        let mut val = v.clone();
        let mut out = Vec::new();
        while let Some(((i, w), r)) = val
            .iter()
            .max_by_key(|((_, w), _)| (w.length(), std::cmp::Reverse(w.clone())))
            .map(|(k, r)| (k.clone(), r.clone()))
        {
            if i != obj {
                return Err(Error::invalid("kl: an upward value has two sources"));
            }
            let word = word_of(&w);
            let b = self.crossings(cols, &word)?;
            let lead = b
                .get(&(i.clone(), w.clone()))
                .ok_or_else(|| Error::invalid("kl: a basis element has no leading term"))?;
            let q = alg
                .div_to_poly(&r, lead)
                .ok_or_else(|| Error::invalid("kl: an upward value is not in the algebra"))?;
            let f = alg.act_poly(&w.inverse(), &q);
            let mut fv = Value::new();
            fv.insert((i.clone(), Perm::identity(n)), alg.poly(self.ring.neg(&f)));
            let sub = alg.val_mul(&b, &fv);
            alg.val_add(&mut val, &sub);
            if val.contains_key(&(i.clone(), w.clone())) {
                return Err(Error::invalid(
                    "kl: peeling off a basis element did not cancel its leading term",
                ));
            }
            out.push((w, word, f));
        }
        Ok(out)
    }

    /// Splits `f` by the exponent of `x_k`, removing it.
    fn by_power(&self, f: &P<R>, k: usize) -> BTreeMap<i32, P<R>> {
        let mut parts: BTreeMap<i32, Vec<(Monomial, R::Elem)>> = BTreeMap::new();
        for (m, c) in &f.terms {
            let mut m2 = m.clone();
            let e = m2.0[k];
            m2.0[k] = 0;
            parts.entry(e).or_default().push((m2, c.clone()));
        }
        parts
            .into_iter()
            .map(|(e, t)| (e, self.ring.normalize(t)))
            .collect()
    }

    /// Renames the dots `x_j ↦ x_{j+shift}` for `j < n` (`shift` may be negative when the dots
    /// below `−shift` are absent).
    pub fn shift_dots(&self, f: &P<R>, n: usize, shift: isize) -> P<R> {
        self.ring.map_monomials(f, |m| {
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

    /// Substitutes polynomials for the bubble variables (`images[c·kz + k − 1]` for `z_{c,k}`).
    pub fn subst_z(&self, f: &P<R>, images: &[P<R>]) -> P<R> {
        let (z, s) = (self.zoff(), self.soff());
        let mut out = self.ring.zero();
        for (m, c) in &f.terms {
            let mut base = m.clone();
            let mut t: Option<P<R>> = None;
            for (i, &e) in m.0[z..s].iter().enumerate() {
                base.0[z + i] = 0;
                if e > 0 {
                    let p = self.ring.pow(&images[i], e as u32);
                    t = Some(match t {
                        None => p,
                        Some(q) => self.ring.mul(&q, &p),
                    });
                }
            }
            let mono = self.ring.monomial(base, c.clone());
            let term = match t {
                None => mono,
                Some(q) => self.ring.mul(&mono, &q),
            };
            out = self.ring.add(&out, &term);
        }
        out
    }

    /// The images of every bubble variable when the reference region moves across upward
    /// strands: `left` when the old reference region lies left of the strands.
    pub fn slide_images(&self, strands: &[(u16, P<R>)], left: bool) -> Result<Vec<P<R>>> {
        let mut out = Vec::with_capacity(self.nc * self.kz);
        for c in 0..self.nc as u16 {
            for k in 1..=self.kz as i64 {
                out.push(self.bubble(c, true, k, strands, left)?);
            }
        }
        Ok(out)
    }

    /// Closes the rightmost strand of `v` (from `cols`, right region `right`) to the right. The
    /// result has the right region `ν = right + α`, which is also the new reference region of
    /// the bubbles.
    pub fn ptr_r(
        &self,
        v: &Value<R::Elem>,
        cols: &[u16],
        right: &[i64],
    ) -> Result<(Value<R::Elem>, Vec<i64>)> {
        let n = cols.len();
        let cn = cols[n - 1];
        let nu = self.left_of(right, &[cn]);
        let h = nu[cn as usize];
        let rest = &cols[..n - 1];
        let m = n - 1;
        let alg = self.alg(m);
        let images = self.slide_images(&[(cn, self.x(n - 1))], false)?;
        let mut out = Value::new();
        for (w, word, f) in self.peel(v, cols, adapted_r)? {
            let f = self.subst_z(&f, &images);
            let parts = self.by_power(&f, n - 1);
            let k0 = w.inverse().0[n - 1] as usize;
            if k0 == n - 1 {
                let mut p = self.ring.zero();
                for (e, fe) in &parts {
                    let b = self.z(cn, *e as i64 + 1 - h)?;
                    p = self.ring.add(&p, &self.ring.mul(fe, &b));
                }
                let mut evs = vec![UpEv::Poly(p)];
                evs.extend(word.iter().map(|&k| UpEv::Cross(k)));
                let r = self.run(self.ident(rest), &mut rest.to_vec(), &evs)?;
                alg.val_add(&mut out, &r);
                continue;
            }
            if cols[k0] != cn {
                return Err(Error::invalid(
                    "kl: a partial trace joins different colours",
                ));
            }
            // the curl ptr(ψ_{n−1}) = −Σ_f x^{−h−f} W_f
            let mut curl = self.ring.zero();
            for f in 0..=(-h) {
                let t = self.ring.mul(&self.xpow(n - 2, -h - f), &self.z(cn, f)?);
                curl = self.ring.sub(&curl, &t);
            }
            let chain = n - 1 - k0;
            for (e, fe) in &parts {
                let e = *e as i64;
                let mut pe = self.ring.mul(&self.xpow(n - 2, e), &curl);
                for j in 0..e {
                    let t = self.ring.mul(&self.xpow(n - 2, j), &self.z(cn, e - j - h)?);
                    pe = self.ring.sub(&pe, &t);
                }
                let mut evs = vec![UpEv::Poly(fe.clone())];
                evs.extend(word[..chain - 1].iter().map(|&k| UpEv::Cross(k)));
                evs.push(UpEv::Poly(pe));
                evs.extend(word[chain..].iter().map(|&k| UpEv::Cross(k)));
                let r = self.run(self.ident(rest), &mut rest.to_vec(), &evs)?;
                alg.val_add(&mut out, &r);
            }
        }
        Ok((out, nu))
    }

    /// Closes the leftmost strand of `v` (from `cols`, right region `right`) to the left; the
    /// right region and the reference region of the bubbles stay.
    pub fn ptr_l(&self, v: &Value<R::Elem>, cols: &[u16], right: &[i64]) -> Result<Value<R::Elem>> {
        let n = cols.len();
        let c1 = cols[0];
        let rest = &cols[1..];
        let m = n - 1;
        let alg = self.alg(m);
        let mut out = Value::new();
        for (w, word, f) in self.peel(v, cols, adapted_l)? {
            let parts: Vec<(i64, P<R>)> = self
                .by_power(&f, 0)
                .into_iter()
                .map(|(e, p)| (e as i64, self.shift_dots(&p, n, -1)))
                .collect();
            let k0 = w.inverse().0[0] as usize;
            if k0 == 0 {
                // a bubble with e dots, outside region right of the closed strand
                let rho = self.left_of(right, rest);
                let strands = self.strands(rest, 0);
                let mut p = self.ring.zero();
                for (e, fe) in &parts {
                    let b = self.bubble(c1, false, e + 1 + rho[c1 as usize], &strands, true)?;
                    p = self.ring.add(&p, &self.ring.mul(fe, &b));
                }
                let mut evs = vec![UpEv::Poly(p)];
                evs.extend(word.iter().map(|&k| UpEv::Cross(k - 1)));
                let r = self.run(self.ident(rest), &mut rest.to_vec(), &evs)?;
                alg.val_add(&mut out, &r);
                continue;
            }
            if cols[k0] != c1 {
                return Err(Error::invalid(
                    "kl: a partial trace joins different colours",
                ));
            }
            // the object (without the closed strand) at the height of the curl
            let mut mid: Vec<u16> = rest.to_vec();
            let s = mid.remove(k0 - 1);
            mid.insert(0, s);
            let h = self.left_of(right, &mid[1..])[c1 as usize];
            let strands = self.strands(&mid, 0);
            let mut curl = self.ring.zero();
            for g in 0..=(h + 2) {
                let t = self.ring.mul(
                    &self.xpow(0, h + 2 - g),
                    &self.bubble(c1, false, g, &strands, true)?,
                );
                curl = self.ring.add(&curl, &t);
            }
            let pre: Vec<UpEv<R::Elem>> =
                word[..k0 - 1].iter().map(|&k| UpEv::Cross(k - 1)).collect();
            let post: Vec<UpEv<R::Elem>> = word[k0..].iter().map(|&k| UpEv::Cross(k - 1)).collect();
            for (e, fe) in &parts {
                let mut pe = self.ring.mul(&self.xpow(0, *e), &curl);
                for j in 0..*e {
                    let b = self.bubble(c1, false, j + 3 + h, &strands, true)?;
                    pe = self
                        .ring
                        .add(&pe, &self.ring.mul(&b, &self.xpow(0, e - 1 - j)));
                }
                let mut evs = vec![UpEv::Poly(fe.clone())];
                evs.extend(pre.iter().cloned());
                evs.push(UpEv::Poly(pe));
                evs.extend(post.iter().cloned());
                let r = self.run(self.ident(rest), &mut rest.to_vec(), &evs)?;
                alg.val_add(&mut out, &r);
            }
        }
        Ok(out)
    }

    /// `v` (on `from` strands) placed among identity strands: `left` colours on its left and
    /// `right` colours on its right.
    pub fn embed(
        &self,
        v: &Value<R::Elem>,
        from: usize,
        left: &[u16],
        right: &[u16],
    ) -> Value<R::Elem> {
        let s = left.len();
        let n = s + from + right.len();
        let mut out = Value::new();
        for ((obj, w), f) in v {
            let mut o: Object = ups(left);
            o.extend_from_slice(obj);
            o.extend(ups(right));
            let mut p: Vec<u8> = (0..s as u8).collect();
            p.extend(w.0.iter().map(|&b| b + s as u8));
            p.extend((s + from) as u8..n as u8);
            let mut den: Den = smallvec::smallvec![0; n * n];
            for a in 0..from {
                for b in a + 1..from {
                    den[(a + s) * n + b + s] = f.den[a * from + b];
                }
            }
            out.insert(
                (o, Perm(p)),
                Frac {
                    num: self.shift_dots(&f.num, from, s as isize),
                    den,
                },
            );
        }
        out
    }
}
