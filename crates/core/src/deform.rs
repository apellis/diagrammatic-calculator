//! Coefficients in `k[h]/(h^{N+1})`: a parameter worked order by order (`docs/complexes.md`,
//! "Parameters and deformations").
//!
//! A category whose morphisms may carry a scalar `h` in their coefficients enters through
//! [`ParamSplit`]: it writes a morphism as `Σ_j h^j f_j` with every `f_j` free of `h`. Then
//! [`Truncated`] is the same category with morphisms taken modulo `h^{N+1}`. Its Hom spaces are
//! finite-dimensional over the field `k`: the degree-`d` part of `Hom(X, Y)` has the basis
//! `h^j b`, `0 ≤ j ≤ N`, `b` in a basis of the degree-`(d − jδ)` part of the `h`-free Hom space,
//! where `δ` is the degree given to `h`. Every operation on complexes therefore works over
//! `k[h]/(h^{N+1})` unchanged, with dimensions counted over `k`.

use crate::complex::{Additive, MorEnds};
use crate::error::{Error, Result};
use crate::ring::Ring;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// A category in which a scalar `h` can be split off the coefficients of morphisms.
pub trait ParamSplit: Additive {
    /// The parts `f_j` of `f = Σ_j h^j f_j`, each free of `h`, indexed by `j` (trailing zero
    /// parts may be left out). An error if `h` is not a scalar of the category, or if `f` has a
    /// negative power of `h`.
    fn param_split(&self, h: &str, f: &Self::Mor) -> Result<Vec<Self::Mor>>;
    /// `h^j f`.
    fn param_mul(&self, h: &str, j: u32, f: &Self::Mor) -> Result<Self::Mor>;
}

/// The category `A` with coefficients modulo `h^{N+1}`, `h` of internal degree `δ`.
pub struct Truncated<'a, A: ParamSplit> {
    pub a: &'a A,
    pub param: String,
    pub order: u32,
    pub degree: i64,
}

impl<'a, A: ParamSplit> Truncated<'a, A> {
    pub fn new(a: &'a A, param: &str, order: u32, degree: i64) -> Result<Self> {
        let t = Truncated {
            a,
            param: param.to_string(),
            order,
            degree,
        };
        // fails here, not later, if h is not a scalar of the category
        t.a.param_split(&t.param, &t.a.zero())?;
        Ok(t)
    }

    /// The parts `f_0, …, f_N` (missing ones are zero).
    pub fn parts(&self, f: &A::Mor) -> Result<Vec<A::Mor>> {
        let mut p = self.a.param_split(&self.param, f)?;
        p.truncate(self.order as usize + 1);
        while p.len() <= self.order as usize {
            p.push(self.a.zero());
        }
        Ok(p)
    }

    /// `Σ_{j ≤ N} h^j f_j`.
    pub fn assemble(&self, parts: &[A::Mor]) -> Result<A::Mor> {
        let mut out = self.a.zero();
        for (j, p) in parts.iter().enumerate().take(self.order as usize + 1) {
            if !self.a.is_zero(p)? {
                out = self
                    .a
                    .add(&out, &self.a.param_mul(&self.param, j as u32, p)?);
            }
        }
        Ok(out)
    }

    /// `f` modulo `h^{N+1}`.
    pub fn reduce(&self, f: &A::Mor) -> Result<A::Mor> {
        self.assemble(&self.parts(f)?)
    }

    fn shift_of(&self, j: usize) -> i64 {
        j as i64 * self.degree
    }

