//! Equality strategies backed by a representation or a basis normal form.
//!
//! Each strategy turns a term into a canonical value, from which it can print a normal form (as a
//! term of the same context), decide whether a term is zero, and compute the action on
//! polynomials. The strategy and the certificate of its faithfulness travel with every result.

pub mod basis_cache;
pub mod kl;
pub mod nilcoxeter;
pub mod skm;
pub mod smash;
pub mod straighten;
pub mod translate;

use crate::context::Context;
use crate::elab::CTerm;
use crate::error::{Error, Result};
use crate::perm::Perm;
use crate::poly::{Monomial, Poly, PolyRing};
use crate::pres::{Certificate, RepresentationDecl};
use crate::ring::Ring;
use crate::term::{Layer, Object, Word};

/// How an equality was decided. See the specification, "Equality strategies".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    BasisNormalForm,
    FaithfulRepresentation,
    ConfluentRewriting,
    FiniteLinearAlgebra,
    BoundedSearch,
}

impl Strategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Strategy::BasisNormalForm => "basis normal form",
            Strategy::FaithfulRepresentation => "faithful representation",
            Strategy::ConfluentRewriting => "confluent rewriting",
            Strategy::FiniteLinearAlgebra => "finite-dimensional linear algebra",
            Strategy::BoundedSearch => "bounded search",
        }
    }
}

/// What a result says about how it was obtained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub strategy: Strategy,
    pub method: String,
    pub certificate: Certificate,
}

/// An element of `⊕_i Pol e(i)`: one polynomial per object.
pub type PolyVector<E> = Vec<(Object, Poly<E>)>;

/// A strategy that evaluates terms of a context to canonical values.
pub trait Evaluator<R: Ring>: Sync {
    fn provenance(&self) -> &Provenance;

    /// The canonical form of a term, written as a term of the same context.
    fn normal_form(&self, t: &CTerm<R>) -> Result<CTerm<R>>;

    /// Whether the term is zero.
    fn is_zero(&self, t: &CTerm<R>) -> Result<bool>;

    /// The action of a term on a polynomial in `x_1, …, x_n` (and the scalars) placed at an object.
    fn act(&self, t: &CTerm<R>, obj: &Object, f: &Poly<R::Elem>) -> Result<PolyVector<R::Elem>>;

    /// The ring that `act` takes and returns polynomials in, for `n` strands.
    fn poly_ring(&self, n: usize) -> PolyRing<R>;

    /// The words of the basis that normal forms are written in, starting at `src` (any target)
    /// and of degree `degree`, in canonical layering. The coordinates of a term in this basis are
    /// the coefficients of its normal form. Strategies without a basis report a precondition.
    fn graded_basis(&self, src: &Object, degree: i64) -> Result<Vec<Word>> {
        let _ = (src, degree);
        Err(Error::precondition(format!(
            "the strategy {} has no basis to enumerate",
            self.provenance().method
        )))
    }

    /// The lowest degree of a basis word starting at `src`.
    fn min_degree(&self, src: &Object) -> Result<i64> {
        let _ = src;
        Err(Error::precondition(format!(
            "the strategy {} has no basis to enumerate",
            self.provenance().method
        )))
    }

    /// The words `ψ_ŵ e(src)`, one for each permutation `w` (its canonical reduced word), with no
    /// dots. Together with the dots they generate the algebra: `x^a ψ_ŵ e(src)` and
    /// `ψ_ŵ x^a e(src)` both span.
    fn crossing_words(&self, src: &Object) -> Result<Vec<Word>> {
        let _ = src;
        Err(Error::precondition(format!(
            "the strategy {} has no basis to enumerate",
            self.provenance().method
        )))
    }

    /// The highest degree computed for words from `src`, when the strategy computes only finitely
    /// many degrees (a finite quotient or a truncation).
    fn max_degree(&self, src: &Object) -> Result<Option<i64>> {
        let _ = src;
        Ok(None)
    }

    /// Whether every graded piece is known: a finite-dimensional algebra, not a truncation.
    fn complete(&self) -> bool {
        false
    }

    /// What a result computed by this strategy should say about it, such as the quotient and the
    /// degrees computed.
    fn notes(&self) -> Vec<String> {
        vec![]
    }

