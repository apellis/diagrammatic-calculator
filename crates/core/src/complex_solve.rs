//! Linear problems about maps between finite direct sums (`docs/complexes.md`, phase M2):
//! factoring a map through another, solving for chain maps and homotopies, testing whether a
//! differential lies in the radical, and inverting a morphism.
//!
//! Every problem is a finite linear system. A degree-`e` map `⊕ X_j⟨s_j⟩ → ⊕ Y_i⟨t_i⟩` is a matrix
//! whose entry `(i, j)` lies in `Hom(X_j, Y_i)` of degree `s_j − t_i + e`, a finite-dimensional
//! space; a [`HomSpace`] is the basis of all such matrices. Unknown maps are vectors in their
//! spaces, and each equation `Σ ± L U R = B` is read in the space it lives in.

#![allow(clippy::type_complexity)]

use crate::complex::{
    Additive, Complex, Failure, Map, Matrix, Pos, Summand, fmt_pos, mat_add, mat_mul, zero_matrix,
};
use crate::error::{Error, Result};
use crate::linalg::{Mat, Reducer};
use crate::ring::Ring;
use std::collections::{BTreeMap, BTreeSet};

type E<A> = <<A as Additive>::Field as Ring>::Elem;

/// The space of degree-`degree` maps from `src` to `tgt`, with a basis of matrices with one
/// nonzero entry each.
pub struct HomSpace<O, M> {
    pub src: Vec<Summand<O>>,
    pub tgt: Vec<Summand<O>>,
    pub degree: i64,
    /// `(row, column, basis morphism)`, entry by entry in row-major order.
    pub basis: Vec<(usize, usize, M)>,
    /// `start[i][j]`: index of the first basis vector of entry `(i, j)`; `dims[i][j]` its count.
    start: Vec<Vec<usize>>,
    dims: Vec<Vec<usize>>,
}

impl<O, M> HomSpace<O, M> {
    pub fn dim(&self) -> usize {
        self.basis.len()
    }
}

/// The algebra degree of entry `(i, j)` of a degree-`e` map from `src` to `tgt`.
pub fn entry_degree<O>(src: &[Summand<O>], tgt: &[Summand<O>], i: usize, j: usize, e: i64) -> i64 {
    src[j].shift - tgt[i].shift + e
}

pub fn hom_space<A: Additive>(
    a: &A,
    src: &[Summand<A::Obj>],
    tgt: &[Summand<A::Obj>],
    e: i64,
) -> Result<HomSpace<A::Obj, A::Mor>> {
    let mut basis = Vec::new();
    let mut start = Vec::with_capacity(tgt.len());
    let mut dims = Vec::with_capacity(tgt.len());
    for i in 0..tgt.len() {
        let mut srow = Vec::with_capacity(src.len());
        let mut drow = Vec::with_capacity(src.len());
        for j in 0..src.len() {
            srow.push(basis.len());
            let d = entry_degree(src, tgt, i, j, e);
            let b = a.hom_basis(&src[j].obj, &tgt[i].obj, d)?;
            drow.push(b.len());
            basis.extend(b.into_iter().map(|f| (i, j, f)));
        }
        start.push(srow);
        dims.push(drow);
    }
    Ok(HomSpace {
        src: src.to_vec(),
        tgt: tgt.to_vec(),
        degree: e,
        basis,
        start,
        dims,
    })
}

/// The coordinates of a matrix in a [`HomSpace`]; an error if an entry does not lie there.
pub fn coords_of<A: Additive>(
    a: &A,
    hs: &HomSpace<A::Obj, A::Mor>,
    m: &Matrix<A::Mor>,
) -> Result<Vec<E<A>>> {
    let k = a.field();
    let mut v = vec![k.zero(); hs.dim()];
    for (i, row) in m.iter().enumerate() {
        for (j, f) in row.iter().enumerate() {
            if a.is_zero(f)? {
                continue;
            }
            let d = entry_degree(&hs.src, &hs.tgt, i, j, hs.degree);
            if hs.dims[i][j] == 0 {
                return Err(Error::invalid(format!(
                    "entry ({}, {}) is {}, but Hom({}, {}) is zero in degree {d}",
                    i + 1,
                    j + 1,
                    a.fmt_mor(f)?,
                    a.fmt_obj(&hs.src[j].obj),
                    a.fmt_obj(&hs.tgt[i].obj)
                )));
            }
            let c = a.coords(&hs.src[j].obj, &hs.tgt[i].obj, d, f)?;
            for (t, x) in c.into_iter().enumerate() {
                v[hs.start[i][j] + t] = x;
            }
        }
    }
    Ok(v)
}

