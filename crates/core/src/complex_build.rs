//! Constructions on bounded complexes (`docs/complexes.md`, phase M3): shifts, cones,
//! totalization, Hom complexes, Gaussian elimination, Euler characteristics, and TikZ output.

#![allow(clippy::type_complexity)]

use crate::complex::{
    Additive, Complex, Map, Matrix, Pos, Squares, Summand, Window, fmt_pos, mat_mul,
    range_over_tests, window, zero_matrix,
};
use crate::complex_solve::{HomSpace, coords_of, hom_space, inverse};
use crate::error::{Error, Result};
use crate::linalg::Mat;
use crate::ring::Ring;
use std::collections::{BTreeMap, BTreeSet};

fn negate<A: Additive>(a: &A, m: &Matrix<A::Mor>) -> Matrix<A::Mor> {
    m.iter()
        .map(|r| r.iter().map(|x| a.neg(x)).collect())
        .collect()
}

fn add_pos(p: &[i64], q: &[i64]) -> Pos {
    p.iter().zip(q).map(|(x, y)| x + y).collect()
}

/// `C[k]⟨s⟩`: the term at `p` is `C(p + k·step)` with every summand shifted by `s`; the
/// differential along axis `a` is multiplied by `(−1)^{k_a}`.
pub fn shift<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    by: &[i64],
    grading: i64,
) -> Complex<A::Obj, A::Mor> {
    let step = c.conv.step();
    let off: Pos = by.iter().map(|k| -k * step).collect();
    Complex {
        dim: c.dim,
        conv: c.conv,
        terms: c
            .terms
            .iter()
            .map(|(p, ss)| {
                (
                    add_pos(p, &off),
                    ss.iter()
                        .map(|s| Summand {
                            obj: s.obj.clone(),
                            shift: s.shift + grading,
                        })
                        .collect(),
                )
            })
            .collect(),
        diffs: c
            .diffs
            .iter()
            .map(|((axis, p), m)| {
                let m = if by[*axis].rem_euclid(2) == 1 {
                    negate(a, m)
                } else {
                    m.clone()
                };
                ((*axis, add_pos(p, &off)), m)
            })
            .collect(),
    }
}

/// The cone of `F : C → D` (one-dimensional): `Cone(p) = C(p + step) ⊕ D(p)`, with differential
/// `[[−d_C, 0], [F, d_D]]`. The summands of `C(p + step)` come first.
pub fn cone<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    d: &Complex<A::Obj, A::Mor>,
    f: &Map<A::Mor>,
) -> Result<Complex<A::Obj, A::Mor>> {
    if c.dim != 1 || d.dim != 1 || c.conv.indexing != d.conv.indexing {
        return Err(Error::precondition(
            "cones are built from one-dimensional complexes with the same indexing; totalize \
             first",
        ));
    }
    let step = c.conv.step();
    let mut ps: BTreeSet<Pos> = c.positions().iter().map(|p| vec![p[0] - step]).collect();
    ps.extend(d.positions());
    let mut out = Complex {
        dim: 1,
        conv: d.conv,
        terms: BTreeMap::new(),
        diffs: BTreeMap::new(),
    };
    for p in &ps {
        let cp = c.next(p, 0);
        let mut ss = c.at(&cp).to_vec();
        ss.extend(d.at(p).iter().cloned());
        if !ss.is_empty() {
            out.terms.insert(p.clone(), ss);
        }
    }
    for p in &ps {
        let q = d.next(p, 0);
        let (c1, c2) = (c.next(p, 0), c.next(&q, 0));
        let (n1, m1) = (c.at(&c1).len(), d.at(p).len());
        let (n2, m2) = (c.at(&c2).len(), d.at(&q).len());
        if n1 + m1 == 0 || n2 + m2 == 0 {
            continue;
        }
        let dc = negate(a, &c.diff(a, 0, &c1));
        let fq = f
            .get(&c1)
            .cloned()
            .unwrap_or_else(|| zero_matrix(a, d.at(&c1).len(), n1));
        let dd = d.diff(a, 0, p);
        let mut m = zero_matrix(a, n2 + m2, n1 + m1);
        for i in 0..n2 {
            for j in 0..n1 {
                m[i][j] = dc[i][j].clone();
            }
        }
        for i in 0..m2 {
            for j in 0..n1 {
                m[n2 + i][j] = fq[i][j].clone();
            }
            for j in 0..m1 {
                m[n2 + i][n1 + j] = dd[i][j].clone();
            }
        }
        out.diffs.insert((0, p.clone()), m);
    }
    Ok(out)
}