    /// Whether every step of `normal_form` is a relation of the presentation, so that a normal
    /// form of zero proves the term zero whatever the certificate says; only that distinct normal
    /// forms differ rests on the certificate.
    fn rewrites_by_relations(&self) -> bool {
        false
    }

    /// The highest degree of a basis word starting at `src` with every dot exponent below `cap`;
    /// those words span the quotient by all `x_k^cap`.
    fn max_degree_below_dot_cap(&self, src: &Object, cap: u32) -> Result<i64> {
        let _ = (src, cap);
        Err(Error::precondition(format!(
            "the strategy {} has no basis to enumerate",
            self.provenance().method
        )))
    }
}

/// A permutation's reduced word, the degree of its crossings, and the degree of a dot on each
/// strand.
type Shape = (Vec<usize>, i64, Vec<i64>);

/// A basis `{ψ_ŵ x^a e(src)}` (or `x^a ψ_ŵ e(src)`) indexed by permutations `w` with their
/// canonical reduced words and exponent vectors `a`, as used by the polynomial and nilCoxeter
/// strategies. Each dot must have positive degree, so every graded piece is finite.
pub struct BasisShape<'c, R: Ring> {
    pub ctx: &'c Context<R>,
    pub dot: u16,
    pub crossing: u16,
    pub dots_on_top: bool,
}

impl<R: Ring> BasisShape<'_, R> {
    /// For each permutation: its reduced word, the degree of `ψ_ŵ e(src)` and the degrees of a dot
    /// on each strand (at the bottom, or at the top when dots are on top).
    fn shapes(&self, src: &Object) -> Result<Vec<Shape>> {
        let n = src.len();
        let mut out = Vec::new();
        for w in Perm::all(n) {
            let word = w.reduced_word();
            let zeros = vec![0; n];
            let Ok(bw) = basis_word(
                self.ctx,
                src,
                &word,
                &zeros,
                self.dot,
                self.crossing,
                self.dots_on_top,
            ) else {
                continue;
            };
            let deg = self.ctx.word_degree(&bw);
            let at = if self.dots_on_top {
                self.ctx.target(&bw)
            } else {
                src.clone()
            };
            let mut dots = Vec::with_capacity(n);
            for k in 0..n {
                let l = self
                    .ctx
                    .match_layer(self.dot as usize, &at, k)
                    .ok_or_else(|| Error::invalid("a dot does not fit its strand"))?;
                let d = self.ctx.layer_degree(&l, &at);
                if d <= 0 {
                    return Err(Error::precondition(
                        "graded pieces are infinite-dimensional: a dot has degree ≤ 0",
                    ));
                }
                dots.push(d);
            }
            out.push((word, deg, dots));
        }
        Ok(out)
    }

    pub fn crossing_words(&self, src: &Object) -> Result<Vec<Word>> {
        let zeros = vec![0; src.len()];
        self.shapes(src)?
            .into_iter()
            .map(|(word, _, _)| {
                let w = basis_word(
                    self.ctx,
                    src,
                    &word,
                    &zeros,
                    self.dot,
                    self.crossing,
                    self.dots_on_top,
                )?;
                Ok(self.ctx.canonical(&w).0)
            })
            .collect()
    }

    pub fn min_degree(&self, src: &Object) -> Result<i64> {
        Ok(self.shapes(src)?.iter().map(|s| s.1).min().unwrap_or(0))
    }

    pub fn max_degree_below_dot_cap(&self, src: &Object, cap: u32) -> Result<i64> {
        let below = cap.saturating_sub(1) as i64;
        Ok(self
            .shapes(src)?
            .iter()
            .map(|(_, deg, dots)| deg + below * dots.iter().sum::<i64>())
            .max()
            .unwrap_or(0))
    }

    pub fn graded_basis(&self, src: &Object, degree: i64) -> Result<Vec<Word>> {
        fn exps(dots: &[i64], k: usize, left: i64, cur: &mut Vec<i32>, out: &mut Vec<Vec<i32>>) {
            if k == dots.len() {
                if left == 0 {
                    out.push(cur.clone());
                }
                return;
            }
            let mut e = 0;
            while e as i64 * dots[k] <= left {
                cur.push(e);
                exps(dots, k + 1, left - e as i64 * dots[k], cur, out);
                cur.pop();
                e += 1;
            }
        }
        let mut out = Vec::new();
        for (word, deg, dots) in self.shapes(src)? {
            let mut all = Vec::new();
            exps(&dots, 0, degree - deg, &mut Vec::new(), &mut all);
            for a in all {
                let bw = basis_word(
                    self.ctx,
                    src,
                    &word,
                    &a,
                    self.dot,
                    self.crossing,
                    self.dots_on_top,
                )?;
                out.push(self.ctx.canonical(&bw).0);
            }
        }
        out.sort();
        Ok(out)
    }
}

