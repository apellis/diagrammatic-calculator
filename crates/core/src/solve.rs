//! Solving for unknown scalars: `lhs = rhs` where the unknowns are scalars of the context.
//!
//! The normal form of `lhs − rhs` is a combination of basis words whose coefficients are
//! polynomials in the unknowns; the equation holds exactly when every coefficient vanishes.
//!
//! * **Tier 1, linear:** every coefficient has degree at most 1 in the unknowns. The system is
//!   solved exactly by linear algebra over the ground field: no solution, one solution, or an
//!   affine family `c = p + Σ t_k v_k` with free parameters `t_k`.
//! * **Tier 2, polynomial:** otherwise the coefficients generate an ideal whose zero set is
//!   classified by Gröbner bases with explicit limits ([`crate::groebner`]): empty, finite (with
//!   the solutions in the ground field), a family of known dimension, or a limit reached.
//!
//! Every solution found is substituted back into the equations as an exact check.

use crate::context::Context;
use crate::elab::CTerm;
use crate::error::{Error, Result};
use crate::groebner::{self, Limits, Solution};
use crate::linalg::{self, Mat};
use crate::poly::{Monomial, Poly, PolyRing};
use crate::ring::Ring;
use serde_json::{Value, json};

/// A polynomial ring in the unknowns with a system of equations in it.
type System<R> = (PolyRing<R>, Vec<Poly<<R as Ring>::Elem>>);

/// The coefficient equations of a normal form, as polynomials in the unknowns only, with the
/// ring they live in. Other scalars may not occur.
fn equations<R: Ring>(ctx: &Context<R>, nf: &CTerm<R>, unknowns: &[String]) -> Result<System<R>> {
    let names = &ctx.coeff.names;
    let mut pos = Vec::with_capacity(unknowns.len());
    for u in unknowns {
        let k = names.iter().position(|n| n == u).ok_or_else(|| {
            Error::invalid(format!("the unknown {u} is not a scalar of the context"))
        })?;
        pos.push(k);
    }
    let ring = PolyRing::new(
        ctx.coeff.base.clone(),
        &unknowns.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let mut eqs = Vec::new();
    for (_, c) in &nf.terms {
        let mut terms = Vec::with_capacity(c.terms.len());
        for (m, a) in &c.terms {
            for (k, &e) in m.0.iter().enumerate() {
                if e != 0 && !pos.contains(&k) {
                    return Err(Error::precondition(format!(
                        "the scalar {} is not an unknown: list it after `for` or give it a value",
                        names[k]
                    )));
                }
            }
            let exps: Vec<i32> = pos.iter().map(|&k| m.0[k]).collect();
            terms.push((Monomial::from(exps), a.clone()));
        }
        eqs.push(ring.normalize(terms));
    }
    Ok((ring, eqs))
}

/// The value of a polynomial at a point of the ground field.
fn evaluate<R: Ring>(ring: &PolyRing<R>, p: &Poly<R::Elem>, point: &[R::Elem]) -> R::Elem {
    let f = &ring.base;
    let mut acc = f.zero();
    for (m, a) in &p.terms {
        let mut t = a.clone();
        for (k, &e) in m.0.iter().enumerate() {
            t = f.mul(&t, &f.pow(&point[k], e as u32));
        }
        acc = f.add(&acc, &t);
    }
    acc
}

/// Solves `nf = 0` for the unknowns, where `nf` is the normal form of `lhs − rhs`.
pub fn solve<R: Ring>(
    ctx: &Context<R>,
    nf: &CTerm<R>,
    unknowns: &[String],
    limits: &Limits,
) -> Result<Value> {
    if !ctx.coeff.base.is_field() {
        return Err(Error::precondition(format!(
            "solving needs a field as ground ring; here it is {}",
            ctx.spec.ring
        )));
    }
    let (ring, eqs) = equations(ctx, nf, unknowns)?;
    let f = &ring.base;
    let n = unknowns.len();
    let linear = eqs
        .iter()
        .all(|p| p.terms.iter().all(|(m, _)| m.degree() <= 1));
    let check = |point: &[R::Elem]| eqs.iter().all(|p| f.is_zero(&evaluate(&ring, p, point)));
    if linear {
        // A c = b with A the linear coefficients and b minus the constants.
        let mut rows = Vec::new();
        let mut rhs = Vec::new();
        for p in &eqs {
            let mut row = vec![f.zero(); n];
            let mut b = f.zero();
            for (m, a) in &p.terms {
                match m.0.iter().position(|&e| e == 1) {
                    Some(k) => row[k] = a.clone(),
                    None => b = f.neg(a),
                }
            }
            rows.push(row);
            rhs.push(b);
        }
        let assign = |v: &[R::Elem]| -> Value {
            unknowns
                .iter()
                .zip(v)
                .map(|(u, x)| (u.clone(), json!(f.fmt_elem(x))))
                .collect::<serde_json::Map<_, _>>()
                .into()
        };
        if rows.is_empty() {
            let zero = vec![f.zero(); n];
            return Ok(json!({
                "tier": 1, "outcome": "family", "dimension": n,
                "particular": assign(&zero),
                "directions": (0..n).map(|k| {
                    let mut v = vec![f.zero(); n];
                    v[k] = f.one();
                    assign(&v)
                }).collect::<Vec<_>>(),
                "equations": 0,
            }));
        }
        let a = Mat::from_rows(n, rows)?;
        return Ok(match linalg::solve(f, &a, &rhs)? {
            None => json!({ "tier": 1, "outcome": "none", "equations": eqs.len() }),
            Some((p, kernel)) => {
                if !check(&p) {
                    return Err(Error::invalid(
                        "internal: the linear solution does not satisfy the equations",
                    ));
                }
                if kernel.is_empty() {
                    json!({ "tier": 1, "outcome": "unique", "solution": assign(&p), "equations": eqs.len() })
                } else {
                    json!({
                        "tier": 1, "outcome": "family", "dimension": kernel.len(),
                        "particular": assign(&p),
                        "directions": kernel.iter().map(|v| assign(v)).collect::<Vec<_>>(),
                        "equations": eqs.len(),
                    })
                }
            }
        });
    }
    let solved = groebner::solve(&ring, &eqs, limits)?;
    if let Solution::Finite(s) = &solved.solution
        && !s.points.iter().all(|p| check(p))
    {
        return Err(Error::invalid(
            "internal: a solution does not satisfy the equations",
        ));
    }
    let mut v = serde_json::to_value(solved.report(&ring))
        .map_err(|e| Error::invalid(format!("internal: {e}")))?;
    if let Some(o) = v.as_object_mut()
        && let Some(k) = o.remove("kind")
    {
        o.insert("outcome".into(), k);
    }
    v["tier"] = json!(2);
    v["equations"] = json!(eqs.len());
    Ok(v)
}
