//! Monotone diagrams: the sorting decompositions of KL III Lemma 3.9 and their straightening.
//!
//! A diagram is *of type RL* when its cups are `1 → F E` and its caps `E F → 1` (besides dots and
//! crossings of strands with the same orientation), and *of type LR* when its cups are
//! `1 → E F` and its caps `F E → 1`. Bending the downward strands at the bottom right of an RL
//! diagram `E_a F_b → F_d E_c` up and those at its top left down gives an upward diagram
//! `E_{rev d} E_a → E_c E_{rev b}` ([`straighten`]); for LR diagrams `F_d E_c → E_a F_b` the
//! mirror bending gives `E_c E_{rev b} → E_{rev d} E_a`.
//!
//! The identity of any 1-morphism is a sum of `p ≫ q` that sorts it ([`decompose`]): with `p` of
//! type LR onto `E…E F…F` (or of type RL onto `F…F E…E`), from the relations `decompose-fe`,
//! `decompose-ef`, `mixed-fe`, `mixed-ef` of the presentation. The bubbles of those relations
//! stay as atoms above the cap they follow.

use crate::error::{Error, Result};
use crate::rep::kl::up::UpEv;
use crate::term::Letter;

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

/// The position, number of inputs and number of outputs of a step.
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

/// Replaces each cup that a cap closes with only dots in between by one bubble step (its
/// degree index from the dots and the weight of its region, as the `cw`/`ccw` families of the
/// presentation read), for a diagram from the empty object with rightmost region `right`.
pub(super) fn collapse_bubbles(a: &[Vec<i64>], right: &[i64], evs: &[Ev]) -> Vec<Ev> {
    let mut evs = evs.to_vec();
    let mut obj: Vec<Letter> = Vec::new();
    let mut i = 0;
    'next: while i < evs.len() {
        if let Ev::Cup {
            pos,
            colour,
            up_left,
        } = evs[i]
        {
            let h = region(a, right, &obj, pos)[colour as usize];
            let (mut b, mut m) = (pos, 0i64);
            let mut between = Vec::new();
            for j in i + 1..evs.len() {
                let e = &evs[j];
                match *e {
                    Ev::Dot(q) if q == b || q == b + 1 => m += 1,
                    Ev::Cap { pos: q, bub: None } if q == b => {
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
                        let (q, s, t) = extent(e);
                        if q + s <= b && !(s == 0 && t == 0 && q > b) {
                            b = b + t - s;
                            between.push(e.clone());
                        } else if q >= b + 2 {
                            between.push(moved(e, -2));
                        } else {
                            break;
                        }
                    }
                }
            }
        }
        apply(&mut obj, &evs[i]);
        i += 1;
    }
    evs
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

