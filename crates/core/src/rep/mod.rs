//! Equality strategies backed by a representation or a basis normal form.
//!
//! Each strategy turns a term into a canonical value, from which it can print a normal form (as a
//! term of the same context), decide whether a term is zero, and compute the action on
//! polynomials. The strategy and the certificate of its faithfulness travel with every result.

pub mod nilcoxeter;
pub mod smash;

use crate::context::Context;
use crate::elab::CTerm;
use crate::error::{Error, Result};
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
pub trait Evaluator<R: Ring> {
    fn provenance(&self) -> &Provenance;

    /// The canonical form of a term, written as a term of the same context.
    fn normal_form(&self, t: &CTerm<R>) -> Result<CTerm<R>>;

    /// Whether the term is zero.
    fn is_zero(&self, t: &CTerm<R>) -> Result<bool>;

    /// The action of a term on a polynomial in `x_1, …, x_n` (and the scalars) placed at an object.
    fn act(&self, t: &CTerm<R>, obj: &Object, f: &Poly<R::Elem>) -> Result<PolyVector<R::Elem>>;

    /// The ring that `act` takes and returns polynomials in, for `n` strands.
    fn poly_ring(&self, n: usize) -> PolyRing<R>;
}

/// Builds the evaluator a presentation file declares.
pub fn evaluator<'a, R: Ring + 'a>(
    ctx: &'a Context<R>,
    decl: &RepresentationDecl,
) -> Result<Box<dyn Evaluator<R> + 'a>> {
    let dot = role(ctx, decl, "dot")?;
    let crossing = role(ctx, decl, "crossing")?;
    let prov = |strategy| Provenance {
        strategy,
        method: decl.name.clone(),
        certificate: decl.certificate.clone(),
    };
    Ok(match decl.kind.as_str() {
        "klr-polynomial" => Box::new(smash::Smash::new(
            ctx,
            dot,
            crossing,
            prov(Strategy::FaithfulRepresentation),
        )),
        "nil-coxeter" => Box::new(nilcoxeter::NilCoxeter::new(
            ctx,
            dot,
            crossing,
            false,
            prov(Strategy::BasisNormalForm),
        )),
        "odd-nil-coxeter" => Box::new(nilcoxeter::NilCoxeter::new(
            ctx,
            dot,
            crossing,
            true,
            prov(Strategy::BasisNormalForm),
        )),
        k => return Err(Error::invalid(format!("unknown representation kind '{k}'"))),
    })
}

/// All evaluators of a context, in file order.
pub fn evaluators<'a, R: Ring + 'a>(
    ctx: &'a Context<R>,
) -> Result<Vec<Box<dyn Evaluator<R> + 'a>>> {
    ctx.pres
        .representations
        .iter()
        .map(|d| evaluator(ctx, d))
        .collect()
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
