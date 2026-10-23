//! The evaluation `Φ` of a 2-morphism `1_μ → w` into upward diagrams with bubbles, as for the
//! Khovanov–Lauda 2-category (`crate::rep::kl::eval`), with the signs of odd colours.
//!
//! Coordinates sorted to the right on `w` (onto `E_a F_b`) are upward diagrams `x` with
//! `f ≫ p_k = unbendR(x) = (x ⊗ 1)(η' cups)`; sorted to the left (onto `F_d E_c`), `y` with
//! `f ≫ p_k = unbendL(y) = (1 ⊗ y)(η cups)`. A run of type RL takes `x` to
//! `y = s · ptr_r^{nb}(U ∘ (1 ⊗ x))` for `m = s · Bend_R(U)`; a run of type LR takes `y` to
//! `x = s · (−1)^{|η'_B| |y|} · ptr_l^{nd}(U ∘ (y ⊗ 1))` for `m = s · Bend_L(U)`, the second sign
//! from moving the cups of `Bend_L` below `y`.

use super::mono::{
    Comp, Ev, Sig, apply, collapse_bubbles, decompose, first_inversion, region, straighten,
};
use super::up::{P, Up, Val};
use crate::error::Result;
use crate::poly::Poly;
use crate::ring::Ring;
use crate::term::Letter;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

/// A coordinate: the component of the sorting decomposition, the permutation and the dots of
/// `ψ_ŵ x^a`, and the bubble exponents.
pub(super) type Key = (usize, Vec<u8>, Vec<i32>, Vec<i32>);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Any,
    Rl,
    Lr,
}

fn kind(ev: &Ev, obj: &[Letter]) -> Kind {
    match ev {
        Ev::Dot(_) | Ev::Cross(_) | Ev::Bub { .. } => Kind::Any,
        Ev::Cup { up_left, .. } => {
            if *up_left {
                Kind::Lr
            } else {
                Kind::Rl
            }
        }
        Ev::Cap { pos, .. } => {
            if obj[*pos].up {
                Kind::Rl
            } else {
                Kind::Lr
            }
        }
    }
}

fn colours(w: &[Letter]) -> Vec<u16> {
    w.iter().map(|l| l.colour).collect()
}

fn rev_colours(w: &[Letter]) -> Vec<u16> {
    w.iter().rev().map(|l| l.colour).collect()
}

/// A target object with the coordinates of a diagram into it.
pub(super) type Evaluated<E> = (Vec<Letter>, Vec<Val<E>>);
type Decompositions = HashMap<(Vec<Letter>, bool), Rc<Vec<Comp>>>;

pub(super) struct Phi<'u, 'a, R: Ring> {
    pub up: &'u Up<'a, R>,
    pub mu: Vec<i64>,
    decs: RefCell<Decompositions>,
}

impl<'u, 'a, R: Ring> Phi<'u, 'a, R> {
    pub fn new(up: &'u Up<'a, R>, mu: Vec<i64>) -> Self {
        Phi {
            up,
            mu,
            decs: RefCell::new(HashMap::new()),
        }
    }

