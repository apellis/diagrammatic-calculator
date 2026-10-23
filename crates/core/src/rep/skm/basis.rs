//! The candidate basis of `HOM_U(s, t)` in the region `μ` for Brundan–Ellis's super Kac–Moody
//! 2-category: reduced lifts of the matchings of the boundary with dots and bubbles, as in the
//! even case (`crate::rep::kl::basis`). That these span and are independent is Brundan–Ellis's
//! Nondegeneracy Conjecture (arXiv:1701.04133v2, Section 8); the solve reports any failure.
//!
//! A matching pairs the boundary points of `s → t` (the letters of `*s t`, an `E_i` with an
//! `F_i`). Its diagram has no closed loops, no arc crossing itself and two arcs crossing at most
//! once ([`basis_between`]); crossings of strands with opposite orientations are sideways
//! crossings. Each arc carries dots at one end, and a monomial in the bubbles of positive degree
//! index sits in the rightmost region (clockwise ones when `⟨i, μ⟩ ≥ 0`, counterclockwise ones
//! otherwise, so that every bubble is a real one). For an odd colour these are the even-index
//! bubbles and at most one odd bubble `δ` (`δ² = 0`, BE2 (2.18)). Bending (`f ↦ (1 ⊗ f) ∘ coev`) takes
//! `HOM(s, t)` isomorphically to `HOM(1, *s t)`, where the evaluation of [`super::eval`] lives.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::ring::Ring;
use crate::term::{Layer, Letter, Word};

/// The generators of the BE2 presentation by role.
#[derive(Clone, Copy, Debug)]
pub(super) struct Roles {
    pub x: u16,
    pub xd: u16,
    pub psi: u16,
    pub psid: u16,
    /// `1 → E F` (`η'`).
    pub cup_ef: u16,
    /// `1 → F E` (`η`).
    pub cup_fe: u16,
    /// `E F → 1` (`ε`).
    pub cap_ef: u16,
    /// `F E → 1` (`ε'`).
    pub cap_fe: u16,
    /// The sideways crossing `F E → E F`.
    pub lcross: u16,
}

fn layer(g: u16, pos: usize, binding: Vec<u16>) -> Layer {
    Layer {
        generator: g,
        pos: pos as u16,
        binding,
    }
}

pub(super) fn flip(l: Letter) -> Letter {
    Letter {
        colour: l.colour,
        up: !l.up,
    }
}

impl Roles {
    pub fn dot(&self, l: Letter, pos: usize) -> Layer {
        layer(if l.up { self.x } else { self.xd }, pos, vec![l.colour])
    }

    /// A cup creating `l` at `pos` and its dual right of it.
    pub fn cup(&self, l: Letter, pos: usize) -> Layer {
        layer(
            if l.up { self.cup_ef } else { self.cup_fe },
            pos,
            vec![l.colour],
        )
    }

    /// A cap on `l` at `pos` and its dual right of it.
    pub fn cap(&self, l: Letter, pos: usize) -> Layer {
        layer(
            if l.up { self.cap_ef } else { self.cap_fe },
            pos,
            vec![l.colour],
        )
    }

    /// A crossing of the letters `l`, `r` at `q`, `q + 1` (sideways when their orientations
    /// differ).
    pub fn cross(&self, l: Letter, r: Letter, q: usize) -> Vec<Layer> {
        match (l.up, r.up) {
            (true, true) => vec![layer(self.psi, q, vec![l.colour, r.colour])],
            (false, false) => vec![layer(self.psid, q, vec![l.colour, r.colour])],
            // F_a E_b → E_b F_a
            (false, true) => vec![
                layer(self.cup_ef, q + 2, vec![l.colour]),
                layer(self.psi, q + 1, vec![r.colour, l.colour]),
                layer(self.cap_fe, q, vec![l.colour]),
            ],
            // E_a F_b → F_b E_a
            (true, false) => vec![
                layer(self.cup_fe, q, vec![r.colour]),
                layer(self.psi, q + 1, vec![r.colour, l.colour]),
                layer(self.cap_ef, q + 2, vec![r.colour]),
            ],
        }
    }
}

