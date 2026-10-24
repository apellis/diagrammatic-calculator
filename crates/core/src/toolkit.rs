//! The graded linear-algebra toolkit: bases, multiplication and Gram matrices, ranks, centres,
//! the Jacobson radical, the semisimple quotient, blocks, idempotent lifting, and primitivity.
//!
//! Everything works on a [`Graded`] algebra: an algebra whose graded pieces `A_d e(i)` have a
//! basis of words and a normal form in that basis. The ambient algebra of a context with a basis
//! strategy ([`Ambient`]) is one: with infinitely many degrees for the algebra itself, or finitely
//! many when the strategy is a finite quotient or truncation ([`Quotient`]). Operations that need the whole algebra (the radical, blocks,
//! primitivity) check that it is finite-dimensional and not truncated, and say so otherwise.
//! Each operation states its precondition in its documentation and returns
//! [`Error::precondition`] when it fails.

use crate::context::Context;
use crate::elab::CTerm;
use crate::error::{Error, Result};
use crate::linalg::{self, Mat, Reducer};
use crate::poly::Poly;
use crate::quotient::Quotient;
use crate::rep::Evaluator;
use crate::ring::Ring;
use crate::term::{Object, Word};
use std::collections::HashMap;

/// A graded algebra with a basis of words in each graded piece.
pub trait Graded<R: Ring> {
    fn context(&self) -> &Context<R>;
    /// The degrees available for words from `src`; the upper end is `i64::MAX` when unbounded.
    fn window(&self, src: &Object) -> Result<(i64, i64)>;
    /// The basis words of `A_d e(src)`, with any target.
    fn piece_words(&self, src: &Object, d: i64) -> Result<Vec<Word>>;
    /// The normal form: a combination of basis words.
    fn reduce(&self, t: &CTerm<R>) -> Result<CTerm<R>>;
    /// Whether every graded piece is known: the algebra is finite-dimensional and not truncated.
    fn complete(&self) -> bool;
}

/// The algebra a strategy computes in, through its basis: the context's algebra, or a quotient
/// or truncation of it when the strategy is a [`Quotient`].
pub struct Ambient<'a, R: Ring> {
    pub ctx: &'a Context<R>,
    pub eval: &'a dyn Evaluator<R>,
}

impl<R: Ring> Graded<R> for Ambient<'_, R> {
    fn context(&self) -> &Context<R> {
        self.ctx
    }
    fn window(&self, src: &Object) -> Result<(i64, i64)> {
        Ok((
            self.eval.min_degree(src)?,
            self.eval.max_degree(src)?.unwrap_or(i64::MAX),
        ))
    }
    fn piece_words(&self, src: &Object, d: i64) -> Result<Vec<Word>> {
        self.eval.graded_basis(src, d)
    }
    fn reduce(&self, t: &CTerm<R>) -> Result<CTerm<R>> {
        self.eval.normal_form(t)
    }
    fn complete(&self) -> bool {
        self.eval.complete()
    }
}

impl<R: Ring> Graded<R> for Quotient<'_, R> {
    fn context(&self) -> &Context<R> {
        self.ctx
    }
    fn window(&self, src: &Object) -> Result<(i64, i64)> {
        Quotient::window(self, src)
    }
    fn piece_words(&self, src: &Object, d: i64) -> Result<Vec<Word>> {
        let p = self.piece(src, d)?;
        Ok(p.quotient_basis()
            .into_iter()
            .map(|k| p.basis[k].clone())
            .collect())
    }
    fn reduce(&self, t: &CTerm<R>) -> Result<CTerm<R>> {
        Quotient::reduce(self, t)
    }
    fn complete(&self) -> bool {
        Evaluator::complete(self)
    }
}

type Coeff<R> = Poly<<R as Ring>::Elem>;
/// Coordinates of an element of the degree-0 centre in a chosen basis of it.
type InCentre<'a, R> = dyn Fn(&CTerm<R>) -> Result<Vec<Coeff<R>>> + 'a;

/// Words indexed for coordinates.
pub struct Indexed {
    pub words: Vec<Word>,
    index: HashMap<Word, usize>,
}