/// The matrix with coordinates `v`.
pub fn matrix_of<A: Additive>(a: &A, hs: &HomSpace<A::Obj, A::Mor>, v: &[E<A>]) -> Matrix<A::Mor> {
    let k = a.field();
    let mut m = zero_matrix(a, hs.tgt.len(), hs.src.len());
    for (idx, (i, j, b)) in hs.basis.iter().enumerate() {
        if !k.is_zero(&v[idx]) {
            m[*i][*j] = a.add(&m[*i][*j], &a.scale(&v[idx], b));
        }
    }
    m
}

/// `sign · L U R` with `U` an unknown map; `left : tgt(U) → tgt(eq)` and `right : src(eq) →
/// src(U)`, `None` meaning the identity.
pub struct Term<M> {
    pub unknown: usize,
    pub sign: i64,
    pub left: Option<Matrix<M>>,
    pub right: Option<Matrix<M>>,
}

/// `Σ terms = rhs` in the space `space`.
pub struct Equation<O, M> {
    pub space: HomSpace<O, M>,
    pub terms: Vec<Term<M>>,
    pub rhs: Matrix<M>,
    /// Where the equation comes from, for messages.
    pub label: String,
}

/// The solutions of a system: one solution, and a basis of the solutions of the homogeneous
/// system.
pub struct Solved<M> {
    pub particular: Vec<Matrix<M>>,
    pub kernel: Vec<Vec<Matrix<M>>>,
}

/// `L · (b at (i, j)) · R`: rows of the equation's target, columns of its source.
fn spread<A: Additive>(
    a: &A,
    t: &Term<A::Mor>,
    i: usize,
    j: usize,
    b: &A::Mor,
    rows: usize,
    cols: usize,
) -> Matrix<A::Mor> {
    let mut out = zero_matrix(a, rows, cols);
    for (r, row) in out.iter_mut().enumerate() {
        let lb = match &t.left {
            None if r == i => b.clone(),
            None => continue,
            Some(l) => {
                if a.is_zero(&l[r][i]).unwrap_or(false) {
                    continue;
                }
                a.compose(&l[r][i], b)
            }
        };
        for (c, x) in row.iter_mut().enumerate() {
            *x = match &t.right {
                None if c == j => lb.clone(),
                None => continue,
                Some(rm) => a.compose(&lb, &rm[j][c]),
            };
        }
    }
    if t.sign < 0 {
        for row in &mut out {
            for x in row.iter_mut() {
                *x = a.neg(x);
            }
        }
    }
    out
}

/// Solves a system of equations in unknown maps.
pub fn solve_system<A: Additive>(
    a: &A,
    unknowns: &[HomSpace<A::Obj, A::Mor>],
    eqs: &[Equation<A::Obj, A::Mor>],
) -> Result<Option<Solved<A::Mor>>> {
    let k = a.field().clone();
    let offs: Vec<usize> = unknowns
        .iter()
        .scan(0, |acc, u| {
            let o = *acc;
            *acc += u.dim();
            Some(o)
        })
        .collect();
    let n: usize = unknowns.iter().map(HomSpace::dim).sum();
    let mut rows: Vec<Vec<E<A>>> = Vec::new();
    let mut rhs: Vec<E<A>> = Vec::new();
    for eq in eqs {
        let m = eq.space.dim();
        let (r, c) = (eq.space.tgt.len(), eq.space.src.len());
        let b = coords_of(a, &eq.space, &eq.rhs)
            .map_err(|e| Error::invalid(format!("{}: {}", eq.label, e.message)))?;
        let mut block = vec![vec![k.zero(); n]; m];
        for t in &eq.terms {
            let u = &unknowns[t.unknown];
            for (idx, (i, j, bm)) in u.basis.iter().enumerate() {
                let img = spread(a, t, *i, *j, bm, r, c);
                let v = coords_of(a, &eq.space, &img).map_err(|e| {
                    Error::invalid(format!("internal: {}: {}", eq.label, e.message))
                })?;
                for (row, x) in v.into_iter().enumerate() {
                    let col = offs[t.unknown] + idx;
                    block[row][col] = k.add(&block[row][col], &x);
                }
            }
        }
        rows.extend(block);
        rhs.extend(b);
    }
    let unpack = |v: &[E<A>]| -> Vec<Matrix<A::Mor>> {
        unknowns
            .iter()
            .zip(&offs)
            .map(|(u, o)| matrix_of(a, u, &v[*o..*o + u.dim()]))
            .collect()
    };
    let (x, ker) = if n == 0 {
        if rhs.iter().any(|x| !k.is_zero(x)) {
            return Ok(None);
        }
        (vec![], vec![])
    } else if rows.is_empty() {
        let basis = (0..n)
            .map(|i| {
                let mut v = vec![k.zero(); n];
                v[i] = k.one();
                v
            })
            .collect();
        (vec![k.zero(); n], basis)
    } else {
        let mat = Mat::from_rows(n, rows)?;
        match crate::linalg::solve(&k, &mat, &rhs)? {
            None => return Ok(None),
            Some(s) => s,
        }
    };
    Ok(Some(Solved {
        particular: unpack(&x),
        kernel: ker.iter().map(|v| unpack(v)).collect(),
    }))
}