/// Every perfect matching of `w` pairing an upward letter with a downward one of its colour, as
/// the partner of each position.
pub(super) fn matchings(w: &[Letter]) -> Vec<Vec<usize>> {
    fn go(w: &[Letter], partner: &mut Vec<Option<usize>>, out: &mut Vec<Vec<usize>>) {
        let Some(p) = partner.iter().position(|x| x.is_none()) else {
            out.push(partner.iter().map(|x| x.unwrap()).collect());
            return;
        };
        for q in p + 1..w.len() {
            if partner[q].is_none() && w[q].colour == w[p].colour && w[q].up != w[p].up {
                partner[p] = Some(q);
                partner[q] = Some(p);
                go(w, partner, out);
                partner[p] = None;
                partner[q] = None;
            }
        }
    }
    let mut out = Vec::new();
    if w.len().is_multiple_of(2) {
        go(w, &mut vec![None; w.len()], &mut out);
    }
    out
}

/// A bubble of colour `c` and degree index `k ≥ 1` in the region `μ` of the empty object: real,
/// clockwise (`[η', x^m on E, ε]`) when `⟨c, μ⟩ ≥ 0` and counterclockwise (`[η, x^m on E, ε']`)
/// otherwise.
pub(super) fn bubble(roles: &Roles, mu: &[i64], c: u16, k: i64) -> Vec<Layer> {
    let h = mu[c as usize];
    let up = Letter {
        colour: c,
        up: true,
    };
    let (l, m, e) = if h >= 0 {
        (up, k - 1 + h, 0)
    } else {
        (flip(up), k - 1 - h, 1)
    };
    let mut out = vec![roles.cup(l, 0)];
    out.extend((0..m).map(|_| roles.dot(up, e)));
    out.push(roles.cap(l, 0));
    out
}

/// Every exponent vector `e` with `Σ e_i deg_i = total` and `e_i ≤ max_i` (all degrees positive).
fn exponents(deg: &[i64], max: &[i64], total: i64) -> Vec<Vec<i64>> {
    fn go(
        deg: &[i64],
        max: &[i64],
        k: usize,
        left: i64,
        cur: &mut Vec<i64>,
        out: &mut Vec<Vec<i64>>,
    ) {
        if k == deg.len() {
            if left == 0 {
                out.push(cur.clone());
            }
            return;
        }
        let mut e = 0;
        while e * deg[k] <= left && e <= max[k] {
            cur.push(e);
            go(deg, max, k + 1, left - e * deg[k], cur, out);
            cur.pop();
            e += 1;
        }
    }
    let mut out = Vec::new();
    if total >= 0 {
        go(deg, max, 0, total, &mut Vec::new(), &mut out);
    }
    out
}

/// `f : s → t` bent to `1 → *s t` with nested cups on the left (`*s` reversed and dual).
pub(super) fn bend(roles: &Roles, w: &Word) -> Word {
    let s = &w.source;
    let n = s.len();
    let mut layers = Vec::new();
    for j in (0..n).rev() {
        layers.push(roles.cup(flip(s[j]), n - 1 - j));
    }
    layers.extend(w.layers.iter().map(|l| Layer {
        pos: l.pos + n as u16,
        ..l.clone()
    }));
    Word {
        source: vec![],
        layers,
    }
}

/// What a strand of a diagram `s → t` belongs to while it is drawn: the arc of a matching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arc {
    /// An arc with both ends on one boundary (a cap at the bottom, a cup at the top).
    Turn(usize),
    /// A strand from the bottom to the top.
    Through(usize),
}

enum Step {
    /// Close the arcs at `r`, `r + 1`.
    Close(usize),
    Cross(usize),
}

