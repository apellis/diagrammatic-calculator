//! Monotone diagrams with odd colours: the sorting decompositions and their straightening, with
//! the signs of the super interchange law.
//!
//! As in the even case (`crate::rep::kl::mono`), a diagram is *of type RL* when its cups are
//! `η : 1 → F E` and its caps `ε : E F → 1`, and *of type LR* when its cups are
//! `η' : 1 → E F` and its caps `ε' : F E → 1`. Bending gives an upward diagram `U` with
//! `m = ± Bend(U)`. The rightward cups and caps are even and agree with the downward dots and
//! crossings (`xd`, `psid` are their mates), so for type RL the sign is the Koszul sign of the
//! reordering of the odd generators by height. The leftward ones have the parity
//! `p(i)(⟨i, λ⟩ + 1)` and bend downward dots and crossings into their left mates, which differ
//! from `xd`, `psid` by BE2 (1.27) and (1.28) (odd-lean `eq_1_27`, `eq_1_28`). So for type LR
//! the downward dots and crossings are first rewritten through the leftward cups and caps, and
//! then the sign is the Koszul sign of the reordering with the cups and caps included, times
//! `(−1)^{p(i)(⟨i,λ⟩+1)}` for each zigzag pulled straight (the relation `zigzag-left-up`).

use crate::error::{Error, Result};
use crate::rep::skm::up::UpEv;
use crate::term::Letter;

/// The data that signs depend on.
pub(super) struct Sig<'a> {
    pub a: &'a [Vec<i64>],
    pub odd: &'a [bool],
    /// The parity of the leftward cups and caps of each colour.
    pub pi: &'a [bool],
    /// The rightmost region.
    pub mu: &'a [i64],
}

/// One step of a diagram, bottom first. Positions are 0-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Ev {
    Dot(usize),
    /// A crossing of two strands with the same orientation.
    Cross(usize),
    /// A cup creating the letters `(colour, up_left)` at `pos` and `(colour, !up_left)` at
    /// `pos + 1`.
    Cup {
        pos: usize,
        colour: u16,
        up_left: bool,
    },
    /// A cap on the letters at `pos` and `pos + 1`, with possibly a bubble (clockwise or not, by
    /// its degree index) of the same colour just above it.
    Cap {
        pos: usize,
        bub: Option<(bool, i64)>,
    },
    /// A closed bubble of a colour (clockwise or not, by its degree index) in the region left of
    /// `pos`.
    Bub {
        pos: usize,
        colour: u16,
        cw: bool,
        index: i64,
    },
}

/// The object after one step.
pub(super) fn apply(obj: &mut Vec<Letter>, ev: &Ev) {
    match ev {
        Ev::Dot(_) | Ev::Bub { .. } => {}
        Ev::Cross(k) => obj.swap(*k, k + 1),
        Ev::Cup {
            pos,
            colour,
            up_left,
        } => {
            obj.insert(
                *pos,
                Letter {
                    colour: *colour,
                    up: !up_left,
                },
            );
            obj.insert(
                *pos,
                Letter {
                    colour: *colour,
                    up: *up_left,
                },
            );
        }
        Ev::Cap { pos, .. } => {
            obj.drain(*pos..pos + 2);
        }
    }
}

/// The parity of a step on `obj` (a cap together with its bubble).
pub(super) fn parity(sig: &Sig, obj: &[Letter], ev: &Ev) -> bool {
    let odd = |c: u16| sig.odd[c as usize];
    match *ev {
        Ev::Dot(q) => odd(obj[q].colour),
        Ev::Cross(q) => odd(obj[q].colour) && odd(obj[q + 1].colour),
        Ev::Cup {
            colour, up_left, ..
        } => up_left && sig.pi[colour as usize],
        Ev::Cap { pos, bub } => {
            let c = obj[pos].colour;
            let turn = !obj[pos].up && sig.pi[c as usize];
            let b = bub.is_some_and(|(_, k)| odd(c) && k % 2 != 0);
            turn != b
        }
        Ev::Bub { colour, index, .. } => odd(colour) && index % 2 != 0,
    }
}