/// Finds `h : X → Y` with `g h = f`, for `f : X → Z` and `g : Y → Z` (lifting along `g`).
pub fn lift<A: Additive>(
    a: &A,
    x: &[Summand<A::Obj>],
    y: &[Summand<A::Obj>],
    z: &[Summand<A::Obj>],
    f: &Matrix<A::Mor>,
    g: &Matrix<A::Mor>,
) -> Result<Option<Solved<A::Mor>>> {
    let u = hom_space(a, x, y, 0)?;
    let eq = Equation {
        space: hom_space(a, x, z, 0)?,
        terms: vec![Term {
            unknown: 0,
            sign: 1,
            left: Some(g.clone()),
            right: None,
        }],
        rhs: f.clone(),
        label: "g h = f".into(),
    };
    solve_system(a, &[u], &[eq])
}

/// Finds `h : Y → Z` with `h g = f`, for `f : X → Z` and `g : X → Y` (extending along `g`).
pub fn extend<A: Additive>(
    a: &A,
    x: &[Summand<A::Obj>],
    y: &[Summand<A::Obj>],
    z: &[Summand<A::Obj>],
    f: &Matrix<A::Mor>,
    g: &Matrix<A::Mor>,
) -> Result<Option<Solved<A::Mor>>> {
    let u = hom_space(a, y, z, 0)?;
    let eq = Equation {
        space: hom_space(a, x, z, 0)?,
        terms: vec![Term {
            unknown: 0,
            sign: 1,
            left: None,
            right: Some(g.clone()),
        }],
        rhs: f.clone(),
        label: "h g = f".into(),
    };
    solve_system(a, &[u], &[eq])
}

/// The positions where a map between `c` and `d` with offset `offset` can be nonzero.
fn map_positions<O: Clone, M: Clone>(
    c: &Complex<O, M>,
    d: &Complex<O, M>,
    offset: &[i64],
) -> Vec<Pos> {
    c.positions()
        .into_iter()
        .filter(|p| {
            let q: Pos = p.iter().zip(offset).map(|(x, o)| x + o).collect();
            !c.at(p).is_empty() && !d.at(&q).is_empty()
        })
        .collect()
}

/// The chain maps `F : C → D` extending the given components `fixed`: unknown components at
/// every other position where both complexes are nonzero.
pub fn solve_chain_map<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    d: &Complex<A::Obj, A::Mor>,
    fixed: &Map<A::Mor>,
) -> Result<(Vec<Pos>, Option<Solved<A::Mor>>)> {
    let zero = vec![0; c.dim];
    let free: Vec<Pos> = map_positions(c, d, &zero)
        .into_iter()
        .filter(|p| !fixed.contains_key(p))
        .collect();
    let index: BTreeMap<&Pos, usize> = free.iter().enumerate().map(|(i, p)| (p, i)).collect();
    let unknowns: Vec<_> = free
        .iter()
        .map(|p| hom_space(a, c.at(p), d.at(p), 0))
        .collect::<Result<_>>()?;
    let mut eqs = Vec::new();
    let mut ps: BTreeSet<Pos> = c.positions();
    ps.extend(d.positions());
    for p in &ps {
        for axis in 0..c.dim {
            let q = c.next(p, axis);
            let (cp, dq) = (c.at(p), d.at(&q));
            if cp.is_empty() || dq.is_empty() {
                continue;
            }
            // d_D(p) F(p) − F(q) d_C(p) = 0, as maps C(p) → D(q)
            let mut terms = Vec::new();
            let mut rhs = zero_matrix(a, dq.len(), cp.len());
            let dd = d.diff(a, axis, p);
            let dc = c.diff(a, axis, p);
            if let Some(&u) = index.get(p) {
                terms.push(Term {
                    unknown: u,
                    sign: 1,
                    left: Some(dd.clone()),
                    right: None,
                });
            } else if let Some(fp) = fixed.get(p) {
                let x = mat_mul(a, &dd, fp, d.at(p).len(), cp.len());
                rhs = mat_add(a, &rhs, &x, -1);
            }
            if let Some(&u) = index.get(&q) {
                terms.push(Term {
                    unknown: u,
                    sign: -1,
                    left: None,
                    right: Some(dc.clone()),
                });
            } else if let Some(fq) = fixed.get(&q) {
                let y = mat_mul(a, fq, &dc, c.at(&q).len(), cp.len());
                rhs = mat_add(a, &rhs, &y, 1);
            }
            eqs.push(Equation {
                space: hom_space(a, cp, dq, 0)?,
                terms,
                rhs,
                label: format!("d F − F d from {}", fmt_pos(p)),
            });
        }
    }
    Ok((free.clone(), solve_system(a, &unknowns, &eqs)?))
}

