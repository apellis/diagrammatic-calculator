//! Equality decided in another presentation through a translation of the generators.
//!
//! A representation of kind `translation` names a shipped target presentation and, for each
//! generator, its image there: a term in the generator's colour variables. A term is carried
//! over layer by layer (each image whiskered by the strands beside it and composed on top), its
//! normal form is computed by the target's first representation, and the result is carried back
//! by the inverse renaming: every generator occurring in the target's normal forms must be the
//! image of exactly one generator, with coefficient 1. That the translation is an isomorphism is
//! the representation's certificate; the conformance checker checks every relation through it.

use super::{Evaluator, PolyVector, Provenance, strand_ring};
use crate::context::Context;
use crate::elab::{CTerm, Scope};
use crate::error::{Error, Result};
use crate::poly::{Poly, PolyRing};
use crate::pres::RepresentationDecl;
use crate::ring::Ring;
use crate::term::{Layer, Object, Word};
use std::collections::BTreeMap;

pub struct Translation<'a, R: Ring> {
    ctx: &'a Context<R>,
    target: Context<R>,
    /// For each generator, its image's template.
    images: Vec<String>,
    /// Target generator → source generator, for images that are one generator.
    back: BTreeMap<u16, u16>,
    prov: Provenance,
}

impl<'a, R: Ring> Translation<'a, R> {
    pub fn new(ctx: &'a Context<R>, decl: &RepresentationDecl, prov: Provenance) -> Result<Self> {
        let pres = crate::builtin::presentation(&decl.target)?;
        if ctx.mu.is_none() && crate::conformance::uses_weights(&pres) {
            return Err(Error::precondition(format!(
                "the translation {} into {} needs the region weight mu=",
                decl.name, decl.target
            )));
        }
        let mut spec = ctx.spec.clone();
        spec.presentation = decl.target.clone();
        let target = Context::new(pres, spec, ctx.base.clone())?;
        let mut images = Vec::new();
        for g in &ctx.pres.generators {
            let im = decl.images.get(&g.name).ok_or_else(|| {
                Error::invalid(format!(
                    "translation {}: no image for the generator {}",
                    decl.name, g.name
                ))
            })?;
            images.push(im.clone());
        }
        let mut t = Translation {
            ctx,
            target,
            images,
            back: BTreeMap::new(),
            prov,
        };
        // The inverse renaming, read off the images with every colour variable at the first
        // colour (images are written uniformly in their colour variables).
        let mut back = BTreeMap::new();
        for (gi, g) in ctx.pres.generators.iter().enumerate() {
            if ctx.colours.is_empty() {
                break;
            }
            let binding = vec![0u16; g.vars.len()];
            let Ok(im) = t.image(gi as u16, &binding, None) else {
                continue;
            };
            if let [(w, c)] = im.terms.as_slice()
                && let [l] = w.layers.as_slice()
                && *c == t.target.coeff.one()
                && l.binding == binding
                && back.insert(l.generator, gi as u16).is_some()
            {
                return Err(Error::invalid(format!(
                    "translation {}: two generators have the image {}",
                    decl.name, t.target.pres.generators[l.generator as usize].name
                )));
            }
        }
        t.back = back;
        Ok(t)
    }

    /// The image of a generator with its colours, elaborated in the target with the region right
    /// of it.
    fn image(&self, g: u16, binding: &[u16], region: Option<Vec<i64>>) -> Result<CTerm<R>> {
        let decl = &self.ctx.pres.generators[g as usize];
        let scope = Scope {
            colour_vars: decl
                .vars
                .iter()
                .cloned()
                .zip(binding.iter().copied())
                .collect(),
            region,
            ..Scope::default()
        };
        let ast = crate::text::parse(&self.images[g as usize])?;
        self.target.elaborate(&ast, &scope)
    }