/// The block of the total term at `n` contributed by position `p` (its first summand index).
type Blocks = BTreeMap<i64, Vec<(Pos, usize)>>;

/// The total complex: `Tot(n) = ⊕_{Σp = n} C(p)` (positions in increasing order), with the
/// differential along axis `a` multiplied by `(−1)^{p_0 + … + p_{a−1}}` when squares commute.
pub fn totalize<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
) -> Result<(Complex<A::Obj, A::Mor>, Blocks)> {
    let mut blocks: Blocks = BTreeMap::new();
    let mut terms: BTreeMap<Pos, Vec<Summand<A::Obj>>> = BTreeMap::new();
    for (p, ss) in &c.terms {
        let n: i64 = p.iter().sum();
        let t = terms.entry(vec![n]).or_default();
        blocks.entry(n).or_default().push((p.clone(), t.len()));
        t.extend(ss.iter().cloned());
    }
    let mut conv = c.conv;
    conv.squares = Squares::Commute;
    let mut out = Complex {
        dim: 1,
        conv,
        terms,
        diffs: BTreeMap::new(),
    };
    for (&n, bs) in &blocks {
        let m_next = n + c.conv.step();
        let rows = out.at(&[m_next]).len();
        if rows == 0 {
            continue;
        }
        let cols = out.at(&[n]).len();
        let mut m = zero_matrix(a, rows, cols);
        let tgt_blocks: BTreeMap<&Pos, usize> = blocks
            .get(&m_next)
            .map(|v| v.iter().map(|(p, o)| (p, *o)).collect())
            .unwrap_or_default();
        for (p, off) in bs {
            for axis in 0..c.dim {
                let q = c.next(p, axis);
                let Some(&toff) = tgt_blocks.get(&q) else {
                    continue;
                };
                let neg = c.conv.squares == Squares::Commute
                    && p[..axis].iter().sum::<i64>().rem_euclid(2) == 1;
                let d = c.diff(a, axis, p);
                for (i, row) in d.iter().enumerate() {
                    for (j, x) in row.iter().enumerate() {
                        m[toff + i][off + j] = if neg { a.neg(x) } else { x.clone() };
                    }
                }
            }
        }
        out.diffs.insert((0, vec![n]), m);
    }
    Ok((out, blocks))
}

/// One step of Gaussian elimination.
pub struct Elimination {
    pub from: i64,
    pub source: usize,
    pub target: usize,
}

/// Gaussian elimination: while some entry `φ` of a differential is an isomorphism, removes its
/// source and target, replacing the differential `[[φ, δ], [γ, ε]]` by `ε − γ φ⁻¹ δ`. The result
/// is homotopy equivalent to the input. One-dimensional complexes only.
pub fn gauss<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    limit: usize,
) -> Result<(Complex<A::Obj, A::Mor>, Vec<Elimination>, bool)> {
    if c.dim != 1 {
        return Err(Error::precondition(
            "Gaussian elimination works on one-dimensional complexes; totalize first",
        ));
    }
    let mut c = Complex {
        dim: 1,
        conv: c.conv,
        terms: c.terms.clone(),
        diffs: c.diffs.clone(),
    };
    let mut steps = Vec::new();
    'outer: loop {
        if steps.len() >= limit {
            return Ok((c, steps, false));
        }
        for p in c.positions() {
            let q = c.next(&p, 0);
            let (x, y) = (c.at(&p).to_vec(), c.at(&q).to_vec());
            if y.is_empty() {
                continue;
            }
            let d = c.diff(a, 0, &p);
            for i in 0..y.len() {
                for j in 0..x.len() {
                    if a.is_zero(&d[i][j])? {
                        continue;
                    }
                    let Some(inv) = inverse(
                        a,
                        std::slice::from_ref(&x[j]),
                        std::slice::from_ref(&y[i]),
                        &vec![vec![d[i][j].clone()]],
                    )?
                    else {
                        continue;
                    };
                    eliminate(a, &mut c, &p, i, j, &inv[0][0]);
                    steps.push(Elimination {
                        from: p[0],
                        source: j,
                        target: i,
                    });
                    continue 'outer;
                }
            }
        }
        return Ok((c, steps, true));
    }
}