/// The homotopies `h` with `f − g = d h + h d`, for maps of one-dimensional complexes;
/// `h(p) : C(p) → D(p − step)`.
pub fn find_homotopy<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    d: &Complex<A::Obj, A::Mor>,
    fg: &Map<A::Mor>,
) -> Result<(Vec<Pos>, Option<Solved<A::Mor>>)> {
    if c.dim != 1 {
        return Err(Error::precondition(
            "homotopies are found for one-dimensional complexes; totalize first",
        ));
    }
    let back = vec![-c.conv.step()];
    let free = map_positions(c, d, &back);
    let index: BTreeMap<&Pos, usize> = free.iter().enumerate().map(|(i, p)| (p, i)).collect();
    let unknowns: Vec<_> = free
        .iter()
        .map(|p| hom_space(a, c.at(p), d.at(&c.prev(p, 0)), 0))
        .collect::<Result<_>>()?;
    let mut eqs = Vec::new();
    let mut ps: BTreeSet<Pos> = c.positions();
    ps.extend(d.positions());
    for p in &ps {
        let (cp, dp) = (c.at(p), d.at(p));
        if cp.is_empty() || dp.is_empty() {
            continue;
        }
        let (bk, fw) = (c.prev(p, 0), c.next(p, 0));
        let mut terms = Vec::new();
        if let Some(&u) = index.get(p) {
            terms.push(Term {
                unknown: u,
                sign: 1,
                left: Some(d.diff(a, 0, &bk)),
                right: None,
            });
        }
        if let Some(&u) = index.get(&fw) {
            terms.push(Term {
                unknown: u,
                sign: 1,
                left: None,
                right: Some(c.diff(a, 0, p)),
            });
        }
        eqs.push(Equation {
            space: hom_space(a, cp, dp, 0)?,
            terms,
            rhs: fg
                .get(p)
                .cloned()
                .unwrap_or_else(|| zero_matrix(a, dp.len(), cp.len())),
            label: format!("d h + h d = f − g at {}", fmt_pos(p)),
        });
    }
    Ok((free.clone(), solve_system(a, &unknowns, &eqs)?))
}

/// Whether `m : X → Y` (degree zero) lies in the radical of the category: whether the left ideal
/// `L = {g m : g ∈ Hom(Y, X)}` of `End(X)` is nilpotent (a nil left ideal of a finite-dimensional
/// algebra is nilpotent, so this is the same as `L ⊆ J(End X)`). Returns `None` if it does, else
/// `Some(g)` with `g` a map back such that `g m` is not nilpotent, when one of the tried maps
/// (basis maps and partial sums) is such.
pub fn radical_witness<A: Additive>(
    a: &A,
    x: &[Summand<A::Obj>],
    y: &[Summand<A::Obj>],
    m: &Matrix<A::Mor>,
) -> Result<Option<Option<Matrix<A::Mor>>>> {
    let k = a.field().clone();
    let back = hom_space(a, y, x, 0)?;
    if back.dim() == 0 {
        return Ok(None);
    }
    let end = hom_space(a, x, x, 0)?;
    let n = end.dim();
    let (nx, ny) = (x.len(), y.len());
    // a basis of L = span{g m}, with the g that produce it
    let mut red = Reducer::new(k.clone(), n);
    let mut gens: Vec<(Matrix<A::Mor>, Matrix<A::Mor>)> = Vec::new();
    for (i, j, b) in &back.basis {
        let mut g = zero_matrix(a, nx, ny);
        g[*i][*j] = b.clone();
        let gm = mat_mul(a, &g, m, ny, nx);
        if red.insert(coords_of(a, &end, &gm)?) {
            gens.push((g, gm));
        }
    }
    if gens.is_empty() {
        return Ok(None);
    }
    // L ⊇ L² ⊇ L³ ⊇ …; L is nilpotent iff the chain reaches 0. Track a generator of a
    // non-nilpotent part through the powers.
    let lgens: Vec<Matrix<A::Mor>> = gens.iter().map(|(_, gm)| gm.clone()).collect();
    let mut power = lgens.clone();
    let mut dim = power.len();
    loop {
        let mut red = Reducer::new(k.clone(), n);
        let mut next = Vec::new();
        for l in &lgens {
            for s in &power {
                let p = mat_mul(a, l, s, nx, nx);
                if red.insert(coords_of(a, &end, &p)?) {
                    next.push(p);
                }
            }
        }
        if next.is_empty() {
            return Ok(None);
        }
        if next.len() == dim {
            break;
        }
        dim = next.len();
        power = next;
    }
    // L is not nil: some generator g m is not nilpotent (a left ideal spanned by nilpotents
    // that is not nilpotent still has a non-nilpotent element; search the generators and their
    // sums, else report the first generator of L)
    for (g, gm) in &gens {
        if !is_nilpotent(a, &end, gm, nx)? {
            return Ok(Some(Some(g.clone())));
        }
    }
    let mut sum_g = zero_matrix(a, nx, ny);
    let mut sum_gm = zero_matrix(a, nx, nx);
    for (g, gm) in &gens {
        sum_g = mat_add(a, &sum_g, g, 1);
        sum_gm = mat_add(a, &sum_gm, gm, 1);
        if !is_nilpotent(a, &end, &sum_gm, nx)? {
            return Ok(Some(Some(sum_g)));
        }
    }
    Ok(Some(None))
}

