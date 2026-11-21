//! Deforming a complex order by order in a parameter `h` (`docs/complexes.md`, "Parameters and
//! deformations").
//!
//! A one-dimensional complex over `k[h]/(h^{N+1})` has differential `d = Σ_a h^a d_a` with every
//! `d_a` free of `h`. Some entries of the differential are declared unknown; the solver adds to
//! them corrections `h^m X_m`, `1 ≤ m ≤ N`, so that `d² = 0`. The coefficient of `h^m` in `d²` is
//! `Σ_{a+b=m} d_b d_a`, and the only terms in it that involve `X_m` are `d_0 X_m + X_m d_0`. So,
//! once the orders below `m` are fixed, order `m` is the linear system
//!
//! ```text
//!     d_0 X_m + X_m d_0 = − Σ_{a+b=m} d_b d_a        (d_m: the given part only)
//! ```
//!
//! in the `h`-free category, with `X_m` restricted to the unknown entries. Its solutions are a
//! particular one plus the kernel (the freedom at order `m`); when there is none, the right side
//! is the obstruction, reported at the first position where the system becomes inconsistent.
//! This is the order-by-order solution of the Maurer–Cartan equation used for iterated cones and
//! for deformations of differentials.

use crate::complex::{
    Complex, Failure, Map, Matrix, Pos, diff_ref, fmt_pos, mat_add, mat_mul, zero_matrix,
};
use crate::complex_solve::{Equation, Term, hom_space, masked_hom_space, solve_system};
use crate::deform::{ParamSplit, Truncated};
use crate::error::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};

/// The entries of the differential from each position that may be corrected.
pub type Mask = BTreeMap<Pos, BTreeSet<(usize, usize)>>;

/// What one order contributes.
pub struct Order<M> {
    pub order: u32,
    /// `X_m` by source position (free of `h`).
    pub correction: Map<M>,
    /// A basis of the solutions of the homogeneous system: other choices of `X_m` differ from
    /// `correction` by a combination of these.
    pub freedom: Vec<Map<M>>,
}

pub struct Deformed<M> {
    /// The parts `d_a`, `0 ≤ a ≤ N`, of the corrected differential, by source position, up to
    /// the last order solved.
    pub parts: BTreeMap<Pos, Vec<Matrix<M>>>,
    pub orders: Vec<Order<M>>,
    /// The order at which the system has no solution, and where.
    pub obstruction: Option<(u32, Failure)>,
}

fn split_matrix<A: ParamSplit>(
    t: &Truncated<'_, A>,
    m: &Matrix<A::Mor>,
    rows: usize,
    cols: usize,
) -> Result<Vec<Matrix<A::Mor>>> {
    let a = t.a;
    let n = t.order as usize + 1;
    let mut out = vec![zero_matrix(a, rows, cols); n];
    for (i, row) in m.iter().enumerate() {
        for (j, f) in row.iter().enumerate() {
            for (k, p) in t.parts(f)?.into_iter().enumerate() {
                out[k][i][j] = p;
            }
        }
    }
    Ok(out)
}

/// The corrected differential `Σ_a h^a d_a`, as a complex over `k[h]/(h^{N+1})`.
pub fn assemble<A: ParamSplit>(
    t: &Truncated<'_, A>,
    c: &Complex<A::Obj, A::Mor>,
    parts: &BTreeMap<Pos, Vec<Matrix<A::Mor>>>,
) -> Result<Complex<A::Obj, A::Mor>> {
    let mut diffs = BTreeMap::new();
    for (p, ms) in parts {
        let (rows, cols) = (c.at(&c.next(p, 0)).len(), c.at(p).len());
        let mut m = zero_matrix(t.a, rows, cols);
        for (i, row) in m.iter_mut().enumerate() {
            for (j, x) in row.iter_mut().enumerate() {
                let entry: Vec<A::Mor> = ms.iter().map(|mm| mm[i][j].clone()).collect();
                *x = t.assemble(&entry)?;
            }
        }
        diffs.insert((0, p.clone()), m);
    }
    Ok(Complex {
        dim: 1,
        conv: c.conv,
        terms: c.terms.clone(),
        diffs,
    })
}

/// `h^m X` entrywise, as maps over `k[h]/(h^{N+1})`.
pub fn lift_map<A: ParamSplit>(
    t: &Truncated<'_, A>,
    m: u32,
    x: &Map<A::Mor>,
) -> Result<Map<A::Mor>> {
    x.iter()
        .map(|(p, mat)| {
            let lifted = mat
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|f| t.a.param_mul(&t.param, m, f))
                        .collect::<Result<Vec<_>>>()
                })
                .collect::<Result<Vec<_>>>()?;
            Ok((p.clone(), lifted))
        })
        .collect()
}