fn eliminate<A: Additive>(
    a: &A,
    c: &mut Complex<A::Obj, A::Mor>,
    p: &[i64],
    i: usize,
    j: usize,
    phi_inv: &A::Mor,
) {
    let q = c.next(p, 0);
    let bk = c.prev(p, 0);
    let d = c.diff(a, 0, p);
    // ε − γ φ⁻¹ δ on the remaining rows and columns
    let rows: Vec<usize> = (0..d.len()).filter(|&r| r != i).collect();
    let cols: Vec<usize> = (0..c.at(p).len()).filter(|&s| s != j).collect();
    let new: Matrix<A::Mor> = rows
        .iter()
        .map(|&r| {
            cols.iter()
                .map(|&s| {
                    let corr = a.compose(&d[r][j], &a.compose(phi_inv, &d[i][s]));
                    a.add(&d[r][s], &a.neg(&corr))
                })
                .collect()
        })
        .collect();
    let into = c.diff(a, 0, &bk);
    let out = c.diff(a, 0, &q);
    let has_into = c.diffs.contains_key(&(0, bk.to_vec()));
    let has_out = c.diffs.contains_key(&(0, q.clone()));
    let mut remove = |pos: &[i64], k: usize| {
        let v = c.terms.get_mut(pos).unwrap();
        v.remove(k);
        if v.is_empty() {
            c.terms.remove(pos);
        }
    };
    remove(p, j);
    remove(&q, i);
    c.diffs.insert((0, p.to_vec()), new);
    if has_into {
        let m: Matrix<A::Mor> = into
            .into_iter()
            .enumerate()
            .filter(|(r, _)| *r != j)
            .map(|(_, row)| row)
            .collect();
        c.diffs.insert((0, bk.to_vec()), m);
    }
    if has_out {
        let m: Matrix<A::Mor> = out
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .enumerate()
                    .filter(|(s, _)| *s != i)
                    .map(|(_, x)| x)
                    .collect()
            })
            .collect();
        c.diffs.insert((0, q.clone()), m);
    }
    // drop differentials whose source or target vanished
    let keys: Vec<(usize, Pos)> = c.diffs.keys().cloned().collect();
    for k in keys {
        if c.at(&k.1).is_empty() || c.at(&c.next(&k.1, 0)).is_empty() {
            c.diffs.remove(&k);
        }
    }
}

/// The Euler characteristic `Σ_p (−1)^{|p|} Σ_i q^{s_i} [X_i]`, grouped by object (by key, named
/// by the first object written): for each object, Laurent coefficients by power of `q`.
pub fn euler<A: Additive>(a: &A, c: &Complex<A::Obj, A::Mor>) -> Vec<(A::Obj, BTreeMap<i64, i64>)> {
    let mut order: Vec<A::Key> = Vec::new();
    let mut by: BTreeMap<A::Key, (A::Obj, BTreeMap<i64, i64>)> = BTreeMap::new();
    for (p, ss) in &c.terms {
        let sign = if p.iter().sum::<i64>().rem_euclid(2) == 0 {
            1
        } else {
            -1
        };
        for s in ss {
            let k = a.key(&s.obj);
            let e = by.entry(k.clone()).or_insert_with(|| {
                order.push(k);
                (s.obj.clone(), BTreeMap::new())
            });
            *e.1.entry(s.shift).or_insert(0) += sign;
        }
    }
    order
        .into_iter()
        .filter_map(|k| {
            let (o, mut coeffs) = by.remove(&k)?;
            coeffs.retain(|_, v| *v != 0);
            (!coeffs.is_empty()).then_some((o, coeffs))
        })
        .collect()
}

/// A Laurent polynomial in `q` as text: `q^2 - 1`, `-q^-1`, `0`.
pub fn laurent_text(coeffs: &BTreeMap<i64, i64>) -> String {
    let mut s = String::new();
    for (&e, &c) in coeffs.iter().rev() {
        if c == 0 {
            continue;
        }
        let (sign, abs) = if c < 0 { ("-", -c) } else { ("+", c) };
        if s.is_empty() {
            if sign == "-" {
                s.push('-');
            }
        } else {
            s.push_str(&format!(" {sign} "));
        }
        let mono = match e {
            0 => String::new(),
            1 => "q".into(),
            e => format!("q^{e}"),
        };
        if mono.is_empty() {
            s.push_str(&abs.to_string());
        } else if abs == 1 {
            s.push_str(&mono);
        } else {
            s.push_str(&format!("{abs}{mono}"));
        }
    }
    if s.is_empty() { "0".into() } else { s }
}