impl Indexed {
    pub fn new(words: Vec<Word>) -> Self {
        let index = words
            .iter()
            .cloned()
            .enumerate()
            .map(|(k, w)| (w, k))
            .collect();
        Indexed { words, index }
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Coordinates of a normal form whose words all lie here.
    pub fn coords<R: Ring>(&self, ctx: &Context<R>, t: &CTerm<R>) -> Result<Vec<Coeff<R>>> {
        let mut v = vec![ctx.coeff.zero(); self.words.len()];
        for (w, c) in &t.terms {
            let k = self.index.get(w).ok_or_else(|| {
                Error::invalid(format!("{} is not among the basis words", ctx.fmt_word(w)))
            })?;
            v[*k] = c.clone();
        }
        Ok(v)
    }

    /// The term with these coordinates.
    pub fn term<R: Ring>(&self, ctx: &Context<R>, v: &[Coeff<R>]) -> CTerm<R> {
        ctx.normalize(
            v.iter()
                .enumerate()
                .filter(|(_, c)| !ctx.coeff.is_zero(c))
                .map(|(k, c)| (self.words[k].clone(), c.clone()))
                .collect(),
        )
    }
}

fn require_field<R: Ring>(ctx: &Context<R>) -> Result<()> {
    if ctx.coeff.is_field() {
        Ok(())
    } else if !ctx.spec.scalars.is_empty() {
        Err(Error::precondition(format!(
            "linear algebra needs a field of coefficients; here they are polynomials in {} over {} \
             (the unknowns of solve cells are scalars of the whole workspace, so check this in a \
             workspace without them)",
            ctx.spec.scalars.join(", "),
            ctx.spec.ring
        )))
    } else {
        Err(Error::precondition(format!(
            "linear algebra needs a field of coefficients; here they are {}",
            ctx.spec.ring
        )))
    }
}

fn require_complete<R: Ring>(alg: &dyn Graded<R>, what: &str) -> Result<()> {
    if alg.complete() {
        Ok(())
    } else {
        Err(Error::precondition(format!(
            "{what} needs the whole algebra: give a finite quotient with a known degree bound, without a truncation"
        )))
    }
}

/// The words of `A_d` (all sources), or of `e(tgt) A_d e(src)`.
pub fn piece(
    alg: &dyn Graded<impl Ring>,
    src: Option<&Object>,
    tgt: Option<&Object>,
    d: i64,
) -> Result<Indexed> {
    let ctx = alg.context();
    let sources: Vec<Object> = match src {
        Some(s) => vec![s.clone()],
        None => ctx.objects.clone(),
    };
    let mut words = Vec::new();
    for s in &sources {
        let (lo, hi) = alg.window(s)?;
        if d < lo {
            continue;
        }
        if d > hi && alg.complete() {
            continue;
        }
        // An incomplete window may still have a separate proven vanishing bound. The evaluator
        // distinguishes that zero piece from an uncomputed degree above an explicit truncation.
        for w in alg.piece_words(s, d)? {
            if tgt.is_none_or(|t| ctx.target(&w) == *t) {
                words.push(w);
            }
        }
    }
    Ok(Indexed::new(words))
}

/// A basis of the whole algebra. Needs a finite algebra (or a truncation, whose computed
/// degrees are listed).
pub fn basis<R: Ring>(alg: &dyn Graded<R>) -> Result<Indexed> {
    let ctx = alg.context();
    let mut words = Vec::new();
    for s in &ctx.objects {
        let (lo, hi) = alg.window(s)?;
        if hi == i64::MAX {
            return Err(Error::precondition(
                "listing the whole basis needs a known upper bound or an explicit degcap; individual degrees can be computed without one",
            ));
        }
        for d in lo..=hi {
            words.extend(alg.piece_words(s, d)?);
        }
    }
    Ok(Indexed::new(words))
}

/// `a ∘ b` in normal form.
pub fn mul<R: Ring>(alg: &dyn Graded<R>, a: &CTerm<R>, b: &CTerm<R>) -> Result<CTerm<R>> {
    alg.reduce(&alg.context().compose(a, b))
}

fn word_term<R: Ring>(ctx: &Context<R>, w: &Word) -> CTerm<R> {
    ctx.monomial_term(w, ctx.coeff.one())
}

/// The matrix of `b ↦ a ∘ b` (`left`) or `b ↦ b ∘ a` from the span of `from` to the span of
/// `to`, with one column per word of `from`.
pub fn multiplication_matrix<R: Ring>(
    alg: &dyn Graded<R>,
    a: &CTerm<R>,
    left: bool,
    from: &Indexed,
    to: &Indexed,
) -> Result<Mat<Coeff<R>>> {
    let ctx = alg.context();
    let mut cols = Vec::with_capacity(from.len());
    for w in &from.words {
        let b = word_term(ctx, w);
        let p = if left {
            mul(alg, a, &b)?
        } else {
            mul(alg, &b, a)?
        };
        cols.push(to.coords(ctx, &p)?);
    }
    Ok(Mat::from_fn(to.len(), from.len(), |i, j| {
        cols[j][i].clone()
    }))
}

/// The dimension of the span of some elements.
pub fn span_rank<R: Ring>(alg: &dyn Graded<R>, elems: &[CTerm<R>]) -> Result<usize> {
    let ctx = alg.context();
    require_field(ctx)?;
    let mut words: Vec<Word> = elems
        .iter()
        .flat_map(|t| alg.reduce(t).map(|t| t.terms.into_iter().map(|(w, _)| w)))
        .flatten()
        .collect();
    words.sort();
    words.dedup();
    let ix = Indexed::new(words);
    let mut r = Reducer::new(ctx.coeff.clone(), ix.len());
    for t in elems {
        r.insert(ix.coords(ctx, &alg.reduce(t)?)?);
    }
    Ok(r.rank())
}

/// The Gram matrix `(λ(x_i ∘ y_j))` of a functional `λ`, given by its values on basis words
/// (words not listed count as 0).
pub fn gram<R: Ring>(
    alg: &dyn Graded<R>,
    xs: &[CTerm<R>],
    ys: &[CTerm<R>],
    functional: &HashMap<Word, Coeff<R>>,
) -> Result<Mat<Coeff<R>>> {
    let ctx = alg.context();
    let mut rows = Vec::with_capacity(xs.len());
    for x in xs {
        let mut row = Vec::with_capacity(ys.len());
        for y in ys {
            row.push(apply(ctx, functional, &mul(alg, x, y)?));
        }
        rows.push(row);
    }
    Mat::from_rows(ys.len(), rows)
}

fn apply<R: Ring>(
    ctx: &Context<R>,
    functional: &HashMap<Word, Coeff<R>>,
    t: &CTerm<R>,
) -> Coeff<R> {
    let mut acc = ctx.coeff.zero();
    for (w, c) in &t.terms {
        if let Some(v) = functional.get(w) {
            acc = ctx.coeff.add(&acc, &ctx.coeff.mul(c, v));
        }
    }
    acc
}

/// The single-layer words of the algebra's generators placed at every object and position: they
/// generate the algebra together with the idempotents.
pub fn generator_layers<R: Ring>(ctx: &Context<R>) -> Vec<Word> {
    let mut out = Vec::new();
    for obj in &ctx.objects {
        for g in 0..ctx.pres.generators.len() {
            for pos in 0..obj.len() {
                if let Some(l) = ctx.match_layer(g, obj, pos) {
                    let w = Word {
                        source: obj.clone(),
                        layers: vec![l],
                    };
                    if ctx.objects.contains(&ctx.target(&w)) {
                        out.push(w);
                    }
                }
            }
        }
    }
    out
}

/// A basis of the degree-`d` part of the centre: elements of `⊕_i e(i) A_d e(i)` commuting with
/// every generator. Needs a field, and the pieces of degree `d + deg g` for the generators `g`.
pub fn centre<R: Ring>(alg: &dyn Graded<R>, d: i64) -> Result<Vec<CTerm<R>>> {
    let ctx = alg.context();
    require_field(ctx)?;
    let mut unknowns = Vec::new();
    for s in &ctx.objects {
        unknowns.extend(piece(alg, Some(s), Some(s), d)?.words);
    }
    let unknowns = Indexed::new(unknowns);
    if unknowns.is_empty() {
        return Ok(vec![]);
    }
    let layers = generator_layers(ctx);
    // For each generator layer g: the coordinates of z g − g z, one column per unknown.
    let mut rows: Vec<Vec<Coeff<R>>> = Vec::new();
    for g in &layers {
        let gt = word_term(ctx, g);
        let tgt_piece = {
            let deg = d + ctx.word_degree(g);
            piece(alg, Some(&g.source), None, deg)?
        };
        if tgt_piece.is_empty() {
            continue;
        }
        let mut cols = Vec::with_capacity(unknowns.len());
        for w in &unknowns.words {
            let b = word_term(ctx, w);
            let t = ctx.sub(&mul(alg, &b, &gt)?, &mul(alg, &gt, &b)?);
            cols.push(tgt_piece.coords(ctx, &t)?);
        }
        for i in 0..tgt_piece.len() {
            let row: Vec<Coeff<R>> = cols.iter().map(|c| c[i].clone()).collect();
            if row.iter().any(|c| !ctx.coeff.is_zero(c)) {
                rows.push(row);
            }
        }
    }
    let kernel = if rows.is_empty() {
        (0..unknowns.len())
            .map(|k| {
                let mut v = vec![ctx.coeff.zero(); unknowns.len()];
                v[k] = ctx.coeff.one();
                v
            })
            .collect()
    } else {
        linalg::kernel(&ctx.coeff, &Mat::from_rows(unknowns.len(), rows)?)?
    };
    Ok(kernel.iter().map(|v| unknowns.term(ctx, v)).collect())
}

/// `tr(L_c)` for a degree-0 endomorphism word `c`: the trace of left multiplication on the whole
/// algebra. Other words have trace 0 (they change the degree or the object).
fn traces<R: Ring>(alg: &dyn Graded<R>, all: &Indexed) -> Result<HashMap<Word, Coeff<R>>> {
    let ctx = alg.context();
    let mut out = HashMap::new();
    for c in &all.words {
        if ctx.word_degree(c) != 0 || ctx.target(c) != c.source {
            continue;
        }
        let ct = word_term(ctx, c);
        let mut tr = ctx.coeff.zero();
        for (j, b) in all.words.iter().enumerate() {
            if ctx.target(b) != c.source {
                continue;
            }
            let p = mul(alg, &ct, &word_term(ctx, b))?;
            let v = all.coords(ctx, &p)?;
            tr = ctx.coeff.add(&tr, &v[j]);
        }
        if !ctx.coeff.is_zero(&tr) {
            out.insert(c.clone(), tr);
        }
    }
    Ok(out)
}

/// The Jacobson radical: the kernel of the trace form `(a, b) ↦ tr(L_{ab})`, which is the
/// radical in characteristic 0 (Dickson). Needs the whole finite algebra over a field of
/// characteristic 0. Returns a basis of the radical, as elements.
pub fn radical<R: Ring>(alg: &dyn Graded<R>) -> Result<Vec<CTerm<R>>> {
    let ctx = alg.context();
    require_field(ctx)?;
    require_complete(alg, "the radical")?;
    if ctx.coeff.characteristic() != 0 {
        return Err(Error::precondition(
            "the radical is computed by the trace form, which needs characteristic 0",
        ));
    }
    let all = basis(alg)?;
    let tr = traces(alg, &all)?;
    // T(b_i, b_j) = λ(b_i b_j); nonzero only when b_i b_j has degree 0.
    let n = all.len();
    let mut m = vec![vec![ctx.coeff.zero(); n]; n];
    for (bi, row) in all.words.iter().zip(m.iter_mut()) {
        for (bj, entry) in all.words.iter().zip(row.iter_mut()) {
            if ctx.word_degree(bi) + ctx.word_degree(bj) != 0 || ctx.target(bj) != bi.source {
                continue;
            }
            *entry = apply(
                ctx,
                &tr,
                &mul(alg, &word_term(ctx, bi), &word_term(ctx, bj))?,
            );
        }
    }
    let k = linalg::kernel(&ctx.coeff, &Mat::from_rows(n, m)?)?;
    Ok(k.iter().map(|v| all.term(ctx, v)).collect())
}

/// The dimension of the semisimple quotient `A / J(A)` (same preconditions as [`radical`]).
pub fn semisimple_dimension<R: Ring>(alg: &dyn Graded<R>) -> Result<usize> {
    Ok(basis(alg)?.len() - radical(alg)?.len())
}

/// Lifts an idempotent modulo a nilpotent ideal: iterates `e ↦ 3e² − 2e³` until `e² = e`.
/// Fails after `max_steps` iterations (`e² − e` was not nilpotent).
pub fn lift_idempotent<R: Ring>(
    alg: &dyn Graded<R>,
    e: &CTerm<R>,
    max_steps: usize,
) -> Result<CTerm<R>> {
    let ctx = alg.context();
    let mut e = alg.reduce(e)?;
    let three = ctx.coeff.int(3);
    let two = ctx.coeff.int(2);
    for _ in 0..max_steps {
        let e2 = mul(alg, &e, &e)?;
        if e2 == e {
            return Ok(e);
        }
        let e3 = mul(alg, &e2, &e)?;
        e = ctx.sub(&ctx.scale(&e2, &three), &ctx.scale(&e3, &two));
    }
    Err(Error::precondition(format!(
        "no idempotent after {max_steps} lifting steps: e² − e is not nilpotent"
    )))
}

/// Whether an idempotent `e` is primitive: `e A e / e J e` is one-dimensional. This is the test
/// for split algebras, such as KLR algebras over any field. Needs what [`radical`] needs.
pub fn is_primitive<R: Ring>(alg: &dyn Graded<R>, e: &CTerm<R>) -> Result<bool> {
    let ctx = alg.context();
    let e = alg.reduce(e)?;
    if e.is_zero() {
        return Ok(false);
    }
    if mul(alg, &e, &e)? != e {
        return Err(Error::precondition("the element is not an idempotent"));
    }
    let all = basis(alg)?;
    let rad = radical(alg)?;
    // the span of e b e, modulo the radical
    let mut r = Reducer::new(ctx.coeff.clone(), all.len());
    for j in &rad {
        r.insert(all.coords(ctx, j)?);
    }
    let base = r.rank();
    for w in &all.words {
        let ebe = mul(alg, &mul(alg, &e, &word_term(ctx, w))?, &e)?;
        r.insert(all.coords(ctx, &ebe)?);
        if r.rank() > base + 1 {
            return Ok(false);
        }
    }
    Ok(r.rank() == base + 1)
}

/// The blocks: primitive central idempotents of the whole algebra, from the degree-0 centre.
/// A graded algebra's central idempotents are homogeneous of degree 0. They are found as the
/// idempotents of the commutative algebra `Z_0`: for a generic `z ∈ Z_0` whose eigenvalues
/// (rational here, since KLR algebras are split) separate the blocks, the spectral projections
/// are polynomials in `z`, lifted to idempotents. Needs a field of characteristic 0 and the
/// whole algebra.
pub fn blocks<R: Ring>(alg: &dyn Graded<R>) -> Result<Vec<CTerm<R>>> {
    let ctx = alg.context();
    require_field(ctx)?;
    require_complete(alg, "the blocks")?;
    let z0 = centre(alg, 0)?;
    if z0.is_empty() {
        return Ok(vec![]);
    }
    let one = ctx.normalize(
        ctx.objects
            .iter()
            .map(|o| {
                (
                    Word {
                        source: o.clone(),
                        layers: vec![],
                    },
                    ctx.coeff.one(),
                )
            })
            .collect(),
    );
    let one = alg.reduce(&one)?;
    // The degree-0 centre as a space; multiplication by z on it.
    let mut words: Vec<Word> = z0
        .iter()
        .flat_map(|t| t.terms.iter().map(|(w, _)| w.clone()))
        .collect();
    words.sort();
    words.dedup();
    let ix = Indexed::new(words);
    let zc: Vec<Vec<Coeff<R>>> = z0
        .iter()
        .map(|t| ix.coords(ctx, t))
        .collect::<Result<_>>()?;
    // Coordinates of an element of Z_0 in the basis z0, by solving.
    let basis_mat = Mat::from_fn(ix.len(), z0.len(), |i, j| zc[j][i].clone());
    let in_z = |t: &CTerm<R>| -> Result<Vec<Coeff<R>>> {
        let v = ix.coords(ctx, t)?;
        match linalg::solve(&ctx.coeff, &basis_mat, &v)? {
            Some((x, _)) => Ok(x),
            None => Err(Error::invalid("a product left the degree-0 centre")),
        }
    };
    // Try a few z = Σ c_k z_k with small integer weights and keep the finest splitting.
    let n = z0.len();
    let mut best: Vec<CTerm<R>> = vec![one.clone()];
    for attempt in 0..8i64 {
        let mut z = ctx.normalize(vec![]);
        for (k, t) in z0.iter().enumerate() {
            let c = ctx.coeff.int((k as i64 * 7 + attempt * 13) % 11 + 1);
            z = ctx.add(&z, &ctx.scale(t, &c));
        }
        let idems = spectral_idempotents(alg, &z, &one, &in_z, n)?;
        if idems.len() > best.len() {
            best = idems;
        }
    }
    Ok(best)
}

/// The idempotents `e_λ` of the commutative algebra generated by `z` with `Σ e_λ = 1`, one per
/// rational eigenvalue `λ` of multiplication by `z` on `Z_0`, lifted from the Lagrange
/// projections. Returns `[1]` if some eigenvalue is not rational.
fn spectral_idempotents<R: Ring>(
    alg: &dyn Graded<R>,
    z: &CTerm<R>,
    one: &CTerm<R>,
    in_z: &InCentre<R>,
    n: usize,
) -> Result<Vec<CTerm<R>>> {
    let ctx = alg.context();
    // powers of z up to the degree of its minimal polynomial
    let mut powers = vec![one.clone()];
    let mut r = Reducer::new(ctx.coeff.clone(), n);
    r.insert(in_z(one)?);
    loop {
        let next = mul(alg, powers.last().unwrap(), z)?;
        if !r.insert(in_z(&next)?) {
            powers.push(next);
            break;
        }
        powers.push(next);
        if powers.len() > n + 1 {
            break;
        }
    }
    let m = powers.len() - 1;
    // minimal polynomial: z^m = Σ_{k<m} c_k z^k
    let cols: Vec<Vec<Coeff<R>>> = powers[..m].iter().map(in_z).collect::<Result<_>>()?;
    let a = Mat::from_fn(n, m, |i, j| cols[j][i].clone());
    let Some((c, _)) = linalg::solve(&ctx.coeff, &a, &in_z(&powers[m])?)? else {
        return Ok(vec![one.clone()]);
    };
    // roots in the integers among small candidates: x^m − Σ c_k x^k
    let eval = |x: i64| -> Coeff<R> {
        let xx = ctx.coeff.int(x);
        let mut acc = ctx.coeff.one();
        for _ in 0..m {
            acc = ctx.coeff.mul(&acc, &xx);
        }
        let mut p = ctx.coeff.one();
        for ck in &c {
            acc = ctx.coeff.sub(&acc, &ctx.coeff.mul(ck, &p));
            p = ctx.coeff.mul(&p, &xx);
        }
        acc
    };
    let roots: Vec<i64> = (-200..=200)
        .filter(|&x| ctx.coeff.is_zero(&eval(x)))
        .collect();
    if roots.len() < 2 {
        return Ok(vec![one.clone()]);
    }
    // Lagrange projections Π_{μ ≠ λ} (z − μ)/(λ − μ), then lift.
    let mut out = Vec::new();
    for &l in &roots {
        let mut p = one.clone();
        for &mu in &roots {
            if mu == l {
                continue;
            }
            let shifted = ctx.sub(z, &ctx.scale(one, &ctx.coeff.int(mu)));
            let inv = ctx
                .coeff
                .div_exact(&ctx.coeff.one(), &ctx.coeff.int(l - mu))
                .ok_or_else(|| Error::precondition("cannot invert an integer here"))?;
            p = ctx.scale(&mul(alg, &p, &shifted)?, &inv);
        }
        out.push(lift_idempotent(alg, &p, 64)?);
    }
    Ok(out)
}