/// Solves for the corrections order by order (see the module documentation).
pub fn deform<A: ParamSplit>(
    t: &Truncated<'_, A>,
    c: &Complex<A::Obj, A::Mor>,
    mask: &Mask,
) -> Result<Deformed<A::Mor>> {
    let a = t.a;
    if c.dim != 1 {
        return Err(Error::precondition(
            "deformations are solved for one-dimensional complexes; totalize first",
        ));
    }
    let n = t.order as usize;
    let delta = t.degree;
    // the parts of every differential between nonzero terms
    let mut parts: BTreeMap<Pos, Vec<Matrix<A::Mor>>> = BTreeMap::new();
    for p in c.positions() {
        let q = c.next(&p, 0);
        if c.at(&q).is_empty() {
            continue;
        }
        let (rows, cols) = (c.at(&q).len(), c.at(&p).len());
        parts.insert(p.clone(), split_matrix(t, &c.diff(a, 0, &p), rows, cols)?);
    }
    for p in mask.keys() {
        if !parts.contains_key(p) {
            return Err(Error::invalid(format!(
                "an unknown is given in the differential from {}, but the complex has no \
                 differential there (no terms at both ends)",
                fmt_pos(p)
            )));
        }
    }
    for (p, ents) in mask {
        let (rows, cols) = (c.at(&c.next(p, 0)).len(), c.at(p).len());
        for &(i, j) in ents {
            if i >= rows || j >= cols {
                return Err(Error::invalid(format!(
                    "the differential from {} has {rows} rows and {cols} columns: there is no \
                     entry (row {i}, col {j}) (rows and columns are numbered from 0)",
                    fmt_pos(p)
                )));
            }
        }
    }
    // order 0 must be a complex
    let squares: Vec<(Pos, Pos, Pos)> = parts
        .keys()
        .filter_map(|p| {
            let q = c.next(p, 0);
            let r = c.next(&q, 0);
            (parts.contains_key(&q) && !c.at(&r).is_empty()).then(|| (p.clone(), q, r))
        })
        .collect();
    let mut out = Deformed {
        parts,
        orders: Vec::new(),
        obstruction: None,
    };
    for (p, q, r) in &squares {
        let dd = mat_mul(
            a,
            &out.parts[q][0],
            &out.parts[p][0],
            c.at(r).len(),
            c.at(p).len(),
        );
        if dd.iter().flatten().any(|f| !a.is_zero(f).unwrap_or(true)) {
            out.obstruction = Some((
                0,
                Failure {
                    condition: "d^2",
                    location: format!(
                        "d ∘ d from {} at order 0: the h-free part is not a complex",
                        fmt_pos(p)
                    ),
                    detail: "the corrections start at order 1, so the differential with h set \
                             to 0 must already square to zero"
                        .into(),
                    maps: vec![diff_ref(p, 0), diff_ref(q, 0)],
                },
            ));
            return Ok(out);
        }
    }
    let free: Vec<Pos> = mask
        .iter()
        .filter(|(_, e)| !e.is_empty())
        .map(|(p, _)| p.clone())
        .collect();
    let index: BTreeMap<&Pos, usize> = free.iter().enumerate().map(|(i, p)| (p, i)).collect();
    for m in 1..=n {
        let e = -(m as i64) * delta;
        let unknowns = free
            .iter()
            .map(|p| {
                let ents = &mask[p];
                masked_hom_space(a, c.at(p), c.at(&c.next(p, 0)), e, &|i, j| {
                    ents.contains(&(i, j))
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut eqs = Vec::new();
        let mut eq_at = Vec::new();
        for (p, q, r) in &squares {
            let (np, nr) = (c.at(p).len(), c.at(r).len());
            let mut rhs = zero_matrix(a, nr, np);
            for x in 0..=m {
                let dd = mat_mul(a, &out.parts[q][m - x], &out.parts[p][x], nr, np);
                rhs = mat_add(a, &rhs, &dd, -1);
            }
            let mut terms = Vec::new();
            if let Some(&u) = index.get(p) {
                terms.push(Term {
                    unknown: u,
                    sign: 1,
                    left: Some(out.parts[q][0].clone()),
                    right: None,
                });
            }
            if let Some(&u) = index.get(q) {
                terms.push(Term {
                    unknown: u,
                    sign: 1,
                    left: None,
                    right: Some(out.parts[p][0].clone()),
                });
            }
            eqs.push(Equation {
                space: hom_space(a, c.at(p), c.at(r), e)?,
                terms,
                rhs,
                label: format!("d ∘ d from {} at order {m}", fmt_pos(p)),
            });
            eq_at.push((p.clone(), q.clone()));
        }
        match solve_system(a, &unknowns, &eqs)? {
            Some(s) => {
                let mut correction = Map::new();
                for (u, p) in free.iter().enumerate() {
                    let x = &s.particular[u];
                    let part = &mut out.parts.get_mut(p).expect("checked above")[m];
                    *part = mat_add(a, part, x, 1);
                    correction.insert(p.clone(), x.clone());
                }
                let freedom = s
                    .kernel
                    .iter()
                    .map(|v| free.iter().cloned().zip(v.iter().cloned()).collect())
                    .collect();
                out.orders.push(Order {
                    order: m as u32,
                    correction,
                    freedom,
                });
            }
            None => {
                // the first equation, in order of positions, that cannot be met with the ones
                // before it
                let mut at = eqs.len() - 1;
                for k in 0..eqs.len() {
                    if solve_system(a, &unknowns, &eqs[..=k])?.is_none() {
                        at = k;
                        break;
                    }
                }
                let (p, q) = &eq_at[at];
                let rhs = &eqs[at].rhs;
                let detail = rhs
                    .iter()
                    .flatten()
                    .find(|f| !a.is_zero(f).unwrap_or(true))
                    .map(|f| t.a.fmt_mor(&a.neg(f)))
                    .transpose()?
                    .unwrap_or_else(|| "0".into());
                out.obstruction = Some((
                    m as u32,
                    Failure {
                        condition: "obstruction",
                        location: format!(
                            "d ∘ d from {} at order {m}: no choice of the unknown entries at \
                             this order makes it vanish",
                            fmt_pos(p)
                        ),
                        detail: format!(
                            "the coefficient of {}^{m} in d ∘ d (with the unknowns zero) has the \
                             entry {detail}; it is not of the form d_0 X + X d_0",
                            t.param
                        ),
                        maps: vec![diff_ref(p, 0), diff_ref(q, 0)],
                    },
                ));
                return Ok(out);
            }
        }
    }
    Ok(out)
}