/// The dimensions of the cohomology of `Hom(C, D)` in internal degree `e`: `Hom^n` is the
/// product over `p` of the degree-`e` maps `C(p) → D(p + n)`, with `δ f = d_D f − (−1)^n f d_C`.
/// Returns, for each `n` where `Hom^n` is nonzero, `(n, dim Hom^n, dim H^n)`.
pub fn hom_cohomology<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    d: &Complex<A::Obj, A::Mor>,
    e: i64,
) -> Result<Vec<(i64, usize, usize)>> {
    if c.dim != 1 || d.dim != 1 || c.conv.indexing != d.conv.indexing {
        return Err(Error::precondition(
            "Hom complexes are built from one-dimensional complexes with the same indexing; \
             totalize first",
        ));
    }
    let k = a.field().clone();
    let step = c.conv.step();
    let mut shifts: BTreeSet<i64> = BTreeSet::new();
    for p in c.positions() {
        for q in d.positions() {
            shifts.insert(q[0] - p[0]);
        }
    }
    let Some((&lo, &hi)) = shifts.iter().next().zip(shifts.iter().next_back()) else {
        return Ok(vec![]);
    };
    // the spaces Hom^n, for n from lo − 1 to hi + 1
    let comps = |n: i64| -> Result<Vec<(Pos, HomSpace<A::Obj, A::Mor>)>> {
        let mut v = Vec::new();
        for p in c.positions() {
            let q = vec![p[0] + n];
            if d.at(&q).is_empty() {
                continue;
            }
            v.push((p.clone(), hom_space(a, c.at(&p), d.at(&q), e)?));
        }
        Ok(v)
    };
    let mut spaces: BTreeMap<i64, Vec<(Pos, HomSpace<A::Obj, A::Mor>)>> = BTreeMap::new();
    for n in lo - 1..=hi + 1 {
        spaces.insert(n, comps(n)?);
    }
    let dim_of = |v: &[(Pos, HomSpace<A::Obj, A::Mor>)]| v.iter().map(|(_, h)| h.dim()).sum();
    // the matrix of δ : Hom^n → Hom^{n+step}
    let delta = |n: i64| -> Result<Mat<<A::Field as Ring>::Elem>> {
        let src = &spaces[&n];
        let tgt = &spaces[&(n + step)];
        let (ns, nt): (usize, usize) = (dim_of(src), dim_of(tgt));
        let toff: BTreeMap<&Pos, usize> = tgt
            .iter()
            .scan(0, |acc, (p, h)| {
                let o = *acc;
                *acc += h.dim();
                Some((p, o))
            })
            .collect();
        let sign = if n.rem_euclid(2) == 0 { -1 } else { 1 };
        let mut cols: Vec<Vec<<A::Field as Ring>::Elem>> = Vec::with_capacity(ns);
        for (p, h) in src {
            let q = vec![p[0] + n];
            for (i, j, b) in &h.basis {
                let mut f = zero_matrix(a, d.at(&q).len(), c.at(p).len());
                f[*i][*j] = b.clone();
                let mut col = vec![k.zero(); nt];
                // d_D f : C(p) → D(q + step), at the target component p
                if let Some(&o) = toff.get(p) {
                    let x = mat_mul(a, &d.diff(a, 0, &q), &f, d.at(&q).len(), c.at(p).len());
                    let th = &tgt.iter().find(|(pp, _)| pp == p).unwrap().1;
                    for (t, v) in coords_of(a, th, &x)?.into_iter().enumerate() {
                        col[o + t] = k.add(&col[o + t], &v);
                    }
                }
                // ±f d_C : C(p − step) → D(q), at the target component p − step
                let pb = c.prev(p, 0);
                if let Some(&o) = toff.get(&pb)
                    && !c.at(&pb).is_empty()
                {
                    let y = mat_mul(a, &f, &c.diff(a, 0, &pb), c.at(p).len(), c.at(&pb).len());
                    let y = if sign < 0 { negate(a, &y) } else { y };
                    let th = &tgt.iter().find(|(pp, _)| *pp == pb).unwrap().1;
                    for (t, v) in coords_of(a, th, &y)?.into_iter().enumerate() {
                        col[o + t] = k.add(&col[o + t], &v);
                    }
                }
                cols.push(col);
            }
        }
        Ok(Mat::from_fn(nt, ns, |i, j| cols[j][i].clone()))
    };
    let rank = |m: &Mat<<A::Field as Ring>::Elem>| -> Result<usize> {
        if m.nrows() == 0 || m.ncols() == 0 {
            Ok(0)
        } else {
            crate::linalg::rank(&k, m)
        }
    };
    let mut out = Vec::new();
    for n in lo..=hi {
        let dn = dim_of(&spaces[&n]);
        if dn == 0 {
            continue;
        }
        let r_out = rank(&delta(n)?)?;
        let r_in = rank(&delta(n - step)?)?;
        out.push((n, dn, dn - r_out - r_in));
    }
    Ok(out)
}