    /// A term carried to the target, layer by layer. Each word is first brought to a narrow
    /// layering ([`Context::narrow`]), and the images are stacked as they come, without
    /// canonical relayering: the target's evaluator then sees diagrams about as wide as the
    /// source's, where the canonical layering could open cups beside a cap before it closes.
    fn forward(&self, t: &CTerm<R>) -> Result<CTerm<R>> {
        let tc = &self.target;
        let mut out = Vec::new();
        for (w, c) in &t.terms {
            let (w, neg) = self.ctx.narrow(w);
            let c = if neg { tc.coeff.neg(c) } else { c.clone() };
            let mut acc: Vec<(Vec<Layer>, Poly<R::Elem>)> = vec![(Vec::new(), c)];
            let mut obj = w.source.clone();
            for l in &w.layers {
                let p = l.pos as usize;
                let n = self.ctx.source_len(l.generator as usize);
                let region = self.ctx.region(&obj, p + n);
                let im = self.image(l.generator, &l.binding, region)?;
                let mut next = Vec::new();
                for (iw, ic) in &im.terms {
                    if iw.source[..] != obj[p..p + n] {
                        return Err(Error::invalid(format!(
                            "translation: the image of {} does not start at its source",
                            self.ctx.pres.generators[l.generator as usize].name
                        )));
                    }
                    let shifted: Vec<Layer> = iw
                        .layers
                        .iter()
                        .map(|x| Layer {
                            pos: x.pos + p as u16,
                            ..x.clone()
                        })
                        .collect();
                    for (ls, ac) in &acc {
                        let mut ls = ls.clone();
                        ls.extend(shifted.iter().cloned());
                        next.push((ls, tc.coeff.mul(ac, ic)));
                    }
                }
                acc = next;
                obj = self.ctx.apply(l, &obj);
            }
            for (layers, c) in acc {
                let (w, neg) = tc.narrow(&Word {
                    source: w.source.clone(),
                    layers,
                });
                out.push((w, if neg { tc.coeff.neg(&c) } else { c }));
            }
        }
        Ok(tc.normalize(out))
    }

    /// A term of the target carried back by the inverse renaming.
    fn backward(&self, t: &CTerm<R>) -> Result<CTerm<R>> {
        let mut v = Vec::new();
        for (w, c) in &t.terms {
            let mut layers = Vec::with_capacity(w.layers.len());
            for l in &w.layers {
                let g = self.back.get(&l.generator).ok_or_else(|| {
                    Error::invalid(format!(
                        "translation: the generator {} of the target is the image of no generator",
                        self.target.pres.generators[l.generator as usize].name
                    ))
                })?;
                layers.push(Layer {
                    generator: *g,
                    ..l.clone()
                });
            }
            let (w, neg) = self.ctx.canonical(&Word {
                source: w.source.clone(),
                layers,
            });
            v.push((
                w,
                if neg {
                    self.ctx.coeff.neg(c)
                } else {
                    c.clone()
                },
            ));
        }
        Ok(self.ctx.normalize(v))
    }

    fn with_target<T>(&self, f: impl FnOnce(&dyn Evaluator<R>) -> Result<T>) -> Result<T> {
        let decl =
            self.target.pres.representations.first().ok_or_else(|| {
                Error::precondition("the target presentation has no representation")
            })?;
        let ev = super::evaluator(&self.target, decl)?;
        f(ev.as_ref())
    }
}

impl<R: Ring> Evaluator<R> for Translation<'_, R> {
    fn provenance(&self) -> &Provenance {
        &self.prov
    }

    fn normal_form(&self, t: &CTerm<R>) -> Result<CTerm<R>> {
        let image = self.forward(t)?;
        let nf = self.with_target(|ev| ev.normal_form(&image))?;
        self.backward(&nf)
    }

    fn is_zero(&self, t: &CTerm<R>) -> Result<bool> {
        let image = self.forward(t)?;
        self.with_target(|ev| ev.is_zero(&image))
    }

    fn act(&self, _: &CTerm<R>, _: &Object, _: &Poly<R::Elem>) -> Result<PolyVector<R::Elem>> {
        Err(Error::precondition(format!(
            "the strategy {} is a translation and has no action on polynomials",
            self.prov.method
        )))
    }

    fn poly_ring(&self, n: usize) -> PolyRing<R> {
        strand_ring(self.ctx, n, false)
    }
}