/// Builds the evaluator a presentation file declares.
pub fn evaluator<'a, R: Ring + 'a>(
    ctx: &'a Context<R>,
    decl: &RepresentationDecl,
) -> Result<Box<dyn Evaluator<R> + 'a>> {
    let prov = |strategy| Provenance {
        strategy,
        method: decl.name.clone(),
        certificate: decl.certificate_in(ctx).clone(),
    };
    let dot = || role(ctx, decl, "dot");
    let crossing = || role(ctx, decl, "crossing");
    Ok(match decl.kind.as_str() {
        "klr-polynomial" => Box::new(smash::Smash::new(
            ctx,
            dot()?,
            crossing()?,
            prov(Strategy::FaithfulRepresentation),
        )),
        "nil-coxeter" => Box::new(nilcoxeter::NilCoxeter::new(
            ctx,
            dot()?,
            crossing()?,
            false,
            prov(Strategy::BasisNormalForm),
        )),
        "odd-nil-coxeter" => Box::new(nilcoxeter::NilCoxeter::new(
            ctx,
            dot()?,
            crossing()?,
            true,
            prov(Strategy::BasisNormalForm),
        )),
        "quiver-hecke-basis" => Box::new(straighten::QuiverHeckeBasis::new(
            ctx,
            dot()?,
            crossing()?,
            prov(Strategy::BasisNormalForm),
        )?),
        "kl-basis" => Box::new(kl::KlBasis::new(
            ctx,
            decl,
            prov(Strategy::BasisNormalForm),
        )?),
        "skm-basis" => Box::new(skm::SkmBasis::new(
            ctx,
            decl,
            prov(Strategy::BasisNormalForm),
        )?),
        "translation" => Box::new(translate::Translation::new(
            ctx,
            decl,
            prov(Strategy::BasisNormalForm),
        )?),
        k => return Err(Error::invalid(format!("unknown representation kind '{k}'"))),
    })
}