/// The sorting decomposition of `1_w` with `w`'s rightmost region `right`: onto
/// `E…E F…F` with `p` of type LR (`r_sorted`), or onto `F…F E…E` with `p` of type RL.
pub(super) fn decompose(a: &[Vec<i64>], right: &[i64], w: &[Letter], r_sorted: bool) -> Vec<Comp> {
    let Some(p) = first_inversion(w, r_sorted) else {
        return vec![Comp {
            coef: 1,
            p: vec![],
            q: vec![],
            sorted: w.to_vec(),
        }];
    };
    let (i, j) = (w[p].colour, w[p + 1].colour);
    let h = region(a, right, w, p + 2)[i as usize];
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
            local.push((1, fwd, back, swapped));
        } else {
            local.push((-1, fwd, back, swapped));
            for f in 0..(-h).max(0) {
                for g in 0..=f {
                    let mut pp = vec![Ev::Dot(p); (f - g) as usize];
                    pp.push(Ev::Cap {
                        pos: p,
                        bub: Some((true, g)),
                    });
                    let mut qq = vec![Ev::Cup {
                        pos: p,
                        colour: i,
                        up_left: false,
                    }];
                    qq.extend(vec![Ev::Dot(p + 1); (-h - 1 - f) as usize]);
                    local.push((1, pp, qq, removed.clone()));
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
            local.push((1, fwd, back, swapped));
        } else {
            local.push((-1, fwd, back, swapped));
            for f in 0..h.max(0) {
                for g in 0..=f {
                    let mut pp = vec![Ev::Dot(p); (f - g) as usize];
                    pp.push(Ev::Cap {
                        pos: p,
                        bub: Some((false, g)),
                    });
                    let mut qq = vec![Ev::Cup {
                        pos: p,
                        colour: i,
                        up_left: true,
                    }];
                    qq.extend(vec![Ev::Dot(p + 1); (h - 1 - f) as usize]);
                    local.push((1, pp, qq, removed.clone()));
                }
            }
        }
    }
    let mut out = Vec::new();
    for (c, pl, ql, mid) in local {
        for sub in decompose(a, right, &mid, r_sorted) {
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

#[derive(Clone, Debug)]
enum SegEv {
    Dot,
    Cross(usize),
    /// A bubble beside the segment, on its right in the picture when `right`.
    Bub {
        colour: u16,
        cw: bool,
        index: i64,
        right: bool,
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
}

#[derive(Clone, Debug)]
enum StrandEv {
    Dot,
    Cross(usize),
    Bub {
        colour: u16,
        cw: bool,
        index: i64,
        right: bool,
    },
}

/// An upward diagram with the colours of its bottom and top.
pub(super) struct Straight<E> {
    pub bottom: Vec<u16>,
    pub top: Vec<u16>,
    pub evs: Vec<UpEv<E>>,
}

/// Bends a monotone diagram (type RL when `rl`, else LR) from `src` into an upward diagram, as in
/// the module documentation.
pub(super) fn straighten<E>(src: &[Letter], evs: &[Ev], rl: bool) -> Result<Straight<E>> {
    let bad = || Error::invalid("kl: a diagram to straighten is not monotone");
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
    let mut ncross = 0;
    for ev in evs {
        match ev {
            Ev::Dot(k) => segs[cur[*k]].evs.push(SegEv::Dot),
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
                });
            }
            Ev::Cross(k) => {
                let (s, t) = (cur[*k], cur[k + 1]);
                if segs[s].up != segs[t].up {
                    return Err(bad());
                }
                segs[s].evs.push(SegEv::Cross(ncross));
                segs[t].evs.push(SegEv::Cross(ncross));
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
                turns.push(Turn {
                    segs: [l, r],
                    bub: *bub,
                });
                cur.drain(*pos..pos + 2);
            }
        }
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
    // trace each strand from its bottom end
    let mut visited = vec![false; segs.len()];
    let mut strands: Vec<(u16, Vec<StrandEv>, usize)> = Vec::new();
    for start in &bottom_ends {
        let first = match start {
            End::Bottom(_) => segs.iter().position(|s| s.lo == *start && s.up),
            End::Top(_) => segs.iter().position(|s| s.hi == *start && !s.up),
            _ => None,
        }
        .ok_or_else(bad)?;
        let colour = segs[first].colour;
        let mut out = Vec::new();
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
                    SegEv::Dot => StrandEv::Dot,
                    SegEv::Cross(c) => StrandEv::Cross(*c),
                    SegEv::Bub {
                        colour,
                        cw,
                        index,
                        right,
                    } => StrandEv::Bub {
                        colour: *colour,
                        cw: *cw,
                        index: *index,
                        right: *right != flip,
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
                            });
                        }
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
        strands.push((colour, out, top));
    }
    if visited.iter().any(|v| !v) {
        return Err(Error::invalid(
            "kl: a diagram to straighten has a closed loop",
        ));
    }
    // sweep: emit any event that is next on its strands, crossings only of neighbours
    let bottom: Vec<u16> = strands.iter().map(|s| s.0).collect();
    let mut order: Vec<usize> = (0..n).collect();
    let mut next = vec![0usize; n];
    let mut out: Vec<UpEv<E>> = Vec::new();
    loop {
        let mut progressed = false;
        for p in 0..n {
            let s = order[p];
            let Some(ev) = strands[s].1.get(next[s]) else {
                continue;
            };
            match ev {
                StrandEv::Dot => {
                    out.push(UpEv::Dot(p));
                    next[s] += 1;
                }
                StrandEv::Bub {
                    colour,
                    cw,
                    index,
                    right,
                } => {
                    out.push(UpEv::Bub {
                        pos: if *right { p + 1 } else { p },
                        colour: *colour,
                        cw: *cw,
                        index: *index,
                    });
                    next[s] += 1;
                }
                StrandEv::Cross(c) => {
                    if p + 1 >= n {
                        continue;
                    }
                    let t = order[p + 1];
                    match strands[t].1.get(next[t]) {
                        Some(StrandEv::Cross(d)) if d == c => {
                            out.push(UpEv::Cross(p));
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
        return Err(Error::invalid("kl: a monotone diagram does not straighten"));
    }
    if order.iter().enumerate().any(|(p, &s)| strands[s].2 != p) {
        return Err(Error::invalid(
            "kl: a straightened diagram ends in the wrong order",
        ));
    }
    let top: Vec<u16> = order.iter().map(|&s| strands[s].0).collect();
    Ok(Straight {
        bottom,
        top,
        evs: out,
    })
}
