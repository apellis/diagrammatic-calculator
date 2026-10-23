//! Quiver Hecke (super)algebras in their basis `ψ_ŵ x^a e(i)`, reached by straightening.
//!
//! The words of dots and crossings on upward strands are rewritten by the presentation's own
//! relations (dot slides, braid moves, the quadratic relation) to the basis of crossings of a
//! fixed reduced word `ŵ` for each permutation above dots at the bottom
//! ([`crate::rewrite::straighten`]). Every step applies a relation, so a term equals its
//! normal form; that the normal forms are linearly independent is the basis theorem the
//! presentation's certificate names. No polynomial action is computed.

use super::{BasisShape, Evaluator, PolyVector, Provenance, strand_ring};
use crate::context::Context;
use crate::elab::CTerm;
use crate::error::{Error, Result};
use crate::poly::{Poly, PolyRing};
use crate::rewrite::{Layout, Rule};
use crate::ring::Ring;
use crate::term::{Object, Word};

/// The number of rewriting steps one normal form may take.
const MAX_STEPS: usize = 1_000_000;

pub struct QuiverHeckeBasis<'a, R: Ring> {
    ctx: &'a Context<R>,
    layout: Layout,
    rules: Vec<Rule<R>>,
    prov: Provenance,
}

impl<'a, R: Ring> QuiverHeckeBasis<'a, R> {
    pub fn new(ctx: &'a Context<R>, dot: u16, crossing: u16, prov: Provenance) -> Result<Self> {
        Ok(QuiverHeckeBasis {
            ctx,
            layout: Layout {
                dot,
                crossing,
                dots_on_top: false,
            },
            rules: crate::rewrite::rules(ctx)?,
            prov,
        })
    }

    fn shape(&self) -> BasisShape<'_, R> {
        BasisShape {
            ctx: self.ctx,
            dot: self.layout.dot,
            crossing: self.layout.crossing,
            dots_on_top: false,
        }
    }

    fn check(&self, t: &CTerm<R>) -> Result<()> {
        for (w, _) in &t.terms {
            if let Some(l) = w
                .layers
                .iter()
                .find(|l| l.generator != self.layout.dot && l.generator != self.layout.crossing)
            {
                let name = &self.ctx.pres.generators[l.generator as usize].name;
                return Err(Error::precondition(format!(
                    "the basis {} covers dots and crossings on upward strands, not the \
                     generator {name}",
                    self.prov.method
                )));
            }
            if w.source.iter().any(|l| !l.up) {
                return Err(Error::precondition(format!(
                    "the basis {} covers upward strands only",
                    self.prov.method
                )));
            }
        }
        Ok(())
    }
}

impl<R: Ring> Evaluator<R> for QuiverHeckeBasis<'_, R> {
    fn provenance(&self) -> &Provenance {
        &self.prov
    }

    fn rewrites_by_relations(&self) -> bool {
        true
    }

    fn normal_form(&self, t: &CTerm<R>) -> Result<CTerm<R>> {
        self.check(t)?;
        crate::rewrite::normal_form(self.ctx, &self.rules, &self.layout, t, MAX_STEPS)
            .map(|(nf, _)| nf)
    }

    fn is_zero(&self, t: &CTerm<R>) -> Result<bool> {
        Ok(self.normal_form(t)?.is_zero())
    }

    fn act(&self, _: &CTerm<R>, _: &Object, _: &Poly<R::Elem>) -> Result<PolyVector<R::Elem>> {
        Err(Error::precondition(format!(
            "the basis {} computes normal forms by rewriting and has no polynomial action",
            self.prov.method
        )))
    }

    fn poly_ring(&self, n: usize) -> PolyRing<R> {
        strand_ring(self.ctx, n, false)
    }

    fn graded_basis(&self, src: &Object, degree: i64) -> Result<Vec<Word>> {
        self.shape().graded_basis(src, degree)
    }

    fn min_degree(&self, src: &Object) -> Result<i64> {
        self.shape().min_degree(src)
    }

    fn crossing_words(&self, src: &Object) -> Result<Vec<Word>> {
        self.shape().crossing_words(src)
    }

    fn max_degree_below_dot_cap(&self, src: &Object, cap: u32) -> Result<i64> {
        self.shape().max_degree_below_dot_cap(src, cap)
    }
}