/// The internal degrees `e` for which some degree-`e` map `C(p) → D(q)` can be nonzero, as a
/// window.
pub fn hom_window<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    d: &Complex<A::Obj, A::Mor>,
    max: Option<i64>,
    span: i64,
) -> Result<Option<Window>> {
    // a degree-e map X⟨s⟩ → Y⟨t⟩ is an element of degree s − t + e: e = deg − s + t
    let mut range: Option<(i64, Option<i64>)> = None;
    for xs in c.terms.values() {
        for x in xs {
            let ys: Vec<Summand<A::Obj>> = d
                .terms
                .values()
                .flatten()
                .map(|y| Summand {
                    obj: y.obj.clone(),
                    shift: y.shift - x.shift,
                })
                .collect();
            let tests = [x.obj.clone()];
            if let Some((lo, hi)) = range_over_tests(a, &tests, &ys)? {
                range = Some(match range {
                    None => (lo, hi),
                    Some((l, h)) => (l.min(lo), h.zip(hi).map(|(x, y)| x.max(y))),
                });
            }
        }
    }
    // range_over_tests gives deg + t − s; the internal degree of the map is the same number
    Ok(window(range, max, span))
}

/// TikZ (tikz-cd) source for a one-dimensional complex: terms joined by arrows labelled by their
/// matrices.
pub fn tikz<A: Additive>(a: &A, c: &Complex<A::Obj, A::Mor>) -> Result<String> {
    if c.dim != 1 {
        return Err(Error::precondition(
            "TikZ output is for one-dimensional complexes; totalize first",
        ));
    }
    let tex_obj = |s: &Summand<A::Obj>| -> String {
        let o = a.fmt_obj(&s.obj);
        if s.shift == 0 {
            format!("P(\\texttt{{{o}}})")
        } else {
            format!("P(\\texttt{{{o}}})\\langle {} \\rangle", s.shift)
        }
    };
    let ps: Vec<Pos> = {
        let mut v: Vec<Pos> = c.positions().into_iter().collect();
        if c.conv.step() < 0 {
            v.reverse();
        }
        v
    };
    let mut cells = Vec::new();
    for (k, p) in ps.iter().enumerate() {
        let term = c
            .at(p)
            .iter()
            .map(&tex_obj)
            .collect::<Vec<_>>()
            .join(" \\oplus ");
        let mut cell = format!("\\underset{{{}}}{{{}}}", fmt_pos(p), term);
        if let Some(q) = ps.get(k + 1)
            && *q == c.next(p, 0)
            && c.diffs.contains_key(&(0, p.clone()))
        {
            let m = c.diff(a, 0, p);
            let mut rows = Vec::new();
            for r in &m {
                let mut cs = Vec::new();
                for x in r {
                    cs.push(if a.is_zero(x)? {
                        "0".to_string()
                    } else {
                        format!("\\texttt{{{}}}", a.fmt_mor(x)?.replace('_', "\\_"))
                    });
                }
                rows.push(cs.join(" & "));
            }
            let label = if m.len() == 1 && m[0].len() == 1 {
                rows[0].clone()
            } else {
                format!(
                    "\\begin{{smallmatrix}} {} \\end{{smallmatrix}}",
                    rows.join(" \\\\ ")
                )
            };
            cell.push_str(&format!(" \\arrow[r, \"{label}\"]"));
        }
        cells.push(cell);
    }
    Ok(format!(
        "\\begin{{tikzcd}}\n  {}\n\\end{{tikzcd}}\n",
        cells.join("\n  & ")
    ))
}