/// Closes every turning arc of a boundary (read as the bottom of a diagram), crossing only
/// where two arcs must cross: repeatedly cap an adjacent pair, or else move the left end of the
/// turning arc of least span one step right. Through strands keep their order. Returns the
/// steps and the objects before each step.
fn close_turns(start: &[(Letter, Arc)]) -> (Vec<Step>, Vec<Vec<(Letter, Arc)>>) {
    let mut cur = start.to_vec();
    let mut steps = Vec::new();
    let mut objs = Vec::new();
    loop {
        if let Some(r) = (0..cur.len().saturating_sub(1))
            .find(|&r| matches!(cur[r].1, Arc::Turn(a) if cur[r + 1].1 == Arc::Turn(a)))
        {
            objs.push(cur.clone());
            steps.push(Step::Close(r));
            cur.drain(r..r + 2);
            continue;
        }
        let ends = |a: usize| {
            let v: Vec<usize> = (0..cur.len())
                .filter(|&p| cur[p].1 == Arc::Turn(a))
                .collect();
            (v[0], v[1])
        };
        let Some((l, _)) = cur
            .iter()
            .filter_map(|x| match x.1 {
                Arc::Turn(a) => Some(ends(a)),
                Arc::Through(_) => None,
            })
            .min_by_key(|&(l, r)| (r - l, l))
        else {
            return (steps, objs);
        };
        objs.push(cur.clone());
        steps.push(Step::Cross(l));
        cur.swap(l, l + 1);
    }
}