    pub fn sig(&self) -> Sig<'_> {
        Sig {
            a: &self.up.a,
            odd: &self.up.odd,
            pi: &self.up.pi,
            mu: &self.mu,
        }
    }

    fn dec(&self, w: &[Letter], r_sorted: bool) -> Rc<Vec<Comp>> {
        let key = (w.to_vec(), r_sorted);
        if let Some(d) = self.decs.borrow().get(&key) {
            return d.clone();
        }
        let d = Rc::new(decompose(&self.sig(), w, r_sorted));
        self.decs.borrow_mut().insert(key, d.clone());
        d
    }

    /// The bottom colours of each coordinate on `w`, sorted to the right (`E_{rev b}`) or to the
    /// left (`E_{rev d}`).
    pub fn bottoms(&self, w: &[Letter], right: bool) -> Vec<Vec<u16>> {
        self.dec(w, right)
            .iter()
            .map(|c| {
                if right {
                    let na = c.sorted.iter().take_while(|l| l.up).count();
                    rev_colours(&c.sorted[na..])
                } else {
                    let nd = c.sorted.iter().take_while(|l| !l.up).count();
                    rev_colours(&c.sorted[..nd])
                }
            })
            .collect()
    }

    /// The sorting the coordinates on `w` are taken in: the one `w` already has, so that its
    /// decomposition is trivial, and to the right otherwise.
    pub fn prefers_right(w: &[Letter]) -> bool {
        first_inversion(w, true).is_none() || first_inversion(w, false).is_some()
    }

    /// The coordinates of `coef · diagram` from the empty object, with its target, sorted as
    /// [`Phi::prefers_right`] says.
    pub fn eval(&self, coef: i64, evs: &[Ev]) -> Result<Evaluated<R::Elem>> {
        let up = self.up;
        let (evs, neg) = collapse_bubbles(&self.sig(), evs);
        let evs = &evs[..];
        let mut w: Vec<Letter> = vec![];
        let mut vals = vec![Val::new()];
        up.add_scaled(
            &[],
            &mut vals[0],
            &up.ident(0),
            if neg { -coef } else { coef },
        );
        let mut right_sorted = true;
        let mut i = 0;
        while i < evs.len() {
            if let (
                Ev::Bub {
                    colour, cw, index, ..
                },
                true,
            ) = (&evs[i], w.is_empty())
            {
                // a bubble with nothing beside it lies in the reference region
                let b = up.bubble(*colour, *cw, *index, &[], true)?;
                vals = vec![up.mul_top(&[], &vals[0], &b)];
                i += 1;
                continue;
            }
            let mut k = Kind::Any;
            let mut run = Vec::new();
            let mut w2 = w.clone();
            while i < evs.len() {
                if matches!(evs[i], Ev::Bub { .. }) && w2.is_empty() {
                    break;
                }
                let ki = kind(&evs[i], &w2);
                if ki != Kind::Any && k != Kind::Any && ki != k {
                    break;
                }
                if ki != Kind::Any {
                    k = ki;
                }
                apply(&mut w2, &evs[i]);
                run.push(evs[i].clone());
                i += 1;
            }
            let rl = match k {
                Kind::Rl => true,
                Kind::Lr => false,
                Kind::Any => right_sorted,
            };
            if rl {
                if !right_sorted {
                    vals = self.to_r(&vals, &w, &[], &w)?;
                }
                vals = self.to_l(&vals, &w, &run, &w2)?;
                right_sorted = false;
            } else {
                if right_sorted {
                    vals = self.to_l(&vals, &w, &[], &w)?;
                }
                vals = self.to_r(&vals, &w, &run, &w2)?;
                right_sorted = true;
            }
            w = w2;
        }
        let want = Self::prefers_right(&w);
        if want && !right_sorted {
            vals = self.to_r(&vals, &w, &[], &w)?;
        } else if !want && right_sorted {
            vals = self.to_l(&vals, &w, &[], &w)?;
        }
        Ok((w, vals))
    }

    /// Coordinates sorted to the right on `w`, through an RL run to `w2`, sorted to the left.
    fn to_l(
        &self,
        xs: &[Val<R::Elem>],
        w: &[Letter],
        run: &[Ev],
        w2: &[Letter],
    ) -> Result<Vec<Val<R::Elem>>> {
        let cr = self.dec(w, true);
        let dl = self.dec(w2, false);
        let mut ys = vec![Val::new(); dl.len()];
        for (ck, x) in cr.iter().zip(xs) {
            if x.is_empty() {
                continue;
            }
            for (dr, y) in dl.iter().zip(ys.iter_mut()) {
                let mut m = ck.q.clone();
                m.extend(run.iter().cloned());
                m.extend(dr.p.iter().cloned());
                let v = self.t_rl(x, &ck.sorted, &m, &dr.sorted)?;
                let nd = dr.sorted.iter().take_while(|l| !l.up).count();
                let cols = rev_colours(&dr.sorted[..nd]);
                self.up.add_scaled(&cols, y, &v, dr.coef);
            }
        }
        Ok(ys)
    }

    /// Coordinates sorted to the left on `w`, through an LR run to `w2`, sorted to the right.
    fn to_r(
        &self,
        ys: &[Val<R::Elem>],
        w: &[Letter],
        run: &[Ev],
        w2: &[Letter],
    ) -> Result<Vec<Val<R::Elem>>> {
        let cl = self.dec(w, false);
        let dr = self.dec(w2, true);
        let mut xs = vec![Val::new(); dr.len()];
        for (cr, y) in cl.iter().zip(ys) {
            if y.is_empty() {
                continue;
            }
            for (dk, x) in dr.iter().zip(xs.iter_mut()) {
                let mut m = cr.q.clone();
                m.extend(run.iter().cloned());
                m.extend(dk.p.iter().cloned());
                let v = self.t_lr(y, &cr.sorted, &m, &dk.sorted)?;
                let na = dk.sorted.iter().take_while(|l| l.up).count();
                let cols = rev_colours(&dk.sorted[na..]);
                self.up.add_scaled(&cols, x, &v, dk.coef);
            }
        }
        Ok(xs)
    }

    /// `unbendR(x) ≫ m = unbendL(y)` for `m : E_a F_b → F_d E_c` of type RL.
    fn t_rl(
        &self,
        x: &Val<R::Elem>,
        src: &[Letter],
        m: &[Ev],
        tgt: &[Letter],
    ) -> Result<Val<R::Elem>> {
        let up = self.up;
        let na = src.iter().take_while(|l| l.up).count();
        let nb = src.len() - na;
        let nd = tgt.iter().take_while(|l| !l.up).count();
        let rev_b = rev_colours(&src[na..]);
        let rev_d = rev_colours(&tgt[..nd]);
        let mut cols = rev_d.clone();
        cols.extend(rev_b.iter().copied());
        let embedded = up.embed(x, nb, &rev_d, &[]);
        let mut acc = Val::new();
        for st in straighten::<R::Elem>(&self.sig(), src, m, true)? {
            let mut top: Vec<u16> = rev_d.clone();
            top.extend(colours(&src[..na]));
            debug_assert_eq!(top, st.bottom);
            let v = up.run(&cols, embedded.clone(), &mut top, &st.evs)?;
            up.add_scaled(&cols, &mut acc, &v, st.coef);
        }
        let mut v = acc;
        let mut cs = cols;
        let mut right = region(&up.a, &self.mu, src, na);
        for _ in 0..nb {
            let (v2, r2) = up.ptr_r(&v, &cs, &right)?;
            v = v2;
            right = r2;
            cs.pop();
        }
        debug_assert_eq!(right, self.mu);
        Ok(v)
    }

    /// `unbendL(y) ≫ m = unbendR(x)` for `m : F_d E_c → E_a F_b` of type LR.
    fn t_lr(
        &self,
        y: &Val<R::Elem>,
        src: &[Letter],
        m: &[Ev],
        tgt: &[Letter],
    ) -> Result<Val<R::Elem>> {
        let up = self.up;
        let nd = src.iter().take_while(|l| !l.up).count();
        let na = tgt.iter().take_while(|l| l.up).count();
        let rev_d = rev_colours(&src[..nd]);
        let rev_b = rev_colours(&tgt[na..]);
        let lam_b = region(&up.a, &self.mu, tgt, na);
        let mut cols = rev_d.clone();
        cols.extend(rev_b.iter().copied());
        let mut v = up.embed(y, nd, &[], &rev_b);
        if !rev_b.is_empty() {
            // the bubbles of y move right across the strands E_{rev b}
            let strands: Vec<(u16, P<R>)> = rev_b
                .iter()
                .enumerate()
                .map(|(j, &c)| (c, up.x(nd + j)))
                .collect();
            let images = up.slide_images(&strands, true)?;
            let ring = up.ring(&cols);
            let mut out = Val::new();
            for (w, f) in v {
                let g = up.subst_z(&cols, &f, &images);
                if !ring.is_zero(&g) {
                    out.insert(w, g);
                }
            }
            v = out;
        }
        // the cups of Bend_L move below y
        let cups_odd = rev_b.iter().filter(|&&c| up.pi[c as usize]).count() % 2 == 1;
        if cups_odd {
            let ring = up.ring(&cols);
            let mut out = Val::new();
            for (w, f) in v {
                let mut terms = Vec::with_capacity(f.terms.len());
                let wo = up.perm_odd(&cols, &w);
                for (mo, c) in f.terms {
                    let odd = wo != up.mono_odd(&cols, &mo);
                    terms.push((mo, if odd { ring.base.neg(&c) } else { c }));
                }
                out.insert(w, ring.normalize(terms));
            }
            v = out;
        }
        let mut acc = Val::new();
        for st in straighten::<R::Elem>(&self.sig(), src, m, false)? {
            let mut top: Vec<u16> = colours(&src[nd..]);
            top.extend(rev_b.iter().copied());
            debug_assert_eq!(top, st.bottom);
            let r = up.run(&cols, v.clone(), &mut top, &st.evs)?;
            up.add_scaled(&cols, &mut acc, &r, st.coef);
        }
        let mut v = acc;
        let mut cs = cols;
        for _ in 0..nd {
            v = up.ptr_l(&v, &cs, &lam_b)?;
            cs.remove(0);
        }
        Ok(v)
    }

    /// Linear coordinates of the values of [`Phi::eval`] on `w`: each component in the basis
    /// `ψ_ŵ x^a z^β δ^γ`, with coefficients polynomials in the scalars.
    pub fn coords(
        &self,
        w: &[Letter],
        xs: &[Val<R::Elem>],
    ) -> Result<BTreeMap<Key, Poly<R::Elem>>> {
        let up = self.up;
        let coeff = &up.ctx.coeff;
        let mut out: BTreeMap<Key, Poly<R::Elem>> = BTreeMap::new();
        let right = Self::prefers_right(w);
        for (k, (cols, x)) in self.bottoms(w, right).iter().zip(xs).enumerate() {
            let n = cols.len();
            for (perm, _, f) in up.peel(cols, x, crate::rep::kl::up::adapted_r)? {
                for (m, e) in &f.terms {
                    let (xe, ze, sm) = up.split(m);
                    let key = (k, perm.0.clone(), xe[..n].to_vec(), ze);
                    let t = coeff.monomial(sm, e.clone());
                    let s = match out.get(&key) {
                        Some(o) => coeff.add(o, &t),
                        None => t,
                    };
                    if coeff.is_zero(&s) {
                        out.remove(&key);
                    } else {
                        out.insert(key, s);
                    }
                }
            }
        }
        Ok(out)
    }
}

/// The values of `Σ c · word`, added up, all on the same target.
pub(super) fn sum_into<R: Ring>(
    up: &Up<'_, R>,
    acc: &mut Vec<Val<R::Elem>>,
    vals: &[Val<R::Elem>],
    bottoms: &[Vec<u16>],
    c: &P<R>,
) {
    if acc.is_empty() {
        acc.resize(vals.len(), Val::new());
    }
    let c = up.lift(c);
    for ((a, v), cols) in acc.iter_mut().zip(vals).zip(bottoms) {
        let scaled = up.mul_bottom(cols, v, &c);
        up.add_into_pub(cols, a, &scaled);
    }
}
