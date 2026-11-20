//! Complexes of right projective modules over a weighted KLR algebra or its cyclotomic quotient
//! (`docs/complexes.md`).
//!
//! The objects are the loadings `ℓ`, standing for the graded right projective modules
//! `P(ℓ) = e(ℓ) A`. A morphism `P(a) → P(b)` is left multiplication by an element of
//! `e(b) A e(a)`: a term whose bottom is `a` and whose top is `b`. Composition is stacking, so
//! `g ∘ f` puts `g` on top of `f`. Isotopic loadings (one order type) give equal objects.
//!
//! A basis of `e(b) A e(a)` in degree `d` is the set of basis diagrams `b_π x^m e(a)` of degree
//! `d`, and in the cyclotomic quotient its free columns modulo the violating ideal
//! (`rep::klrw_cyclo`). Coordinates are read off the normal form.

use super::klrw::{Elem, Klrw, Summand};
use super::klrw_cyclo::monomials;
use super::weighted::{Loading, OrderType};
use crate::complex::Additive;
use crate::error::{Error, Result};
use crate::perm::Perm;
use crate::poly::Poly;
use crate::ring::Ring;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

type C<R> = Poly<<R as Ring>::Elem>;
type Col = (Perm, Vec<u32>);

impl<R: Ring> Klrw<R> {
    /// The basis columns `b_π x^m` of `e(top) A e(bottom)` in degree `d`.
    fn hom_cols(&self, bottom: &Loading, top: &Loading, d: i64) -> Result<Vec<Col>> {
        if self.cyclo.is_some() {
            let piece = self.ideal_piece(bottom, top, d)?;
            return Ok(piece
                .span
                .free_columns()
                .into_iter()
                .map(|j| piece.cols[j].clone())
                .collect());
        }
        let wb = self.dot_weights(bottom);
        let mut cols = Vec::new();
        for p in self.compatible(bottom, top) {
            let dp = self.perm_degree(bottom, top, &p)?;
            for m in monomials(&wb, d - dp) {
                cols.push((p.clone(), m));
            }
        }
        Ok(cols)
    }

    fn ends(&self, f: &Elem<C<R>>, src: &Loading, tgt: &Loading) -> Result<Vec<Summand<C<R>>>> {
        let (os, ot) = (self.data.order_type(src), self.data.order_type(tgt));
        let nf = self.normal_form(f)?;
        for s in &nf {
            if self.data.order_type(&s.bottom) != os || self.data.order_type(&s.top) != ot {
                return Err(Error::invalid(format!(
                    "the term has a summand from e({}) to e({}), not from e({}) to e({})",
                    self.fmt_loading(&s.bottom),
                    self.fmt_loading(&s.top),
                    self.fmt_loading(src),
                    self.fmt_loading(tgt)
                )));
            }
        }
        Ok(nf)
    }
}

impl<R: Ring> Additive for Klrw<R> {
    type Field = R;
    type Obj = Loading;
    type Key = OrderType;
    type Mor = Elem<C<R>>;

    fn field(&self) -> &R {
        &self.klr.base
    }

    fn parse_obj(&self, s: &str) -> Result<Loading> {
        let t = s.trim();
        let inner = t
            .strip_prefix("e(")
            .and_then(|x| x.strip_suffix(')'))
            .unwrap_or(t);
        self.parse_loading(inner)
    }

    fn fmt_obj(&self, o: &Loading) -> String {
        self.fmt_loading(o)
    }

    fn key(&self, o: &Loading) -> OrderType {
        self.data.order_type(o)
    }

    fn parse_mor(&self, s: &str) -> Result<Self::Mor> {
        self.term(&crate::text::parse(s)?, &BTreeMap::new())
    }

    fn fmt_mor(&self, f: &Self::Mor) -> Result<String> {
        Ok(self.fmt_summands(&self.normal_form(f)?))
    }

    fn zero(&self) -> Self::Mor {
        Elem::default()
    }

    fn identity(&self, o: &Loading) -> Self::Mor {
        Klrw::identity(self, o)
    }

    fn add(&self, f: &Self::Mor, g: &Self::Mor) -> Self::Mor {
        Klrw::add(self, f, g)
    }

    fn neg(&self, f: &Self::Mor) -> Self::Mor {
        Klrw::neg(self, f)
    }

    fn scale(&self, c: &R::Elem, f: &Self::Mor) -> Self::Mor {
        Klrw::scale(self, &self.coeff().constant(c.clone()), f)
    }

    fn compose(&self, g: &Self::Mor, f: &Self::Mor) -> Self::Mor {
        Klrw::compose(self, g, f)
    }

    fn is_zero(&self, f: &Self::Mor) -> Result<bool> {
        Ok(self.normal_form(f)?.is_empty())
    }

