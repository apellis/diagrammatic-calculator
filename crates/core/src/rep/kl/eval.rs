//! The evaluation `Φ` of a 2-morphism `1_μ → w` into upward diagrams (KL III Lemma 3.9).
//!
//! Sort `w` by the decomposition `1_w = Σ p_k ≫ q_k` onto words `E_a F_b` ([`decompose`]). Then
//! `f ↦ (f ≫ p_k)_k` is injective, and each `f ≫ p_k` is the right unbending of an upward
//! diagram `E_{rev b} → E_a` with bubbles, an element of `R(ν) ⊗ Π` ([`super::up`]). These
//! coordinates are carried through the diagram one run of layers at a time: a run of type RL
//! takes coordinates sorted to the right to coordinates sorted to the left (`F_d E_c`, unbent on
//! the left) by straightening `q_k ≫ run ≫ p'_r` and closing the bent strands with a right
//! partial trace; a run of type LR goes back with a left partial trace (categorification-lean
//! `KL3.Diagram.goodR_comp`, `goodL_comp`, `inv_step`).

use super::mono::{
    Comp, Ev, apply, collapse_bubbles, decompose, first_inversion, region, straighten,
};
use super::up::{P, Up, adapted_r};
use crate::error::Result;
use crate::poly::Poly;
use crate::rep::smash::{Frac, Value};
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
pub(super) type Evaluated<E> = (Vec<Letter>, Vec<Value<E>>);
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

    fn dec(&self, w: &[Letter], r_sorted: bool) -> Rc<Vec<Comp>> {
        let key = (w.to_vec(), r_sorted);
        if let Some(d) = self.decs.borrow().get(&key) {
            return d.clone();
        }
        let d = Rc::new(decompose(&self.up.a, &self.mu, w, r_sorted));
        self.decs.borrow_mut().insert(key, d.clone());
        d
    }

    /// The number of strands of each coordinate on `w`, sorted to the right or to the left.
    pub fn sizes(&self, w: &[Letter], right: bool) -> Vec<usize> {
        self.dec(w, right)
            .iter()
            .map(|c| c.sorted.iter().filter(|l| !l.up).count())
            .collect()
    }

    /// The sorting the coordinates on `w` are taken in: the one `w` already has, so that its
    /// decomposition is trivial, and to the right otherwise.
    pub fn prefers_right(w: &[Letter]) -> bool {
        first_inversion(w, true).is_none() || first_inversion(w, false).is_some()
    }

    /// The coordinates of a diagram from the empty object, with its target, sorted as
    /// [`Phi::prefers_right`] says.
    pub fn eval(&self, evs: &[Ev]) -> Result<Evaluated<R::Elem>> {
        let evs = &collapse_bubbles(&self.up.a, &self.mu, evs)[..];
        let mut w: Vec<Letter> = vec![];
        let mut vals = vec![self.up.ident(&[])];
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
                let b = self.up.bubble(*colour, *cw, *index, &[], true)?;
                vals = vec![self.up.mul_poly(0, &vals[0], &b)];
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
        xs: &[Value<R::Elem>],
        w: &[Letter],
        run: &[Ev],
        w2: &[Letter],
    ) -> Result<Vec<Value<R::Elem>>> {
        let cr = self.dec(w, true);
        let dl = self.dec(w2, false);
        let mut ys = vec![Value::new(); dl.len()];
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
                self.up.add_into(nd, y, &self.up.scale(&v, dr.coef));
            }
        }
        Ok(ys)
    }

    /// Coordinates sorted to the left on `w`, through an LR run to `w2`, sorted to the right.
    fn to_r(
        &self,
        ys: &[Value<R::Elem>],
        w: &[Letter],
        run: &[Ev],
        w2: &[Letter],
    ) -> Result<Vec<Value<R::Elem>>> {
        let cl = self.dec(w, false);
        let dr = self.dec(w2, true);
        let mut xs = vec![Value::new(); dr.len()];
        for (cr, y) in cl.iter().zip(ys) {
            if y.is_empty() {
                continue;
            }
            for (dk, x) in dr.iter().zip(xs.iter_mut()) {
                let mut m = cr.q.clone();
                m.extend(run.iter().cloned());
                m.extend(dk.p.iter().cloned());
                let v = self.t_lr(y, &cr.sorted, &m, &dk.sorted)?;
                let nb = dk.sorted.iter().filter(|l| !l.up).count();
                self.up.add_into(nb, x, &self.up.scale(&v, dk.coef));
            }
        }
        Ok(xs)
    }

    /// `unbendR(x) ≫ m = unbendL(y)` for `m : E_a F_b → F_d E_c` of type RL.
    fn t_rl(
        &self,
        x: &Value<R::Elem>,
        src: &[Letter],
        m: &[Ev],
        tgt: &[Letter],
    ) -> Result<Value<R::Elem>> {
        let up = self.up;
        let st = straighten::<R::Elem>(src, m, true)?;
        let na = src.iter().take_while(|l| l.up).count();
        let nb = src.len() - na;
        let nd = tgt.iter().take_while(|l| !l.up).count();
        let rev_b = rev_colours(&src[na..]);
        let rev_d = rev_colours(&tgt[..nd]);
        let mut top: Vec<u16> = rev_d.clone();
        top.extend(colours(&src[..na]));
        debug_assert_eq!(top, st.bottom);
        let mut v = up.embed(x, nb, &rev_d, &[]);
        v = up.run(v, &mut top, &st.evs)?;
        debug_assert!(v.is_empty() || top == st.top);
        let mut cols = rev_d;
        cols.extend(rev_b);
        let mut right = region(&up.a, &self.mu, src, na);
        for _ in 0..nb {
            let (v2, r2) = up.ptr_r(&v, &cols, &right)?;
            v = v2;
            right = r2;
            cols.pop();
        }
        debug_assert_eq!(right, self.mu);
        Ok(v)
    }

    /// `unbendL(y) ≫ m = unbendR(x)` for `m : F_d E_c → E_a F_b` of type LR.
    fn t_lr(
        &self,
        y: &Value<R::Elem>,
        src: &[Letter],
        m: &[Ev],
        tgt: &[Letter],
    ) -> Result<Value<R::Elem>> {
        let up = self.up;
        let st = straighten::<R::Elem>(src, m, false)?;
        let nd = src.iter().take_while(|l| !l.up).count();
        let na = tgt.iter().take_while(|l| l.up).count();
        let rev_d = rev_colours(&src[..nd]);
        let rev_b = rev_colours(&tgt[na..]);
        let lam_b = region(&up.a, &self.mu, tgt, na);
        let n = nd + rev_b.len();
        let mut v = up.embed(y, nd, &[], &rev_b);
        if !rev_b.is_empty() {
            // the bubbles of y move right across the strands E_{rev b}
            let strands: Vec<(u16, P<R>)> = rev_b
                .iter()
                .enumerate()
                .map(|(j, &c)| (c, up.x(nd + j)))
                .collect();
            let images = up.slide_images(&strands, true)?;
            let alg = up.alg(n);
            let mut out = Value::new();
            for (k, f) in v {
                let num = up.subst_z(&f.num, &images);
                if !up.ring.is_zero(&num) {
                    out.insert(k, alg.canon(Frac { num, den: f.den }));
                }
            }
            v = out;
        }
        let mut top: Vec<u16> = colours(&src[nd..]);
        top.extend(rev_b.iter().copied());
        debug_assert_eq!(top, st.bottom);
        v = up.run(v, &mut top, &st.evs)?;
        debug_assert!(v.is_empty() || top == st.top);
        let mut cols = rev_d;
        cols.extend(rev_b);
        for _ in 0..nd {
            v = up.ptr_l(&v, &cols, &lam_b)?;
            cols.remove(0);
        }
        Ok(v)
    }

    /// Linear coordinates of the values of [`Phi::eval`] on `w`: each component in the basis
    /// `ψ_ŵ x^a z^β`, with coefficients polynomials in the scalars.
    pub fn coords(
        &self,
        w: &[Letter],
        xs: &[Value<R::Elem>],
    ) -> Result<BTreeMap<Key, Poly<R::Elem>>> {
        let up = self.up;
        let coeff = &up.ctx.coeff;
        let mut out: BTreeMap<Key, Poly<R::Elem>> = BTreeMap::new();
        let right = Self::prefers_right(w);
        for (k, (c, x)) in self.dec(w, right).iter().zip(xs).enumerate() {
            // the bottom of the unbent coordinate: E_{rev b} of E_a F_b, or E_{rev d} of F_d E_c
            let cols = if right {
                let na = c.sorted.iter().take_while(|l| l.up).count();
                rev_colours(&c.sorted[na..])
            } else {
                let nd = c.sorted.iter().take_while(|l| !l.up).count();
                rev_colours(&c.sorted[..nd])
            };
            let n = cols.len();
            for (perm, _, f) in up.peel(x, &cols, adapted_r)? {
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
    acc: &mut Vec<Value<R::Elem>>,
    vals: &[Value<R::Elem>],
    sizes: &[usize],
    c: &P<R>,
) {
    if acc.is_empty() {
        acc.resize(vals.len(), Value::new());
    }
    for ((a, v), &n) in acc.iter_mut().zip(vals).zip(sizes) {
        let scaled = up.mul_poly(n, v, c);
        up.add_into(n, a, &scaled);
    }
}