fn is_nilpotent<A: Additive>(
    a: &A,
    end: &HomSpace<A::Obj, A::Mor>,
    x: &Matrix<A::Mor>,
    n: usize,
) -> Result<bool> {
    let k = a.field().clone();
    let mut p = x.clone();
    for _ in 0..=end.dim() {
        let v = coords_of(a, end, &p)?;
        if v.iter().all(|c| k.is_zero(c)) {
            return Ok(true);
        }
        p = mat_mul(a, &p, x, n, n);
    }
    Ok(false)
}

/// The inverse of an isomorphism `f : X → Y` (degree zero), or `None` if `f` is not invertible.
pub fn inverse<A: Additive>(
    a: &A,
    x: &[Summand<A::Obj>],
    y: &[Summand<A::Obj>],
    f: &Matrix<A::Mor>,
) -> Result<Option<Matrix<A::Mor>>> {
    let u = hom_space(a, y, x, 0)?;
    if u.dim() == 0 {
        return Ok(None);
    }
    let id = |ss: &[Summand<A::Obj>]| -> Matrix<A::Mor> {
        let mut m = zero_matrix(a, ss.len(), ss.len());
        for (i, s) in ss.iter().enumerate() {
            m[i][i] = a.identity(&s.obj);
        }
        m
    };
    let eqs = vec![
        Equation {
            space: hom_space(a, x, x, 0)?,
            terms: vec![Term {
                unknown: 0,
                sign: 1,
                left: None,
                right: Some(f.clone()),
            }],
            rhs: id(x),
            label: "g f = 1".into(),
        },
        Equation {
            space: hom_space(a, y, y, 0)?,
            terms: vec![Term {
                unknown: 0,
                sign: 1,
                left: Some(f.clone()),
                right: None,
            }],
            rhs: id(y),
            label: "f g = 1".into(),
        },
    ];
    Ok(solve_system(a, &[u], &eqs)?.map(|s| s.particular.into_iter().next().unwrap()))
}

/// Whether every differential of `c` lies in the radical. Failures name a differential and a
/// map back that makes a non-nilpotent endomorphism.
pub fn check_minimal<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
) -> Result<Vec<(Failure, Option<Matrix<A::Mor>>)>> {
    let mut out = Vec::new();
    for p in c.positions() {
        for axis in 0..c.dim {
            let q = c.next(&p, axis);
            let (x, y) = (c.at(&p), c.at(&q));
            if y.is_empty() {
                continue;
            }
            let m = c.diff(a, axis, &p);
            if let Some(g) = radical_witness(a, x, y, &m)? {
                let along = if c.dim == 1 {
                    String::new()
                } else {
                    format!(" along axis {axis}")
                };
                out.push((
                    Failure {
                        condition: "minimal",
                        location: format!("the differential from {}{along}", fmt_pos(&p)),
                        detail: "it is not in the radical: the maps g d, for g a map back, \
                                 span a left ideal of End that is not nilpotent, so the complex \
                                 has a contractible summand there"
                            .into(),
                    },
                    g,
                ));
            }
        }
    }
    Ok(out)
}