/// The basis words of `HOM(s, t)` of the given degree, drawn directly: for each matching of the
/// boundary (the matchings of `*s t`), caps closed at the bottom, the through strands permuted by
/// a reduced word, cups opened at the top, each arc dotted at its left end (bottom for caps and
/// through strands, top for cups), and the bubbles of `μ` at the bottom right. These are reduced
/// lifts of the chord diagrams of [`basis`] (equal to them up to lower terms by isotopy and the
/// braid relations), so they form a basis too.
pub(super) fn basis_between<R: Ring>(
    ctx: &Context<R>,
    roles: &Roles,
    s: &[Letter],
    t: &[Letter],
    degree: i64,
) -> Result<Vec<Word>> {
    let cartan = ctx.cartan.as_ref().expect("checked by the strategy");
    let mu = ctx
        .mu
        .as_ref()
        .ok_or_else(|| Error::precondition("the BE2 normal form needs the region weight mu="))?;
    let n = s.len();
    let nc = ctx.colours.len();
    let mut bent: Vec<Letter> = s.iter().rev().map(|&l| flip(l)).collect();
    bent.extend_from_slice(t);
    let mut out = Vec::new();
    for partner in matchings(&bent) {
        // label the boundary points by arc
        let bottom: Vec<(Letter, Arc)> = (0..n)
            .map(|a| {
                let j = n - 1 - a;
                let q = partner[j];
                let arc = if q < n {
                    Arc::Turn(j.min(q))
                } else {
                    Arc::Through(j.min(q))
                };
                (s[a], arc)
            })
            .collect();
        let top: Vec<(Letter, Arc)> = (0..t.len())
            .map(|b| {
                let j = n + b;
                let q = partner[j];
                let arc = if q >= n {
                    Arc::Turn(j.min(q))
                } else {
                    Arc::Through(j.min(q))
                };
                (t[b], arc)
            })
            .collect();
        let mut layers = Vec::new();
        // caps at the bottom
        let (steps, objs) = close_turns(&bottom);
        for (st, o) in steps.iter().zip(&objs) {
            match *st {
                Step::Close(r) => layers.push(roles.cap(o[r].0, r)),
                Step::Cross(k) => layers.extend(roles.cross(o[k].0, o[k + 1].0, k)),
            }
        }
        let mut mid: Vec<(Letter, Arc)> = bottom.clone();
        if let Some(last) = objs.last() {
            mid = last.clone();
            match steps.last() {
                Some(Step::Close(r)) => {
                    mid.drain(*r..*r + 2);
                }
                Some(Step::Cross(k)) => mid.swap(*k, k + 1),
                None => {}
            }
        }
        // the through strands into their top order
        let (tsteps, tobjs) = close_turns(&top);
        let mut upper: Vec<(Letter, Arc)> = top.clone();
        if let Some(last) = tobjs.last() {
            upper = last.clone();
            match tsteps.last() {
                Some(Step::Close(r)) => {
                    upper.drain(*r..*r + 2);
                }
                Some(Step::Cross(k)) => upper.swap(*k, k + 1),
                None => {}
            }
        }
        let rank = |a: &Arc| {
            upper
                .iter()
                .position(|x| x.1 == *a)
                .expect("a through strand")
        };
        while let Some(k) =
            (0..mid.len().saturating_sub(1)).find(|&k| rank(&mid[k].1) > rank(&mid[k + 1].1))
        {
            layers.extend(roles.cross(mid[k].0, mid[k + 1].0, k));
            mid.swap(k, k + 1);
        }
        // cups at the top: the closing of the top boundary, reversed
        for (st, o) in tsteps.iter().zip(&tobjs).rev() {
            match *st {
                Step::Close(r) => layers.push(roles.cup(o[r].0, r)),
                Step::Cross(k) => {
                    // below this step the strands at k, k + 1 are swapped
                    layers.extend(roles.cross(o[k + 1].0, o[k].0, k));
                }
            }
        }
        let base = Word {
            source: s.to_vec(),
            layers,
        };
        let d0 = ctx.word_degree(&base);
        let rest = degree - d0;
        if rest < 0 {
            continue;
        }
        // dots: one exponent per arc, at its left end
        let mut ends: Vec<(bool, usize, Letter)> = Vec::new(); // (on top, position, letter)
        for (a, &(l, arc)) in bottom.iter().enumerate() {
            let first = bottom.iter().position(|x| x.1 == arc) == Some(a);
            if first {
                ends.push((false, a, l));
            }
        }
        for (b, &(l, arc)) in top.iter().enumerate() {
            if matches!(arc, Arc::Turn(_)) && top.iter().position(|x| x.1 == arc) == Some(b) {
                ends.push((true, b, l));
            }
        }
        let mut deg: Vec<i64> = ends
            .iter()
            .map(|e| cartan.form(e.2.colour as usize, e.2.colour as usize))
            .collect();
        let mut max: Vec<i64> = vec![i64::MAX; deg.len()];
        let mut bubs = Vec::new();
        for c in 0..nc {
            let d = 2 * cartan.d[c];
            let odd = ctx.colours[c].odd;
            for k in 1..=rest / d {
                if odd && k % 2 == 1 && k > 1 {
                    continue;
                }
                bubs.push((c as u16, k));
                deg.push(d * k);
                max.push(if odd && k == 1 { 1 } else { i64::MAX });
            }
        }
        for e in exponents(&deg, &max, rest) {
            let mut layers = Vec::new();
            for (b, &(c, k)) in bubs.iter().enumerate() {
                for _ in 0..e[ends.len() + b] {
                    layers.extend(bubble(roles, mu, c, k).into_iter().map(|l| Layer {
                        pos: l.pos + n as u16,
                        ..l
                    }));
                }
            }
            for (a, &(on_top, p, l)) in ends.iter().enumerate() {
                if !on_top {
                    for _ in 0..e[a] {
                        layers.push(roles.dot(l, p));
                    }
                }
            }
            layers.extend(base.layers.iter().cloned());
            for (a, &(on_top, p, l)) in ends.iter().enumerate() {
                if on_top {
                    for _ in 0..e[a] {
                        layers.push(roles.dot(l, p));
                    }
                }
            }
            let word = ctx
                .canonical(&Word {
                    source: s.to_vec(),
                    layers,
                })
                .0;
            if ctx.word_degree(&word) != degree || ctx.target(&word) != t {
                return Err(Error::invalid(
                    "skm: a basis word has the wrong degree or target",
                ));
            }
            out.push(word);
        }
    }
    Ok(out)
}