/// The Koszul sign of listing items in the given order, each with its original height and
/// parity: whether an odd number of pairs of odd items change their order.
pub(super) fn koszul<K: Ord>(items: &[(K, bool)]) -> bool {
    let mut neg = false;
    for s in 0..items.len() {
        if !items[s].1 {
            continue;
        }
        for t in s + 1..items.len() {
            if items[t].1 && items[s].0 > items[t].0 {
                neg = !neg;
            }
        }
    }
    neg
}

/// The position, number of inputs, and number of outputs of a step.
fn extent(ev: &Ev) -> (usize, usize, usize) {
    match *ev {
        Ev::Dot(q) => (q, 1, 1),
        Ev::Cross(q) => (q, 2, 2),
        Ev::Cup { pos, .. } => (pos, 0, 2),
        Ev::Cap { pos, .. } => (pos, 2, 0),
        Ev::Bub { pos, .. } => (pos, 0, 0),
    }
}

fn moved(ev: &Ev, by: isize) -> Ev {
    let sh = |q: usize| (q as isize + by) as usize;
    match ev.clone() {
        Ev::Dot(q) => Ev::Dot(sh(q)),
        Ev::Cross(q) => Ev::Cross(sh(q)),
        Ev::Cup {
            pos,
            colour,
            up_left,
        } => Ev::Cup {
            pos: sh(pos),
            colour,
            up_left,
        },
        Ev::Cap { pos, bub } => Ev::Cap { pos: sh(pos), bub },
        Ev::Bub {
            pos,
            colour,
            cw,
            index,
        } => Ev::Bub {
            pos: sh(pos),
            colour,
            cw,
            index,
        },
    }
}

/// Replaces each cup that a cap closes with only dots in between by one bubble step (its degree
/// index from the dots and the weight of its region), for a diagram from the empty object.
/// The bubble is `[η', dots on E, ε]` (clockwise) or `[η, dots on E, ε']`; dots on the
/// downward leg move round the rightward turn onto the upward one. Returns whether the
/// reordering of the odd steps changed the sign.
pub(super) fn collapse_bubbles(sig: &Sig, evs: &[Ev]) -> (Vec<Ev>, bool) {
    let mut evs = evs.to_vec();
    let mut neg = false;
    let mut obj: Vec<Letter> = Vec::new();
    let mut i = 0;
    'next: while i < evs.len() {
        if let Ev::Cup {
            pos,
            colour,
            up_left,
        } = evs[i]
        {
            let h = region(sig.a, sig.mu, &obj, pos)[colour as usize];
            let (mut b, mut m) = (pos, 0i64);
            let mut between = Vec::new();
            let mut between_par = Vec::new();
            let (mut e_leg, mut f_leg) = (Vec::new(), Vec::new());
            let mut o = obj.clone();
            apply(&mut o, &evs[i]);
            for j in i + 1..evs.len() {
                let e = evs[j].clone();
                let par = parity(sig, &o, &e);
                match e {
                    Ev::Dot(q) if q == b || q == b + 1 => {
                        m += 1;
                        if (q == b) == up_left {
                            e_leg.push((j, par));
                        } else {
                            f_leg.push((j, par));
                        }
                    }
                    Ev::Cap { pos: q, bub: None } if q == b => {
                        let pi = sig.pi[colour as usize];
                        let mut order = vec![(i, up_left && pi)];
                        if up_left {
                            order.extend(e_leg.iter().copied());
                            order.extend(f_leg.iter().rev().copied());
                        } else {
                            order.extend(f_leg.iter().rev().copied());
                            order.extend(e_leg.iter().copied());
                        }
                        order.push((j, !up_left && pi));
                        order.extend(between_par);
                        neg ^= koszul(&order);
                        let index = if up_left { m + 1 - h } else { m + 1 + h };
                        let mut out = evs[..i].to_vec();
                        out.push(Ev::Bub {
                            pos,
                            colour,
                            cw: up_left,
                            index,
                        });
                        out.extend(between);
                        out.extend_from_slice(&evs[j + 1..]);
                        evs = out;
                        i += 1;
                        continue 'next;
                    }
                    _ => {
                        let (q, s, t) = extent(&e);
                        if q + s <= b && !(s == 0 && t == 0 && q > b) {
                            b = b + t - s;
                            between.push(e.clone());
                        } else if q >= b + 2 {
                            between.push(moved(&e, -2));
                        } else {
                            break;
                        }
                        between_par.push((j, par));
                    }
                }
                apply(&mut o, &e);
            }
        }
        apply(&mut obj, &evs[i]);
        i += 1;
    }
    (evs, neg)
}