/// All evaluators of a context, in file order, leaving out those whose precondition fails in it
/// (a translation that holds for even colours only, in a context with an odd colour).
pub fn evaluators<'a, R: Ring + 'a>(
    ctx: &'a Context<R>,
) -> Result<Vec<Box<dyn Evaluator<R> + 'a>>> {
    let mut out = Vec::new();
    for d in &ctx.pres.representations {
        match evaluator(ctx, d) {
            Ok(e) => out.push(e),
            Err(e) if e.kind == crate::error::ErrorKind::Precondition => {}
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

/// The first representation of a context that applies in it.
pub fn first_applicable<R: Ring>(ctx: &Context<R>) -> Result<&RepresentationDecl> {
    let mut why = None;
    for d in &ctx.pres.representations {
        match evaluator(ctx, d) {
            Ok(_) => return Ok(d),
            Err(e) if e.kind == crate::error::ErrorKind::Precondition => {
                why.get_or_insert(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(why.unwrap_or_else(|| {
        Error::precondition(format!(
            "presentation {} declares no representation",
            ctx.pres.name
        ))
    }))
}

/// The basis a declared representation writes normal forms in: the dot and crossing generators
/// and whether the dots sit on top (`x^a ψ_ŵ`) or at the bottom (`ψ_ŵ x^a`).
pub fn basis_layout<R: Ring>(
    ctx: &Context<R>,
    decl: &RepresentationDecl,
) -> Result<crate::rewrite::Layout> {
    let dots_on_top = match decl.kind.as_str() {
        "klr-polynomial" | "quiver-hecke-basis" => false,
        "nil-coxeter" | "odd-nil-coxeter" => true,
        k => {
            return Err(Error::precondition(format!(
                "representation {} ({k}) has no basis of crossings and dots",
                decl.name
            )));
        }
    };
    Ok(crate::rewrite::Layout {
        dot: role(ctx, decl, "dot")?,
        crossing: role(ctx, decl, "crossing")?,
        dots_on_top,
    })
}

fn role<R: Ring>(ctx: &Context<R>, decl: &RepresentationDecl, r: &str) -> Result<u16> {
    let name = decl
        .roles
        .get(r)
        .ok_or_else(|| Error::invalid(format!("representation {} has no '{r}' role", decl.name)))?;
    ctx.pres.generator(name).map(|g| g as u16).ok_or_else(|| {
        Error::invalid(format!(
            "representation {}: no generator '{name}'",
            decl.name
        ))
    })
}

/// The ring `x_1, …, x_n` followed by the context's scalars; the `x` are odd if `odd`.
pub fn strand_ring<R: Ring>(ctx: &Context<R>, n: usize, odd: bool) -> PolyRing<R> {
    let mut names: Vec<String> = (1..=n).map(|k| format!("x_{k}")).collect();
    names.extend(ctx.spec.scalars.iter().cloned());
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut r = PolyRing::new(ctx.base.clone(), &refs);
    for k in 0..n {
        r.odd[k] = odd;
    }
    r
}

/// A coefficient (polynomial in the scalars) as a polynomial of the strand ring.
pub fn lift_coeff<E: Clone>(c: &Poly<E>, n: usize) -> Poly<E> {
    Poly {
        terms: c
            .terms
            .iter()
            .map(|(m, x)| {
                let mut e: crate::poly::Exps = smallvec::smallvec![0; n];
                e.extend_from_slice(&m.0);
                (Monomial(e), x.clone())
            })
            .collect(),
    }
}

/// Splits a strand-ring monomial into its dot exponents and its scalar part.
pub fn split_monomial(m: &Monomial, n: usize) -> (Vec<i32>, Monomial) {
    (m.0[..n].to_vec(), Monomial::from(&m.0[n..]))
}

/// The word `ψ_ŵ ∘ x^a e(obj)` (dots at the bottom) when `dots_on_top` is false, or
/// `x^a ∘ ψ_ŵ e(obj)` when it is true. `word` lists crossing positions topmost first. The dots
/// read as the normal-ordered monomial `x_1^{a_1} ∘ ⋯ ∘ x_n^{a_n}`, so `x_n` is lowest; this
/// matters when dots are odd.
pub fn basis_word<R: Ring>(
    ctx: &Context<R>,
    obj: &Object,
    word: &[usize],
    exps: &[i32],
    dot: u16,
    crossing: u16,
    dots_on_top: bool,
) -> Result<Word> {
    let mut layers = Vec::new();
    let mut cur = obj.clone();
    let push = |g: u16, pos: usize, cur: &mut Object, layers: &mut Vec<Layer>| -> Result<()> {
        let l = ctx
            .match_layer(g as usize, cur, pos)
            .ok_or_else(|| Error::invalid("a basis generator does not fit its object"))?;
        *cur = ctx.apply(&l, cur);
        layers.push(l);
        Ok(())
    };
    let dots = |cur: &mut Object, layers: &mut Vec<Layer>| -> Result<()> {
        for (k, &e) in exps.iter().enumerate().rev() {
            for _ in 0..e {
                push(dot, k, cur, layers)?;
            }
        }
        Ok(())
    };
    if !dots_on_top {
        dots(&mut cur, &mut layers)?;
    }
    for &k in word.iter().rev() {
        push(crossing, k, &mut cur, &mut layers)?;
    }
    if dots_on_top {
        dots(&mut cur, &mut layers)?;
    }
    Ok(Word {
        source: obj.clone(),
        layers,
    })
}

/// The object after permuting strands by `w`: the strand at position `a` ends at `w(a)`.
pub fn permute_object(obj: &[crate::term::Letter], w: &crate::perm::Perm) -> Object {
    let mut out = obj.to_vec();
    for (a, l) in obj.iter().enumerate() {
        out[w.0[a] as usize] = *l;
    }
    out
}