    fn mor_degree(&self, src: &Loading, tgt: &Loading, f: &Self::Mor) -> Result<Option<i64>> {
        let nf = self.ends(f, src, tgt)?;
        let degs: BTreeSet<i64> = nf.iter().map(|s| self.degree(s)).collect::<Result<_>>()?;
        match degs.len() {
            0 => Ok(None),
            1 => Ok(degs.into_iter().next()),
            _ => Err(Error::invalid(format!(
                "the term is not homogeneous: it has summands of degrees {}",
                degs.iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    fn mor_ends(&self, f: &Self::Mor) -> Result<Option<crate::complex::MorEnds<Loading>>> {
        let nf = self.normal_form(f)?;
        let Some(first) = nf.first() else {
            return Ok(None);
        };
        let (os, ot) = (
            self.data.order_type(&first.bottom),
            self.data.order_type(&first.top),
        );
        let d = self.degree(first)?;
        for s in &nf[1..] {
            if self.data.order_type(&s.bottom) != os || self.data.order_type(&s.top) != ot {
                return Err(Error::invalid(format!(
                    "the term is not a map between two objects: it has summands from e({}) to \
                     e({}) and from e({}) to e({})",
                    self.fmt_loading(&first.bottom),
                    self.fmt_loading(&first.top),
                    self.fmt_loading(&s.bottom),
                    self.fmt_loading(&s.top)
                )));
            }
            let e = self.degree(s)?;
            if e != d {
                return Err(Error::invalid(format!(
                    "the term is not homogeneous: it has summands of degrees {d} and {e}"
                )));
            }
        }
        Ok(Some((first.bottom.clone(), first.top.clone(), d)))
    }

    fn hom_degrees(&self, src: &Loading, tgt: &Loading) -> Result<Option<(i64, Option<i64>)>> {
        if self.cyclo.is_some() {
            let g = self.graded_dimension(src, tgt)?;
            return Ok(match (g.keys().next(), g.keys().next_back()) {
                (Some(lo), Some(hi)) => Some((*lo, Some(*hi))),
                _ => None,
            });
        }
        let perms = self.compatible(src, tgt);
        if perms.is_empty() {
            return Ok(None);
        }
        let degs: Vec<i64> = perms
            .iter()
            .map(|p| self.perm_degree(src, tgt, p))
            .collect::<Result<_>>()?;
        let lo = *degs.iter().min().unwrap();
        let hi = (src.n() == 0).then(|| *degs.iter().max().unwrap());
        Ok(Some((lo, hi)))
    }

    fn hom_basis(&self, src: &Loading, tgt: &Loading, d: i64) -> Result<Vec<Self::Mor>> {
        let one = self.coeff().one();
        self.hom_cols(src, tgt, d)?
            .into_iter()
            .map(|(perm, dots)| {
                self.summand_elem(&Summand {
                    bottom: src.clone(),
                    top: tgt.clone(),
                    perm,
                    dots,
                    coeff: one.clone(),
                })
            })
            .collect()
    }

    fn coords(&self, src: &Loading, tgt: &Loading, d: i64, f: &Self::Mor) -> Result<Vec<R::Elem>> {
        let cols = self.hom_cols(src, tgt, d)?;
        let index: BTreeMap<&Col, usize> = cols.iter().enumerate().map(|(i, c)| (c, i)).collect();
        let k = &self.klr.base;
        let ring = self.coeff();
        let mut v = vec![k.zero(); cols.len()];
        for s in self.ends(f, src, tgt)? {
            let i = index
                .get(&(s.perm.clone(), s.dots.clone()))
                .ok_or_else(|| {
                    Error::invalid(format!(
                        "the summand {} does not have degree {d}",
                        self.fmt_summands(std::slice::from_ref(&s))
                    ))
                })?;
            if !ring.is_constant(&s.coeff) {
                return Err(Error::precondition(format!(
                    "linear algebra needs numeric coefficients, not {}",
                    ring.fmt_elem(&s.coeff)
                )));
            }
            v[*i] = k.add(&v[*i], &ring.constant_term(&s.coeff));
        }
        Ok(v)
    }

    fn test_objects(&self, objs: &[Loading]) -> Result<Vec<Loading>> {
        let mut seen: BTreeSet<OrderType> = BTreeSet::new();
        let mut colour_sets: BTreeSet<Vec<u16>> = BTreeSet::new();
        let mut out = Vec::new();
        for o in objs {
            let mut c = o.colours();
            c.sort();
            colour_sets.insert(c);
        }
        for c in &colour_sets {
            // the caller's loadings first, so that results name them as written
            let mut ls: Vec<Loading> = objs
                .iter()
                .filter(|o| {
                    let mut oc = o.colours();
                    oc.sort();
                    &oc == c
                })
                .cloned()
                .collect();
            if c.is_empty() {
                ls.push(Loading(vec![]));
            } else {
                ls.extend(self.order_types(c));
            }
            for l in ls {
                if seen.insert(self.data.order_type(&l)) {
                    out.push(l);
                }
            }
        }
        Ok(out)
    }

    fn linear_algebra(&self) -> Result<()> {
        if !self.klr.base.is_field() {
            return Err(Error::precondition(
                "homology and kernels need a field of coefficients: use QQ, GF(p), or QQ(q)",
            ));
        }
        Ok(())
    }

    fn provenance(&self) -> Value {
        self.provenance_json()
    }
}