/// One term `coef · p ≫ q` of a sorting decomposition of the identity of a word.
#[derive(Clone, Debug)]
pub(super) struct Comp {
    pub coef: i64,
    pub p: Vec<Ev>,
    pub q: Vec<Ev>,
    pub sorted: Vec<Letter>,
}

/// The weight of the region left of position `p` of `w`, from the weight `right` of its
/// rightmost region.
pub(super) fn region(a: &[Vec<i64>], right: &[i64], w: &[Letter], p: usize) -> Vec<i64> {
    let mut r = right.to_vec();
    for l in &w[p..] {
        let s = if l.up { 1 } else { -1 };
        for (c, x) in r.iter_mut().enumerate() {
            *x += s * a[c][l.colour as usize];
        }
    }
    r
}

/// Whether `w` is sorted: all `E` before all `F` (`r_sorted`), or all `F` before all `E`.
pub(super) fn first_inversion(w: &[Letter], r_sorted: bool) -> Option<usize> {
    (0..w.len().saturating_sub(1)).find(|&p| {
        if r_sorted {
            !w[p].up && w[p + 1].up
        } else {
            w[p].up && !w[p + 1].up
        }
    })
}

fn sign(neg: bool) -> i64 {
    if neg { -1 } else { 1 }
}

/// The sorting decomposition of `1_w` (rightmost region `sig.mu`): onto `E…E F…F` with `p` of
/// type LR (`r_sorted`), or onto `F…F E…E` with `p` of type RL, from the inversion relation of
/// BE2 (Definition 2.1, with `t_ij = 1`, `s_ij^pq = 0`) and (1.28).
pub(super) fn decompose(sig: &Sig, w: &[Letter], r_sorted: bool) -> Vec<Comp> {
    let Some(p) = first_inversion(w, r_sorted) else {
        return vec![Comp {
            coef: 1,
            p: vec![],
            q: vec![],
            sorted: w.to_vec(),
        }];
    };
    let (i, j) = (w[p].colour, w[p + 1].colour);
    let odd = |c: u16| sig.odd[c as usize];
    let hr = region(sig.a, sig.mu, w, p + 2);
    let h = hr[i as usize];
    let mut swapped = w.to_vec();
    swapped.swap(p, p + 1);
    let mut removed = w.to_vec();
    removed.drain(p..p + 2);
    // (coef, p, q, middle word)
    type Local = (i64, Vec<Ev>, Vec<Ev>, Vec<Letter>);
    let mut local: Vec<Local> = Vec::new();
    if r_sorted {
        // F_i E_j → E_j F_i (LR) and back (RL)
        let fwd = vec![
            Ev::Cup {
                pos: p + 2,
                colour: i,
                up_left: true,
            },
            Ev::Cross(p + 1),
            Ev::Cap { pos: p, bub: None },
        ];
        let back = vec![
            Ev::Cup {
                pos: p,
                colour: i,
                up_left: false,
            },
            Ev::Cross(p + 1),
            Ev::Cap {
                pos: p + 2,
                bub: None,
            },
        ];
        if i != j {
            let s = odd(i) && odd(j) && (hr[i as usize] + 1) % 2 != 0;
            local.push((sign(s), fwd, back, swapped));
        } else {
            let pi = odd(i) && (h + 1) % 2 != 0;
            local.push((sign(!pi), fwd, back, swapped));
            for n in 0..=(-h - 1) {
                for r in 0..=(-h - n - 1) {
                    let mut pp = vec![Ev::Dot(p + 1); r as usize];
                    pp.push(Ev::Cap {
                        pos: p,
                        bub: Some((true, -n - r - 1 - h)),
                    });
                    let mut qq = vec![Ev::Cup {
                        pos: p,
                        colour: i,
                        up_left: false,
                    }];
                    qq.extend(vec![Ev::Dot(p + 1); n as usize]);
                    let s = odd(i) && (h + n + r + 1) % 2 != 0;
                    local.push((sign(s), pp, qq, removed.clone()));
                }
            }
        }
    } else {
        // E_i F_j → F_j E_i (RL) and back (LR)
        let fwd = vec![
            Ev::Cup {
                pos: p,
                colour: j,
                up_left: false,
            },
            Ev::Cross(p + 1),
            Ev::Cap {
                pos: p + 2,
                bub: None,
            },
        ];
        let back = vec![
            Ev::Cup {
                pos: p + 2,
                colour: j,
                up_left: true,
            },
            Ev::Cross(p + 1),
            Ev::Cap { pos: p, bub: None },
        ];
        if i != j {
            let s = odd(i) && odd(j) && (hr[j as usize] + 1) % 2 != 0;
            local.push((sign(s), fwd, back, swapped));
        } else {
            let pi = odd(i) && (h + 1) % 2 != 0;
            local.push((sign(!pi), fwd, back, swapped));
            for n in 0..=(h - 1) {
                for r in 0..=(h - n - 1) {
                    let mut pp = vec![Ev::Dot(p); n as usize];
                    pp.push(Ev::Cap {
                        pos: p,
                        bub: Some((false, h - n - r - 1)),
                    });
                    let mut qq = vec![Ev::Cup {
                        pos: p,
                        colour: i,
                        up_left: true,
                    }];
                    qq.extend(vec![Ev::Dot(p); r as usize]);
                    let s = odd(i) && (h + n + r + 1) % 2 != 0;
                    local.push((sign(s), pp, qq, removed.clone()));
                }
            }
        }
    }
    let mut out = Vec::new();
    for (c, pl, ql, mid) in local {
        for sub in decompose(sig, &mid, r_sorted) {
            let mut p = pl.clone();
            p.extend(sub.p);
            let mut q = sub.q;
            q.extend(ql.iter().cloned());
            out.push(Comp {
                coef: c * sub.coef,
                p,
                q,
                sorted: sub.sorted,
            });
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    Bottom(usize),
    Top(usize),
    Turn(usize),
    Open,
}

/// An atom of a diagram: its height (its step's index, twice, plus one for the bubble of a cap)
/// and its parity.
type Atom = (i64, bool);

#[derive(Clone, Debug)]
enum SegEv {
    Dot(Atom),
    Cross(usize, Atom),
    /// A bubble beside the segment, on its right in the picture when `right`.
    Bub {
        colour: u16,
        cw: bool,
        index: i64,
        right: bool,
        atom: Atom,
    },
}

struct Seg {
    up: bool,
    colour: u16,
    evs: Vec<SegEv>,
    lo: End,
    hi: End,
}

struct Turn {
    segs: [usize; 2],
    bub: Option<(bool, i64)>,
    bub_atom: Atom,
    atom: Atom,
    colour: u16,
    cup: bool,
}

#[derive(Clone, Debug)]
enum StrandEv {
    Dot(Atom),
    Cross(usize, Atom),
    Bub {
        colour: u16,
        cw: bool,
        index: i64,
        right: bool,
        atom: Atom,
    },
}

/// An upward diagram with the colours of its bottom, and the sign of `m = ± Bend(U)`.
pub(super) struct Straight<E> {
    pub coef: i64,
    pub bottom: Vec<u16>,
    pub evs: Vec<UpEv<E>>,
}

/// Rewrites the downward dots and crossings of a diagram of type LR through the leftward cups
/// and caps: `xd = 2 δ − ldot` for an odd colour and `xd = ldot` for an even one (BE2 (1.26),
/// (1.27)), `psid = (−1)^{p_a p_b (⟨a,λ⟩ + ⟨b,λ⟩ + 1)} lcrossMate` (BE2 (1.28) corrected).
fn expand_lr(sig: &Sig, src: &[Letter], evs: &[Ev]) -> Vec<(i64, Vec<Ev>)> {
    let mut terms: Vec<(i64, Vec<Ev>)> = vec![(1, vec![])];
    let mut obj = src.to_vec();
    for ev in evs {
        match *ev {
            Ev::Dot(q) if !obj[q].up => {
                let c = obj[q].colour;
                let ldot = [
                    Ev::Cup {
                        pos: q + 1,
                        colour: c,
                        up_left: true,
                    },
                    Ev::Dot(q + 1),
                    Ev::Cap { pos: q, bub: None },
                ];
                if sig.odd[c as usize] {
                    let mut next = Vec::with_capacity(2 * terms.len());
                    for (k, t) in terms {
                        let mut a = t.clone();
                        a.extend(ldot.iter().cloned());
                        next.push((-k, a));
                        let mut b = t;
                        b.push(Ev::Bub {
                            pos: q,
                            colour: c,
                            cw: true,
                            index: 1,
                        });
                        next.push((2 * k, b));
                    }
                    terms = next;
                } else {
                    for (_, t) in terms.iter_mut() {
                        t.extend(ldot.iter().cloned());
                    }
                }
            }
            Ev::Cross(q) if !obj[q].up => {
                let (a, b) = (obj[q].colour, obj[q + 1].colour);
                let h = region(sig.a, sig.mu, &obj, q + 2);
                let s = sig.odd[a as usize]
                    && sig.odd[b as usize]
                    && (h[a as usize] + h[b as usize] + 1) % 2 != 0;
                let mate = [
                    Ev::Cup {
                        pos: q + 2,
                        colour: a,
                        up_left: true,
                    },
                    Ev::Cup {
                        pos: q + 3,
                        colour: b,
                        up_left: true,
                    },
                    Ev::Cross(q + 2),
                    Ev::Cap {
                        pos: q + 1,
                        bub: None,
                    },
                    Ev::Cap { pos: q, bub: None },
                ];
                for (k, t) in terms.iter_mut() {
                    t.extend(mate.iter().cloned());
                    if s {
                        *k = -*k;
                    }
                }
            }
            _ => {
                for (_, t) in terms.iter_mut() {
                    t.push(ev.clone());
                }
            }
        }
        apply(&mut obj, ev);
    }
    terms
}

/// Bends a monotone diagram (type RL when `rl`, else LR) from `src` into upward diagrams, as in
/// the module documentation: `m = Σ coef · Bend(U)`.
pub(super) fn straighten<E>(
    sig: &Sig,
    src: &[Letter],
    evs: &[Ev],
    rl: bool,
) -> Result<Vec<Straight<E>>> {
    let terms = if rl {
        vec![(1, evs.to_vec())]
    } else {
        expand_lr(sig, src, evs)
    };
    let mut out = Vec::with_capacity(terms.len());
    for (k, t) in terms {
        let mut st = straighten_one::<E>(sig, src, &t, rl)?;
        st.coef *= k;
        out.push(st);
    }
    Ok(out)
}

fn straighten_one<E>(sig: &Sig, src: &[Letter], evs: &[Ev], rl: bool) -> Result<Straight<E>> {
    let bad = || Error::invalid("skm: a diagram to straighten is not monotone");
    let mut segs: Vec<Seg> = Vec::new();
    let mut turns: Vec<Turn> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    for (p, l) in src.iter().enumerate() {
        segs.push(Seg {
            up: l.up,
            colour: l.colour,
            evs: vec![],
            lo: End::Bottom(p),
            hi: End::Open,
        });
        cur.push(p);
    }
    let mut obj = src.to_vec();
    let mut ncross = 0;
    for (n, ev) in evs.iter().enumerate() {
        let height = 2 * n as i64;
        let par = parity(sig, &obj, ev);
        match ev {
            Ev::Dot(k) => segs[cur[*k]].evs.push(SegEv::Dot((height, par))),
            Ev::Bub {
                pos,
                colour,
                cw,
                index,
            } => {
                // carried by the strand on its left, or else the one on its right
                let (s, right) = match pos.checked_sub(1) {
                    Some(q) => (cur[q], true),
                    None => (*cur.first().ok_or_else(bad)?, false),
                };
                segs[s].evs.push(SegEv::Bub {
                    colour: *colour,
                    cw: *cw,
                    index: *index,
                    right,
                    atom: (height, par),
                });
            }
            Ev::Cross(k) => {
                let (s, t) = (cur[*k], cur[k + 1]);
                if segs[s].up != segs[t].up {
                    return Err(bad());
                }
                segs[s].evs.push(SegEv::Cross(ncross, (height, par)));
                segs[t].evs.push(SegEv::Cross(ncross, (height, par)));
                ncross += 1;
                cur.swap(*k, k + 1);
            }
            Ev::Cup {
                pos,
                colour,
                up_left,
            } => {
                if *up_left == rl {
                    return Err(bad());
                }
                let t = turns.len();
                let l = segs.len();
                for up in [*up_left, !up_left] {
                    segs.push(Seg {
                        up,
                        colour: *colour,
                        evs: vec![],
                        lo: End::Turn(t),
                        hi: End::Open,
                    });
                }
                turns.push(Turn {
                    segs: [l, l + 1],
                    bub: None,
                    bub_atom: (0, false),
                    atom: (height, par),
                    colour: *colour,
                    cup: true,
                });
                cur.insert(*pos, l + 1);
                cur.insert(*pos, l);
            }
            Ev::Cap { pos, bub } => {
                let (l, r) = (cur[*pos], cur[pos + 1]);
                if segs[l].colour != segs[r].colour || segs[l].up == segs[r].up || segs[l].up != rl
                {
                    return Err(bad());
                }
                let t = turns.len();
                segs[l].hi = End::Turn(t);
                segs[r].hi = End::Turn(t);
                let c = segs[l].colour;
                let turn_par = !rl && sig.pi[c as usize];
                let bub_par = bub.is_some_and(|(_, k)| sig.odd[c as usize] && k % 2 != 0);
                turns.push(Turn {
                    segs: [l, r],
                    bub: *bub,
                    bub_atom: (height + 1, bub_par),
                    atom: (height, turn_par),
                    colour: c,
                    cup: false,
                });
                cur.drain(*pos..pos + 2);
            }
        }
        apply(&mut obj, ev);
    }
    for (p, &s) in cur.iter().enumerate() {
        segs[s].hi = End::Top(p);
    }
    let tgt: Vec<Letter> = cur
        .iter()
        .map(|&s| Letter {
            colour: segs[s].colour,
            up: segs[s].up,
        })
        .collect();
    // RL: src = E_a F_b, tgt = F_d E_c. LR: src = F_d E_c, tgt = E_a F_b.
    let (sorted_r, sorted_l) = if rl { (src, &tgt[..]) } else { (&tgt[..], src) };
    let na = sorted_r.iter().take_while(|l| l.up).count();
    let nb = sorted_r.len() - na;
    let nd = sorted_l.iter().take_while(|l| !l.up).count();
    let nc = sorted_l.len() - nd;
    if sorted_r[na..].iter().any(|l| l.up) || sorted_l[nd..].iter().any(|l| !l.up) {
        return Err(bad());
    }
    // the endpoints of the upward diagram, bottom and top, left to right
    let (bottom_ends, top_ends): (Vec<End>, Vec<End>) = if rl {
        (
            (0..nd)
                .map(|j| End::Top(nd - 1 - j))
                .chain((0..na).map(End::Bottom))
                .collect(),
            (0..nc)
                .map(|i| End::Top(nd + i))
                .chain((0..nb).map(|j| End::Bottom(na + nb - 1 - j)))
                .collect(),
        )
    } else {
        (
            (0..nc)
                .map(|i| End::Bottom(nd + i))
                .chain((0..nb).map(|j| End::Top(na + nb - 1 - j)))
                .collect(),
            (0..nd)
                .map(|j| End::Bottom(nd - 1 - j))
                .chain((0..na).map(End::Top))
                .collect(),
        )
    };
    let n = bottom_ends.len();
    if top_ends.len() != n {
        return Err(bad());
    }
    // trace each strand from its bottom end, with the turns on its way
    let mut visited = vec![false; segs.len()];
    let mut strands: Vec<(u16, Vec<StrandEv>, usize, Vec<usize>)> = Vec::new();
    for start in &bottom_ends {
        let first = match start {
            End::Bottom(_) => segs.iter().position(|s| s.lo == *start && s.up),
            End::Top(_) => segs.iter().position(|s| s.hi == *start && !s.up),
            _ => None,
        }
        .ok_or_else(bad)?;
        let colour = segs[first].colour;
        let mut out = Vec::new();
        let mut path = Vec::new();
        let mut s = first;
        let end = loop {
            if visited[s] {
                return Err(bad());
            }
            visited[s] = true;
            let seg = &segs[s];
            // a downward segment turns half way round when bent up: its sides swap
            let flip = !seg.up;
            let push = |e: &SegEv, out: &mut Vec<StrandEv>| {
                out.push(match e {
                    SegEv::Dot(a) => StrandEv::Dot(*a),
                    SegEv::Cross(c, a) => StrandEv::Cross(*c, *a),
                    SegEv::Bub {
                        colour,
                        cw,
                        index,
                        right,
                        atom,
                    } => StrandEv::Bub {
                        colour: *colour,
                        cw: *cw,
                        index: *index,
                        right: *right != flip,
                        atom: *atom,
                    },
                })
            };
            if seg.up {
                seg.evs.iter().for_each(|e| push(e, &mut out));
                match seg.hi {
                    End::Turn(t) => {
                        let tr = &turns[t];
                        if let Some((cw, index)) = tr.bub {
                            // the up strand turns towards its partner; the bubble is outside
                            out.push(StrandEv::Bub {
                                colour: seg.colour,
                                cw,
                                index,
                                right: tr.segs[1] == s,
                                atom: tr.bub_atom,
                            });
                        }
                        path.push(t);
                        s = if tr.segs[0] == s {
                            tr.segs[1]
                        } else {
                            tr.segs[0]
                        };
                    }
                    e => break e,
                }
            } else {
                seg.evs.iter().rev().for_each(|e| push(e, &mut out));
                match seg.lo {
                    End::Turn(t) => {
                        let tr = &turns[t];
                        path.push(t);
                        s = if tr.segs[0] == s {
                            tr.segs[1]
                        } else {
                            tr.segs[0]
                        };
                    }
                    e => break e,
                }
            }
        };
        let top = top_ends.iter().position(|e| *e == end).ok_or_else(bad)?;
        strands.push((colour, out, top, path));
    }
    if visited.iter().any(|v| !v) {
        return Err(Error::invalid(
            "skm: a diagram to straighten has a closed loop",
        ));
    }
    // sweep: emit any event that is next on its strands, crossings only of neighbours
    let bottom: Vec<u16> = strands.iter().map(|s| s.0).collect();
    let mut order: Vec<usize> = (0..n).collect();
    let mut next = vec![0usize; n];
    let mut out: Vec<UpEv<E>> = Vec::new();
    let mut emitted: Vec<Atom> = Vec::new();
    loop {
        let mut progressed = false;
        for p in 0..n {
            let s = order[p];
            let Some(ev) = strands[s].1.get(next[s]) else {
                continue;
            };
            match ev {
                StrandEv::Dot(a) => {
                    out.push(UpEv::Dot(p));
                    emitted.push(*a);
                    next[s] += 1;
                }
                StrandEv::Bub {
                    colour,
                    cw,
                    index,
                    right,
                    atom,
                } => {
                    out.push(UpEv::Bub {
                        pos: if *right { p + 1 } else { p },
                        colour: *colour,
                        cw: *cw,
                        index: *index,
                    });
                    emitted.push(*atom);
                    next[s] += 1;
                }
                StrandEv::Cross(c, a) => {
                    if p + 1 >= n {
                        continue;
                    }
                    let t = order[p + 1];
                    match strands[t].1.get(next[t]) {
                        Some(StrandEv::Cross(d, _)) if d == c => {
                            out.push(UpEv::Cross(p));
                            emitted.push(*a);
                            next[s] += 1;
                            next[t] += 1;
                            order.swap(p, p + 1);
                        }
                        _ => continue,
                    }
                }
            }
            progressed = true;
            break;
        }
        if !progressed {
            break;
        }
    }
    if (0..n).any(|s| next[s] < strands[s].1.len()) {
        return Err(Error::invalid(
            "skm: a monotone diagram does not straighten",
        ));
    }
    if order.iter().enumerate().any(|(p, &s)| strands[s].2 != p) {
        return Err(Error::invalid(
            "skm: a straightened diagram ends in the wrong order",
        ));
    }
    // the sign: the odd atoms reordered from their heights in m to their order in Bend(U)
    let neg = if rl {
        koszul(&emitted)
    } else {
        // Bend_L(U) = (ε' caps, innermost lowest) ∘ (1 ⊗ U ⊗ 1) ∘ (η' cups, outermost lowest);
        // the cups on the strands from the bottom right and the caps on the strands to the top
        // left are its own, the others pair into zigzags, and a downward strand without turns
        // gets a zigzag-left-down (no sign) at the bottom.
        let mut cups: Vec<Option<Atom>> = vec![None; nb];
        let mut caps: Vec<Option<Atom>> = vec![None; nd];
        let mut pairs: Vec<Atom> = Vec::new();
        let mut zig = false;
        let mut virt = 0i64;
        for (s, (_, _, tpos, path)) in strands.iter().enumerate() {
            let mut path: &[usize] = path;
            let start_down = s >= nc;
            let end_down = *tpos < nd;
            if start_down {
                match path.first() {
                    Some(&t) if turns[t].cup => {
                        cups[s - nc] = Some(turns[t].atom);
                        path = &path[1..];
                    }
                    None if end_down => {
                        virt += 1;
                        cups[s - nc] = Some((-2 * virt, turns_par(sig, strands[s].0)));
                        caps[*tpos] = Some((-2 * virt + 1, turns_par(sig, strands[s].0)));
                        continue;
                    }
                    _ => return Err(bad()),
                }
            }
            if end_down {
                match path.last() {
                    Some(&t) if !turns[t].cup => {
                        caps[*tpos] = Some(turns[t].atom);
                        path = &path[..path.len() - 1];
                    }
                    _ => return Err(bad()),
                }
            }
            if !path.len().is_multiple_of(2) {
                return Err(bad());
            }
            for z in path.chunks(2) {
                let (cap, cup) = (&turns[z[0]], &turns[z[1]]);
                if cap.cup || !cup.cup {
                    return Err(bad());
                }
                pairs.push(cup.atom);
                pairs.push(cap.atom);
                zig ^= sig.pi[cap.colour as usize];
            }
        }
        let mut items: Vec<Atom> = Vec::new();
        items.extend(
            cups.into_iter()
                .map(|c| c.ok_or_else(bad))
                .collect::<Result<Vec<_>>>()?,
        );
        items.extend(emitted.iter().copied());
        items.extend(pairs);
        items.extend(
            caps.into_iter()
                .map(|c| c.ok_or_else(bad))
                .collect::<Result<Vec<_>>>()?,
        );
        koszul(&items) ^ zig
    };
    Ok(Straight {
        coef: if neg { -1 } else { 1 },
        bottom,
        evs: out,
    })
}

/// The parity of the leftward cups and caps of a colour.
fn turns_par(sig: &Sig, c: u16) -> bool {
    sig.pi[c as usize]
}