    fn mixed(&self, degs: &BTreeSet<i64>) -> Error {
        Error::invalid(format!(
            "the term is not homogeneous: counting {} in degree {}, it has summands of degrees {}",
            self.param,
            self.degree,
            degs.iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

impl<A: ParamSplit> Additive for Truncated<'_, A> {
    type Field = A::Field;
    type Obj = A::Obj;
    type Key = A::Key;
    type Mor = A::Mor;

    fn field(&self) -> &Self::Field {
        self.a.field()
    }

    fn parse_obj(&self, s: &str) -> Result<Self::Obj> {
        self.a.parse_obj(s)
    }

    fn fmt_obj(&self, o: &Self::Obj) -> String {
        self.a.fmt_obj(o)
    }

    fn key(&self, o: &Self::Obj) -> Self::Key {
        self.a.key(o)
    }

    fn parse_mor(&self, s: &str) -> Result<Self::Mor> {
        self.reduce(&self.a.parse_mor(s)?)
    }

    fn fmt_mor(&self, f: &Self::Mor) -> Result<String> {
        self.a.fmt_mor(&self.reduce(f)?)
    }

    fn zero(&self) -> Self::Mor {
        self.a.zero()
    }

    fn identity(&self, o: &Self::Obj) -> Self::Mor {
        self.a.identity(o)
    }

    fn add(&self, f: &Self::Mor, g: &Self::Mor) -> Self::Mor {
        self.a.add(f, g)
    }

    fn neg(&self, f: &Self::Mor) -> Self::Mor {
        self.a.neg(f)
    }

    fn scale(&self, c: &<Self::Field as Ring>::Elem, f: &Self::Mor) -> Self::Mor {
        self.a.scale(c, f)
    }

    fn compose(&self, g: &Self::Mor, f: &Self::Mor) -> Self::Mor {
        let gf = self.a.compose(g, f);
        // a composite whose split fails (it cannot, for terms built in this category) is kept
        // whole: the next test for zero reports the error
        self.reduce(&gf).unwrap_or(gf)
    }

    fn is_zero(&self, f: &Self::Mor) -> Result<bool> {
        for p in self.parts(f)? {
            if !self.a.is_zero(&p)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn mor_degree(&self, src: &Self::Obj, tgt: &Self::Obj, f: &Self::Mor) -> Result<Option<i64>> {
        let mut degs = BTreeSet::new();
        for (j, p) in self.parts(f)?.iter().enumerate() {
            if let Some(d) = self.a.mor_degree(src, tgt, p)? {
                degs.insert(d + self.shift_of(j));
            }
        }
        match degs.len() {
            0 => Ok(None),
            1 => Ok(degs.into_iter().next()),
            _ => Err(self.mixed(&degs)),
        }
    }

    fn mor_ends(&self, f: &Self::Mor) -> Result<Option<MorEnds<Self::Obj>>> {
        let mut found: Option<MorEnds<Self::Obj>> = None;
        let mut degs = BTreeSet::new();
        for (j, p) in self.parts(f)?.iter().enumerate() {
            let Some((s, t, d)) = self.a.mor_ends(p)? else {
                continue;
            };
            let d = d + self.shift_of(j);
            degs.insert(d);
            match &found {
                None => found = Some((s, t, d)),
                Some((s0, t0, _)) => {
                    if self.a.key(s0) != self.a.key(&s) || self.a.key(t0) != self.a.key(&t) {
                        return Err(Error::invalid(format!(
                            "the term is not a map between two objects: it has summands from \
                             {} to {} and from {} to {}",
                            self.a.fmt_obj(s0),
                            self.a.fmt_obj(t0),
                            self.a.fmt_obj(&s),
                            self.a.fmt_obj(&t)
                        )));
                    }
                }
            }
        }
        if degs.len() > 1 {
            return Err(self.mixed(&degs));
        }
        Ok(found)
    }

    fn hom_degrees(&self, src: &Self::Obj, tgt: &Self::Obj) -> Result<Option<(i64, Option<i64>)>> {
        let Some((lo, hi)) = self.a.hom_degrees(src, tgt)? else {
            return Ok(None);
        };
        let span = self.shift_of(self.order as usize);
        Ok(Some((lo + span.min(0), hi.map(|h| h + span.max(0)))))
    }

    fn hom_basis(&self, src: &Self::Obj, tgt: &Self::Obj, d: i64) -> Result<Vec<Self::Mor>> {
        let mut out = Vec::new();
        for j in 0..=self.order {
            for b in self.a.hom_basis(src, tgt, d - self.shift_of(j as usize))? {
                out.push(self.a.param_mul(&self.param, j, &b)?);
            }
        }
        Ok(out)
    }

    fn coords(
        &self,
        src: &Self::Obj,
        tgt: &Self::Obj,
        d: i64,
        f: &Self::Mor,
    ) -> Result<Vec<<Self::Field as Ring>::Elem>> {
        let mut out = Vec::new();
        for (j, p) in self.parts(f)?.iter().enumerate() {
            let e = d - self.shift_of(j);
            if self.a.is_zero(p)? {
                let n = self.a.hom_basis(src, tgt, e)?.len();
                out.extend(std::iter::repeat_n(self.field().zero(), n));
            } else {
                out.extend(self.a.coords(src, tgt, e, p).map_err(|err| Error {
                    message: format!("in the coefficient of {}^{j}: {}", self.param, err.message),
                    ..err
                })?);
            }
        }
        Ok(out)
    }

    fn test_objects(&self, objs: &[Self::Obj]) -> Result<Vec<Self::Obj>> {
        self.a.test_objects(objs)
    }

    fn linear_algebra(&self) -> Result<()> {
        self.a.linear_algebra()
    }

    fn provenance(&self) -> Value {
        let mut p = self.a.provenance();
        let t = json!({
            "param": self.param,
            "order": self.order,
            "degree": self.degree,
            "note": format!(
                "coefficients in k[{h}]/({h}^{n}), {h} of degree {d}: terms with {h}^{n} or higher \
                 are dropped, and dimensions are counted over k",
                h = self.param,
                n = self.order + 1,
                d = self.degree
            ),
        });
        match p.as_object_mut() {
            Some(o) => {
                o.insert("truncation".into(), t);
            }
            None => p = json!({ "base": p, "truncation": t }),
        }
        p
    }
}
